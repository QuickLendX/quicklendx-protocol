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
//!
//! # Settlement-Dispute Interaction Invariants
//!
//! ## Critical Safety Property: Mutual Exclusion
//! **Settlement finalization is BLOCKED while `dispute_status != DisputeStatus::None`.**
//!
//! ### Rationale
//! Disputes represent contested invoice states. Allowing settlement during disputes could:
//! - Release funds to a party later determined to be in breach
//! - Create irreversible state contradicting dispute resolution
//! - Prevent proper refund pathways for the disadvantaged party
//!
//! ### Implementation
//! `settle_invoice_internal()` enforces two sequential guards:
//! 1. `ensure_payable_status()` Ã¢â‚¬â€ invoice must be `Funded`.
//! 2. **Dispute-active guard** Ã¢â‚¬â€ `invoice.dispute_status` must NOT be
//!    `Disputed` or `UnderReview`; returns `QuickLendXError::DisputeActive`
//!    (2204) while either open state is present.
//!    `Resolved` is intentionally allowed: once the admin has issued a
//!    ruling the dispute is concluded and the admin's outcome governs, so
//!    a business-favourable resolution can proceed to settlement normally.
//!
//! The explicit check is required because disputes do NOT change `invoice.status`;
//! the invoice remains `Funded` throughout the dispute lifecycle.  Without the
//! second guard a business could finalize settlement during an active dispute,
//! releasing escrowed funds before admin resolution and closing the investor's
//! refund pathway.  See `test_settle_blocked_while_disputed` and
//! `test_settle_blocked_while_under_review` (negative tests) for regression
//! coverage, and `test_settle_allowed_after_dispute_resolved` for the
//! unblock path.
//!
//! ### Partial Payments During Disputes
//! `record_payment()` continues to function during disputes to:
//! - Track business good-faith payment attempts
//! - Provide payment history for dispute resolution
//! - Avoid hostile user experience (blocking all payments)
//!
//! However, `settle_invoice_internal()` will block finalization, so `total_paid` may
//! reach `invoice.amount` without triggering settlement completion.
//!
//! ### Escrow Safety During Disputes
//! - Escrow release requires `invoice.status == Paid` (unreachable during dispute)
//! - Escrow refund requires `invoice.status == Cancelled/Refunded`
//! - Dispute resolution determines which outcome (release vs. refund) becomes available
//!
//! **See**: `docs/settlement-dispute-interaction.md` for complete state machine and
//! resolution outcome mappings.
//!
//! ## Dispute Resolution Outcomes
//!
//! ### 1. Resolution in Favor of Investor
//! - Admin transitions invoice to `Cancelled` or `Refunded`
//! - Escrow refund becomes available via `refund_escrow()`
//! - Settlement permanently blocked
//! - **Guarantee**: Investor recovers principal; business does not receive funds
//!
//! ### 2. Resolution in Favor of Business
//! - Invoice returns to `Funded` (or equivalent settleable state)
//! - Business completes remaining payments
//! - Settlement proceeds normally via `settle_invoice()`
//! - **Guarantee**: Investor receives agreed returns; platform receives fees
//!
//! ### 3. Neutral Resolution
//! - Platform policy applies (settlement proceeds, partial refund, or mediation)
//! - **Guarantee**: No permanent fund freeze; deterministic resolution path provided
//!
//! ## Testing
//! Comprehensive integration tests validate:
//! - Settlement blocked during `Disputed` and `UnderReview` statuses
//! - Escrow double-spend prevention during state transitions
//! - Refund pathway integrity after investor-favorable resolution
//! - Settlement unblock after business-favorable resolution
//!
//! **See**: `src/test_settlement_dispute_interaction.rs` for complete test matrix.

use crate::errors::QuickLendXError;
use crate::events::{emit_invoice_settled, emit_partial_payment, emit_repayment_allocated};
use crate::investment::InvestmentStorage;
use crate::payments::transfer_funds;
use crate::profits::BPS_DENOMINATOR;
use crate::storage::InvoiceStorage;
use crate::types::InvestmentStatus;
use crate::types::{DisputeStatus, Invoice, InvoiceStatus, PaymentRecord as InvoicePaymentRecord};
use soroban_sdk::{contracttype, symbol_short, Address, BytesN, Env, String, Vec};

const MAX_INLINE_PAYMENT_HISTORY: u32 = 32;

/// Maximum number of discrete payment records per invoice.
/// Prevents unbounded storage growth and protects against payment-count overflow.
const MAX_PAYMENT_COUNT: u32 = 1_000;

