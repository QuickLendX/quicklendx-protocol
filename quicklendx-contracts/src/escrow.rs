//! Escrow funding flow: accept a bid and lock investor funds in escrow.
//!
//! Called from the public API with a reentrancy guard. Validates invoice/bid state,
//! creates escrow via payments, and updates bid, invoice, and investment state.
//!
//! ## One-Escrow-Per-Invoice Invariant
//! Each invoice may have **at most one** escrow record across its entire lifetime.
//! This is enforced at two independent layers:
//!
//! 1. **`load_accept_bid_context`** - checks `EscrowStorage::get_escrow_by_invoice`
//!    and `InvestmentStorage::get_investment_by_invoice` before any state changes.
//! 2. **`payments::create_escrow`** - re-checks `get_escrow_by_invoice` before the
//!    token transfer, so the guard holds even if the higher-level check is bypassed.
//!
//! Any duplicate attempt returns [`QuickLendXError::InvoiceAlreadyFunded`] or
//! [`QuickLendXError::InvalidStatus`] and leaves all state unchanged.
//! See `test_escrow_uniqueness.rs` for the full attack-vector test suite.

use crate::admin::AdminStorage;
use crate::errors::QuickLendXError;
use crate::events::{
    emit_bid_accepted, emit_escrow_refunded, emit_investment_withdrawn, emit_invoice_funded,
};
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
fn get_bid_acceptance_record(
    env: &Env,
    request_key: &BytesN<32>,
) -> Option<BidAcceptanceRecord> {
    env.storage()
        .persistent()
        .get(&(BID_ACCEPTANCE_RECORD_KEY, request_key.clone()))
}

/// Persist a durable bid-acceptance record and extend its TTL so it does not
/// expire while the escrow it references remains live.
fn store_bid_acceptance_record(
    env: &Env,
    request_key: &BytesN<32>,
    record: &BidAcceptanceRecord,
) {
    let key = (BID_ACCEPTANCE_RECORD_KEY, request_key.clone());
    env.storage().persistent().set(&key, record);
    crate::storage::extend_persistent_ttl(env, &key);
}

/// Validate the invoice, bid, and escrow state before any funds move.
///
/// # Security
/// - Authorization is checked against the exact invoice being funded
/// - The bid must belong to that invoice
/// - The invoice must not already have escrow, funding metadata, or an investment
///
/// ## One-Escrow-Per-Invoice Invariant (Two-Layer Guard)
/// This function implements the outer guard of the one-escrow-per-invoice invariant:
/// it checks for existing escrow or investment records before any state changes.
/// The inner guard is in `payments::create_escrow`, which re-checks
/// `EscrowStorage::get_escrow_by_invoice` before the token transfer.
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

    // Outer guard: check for existing escrow or investment record before any state changes.
    // This is the first layer of the one-escrow-per-invoice invariant.
    // The second layer is in payments::create_escrow which re-checks get_escrow_by_invoice
    // before the token transfer, ensuring the invariant holds even if this check is bypassed.
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
            // Single transfer for the full bid amount avoids a stale balance read
            // that could occur when two sequential transfer_funds calls are made
            // from the same source address within one transaction.
            crate::payments::transfer_funds(
                env,
                &invoice.currency,
                &bid.investor,
                &env.current_contract_address(),
                bid.bid_amount,
            )?;

            let mut fees_collected = soroban_sdk::Map::new(env);
            fees_collected.set(crate::fees::FeeType::Origination, total_fee);
            crate::fees::FeeManager::collect_fees(env, &bid.investor, fees_collected, total_fee)?;

            // Funds are already in the contract from the single transfer above.
            // Create the escrow record without a second transfer.
            let escrow_id = create_escrow_record_only(
                env,
                invoice_id,
                &bid.investor,
                &invoice.business,
                escrow_amount,
                &invoice.currency,
            )?;

            // 6. Update states
            update_states_after_funding(env, invoice_id, &mut invoice, &mut bid)?;

            crate::qlx_log!(env, "escrow", "Invoice funded and bid accepted");

            // 7. Events
            emit_invoice_funded(env, invoice_id, &bid.investor, bid.bid_amount);
            emit_bid_accepted(env, &bid, invoice_id, &invoice.business);
            crate::audit::log_bid_accepted(
                env,
                invoice_id.clone(),
                invoice.business.clone(),
                bid.bid_amount,
                bid.bid_id.clone(),
            );

            // Lifecycle trigger: emits `NotificationType::BidAccepted` to the investor
            let _ =
                crate::notifications::NotificationSystem::notify_bid_accepted(env, &invoice, &bid);

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
    update_states_after_funding(env, invoice_id, &mut invoice, &mut bid)?;

    crate::qlx_log!(env, "escrow", "Invoice funded and bid accepted");

    // 7. Events
    emit_invoice_funded(env, invoice_id, &bid.investor, bid.bid_amount);
    emit_bid_accepted(env, &bid, invoice_id, &invoice.business);
    crate::audit::log_bid_accepted(
        env,
        invoice_id.clone(),
        invoice.business.clone(),
        bid.bid_amount,
        bid.bid_id.clone(),
    );

    // Lifecycle trigger: emits `NotificationType::BidAccepted` to the investor
    // after escrow funding and state transitions complete successfully.
    let _ = crate::notifications::NotificationSystem::notify_bid_accepted(env, &invoice, &bid);

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
/// one-escrow-per-invoice two-layer guard and all lifecycle checks, and only
/// on success binds the request key to the funded payload and escrow ID.
pub fn accept_bid_and_fund_with_key(
    env: &Env,
    invoice_id: &BytesN<32>,
    bid_id: &BytesN<32>,
    request_key: &BytesN<32>,
) -> Result<BytesN<32>, QuickLendXError> {
    if let Some(record) = get_bid_acceptance_record(env, request_key) {
        if record.invoice_id == *invoice_id && record.bid_id == *bid_id {
            let invoice = InvoiceStorage::get_invoice(env, &record.invoice_id)
                .ok_or(QuickLendXError::InvoiceNotFound)?;
            invoice.business.require_auth();
            require_business_active(env, &invoice.business)?;
            require_business_not_pending(env, &invoice.business)?;
            return Ok(record.escrow_id);
        }
        return Err(QuickLendXError::DuplicateBid);
    }

    let escrow_id = accept_bid_and_fund(env, invoice_id, bid_id)?;

    store_bid_acceptance_record(
        env,
        request_key,
        &BidAcceptanceRecord {
            invoice_id: invoice_id.clone(),
            bid_id: bid_id.clone(),
            escrow_id: escrow_id.clone(),
        },
    );

    Ok(escrow_id)
}

