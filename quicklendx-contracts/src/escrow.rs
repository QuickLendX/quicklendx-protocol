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
use crate::payments::{create_escrow, refund_escrow};
use soroban_sdk::{Address, BytesN, Env, Vec};

/// Accept a bid and fund the invoice: transfer in from investor, create escrow, update state.
///
/// Caller (business) must be authorized. Invoice must be Verified; bid must be Placed and not expired.
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
    // 1. Retrieve Invoice
    let mut invoice =
        InvoiceStorage::get_invoice(env, invoice_id).ok_or(QuickLendXError::InvoiceNotFound)?;

    // 2. Auth checks
    // Verify that the caller is the business owner of the invoice
    invoice.business.require_auth();

    // 3. Invariant checks
    // Invoice must be in Verified status
    if invoice.status != InvoiceStatus::Verified {
        // If it's already funded, return specific error
        if invoice.status == InvoiceStatus::Funded {
            return Err(QuickLendXError::InvoiceAlreadyFunded);
        }
        return Err(QuickLendXError::InvoiceNotAvailableForFunding);
    }

    // 4. Retrieve Bid
    let mut bid = BidStorage::get_bid(env, bid_id).ok_or(QuickLendXError::StorageKeyNotFound)?;

    // Bid must match invoice
    if bid.invoice_id != *invoice_id {
        return Err(QuickLendXError::Unauthorized);
    }

    // Bid must be Placed
    if bid.status != BidStatus::Placed {
        return Err(QuickLendXError::InvalidStatus);
    }

    // Check bid expiration
    if bid.is_expired(env.ledger().timestamp()) {
        return Err(QuickLendXError::InvalidStatus);
    }

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
    // mark_as_funded updates status, funded_amount, investor, and logs audit
    invoice.mark_as_funded(
        env,
        bid.investor.clone(),
        bid.bid_amount,
    );
    InvoiceStorage::update_invoice(env, &invoice);

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
/// # Errors
/// * `InvoiceNotFound`, `StorageKeyNotFound`, `InvalidStatus`, `Unauthorized`, `NotAdmin`
pub fn refund_escrow_funds(
    env: &Env,
    invoice_id: &BytesN<32>,
    caller: &Address,
) -> Result<(), QuickLendXError> {
    // 1. Retrieve Invoice
    let mut invoice =
        InvoiceStorage::get_invoice(env, invoice_id).ok_or(QuickLendXError::InvoiceNotFound)?;

    // 2. Authorization check
    // Caller must be either the Admin or the Business owner
    let is_admin = AdminStorage::is_admin(env, caller);
    let is_business = &invoice.business == caller;

    if !is_admin && !is_business {
        return Err(QuickLendXError::Unauthorized);
    }

    // Explicitly require auth from the caller
    caller.require_auth();

    // 3. State check
    // Invoice must be in Funded status to be eligible for refund
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