/// Suggested default page size for off-chain consumers fetching settlement/payment records.
/// This is a soft hint, not a hard limit. Indexers can request fewer or more records per query
/// up to `MAX_QUERY_LIMIT` (currently 50). The soft cap helps standardize pagination patterns
/// across different clients while allowing flexibility for specific use cases.
pub const DEFAULT_SETTLEMENT_BATCH_SIZE_SOFT_CAP: u32 = 25;

/// Hard upper bound for settlement batch queries enforced by the contract.
/// This matches `crate::MAX_QUERY_LIMIT` and represents the maximum number of payment
/// records that can be returned in a single `get_payment_records` query.
pub const MAX_SETTLEMENT_BATCH_SIZE_SOFT_CAP: u32 = 50;

#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(test, derive(Debug))]
enum SettlementDataKey {
    PaymentCount(BytesN<32>),
    Payment(BytesN<32>, u32),
    PaymentNonce(BytesN<32>, String),
    /// Marks an invoice as finalized to guard against double-settlement.
    Finalized(BytesN<32>),
    /// Per-invoice settlement currency whitelist (defence-in-depth).
    /// Stored at invoice creation; checked at settlement time.
    SettlementCurrencies(BytesN<32>),
    /// Cumulative repayment allocation ledger for an invoice.
    Allocation(BytesN<32>),
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

/// Durable cumulative repayment buckets for an invoice.
///
/// Additive storage: missing keys reconstruct from `invoice.total_paid` with
/// `assessed_late = 0` (no retroactive late fee on upgrade).
#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(test, derive(Debug))]
pub struct RepaymentLedger {
    pub principal: i128,
    pub investor_profit: i128,
    pub platform_fee: i128,
    pub late_penalty: i128,
    pub total_paid: i128,
    pub assessed_late: i128,
    pub late_assessed: bool,
}

/// Result of a committed `record_payment` call.
pub struct RecordedPayment {
    pub progress: Progress,
    pub operation_id: BytesN<32>,
    pub applied_amount: i128,
    pub fee_bps: i128,
    pub applied_principal: i128,
    pub applied_investor_profit: i128,
    pub applied_platform_fee: i128,
    pub applied_late_penalty: i128,
    pub ledger: RepaymentLedger,
}

/// Allocate cumulative repayment buckets for a given `total_paid`.
///
/// Waterfall: principal → contractual profit (platform fee split) → investor late penalty.
///
/// # Invariants
/// `principal + investor_profit + platform_fee + late_penalty == total_paid`
pub fn allocate_cumulative_repayment(
    investment: i128,
    face: i128,
    total_paid: i128,
    fee_bps: i128,
    assessed_late: i128,
) -> Result<RepaymentLedger, QuickLendXError> {
    if investment < 0 || face <= 0 || total_paid < 0 || assessed_late < 0 {
        return Err(QuickLendXError::InvalidAmount);
    }
    if investment > crate::protocol_limits::MAX_INVOICE_AMOUNT
        || face > crate::protocol_limits::MAX_INVOICE_AMOUNT
        || assessed_late > crate::protocol_limits::MAX_INVOICE_AMOUNT
    {
        return Err(QuickLendXError::InvalidAmount);
    }

    let fee_bps = fee_bps.clamp(0, BPS_DENOMINATOR);

    let principal = if total_paid < investment {
        total_paid
    } else {
        investment
    };
    let after_principal = total_paid
        .checked_sub(principal)
        .ok_or(QuickLendXError::ArithmeticOverflow)?;

    let profit_cap = if face > investment {
        face.checked_sub(investment)
            .ok_or(QuickLendXError::ArithmeticOverflow)?
    } else {
        0
    };
    let profit_pool = if after_principal < profit_cap {
        after_principal
    } else {
        profit_cap
    };

    let platform_fee = profit_pool
        .checked_mul(fee_bps)
        .ok_or(QuickLendXError::ArithmeticOverflow)?
        .checked_div(BPS_DENOMINATOR)
        .ok_or(QuickLendXError::ArithmeticOverflow)?;
    let investor_profit = profit_pool
        .checked_sub(platform_fee)
        .ok_or(QuickLendXError::ArithmeticOverflow)?;

    let after_profit = after_principal
        .checked_sub(profit_pool)
        .ok_or(QuickLendXError::ArithmeticOverflow)?;
    let late_penalty = if after_profit < assessed_late {
        after_profit
    } else {
        assessed_late
    };
    let leftover = after_profit
        .checked_sub(late_penalty)
        .ok_or(QuickLendXError::ArithmeticOverflow)?;
    if leftover != 0 {
        return Err(QuickLendXError::InvalidAmount);
    }

    let recon = principal
        .checked_add(investor_profit)
        .ok_or(QuickLendXError::ArithmeticOverflow)?
        .checked_add(platform_fee)
        .ok_or(QuickLendXError::ArithmeticOverflow)?
        .checked_add(late_penalty)
        .ok_or(QuickLendXError::ArithmeticOverflow)?;
    if recon != total_paid {
        return Err(QuickLendXError::InvalidAmount);
    }

    Ok(RepaymentLedger {
        principal,
        investor_profit,
        platform_fee,
        late_penalty,
        total_paid,
        assessed_late,
        late_assessed: false,
    })
}

fn empty_repayment_ledger() -> RepaymentLedger {
    RepaymentLedger {
        principal: 0,
        investor_profit: 0,
        platform_fee: 0,
        late_penalty: 0,
        total_paid: 0,
        assessed_late: 0,
        late_assessed: false,
    }
}

/// Read the stored repayment ledger, if any.
pub fn get_repayment_ledger(env: &Env, invoice_id: &BytesN<32>) -> Option<RepaymentLedger> {
    env.storage()
        .persistent()
        .get(&SettlementDataKey::Allocation(invoice_id.clone()))
}

fn store_repayment_ledger(env: &Env, invoice_id: &BytesN<32>, ledger: &RepaymentLedger) {
    env.storage()
        .persistent()
        .set(&SettlementDataKey::Allocation(invoice_id.clone()), ledger);
}

fn settlement_fee_bps(env: &Env) -> i128 {
    if let Ok(config) = crate::fees::FeeManager::get_platform_fee_config(env) {
        return (config.fee_bps as i128).clamp(0, BPS_DENOMINATOR);
    }
    crate::profits::PlatformFee::get_config(env).fee_bps as i128
}

fn investment_principal(env: &Env, invoice: &Invoice) -> i128 {
    InvestmentStorage::get_investment_by_invoice(env, &invoice.id)
        .map(|inv| inv.amount)
        .unwrap_or(invoice.amount)
}

fn load_or_reconstruct_ledger(
    env: &Env,
    invoice: &Invoice,
    fee_bps: i128,
) -> Result<RepaymentLedger, QuickLendXError> {
    if let Some(ledger) = get_repayment_ledger(env, &invoice.id) {
        return Ok(ledger);
    }
    if invoice.total_paid == 0 {
        return Ok(empty_repayment_ledger());
    }
    let mut ledger = allocate_cumulative_repayment(
        investment_principal(env, invoice),
        invoice.amount,
        invoice.total_paid,
        fee_bps,
        0,
    )?;
    ledger.assessed_late = 0;
    ledger.late_assessed = false;
    Ok(ledger)
}

fn encode_allocation_audit(
    env: &Env,
    applied: i128,
    principal: i128,
    investor_profit: i128,
    platform_fee: i128,
    late_penalty: i128,
) -> String {
    let mut buf = [0u8; 192];
    let mut n = 0usize;
    let mut push = |label: &[u8], value: i128| {
        if n > 0 && n < buf.len() {
            buf[n] = b',';
            n += 1;
        }
        for b in label {
            if n < buf.len() {
                buf[n] = *b;
                n += 1;
            }
        }
        let mut num = [0u8; 41];
        let len = crate::audit::write_i128_to_buf(&mut num, value);
        for i in 0..len {
            if n < buf.len() {
                buf[n] = num[i];
                n += 1;
            }
        }
    };
    push(b"a=", applied);
    push(b"p=", principal);
    push(b"ip=", investor_profit);
    push(b"pf=", platform_fee);
    push(b"l=", late_penalty);
    String::from_str(env, core::str::from_utf8(&buf[..n]).unwrap_or("alloc"))
}

/// Record a partial payment for an invoice.
///
/// If the total paid amount reaches the invoice total, the settlement is finalized.
/// This method provides strictly ordered record persistence and rejects duplicate nonces.
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
    let mut cache = crate::storage::StorageReadCache::new();

