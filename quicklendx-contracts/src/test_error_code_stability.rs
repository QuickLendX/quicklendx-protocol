//! Error-code discriminant stability snapshot test.
//!
//! # Purpose
//! The Soroban `#[contracterror]` macro assigns `#[repr(u32)]` discriminants
//! that become part of the on-chain ABI.  Renumbering any variant silently
//! changes the error code returned to callers — a **breaking change** that
//! off-chain indexers, monitoring rules, and UI error handlers depend on.
//!
//! This test asserts that every variant in `QuickLendXError` still has the
//! **exact** numeric value recorded in `test_snapshots/error_codes.txt`.
//! Any diff in that file is a **reviewable, intentional** breaking change.

#![cfg(test)]

use crate::errors::QuickLendXError;

const SNAPSHOT: &str = include_str!("test_snapshots/error_codes.txt");

#[test]
fn test_snapshot_file_exists_and_is_nonempty() {
    let non_comment_lines: Vec<&str> = SNAPSHOT
        .lines()
        .filter(|l| !l.trim().is_empty() && !l.trim().starts_with('#'))
        .collect();
    assert!(non_comment_lines.len() >= 80);
}

// --- Invoice lifecycle (1000-1008) ---

#[test]
fn test_error_code_invoice_not_found() {
    assert_snapshot_entry("InvoiceNotFound", QuickLendXError::InvoiceNotFound as u32);
}
#[test]
fn test_error_code_invoice_not_available_for_funding() {
    assert_snapshot_entry("InvoiceNotAvailableForFunding", QuickLendXError::InvoiceNotAvailableForFunding as u32);
}
#[test]
fn test_error_code_invoice_already_funded() {
    assert_snapshot_entry("InvoiceAlreadyFunded", QuickLendXError::InvoiceAlreadyFunded as u32);
}
#[test]
fn test_error_code_invoice_amount_invalid() {
    assert_snapshot_entry("InvoiceAmountInvalid", QuickLendXError::InvoiceAmountInvalid as u32);
}
#[test]
fn test_error_code_invoice_due_date_invalid() {
    assert_snapshot_entry("InvoiceDueDateInvalid", QuickLendXError::InvoiceDueDateInvalid as u32);
}
#[test]
fn test_error_code_invoice_not_funded() {
    assert_snapshot_entry("InvoiceNotFunded", QuickLendXError::InvoiceNotFunded as u32);
}
#[test]
fn test_error_code_invoice_already_defaulted() {
    assert_snapshot_entry("InvoiceAlreadyDefaulted", QuickLendXError::InvoiceAlreadyDefaulted as u32);
}
#[test]
fn test_error_code_invoice_frozen() {
    assert_snapshot_entry("InvoiceFrozen", QuickLendXError::InvoiceFrozen as u32);
}
#[test]
fn test_error_code_invalid_freeze_reason() {
    assert_snapshot_entry("InvalidFreezeReason", QuickLendXError::InvalidFreezeReason as u32);
}

// --- Authorization (1100-1104) ---

#[test]
fn test_error_code_unauthorized() {
    assert_snapshot_entry("Unauthorized", QuickLendXError::Unauthorized as u32);
}
#[test]
fn test_error_code_not_business_owner() {
    assert_snapshot_entry("NotBusinessOwner", QuickLendXError::NotBusinessOwner as u32);
}
#[test]
fn test_error_code_not_investor() {
    assert_snapshot_entry("NotInvestor", QuickLendXError::NotInvestor as u32);
}
#[test]
fn test_error_code_not_admin() {
    assert_snapshot_entry("NotAdmin", QuickLendXError::NotAdmin as u32);
}
#[test]
fn test_error_code_self_call_not_allowed() {
    assert_snapshot_entry("SelfCallNotAllowed", QuickLendXError::SelfCallNotAllowed as u32);
}

// --- Input validation (1200-1205) ---

#[test]
fn test_error_code_invalid_amount() {
    assert_snapshot_entry("InvalidAmount", QuickLendXError::InvalidAmount as u32);
}
#[test]
fn test_error_code_invalid_address() {
    assert_snapshot_entry("InvalidAddress", QuickLendXError::InvalidAddress as u32);
}
#[test]
fn test_error_code_invalid_currency() {
    assert_snapshot_entry("InvalidCurrency", QuickLendXError::InvalidCurrency as u32);
}
#[test]
fn test_error_code_invalid_timestamp() {
    assert_snapshot_entry("InvalidTimestamp", QuickLendXError::InvalidTimestamp as u32);
}
#[test]
fn test_error_code_invalid_description() {
    assert_snapshot_entry("InvalidDescription", QuickLendXError::InvalidDescription as u32);
}
#[test]
fn test_error_code_self_transfer() {
    assert_snapshot_entry("SelfTransfer", QuickLendXError::SelfTransfer as u32);
}

