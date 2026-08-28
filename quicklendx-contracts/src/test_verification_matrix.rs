#![cfg(test)]

use crate::errors::QuickLendXError;
use crate::verification::{
    BusinessVerification, BusinessVerificationStatus, BusinessVerificationStorage,
};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::{Address, Env, String};

fn setup_env() -> (Env, Address) {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000_000);
    let business = Address::generate(&env);
    (env, business)
}

fn create_verification(
    env: &Env,
    business: &Address,
    status: BusinessVerificationStatus,
    reason: Option<&str>,
) -> BusinessVerification {
    BusinessVerification {
        business: business.clone(),
        status,
        verified_at: Some(env.ledger().timestamp()),
        verified_by: Some(Address::generate(env)),
        kyc_data: String::from_str(env, "encrypted_kyc_data"),
        submitted_at: env.ledger().timestamp(),
        rejection_reason: reason.map(|r| String::from_str(env, r)),
    }
}

// ============================================================================
// 1. ONE TEST PER ALLOWED TRANSITION
// ============================================================================

#[test]
fn allows_transition_none_to_pending() {
    let res = BusinessVerificationStorage::validate_state_transition(
        None,
        BusinessVerificationStatus::Pending,
    );
    assert!(res.is_ok(), "None -> Pending transition must be allowed");
}

#[test]
fn allows_transition_pending_to_verified() {
    let res = BusinessVerificationStorage::validate_state_transition(
        Some(BusinessVerificationStatus::Pending),
        BusinessVerificationStatus::Verified,
    );
    assert!(
        res.is_ok(),
        "Pending -> Verified transition must be allowed"
    );
}

#[test]
fn allows_transition_pending_to_rejected() {
    let res = BusinessVerificationStorage::validate_state_transition(
        Some(BusinessVerificationStatus::Pending),
        BusinessVerificationStatus::Rejected,
    );
    assert!(
        res.is_ok(),
        "Pending -> Rejected transition must be allowed"
    );
}

#[test]
fn allows_transition_rejected_to_pending() {
    let res = BusinessVerificationStorage::validate_state_transition(
        Some(BusinessVerificationStatus::Rejected),
        BusinessVerificationStatus::Pending,
    );
    assert!(
        res.is_ok(),
        "Rejected -> Pending transition must be allowed"
    );
}

// ============================================================================
// 2. DISALLOWED & SAD PATH TRANSITION TESTS
// ============================================================================

#[test]
fn rejects_transition_verified_to_any_state() {
    let states = [
        BusinessVerificationStatus::Pending,
        BusinessVerificationStatus::Verified,
        BusinessVerificationStatus::Rejected,
    ];

    for target_status in states {
        let res = BusinessVerificationStorage::validate_state_transition(
            Some(BusinessVerificationStatus::Verified),
            target_status.clone(),
        );
        assert_eq!(
            res,
            Err(QuickLendXError::InvalidKYCStatus),
            "Verified -> {:?} must be rejected as Verified is a terminal state",
            target_status
        );
    }
}

#[test]
fn rejects_duplicate_pending_transition() {
    let res = BusinessVerificationStorage::validate_state_transition(
        Some(BusinessVerificationStatus::Pending),
        BusinessVerificationStatus::Pending,
    );
    assert_eq!(
        res,
        Err(QuickLendXError::KYCAlreadyPending),
        "Pending -> Pending duplicate transition must be rejected"
    );
}

#[test]
fn rejects_duplicate_rejected_transition() {
    let res = BusinessVerificationStorage::validate_state_transition(
        Some(BusinessVerificationStatus::Rejected),
        BusinessVerificationStatus::Rejected,
    );
    assert_eq!(
        res,
        Err(QuickLendXError::InvalidKYCStatus),
        "Rejected -> Rejected duplicate transition must be rejected"
    );
}

#[test]
fn rejects_direct_rejected_to_verified_transition() {
    let res = BusinessVerificationStorage::validate_state_transition(
        Some(BusinessVerificationStatus::Rejected),
        BusinessVerificationStatus::Verified,
    );
    assert_eq!(
        res,
        Err(QuickLendXError::InvalidKYCStatus),
        "Rejected -> Verified direct transition must be rejected (must go through Pending first)"
    );
}

#[test]
fn rejects_unsubmitted_verification_or_rejection() {
    assert_eq!(
        BusinessVerificationStorage::validate_state_transition(
            None,
            BusinessVerificationStatus::Verified
        ),
        Err(QuickLendXError::InvalidKYCStatus),
        "None -> Verified transition without submission must be rejected"
    );

    assert_eq!(
        BusinessVerificationStorage::validate_state_transition(
            None,
            BusinessVerificationStatus::Rejected
        ),
        Err(QuickLendXError::InvalidKYCStatus),
        "None -> Rejected transition without submission must be rejected"
    );
}

