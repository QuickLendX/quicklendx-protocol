//! Escrow funding flow: accept a bid and lock investor funds in escrow.
//!
//! Called from the public API with a reentrancy guard. Validates invoice/bid state,
//! creates escrow via payments, and updates bid, invoice, and investment state.

use crate::admin::AdminStorage;
use crate::bid::{BidStatus, BidStorage};
use crate::errors::QuickLendXError;
use crate::events::{emit_escrow_refunded, emit_invoice_funded};
use crate::investment::{Investment, InvestmentStatus, InvestmentStorage};
use crate::invoice::{InvoiceStatus, InvoiceStorage};
use crate::payments::{create_escrow, refund_escrow, EscrowStorage};
use crate::verification::require_business_not_pending;
use soroban_sdk::{Address, BytesN, Env, Vec};

/// Loaded and validated state required to accept a bid.
pub(crate) struct AcceptBidContext {
    pub invoice: crate::invoice::Invoice,
    pub bid: crate::bid::Bid,
}

/// Validate the invoice, bid, and escrow state before any funds move.
///
/// # Security
/// - Authorization is checked against the exact invoice being funded
/// - The bid must belong to that invoice
/// - The invoice must not already have escrow, funding metadata, or an investment
pub(crate) fn load_accept_bid_context(
    env: &Env,
    invoice_id: &BytesN<32>,
    bid_id: &BytesN<32>,
) -> Result<AcceptBidContext, QuickLendXError> {
    BidStorage::cleanup_expired_bids(env, invoice_id);

    let invoice =
        InvoiceStorage::get_invoice(env, invoice_id).ok_or(QuickLendXError::InvoiceNotFound)?;

    invoice.business.require_auth();
    require_business_not_pending(env, &invoice.business)?;

    if invoice.status == InvoiceStatus::Funded {
        return Err(QuickLendXError::InvoiceAlreadyFunded);
    }

    if !invoice.is_available_for_funding() {
        return Err(QuickLendXError::InvoiceNotAvailableForFunding);
    }

    if invoice.funded_amount != 0 || invoice.funded_at.is_some() || invoice.investor.is_some() {
        return Err(QuickLendXError::InvalidStatus);
    }

    if EscrowStorage::get_escrow_by_invoice(env, invoice_id).is_some()
        || InvestmentStorage::get_investment_by_invoice(env, invoice_id).is_some()
    {
        return Err(QuickLendXError::InvalidStatus);
    }

    let bid = BidStorage::get_bid(env, bid_id).ok_or(QuickLendXError::StorageKeyNotFound)?;

    if bid.invoice_id != *invoice_id {
        return Err(QuickLendXError::Unauthorized);
    }

    if bid.status != BidStatus::Placed {
        return Err(QuickLendXError::InvalidStatus);
    }

    if bid.is_expired(env.ledger().timestamp()) {
        return Err(QuickLendXError::InvalidStatus);
    }

    if bid.bid_amount <= 0 {
        return Err(QuickLendXError::InvalidAmount);
    }

    Ok(AcceptBidContext { invoice, bid })
}

/// Accept a bid and fund the invoice: transfer in from investor, create escrow, update state.
///
/// Caller (business) must be authorized. Invoice must be Verified; bid must be Placed and not expired.
///
/// # Invariants
/// * Each invoice maps to at most one active escrow record (Held status).
/// * Duplicate escrow creation attempts for the same invoice are rejected.
///
/// # Returns
/// * `Ok(escrow_id)` - The new escrow ID
///
/// # Errors
/// * `InvoiceNotFound`, `StorageKeyNotFound`, `InvalidStatus`, `InvoiceAlreadyFunded`,
///   `InvoiceNotAvailableForFunding`, `Unauthorized`, or errors from `create_escrow`
pub fn accept_bid_and_fund(
    env: &Env,
    invoice_id: &BytesN<32>,
    bid_id: &BytesN<32>,
) -> Result<BytesN<32>, QuickLendXError> {
    let AcceptBidContext {
        mut invoice,
        mut bid,
    } = load_accept_bid_context(env, invoice_id, bid_id)?;

    // 5. Lock funds in escrow
    // This calls payments::create_escrow which calls token transfer and emits emit_escrow_created
    let escrow_id = create_escrow(
        env,
        invoice_id,
        &bid.investor,
        &invoice.business,
        bid.bid_amount,
        &invoice.currency,
    )?;

    // 6. Update states

    // Update Bid
    bid.status = BidStatus::Accepted;
    BidStorage::update_bid(env, &bid);

    // Update Invoice
    // Remove from old status list before changing status
    InvoiceStorage::remove_from_status_invoices(env, &InvoiceStatus::Verified, invoice_id);

    // mark_as_funded updates status, funded_amount, investor, and logs audit
    invoice.mark_as_funded(
        env,
        bid.investor.clone(),
        bid.bid_amount,
        env.ledger().timestamp(),
    );
    InvoiceStorage::update_invoice(env, &invoice);

    // Add to new status list after status change
    InvoiceStorage::add_to_status_invoices(env, &InvoiceStatus::Funded, invoice_id);

    // Create Investment
    let investment_id = InvestmentStorage::generate_unique_investment_id(env);
    let investment = Investment {
        investment_id: investment_id.clone(),
        invoice_id: invoice_id.clone(),
        investor: bid.investor.clone(),
        amount: bid.bid_amount,
        funded_at: env.ledger().timestamp(),
        status: InvestmentStatus::Active,
        insurance: Vec::new(env),
    };
    InvestmentStorage::store_investment(env, &investment);

    // 7. Events
    emit_invoice_funded(env, invoice_id, &bid.investor, bid.bid_amount);

    Ok(escrow_id)
}