/// State transitions that follow a successful escrow creation and funding.
fn update_states_after_funding(
    env: &Env,
    invoice_id: &BytesN<32>,
    invoice: &mut crate::types::Invoice,
    bid: &mut crate::types::Bid,
) -> Result<(), QuickLendXError> {
    bid.status = BidStatus::Accepted;
    BidStorage::update_bid(env, bid);

    InvoiceStorage::remove_from_status_invoices(env, InvoiceStatus::Verified, invoice_id);

    invoice.mark_as_funded(
        env,
        bid.investor.clone(),
        bid.bid_amount,
        env.ledger().timestamp(),
    );
    InvoiceStorage::update_invoice(env, invoice);

    InvoiceStorage::add_to_status_invoices(env, InvoiceStatus::Funded, invoice_id);

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

    crate::qlx_log!(env, "escrow", "Invoice funded and bid accepted");

    // 7. Audit & Events
    crate::events::emit_bid_accepted(env, &bid, invoice_id, &invoice.business);
    crate::audit::log_bid_accepted(
        env,
        invoice_id.clone(),
        bid.investor.clone(),
        bid.bid_amount,
    );
    emit_invoice_funded(env, invoice_id, &bid.investor, bid.bid_amount);
    crate::audit::log_invoice_funded(
        env,
        invoice_id.clone(),
        bid.investor.clone(),
        bid.bid_amount,
    );

    // Lifecycle trigger: emits `NotificationType::BidAccepted` to the investor
    // after escrow funding and state transitions complete successfully.
    let _ = crate::notifications::NotificationSystem::notify_bid_accepted(env, &invoice, &bid);

    Ok(())
}

/// Explicitly refund escrowed funds to the investor.
///
/// Can be triggered by the Admin or the Business owner of the invoice.
/// Invoice must be in Funded status.
///
/// # Correctness
/// - Refunds the exact `escrow.amount` stored for the invoice.
/// - Sends funds to the stored `escrow.investor`, never to a caller-controlled recipient.
/// - Uses `payments::refund_escrow` which rejects any escrow not in `Held` status,
///   making repeated refund attempts fail and preventing double refunds.
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
    let escrow = crate::payments::EscrowStorage::get_escrow_by_invoice(env, invoice_id).unwrap();

    // 5. Transfer funds and update escrow state
    // This calls payments::refund_escrow which handles the token transfer and status update
    refund_escrow(env, invoice_id, caller)?;

    // 6. Update internal states

    // Update Invoice status to Refunded
    let previous_status = invoice.status;
    invoice.mark_as_refunded(env, caller.clone());
    InvoiceStorage::update_invoice(env, &invoice);

    // Update status indices
    InvoiceStorage::remove_from_status_invoices(env, previous_status, invoice_id);
    InvoiceStorage::add_to_status_invoices(env, InvoiceStatus::Refunded, invoice_id);

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

    crate::qlx_log!(env, "escrow", "Escrow refunded successfully");

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

