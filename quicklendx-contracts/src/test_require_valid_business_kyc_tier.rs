#![cfg(test)]

use crate::errors::QuickLendXError;
use crate::verification::{
    require_valid_business_kyc_tier, BusinessVerification, BusinessVerificationStatus,
};
use soroban_sdk::{testutils::Address as _, Address, Env, String};

fn mock_verification(env: &Env, status: BusinessVerificationStatus) -> BusinessVerification {
    let business = Address::generate(env);
    BusinessVerification {
        business,
        status,
        verified_at: None,
        verified_by: None,
        kyc_data: String::from_str(env, "sample_kyc_data"),
        submitted_at: env.ledger().timestamp(),
        rejection_reason: None,
    }
}

#[test]
fn test_require_valid_business_kyc_tier_verified_succeeds() {
    let env = Env::default();
    let verification = mock_verification(&env, BusinessVerificationStatus::Verified);
    let result = require_valid_business_kyc_tier(&verification);
    assert!(result.is_ok(), "Verified status must return Ok(())");
}

#[test]
fn test_require_valid_business_kyc_tier_pending_fails() {
    let env = Env::default();
    let verification = mock_verification(&env, BusinessVerificationStatus::Pending);
    let result = require_valid_business_kyc_tier(&verification);
    assert_eq!(
        result,
        Err(QuickLendXError::KYCAlreadyPending),
        "Pending status must return KYCAlreadyPending"
    );
}

#[test]
fn test_require_valid_business_kyc_tier_rejected_fails() {
    let env = Env::default();
    let verification = mock_verification(&env, BusinessVerificationStatus::Rejected);
    let result = require_valid_business_kyc_tier(&verification);
    assert_eq!(
        result,
        Err(QuickLendXError::BusinessNotVerified),
        "Rejected status must return BusinessNotVerified"
    );
}
