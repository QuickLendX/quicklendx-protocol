use crate::bid::Bid;
use crate::invoice::{Invoice, InvoiceMetadata};
use crate::payments::Escrow;
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

// Analytics Events

/// Emit event when platform metrics are updated
pub fn emit_platform_metrics_updated(
    env: &Env,
    total_invoices: u32,
    total_volume: i128,
    total_fees: i128,
    success_rate: i128,
) {
    env.events().publish(
        (symbol_short!("plt_met"),),
        (
            total_invoices,
            total_volume,
            total_fees,
            success_rate,
            env.ledger().timestamp(),
        ),
    );
}

/// Emit event when performance metrics are updated
pub fn emit_performance_metrics_updated(
    env: &Env,
    average_settlement_time: u64,
    transaction_success_rate: i128,
    user_satisfaction_score: u32,
) {
    env.events().publish(
        (symbol_short!("perf_met"),),
        (
            average_settlement_time,
            transaction_success_rate,
            user_satisfaction_score,
            env.ledger().timestamp(),
        ),
    );
}

/// Emit event when user behavior metrics are calculated
pub fn emit_user_behavior_analyzed(
    env: &Env,
    user: &Address,
    total_investments: u32,
    success_rate: i128,
    risk_score: u32,
) {
    env.events().publish(
        (symbol_short!("usr_beh"),),
        (
            user.clone(),
            total_investments,
            success_rate,
            risk_score,
            env.ledger().timestamp(),
        ),
    );
}

/// Emit event when financial metrics are calculated
pub fn emit_financial_metrics_calculated(
    env: &Env,
    period: &crate::analytics::TimePeriod,
    total_volume: i128,
    total_fees: i128,
    average_return_rate: i128,
) {
    env.events().publish(
        (symbol_short!("fin_met"),),
        (
            period.clone(),
            total_volume,
            total_fees,
            average_return_rate,
            env.ledger().timestamp(),
        ),
    );
}

/// Emit event when business report is generated
pub fn emit_business_report_generated(
    env: &Env,
    report_id: &BytesN<32>,
    business: &Address,
    period: &crate::analytics::TimePeriod,
    invoices_uploaded: u32,
    success_rate: i128,
) {
    env.events().publish(
        (symbol_short!("biz_rpt"),),
        (
            report_id.clone(),
            business.clone(),
            period.clone(),
            invoices_uploaded,
            success_rate,
            env.ledger().timestamp(),
        ),
    );
}

/// Emit event when investor report is generated
pub fn emit_investor_report_generated(
    env: &Env,
    report_id: &BytesN<32>,
    investor: &Address,
    period: &crate::analytics::TimePeriod,
    investments_made: u32,
    average_return_rate: i128,
) {
    env.events().publish(
        (symbol_short!("inv_rpt"),),
        (
            report_id.clone(),
            investor.clone(),
            period.clone(),
            investments_made,
            average_return_rate,
            env.ledger().timestamp(),
        ),
    );
}


/// Emit event when analytics query is performed
pub fn emit_analytics_query(
    env: &Env,
    query_type: &String,
    filters_applied: u32,
    result_count: u32,
) {
    env.events().publish(
        (symbol_short!("anal_qry"),),
        (
            query_type.clone(),
            filters_applied,
            result_count,
            env.ledger().timestamp(),
        ),
    );
}

/// Emit event when analytics export is requested
pub fn emit_analytics_export(
    env: &Env,
    export_type: &String,
    requested_by: &Address,
    record_count: u32,
) {
    env.events().publish(
        (symbol_short!("anal_exp"),),
        (
            export_type.clone(),
            requested_by.clone(),
            record_count,
            env.ledger().timestamp(),
        ),
    );
}
