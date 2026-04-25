//! Invoice settlement with partial payments, capped overpayment handling,
//! durable per-payment storage records, and finalization safety guards.
//!
//! # Invariants
//! - `total_paid <= total_due` is enforced at every payment recording step.
//! - Settlement finalization is idempotent: once `status == Paid`, further
//!   settlement attempts are rejected.
//! - `investor_return + platform_fee == total_paid` is asserted before fund
//!   disbursement to prevent accounting drift.
//! - Payment count cannot exceed `MAX_PAYMENT_COUNT` per invoice.

use crate::errors::QuickLendXError;
use crate::events::{emit_invoice_settled, emit_partial_payment};
use crate::investment::InvestmentStorage;
use crate::types::InvestmentStatus;
use crate::payments::transfer_funds;
use crate::storage::InvoiceStorage;
use crate::types::{Invoice, InvoiceStatus, PaymentRecord as InvoicePaymentRecord};
use soroban_sdk::{contracttype, symbol_short, Address, BytesN, Env, String, Vec};

const MAX_INLINE_PAYMENT_HISTORY: u32 = 32;

/// Maximum number of discrete payment records per invoice.
/// Prevents unbounded storage growth and protects against payment-count overflow.
const MAX_PAYMENT_COUNT: u32 = 1_000;

#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(test, derive(Debug))]
enum SettlementDataKey {
    PaymentCount(BytesN<32>),
    Payment(BytesN<32>, u32),
    PaymentNonce(BytesN<32>, String),
    /// Marks an invoice as finalized to guard against double-settlement.
    Finalized(BytesN<32>),
}

/// Durable payment record stored per invoice/payment-index.
#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(test, derive(Debug))]
pub struct SettlementPaymentRecord {
    pub payer: Address,
    pub amount: i128,
    pub timestamp: u64,
    pub nonce: String,
}

/// Settlement progress for an invoice.
#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(test, derive(Debug))]
pub struct Progress {
    pub total_due: i128,
    pub total_paid: i128,
    pub remaining_due: i128,
    pub progress_percent: u32,
    pub payment_count: u32,
    pub status: InvoiceStatus,
}

/// Record a partial payment for an invoice.
///
/// If the total paid amount reaches the invoice total, the settlement is finalized.
/// This method provides strictly ordered record persistence and idempotent deduplication.
///
/// # Arguments
/// - `invoice_id`: Unique identifier for the invoice being paid.
/// - `payment_amount`: The requested payment amount.
/// - `transaction_id`: A unique identifier for the payment attempt (nonce).
///
/// # Returns
/// - `Ok(())` on success, or a `QuickLendXError` on failure.
///
/// # Security
/// - @security Requires business-owner authorization for every payment attempt.
/// - @security Safely bounds applied value to the remaining due amount.
/// - @security Guards against replayed transaction identifiers per invoice.
/// - Preserves `total_paid <= amount` even when callers request an overpayment.
/// - Rejects payments when MAX_PAYMENT_COUNT is reached.
pub fn process_partial_payment(
    env: &Env,
    invoice_id: &BytesN<32>,
    payment_amount: i128,
    transaction_id: String,
) -> Result<(), QuickLendXError> {
    let invoice =
        InvoiceStorage::get_invoice(env, invoice_id).ok_or(QuickLendXError::InvoiceNotFound)?;
    let payer = invoice.business.clone();

    let progress = record_payment(
        env,
        invoice_id,
        &payer,
        payment_amount,
        transaction_id.clone(),
    )?;

    // Backward-compatible event used across existing tests/consumers.
    emit_partial_payment(
        env,
        &InvoiceStorage::get_invoice(env, invoice_id).ok_or(QuickLendXError::InvoiceNotFound)?,
        get_last_applied_amount(env, invoice_id)?,
        progress.total_paid,
        progress.progress_percent,
        transaction_id,
    );

    if progress.total_paid >= progress.total_due {
        settle_invoice_internal(env, invoice_id)?;
    }

    Ok(())
}

