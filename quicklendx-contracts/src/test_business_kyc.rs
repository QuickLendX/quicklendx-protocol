/// Comprehensive test suite for business KYC verification
///
/// Test Coverage:
/// 1. Business KYC Submission and Verification
/// 2. Admin-Only Operations
/// 3. Edge Cases (Duplicate submission, Re-submission)
/// 4. Integration with Invoice Upload
///
/// Target: 95%+ test coverage for business verification flow
#[cfg(test)]
mod test_business_kyc {
    use crate::errors::QuickLendXError;
    use crate::invoice::InvoiceCategory;
    use crate::verification::BusinessVerificationStatus;
    use crate::{QuickLendXContract, QuickLendXContractClient};
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        Address, Env, String, Vec,
    };

    // Helper: Setup contract with admin
    fn setup() -> (Env, QuickLendXContractClient<'static>, Address) {
        let env = Env::default();
        let contract_id = env.register(QuickLendXContract, ());
        let client = QuickLendXContractClient::new(&env, &contract_id);

        // Initialize admin
        let admin = Address::generate(&env);
        env.mock_all_auths();
        let _ = client.try_initialize_admin(&admin);

        (env, client, admin)
    }

    // ============================================================================
    // Category 1: Business KYC Submission Tests
    // ============================================================================

    #[test]
    fn test_business_kyc_submission_succeeds() {
        let (env, client, _admin) = setup();
        let business = Address::generate(&env);
        let kyc_data = String::from_str(&env, "Valid Business KYC data");

        let result = client.try_submit_kyc_application(&business, &kyc_data);
        assert!(result.is_ok(), "Valid KYC submission must succeed");

        // Verify business is in pending status
        let verification = client.get_business_verification_status(&business);
        assert!(verification.is_some(), "Verification record must exist");

        let verification = verification.unwrap();
        assert_eq!(verification.status, BusinessVerificationStatus::Pending);
        assert_eq!(verification.business, business);
        assert_eq!(verification.kyc_data, kyc_data);
    }

    #[test]
    fn test_business_kyc_duplicate_submission_fails() {
        let (env, client, _admin) = setup();
        let business = Address::generate(&env);
        let kyc_data = String::from_str(&env, "Valid KYC data");

        // First submission should succeed
        let result1 = client.try_submit_kyc_application(&business, &kyc_data);
        assert!(result1.is_ok(), "First KYC submission must succeed");

        // Second submission should fail
        let result2 = client.try_submit_kyc_application(&business, &kyc_data);
        assert!(result2.is_err(), "Duplicate KYC submission must fail");

        let error = result2.unwrap_err().unwrap();
        assert_eq!(error, QuickLendXError::KYCAlreadyPending);
    }

    #[test]
    fn test_business_kyc_resubmission_after_rejection() {
        let (env, client, admin) = setup();
        let business = Address::generate(&env);
        let kyc_data = String::from_str(&env, "Valid KYC data");

        // Submit and reject
        let _ = client.try_submit_kyc_application(&business, &kyc_data);
        let _ = client.try_reject_business(
            &admin,
            &business,
            &String::from_str(&env, "Insufficient documentation"),
        );

        // Resubmission after rejection should succeed
        let new_kyc_data = String::from_str(&env, "Updated KYC data");
        let result = client.try_submit_kyc_application(&business, &new_kyc_data);
        assert!(
            result.is_ok(),
            "KYC resubmission after rejection must succeed"
        );

        let verification = client.get_business_verification_status(&business);
        assert!(verification.is_some());
        assert_eq!(
            verification.unwrap().status,
            BusinessVerificationStatus::Pending
        );
    }

    // ============================================================================
    // Category 2: Admin-Only Business Verification Tests
    // ============================================================================

    #[test]
    fn test_admin_can_verify_business() {
        let (env, client, admin) = setup();
        let business = Address::generate(&env);
        let kyc_data = String::from_str(&env, "Business KYC data");

        // Submit KYC first
        let _ = client.try_submit_kyc_application(&business, &kyc_data);

        // Admin verification should succeed
        let result = client.try_verify_business(&admin, &business);
        assert!(result.is_ok(), "Admin business verification must succeed");

        // Verify business status
        let verification = client.get_business_verification_status(&business);
        assert!(verification.is_some());

        let verification = verification.unwrap();
        assert_eq!(verification.status, BusinessVerificationStatus::Verified);
        assert!(verification.verified_by.is_some());
        assert_eq!(verification.verified_by.unwrap(), admin);

        // Also check if in verified list
        let verified_list = client.get_verified_businesses();
        assert!(verified_list.contains(&business));
    }

    #[test]
    fn test_non_admin_cannot_verify_business() {
        let (env, client, _admin) = setup();
        let business = Address::generate(&env);
        let non_admin = Address::generate(&env);
        let kyc_data = String::from_str(&env, "Valid KYC data");

        // Submit KYC first
        let _ = client.try_submit_kyc_application(&business, &kyc_data);

        // Non-admin verification should fail
        let result = client.try_verify_business(&non_admin, &business);
        assert!(result.is_err(), "Non-admin verification must fail");

        let error = result.unwrap_err().unwrap();
        assert_eq!(error, QuickLendXError::NotAdmin);
    }

    #[test]
    fn test_admin_can_reject_business() {
        let (env, client, admin) = setup();
        let business = Address::generate(&env);
        let kyc_data = String::from_str(&env, "Insufficient KYC data");
        let rejection_reason = String::from_str(&env, "Missing tax docs");

        // Submit KYC first
        let _ = client.try_submit_kyc_application(&business, &kyc_data);

        // Admin rejection should succeed
        let result = client.try_reject_business(&admin, &business, &rejection_reason);
        assert!(result.is_ok(), "Admin business rejection must succeed");

        // Verify business status
        let verification = client.get_business_verification_status(&business);
        assert!(verification.is_some());

        let verification = verification.unwrap();
        assert_eq!(verification.status, BusinessVerificationStatus::Rejected);
        assert!(verification.rejection_reason.is_some());
        assert_eq!(verification.rejection_reason.unwrap(), rejection_reason);

        // Check rejected list
        let rejected_list = client.get_rejected_businesses();
        assert!(rejected_list.contains(&business));
    }

    #[test]
    fn test_verify_non_existent_business_fails() {
        let (env, client, admin) = setup();
        let business = Address::generate(&env);

        // Try to verify without KYC submission
        let result = client.try_verify_business(&admin, &business);
        assert!(
            result.is_err(),
            "Verification without KYC submission must fail"
        );

        let error = result.unwrap_err().unwrap();
        assert_eq!(error, QuickLendXError::KYCNotFound);
    }

    // ============================================================================
    // Category 3: Integration Tests
    // ============================================================================

    #[test]
    fn test_unverified_business_cannot_upload_invoice() {
        let (env, client, _admin) = setup();
        let business = Address::generate(&env);
        let currency = Address::generate(&env);
        let due_date = env.ledger().timestamp() + 86400;

        // Try upload without verification
        let result = client.try_upload_invoice(
            &business,
            &1000,
            &currency,
            &due_date,
            &String::from_str(&env, "Test Invoice"),
            &InvoiceCategory::Services,
            &Vec::new(&env),
        );

        assert!(result.is_err(), "Unverified business cannot upload invoice");
        let error = result.unwrap_err().unwrap();
        assert_eq!(error, QuickLendXError::BusinessNotVerified);
    }

    #[test]
    fn test_verified_business_can_upload_invoice() {
        let (env, client, admin) = setup();
        let business = Address::generate(&env);
        let currency = Address::generate(&env);
        let due_date = env.ledger().timestamp() + 86400;
        let kyc_data = String::from_str(&env, "KYC Data");

        // Verify business
        let _ = client.try_submit_kyc_application(&business, &kyc_data);
        let _ = client.try_verify_business(&admin, &business);

        // Upload should succeed
        let result = client.try_upload_invoice(
            &business,
            &1000,
            &currency,
            &due_date,
            &String::from_str(&env, "Test Invoice"),
            &InvoiceCategory::Services,
            &Vec::new(&env),
        );

        assert!(
            result.is_ok(),
            "Verified business must be able to upload invoice"
        );
    }

    #[test]
    fn test_pending_business_cannot_upload_invoice() {
        let (env, client, _admin) = setup();
        let business = Address::generate(&env);
        let currency = Address::generate(&env);
        let due_date = env.ledger().timestamp() + 86400;
        let kyc_data = String::from_str(&env, "KYC Data");

        // Submit KYC but stay pending
        let _ = client.try_submit_kyc_application(&business, &kyc_data);

        // Upload should fail
        let result = client.try_upload_invoice(
            &business,
            &1000,
            &currency,
            &due_date,
            &String::from_str(&env, "Test Invoice"),
            &InvoiceCategory::Services,
            &Vec::new(&env),
        );

        assert!(result.is_err(), "Pending business cannot upload invoice");
        let error = result.unwrap_err().unwrap();
        assert_eq!(error, QuickLendXError::BusinessNotVerified);
    }
}