    let invoice = cache
        .get_invoice(env, invoice_id)
        .ok_or(QuickLendXError::InvoiceNotFound)?;
    let payer = invoice.business.clone();

    crate::qlx_log!(
        env,
        "settlement",
        "Recording partial payment: amount={}",
        payment_amount
    );

    let recorded = record_payment(
        env,
        invoice_id,
        &payer,
        payment_amount,
        transaction_id.clone(),
    )?;

    // Invalidate cache since record_payment may have updated the invoice in storage.
    cache.invalidate_invoice(invoice_id);

    // Read updated invoice once; the cache avoids a redundant storage trip for
    // the notification below.
    let invoice_post = cache
        .get_invoice(env, invoice_id)
        .ok_or(QuickLendXError::InvoiceNotFound)?;

    // Backward-compatible event used across existing tests/consumers.
    emit_partial_payment(
        env,
        &invoice_post,
        recorded.applied_amount,
        recorded.progress.total_paid,
        recorded.progress.progress_percent,
        transaction_id,
    );

    emit_repayment_allocated(
        env,
        recorded.operation_id.clone(),
        invoice_id,
        &payer,
        recorded.applied_principal,
        recorded.applied_investor_profit,
        recorded.applied_platform_fee,
        recorded.applied_late_penalty,
        recorded.ledger.principal,
        recorded.ledger.investor_profit,
        recorded.ledger.platform_fee,
        recorded.ledger.late_penalty,
        recorded.progress.total_paid,
        recorded.progress.total_due,
        recorded.fee_bps,
    );

