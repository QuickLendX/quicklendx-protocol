use crate::audit::AuditLogEntry;
use crate::bid::Bid;
use crate::invoice::{Invoice, InvoiceMetadata};
use crate::payments::{Escrow, EscrowStatus};
use crate::profits::PlatformFeeConfig;
use crate::verification::InvestorVerification;
use soroban_sdk::{symbol_short, Address, BytesN, Env, String};

pub fn emit_invoice_uploaded(env: &Env, invoice: &Invoice) {
    env.events().publish(
        (symbol_short!("inv_up"),),
        (
            invoice.id.clone(),
            invoice.business.clone(),
            invoice.amount,
            invoice.currency.clone(),
            invoice.due_date,
        ),
    );
}

pub fn emit_invoice_verified(env: &Env, invoice: &Invoice) {
    env.events().publish(
        (symbol_short!("inv_ver"),),
        (invoice.id.clone(), invoice.business.clone()),
    );
}

pub fn emit_invoice_metadata_updated(env: &Env, invoice: &Invoice, metadata: &InvoiceMetadata) {
    let mut total = 0i128;
    for record in metadata.line_items.iter() {
        total = total.saturating_add(record.3);
    }

    env.events().publish(
        (symbol_short!("inv_meta"),),
        (
            invoice.id.clone(),
            metadata.customer_name.clone(),
            metadata.tax_id.clone(),
            metadata.line_items.len() as u32,
            total,
        ),
    );
}

pub fn emit_invoice_metadata_cleared(env: &Env, invoice: &Invoice) {
    env.events().publish(
        (symbol_short!("inv_mclr"),),
        (invoice.id.clone(), invoice.business.clone()),
    );
}

pub fn emit_investor_verified(env: &Env, verification: &InvestorVerification) {
    env.events().publish(
        (symbol_short!("inv_veri"),),
        (
            verification.investor.clone(),
            verification.investment_limit,
            verification.verified_at,
        ),
    );
}

pub fn emit_invoice_settled(
    env: &Env,
    invoice: &crate::invoice::Invoice,
    investor_return: i128,
    platform_fee: i128,
) {
    env.events().publish(
        (symbol_short!("inv_set"),),
        (
            invoice.id.clone(),
            invoice.business.clone(),
            investor_return,
            platform_fee,
        ),
    );
}

pub fn emit_partial_payment(
    env: &Env,
    invoice: &Invoice,
    payment_amount: i128,
    total_paid: i128,
    progress: u32,
    transaction_id: String,
) {
    env.events().publish(
        (symbol_short!("inv_pp"),),
        (
            invoice.id.clone(),
            invoice.business.clone(),
            payment_amount,
            total_paid,
            progress,
            transaction_id,
        ),
    );
}

pub fn emit_invoice_expired(env: &Env, invoice: &crate::invoice::Invoice) {
    env.events().publish(
        (symbol_short!("inv_exp"),),
        (
            invoice.id.clone(),
            invoice.business.clone(),
            invoice.due_date,
        ),
    );
}

pub fn emit_invoice_defaulted(env: &Env, invoice: &crate::invoice::Invoice) {
    env.events().publish(
        (symbol_short!("inv_def"),),
        (invoice.id.clone(), invoice.business.clone()),
    );
}

pub fn emit_insurance_added(
    env: &Env,
    investment_id: &BytesN<32>,
    invoice_id: &BytesN<32>,
    investor: &Address,
    provider: &Address,
    coverage_percentage: u32,
    coverage_amount: i128,
    premium_amount: i128,
) {
    env.events().publish(
        (symbol_short!("ins_add"),),
        (
            investment_id.clone(),
            invoice_id.clone(),
            investor.clone(),
            provider.clone(),
            coverage_percentage,
            coverage_amount,
            premium_amount,
        ),
    );
}

pub fn emit_insurance_premium_collected(
    env: &Env,
    investment_id: &BytesN<32>,
    provider: &Address,
    premium_amount: i128,
) {
    env.events().publish(
        (symbol_short!("ins_prm"),),
        (investment_id.clone(), provider.clone(), premium_amount),
    );
}

pub fn emit_insurance_claimed(
    env: &Env,
    investment_id: &BytesN<32>,
    invoice_id: &BytesN<32>,
    provider: &Address,
    coverage_amount: i128,
) {
    env.events().publish(
        (symbol_short!("ins_clm"),),
        (
            investment_id.clone(),
            invoice_id.clone(),
            provider.clone(),
            coverage_amount,
        ),
    );
}

