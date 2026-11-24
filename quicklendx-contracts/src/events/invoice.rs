#![allow(deprecated)]

use crate::observability::{TransitionPhase, OBSERVABILITY_SCHEMA_VERSION};
use crate::types::{Invoice, InvoiceMetadata};
use soroban_sdk::{contractevent, Address, BytesN, Env, String};

/// Semantic alias: `InvoiceCreated` == `InvoiceUploaded`.
/// Both refer to the same event schema; use `TOPIC_INVOICE_UPLOADED` as the topic.
pub type InvoiceCreated = InvoiceUploaded;

/// Semantic alias: `LoanSettled` == `InvoiceSettled`.
/// Emitted when a loan (invoice) is fully settled.
/// Use `TOPIC_INVOICE_SETTLED` as the topic.
pub type LoanSettled = InvoiceSettled;

/// Emitted when a new invoice is uploaded / created by a business.
///
/// Topic: [`TOPIC_INVOICE_UPLOADED`] (`"inv_up"`)
///
/// # Fields
/// - `invoice_id` – Unique 32-byte invoice identifier.
/// - `business` – Address of the business that owns the invoice.
/// - `amount` – Invoice face value in the smallest currency unit.
/// - `currency` – Token contract address for the invoice currency.
/// - `due_date` – Unix timestamp when the invoice is due.
/// - `timestamp` – Ledger timestamp at emission time.
#[contractevent]
pub struct InvoiceUploaded {
    pub invoice_id: BytesN<32>,
    pub business: Address,
    pub amount: i128,
    pub currency: Address,
    pub due_date: u64,
    pub timestamp: u64,
}

/// Emitted when an invoice is verified by an admin.
///
/// Topic: [`TOPIC_INVOICE_VERIFIED`] (`"inv_ver"`)
#[derive(Debug, PartialEq)]
#[contractevent]
pub struct InvoiceVerified {
    pub invoice_id: BytesN<32>,
    pub business: Address,
    pub timestamp: u64,
}

/// Emitted when an invoice is cancelled by the business owner.
///
/// Topic: [`TOPIC_INVOICE_CANCELLED`] (`"inv_canc"`)
#[derive(Debug, PartialEq)]
#[contractevent]
pub struct InvoiceCancelled {
    pub invoice_id: BytesN<32>,
    pub business: Address,
    pub timestamp: u64,
}

/// Emitted when an invoice is fully settled (loan repaid).
///
/// Topic: [`TOPIC_INVOICE_SETTLED`] (`"inv_set"`)
///
/// # Fields
/// - `invoice_id` – Unique 32-byte invoice identifier.
/// - `amount` – Total amount settled (sum of all payments applied to this invoice).
/// - `ledger` – Ledger sequence number at the time of settlement.
/// - `business` – Address of the business that owns the invoice.
/// - `investor` – Address of the investor who funded the invoice.
/// - `investor_return` – Amount returned to the investor after fees.
/// - `platform_fee` – Fee taken by the platform.
/// - `timestamp` – Ledger timestamp at emission time.
///
/// # Security
/// No PII is included. `investor_return` and `platform_fee` are derived
/// from validated contract state only.
#[derive(Debug, PartialEq)]
#[contractevent]
pub struct InvoiceSettled {
    pub invoice_id: BytesN<32>,
    pub amount: i128,
    pub ledger: u32,
    pub business: Address,
    pub investor: Address,
    pub investor_return: i128,
    pub platform_fee: i128,
    pub timestamp: u64,
}

/// Emitted when an invoice is marked as defaulted.
///
/// Topic: [`TOPIC_INVOICE_DEFAULTED`] (`"inv_def"`)
#[derive(Debug, PartialEq)]
#[contractevent]
pub struct InvoiceDefaulted {
    pub invoice_id: BytesN<32>,
    pub business: Address,
    pub investor: Address,
    pub timestamp: u64,
}

/// Emitted when an invoice expires past its due date without payment.
///
/// Topic: [`TOPIC_INVOICE_EXPIRED`] (`"inv_exp"`)
#[derive(Debug, PartialEq)]
#[contractevent]
pub struct InvoiceExpired {
    pub invoice_id: BytesN<32>,
    pub business: Address,
    pub due_date: u64,
}

