//! Escrow funding flow: accept a bid and lock investor funds in escrow.
//!
//! Called from the public API with a reentrancy guard. Validates invoice/bid state,
//! creates escrow via payments, and updates bid, invoice, and investment state.

use crate::bid::{BidStatus, BidStorage};
use crate::errors::QuickLendXError;
use crate::events::emit_invoice_funded;
use crate::investment::{Investment, InvestmentStatus, InvestmentStorage};
use crate::invoice::{InvoiceStatus, InvoiceStorage};
use crate::payments::create_escrow;
use soroban_sdk::{BytesN, Env, Vec};

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
    let mut invoice = InvoiceStorage::get_invoice(env, invoice_id)
        .ok_or(QuickLendXError::InvoiceNotFound)?;

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
    let mut bid = BidStorage::get_bid(env, bid_id)
        .ok_or(QuickLendXError::StorageKeyNotFound)?;

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
    invoice.mark_as_funded(env, bid.investor.clone(), bid.bid_amount, env.ledger().timestamp());
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