pub fn emit_platform_fee_updated(env: &Env, config: &PlatformFeeConfig) {
    env.events().publish(
        (symbol_short!("fee_upd"),),
        (config.fee_bps, config.updated_at, config.updated_by.clone()),
    );
}

/// Emit event when escrow is created
pub fn emit_escrow_created(env: &Env, escrow: &Escrow) {
    env.events().publish(
        (symbol_short!("esc_cr"),),
        (
            escrow.escrow_id.clone(),
            escrow.invoice_id.clone(),
            escrow.investor.clone(),
            escrow.business.clone(),
            escrow.amount,
        ),
    );
}

/// Emit event when escrow funds are released to business
pub fn emit_escrow_released(
    env: &Env,
    escrow_id: &BytesN<32>,
    invoice_id: &BytesN<32>,
    business: &Address,
    amount: i128,
) {
    env.events().publish(
        (symbol_short!("esc_rel"),),
        (
            escrow_id.clone(),
            invoice_id.clone(),
            business.clone(),
            amount,
        ),
    );
}

/// Emit event when escrow funds are refunded to investor
pub fn emit_escrow_refunded(
    env: &Env,
    escrow_id: &BytesN<32>,
    invoice_id: &BytesN<32>,
    investor: &Address,
    amount: i128,
) {
    env.events().publish(
        (symbol_short!("esc_ref"),),
        (
            escrow_id.clone(),
            invoice_id.clone(),
            investor.clone(),
            amount,
        ),
    );
}

pub fn emit_bid_expired(env: &Env, bid: &Bid) {
    env.events().publish(
        (symbol_short!("bid_exp"),),
        (
            bid.bid_id.clone(),
            bid.invoice_id.clone(),
            bid.investor.clone(),
            bid.bid_amount,
            bid.expiration_timestamp,
        ),
    );
}

/// Emit event when escrow status changes
pub fn emit_escrow_status_changed(
    env: &Env,
    escrow_id: &BytesN<32>,
    old_status: EscrowStatus,
    new_status: EscrowStatus,
) {
    env.events().publish(
        (symbol_short!("esc_st"),),
        (escrow_id.clone(), old_status, new_status),
    );
}

/// Emit event when backup is created
pub fn emit_backup_created(env: &Env, backup_id: &BytesN<32>, invoice_count: u32) {
    env.events().publish(
        (symbol_short!("bkup_crt"),),
        (backup_id.clone(), invoice_count, env.ledger().timestamp()),
    );
}

/// Emit event when backup is restored
pub fn emit_backup_restored(env: &Env, backup_id: &BytesN<32>, invoice_count: u32) {
    env.events().publish(
        (symbol_short!("bkup_rstr"),),
        (backup_id.clone(), invoice_count, env.ledger().timestamp()),
    );
}

/// Emit event when backup is validated
pub fn emit_backup_validated(env: &Env, backup_id: &BytesN<32>, success: bool) {
    env.events().publish(
        (symbol_short!("bkup_vd"),),
        (backup_id.clone(), success, env.ledger().timestamp()),
    );
}

/// Emit event when backup is archived
pub fn emit_backup_archived(env: &Env, backup_id: &BytesN<32>) {
    env.events().publish(
        (symbol_short!("bkup_ar"),),
        (backup_id.clone(), env.ledger().timestamp()),
    );
}

/// Emit audit log event
pub fn emit_audit_log_created(env: &Env, entry: &AuditLogEntry) {
    env.events().publish(
        (symbol_short!("aud_log"),),
        (
            entry.audit_id.clone(),
            entry.invoice_id.clone(),
            entry.operation.clone(),
            entry.actor.clone(),
            entry.timestamp,
        ),
    );
}

/// Emit audit validation event
pub fn emit_audit_validation(env: &Env, invoice_id: &BytesN<32>, is_valid: bool) {
    env.events().publish(
        (symbol_short!("aud_val"),),
        (invoice_id.clone(), is_valid, env.ledger().timestamp()),
    );
}

/// Emit audit query event
pub fn emit_audit_query(env: &Env, query_type: String, result_count: u32) {
    env.events()
        .publish((symbol_short!("aud_qry"),), (query_type, result_count));
}