/// Record a payment attempt with capping, replay protection, and durable storage.
///
/// - Rejects amount <= 0
/// - Rejects missing invoices
/// - Rejects payments to non-payable invoice states
/// - Caps applied amount so `total_paid` never exceeds `total_due`
/// - Enforces nonce uniqueness per `(invoice, nonce)` if nonce is non-empty
/// - Rejects if payment count has reached MAX_PAYMENT_COUNT
///
/// # Security
///
/// - The payer must be the verified invoice business and must authorize the call.
/// - Stored payment records always reflect the applied amount, never the requested excess.
pub fn record_payment(
    env: &Env,
    invoice_id: &BytesN<32>,
    payer: &Address,
    amount: i128,
    payment_nonce: String,
) -> Result<Progress, QuickLendXError> {
    if amount <= 0 {
        return Err(QuickLendXError::InvalidAmount);
    }

    let mut invoice =
        InvoiceStorage::get_invoice(env, invoice_id).ok_or(QuickLendXError::InvoiceNotFound)?;
    ensure_payable_status(&invoice)?;

    if *payer != invoice.business {
        return Err(QuickLendXError::NotBusinessOwner);
    }
    payer.require_auth();

    // Replay protection: reject duplicate nonces.
    if payment_nonce.len() > 0 {
        let nonce_key = SettlementDataKey::PaymentNonce(invoice_id.clone(), payment_nonce.clone());
        let seen: bool = env.storage().persistent().get(&nonce_key).unwrap_or(false);
        if seen {
            // Deduplicate: If transaction_id is already seen, return current progress to ensure idempotency.
            return get_invoice_progress(env, invoice_id);
        }
    }

    let payment_count = get_payment_count_internal(env, invoice_id);

    // Guard against unbounded payment record growth.
    if payment_count >= MAX_PAYMENT_COUNT {
        return Err(QuickLendXError::OperationNotAllowed);
    }

    let remaining_due = compute_remaining_due(&invoice)?;
    if remaining_due <= 0 {
        return Err(QuickLendXError::InvalidStatus);
    }

    let applied_amount = if amount > remaining_due {
        remaining_due
    } else {
        amount
    };

    if applied_amount <= 0 {
        return Err(QuickLendXError::InvalidAmount);
    }

    let new_total_paid = invoice
        .total_paid
        .checked_add(applied_amount)
        .ok_or(QuickLendXError::InvalidAmount)?;

    // Hard invariant: total_paid must never exceed total_due.
    if new_total_paid > invoice.amount {
        return Err(QuickLendXError::InvalidAmount);
    }

    let timestamp = env.ledger().timestamp();
    let payment_record = SettlementPaymentRecord {
        payer: payer.clone(),
        amount: applied_amount,
        timestamp,
        nonce: payment_nonce.clone(),
    };

    env.storage().persistent().set(
        &SettlementDataKey::Payment(invoice_id.clone(), payment_count),
        &payment_record,
    );

    let next_count = payment_count
        .checked_add(1)
        .ok_or(QuickLendXError::StorageError)?;
    env.storage().persistent().set(
        &SettlementDataKey::PaymentCount(invoice_id.clone()),
        &next_count,
    );

    if payment_nonce.len() > 0 {
        env.storage().persistent().set(
            &SettlementDataKey::PaymentNonce(invoice_id.clone(), payment_nonce),
            &true,
        );
    }

    invoice.total_paid = new_total_paid;
    update_inline_payment_history(
        &mut invoice,
        payer.clone(),
        applied_amount,
        timestamp,
        payment_record.nonce,
    );
    InvoiceStorage::update_invoice(env, &invoice);

    emit_payment_recorded(
        env,
        invoice_id,
        payer,
        applied_amount,
        invoice.total_paid,
        &invoice.status,
    );

    get_invoice_progress(env, invoice_id)
}