/// Emitted on each partial payment towards an invoice.
///
/// Topic: [`TOPIC_PARTIAL_PAYMENT`] (`"inv_pp"`)
#[derive(Debug, PartialEq)]
#[contractevent]
pub struct PartialPayment {
    pub invoice_id: BytesN<32>,
    pub business: Address,
    pub payment_amount: i128,
    pub total_paid: i128,
    pub progress: u32,
    pub transaction_id: String,
}

/// Emitted when a payment record is durably stored.
///
/// Topic: [`TOPIC_PAYMENT_RECORDED`] (`"pay_rec"`)
#[derive(Debug, PartialEq)]
#[contractevent]
pub struct PaymentRecorded {
    pub invoice_id: BytesN<32>,
    pub payer: Address,
    pub amount: i128,
    pub transaction_id: String,
    pub timestamp: u64,
}

/// Additive versioned repayment allocation for a committed payment.
///
/// Topic: [`TOPIC_REPAYMENT_ALLOCATED`] (`"repayment_allocated"`)
///
/// Emitted only after storage commit. `operation_id` matches the
/// `PaymentProcessed` audit entry. Legacy `PartialPayment` / `pay_rec`
/// payloads are unchanged.
#[derive(Debug, PartialEq)]
#[contractevent]
pub struct RepaymentAllocated {
    pub schema_version: u32,
    pub operation_id: BytesN<32>,
    pub phase: TransitionPhase,
    pub invoice_id: BytesN<32>,
    pub payer: Address,
    pub applied_principal: i128,
    pub applied_investor_profit: i128,
    pub applied_platform_fee: i128,
    pub applied_late_penalty: i128,
    pub cumulative_principal: i128,
    pub cumulative_investor_profit: i128,
    pub cumulative_platform_fee: i128,
    pub cumulative_late_penalty: i128,
    pub total_paid: i128,
    pub total_due: i128,
    pub fee_bps: i128,
}

/// Emitted when an invoice reaches final settlement (all funds disbursed).
///
/// Topic: [`TOPIC_INVOICE_SETTLED_FINAL`] (`"inv_stlf"`)
#[derive(Debug, PartialEq)]
#[contractevent]
pub struct InvoiceSettledFinal {
    pub invoice_id: BytesN<32>,
    pub business: Address,
    pub investor: Address,
    pub total_paid: i128,
    pub timestamp: u64,
}

/// Emitted when invoice metadata is updated.
///
/// Topic: `"invoice_metadata_updated"`
///
/// # Security
/// **NO PII**: This event does NOT include customer_name or tax_id to prevent
/// PII leakage. Only aggregate statistics (line_item_count, total_value) are included.
#[contractevent]
pub struct InvoiceMetadataUpdated {
    pub invoice_id: BytesN<32>,
    pub line_item_count: u32,
    pub total_value: i128,
    pub timestamp: u64,
}

#[contractevent]
pub struct InvoiceMetadataCleared {
    pub invoice_id: BytesN<32>,
    pub business: Address,
}

#[derive(Debug, PartialEq)]
#[contractevent]
pub struct InvoiceFunded {
    pub invoice_id: BytesN<32>,
    pub investor: Address,
    pub amount: i128,
    pub timestamp: u64,
}

#[contractevent]
pub struct InvoiceCategoryUpdated {
    pub invoice_id: BytesN<32>,
    pub business: Address,
    pub old_category: crate::types::InvoiceCategory,
    pub new_category: crate::types::InvoiceCategory,
}

#[contractevent]
pub struct InvoiceTagAdded {
    pub invoice_id: BytesN<32>,
    pub business: Address,
    pub tag: String,
}

#[contractevent]
pub struct InvoiceTagRemoved {
    pub invoice_id: BytesN<32>,
    pub business: Address,
    pub tag: String,
}

#[contractevent]
pub struct InvoiceStatusUpdated {
    pub invoice_id: BytesN<32>,
    pub status: crate::types::InvoiceStatus,
}