/// Emit event when invoice category is updated
pub fn emit_invoice_category_updated(
    env: &Env,
    invoice_id: &BytesN<32>,
    business: &Address,
    old_category: &crate::invoice::InvoiceCategory,
    new_category: &crate::invoice::InvoiceCategory,
) {
    env.events().publish(
        (symbol_short!("cat_upd"),),
        (
            invoice_id.clone(),
            business.clone(),
            old_category.clone(),
            new_category.clone(),
        ),
    );
}

/// Emit event when a tag is added to an invoice
pub fn emit_invoice_tag_added(
    env: &Env,
    invoice_id: &BytesN<32>,
    business: &Address,
    tag: &String,
) {
    env.events().publish(
        (symbol_short!("tag_add"),),
        (invoice_id.clone(), business.clone(), tag.clone()),
    );
}

/// Emit event when a tag is removed from an invoice
pub fn emit_invoice_tag_removed(
    env: &Env,
    invoice_id: &BytesN<32>,
    business: &Address,
    tag: &String,
) {
    env.events().publish(
        (symbol_short!("tag_rm"),),
        (invoice_id.clone(), business.clone(), tag.clone()),
    );
}

/// Emit event when a dispute is created
pub fn emit_dispute_created(
    env: &Env,
    invoice_id: &BytesN<32>,
    created_by: &Address,
    reason: &String,
) {
    env.events().publish(
        (symbol_short!("dsp_cr"),),
        (
            invoice_id.clone(),
            created_by.clone(),
            reason.clone(),
            env.ledger().timestamp(),
        ),
    );
}

/// Emit event when a dispute is put under review
pub fn emit_dispute_under_review(env: &Env, invoice_id: &BytesN<32>, reviewed_by: &Address) {
    env.events().publish(
        (symbol_short!("dsp_ur"),),
        (
            invoice_id.clone(),
            reviewed_by.clone(),
            env.ledger().timestamp(),
        ),
    );
}

/// Emit event when a dispute is resolved
pub fn emit_dispute_resolved(
    env: &Env,
    invoice_id: &BytesN<32>,
    resolved_by: &Address,
    resolution: &String,
) {
    env.events().publish(
        (symbol_short!("dsp_rs"),),
        (
            invoice_id.clone(),
            resolved_by.clone(),
            resolution.clone(),
            env.ledger().timestamp(),
        ),
    );
}

/// Emit event when a payment is detected
pub fn emit_payment_detected(
    env: &Env,
    invoice_id: &BytesN<32>,
    payment_amount: i128,
    transaction_id: &String,
    source: &String,
) {
    env.events().publish(
        (symbol_short!("pay_det"),),
        (
            invoice_id.clone(),
            payment_amount,
            transaction_id.clone(),
            source.clone(),
            env.ledger().timestamp(),
        ),
    );
}

/// Emit event when automated settlement is triggered
pub fn emit_automated_settlement_triggered(
    env: &Env,
    invoice_id: &BytesN<32>,
    payment_amount: i128,
    settlement_id: &BytesN<32>,
) {
    env.events().publish(
        (symbol_short!("auto_set"),),
        (
            invoice_id.clone(),
            payment_amount,
            settlement_id.clone(),
            env.ledger().timestamp(),
        ),
    );
}

/// Emit event when settlement queue item is processed
pub fn emit_settlement_queued(
    env: &Env,
    invoice_id: &BytesN<32>,
    queue_id: &BytesN<32>,
    priority: u32,
) {
    env.events().publish(
        (symbol_short!("set_queue"),),
        (
            invoice_id.clone(),
            queue_id.clone(),
            priority,
            env.ledger().timestamp(),
        ),
    );
}

/// Emit event when settlement retry is attempted
pub fn emit_settlement_retry(
    env: &Env,
    invoice_id: &BytesN<32>,
    settlement_id: &BytesN<32>,
    retry_count: u32,
    reason: &String,
) {
    env.events().publish(
        (symbol_short!("set_retry"),),
        (
            invoice_id.clone(),
            settlement_id.clone(),
            retry_count,
            reason.clone(),
            env.ledger().timestamp(),
        ),
    );
}

/// Emit event when payment validation fails
pub fn emit_payment_validation_failed(
    env: &Env,
    invoice_id: &BytesN<32>,
    payment_amount: i128,
    reason: &String,
) {
    env.events().publish(
        (symbol_short!("pay_val_f"),),
        (
            invoice_id.clone(),
            payment_amount,
            reason.clone(),
            env.ledger().timestamp(),
        ),
    );
}