/// Explicitly refund escrowed funds to the investor.
///
/// Can be triggered by the Admin or the Business owner of the invoice.
/// Invoice must be in Funded status.
///
/// # Finality
/// The `invoice.status != InvoiceStatus::Funded` check makes refund a
/// one-shot operation: once an invoice becomes `Refunded`, `Paid`, or
/// `Defaulted`, this function returns `InvalidStatus` without moving any
/// funds. Combined with `payments::refund_escrow` only operating on escrows
/// in `Held` state, this prevents double refunds even under concurrent or
/// retried calls.
///
/// # Errors
/// * `InvoiceNotFound`, `StorageKeyNotFound`, `InvalidStatus`, `Unauthorized`, `NotAdmin`
pub fn refund_escrow_funds(
    env: &Env,
    invoice_id: &BytesN<32>,
    caller: &Address,
) -> Result<(), QuickLendXError> {
    // 1. Mandatory authentication check
    caller.require_auth();

    // 2. Retrieve Invoice
    let mut invoice =
        InvoiceStorage::get_invoice(env, invoice_id).ok_or(QuickLendXError::InvoiceNotFound)?;

    // 3. Authorization Matrix
    // Only the Contract Admin or the Business owner of the invoice is authorized
    let is_admin = AdminStorage::is_admin(env, caller);
    let is_business = &invoice.business == caller;

    if !is_admin && !is_business {
        return Err(QuickLendXError::Unauthorized);
    }

    // 4. State Protections
    // Escrow refund is ONLY permitted if the invoice is currently in Funded status
    if invoice.status != InvoiceStatus::Funded {
        return Err(QuickLendXError::InvalidStatus);
    }

    // 4. Retrieve Escrow
    let escrow = crate::payments::EscrowStorage::get_escrow_by_invoice(env, invoice_id)
        .ok_or(QuickLendXError::StorageKeyNotFound)?;

    // 5. Transfer funds and update escrow state
    // This calls payments::refund_escrow which handles the token transfer and status update
    refund_escrow(env, invoice_id)?;

    // 6. Update internal states

    // Update Invoice status to Refunded
    let previous_status = invoice.status.clone();
    invoice.mark_as_refunded(env, caller.clone());
    InvoiceStorage::update_invoice(env, &invoice);

    // Update status indices
    InvoiceStorage::remove_from_status_invoices(env, &previous_status, invoice_id);
    InvoiceStorage::add_to_status_invoices(env, &InvoiceStatus::Refunded, invoice_id);

    // Update Bid status to Cancelled (find the accepted bid first)
    // In our protocol, a Funded invoice has exactly one Accepted bid
    let bids = BidStorage::get_bid_records_for_invoice(env, invoice_id);
    for mut bid in bids.iter() {
        if bid.status == BidStatus::Accepted {
            bid.status = BidStatus::Cancelled;
            BidStorage::update_bid(env, &bid);
            break;
        }
    }

    // Update Investment status to Refunded
    if let Some(mut investment) = InvestmentStorage::get_investment_by_invoice(env, invoice_id) {
        investment.status = InvestmentStatus::Refunded;
        InvestmentStorage::update_investment(env, &investment);
    }

    // 7. Emit events
    emit_escrow_refunded(
        env,
        &escrow.escrow_id,
        invoice_id,
        &escrow.investor,
        escrow.amount,
    );

    Ok(())
}