    // Lifecycle trigger: emits `NotificationType::PaymentReceived` for each
    // applied partial payment. Notification failures must not roll back funds.
    let _ = crate::notifications::NotificationSystem::notify_payment_received(
        env,
        &invoice_post,
        recorded.applied_amount,
    );

    if progress.total_paid >= progress.total_due {
        settle_invoice_internal(env, invoice_id, &payer)?;
    }

    Ok(())
}

/// Record a payment attempt with capping, replay protection, and durable storage.
///
/// This function is the core payment recording primitive. It validates, caps, and
/// persists payment records while maintaining critical security invariants.
///
/// # Arguments
/// - `invoice_id`: Unique identifier for the invoice being paid.
/// - `payer`: Verified invoice business address (must match invoice.business).
/// - `amount`: The requested payment amount (may be capped if overpaying).
/// - `payment_nonce`: Unique transaction identifier; empty string skips replay check.
///
/// # Returns
/// - `Ok(Progress)` containing updated payment state.
/// - `Err(QuickLendXError)` on validation failure.
///
/// # Security Invariants (Fuzz-Tested)
///
/// 1. **Capping Invariant**: `total_paid` never exceeds `total_due`. If `amount > remaining_due`,
///    only `remaining_due` is applied. This prevents overpayment attacks and ensures the
///    accounting identity `investor_return + platform_fee == total_paid` holds.
///
/// 2. **Replay Protection Invariant**: Each `(invoice_id, nonce)` pair is unique. Duplicate
///    nonces are rejected with `DuplicateNonce`. Empty nonces bypass this check intentionally
///    (caller responsibility for uniqueness).
///
/// 3. **Payment Count Bound**: `payment_count <= MAX_PAYMENT_COUNT`. Payment count exhaustion
///    returns `OperationNotAllowed` and cannot be bypassed.
///
/// # Error Conditions
/// - `InvalidAmount`: `amount <= 0`, `applied_amount <= 0`, or `new_total_paid > total_due`.
/// - `InvoiceNotFound`: No invoice exists for `invoice_id`.
/// - `InvalidStatus`: Invoice is not in `Funded` state or `remaining_due == 0`.
/// - `NotBusinessOwner`: `payer` does not match invoice business.
/// - `OperationNotAllowed`: Payment count has reached `MAX_PAYMENT_COUNT`.
/// - `DuplicateNonce`: `payment_nonce` has already been recorded for this invoice.
pub fn record_payment(
    env: &Env,
    invoice_id: &BytesN<32>,
    payer: &Address,
    amount: i128,
    payment_nonce: String,
) -> Result<RecordedPayment, QuickLendXError> {
    if amount <= 0 {
        return Err(QuickLendXError::InvalidAmount);
    }

    if crate::storage::InvoiceStorage::is_frozen(env, invoice_id) {
        crate::storage::InvoiceStorage::require_lock_within_time_limit(env, invoice_id)?;
        return Err(QuickLendXError::InvoiceFrozen);
    }

    let mut invoice =
        InvoiceStorage::get_invoice(env, invoice_id).ok_or(QuickLendXError::InvoiceNotFound)?;
    ensure_payable_status(&invoice)?;

    if *payer != invoice.business {
        return Err(QuickLendXError::NotBusinessOwner);
    }
    payer.require_auth();

    crate::verification::validate_transaction_hash(env, &payment_nonce)?;

    let nonce_key = SettlementDataKey::PaymentNonce(invoice_id.clone(), payment_nonce.clone());
    let seen: bool = env.storage().persistent().get(&nonce_key).unwrap_or(false);
    if seen {
        return Err(QuickLendXError::DuplicateNonce);
    }

    let payment_count = get_payment_count_internal(env, invoice_id);
    if payment_count >= MAX_PAYMENT_COUNT {
        return Err(QuickLendXError::OperationNotAllowed);
    }

    let fee_bps = settlement_fee_bps(env);
    let mut ledger = load_or_reconstruct_ledger(env, &invoice, fee_bps)?;

    if !ledger.late_assessed
        && env.ledger().timestamp() > invoice.due_date
        && invoice
            .late_payment_penalty_bps
            .map(|bps| bps > 0)
            .unwrap_or(false)
    {
        let remaining_contractual = contractual_remaining(&invoice)?;
        let bps = invoice.late_payment_penalty_bps.unwrap_or(0) as i128;
        ledger.assessed_late = remaining_contractual
            .checked_mul(bps)
            .ok_or(QuickLendXError::ArithmeticOverflow)?
            .checked_div(BPS_DENOMINATOR)
            .ok_or(QuickLendXError::ArithmeticOverflow)?;
        ledger.late_assessed = true;
    }

    let remaining_due = remaining_due_with_late(&invoice, ledger.assessed_late)?;
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
    let total_due = invoice
        .amount
        .checked_add(ledger.assessed_late)
        .ok_or(QuickLendXError::InvalidAmount)?;
    if new_total_paid > total_due {
        return Err(QuickLendXError::InvalidAmount);
    }

    let previous = ledger.clone();
    let mut next_ledger = allocate_cumulative_repayment(
        investment_principal(env, &invoice),
        invoice.amount,
        new_total_paid,
        fee_bps,
        ledger.assessed_late,
    )?;
    next_ledger.assessed_late = ledger.assessed_late;
    next_ledger.late_assessed = ledger.late_assessed;

    let applied_principal = next_ledger
        .principal
        .checked_sub(previous.principal)
        .ok_or(QuickLendXError::ArithmeticOverflow)?;
    let applied_investor_profit = next_ledger
        .investor_profit
        .checked_sub(previous.investor_profit)
        .ok_or(QuickLendXError::ArithmeticOverflow)?;
    let applied_platform_fee = next_ledger
        .platform_fee
        .checked_sub(previous.platform_fee)
        .ok_or(QuickLendXError::ArithmeticOverflow)?;
    let applied_late_penalty = next_ledger
        .late_penalty
        .checked_sub(previous.late_penalty)
        .ok_or(QuickLendXError::ArithmeticOverflow)?;
    if applied_principal < 0
        || applied_investor_profit < 0
        || applied_platform_fee < 0
        || applied_late_penalty < 0
    {
        return Err(QuickLendXError::InvalidAmount);
    }

    let operation_id = crate::observability::allocate_operation_id(env);
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

    if !payment_nonce.is_empty() {
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
    store_repayment_ledger(env, invoice_id, &next_ledger);

    crate::qlx_log!(
        env,
        "settlement",
        "Payment recorded: applied={} total_paid={}",
        applied_amount,
        new_total_paid
    );

    emit_payment_recorded(
        env,
        invoice_id,
        payer,
        applied_amount,
        invoice.total_paid,
        &invoice.status,
    );

    crate::audit::log_payment_processed_with_id(
        env,
        invoice_id.clone(),
        payer.clone(),
        applied_amount,
        encode_allocation_audit(
            env,
            applied_amount,
            applied_principal,
            applied_investor_profit,
            applied_platform_fee,
            applied_late_penalty,
        ),
        operation_id.clone(),
    );

    let progress = get_invoice_progress(env, invoice_id)?;
    Ok(RecordedPayment {
        progress,
        operation_id,
        applied_amount,
        fee_bps,
        applied_principal,
        applied_investor_profit,
        applied_platform_fee,
        applied_late_penalty,
        ledger: next_ledger,
    })
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
    snap: &crate::types::Investment,
    business: &Address,
) -> Result<(), QuickLendXError> {
    if payment_amount <= 0 {
        return Err(QuickLendXError::InvalidAmount);
    }

    crate::qlx_log!(
        env,
        "settlement",
        "Full settlement initiated: payment={}",
        payment_amount
    );

    // Early double-settle guard: reject if already finalized.
    if is_finalized(env, invoice_id) {
        return Err(QuickLendXError::InvalidStatus);
    }

    if crate::storage::InvoiceStorage::is_frozen(env, invoice_id) {
        crate::storage::InvoiceStorage::require_lock_within_time_limit(env, invoice_id)?;
        return Err(QuickLendXError::InvoiceFrozen);
    }

    let invoice =
        InvoiceStorage::get_invoice(env, invoice_id).ok_or(QuickLendXError::InvoiceNotFound)?;
    ensure_payable_status(&invoice)?;
    require_no_active_dispute(&invoice)?;
    let payer = invoice.business.clone();

    let remaining_due = compute_remaining_due(env, &invoice)?;
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

    let investment = InvestmentStorage::get_investment_by_invoice(env, invoice_id).unwrap();

    if projected_total < investment.amount {
        return Err(QuickLendXError::PaymentTooLow);
    }
    let total_due = invoice
        .amount
        .checked_add(preview_assessed_late(env, &invoice)?)
        .ok_or(QuickLendXError::InvalidAmount)?;
    if projected_total < total_due {
        return Err(QuickLendXError::PaymentTooLow);
    }

    let nonce = make_settlement_nonce(env);
    record_payment(env, invoice_id, &payer, payment_amount, nonce)?;
    settle_invoice_internal(env, invoice_id, business)
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
    let total_due = invoice
        .amount
        .checked_add(preview_assessed_late(env, &invoice)?)
        .ok_or(QuickLendXError::InvalidAmount)?;
    let total_paid = invoice.total_paid;
    let remaining_due = compute_remaining_due(env, &invoice)?;

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

/// Returns the suggested default page size for fetching settlement/payment records.
/// This is a soft hint for off-chain indexers and query clients. The actual limit
/// enforced by `get_payment_records` may differ based on `MAX_QUERY_LIMIT`.
///
/// # Returns
/// `DEFAULT_SETTLEMENT_BATCH_SIZE_SOFT_CAP` (25) Ã¢â‚¬â€ the recommended batch size.
pub fn default_settlement_batch_size_soft_cap() -> u32 {
    DEFAULT_SETTLEMENT_BATCH_SIZE_SOFT_CAP
}

/// Returns the maximum page size for fetching settlement/payment records.
/// This represents the hard upper bound enforced by the contract. Query requests
/// exceeding this limit will be clamped to this value.
///
/// # Returns
/// `MAX_SETTLEMENT_BATCH_SIZE_SOFT_CAP` (50) Ã¢â‚¬â€ the maximum allowed batch size.
pub fn max_settlement_batch_size_soft_cap() -> u32 {
    MAX_SETTLEMENT_BATCH_SIZE_SOFT_CAP
}

/// Store the per-invoice settlement currency whitelist.
///
/// Called at invoice creation time to record the currencies that may be used
/// to settle this invoice.  By default the whitelist contains only the
/// invoice's own `currency`, providing defence-in-depth against storage-level
/// corruption of `invoice.currency`.
///
/// When the stored whitelist is empty, no restriction is enforced (backward
/// compatible fallback for invoices created before this feature).
///
/// # Arguments
/// * `env` Ã¢â‚¬â€ The contract environment.
/// * `invoice_id` Ã¢â‚¬â€ The invoice whose whitelist to set.
/// * `currencies` Ã¢â‚¬â€ Allowed settlement currencies for this invoice.
pub fn store_settlement_currencies(
    env: &Env,
    invoice_id: &BytesN<32>,
    currencies: &soroban_sdk::Vec<Address>,
) {
    env.storage().persistent().set(
        &SettlementDataKey::SettlementCurrencies(invoice_id.clone()),
        currencies,
    );
}

/// Check that `invoice_currency` is in the per-invoice settlement currency
/// whitelist.  When no whitelist is stored (backward compat) the check passes.
fn require_settlement_currency_allowed(
    env: &Env,
    invoice_id: &BytesN<32>,
    invoice_currency: &Address,
) -> Result<(), QuickLendXError> {
    let stored: Option<soroban_sdk::Vec<Address>> = env
        .storage()
        .persistent()
        .get(&SettlementDataKey::SettlementCurrencies(invoice_id.clone()));
    if let Some(allowed) = stored {
        if allowed.is_empty() {
            return Ok(());
        }
        for c in allowed.iter() {
            if c == *invoice_currency {
                return Ok(());
            }
        }
        return Err(QuickLendXError::SettlementCurrencyNotAllowed);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn settle_invoice_internal(
    env: &Env,
    invoice_id: &BytesN<32>,
    business: &Address,
) -> Result<(), QuickLendXError> {
    // Double-finalization guard: reject if already settled.
    if is_finalized(env, invoice_id) {
        return Err(QuickLendXError::InvalidStatus);
    }

    let mut invoice =
        InvoiceStorage::get_invoice(env, invoice_id).ok_or(QuickLendXError::InvoiceNotFound)?;
    ensure_payable_status(&invoice)?;
    require_no_active_dispute(&invoice)?;

    let investment = InvestmentStorage::get_investment_by_invoice(env, invoice_id).unwrap();

    if compute_remaining_due(env, &invoice)? != 0 || invoice.total_paid < investment.amount {
        return Err(QuickLendXError::PaymentTooLow);
    }

    // Auto-release escrow funds to business if they are still held in the contract.
    // This ensures the business receives the original funded amount during the settlement transition.
    if let Some(escrow) = crate::payments::EscrowStorage::get_escrow_by_invoice(env, invoice_id) {
        if escrow.status == crate::payments::EscrowStatus::Held {
            crate::payments::release_escrow(env, invoice_id, business)?;
        }
    }

    let investor_address = invoice
        .investor
        .clone()
        .ok_or(QuickLendXError::NotInvestor)?;

    let fee_bps = settlement_fee_bps(env);
    let ledger = load_or_reconstruct_ledger(env, &invoice, fee_bps)?;
    let investor_return = ledger
        .principal
        .checked_add(ledger.investor_profit)
        .ok_or(QuickLendXError::ArithmeticOverflow)?
        .checked_add(ledger.late_penalty)
        .ok_or(QuickLendXError::ArithmeticOverflow)?;
    let platform_fee = ledger.platform_fee;

    // Accounting invariant: disbursement must exactly equal total_paid.
    // This prevents any accounting drift from rounding or logic errors.
    let disbursement_total = investor_return
        .checked_add(platform_fee)
        .ok_or(QuickLendXError::InvalidAmount)?;
    if disbursement_total != invoice.total_paid {
        return Err(QuickLendXError::InvalidAmount);
    }
    // Defense-in-depth (#2464): re-verify the same identity through the
    // crate's dedicated, independently-tested dust-check utility rather than
    // trusting the arithmetic above alone -- matches the "explicit guard
    // against future arithmetic changes" pattern already used elsewhere in
    // this crate (see `Investment::calculate_premium`).
    if !crate::profits::verify_no_dust(investor_return, platform_fee, invoice.total_paid) {
        return Err(QuickLendXError::InvalidAmount);
    }

    // #2464: mark finalized now, before any fund movement or other
    // externally observable effect below -- not after, as this function
    // used to. Every accounting/authorization/lifecycle check for this
    // settlement has passed by this point and nothing has happened yet, so
    // this is the correct checks-effects-interactions boundary: the only
    // way a caller can ever observe `is_finalized(invoice_id) == true` is
    // once every precondition already held, matching the ordering
    // `record_payment` and `handle_default`'s guard-setting already follow
    // elsewhere in this crate. Soroban's own transaction atomicity already
    // reverts every effect below if anything after this point fails or
    // panics, so this change doesn't alter what a failed call leaves
    // behind -- it exists so this function's own ordering matches that
    // guarantee rather than relying on it silently.
    mark_finalized(env, invoice_id);

    // Auto-release escrow funds to business if they are still held in the contract.
    // This ensures the business receives the original funded amount during the settlement transition.
    if let Some(escrow) = crate::payments::EscrowStorage::get_escrow_by_invoice(env, invoice_id) {
        if escrow.status == crate::payments::EscrowStatus::Held {
            crate::payments::release_escrow(env, invoice_id)?;
        }
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

    let previous_status = invoice.status;
    let paid_at = env.ledger().timestamp();
    invoice.mark_as_paid(env, business_address.clone(), env.ledger().timestamp());
    InvoiceStorage::update_invoice(env, &invoice);

    if previous_status != invoice.status {
        InvoiceStorage::remove_from_status_invoices(env, previous_status, invoice_id);
        InvoiceStorage::add_to_status_invoices(env, invoice.status, invoice_id);
    }

    let mut updated_investment = investment;
    updated_investment.status = InvestmentStatus::Completed;
    InvestmentStorage::update_investment(env, &updated_investment);

    crate::qlx_log!(
        env,
        "settlement",
        "Invoice settled: investor_return={} platform_fee={}",
        investor_return,
        platform_fee
    );

    emit_invoice_settled(env, &invoice, investor_return, platform_fee);
    emit_invoice_settled_final(env, invoice_id, invoice.total_paid, paid_at);

    // Lifecycle trigger: emits `NotificationType::InvoiceStatusChanged` when an
    // invoice reaches the terminal `Paid` state during final settlement.
    let _ = crate::notifications::NotificationSystem::notify_invoice_status_changed(
        env,
        &invoice,
        &previous_status,
        &invoice.status,
    );

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
    // Explicit transition matrix for payments:
    // Only Funded and Defaulted (late payments) invoices can accept payments.
    if invoice.status == InvoiceStatus::Paid
        || invoice.status == InvoiceStatus::Cancelled
        || invoice.status == InvoiceStatus::Refunded
    {
        return Err(QuickLendXError::InvalidStatus);
    }

    if invoice.status != InvoiceStatus::Funded && invoice.status != InvoiceStatus::Defaulted {
        return Err(QuickLendXError::InvalidStatus);
    }

    Ok(())
}

fn require_no_active_dispute(invoice: &Invoice) -> Result<(), QuickLendXError> {
    if invoice.dispute_status == DisputeStatus::Disputed
        || invoice.dispute_status == DisputeStatus::UnderReview
    {
        return Err(QuickLendXError::DisputeActive);
    }
    Ok(())
}

fn contractual_remaining(invoice: &Invoice) -> Result<i128, QuickLendXError> {
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

fn remaining_due_with_late(
    invoice: &Invoice,
    assessed_late: i128,
) -> Result<i128, QuickLendXError> {
    let total_due = invoice
        .amount
        .checked_add(assessed_late)
        .ok_or(QuickLendXError::InvalidAmount)?;
    if invoice.total_paid >= total_due {
        return Ok(0);
    }
    total_due
        .checked_sub(invoice.total_paid)
        .ok_or(QuickLendXError::InvalidAmount)
}

fn preview_assessed_late(env: &Env, invoice: &Invoice) -> Result<i128, QuickLendXError> {
    if let Some(ledger) = get_repayment_ledger(env, &invoice.id) {
        return Ok(ledger.assessed_late);
    }
    if env.ledger().timestamp() > invoice.due_date {
        if let Some(bps) = invoice.late_payment_penalty_bps {
            if bps > 0 {
                let remaining = contractual_remaining(invoice)?;
                return remaining
                    .checked_mul(bps as i128)
                    .ok_or(QuickLendXError::ArithmeticOverflow)?
                    .checked_div(BPS_DENOMINATOR)
                    .ok_or(QuickLendXError::ArithmeticOverflow);
            }
        }
    }
    Ok(0)
}

fn compute_remaining_due(env: &Env, invoice: &Invoice) -> Result<i128, QuickLendXError> {
    remaining_due_with_late(invoice, preview_assessed_late(env, invoice)?)
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
            *status,
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

#[cfg(all(test, feature = "legacy-tests"))]
mod test {
    use super::*;
    use crate::investment::InvestmentStorage;
    use crate::types::{Investment, InvestmentStatus};
    use soroban_sdk::{testutils::Address as _, Address, BytesN, Env, Vec};

    fn create_test_investment(env: &Env, invoice_id: &BytesN<32>, amount: i128) -> Investment {
        Investment {
            investment_id: BytesN::from_array(env, &[1; 32]),
            invoice_id: invoice_id.clone(),
            investor: Address::generate(env),
            amount,
            funded_at: 100,
            status: InvestmentStatus::Active,
            insurance: Vec::new(env),
        }
    }

    #[test]
    fn test_investment_snapshot_fresh() {
        let env = Env::default();
        env.mock_all_auths();
        let invoice_id = BytesN::from_array(&env, &[0; 32]);
        let investment = create_test_investment(&env, &invoice_id, 1000);

        InvestmentStorage::store_investment(&env, &investment);

        let result = require_matching_investment_snapshot(&env, &invoice_id, &investment);
        assert!(result.is_ok());
    }

    #[test]
    fn test_investment_snapshot_stale() {
        let env = Env::default();
        env.mock_all_auths();
        let invoice_id = BytesN::from_array(&env, &[0; 32]);

        let stored_investment = create_test_investment(&env, &invoice_id, 1000);
        InvestmentStorage::store_investment(&env, &stored_investment);

        let mut snapshot_investment = stored_investment.clone();
        snapshot_investment.amount = 2000;

        let result = require_matching_investment_snapshot(&env, &invoice_id, &snapshot_investment);
        assert_eq!(result, Err(QuickLendXError::StaleInvestmentSnapshot));
    }

    #[test]
    fn test_investment_snapshot_missing() {
        let env = Env::default();
        env.mock_all_auths();
        let invoice_id = BytesN::from_array(&env, &[0; 32]);
        let investment = create_test_investment(&env, &invoice_id, 1000);

        let result = require_matching_investment_snapshot(&env, &invoice_id, &investment);
        assert_eq!(result, Err(QuickLendXError::StorageKeyNotFound));
    }
}
