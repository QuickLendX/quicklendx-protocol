use crate::invoice::Invoice;
use crate::payments::{Escrow, EscrowStatus};
use crate::audit::{AuditLogEntry, AuditOperation};
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

pub fn emit_invoice_defaulted(env: &Env, invoice: &crate::invoice::Invoice) {
    env.events().publish(
        (symbol_short!("inv_def"),),
        (invoice.id.clone(), invoice.business.clone()),
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
    env.events().publish(
        (symbol_short!("aud_qry"),),
        (query_type, result_count),
    );
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