// --- Storage (1300-1301) ---

#[test]
fn test_error_code_storage_error() {
    assert_snapshot_entry("StorageError", QuickLendXError::StorageError as u32);
}
#[test]
fn test_error_code_storage_key_not_found() {
    assert_snapshot_entry("StorageKeyNotFound", QuickLendXError::StorageKeyNotFound as u32);
}

// --- Business logic (1400-1410) ---

#[test]
fn test_error_code_insufficient_funds() {
    assert_snapshot_entry("InsufficientFunds", QuickLendXError::InsufficientFunds as u32);
}
#[test]
fn test_error_code_invalid_status() {
    assert_snapshot_entry("InvalidStatus", QuickLendXError::InvalidStatus as u32);
}
#[test]
fn test_error_code_operation_not_allowed() {
    assert_snapshot_entry("OperationNotAllowed", QuickLendXError::OperationNotAllowed as u32);
}
#[test]
fn test_error_code_payment_too_low() {
    assert_snapshot_entry("PaymentTooLow", QuickLendXError::PaymentTooLow as u32);
}
#[test]
fn test_error_code_platform_account_not_configured() {
    assert_snapshot_entry("PlatformAccountNotConfigured", QuickLendXError::PlatformAccountNotConfigured as u32);
}
#[test]
fn test_error_code_invalid_coverage_percentage() {
    assert_snapshot_entry("InvalidCoveragePercentage", QuickLendXError::InvalidCoveragePercentage as u32);
}
#[test]
fn test_error_code_max_bids_per_invoice_exceeded() {
    assert_snapshot_entry("MaxBidsPerInvoiceExceeded", QuickLendXError::MaxBidsPerInvoiceExceeded as u32);
}
#[test]
fn test_error_code_max_active_bids_per_investor_exceeded() {
    assert_snapshot_entry("MaxActiveBidsPerInvestorExceeded", QuickLendXError::MaxActiveBidsPerInvestorExceeded as u32);
}
#[test]
fn test_error_code_max_invoices_per_business_exceeded() {
    assert_snapshot_entry("MaxInvoicesPerBusinessExceeded", QuickLendXError::MaxInvoicesPerBusinessExceeded as u32);
}
#[test]
fn test_error_code_invalid_bid_ttl() {
    assert_snapshot_entry("InvalidBidTtl", QuickLendXError::InvalidBidTtl as u32);
}
#[test]
fn test_error_code_insufficient_kyc_tier() {
    assert_snapshot_entry("InsufficientKYCTier", QuickLendXError::InsufficientKYCTier as u32);
}

// --- Rating (1500-1504) ---

#[test]
fn test_error_code_invalid_rating() {
    assert_snapshot_entry("InvalidRating", QuickLendXError::InvalidRating as u32);
}
#[test]
fn test_error_code_not_funded() {
    assert_snapshot_entry("NotFunded", QuickLendXError::NotFunded as u32);
}
#[test]
fn test_error_code_already_rated() {
    assert_snapshot_entry("AlreadyRated", QuickLendXError::AlreadyRated as u32);
}
#[test]
fn test_error_code_not_rater() {
    assert_snapshot_entry("NotRater", QuickLendXError::NotRater as u32);
}
#[test]
fn test_error_code_invalid_rating_override_reason() {
    assert_snapshot_entry("InvalidRatingOverrideReason", QuickLendXError::InvalidRatingOverrideReason as u32);
}

// --- KYC / verification (1600-1660) ---