/// Withdraw an active investment: refunds escrowed funds to the investor and
/// transitions the investment to [`InvestmentStatus::Withdrawn`].
///
/// Only the investor who owns the investment may call this entry point.
///
/// # Preconditions (checked)
/// - `investor` is authorized
/// - The investment exists, is in [`InvestmentStatus::Active`], and belongs to `investor`
/// - The associated escrow is still [`EscrowStatus::Held`] (funds have not been released)
/// - The invoice is in [`InvoiceStatus::Funded`] (no settlement has occurred)
///
/// # Postconditions
/// - Escrowed funds are returned to the investor via the existing `refund_escrow` path
/// - The investment transitions `Active → Withdrawn` via `InvestmentStorage::update_investment`,
///   which enforces `validate_transition` and removes the investment from the active index
/// - The invoice has its funded fields cleared and status restored to `Verified`
/// - The accepted bid is cancelled
/// - A [`TOPIC_INVESTMENT_WITHDRAWN`] event is emitted
///
/// # Reentrancy
/// The token-moving path is wrapped in `with_payment_guard` by the caller (lib.rs entrypoint).
/// This function performs the refund before updating state, so a reentrant call would
/// fail at the escrow status check (escrow no longer `Held`).
///
/// # Security
/// - Authorization: `investor.require_auth()` ensures only the investor can withdraw
/// - Escrow guard: `payments::refund_escrow` rejects any escrow not in `Held` status,
///   preventing double-withdrawal even if `withdraw_investment` is called again
/// - Transition guard: `InvestmentStorage::update_investment` calls `validate_transition`,
///   rejecting any attempt to withdraw from a terminal state
/// - Cross-module consistency: invoice, escrow, bid, and investment state are all updated
///   atomically in the same function body, preserving the protocol's state machine
///
/// # Errors
/// * `QuickLendXError::Unauthorized` — caller is not the investment's investor
/// * `QuickLendXError::InvalidStatus` — investment is not Active, or escrow is not Held
/// * `QuickLendXError::InvoiceNotFound` — invoice not found
/// * `QuickLendXError::InvoiceNotAvailableForFunding` — invoice is not in Funded status
/// * `QuickLendXError::StorageKeyNotFound` — escrow not found for the invoice
pub fn withdraw_investment(
    env: &Env,
    invoice_id: &BytesN<32>,
    investor: &Address,
) -> Result<(), QuickLendXError> {
    // 1. Mandatory authentication check
    investor.require_auth();

    // 2. Validate investment exists, is Active, and belongs to caller
    let mut investment = InvestmentStorage::get_investment_by_invoice(env, invoice_id).unwrap();

    if investment.status != InvestmentStatus::Active {
        return Err(QuickLendXError::InvalidStatus);
    }

    if &investment.investor != investor {
        return Err(QuickLendXError::Unauthorized);
    }

    // 3. Validate invoice is still Funded (not yet settled/paid/defaulted)
    let mut invoice =
        InvoiceStorage::get_invoice(env, invoice_id).ok_or(QuickLendXError::InvoiceNotFound)?;

    if invoice.status != InvoiceStatus::Funded {
        return Err(QuickLendXError::InvalidStatus);
    }

    // 4. Validate escrow exists and is still Held
    let escrow = EscrowStorage::get_escrow_by_invoice(env, invoice_id).unwrap();

    if escrow.status != EscrowStatus::Held {
        return Err(QuickLendXError::InvalidStatus);
    }

    // 5. Refund escrowed funds to the investor (token transfer + escrow status → Refunded)
    refund_escrow(env, invoice_id, investor)?;

    // 6. Restore invoice to Verified state and clear funded fields
    let previous_status = invoice.status;
    invoice.status = InvoiceStatus::Verified;
    invoice.funded_amount = 0;
    invoice.funded_at = None;
    invoice.investor = None;
    InvoiceStorage::update_invoice(env, &invoice);

    // Update invoice status lists
    InvoiceStorage::remove_from_status_invoices(env, previous_status, invoice_id);
    InvoiceStorage::add_to_status_invoices(env, InvoiceStatus::Verified, invoice_id);

    // 7. Cancel the accepted bid
    let bids = BidStorage::get_bid_records_for_invoice(env, invoice_id);
    for mut bid in bids.iter() {
        if bid.status == BidStatus::Accepted {
            bid.status = BidStatus::Cancelled;
            BidStorage::update_bid(env, &bid);
            break;
        }
    }

    // 8. Transition investment Active → Withdrawn
    investment.status = InvestmentStatus::Withdrawn;
    InvestmentStorage::update_investment(env, &investment);

    crate::qlx_log!(env, "escrow", "Investment withdrawn successfully");

    // 9. Emit events
    emit_investment_withdrawn(
        env,
        &investment.investment_id,
        invoice_id,
        investor,
        escrow.amount,
    );

    emit_escrow_refunded(env, &escrow.escrow_id, invoice_id, investor, escrow.amount);

    Ok(())
}