/// Emitted when an admin applies a freeze to an invoice via `freeze_invoice`.
///
/// Topic: [`TOPIC_INVOICE_FROZEN`] (`"invoice_frozen"`)
///
/// # Fields
/// - `invoice_id` – The frozen invoice.
/// - `frozen_by` – Address of the admin who applied the freeze.
/// - `reason` – Machine-readable label for the [`crate::types::BusinessFreezeReason`]
///   variant (e.g. `"admin_action"`, `"compliance_violation"`, `"fraud_suspected"`).
/// - `freeze_appeal_channel` – A short pointer to the off-chain appeals process.
///   Set to `"docs/APPEALS.md"` by the emitter.  Downstream consumers (dashboards,
///   notification pipelines) can surface this string directly to the affected
///   business so they know where to file an appeal without paging an engineer.
///   This field contains **no PII** — it is a static URL/path.
/// - `timestamp` – Ledger timestamp at emission time.
///
/// # Backwards compatibility
/// This event is **additive**.  Indexers that do not recognise the
/// `freeze_appeal_channel` field can safely ignore it.
///
/// # Security
/// No PII is included.  The `reason` field is the string label returned by
/// `BusinessFreezeReason::label()`, not a free-text admin comment.
#[derive(Debug, PartialEq)]
#[contractevent]
pub struct InvoiceFrozen {
    pub invoice_id: BytesN<32>,
    pub frozen_by: Address,
    pub reason: String,
    pub freeze_appeal_channel: String,
    pub timestamp: u64,
}