#[test]
fn test_error_code_business_not_verified() {
    assert_snapshot_entry("BusinessNotVerified", QuickLendXError::BusinessNotVerified as u32);
}
#[test]
fn test_error_code_kyc_already_pending() {
    assert_snapshot_entry("KYCAlreadyPending", QuickLendXError::KYCAlreadyPending as u32);
}
#[test]
fn test_error_code_kyc_already_verified() {
    assert_snapshot_entry("KYCAlreadyVerified", QuickLendXError::KYCAlreadyVerified as u32);
}
#[test]
fn test_error_code_kyc_not_found() {
    assert_snapshot_entry("KYCNotFound", QuickLendXError::KYCNotFound as u32);
}
#[test]
fn test_error_code_invalid_kyc_status() {
    assert_snapshot_entry("InvalidKYCStatus", QuickLendXError::InvalidKYCStatus as u32);
}
#[test]
fn test_error_code_investor_not_verified() {
    assert_snapshot_entry("InvestorNotVerified", QuickLendXError::InvestorNotVerified as u32);
}
#[test]
fn test_error_code_business_deleted() {
    assert_snapshot_entry("BusinessDeleted", QuickLendXError::BusinessDeleted as u32);
}

// --- Audit (1700-1702) ---

#[test]
fn test_error_code_audit_log_not_found() {
    assert_snapshot_entry("AuditLogNotFound", QuickLendXError::AuditLogNotFound as u32);
}
#[test]
fn test_error_code_audit_integrity_error() {
    assert_snapshot_entry("AuditIntegrityError", QuickLendXError::AuditIntegrityError as u32);
}
#[test]
fn test_error_code_audit_query_error() {
    assert_snapshot_entry("AuditQueryError", QuickLendXError::AuditQueryError as u32);
}

// --- Category / tag (1800-1801) ---

#[test]
fn test_error_code_invalid_tag() {
    assert_snapshot_entry("InvalidTag", QuickLendXError::InvalidTag as u32);
}
#[test]
fn test_error_code_tag_limit_exceeded() {
    assert_snapshot_entry("TagLimitExceeded", QuickLendXError::TagLimitExceeded as u32);
}

// --- Fee configuration (1850-1858) ---

#[test]
fn test_error_code_invalid_fee_configuration() {
    assert_snapshot_entry("InvalidFeeConfiguration", QuickLendXError::InvalidFeeConfiguration as u32);
}
#[test]
fn test_error_code_treasury_not_configured() {
    assert_snapshot_entry("TreasuryNotConfigured", QuickLendXError::TreasuryNotConfigured as u32);
}
#[test]
fn test_error_code_invalid_fee_basis_points() {
    assert_snapshot_entry("InvalidFeeBasisPoints", QuickLendXError::InvalidFeeBasisPoints as u32);
}
#[test]
fn test_error_code_rotation_already_pending() {
    assert_snapshot_entry("RotationAlreadyPending", QuickLendXError::RotationAlreadyPending as u32);
}
#[test]
fn test_error_code_rotation_not_found() {
    assert_snapshot_entry("RotationNotFound", QuickLendXError::RotationNotFound as u32);
}
#[test]
fn test_error_code_rotation_expired() {
    assert_snapshot_entry("RotationExpired", QuickLendXError::RotationExpired as u32);
}
#[test]
fn test_error_code_arithmetic_overflow() {
    assert_snapshot_entry("ArithmeticOverflow", QuickLendXError::ArithmeticOverflow as u32);
}
#[test]
fn test_error_code_rotation_timelock_not_elapsed() {
    assert_snapshot_entry("RotationTimelockNotElapsed", QuickLendXError::RotationTimelockNotElapsed as u32);
}
#[test]
fn test_error_code_no_pending_treasury_rotation() {
    assert_snapshot_entry("NoPendingTreasuryRotation", QuickLendXError::NoPendingTreasuryRotation as u32);
}

// --- Dispute (1900-1907) ---

#[test]
fn test_error_code_dispute_not_found() {
    assert_snapshot_entry("DisputeNotFound", QuickLendXError::DisputeNotFound as u32);
}
#[test]
fn test_error_code_dispute_already_exists() {
    assert_snapshot_entry("DisputeAlreadyExists", QuickLendXError::DisputeAlreadyExists as u32);
}
#[test]
fn test_error_code_dispute_not_authorized() {
    assert_snapshot_entry("DisputeNotAuthorized", QuickLendXError::DisputeNotAuthorized as u32);
}
#[test]
fn test_error_code_dispute_already_resolved() {
    assert_snapshot_entry("DisputeAlreadyResolved", QuickLendXError::DisputeAlreadyResolved as u32);
}
#[test]
fn test_error_code_dispute_not_under_review() {
    assert_snapshot_entry("DisputeNotUnderReview", QuickLendXError::DisputeNotUnderReview as u32);
}
#[test]
fn test_error_code_invalid_dispute_reason() {
    assert_snapshot_entry("InvalidDisputeReason", QuickLendXError::InvalidDisputeReason as u32);
}
#[test]
fn test_error_code_invalid_dispute_evidence() {
    assert_snapshot_entry("InvalidDisputeEvidence", QuickLendXError::InvalidDisputeEvidence as u32);
}
#[test]
fn test_error_code_dispute_active() {
    assert_snapshot_entry("DisputeActive", QuickLendXError::DisputeActive as u32);
}