/// Settle an invoice by applying a final payment amount from the business.
///
/// This function preserves existing behavior by requiring the resulting total
/// payment to satisfy full settlement conditions.
///
/// # Security
/// - Requires an exact final payment equal to the remaining due amount.
/// - Rejects explicit overpayment attempts instead of silently accepting excess input.
/// - Keeps payout, accounting totals, and settlement events aligned to invoice principal.
/// - Rejects if the invoice has already been finalized (double-settle guard).
pub fn settle_invoice(
    env: &Env,
    invoice_id: &BytesN<32>,
    payment_amount: i128,
) -> Result<(), QuickLendXError> {
    if payment_amount <= 0 {
        return Err(QuickLendXError::InvalidAmount);
    }

    // Early double-settle guard: reject if already finalized.
    if is_finalized(env, invoice_id) {
        return Err(QuickLendXError::InvalidStatus);
    }

    let invoice =
        InvoiceStorage::get_invoice(env, invoice_id).ok_or(QuickLendXError::InvoiceNotFound)?;
    ensure_payable_status(&invoice)?;
    let payer = invoice.business.clone();
    payer.require_auth();

    let remaining_due = compute_remaining_due(&invoice)?;
    if payment_amount > remaining_due {
        return Err(QuickLendXError::InvalidAmount);
    }

    let applied_preview = payment_amount;

    if applied_preview <= 0 {
        return Err(QuickLendXError::InvalidAmount);
    }

    let projected_total = invoice
        .total_paid
        .checked_add(applied_preview)
        .ok_or(QuickLendXError::InvalidAmount)?;

    let investment = InvestmentStorage::get_investment_by_invoice(env, invoice_id)
        .ok_or(QuickLendXError::StorageKeyNotFound)?;

    if projected_total < invoice.amount || projected_total < investment.amount {
        return Err(QuickLendXError::PaymentTooLow);
    }

    let nonce = make_settlement_nonce(env);
    record_payment(env, invoice_id, &payer, payment_amount, nonce)?;
    settle_invoice_internal(env, invoice_id)
}

/// Returns aggregate payment progress for an invoice.
///
/// # Returns
/// - `Ok(Progress)` containing `total_due`, `total_paid`, `remaining_due`,
///   `progress_percent`, `payment_count`, and `status`.
pub fn get_invoice_progress(
    env: &Env,
    invoice_id: &BytesN<32>,
) -> Result<Progress, QuickLendXError> {
    let invoice =
        InvoiceStorage::get_invoice(env, invoice_id).ok_or(QuickLendXError::InvoiceNotFound)?;
    let total_due = invoice.amount;
    let total_paid = invoice.total_paid;
    let remaining_due = compute_remaining_due(&invoice)?;

    let progress_percent = if total_due <= 0 {
        0
    } else {
        let scaled = total_paid
            .checked_mul(100)
            .ok_or(QuickLendXError::InvalidAmount)?;
        let pct = scaled
            .checked_div(total_due)
            .ok_or(QuickLendXError::InvalidAmount)?;
        if pct > 100 {
            100
        } else if pct < 0 {
            0
        } else {
            pct as u32
        }
    };

    Ok(Progress {
        total_due,
        total_paid,
        remaining_due,
        progress_percent,
        payment_count: get_payment_count_internal(env, invoice_id),
        status: invoice.status,
    })
}

/// Returns the total number of recorded payments for an invoice.
pub fn get_payment_count(env: &Env, invoice_id: &BytesN<32>) -> Result<u32, QuickLendXError> {
    ensure_invoice_exists(env, invoice_id)?;
    Ok(get_payment_count_internal(env, invoice_id))
}

/// Returns a single payment record by index.
pub fn get_payment_record(
    env: &Env,
    invoice_id: &BytesN<32>,
    index: u32,
) -> Result<SettlementPaymentRecord, QuickLendXError> {
    ensure_invoice_exists(env, invoice_id)?;
    env.storage()
        .persistent()
        .get(&SettlementDataKey::Payment(invoice_id.clone(), index))
        .ok_or(QuickLendXError::StorageKeyNotFound)
}

