//! Escrow funding flow: accept a bid and lock investor funds in escrow.
//!
//! Called from the public API with a reentrancy guard. Validates invoice/bid state,
//! creates escrow via payments, and updates bid, invoice, and investment state.
//!
//! ## One-Live-Escrow-Per-Invoice Invariant
//! Each invoice may have **at most one live** escrow record (`EscrowStatus::Held`)
//! at a time. Terminal escrows (`Released`, `Refunded`) are immutable history and
//! remain reachable by their `escrow_id`; the invoice→current mapping is advanced
//! to any newly created escrow. The same rule applies to investments: only an
//! `Active` investment is live; terminal investments remain reachable by ID.
//! This is enforced at two independent layers:
//!
//! 1. **`load_accept_bid_context`** - rejects a `Held` escrow or `Active`
//!    investment before any state changes.
//! 2. **`payments::create_escrow`** - re-checks for a `Held` escrow before the
//!    token transfer, so the guard holds even if the higher-level check is bypassed.
//!
//! Any duplicate attempt against a live record returns
//! [`QuickLendXError::InvoiceAlreadyFunded`] or [`QuickLendXError::InvalidStatus`]
//! and leaves all state unchanged. See `test_escrow_uniqueness.rs` for the full
//! attack-vector test suite.

use crate::admin::AdminStorage;
use crate::errors::QuickLendXError;
use crate::events::{emit_investment_withdrawn, emit_invoice_funded};
use crate::payments::{
    create_escrow, create_escrow_record_only, refund_escrow, EscrowStatus, EscrowStorage,
};
use crate::storage::{BidStorage, InvestmentStorage, InvoiceStorage};
use crate::types::{BidStatus, Investment, InvestmentStatus, InvoiceStatus};
use crate::verification::{
    require_business_active, require_business_not_pending, require_investor_not_frozen,
    require_investor_not_pending, validate_investor_investment,
};
use soroban_sdk::{contracttype, symbol_short, Address, BytesN, Env, Symbol, Vec};

/// Loaded and validated state required to accept a bid.
pub(crate) struct AcceptBidContext {
    pub invoice: crate::types::Invoice,
    pub bid: crate::types::Bid,
}

/// Durable outcome of a keyed bid-acceptance operation.
///
/// The record binds a caller-provided `request_key` to the exact payload it
/// funded (`invoice_id` + `bid_id`) and to the resulting `escrow_id`. Safe
/// retries replay the same payload against the same key and receive the cached
/// escrow ID deterministically; reusing the key with a different payload is
/// rejected.
#[contracttype]
#[derive(Clone)]
pub struct BidAcceptanceRecord {
    pub invoice_id: BytesN<32>,
    pub bid_id: BytesN<32>,
    pub escrow_id: BytesN<32>,
}

/// Storage namespace for keyed bid-acceptance records.
const BID_ACCEPTANCE_RECORD_KEY: Symbol = symbol_short!("bid_acc");

/// Look up a durable bid-acceptance record by request key.
fn get_bid_acceptance_record(env: &Env, request_key: &BytesN<32>) -> Option<BidAcceptanceRecord> {
    env.storage()
        .persistent()
        .get(&(BID_ACCEPTANCE_RECORD_KEY, request_key.clone()))
}

/// Persist a durable bid-acceptance record and extend its TTL so it does not
/// expire while the escrow it references remains live.
fn store_bid_acceptance_record(env: &Env, request_key: &BytesN<32>, record: &BidAcceptanceRecord) {
    let key = (BID_ACCEPTANCE_RECORD_KEY, request_key.clone());
    env.storage().persistent().set(&key, record);
    crate::storage::extend_persistent_ttl(env, &key);
}