// --- Notification (2000-2002) ---

#[test]
fn test_error_code_notification_not_found() {
    assert_snapshot_entry("NotificationNotFound", QuickLendXError::NotificationNotFound as u32);
}
#[test]
fn test_error_code_notification_blocked() {
    assert_snapshot_entry("NotificationBlocked", QuickLendXError::NotificationBlocked as u32);
}
#[test]
fn test_error_code_notification_duplicate() {
    assert_snapshot_entry("NotificationDuplicate", QuickLendXError::NotificationDuplicate as u32);
}

// --- Emergency withdraw (2100-2106) ---

#[test]
fn test_error_code_contract_paused() {
    assert_snapshot_entry("ContractPaused", QuickLendXError::ContractPaused as u32);
}
#[test]
fn test_error_code_emergency_withdraw_not_found() {
    assert_snapshot_entry("EmergencyWithdrawNotFound", QuickLendXError::EmergencyWithdrawNotFound as u32);
}
#[test]
fn test_error_code_emergency_withdraw_timelock_not_elapsed() {
    assert_snapshot_entry("EmergencyWithdrawTimelockNotElapsed", QuickLendXError::EmergencyWithdrawTimelockNotElapsed as u32);
}
#[test]
fn test_error_code_emergency_withdraw_expired() {
    assert_snapshot_entry("EmergencyWithdrawExpired", QuickLendXError::EmergencyWithdrawExpired as u32);
}
#[test]
fn test_error_code_emergency_withdraw_cancelled() {
    assert_snapshot_entry("EmergencyWithdrawCancelled", QuickLendXError::EmergencyWithdrawCancelled as u32);
}
#[test]
fn test_error_code_emergency_withdraw_already_exists() {
    assert_snapshot_entry("EmergencyWithdrawAlreadyExists", QuickLendXError::EmergencyWithdrawAlreadyExists as u32);
}
#[test]
fn test_error_code_emergency_withdraw_insufficient_balance() {
    assert_snapshot_entry("EmergencyWithdrawInsufficientBalance", QuickLendXError::EmergencyWithdrawInsufficientBalance as u32);
}

// --- Other (2200-2207) ---

#[test]
fn test_error_code_token_transfer_failed() {
    assert_snapshot_entry("TokenTransferFailed", QuickLendXError::TokenTransferFailed as u32);
}
#[test]
fn test_error_code_maintenance_mode_active() {
    assert_snapshot_entry("MaintenanceModeActive", QuickLendXError::MaintenanceModeActive as u32);
}
#[test]
fn test_error_code_duplicate_default_transition() {
    assert_snapshot_entry("DuplicateDefaultTransition", QuickLendXError::DuplicateDefaultTransition as u32);
}
#[test]
fn test_error_code_backup_version_unsupported() {
    assert_snapshot_entry("BackupVersionUnsupported", QuickLendXError::BackupVersionUnsupported as u32);
}
#[test]
fn test_error_code_invalid_ledger_sequence() {
    assert_snapshot_entry("InvalidLedgerSequence", QuickLendXError::InvalidLedgerSequence as u32);
}
#[test]
fn test_error_code_insurance_not_active() {
    assert_snapshot_entry("InsuranceNotActive", QuickLendXError::InsuranceNotActive as u32);
}
#[test]
fn test_error_code_active_dispute_exists() {
    assert_snapshot_entry("ActiveDisputeExists", QuickLendXError::ActiveDisputeExists as u32);
}

// ---------------------------------------------------------------------------
// Helper: parse snapshot and assert a specific entry
// ---------------------------------------------------------------------------

fn assert_snapshot_entry(variant_name: &str, expected_value: u32) {
    for line in SNAPSHOT.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let parts: Vec<&str> = line.splitn(2, '=').map(str::trim).collect();
        if parts.len() != 2 {
            continue;
        }
        if parts[0] == variant_name {
            let value: u32 = parts[1].parse().expect("Invalid numeric value in snapshot");
            assert_eq!(value, expected_value);
            return;
        }
    }
    panic!("Variant '{}' not found in snapshot file", variant_name);
}