/// Emit an [`InvoiceFrozen`] event.
///
/// `reason_label` should come from [`crate::types::BusinessFreezeReason::label()`].
/// `freeze_appeal_channel` is always `"docs/APPEALS.md"` — a static pointer
/// to the operator-facing appeals runbook so that any off-chain consumer of
/// this event knows immediately where to direct the frozen business.
pub fn emit_invoice_frozen(
    env: &Env,
    invoice_id: &BytesN<32>,
    frozen_by: &Address,
    reason_label: &str,
) {
    InvoiceFrozen {
        invoice_id: invoice_id.clone(),
        frozen_by: frozen_by.clone(),
        reason: String::from_str(env, reason_label),
        freeze_appeal_channel: String::from_str(env, "docs/APPEALS.md"),
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}

pub fn emit_invoice_uploaded(env: &Env, invoice: &Invoice) {
    InvoiceUploaded {
        invoice_id: invoice.id.clone(),
        business: invoice.business.clone(),
        amount: invoice.amount,
        currency: invoice.currency.clone(),
        due_date: invoice.due_date,
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}

pub fn emit_invoice_verified(env: &Env, invoice: &Invoice) {
    InvoiceVerified {
        invoice_id: invoice.id.clone(),
        business: invoice.business.clone(),
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}

pub fn emit_invoice_cancelled(env: &Env, invoice: &Invoice) {
    InvoiceCancelled {
        invoice_id: invoice.id.clone(),
        business: invoice.business.clone(),
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}

pub fn emit_invoice_metadata_updated(env: &Env, invoice: &Invoice, metadata: &InvoiceMetadata) {
    let mut total = 0i128;
    for record in metadata.line_items.iter() {
        total = total.saturating_add(record.3);
    }

    InvoiceMetadataUpdated {
        invoice_id: invoice.id.clone(),
        line_item_count: metadata.line_items.len(),
        total_value: total,
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}

pub fn emit_invoice_metadata_cleared(env: &Env, invoice: &Invoice) {
    InvoiceMetadataCleared {
        invoice_id: invoice.id.clone(),
        business: invoice.business.clone(),
    }
    .publish(env);
}

pub fn emit_invoice_settled(
    env: &Env,
    invoice: &crate::types::Invoice,
    investor_return: i128,
    platform_fee: i128,
) {
    InvoiceSettled {
        invoice_id: invoice.id.clone(),
        amount: invoice.total_paid,
        ledger: env.ledger().sequence(),
        business: invoice.business.clone(),
        investor: invoice.investor.clone().unwrap_or(Address::from_str(
            env,
            "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
        )),
        investor_return,
        platform_fee,
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}

pub fn emit_partial_payment(
    env: &Env,
    invoice: &Invoice,
    payment_amount: i128,
    total_paid: i128,
    progress: u32,
    transaction_id: String,
) {
    PartialPayment {
        invoice_id: invoice.id.clone(),
        business: invoice.business.clone(),
        payment_amount,
        total_paid,
        progress,
        transaction_id,
    }
    .publish(env);
}

pub fn emit_payment_recorded(
    env: &Env,
    invoice_id: &BytesN<32>,
    payer: &Address,
    amount: i128,
    transaction_id: String,
) {
    PaymentRecorded {
        invoice_id: invoice_id.clone(),
        payer: payer.clone(),
        amount,
        transaction_id,
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}

/// Emit a committed [`RepaymentAllocated`] record. Call only after payment
/// storage has been written.
pub fn emit_repayment_allocated(
    env: &Env,
    operation_id: BytesN<32>,
    invoice_id: &BytesN<32>,
    payer: &Address,
    applied_principal: i128,
    applied_investor_profit: i128,
    applied_platform_fee: i128,
    applied_late_penalty: i128,
    cumulative_principal: i128,
    cumulative_investor_profit: i128,
    cumulative_platform_fee: i128,
    cumulative_late_penalty: i128,
    total_paid: i128,
    total_due: i128,
    fee_bps: i128,
) {
    RepaymentAllocated {
        schema_version: OBSERVABILITY_SCHEMA_VERSION,
        operation_id,
        phase: TransitionPhase::Committed,
        invoice_id: invoice_id.clone(),
        payer: payer.clone(),
        applied_principal,
        applied_investor_profit,
        applied_platform_fee,
        applied_late_penalty,
        cumulative_principal,
        cumulative_investor_profit,
        cumulative_platform_fee,
        cumulative_late_penalty,
        total_paid,
        total_due,
        fee_bps,
    }
    .publish(env);
}

pub fn emit_invoice_settled_final(
    env: &Env,
    invoice_id: &BytesN<32>,
    business: &Address,
    investor: &Address,
    total_paid: i128,
) {
    InvoiceSettledFinal {
        invoice_id: invoice_id.clone(),
        business: business.clone(),
        investor: investor.clone(),
        total_paid,
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}

pub fn emit_invoice_expired(env: &Env, invoice: &crate::types::Invoice) {
    InvoiceExpired {
        invoice_id: invoice.id.clone(),
        business: invoice.business.clone(),
        due_date: invoice.due_date,
    }
    .publish(env);
}

pub fn emit_invoice_defaulted(env: &Env, invoice: &crate::types::Invoice) {
    InvoiceDefaulted {
        invoice_id: invoice.id.clone(),
        business: invoice.business.clone(),
        investor: invoice.investor.clone().unwrap_or(Address::from_str(
            env,
            "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
        )),
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}

pub fn emit_invoice_funded(env: &Env, invoice_id: &BytesN<32>, investor: &Address, amount: i128) {
    InvoiceFunded {
        invoice_id: invoice_id.clone(),
        investor: investor.clone(),
        amount,
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}

pub fn emit_invoice_category_updated(
    env: &Env,
    invoice_id: &BytesN<32>,
    business: &Address,
    old_category: &crate::types::InvoiceCategory,
    new_category: &crate::types::InvoiceCategory,
) {
    InvoiceCategoryUpdated {
        invoice_id: invoice_id.clone(),
        business: business.clone(),
        old_category: *old_category,
        new_category: *new_category,
    }
    .publish(env);
}

pub fn emit_invoice_tag_added(
    env: &Env,
    invoice_id: &BytesN<32>,
    business: &Address,
    tag: &String,
) {
    InvoiceTagAdded {
        invoice_id: invoice_id.clone(),
        business: business.clone(),
        tag: tag.clone(),
    }
    .publish(env);
}

pub fn emit_invoice_tag_removed(
    env: &Env,
    invoice_id: &BytesN<32>,
    business: &Address,
    tag: &String,
) {
    InvoiceTagRemoved {
        invoice_id: invoice_id.clone(),
        business: business.clone(),
        tag: tag.clone(),
    }
    .publish(env);
}

pub fn emit_invoice_status_updated(
    env: &Env,
    invoice_id: BytesN<32>,
    status: crate::types::InvoiceStatus,
) {
    InvoiceStatusUpdated { invoice_id, status }.publish(env);
}
