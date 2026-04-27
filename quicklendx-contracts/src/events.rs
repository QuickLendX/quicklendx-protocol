#![allow(deprecated)]

use crate::bid::Bid;
use crate::fees::FeeType;
use crate::payments::Escrow;
use crate::profits::PlatformFeeConfig;
use crate::types::{Invoice, InvoiceMetadata};
use crate::verification::InvestorVerification;
use soroban_sdk::{contractevent, symbol_short, Address, BytesN, Env, String, Symbol};

// ============================================================================
// Structured Event Types
// ============================================================================

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvoiceUploaded {
    pub invoice_id: BytesN<32>,
    pub business: Address,
    pub amount: i128,
    pub currency: Address,
    pub due_date: u64,
    pub timestamp: u64,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvoiceVerified {
    pub invoice_id: BytesN<32>,
    pub business: Address,
    pub timestamp: u64,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvoiceCancelled {
    pub invoice_id: BytesN<32>,
    pub business: Address,
    pub timestamp: u64,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvoiceSettled {
    pub invoice_id: BytesN<32>,
    pub business: Address,
    pub investor: Address,
    pub investor_return: i128,
    pub platform_fee: i128,
    pub timestamp: u64,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvoiceDefaulted {
    pub invoice_id: BytesN<32>,
    pub business: Address,
    pub investor: Address,
    pub timestamp: u64,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvoiceExpired {
    pub invoice_id: BytesN<32>,
    pub business: Address,
    pub due_date: u64,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PartialPayment {
    pub invoice_id: BytesN<32>,
    pub business: Address,
    pub payment_amount: i128,
    pub total_paid: i128,
    pub progress: u32,
    pub transaction_id: String,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PaymentRecorded {
    pub invoice_id: BytesN<32>,
    pub payer: Address,
    pub amount: i128,
    pub transaction_id: String,
    pub timestamp: u64,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvoiceSettledFinal {
    pub invoice_id: BytesN<32>,
    pub business: Address,
    pub investor: Address,
    pub total_paid: i128,
    pub timestamp: u64,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BidPlaced {
    pub bid_id: BytesN<32>,
    pub invoice_id: BytesN<32>,
    pub investor: Address,
    pub bid_amount: i128,
    pub expected_return: i128,
    pub timestamp: u64,
    pub expiration_timestamp: u64,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BidAccepted {
    pub bid_id: BytesN<32>,
    pub invoice_id: BytesN<32>,
    pub investor: Address,
    pub business: Address,
    pub bid_amount: i128,
    pub expected_return: i128,
    pub timestamp: u64,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BidWithdrawn {
    pub bid_id: BytesN<32>,
    pub invoice_id: BytesN<32>,
    pub investor: Address,
    pub bid_amount: i128,
    pub timestamp: u64,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BidExpired {
    pub bid_id: BytesN<32>,
    pub invoice_id: BytesN<32>,
    pub investor: Address,
    pub bid_amount: i128,
    pub expiration_timestamp: u64,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EscrowCreated {
    pub escrow_id: BytesN<32>,
    pub invoice_id: BytesN<32>,
    pub investor: Address,
    pub business: Address,
    pub amount: i128,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EscrowReleased {
    pub escrow_id: BytesN<32>,
    pub invoice_id: BytesN<32>,
    pub business: Address,
    pub amount: i128,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EscrowRefunded {
    pub escrow_id: BytesN<32>,
    pub invoice_id: BytesN<32>,
    pub investor: Address,
    pub amount: i128,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvoiceMetadataUpdated {
    pub invoice_id: BytesN<32>,
    pub customer_name: String,
    pub tax_id: String,
    pub line_item_count: u32,
    pub total_value: i128,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvoiceMetadataCleared {
    pub invoice_id: BytesN<32>,
    pub business: Address,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvestorVerified {
    pub investor: Address,
    pub investment_limit: i128,
    pub verified_at: u64,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvoiceFunded {
    pub invoice_id: BytesN<32>,
    pub investor: Address,
    pub amount: i128,
    pub timestamp: u64,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InsuranceAdded {
    pub investment_id: BytesN<32>,
    pub invoice_id: BytesN<32>,
    pub investor: Address,
    pub provider: Address,
    pub coverage_percentage: u32,
    pub coverage_amount: i128,
    pub premium_amount: i128,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InsurancePremiumCollected {
    pub investment_id: BytesN<32>,
    pub provider: Address,
    pub premium_amount: i128,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InsuranceClaimed {
    pub investment_id: BytesN<32>,
    pub invoice_id: BytesN<32>,
    pub provider: Address,
    pub coverage_amount: i128,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlatformFeeUpdated {
    pub fee_bps: u32,
    pub updated_at: u64,
    pub updated_by: Address,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FeeStructureUpdated {
    pub fee_type: FeeType,
    pub old_fee_bps: u32,
    pub new_fee_bps: u32,
    pub updated_by: Address,
    pub timestamp: u64,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlatformFeeRouted {
    pub invoice_id: BytesN<32>,
    pub recipient: Address,
    pub fee_amount: i128,
    pub timestamp: u64,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlatformFeeConfigUpdated {
    pub old_fee_bps: u32,
    pub new_fee_bps: u32,
    pub updated_by: Address,
    pub timestamp: u64,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TreasuryConfigured {
    pub treasury_address: Address,
    pub configured_by: Address,
    pub timestamp: u64,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BackupCreated {
    pub backup_id: BytesN<32>,
    pub invoice_count: u32,
    pub timestamp: u64,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BackupRestored {
    pub backup_id: BytesN<32>,
    pub invoice_count: u32,
    pub timestamp: u64,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BackupValidated {
    pub backup_id: BytesN<32>,
    pub success: bool,
    pub timestamp: u64,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BackupArchived {
    pub backup_id: BytesN<32>,
    pub timestamp: u64,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetentionPolicyUpdated {
    pub max_backups: u32,
    pub max_age_seconds: u64,
    pub auto_cleanup_enabled: bool,
    pub timestamp: u64,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BackupsCleaned {
    pub removed_count: u32,
    pub timestamp: u64,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuditValidation {
    pub invoice_id: BytesN<32>,
    pub is_valid: bool,
    pub timestamp: u64,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuditQuery {
    pub query_type: String,
    pub result_count: u32,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvoiceCategoryUpdated {
    pub invoice_id: BytesN<32>,
    pub business: Address,
    pub old_category: crate::invoice::InvoiceCategory,
    pub new_category: crate::invoice::InvoiceCategory,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvoiceTagAdded {
    pub invoice_id: BytesN<32>,
    pub business: Address,
    pub tag: String,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvoiceTagRemoved {
    pub invoice_id: BytesN<32>,
    pub business: Address,
    pub tag: String,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DisputeCreated {
    pub invoice_id: BytesN<32>,
    pub created_by: Address,
    pub reason: String,
    pub timestamp: u64,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DisputeUnderReview {
    pub invoice_id: BytesN<32>,
    pub reviewed_by: Address,
    pub timestamp: u64,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DisputeResolved {
    pub invoice_id: BytesN<32>,
    pub resolved_by: Address,
    pub resolution: String,
    pub timestamp: u64,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProfitFeeBreakdown {
    pub invoice_id: BytesN<32>,
    pub investment_amount: i128,
    pub payment_amount: i128,
    pub gross_profit: i128,
    pub platform_fee: i128,
    pub investor_return: i128,
    pub fee_bps_applied: i128,
    pub timestamp: u64,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BidTtlUpdated {
    pub old_days: u64,
    pub new_days: u64,
    pub admin: Address,
    pub timestamp: u64,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EmergencyWithdrawalInitiated {
    pub token: Address,
    pub amount: i128,
    pub target: Address,
    pub unlock_at: u64,
    pub admin: Address,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EmergencyWithdrawalExecuted {
    pub token: Address,
    pub amount: i128,
    pub target: Address,
    pub admin: Address,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EmergencyWithdrawalCancelled {
    pub token: Address,
    pub amount: i128,
    pub target: Address,
    pub admin: Address,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdminSet {
    pub admin: Address,
    pub timestamp: u64,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdminTransferred {
    pub old_admin: Address,
    pub new_admin: Address,
    pub timestamp: u64,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RevenueDistributed {
    pub period: u64,
    pub treasury_amount: i128,
    pub developer_amount: i128,
    pub platform_amount: i128,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvoiceStatusUpdated {
    pub invoice_id: BytesN<32>,
    pub status: crate::invoice::InvoiceStatus,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdminInitialized {
    pub admin: Address,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProtocolInitialized {
    pub admin: Address,
    pub treasury: Address,
    pub fee_bps: u32,
    pub min_invoice_amount: i128,
    pub max_due_date_days: u64,
    pub grace_period_seconds: u64,
    pub timestamp: u64,
}

// ============================================================================
// Invoice Event Emitters
// ============================================================================

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
        customer_name: metadata.customer_name.clone(),
        tax_id: metadata.tax_id.clone(),
        line_item_count: metadata.line_items.len() as u32,
        total_value: total,
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

pub fn emit_investor_verified(env: &Env, verification: &InvestorVerification) {
    InvestorVerified {
        investor: verification.investor.clone(),
        investment_limit: verification.investment_limit,
        verified_at: verification.verified_at.unwrap_or(0),
    }
    .publish(env);
}

pub fn emit_invoice_settled(
    env: &Env,
    invoice: &crate::invoice::Invoice,
    investor_return: i128,
    platform_fee: i128,
) {
    InvoiceSettled {
        invoice_id: invoice.id.clone(),
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

pub fn emit_invoice_expired(env: &Env, invoice: &crate::invoice::Invoice) {
    InvoiceExpired {
        invoice_id: invoice.id.clone(),
        business: invoice.business.clone(),
        due_date: invoice.due_date,
    }
    .publish(env);
}

pub fn emit_invoice_defaulted(env: &Env, invoice: &crate::invoice::Invoice) {
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

// ============================================================================
// Insurance Event Emitters
// ============================================================================

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
    InsuranceAdded {
        investment_id: investment_id.clone(),
        invoice_id: invoice_id.clone(),
        investor: investor.clone(),
        provider: provider.clone(),
        coverage_percentage,
        coverage_amount,
        premium_amount,
    }
    .publish(env);
}

pub fn emit_insurance_premium_collected(
    env: &Env,
    investment_id: &BytesN<32>,
    provider: &Address,
    premium_amount: i128,
) {
    InsurancePremiumCollected {
        investment_id: investment_id.clone(),
        provider: provider.clone(),
        premium_amount,
    }
    .publish(env);
}

pub fn emit_insurance_claimed(
    env: &Env,
    investment_id: &BytesN<32>,
    invoice_id: &BytesN<32>,
    provider: &Address,
    coverage_amount: i128,
) {
    InsuranceClaimed {
        investment_id: investment_id.clone(),
        invoice_id: invoice_id.clone(),
        provider: provider.clone(),
        coverage_amount,
    }
    .publish(env);
}

// ============================================================================
// Platform Fee Event Emitters
// ============================================================================

pub fn emit_platform_fee_updated(env: &Env, config: &PlatformFeeConfig) {
    PlatformFeeUpdated {
        fee_bps: config.fee_bps,
        updated_at: config.updated_at,
        updated_by: config.updated_by.clone(),
    }
    .publish(env);
}

pub fn emit_fee_structure_updated(
    env: &Env,
    fee_type: &FeeType,
    old_fee_bps: u32,
    new_fee_bps: u32,
    updated_by: &Address,
) {
    FeeStructureUpdated {
        fee_type: fee_type.clone(),
        old_fee_bps,
        new_fee_bps,
        updated_by: updated_by.clone(),
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}

pub fn emit_platform_fee_routed(
    env: &Env,
    invoice_id: &BytesN<32>,
    recipient: &Address,
    fee_amount: i128,
) {
    PlatformFeeRouted {
        invoice_id: invoice_id.clone(),
        recipient: recipient.clone(),
        fee_amount,
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}

pub fn emit_platform_fee_config_updated(
    env: &Env,
    old_fee_bps: u32,
    new_fee_bps: u32,
    updated_by: &Address,
) {
    PlatformFeeConfigUpdated {
        old_fee_bps,
        new_fee_bps,
        updated_by: updated_by.clone(),
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}

pub fn emit_treasury_configured(env: &Env, treasury_address: &Address, configured_by: &Address) {
    TreasuryConfigured {
        treasury_address: treasury_address.clone(),
        configured_by: configured_by.clone(),
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}

// ============================================================================
// Escrow Event Emitters
// ============================================================================

pub fn emit_escrow_created(env: &Env, escrow: &Escrow) {
    EscrowCreated {
        escrow_id: escrow.escrow_id.clone(),
        invoice_id: escrow.invoice_id.clone(),
        investor: escrow.investor.clone(),
        business: escrow.business.clone(),
        amount: escrow.amount,
    }
    .publish(env);
}

pub fn emit_admin_initialized(env: &Env, admin: &Address) {
    AdminInitialized {
        admin: admin.clone(),
    }
    .publish(env);
}

pub fn emit_escrow_released(
    env: &Env,
    escrow_id: &BytesN<32>,
    invoice_id: &BytesN<32>,
    business: &Address,
    amount: i128,
) {
    EscrowReleased {
        escrow_id: escrow_id.clone(),
        invoice_id: invoice_id.clone(),
        business: business.clone(),
        amount,
    }
    .publish(env);
}

pub fn emit_escrow_refunded(
    env: &Env,
    escrow_id: &BytesN<32>,
    invoice_id: &BytesN<32>,
    investor: &Address,
    amount: i128,
) {
    EscrowRefunded {
        escrow_id: escrow_id.clone(),
        invoice_id: invoice_id.clone(),
        investor: investor.clone(),
        amount,
    }
    .publish(env);
}

// ============================================================================
// Bid Event Emitters
// ============================================================================

pub fn emit_bid_placed(env: &Env, bid: &Bid) {
    BidPlaced {
        bid_id: bid.bid_id.clone(),
        invoice_id: bid.invoice_id.clone(),
        investor: bid.investor.clone(),
        bid_amount: bid.bid_amount,
        expected_return: bid.expected_return,
        timestamp: bid.timestamp,
        expiration_timestamp: bid.expiration_timestamp,
    }
    .publish(env);
}

pub fn emit_bid_withdrawn(env: &Env, bid: &Bid) {
    BidWithdrawn {
        bid_id: bid.bid_id.clone(),
        invoice_id: bid.invoice_id.clone(),
        investor: bid.investor.clone(),
        bid_amount: bid.bid_amount,
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}

pub fn emit_bid_accepted(env: &Env, bid: &Bid, invoice_id: &BytesN<32>, business: &Address) {
    BidAccepted {
        bid_id: bid.bid_id.clone(),
        invoice_id: invoice_id.clone(),
        investor: bid.investor.clone(),
        business: business.clone(),
        bid_amount: bid.bid_amount,
        expected_return: bid.expected_return,
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}

pub fn emit_bid_expired(env: &Env, bid: &Bid) {
    BidExpired {
        bid_id: bid.bid_id.clone(),
        invoice_id: bid.invoice_id.clone(),
        investor: bid.investor.clone(),
        bid_amount: bid.bid_amount,
        expiration_timestamp: bid.expiration_timestamp,
    }
    .publish(env);
}

// ============================================================================
// Backup Event Emitters
// ============================================================================

pub fn emit_backup_created(env: &Env, backup_id: &BytesN<32>, invoice_count: u32) {
    BackupCreated {
        backup_id: backup_id.clone(),
        invoice_count,
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}

pub fn emit_backup_restored(env: &Env, backup_id: &BytesN<32>, invoice_count: u32) {
    BackupRestored {
        backup_id: backup_id.clone(),
        invoice_count,
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}

pub fn emit_backup_validated(env: &Env, backup_id: &BytesN<32>, success: bool) {
    BackupValidated {
        backup_id: backup_id.clone(),
        success,
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}

pub fn emit_backup_archived(env: &Env, backup_id: &BytesN<32>) {
    BackupArchived {
        backup_id: backup_id.clone(),
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}

pub fn emit_retention_policy_updated(
    env: &Env,
    max_backups: u32,
    max_age_seconds: u64,
    auto_cleanup_enabled: bool,
) {
    RetentionPolicyUpdated {
        max_backups,
        max_age_seconds,
        auto_cleanup_enabled,
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}

pub fn emit_backups_cleaned(env: &Env, removed_count: u32) {
    BackupsCleaned {
        removed_count,
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}

// ============================================================================
// Audit Event Emitters
// ============================================================================

pub fn emit_audit_validation(env: &Env, invoice_id: &BytesN<32>, is_valid: bool) {
    AuditValidation {
        invoice_id: invoice_id.clone(),
        is_valid,
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}

pub fn emit_audit_query(env: &Env, query_type: String, result_count: u32) {
    AuditQuery {
        query_type,
        result_count,
    }
    .publish(env);
}

// ============================================================================
// Invoice Category / Tag Event Emitters
// ============================================================================

pub fn emit_invoice_category_updated(
    env: &Env,
    invoice_id: &BytesN<32>,
    business: &Address,
    old_category: &crate::invoice::InvoiceCategory,
    new_category: &crate::invoice::InvoiceCategory,
) {
    InvoiceCategoryUpdated {
        invoice_id: invoice_id.clone(),
        business: business.clone(),
        old_category: old_category.clone(),
        new_category: new_category.clone(),
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

// ============================================================================
// Dispute Event Emitters
// ============================================================================

pub fn emit_dispute_created(
    env: &Env,
    invoice_id: &BytesN<32>,
    created_by: &Address,
    reason: &String,
) {
    DisputeCreated {
        invoice_id: invoice_id.clone(),
        created_by: created_by.clone(),
        reason: reason.clone(),
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}

pub fn emit_dispute_under_review(env: &Env, invoice_id: &BytesN<32>, reviewed_by: &Address) {
    DisputeUnderReview {
        invoice_id: invoice_id.clone(),
        reviewed_by: reviewed_by.clone(),
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}

pub fn emit_dispute_resolved(
    env: &Env,
    invoice_id: &BytesN<32>,
    resolved_by: &Address,
    resolution: &String,
) {
    DisputeResolved {
        invoice_id: invoice_id.clone(),
        resolved_by: resolved_by.clone(),
        resolution: resolution.clone(),
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}

// ============================================================================
// Profit / Fee Breakdown Event Emitter
// ============================================================================

#[allow(dead_code)]
pub fn emit_profit_fee_breakdown(
    env: &Env,
    invoice_id: &BytesN<32>,
    investment_amount: i128,
    payment_amount: i128,
    gross_profit: i128,
    platform_fee: i128,
    investor_return: i128,
    fee_bps_applied: i128,
) {
    ProfitFeeBreakdown {
        invoice_id: invoice_id.clone(),
        investment_amount,
        payment_amount,
        gross_profit,
        platform_fee,
        investor_return,
        fee_bps_applied,
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}

pub fn emit_bid_ttl_updated(env: &Env, old_days: u64, new_days: u64, admin: &Address) {
    BidTtlUpdated {
        old_days,
        new_days,
        admin: admin.clone(),
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}

pub fn emit_emergency_withdrawal_initiated(
    env: &Env,
    token: Address,
    amount: i128,
    target: Address,
    unlock_at: u64,
    admin: Address,
) {
    EmergencyWithdrawalInitiated {
        token,
        amount,
        target,
        unlock_at,
        admin,
    }
    .publish(env);
}

pub fn emit_emergency_withdrawal_executed(
    env: &Env,
    token: Address,
    amount: i128,
    target: Address,
    admin: Address,
) {
    EmergencyWithdrawalExecuted {
        token,
        amount,
        target,
        admin,
    }
    .publish(env);
}

pub fn emit_emergency_withdrawal_cancelled(
    env: &Env,
    token: Address,
    amount: i128,
    target: Address,
    admin: Address,
) {
    EmergencyWithdrawalCancelled {
        token,
        amount,
        target,
        admin,
    }
    .publish(env);
}

pub fn emit_admin_set(env: &Env, admin: &Address) {
    AdminSet {
        admin: admin.clone(),
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}

pub fn emit_admin_initialized(env: &Env, admin: &Address) {
    emit_admin_set(env, admin);
}

pub fn emit_admin_transferred(env: &Env, old_admin: &Address, new_admin: &Address) {
    AdminTransferred {
        old_admin: old_admin.clone(),
        new_admin: new_admin.clone(),
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}

pub fn emit_revenue_distributed(
    env: &Env,
    period: u64,
    treasury_amount: i128,
    developer_amount: i128,
    platform_amount: i128,
) {
    RevenueDistributed {
        period,
        treasury_amount,
        developer_amount,
        platform_amount,
    }
    .publish(env);
}

pub fn emit_invoice_status_updated(
    env: &Env,
    invoice_id: BytesN<32>,
    status: crate::invoice::InvoiceStatus,
) {
    InvoiceStatusUpdated { invoice_id, status }.publish(env);
}

pub fn emit_protocol_initialized(
    env: &Env,
    admin: &Address,
    treasury: &Address,
    fee_bps: u32,
    min_invoice_amount: i128,
    max_due_date_days: u64,
    grace_period_seconds: u64,
) {
    ProtocolInitialized {
        admin: admin.clone(),
        treasury: treasury.clone(),
        fee_bps,
        min_invoice_amount,
        max_due_date_days,
        grace_period_seconds,
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}

pub fn emit_admin_initialized(env: &Env, admin: &Address) {
    env.events().publish((symbol_short!("admin"),), admin.clone());
}