// ============================================================================
// 3. EXHAUSTIVE TRANSITION MATRIX TEST
// ============================================================================

#[test]
fn test_business_verification_full_state_transition_matrix() {
    let old_statuses: [Option<BusinessVerificationStatus>; 4] = [
        None,
        Some(BusinessVerificationStatus::Pending),
        Some(BusinessVerificationStatus::Verified),
        Some(BusinessVerificationStatus::Rejected),
    ];

    let new_statuses: [BusinessVerificationStatus; 3] = [
        BusinessVerificationStatus::Pending,
        BusinessVerificationStatus::Verified,
        BusinessVerificationStatus::Rejected,
    ];

    for old_status in &old_statuses {
        for new_status in &new_statuses {
            let is_allowed = match (old_status, new_status) {
                (None, BusinessVerificationStatus::Pending) => true,
                (
                    Some(BusinessVerificationStatus::Pending),
                    BusinessVerificationStatus::Verified,
                ) => true,
                (
                    Some(BusinessVerificationStatus::Pending),
                    BusinessVerificationStatus::Rejected,
                ) => true,
                (
                    Some(BusinessVerificationStatus::Rejected),
                    BusinessVerificationStatus::Pending,
                ) => true,
                _ => false,
            };

            let res = BusinessVerificationStorage::validate_state_transition(
                old_status.clone(),
                new_status.clone(),
            );

            assert_eq!(
                res.is_ok(),
                is_allowed,
                "Matrix check failed for transition ({:?}, {:?}): expected allowed={}, got result={:?}",
                old_status,
                new_status,
                is_allowed,
                res
            );
        }
    }
}

// ============================================================================
// 4. STORAGE UPDATE & INDEX CONSISTENCY MATRIX INTEGRATION TEST
// ============================================================================

#[test]
fn allows_full_lifecycle_storage_updates_across_all_valid_transitions() {
    let (env, business) = setup_env();

    // 1. None -> Pending
    let pending_ver =
        create_verification(&env, &business, BusinessVerificationStatus::Pending, None);
    assert!(BusinessVerificationStorage::update_verification(&env, &pending_ver).is_ok());
    assert_eq!(
        BusinessVerificationStorage::get_pending_businesses(&env).len(),
        1
    );
    assert_eq!(
        BusinessVerificationStorage::get_verified_businesses(&env).len(),
        0
    );
    assert_eq!(
        BusinessVerificationStorage::get_rejected_businesses(&env).len(),
        0
    );

    // 2. Pending -> Rejected
    let rejected_ver = create_verification(
        &env,
        &business,
        BusinessVerificationStatus::Rejected,
        Some("Missing documents"),
    );
    assert!(BusinessVerificationStorage::update_verification(&env, &rejected_ver).is_ok());
    assert_eq!(
        BusinessVerificationStorage::get_pending_businesses(&env).len(),
        0
    );
    assert_eq!(
        BusinessVerificationStorage::get_rejected_businesses(&env).len(),
        1
    );

    // 3. Rejected -> Pending
    let resubmitted_ver = create_verification(
        &env,
        &business,
        BusinessVerificationStatus::Pending,
        Some("Missing documents"),
    );
    assert!(BusinessVerificationStorage::update_verification(&env, &resubmitted_ver).is_ok());
    assert_eq!(
        BusinessVerificationStorage::get_pending_businesses(&env).len(),
        1
    );
    assert_eq!(
        BusinessVerificationStorage::get_rejected_businesses(&env).len(),
        0
    );

    // 4. Pending -> Verified
    let verified_ver = create_verification(
        &env,
        &business,
        BusinessVerificationStatus::Verified,
        Some("Missing documents"),
    );
    assert!(BusinessVerificationStorage::update_verification(&env, &verified_ver).is_ok());
    assert_eq!(
        BusinessVerificationStorage::get_verified_businesses(&env).len(),
        1
    );
    assert_eq!(
        BusinessVerificationStorage::get_pending_businesses(&env).len(),
        0
    );

    // 5. Verified -> Pending (invalid, should fail)
    let re_pending =
        create_verification(&env, &business, BusinessVerificationStatus::Pending, None);
    assert_eq!(
        BusinessVerificationStorage::update_verification(&env, &re_pending),
        Err(QuickLendXError::InvalidKYCStatus)
    );
}
