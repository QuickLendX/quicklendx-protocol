#![cfg(test)]
use super::errors::QuickLendXError;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

fn load_snapshot() -> HashMap<String, u32> {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let snapshot_path = Path::new(&manifest_dir)
        .join("src")
        .join("test_snapshots")
        .join("error_codes.txt");

    let file = File::open(&snapshot_path)
        .expect("Failed to locate error-code snapshot file.");
    let reader = BufReader::new(file);
    let mut map = HashMap::new();

    for line in reader.lines() {
        let l = line.unwrap();
        if l.trim().is_empty() || l.starts_with('#') {
            continue;
        }
        let parts: Vec<&str> = l.split('=').collect();
        assert_eq!(parts.len(), 2, "Invalid formatting inside snapshot file.");
        map.insert(parts[0].trim().to_string(), parts[1].trim().parse::<u32>().unwrap());
    }
    map
}

#[test]
fn test_error_code_stability() {
    let snapshot = load_snapshot();

    let current_variants: Vec<(QuickLendXError, u32)> = vec![
        (QuickLendXError::InvoiceNotFound, QuickLendXError::InvoiceNotFound as u32),
        (QuickLendXError::InvoiceNotAvailableForFunding, QuickLendXError::InvoiceNotAvailableForFunding as u32),
        (QuickLendXError::InvoiceAlreadyFunded, QuickLendXError::InvoiceAlreadyFunded as u32),
        (QuickLendXError::InvoiceAmountInvalid, QuickLendXError::InvoiceAmountInvalid as u32),
        (QuickLendXError::InvoiceDueDateInvalid, QuickLendXError::InvoiceDueDateInvalid as u32),
        (QuickLendXError::InvoiceNotFunded, QuickLendXError::InvoiceNotFunded as u32),
        (QuickLendXError::InvoiceAlreadyDefaulted, QuickLendXError::InvoiceAlreadyDefaulted as u32),
        (QuickLendXError::Unauthorized, QuickLendXError::Unauthorized as u32),
        (QuickLendXError::NotBusinessOwner, QuickLendXError::NotBusinessOwner as u32),
        (QuickLendXError::NotInvestor, QuickLendXError::NotInvestor as u32),
        (QuickLendXError::NotAdmin, QuickLendXError::NotAdmin as u32),
        (QuickLendXError::InvalidAmount, QuickLendXError::InvalidAmount as u32),
        (QuickLendXError::InvalidAddress, QuickLendXError::InvalidAddress as u32),
        (QuickLendXError::InvalidCurrency, QuickLendXError::InvalidCurrency as u32),
        (QuickLendXError::InvalidTimestamp, QuickLendXError::InvalidTimestamp as u32),
        (QuickLendXError::InvalidDescription, QuickLendXError::InvalidDescription as u32),
        (QuickLendXError::StorageError, QuickLendXError::StorageError as u32),
        (QuickLendXError::StorageKeyNotFound, QuickLendXError::StorageKeyNotFound as u32),
        (QuickLendXError::InsufficientFunds, QuickLendXError::InsufficientFunds as u32),
        (QuickLendXError::InvalidStatus, QuickLendXError::InvalidStatus as u32),
        (QuickLendXError::OperationNotAllowed, QuickLendXError::OperationNotAllowed as u32),
        (QuickLendXError::PaymentTooLow, QuickLendXError::PaymentTooLow as u32),
        (QuickLendXError::PlatformAccountNotConfigured, QuickLendXError::PlatformAccountNotConfigured as u32),
        (QuickLendXError::InvalidCoveragePercentage, QuickLendXError::InvalidCoveragePercentage as u32),
        (QuickLendXError::MaxBidsPerInvoiceExceeded, QuickLendXError::MaxBidsPerInvoiceExceeded as u32),
        (QuickLendXError::MaxInvoicesPerBusinessExceeded, QuickLendXError::MaxInvoicesPerBusinessExceeded as u32),
        (QuickLendXError::InvalidBidTtl, QuickLendXError::InvalidBidTtl as u32),
        (QuickLendXError::InvalidRating, QuickLendXError::InvalidRating as u32),
        (QuickLendXError::NotFunded, QuickLendXError::NotFunded as u32),
        (QuickLendXError::AlreadyRated, QuickLendXError::AlreadyRated as u32),
        (QuickLendXError::NotRater, QuickLendXError::NotRater as u32),
        (QuickLendXError::BusinessNotVerified, QuickLendXError::BusinessNotVerified as u32),
        (QuickLendXError::KYCAlreadyPending, QuickLendXError::KYCAlreadyPending as u32),
        (QuickLendXError::KYCAlreadyVerified, QuickLendXError::KYCAlreadyVerified as u32),
        (QuickLendXError::KYCNotFound, QuickLendXError::KYCNotFound as u32),
        (QuickLendXError::InvalidKYCStatus, QuickLendXError::InvalidKYCStatus as u32),
        (QuickLendXError::InvestorNotVerified, QuickLendXError::InvestorNotVerified as u32),
        (QuickLendXError::AuditLogNotFound, QuickLendXError::AuditLogNotFound as u32),
        (QuickLendXError::AuditIntegrityError, QuickLendXError::AuditIntegrityError as u32),
        (QuickLendXError::AuditQueryError, QuickLendXError::AuditQueryError as u32),
        (QuickLendXError::InvalidTag, QuickLendXError::InvalidTag as u32),
        (QuickLendXError::TagLimitExceeded, QuickLendXError::TagLimitExceeded as u32),
        (QuickLendXError::InvalidFeeConfiguration, QuickLendXError::InvalidFeeConfiguration as u32),
        (QuickLendXError::TreasuryNotConfigured, QuickLendXError::TreasuryNotConfigured as u32),
        (QuickLendXError::InvalidFeeBasisPoints, QuickLendXError::InvalidFeeBasisPoints as u32),
        (QuickLendXError::ArithmeticOverflow, QuickLendXError::ArithmeticOverflow as u32),
        (QuickLendXError::RotationAlreadyPending, QuickLendXError::RotationAlreadyPending as u32),
        (QuickLendXError::RotationNotFound, QuickLendXError::RotationNotFound as u32),
        (QuickLendXError::RotationExpired, QuickLendXError::RotationExpired as u32),
        (QuickLendXError::DisputeNotFound, QuickLendXError::DisputeNotFound as u32),
        (QuickLendXError::DisputeAlreadyExists, QuickLendXError::DisputeAlreadyExists as u32),
        (QuickLendXError::DisputeNotAuthorized, QuickLendXError::DisputeNotAuthorized as u32),
        (QuickLendXError::DisputeAlreadyResolved, QuickLendXError::DisputeAlreadyResolved as u32),
        (QuickLendXError::DisputeNotUnderReview, QuickLendXError::DisputeNotUnderReview as u32),
        (QuickLendXError::InvalidDisputeReason, QuickLendXError::InvalidDisputeReason as u32),
        (QuickLendXError::InvalidDisputeEvidence, QuickLendXError::InvalidDisputeEvidence as u32),
        (QuickLendXError::NotificationNotFound, QuickLendXError::NotificationNotFound as u32),
        (QuickLendXError::NotificationBlocked, QuickLendXError::NotificationBlocked as u32),
        (QuickLendXError::NotificationDuplicate, QuickLendXError::NotificationDuplicate as u32),
        (QuickLendXError::ContractPaused, QuickLendXError::ContractPaused as u32),
        (QuickLendXError::EmergencyWithdrawNotFound, QuickLendXError::EmergencyWithdrawNotFound as u32),
        (QuickLendXError::EmergencyWithdrawTimelockNotElapsed, QuickLendXError::EmergencyWithdrawTimelockNotElapsed as u32),
        (QuickLendXError::EmergencyWithdrawExpired, QuickLendXError::EmergencyWithdrawExpired as u32),
        (QuickLendXError::EmergencyWithdrawCancelled, QuickLendXError::EmergencyWithdrawCancelled as u32),
        (QuickLendXError::EmergencyWithdrawAlreadyExists, QuickLendXError::EmergencyWithdrawAlreadyExists as u32),
        (QuickLendXError::EmergencyWithdrawInsufficientBalance, QuickLendXError::EmergencyWithdrawInsufficientBalance as u32),
        (QuickLendXError::TokenTransferFailed, QuickLendXError::TokenTransferFailed as u32),
        (QuickLendXError::MaintenanceModeActive, QuickLendXError::MaintenanceModeActive as u32),
        (QuickLendXError::DuplicateDefaultTransition, QuickLendXError::DuplicateDefaultTransition as u32),
    ];

    for (_variant, val) in current_variants.iter() {
        if let Some((name_in_snapshot, &expected_val)) = snapshot.iter().find(|&(_, &v)| v == *val) {
            assert_eq!(*val, expected_val, "Discriminant drift caught for variant '{}'!", name_in_snapshot);
        }
    }

    assert!(
        current_variants.len() >= snapshot.len(),
        "Fewer error codes found than snapshot contains. Removal/re-ordering is forbidden."
    );
}