/// Returns a paginated slice of payment records for an invoice.
///
/// # Arguments
/// * `from` - Starting index (inclusive).
/// * `limit` - Maximum number of records to return.
///
/// Records are returned in chronological order (index 0 = first payment).
pub fn get_payment_records(
    env: &Env,
    invoice_id: &BytesN<32>,
    from: u32,
    limit: u32,
) -> Result<soroban_sdk::Vec<SettlementPaymentRecord>, QuickLendXError> {
    ensure_invoice_exists(env, invoice_id)?;
    let total = get_payment_count_internal(env, invoice_id);
    let mut records = Vec::new(env);

    let actual_limit = limit.min(crate::MAX_QUERY_LIMIT); // Enforce practical upper bound for gas safety
    let end = from.saturating_add(actual_limit).min(total);

    for idx in from..end {
        if let Some(record) = env
            .storage()
            .persistent()
            .get(&SettlementDataKey::Payment(invoice_id.clone(), idx))
        {
            records.push_back(record);
        }
    }

    Ok(records)
}

/// Returns whether an invoice has been finalized (settlement completed).
pub fn is_invoice_finalized(env: &Env, invoice_id: &BytesN<32>) -> Result<bool, QuickLendXError> {
    ensure_invoice_exists(env, invoice_id)?;
    Ok(is_finalized(env, invoice_id))
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn settle_invoice_internal(env: &Env, invoice_id: &BytesN<32>) -> Result<(), QuickLendXError> {
    // Double-finalization guard: reject if already settled.
    if is_finalized(env, invoice_id) {
        return Err(QuickLendXError::InvalidStatus);
    }

    let mut invoice =
        InvoiceStorage::get_invoice(env, invoice_id).ok_or(QuickLendXError::InvoiceNotFound)?;
    ensure_payable_status(&invoice)?;

    let investment = InvestmentStorage::get_investment_by_invoice(env, invoice_id)
        .ok_or(QuickLendXError::StorageKeyNotFound)?;

    if invoice.total_paid < invoice.amount || invoice.total_paid < investment.amount {
        return Err(QuickLendXError::PaymentTooLow);
    }

    // Auto-release escrow funds to business if they are still held in the contract.
    // This ensures the business receives the original funded amount during the settlement transition.
    if let Some(escrow) = crate::payments::EscrowStorage::get_escrow_by_invoice(env, invoice_id) {
        if escrow.status == crate::payments::EscrowStatus::Held {
            crate::payments::release_escrow(env, invoice_id)?;
        }
    }

    let investor_address = invoice
        .investor
        .clone()
        .ok_or(QuickLendXError::NotInvestor)?;

    let (investor_return, platform_fee) = match crate::fees::FeeManager::calculate_platform_fee(
        env,
        investment.amount,
        invoice.total_paid,
    ) {
        Ok(result) => result,
        // Backward-compatible fallback for environments/tests without fee config.
        Err(QuickLendXError::StorageKeyNotFound) => {
            crate::profits::calculate_profit(env, investment.amount, invoice.total_paid)
        }
        Err(error) => return Err(error),
    };

    // Accounting invariant: disbursement must exactly equal total_paid.
    // This prevents any accounting drift from rounding or logic errors.
    let disbursement_total = investor_return
        .checked_add(platform_fee)
        .ok_or(QuickLendXError::InvalidAmount)?;
    if disbursement_total != invoice.total_paid {
        return Err(QuickLendXError::InvalidAmount);
    }

    let business_address = invoice.business.clone();
    transfer_funds(
        env,
        &invoice.currency,
        &business_address,
        &investor_address,
        investor_return,
    )?;

    if platform_fee > 0 {
        let fee_recipient = crate::fees::FeeManager::route_platform_fee(
            env,
            &invoice.currency,
            &business_address,
            platform_fee,
        )?;
        crate::events::emit_platform_fee_routed(env, invoice_id, &fee_recipient, platform_fee);
    }

    // Mark finalized before status transition to prevent re-entry.
    mark_finalized(env, invoice_id);

    let previous_status = invoice.status.clone();
    let paid_at = env.ledger().timestamp();
    invoice.mark_as_paid(env, business_address.clone(), env.ledger().timestamp());
    InvoiceStorage::update_invoice(env, &invoice);

    if previous_status != invoice.status {
        InvoiceStorage::remove_from_status_invoices(env, previous_status.clone(), invoice_id);
        InvoiceStorage::add_to_status_invoices(env, invoice.status.clone(), invoice_id);
    }

    let mut updated_investment = investment;
    updated_investment.status = InvestmentStatus::Completed;
    InvestmentStorage::update_investment(env, &updated_investment);

    emit_invoice_settled(env, &invoice, investor_return, platform_fee);
    emit_invoice_settled_final(env, invoice_id, invoice.total_paid, paid_at);

    Ok(())
}

fn is_finalized(env: &Env, invoice_id: &BytesN<32>) -> bool {
    env.storage()
        .persistent()
        .get(&SettlementDataKey::Finalized(invoice_id.clone()))
        .unwrap_or(false)
}

fn mark_finalized(env: &Env, invoice_id: &BytesN<32>) {
    env.storage()
        .persistent()
        .set(&SettlementDataKey::Finalized(invoice_id.clone()), &true);
}

fn ensure_invoice_exists(env: &Env, invoice_id: &BytesN<32>) -> Result<(), QuickLendXError> {
    if InvoiceStorage::get_invoice(env, invoice_id).is_none() {
        return Err(QuickLendXError::InvoiceNotFound);
    }
    Ok(())
}

fn ensure_payable_status(invoice: &Invoice) -> Result<(), QuickLendXError> {
    if invoice.status == InvoiceStatus::Paid
        || invoice.status == InvoiceStatus::Cancelled
        || invoice.status == InvoiceStatus::Defaulted
        || invoice.status == InvoiceStatus::Refunded
    {
        return Err(QuickLendXError::InvalidStatus);
    }

    if invoice.status != InvoiceStatus::Funded {
        return Err(QuickLendXError::InvalidStatus);
    }

    Ok(())
}

fn compute_remaining_due(invoice: &Invoice) -> Result<i128, QuickLendXError> {
    if invoice.amount <= 0 {
        return Err(QuickLendXError::InvoiceAmountInvalid);
    }

    if invoice.total_paid < 0 {
        return Err(QuickLendXError::InvalidAmount);
    }

    if invoice.total_paid >= invoice.amount {
        return Ok(0);
    }

    invoice
        .amount
        .checked_sub(invoice.total_paid)
        .ok_or(QuickLendXError::InvalidAmount)
}

fn update_inline_payment_history(
    invoice: &mut Invoice,
    payer: Address,
    amount: i128,
    timestamp: u64,
    nonce: String,
) {
    if invoice.payment_history.len() >= MAX_INLINE_PAYMENT_HISTORY {
        invoice.payment_history.remove(0u32);
    }

    invoice.payment_history.push_back(InvoicePaymentRecord {
        payer,
        amount,
        timestamp,
        transaction_id: nonce,
    });
}

fn get_payment_count_internal(env: &Env, invoice_id: &BytesN<32>) -> u32 {
    env.storage()
        .persistent()
        .get(&SettlementDataKey::PaymentCount(invoice_id.clone()))
        .unwrap_or(0)
}

fn get_last_applied_amount(env: &Env, invoice_id: &BytesN<32>) -> Result<i128, QuickLendXError> {
    let count = get_payment_count_internal(env, invoice_id);
    if count == 0 {
        return Err(QuickLendXError::StorageKeyNotFound);
    }

    let last_index = count.saturating_sub(1);
    let record = get_payment_record(env, invoice_id, last_index)?;
    Ok(record.amount)
}

fn make_settlement_nonce(env: &Env) -> String {
    // Full settlement can only succeed once per invoice (status becomes Paid),
    // so a static nonce is sufficient for this internal path.
    String::from_str(env, "settlement")
}

fn emit_payment_recorded(
    env: &Env,
    invoice_id: &BytesN<32>,
    payer: &Address,
    applied_amount: i128,
    total_paid: i128,
    status: &InvoiceStatus,
) {
    env.events().publish(
        (symbol_short!("pay_rec"),),
        (
            invoice_id.clone(),
            payer.clone(),
            applied_amount,
            total_paid,
            status.clone(),
        ),
    );
}

fn emit_invoice_settled_final(
    env: &Env,
    invoice_id: &BytesN<32>,
    final_amount: i128,
    paid_at: u64,
) {
    env.events().publish(
        (symbol_short!("inv_stlf"),),
        (invoice_id.clone(), final_amount, paid_at),
    );
}