/// Validate the invoice, bid, and escrow state before any funds move.
///
/// # Security
/// - Authorization is checked against the exact invoice being funded
/// - The bid must belong to that invoice
/// - The invoice must not already have a *live* escrow or investment
///
/// ## One-Live-Escrow-Per-Invoice Invariant (Two-Layer Guard)
/// This function implements the outer guard: it rejects an invoice that still has
/// a `Held` escrow or an `Active` investment before any state changes. Terminal
/// records (`Released`/`Refunded` escrow, `Withdrawn`/`Completed`/`Defaulted`/
/// `Refunded` investment) are audit history and must not block re-funding after a
/// withdrawal. The inner guard is in `payments::create_escrow`, which re-checks
/// for a `Held` escrow before the token transfer.
/// If either guard fails, the function returns an error and no state is mutated.
pub(crate) fn load_accept_bid_context(
    env: &Env,
    invoice_id: &BytesN<32>,
    bid_id: &BytesN<32>,
) -> Result<AcceptBidContext, QuickLendXError> {
    BidStorage::cleanup_expired_bids(env, invoice_id);

    let invoice =
        InvoiceStorage::get_invoice(env, invoice_id).ok_or(QuickLendXError::InvoiceNotFound)?;

    invoice.business.require_auth();
    require_business_active(env, &invoice.business)?;
    require_business_not_pending(env, &invoice.business)?;

    if invoice.status == InvoiceStatus::Funded {
        return Err(QuickLendXError::InvoiceAlreadyFunded);
    }

    if !invoice.is_available_for_funding() {
        return Err(QuickLendXError::InvoiceNotAvailableForFunding);
    }

    // Reject bid acceptance on invoices past their due date.
    // An escrow created for an already-expired invoice would lock
    // investor funds into an obligation that cannot be settled on
    // time.  The investor must be able to place bids freely, but
    // once the due date passes the business should not be able to
    // accept new funding.
    if env.ledger().timestamp() > invoice.due_date {
        return Err(QuickLendXError::OperationNotAllowed);
    }

    if invoice.funded_amount != 0 || invoice.funded_at.is_some() || invoice.investor.is_some() {
        return Err(QuickLendXError::InvalidStatus);
    }

    // Outer guard: reject only if a *live* escrow or investment still exists.
    // Terminal records from a prior funding cycle (Refunded/Released escrow,
    // Withdrawn/Completed/Defaulted/Refunded investment) are retained for audit
    // and must not prevent an invoice that has returned to the market from being
    // funded again. The second layer is in payments::create_escrow which
    // re-checks for a Held escrow before the token transfer.
    if let Some(escrow) = EscrowStorage::get_escrow_by_invoice(env, invoice_id) {
        if escrow.status == EscrowStatus::Held {
            return Err(QuickLendXError::InvalidStatus);
        }
    }
    if let Some(investment) = InvestmentStorage::get_investment_by_invoice(env, invoice_id) {
        if investment.status == InvestmentStatus::Active {
            return Err(QuickLendXError::InvalidStatus);
        }
    }

    let bid = BidStorage::get_bid(env, bid_id).ok_or(QuickLendXError::StorageKeyNotFound)?;

    if bid.invoice_id != *invoice_id {
        return Err(QuickLendXError::Unauthorized);
    }

    // Enforce the legal transition matrix at the accept entry point: only
    // `Placed -> Accepted` is legal. Cancelled, Withdrawn, Expired, and
    // already-Accepted bids are rejected without touching any state.
    if BidStatus::validate_transition(&bid.status, &BidStatus::Accepted).is_err() {
        return Err(QuickLendXError::InvalidStatus);
    }

    // KYC and freeze status are checked again at acceptance time. A bid can
    // remain open after its investor's verification changes, so placement-time
    // validation alone is not sufficient authorization for moving funds.
    require_investor_not_frozen(env, &bid.investor)?;
    require_investor_not_pending(env, &bid.investor)?;

    if bid.is_expired(env.ledger().timestamp()) {
        return Err(QuickLendXError::BidStale);
    }

    if bid.bid_amount <= 0 {
        return Err(QuickLendXError::InvalidAmount);
    }

    // Re-verify investor KYC status and aggregate investment capacity before accepting bid.
    validate_investor_investment(env, &bid.investor, 0)?;

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

    crate::qlx_log!(env, "escrow", "Accepting bid and funding invoice");

    let mut escrow_amount = bid.bid_amount;
    if let Some(fee_bps) = invoice.origination_fee_bps {
        if fee_bps > crate::init::MAX_FEE_BPS {
            return Err(QuickLendXError::InvalidFeeBasisPoints);
        }

        let total_fee = (bid
            .bid_amount
            .checked_mul(fee_bps as i128)
            .ok_or(QuickLendXError::ArithmeticOverflow)?)
        .checked_div(10000)
        .ok_or(QuickLendXError::ArithmeticOverflow)?;

        escrow_amount = bid
            .bid_amount
            .checked_sub(total_fee)
            .ok_or(QuickLendXError::ArithmeticOverflow)?;

        if total_fee > 0 {
            crate::payments::require_matching_currency_precision(
                env,
                &invoice.currency,
                bid.bid_amount,
            )?;

            // This validates the net escrow amount and records it before the
            // single gross transfer. Any later error rolls back the transaction.
            let escrow_id = create_escrow_record_only(
                env,
                invoice_id,
                &bid.investor,
                &invoice.business,
                escrow_amount,
                &invoice.currency,
            )?;

            crate::payments::transfer_funds(
                env,
                &invoice.currency,
                &bid.investor,
                &env.current_contract_address(),
                bid.bid_amount,
            )?;
            crate::fees::FeeManager::collect_origination_fee(
                env,
                &bid.investor,
                invoice_id,
                total_fee,
            )?;

            // 6. Update states
            update_states_after_funding(env, invoice_id, &mut invoice, &mut bid, escrow_amount)?;

            crate::qlx_log!(env, "escrow", "Invoice funded and bid accepted");

            return Ok(escrow_id);
        }
    }

    // No fee, or fee is zero: use the standard create_escrow which handles its own transfer.
    let escrow_id = create_escrow(
        env,
        invoice_id,
        &bid.investor,
        &invoice.business,
        escrow_amount,
        &invoice.currency,
    )?;

    // 6. Update states
    update_states_after_funding(env, invoice_id, &mut invoice, &mut bid, escrow_amount)?;

    crate::qlx_log!(env, "escrow", "Invoice funded and bid accepted");

    Ok(escrow_id)
}

/// Accept a bid and fund the invoice while binding the operation to a durable
/// request key.
///
/// # Idempotency contract
/// - A **safe retry** (same `request_key`, `invoice_id`, and `bid_id`) does
///   not move funds again: the previously created escrow ID is returned
///   deterministically, after re-verifying the recording invoice's business
///   authorization.
/// - **Conflicting reuse** of `request_key` with a different payload is
///   rejected with [`QuickLendXError::DuplicateBid`] and leaves all state
///   unchanged.
/// - A **rejected or failed attempt never stores a record**, so a corrected
///   retry with the same key remains available and no partial state lingers.
///
/// On a fresh attempt this defers to `accept_bid_and_fund`, inheriting the
/// one-escrow-per-invoice two-
