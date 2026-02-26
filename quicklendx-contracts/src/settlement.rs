//! Invoice settlement with partial payments, capped overpayment handling,
//! and durable per-payment storage records.

use crate::audit::{log_payment_processed, log_settlement_completed};
use crate::errors::QuickLendXError;
use crate::events::{emit_invoice_settled, emit_partial_payment};
use crate::investment::{InvestmentStatus, InvestmentStorage};
use crate::invoice::{
    Invoice, InvoiceStatus, InvoiceStorage, PaymentRecord as InvoicePaymentRecord,
};
use crate::notifications::NotificationSystem;
use crate::{defaults::DEFAULT_GRACE_PERIOD, events::emit_invoice_settled_final};
use crate::events::TOPIC_INVOICE_SETTLED_FINAL;
use crate::payments::transfer_funds;
use soroban_sdk::{contracttype, symbol_short, Address, BytesN, Env, String, Vec};

const MAX_INLINE_PAYMENT_HISTORY: u32 = 32;

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
enum SettlementDataKey {
    PaymentCount(BytesN<32>),
    Payment(BytesN<32>, u32),
    PaymentNonce(BytesN<32>, Address, String),
}

/// Durable payment record stored per invoice/payment-index.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SettlementPaymentRecord {
    pub payer: Address,
    pub amount: i128,
    pub timestamp: u64,
    pub nonce: String,
}

/// Settlement progress for an invoice.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Progress {
    pub total_due: i128,
    pub total_paid: i128,
    pub remaining_due: i128,
    pub progress_percent: u32,
    pub payment_count: u32,
    pub status: InvoiceStatus,
}

/// Record a partial payment. If total reaches invoice total, settlement is finalized.
///
/// Business authorization is required and payer is recorded as the business address.
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
/// - Enforces nonce uniqueness per `(invoice, payer, nonce)` if nonce is non-empty
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

    if payment_nonce.len() > 0 {
        let nonce_key = SettlementDataKey::PaymentNonce(
            invoice_id.clone(),
            payer.clone(),
            payment_nonce.clone(),
        );
        let seen: bool = env.storage().persistent().get(&nonce_key).unwrap_or(false);
        if seen {
            return Err(QuickLendXError::OperationNotAllowed);
        }
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

    if new_total_paid > invoice.amount {
        return Err(QuickLendXError::InvalidAmount);
    }

    let payment_count = get_payment_count_internal(env, invoice_id);
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
            &SettlementDataKey::PaymentNonce(invoice_id.clone(), payer.clone(), payment_nonce),
            &true,
        );
    }

    invoice.total_paid = new_total_paid;
    update_inline_payment_history(
        &mut invoice,
        applied_amount,
        timestamp,
        payment_record.nonce,
    );
    InvoiceStorage::update_invoice(env, &invoice);

    log_payment_processed(
        env,
        invoice.id.clone(),
        payer.clone(),
        applied_amount,
        String::from_str(env, "recorded"),
    );

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

/// Settle invoice by applying a final payment amount from the business.
///
/// This function preserves existing behavior by requiring the resulting total
/// payment to satisfy full settlement conditions.
pub fn settle_invoice(
    env: &Env,
    invoice_id: &BytesN<32>,
    payment_amount: i128,
) -> Result<(), QuickLendXError> {
    if payment_amount <= 0 {
        return Err(QuickLendXError::InvalidAmount);
    }

    let invoice =
        InvoiceStorage::get_invoice(env, invoice_id).ok_or(QuickLendXError::InvoiceNotFound)?;
    ensure_payable_status(&invoice)?;
    let payer = invoice.business.clone();
    payer.require_auth();

    let remaining_due = compute_remaining_due(&invoice)?;
    let applied_preview = if payment_amount > remaining_due {
        remaining_due
    } else {
        payment_amount
    };

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

/// Returns the number of stored payment records for an invoice.
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

/// Returns payment records in insertion order for `[start, start+limit)`.
pub fn get_payment_records(
    env: &Env,
    invoice_id: &BytesN<32>,
    start: u32,
    limit: u32,
) -> Result<Vec<SettlementPaymentRecord>, QuickLendXError> {
    ensure_invoice_exists(env, invoice_id)?;

    let count = get_payment_count_internal(env, invoice_id);
    let mut records = Vec::new(env);

    if start >= count || limit == 0 {
        return Ok(records);
    }

    let max_limit = if limit > 100 { 100 } else { limit };
    let mut index = start;
    let mut collected = 0u32;

    while index < count && collected < max_limit {
        let record = get_payment_record(env, invoice_id, index)?;
        records.push_back(record);
        index = index.saturating_add(1);
        collected = collected.saturating_add(1);
    }

    Ok(records)
}

fn settle_invoice_internal(env: &Env, invoice_id: &BytesN<32>) -> Result<(), QuickLendXError> {
    let mut invoice =
        InvoiceStorage::get_invoice(env, invoice_id).ok_or(QuickLendXError::InvoiceNotFound)?;
    ensure_payable_status(&invoice)?;

    let investment = InvestmentStorage::get_investment_by_invoice(env, invoice_id)
        .ok_or(QuickLendXError::StorageKeyNotFound)?;

    if invoice.total_paid < invoice.amount || invoice.total_paid < investment.amount {
        return Err(QuickLendXError::PaymentTooLow);
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

    let previous_status = invoice.status.clone();
    let paid_at = env.ledger().timestamp();
    invoice.mark_as_paid(env, business_address.clone(), paid_at);
    InvoiceStorage::update_invoice(env, &invoice);

    if previous_status != invoice.status {
        InvoiceStorage::remove_from_status_invoices(env, &previous_status, invoice_id);
        InvoiceStorage::add_to_status_invoices(env, &invoice.status, invoice_id);
    }

    let mut updated_investment = investment;
    updated_investment.status = InvestmentStatus::Completed;
    InvestmentStorage::update_investment(env, &updated_investment);

    log_settlement_completed(
        env,
        invoice.id.clone(),
        business_address.clone(),
        invoice.total_paid,
    );

    emit_invoice_settled(env, &invoice, investor_return, platform_fee);
    emit_invoice_settled_final(env, invoice_id, invoice.total_paid, paid_at);

    let _ = NotificationSystem::notify_payment_received(env, &invoice, invoice.total_paid);

    Ok(())
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
    amount: i128,
    timestamp: u64,
    nonce: String,
) {
    if invoice.payment_history.len() >= MAX_INLINE_PAYMENT_HISTORY {
        invoice.payment_history.remove(0u32);
    }

    invoice.payment_history.push_back(InvoicePaymentRecord {
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
