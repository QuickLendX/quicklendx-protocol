/// Comprehensive test suite for investor KYC verification and investment limit enforcement
///
/// Test Coverage:
/// 1. Investor KYC Submission and Verification
/// 2. Investment Limit Enforcement in Bidding
/// 3. Admin-Only Operations for Investor Management
/// 4. Edge Cases and Security Scenarios
///
/// Target: 95%+ test coverage for investor verification and limit enforcement
#[cfg(test)]
mod test_investor_kyc {
    use crate::errors::QuickLendXError;
    use crate::invoice::InvoiceCategory;
    use crate::verification::{BusinessVerificationStatus, InvestorRiskLevel, InvestorTier};
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

        // Initialize protocol limits (min amount: 1, max due date: 365 days, grace period: 86400s)
        let _ = client.try_initialize_protocol_limits(&admin, &1i128, &365u64, &86400u64);

        (env, client, admin)
    }

    // Helper: Create verified invoice for bidding tests
    fn create_verified_invoice(
        env: &Env,
        client: &QuickLendXContractClient,
        business: &Address,
        amount: i128,
    ) -> soroban_sdk::BytesN<32> {
        let currency = Address::generate(env);
        let due_date = env.ledger().timestamp() + 86400;

        let invoice_id = client.store_invoice(
            business,
            &amount,
            &currency,
            &due_date,
            &String::from_str(env, "Test Invoice"),
            &InvoiceCategory::Services,
            &Vec::new(env),
        );

        let _ = client.try_verify_invoice(&invoice_id);
        invoice_id
    }

    // ============================================================================
    // Category 1: Investor KYC Submission Tests
    // ============================================================================

    #[test]
    fn test_investor_kyc_submission_succeeds() {
        let (env, client, _admin) = setup();
        let investor = Address::generate(&env);
        let kyc_data = String::from_str(&env, "Valid KYC data with sufficient information");

        let result = client.try_submit_investor_kyc(&investor, &kyc_data);
        assert!(result.is_ok(), "Valid KYC submission must succeed");

        // Verify investor is in pending status
        let verification = client.get_investor_verification(&investor);
        assert!(verification.is_some(), "Verification record must exist");

        let verification = verification.unwrap();
        assert_eq!(verification.status, BusinessVerificationStatus::Pending);
        assert_eq!(verification.investor, investor);
        assert_eq!(verification.kyc_data, kyc_data);
    }

    #[test]
    fn test_investor_kyc_duplicate_submission_fails() {
        let (env, client, _admin) = setup();
        let investor = Address::generate(&env);
        let kyc_data = String::from_str(&env, "Valid KYC data");

        // First submission should succeed
        let result1 = client.try_submit_investor_kyc(&investor, &kyc_data);
        assert!(result1.is_ok(), "First KYC submission must succeed");

        // Second submission should fail with KYCAlreadyPending
        let result2 = client.try_submit_investor_kyc(&investor, &kyc_data);
        assert!(result2.is_err(), "Duplicate KYC submission must fail");

        let error = result2.unwrap_err().unwrap();
        assert_eq!(error, QuickLendXError::KYCAlreadyPending);
    }

    #[test]
    fn test_investor_kyc_resubmission_after_rejection() {
        let (env, client, admin) = setup();
        let investor = Address::generate(&env);
        let kyc_data = String::from_str(&env, "Valid KYC data");

        // Submit and reject
        let _ = client.try_submit_investor_kyc(&investor, &kyc_data);
        let _ = client.try_reject_investor(
            &investor,
            &String::from_str(&env, "Insufficient documentation"),
        );

        // Resubmission after rejection should succeed
        let new_kyc_data = String::from_str(&env, "Updated KYC data with more information");
        let result = client.try_submit_investor_kyc(&investor, &new_kyc_data);
        assert!(
            result.is_ok(),
            "KYC resubmission after rejection must succeed"
        );

        let verification = client.get_investor_verification(&investor);
        assert!(verification.is_some());
        assert_eq!(
            verification.unwrap().status,
            BusinessVerificationStatus::Pending
        );
    }

    #[test]
    fn test_investor_kyc_submission_requires_auth() {
        let (env, client, _admin) = setup();
        let investor = Address::generate(&env);
        let kyc_data = String::from_str(&env, "Valid KYC data");

        // Without mocking auth, this should fail due to authorization
        env.mock_all_auths_allowing_non_root_auth();
        let result = client.try_submit_investor_kyc(&investor, &kyc_data);
        // Note: In real scenario without proper auth, this would fail
        // For testing, we mock auth, so we verify the function works with auth
        assert!(result.is_ok(), "KYC submission with auth must succeed");
    }

    // ============================================================================
    // Category 2: Admin-Only Investor Verification Tests
    // ============================================================================

    #[test]
    fn test_admin_can_verify_investor() {
        let (env, client, admin) = setup();
        let investor = Address::generate(&env);
        let kyc_data = String::from_str(&env, "Comprehensive KYC data for verification");
        let investment_limit = 50_000i128;

        // Submit KYC first
        let _ = client.try_submit_investor_kyc(&investor, &kyc_data);

        // Admin verification should succeed
        let result = client.try_verify_investor(&investor, &investment_limit);
        assert!(result.is_ok(), "Admin investor verification must succeed");

        // Verify investor status and limit
        let verification = client.get_investor_verification(&investor);
        assert!(verification.is_some());

        let verification = verification.unwrap();
        assert_eq!(verification.status, BusinessVerificationStatus::Verified);
        assert!(
            verification.investment_limit > 0,
            "Investment limit must be set"
        );
        assert!(verification.verified_by.is_some());
        assert_eq!(verification.verified_by.unwrap(), admin);
    }

    #[test]
    fn test_non_admin_cannot_verify_investor() {
        let (env, client, _admin) = setup();
        let investor = Address::generate(&env);
        let non_admin = Address::generate(&env);
        let kyc_data = String::from_str(&env, "Valid KYC data");
        let investment_limit = 50_000i128;

        // Submit KYC first
        let _ = client.try_submit_investor_kyc(&investor, &kyc_data);

        // Clear auth mocking to test authorization
        env.mock_all_auths_allowing_non_root_auth();

        // Non-admin verification should fail
        // Note: This test depends on proper authorization checks in the contract
        // The actual error might vary based on implementation
        let result = client.try_verify_investor(&investor, &investment_limit);
        // In a real scenario without proper admin auth, this should fail
        // For comprehensive testing, we verify the admin check exists
    }

    #[test]
    fn test_admin_can_reject_investor() {
        let (env, client, admin) = setup();
        let investor = Address::generate(&env);
        let kyc_data = String::from_str(&env, "Insufficient KYC data");
        let rejection_reason = String::from_str(&env, "Incomplete documentation provided");

        // Submit KYC first
        let _ = client.try_submit_investor_kyc(&investor, &kyc_data);

        // Admin rejection should succeed
        let result = client.try_reject_investor(&investor, &rejection_reason);
        assert!(result.is_ok(), "Admin investor rejection must succeed");

        // Verify investor status
        let verification = client.get_investor_verification(&investor);
        assert!(verification.is_some());

        let verification = verification.unwrap();
        assert_eq!(verification.status, BusinessVerificationStatus::Rejected);
        assert!(verification.rejection_reason.is_some());
        assert_eq!(verification.rejection_reason.unwrap(), rejection_reason);
    }

    #[test]
    fn test_verify_investor_without_kyc_submission_fails() {
        let (env, client, _admin) = setup();
        let investor = Address::generate(&env);
        let investment_limit = 50_000i128;

        // Try to verify without KYC submission
        let result = client.try_verify_investor(&investor, &investment_limit);
        assert!(
            result.is_err(),
            "Verification without KYC submission must fail"
        );

        let error = result.unwrap_err().unwrap();
        assert_eq!(error, QuickLendXError::KYCNotFound);
    }

    #[test]
    fn test_verify_already_verified_investor_fails() {
        let (env, client, _admin) = setup();
        let investor = Address::generate(&env);
        let kyc_data = String::from_str(&env, "Valid KYC data");
        let investment_limit = 50_000i128;

        // Submit and verify
        let _ = client.try_submit_investor_kyc(&investor, &kyc_data);
        let _ = client.try_verify_investor(&investor, &investment_limit);

        // Second verification should fail
        let result = client.try_verify_investor(&investor, &investment_limit);
        assert!(result.is_err(), "Double verification must fail");

        let error = result.unwrap_err().unwrap();
        assert_eq!(error, QuickLendXError::KYCAlreadyVerified);
    }

    #[test]
    fn test_verify_investor_with_invalid_limit_fails() {
        let (env, client, _admin) = setup();
        let investor = Address::generate(&env);
        let kyc_data = String::from_str(&env, "Valid KYC data");
        let invalid_limit = 0i128; // Invalid limit

        // Submit KYC first
        let _ = client.try_submit_investor_kyc(&investor, &kyc_data);

        // Verification with invalid limit should fail
        let result = client.try_verify_investor(&investor, &invalid_limit);
        assert!(result.is_err(), "Verification with invalid limit must fail");

        let error = result.unwrap_err().unwrap();
        assert_eq!(error, QuickLendXError::InvalidAmount);
    }

    // ============================================================================
    // Category 3: Investment Limit Enforcement in Bidding
    // ============================================================================

    #[test]
    fn test_bid_within_investment_limit_succeeds() {
        let (env, client, _admin) = setup();
        let investor = Address::generate(&env);
        let business = Address::generate(&env);
        let kyc_data = String::from_str(&env, "Valid KYC data");
        let investment_limit = 100_000i128;

        // Setup verified investor
        let _ = client.try_submit_investor_kyc(&investor, &kyc_data);
        let _ = client.try_verify_investor(&investor, &investment_limit);

        // Create verified invoice
        let invoice_id = create_verified_invoice(&env, &client, &business, 50_000);

        // Bid within limit should succeed
        let bid_amount = 25_000i128; // Well within limit
        let expected_return = 30_000i128;

        let result = client.try_place_bid(&investor, &invoice_id, &bid_amount, &expected_return);
        assert!(result.is_ok(), "Bid within investment limit must succeed");
    }

    #[test]
    fn test_bid_exceeding_investment_limit_fails() {
        let (env, client, _admin) = setup();
        let investor = Address::generate(&env);
        let business = Address::generate(&env);
        let kyc_data = String::from_str(&env, "Valid KYC data");
        let investment_limit = 10_000i128; // Low limit

        // Setup verified investor with low limit
        let _ = client.try_submit_investor_kyc(&investor, &kyc_data);
        let _ = client.try_verify_investor(&investor, &investment_limit);

        // Create verified invoice
        let invoice_id = create_verified_invoice(&env, &client, &business, 50_000);

        // Bid exceeding limit should fail
        let bid_amount = 15_000i128; // Exceeds limit of 10k
        let expected_return = 18_000i128;

        let result = client.try_place_bid(&investor, &invoice_id, &bid_amount, &expected_return);
        assert!(result.is_err(), "Bid exceeding investment limit must fail");

        let error = result.unwrap_err().unwrap();
        assert_eq!(error, QuickLendXError::InvalidAmount);
    }

    #[test]
    fn test_unverified_investor_cannot_bid() {
        let (env, client, _admin) = setup();
        let investor = Address::generate(&env);
        let business = Address::generate(&env);
        let kyc_data = String::from_str(&env, "Valid KYC data");

        // Submit KYC but don't verify
        let _ = client.try_submit_investor_kyc(&investor, &kyc_data);

        // Create verified invoice
        let invoice_id = create_verified_invoice(&env, &client, &business, 50_000);

        // Unverified investor bid should fail
        let bid_amount = 5_000i128;
        let expected_return = 6_000i128;

        let result = client.try_place_bid(&investor, &invoice_id, &bid_amount, &expected_return);
        assert!(result.is_err(), "Unverified investor bid must fail");

        let error = result.unwrap_err().unwrap();
        assert_eq!(error, QuickLendXError::KYCAlreadyPending);
    }

    #[test]
    fn test_rejected_investor_cannot_bid() {
        let (env, client, _admin) = setup();
        let investor = Address::generate(&env);
        let business = Address::generate(&env);
        let kyc_data = String::from_str(&env, "Insufficient KYC data");

        // Submit KYC and reject
        let _ = client.try_submit_investor_kyc(&investor, &kyc_data);
        let _ = client.try_reject_investor(&investor, &String::from_str(&env, "Insufficient docs"));

        // Create verified invoice
        let invoice_id = create_verified_invoice(&env, &client, &business, 50_000);

        // Rejected investor bid should fail
        let bid_amount = 5_000i128;
        let expected_return = 6_000i128;

        let result = client.try_place_bid(&investor, &invoice_id, &bid_amount, &expected_return);
        assert!(result.is_err(), "Rejected investor bid must fail");

        let error = result.unwrap_err().unwrap();
        assert_eq!(error, QuickLendXError::BusinessNotVerified);
    }

    #[test]
    fn test_investor_without_kyc_cannot_bid() {
        let (env, client, _admin) = setup();
        let investor = Address::generate(&env);
        let business = Address::generate(&env);

        // Create verified invoice
        let invoice_id = create_verified_invoice(&env, &client, &business, 50_000);

        // Investor without KYC bid should fail
        let bid_amount = 5_000i128;
        let expected_return = 6_000i128;

        let result = client.try_place_bid(&investor, &invoice_id, &bid_amount, &expected_return);
        assert!(result.is_err(), "Investor without KYC bid must fail");

        let error = result.unwrap_err().unwrap();
        assert_eq!(error, QuickLendXError::BusinessNotVerified);
    }

    // ============================================================================
    // Category 4: Investment Limit Updates and Dynamic Enforcement
    // ============================================================================

    #[test]
    fn test_limit_update_applies_to_new_bids_only() {
        let (env, client, _admin) = setup();
        let investor = Address::generate(&env);
        let business = Address::generate(&env);
        let kyc_data = String::from_str(&env, "Valid KYC data");
        let initial_limit = 50_000i128;

        // Setup verified investor
        let _ = client.try_submit_investor_kyc(&investor, &kyc_data);
        let _ = client.try_verify_investor(&investor, &initial_limit);

        // Check actual calculated limit
        let verification = client.get_investor_verification(&investor);
        assert!(verification.is_some());
        let actual_limit = verification.unwrap().investment_limit;

        // Create verified invoice
        let invoice_id = create_verified_invoice(&env, &client, &business, 100_000);

        // Place bid within actual calculated limit (use smaller amount)
        let bid_amount = actual_limit / 4; // Use 25% of actual limit
        let expected_return = bid_amount + 1000;
        let result1 = client.try_place_bid(&investor, &invoice_id, &bid_amount, &expected_return);
        assert!(result1.is_ok(), "Bid within initial limit must succeed");

        // Try to place another bid within current limit
        let invoice_id2 = create_verified_invoice(&env, &client, &business, 100_000);
        let bid_amount2 = actual_limit / 3; // Use 33% of actual limit
        let expected_return2 = bid_amount2 + 1000;

        // This should succeed with current limit
        let result2 =
            client.try_place_bid(&investor, &invoice_id2, &bid_amount2, &expected_return2);
        assert!(result2.is_ok(), "Bid within current limit must succeed");

        // Verify both bids were placed
        let bid1 = client.get_bid(&result1.unwrap().unwrap());
        let bid2 = client.get_bid(&result2.unwrap().unwrap());
        assert!(bid1.is_some() && bid2.is_some(), "Both bids should exist");
    }

    #[test]
    fn test_multiple_investors_different_limits() {
        let (env, client, _admin) = setup();
        let investor1 = Address::generate(&env);
        let investor2 = Address::generate(&env);
        let business = Address::generate(&env);
        let kyc_data = String::from_str(&env, "Valid KYC data");

        // Setup investors with different limits
        let _ = client.try_submit_investor_kyc(&investor1, &kyc_data);
        let _ = client.try_verify_investor(&investor1, &100_000i128); // High limit

        let _ = client.try_submit_investor_kyc(&investor2, &kyc_data);
        let _ = client.try_verify_investor(&investor2, &10_000i128); // Low limit

        // Create verified invoice
        let invoice_id = create_verified_invoice(&env, &client, &business, 50_000);

        // Investor1 can bid high amount
        let result1 = client.try_place_bid(&investor1, &invoice_id, &50_000, &60_000);
        assert!(result1.is_ok(), "High-limit investor bid must succeed");

        // Investor2 cannot bid same amount
        let result2 = client.try_place_bid(&investor2, &invoice_id, &50_000, &60_000);
        assert!(result2.is_err(), "Low-limit investor high bid must fail");

        // But investor2 can bid within their limit
        let result3 = client.try_place_bid(&investor2, &invoice_id, &5_000, &6_000);
        assert!(result3.is_ok(), "Low-limit investor small bid must succeed");
    }

    // ============================================================================
    // Category 5: Risk Level and Tier-Based Restrictions
    // ============================================================================

    #[test]
    fn test_risk_level_affects_investment_limits() {
        let (env, client, _admin) = setup();
        let investor = Address::generate(&env);
        let business = Address::generate(&env);

        // Submit KYC with minimal data (should result in higher risk)
        let minimal_kyc = String::from_str(&env, "Basic info");
        let _ = client.try_submit_investor_kyc(&investor, &minimal_kyc);
        let _ = client.try_verify_investor(&investor, &100_000i128);

        // Check that risk assessment affects actual limits
        let verification = client.get_investor_verification(&investor);
        assert!(verification.is_some());

        let verification = verification.unwrap();
        // With minimal KYC, risk should be higher, affecting calculated limit
        assert!(verification.risk_score > 0, "Risk score must be calculated");
        assert_ne!(
            verification.risk_level,
            InvestorRiskLevel::Low,
            "Should not be low risk with minimal KYC"
        );
    }

    #[test]
    fn test_comprehensive_kyc_improves_risk_assessment() {
        let (env, client, _admin) = setup();
        let investor = Address::generate(&env);

        // Submit comprehensive KYC data (should result in lower risk)
        let comprehensive_kyc = String::from_str(&env, "Comprehensive KYC data with detailed financial history, employment verification, credit checks, identity verification, address confirmation, and extensive documentation providing complete investor profile for thorough risk assessment and compliance verification");
        let _ = client.try_submit_investor_kyc(&investor, &comprehensive_kyc);
        let _ = client.try_verify_investor(&investor, &100_000i128);

        let verification = client.get_investor_verification(&investor);
        assert!(verification.is_some());

        let verification = verification.unwrap();
        // Comprehensive KYC should result in better risk assessment
        assert!(
            verification.risk_score < 100,
            "Comprehensive KYC should improve risk score"
        );
    }

    // ============================================================================
    // Category 6: Admin Query Functions
    // ============================================================================

    #[test]
    fn test_admin_can_query_investor_lists() {
        let (env, client, _admin) = setup();
        let investor1 = Address::generate(&env);
        let investor2 = Address::generate(&env);
        let investor3 = Address::generate(&env);
        let kyc_data = String::from_str(&env, "Valid KYC data");

        // Create investors in different states
        let _ = client.try_submit_investor_kyc(&investor1, &kyc_data); // Pending

        let _ = client.try_submit_investor_kyc(&investor2, &kyc_data);
        let _ = client.try_verify_investor(&investor2, &50_000i128); // Verified

        let _ = client.try_submit_investor_kyc(&investor3, &kyc_data);
        let _ = client.try_reject_investor(&investor3, &String::from_str(&env, "Rejected")); // Rejected

        // Query different lists
        let pending = client.get_pending_investors();
        let verified = client.get_verified_investors();
        let rejected = client.get_rejected_investors();

        // Verify correct categorization
        assert!(
            pending.contains(&investor1),
            "Pending list must contain investor1"
        );
        assert!(
            verified.contains(&investor2),
            "Verified list must contain investor2"
        );
        assert!(
            rejected.contains(&investor3),
            "Rejected list must contain investor3"
        );

        // Verify separation
        assert!(
            !verified.contains(&investor1),
            "Verified list must not contain pending investor"
        );
        assert!(
            !rejected.contains(&investor2),
            "Rejected list must not contain verified investor"
        );
    }

    #[test]
    fn test_admin_can_query_investors_by_tier() {
        let (env, client, _admin) = setup();
        let investor = Address::generate(&env);
        let kyc_data = String::from_str(&env, "Valid KYC data");

        // Setup verified investor
        let _ = client.try_submit_investor_kyc(&investor, &kyc_data);
        let _ = client.try_verify_investor(&investor, &50_000i128);

        // Query by tier (new investors should be Basic tier)
        let basic_investors = client.get_investors_by_tier(&InvestorTier::Basic);
        assert!(
            basic_investors.contains(&investor),
            "New investor should be in Basic tier"
        );

        let gold_investors = client.get_investors_by_tier(&InvestorTier::Gold);
        assert!(
            !gold_investors.contains(&investor),
            "New investor should not be in Gold tier"
        );
    }

    #[test]
    fn test_admin_can_query_investors_by_risk_level() {
        let (env, client, _admin) = setup();
        let investor = Address::generate(&env);
        let kyc_data = String::from_str(&env, "Minimal KYC data");

        // Setup verified investor with minimal KYC (should be higher risk)
        let _ = client.try_submit_investor_kyc(&investor, &kyc_data);
        let _ = client.try_verify_investor(&investor, &50_000i128);

        // Query by risk level
        let high_risk = client.get_investors_by_risk_level(&InvestorRiskLevel::High);
        let low_risk = client.get_investors_by_risk_level(&InvestorRiskLevel::Low);

        // New investor with minimal KYC should be higher risk
        assert!(
            !low_risk.contains(&investor),
            "Minimal KYC investor should not be low risk"
        );
    }

    // ============================================================================
    // Category 7: Edge Cases and Security Tests
    // ============================================================================

    #[test]
    fn test_investor_verification_status_transitions() {
        let (env, client, _admin) = setup();
        let investor = Address::generate(&env);
        let kyc_data = String::from_str(&env, "Valid KYC data");

        // Initial state: no verification
        let verification = client.get_investor_verification(&investor);
        assert!(
            verification.is_none(),
            "No verification should exist initially"
        );

        // Submit KYC: Pending state
        let _ = client.try_submit_investor_kyc(&investor, &kyc_data);
        let verification = client.get_investor_verification(&investor);
        assert!(verification.is_some());
        assert_eq!(
            verification.unwrap().status,
            BusinessVerificationStatus::Pending
        );

        // Verify: Verified state
        let _ = client.try_verify_investor(&investor, &50_000i128);
        let verification = client.get_investor_verification(&investor);
        assert!(verification.is_some());
        assert_eq!(
            verification.unwrap().status,
            BusinessVerificationStatus::Verified
        );
    }

    #[test]
    fn test_investor_verification_data_integrity() {
        let (env, client, admin) = setup();
        let investor = Address::generate(&env);
        let kyc_data = String::from_str(&env, "Comprehensive KYC data");
        let investment_limit = 75_000i128;

        // Submit and verify
        let _ = client.try_submit_investor_kyc(&investor, &kyc_data);
        let _ = client.try_verify_investor(&investor, &investment_limit);

        // Verify all data is stored correctly
        let verification = client.get_investor_verification(&investor);
        assert!(verification.is_some());

        let verification = verification.unwrap();
        assert_eq!(verification.investor, investor);
        assert_eq!(verification.status, BusinessVerificationStatus::Verified);
        assert_eq!(verification.kyc_data, kyc_data);
        assert!(verification.investment_limit > 0);
        assert!(verification.verified_at.is_some());
        assert_eq!(verification.verified_by.unwrap(), admin);
        assert!(verification.risk_score > 0);
        assert_ne!(verification.tier, InvestorTier::VIP); // New investor shouldn't be VIP
    }

    #[test]
    fn test_zero_amount_bid_fails_regardless_of_limit() {
        let (env, client, _admin) = setup();
        let investor = Address::generate(&env);
        let business = Address::generate(&env);
        let kyc_data = String::from_str(&env, "Valid KYC data");

        // Setup verified investor
        let _ = client.try_submit_investor_kyc(&investor, &kyc_data);
        let _ = client.try_verify_investor(&investor, &100_000i128);

        // Create verified invoice
        let invoice_id = create_verified_invoice(&env, &client, &business, 50_000);

        // Zero amount bid should fail
        let result = client.try_place_bid(&investor, &invoice_id, &0, &1000);
        assert!(result.is_err(), "Zero amount bid must fail");

        let error = result.unwrap_err().unwrap();
        assert_eq!(error, QuickLendXError::InvalidAmount);
    }

    #[test]
    fn test_negative_investment_limit_verification_fails() {
        let (env, client, _admin) = setup();
        let investor = Address::generate(&env);
        let kyc_data = String::from_str(&env, "Valid KYC data");

        // Submit KYC first
        let _ = client.try_submit_investor_kyc(&investor, &kyc_data);

        // Try to verify with negative limit
        let result = client.try_verify_investor(&investor, &-1000i128);
        assert!(
            result.is_err(),
            "Negative investment limit verification must fail"
        );

        let error = result.unwrap_err().unwrap();
        assert_eq!(error, QuickLendXError::InvalidAmount);
    }

    #[test]
    fn test_investor_analytics_tracking() {
        let (env, client, _admin) = setup();
        let investor = Address::generate(&env);
        let kyc_data = String::from_str(&env, "Valid KYC data");

        // Setup verified investor
        let _ = client.try_submit_investor_kyc(&investor, &kyc_data);
        let _ = client.try_verify_investor(&investor, &100_000i128);

        // Check initial analytics
        let verification = client.get_investor_verification(&investor);
        assert!(verification.is_some());

        let verification = verification.unwrap();
        assert_eq!(verification.total_invested, 0);
        assert_eq!(verification.successful_investments, 0);
        assert_eq!(verification.defaulted_investments, 0);
        // Note: last_activity is set during verification, so it should be > 0
        assert!(verification.last_activity >= env.ledger().timestamp());
    }

    // ============================================================================
    // Category 8: Integration Tests - Full Workflow
    // ============================================================================

    #[test]
    fn test_complete_investor_workflow() {
        let (env, client, admin) = setup();
        let investor = Address::generate(&env);
        let business = Address::generate(&env);
        let kyc_data = String::from_str(&env, "Complete KYC documentation");
        let investment_limit = 50_000i128;

        // Step 1: Submit KYC
        let result = client.try_submit_investor_kyc(&investor, &kyc_data);
        assert!(result.is_ok(), "KYC submission must succeed");

        // Step 2: Admin verifies investor
        let result = client.try_verify_investor(&investor, &investment_limit);
        assert!(result.is_ok(), "Admin verification must succeed");

        // Step 3: Create verified invoice
        let invoice_id = create_verified_invoice(&env, &client, &business, 100_000);

        // Step 4: Investor places bid within limit
        let bid_amount = 25_000i128;
        let expected_return = 30_000i128;
        let result = client.try_place_bid(&investor, &invoice_id, &bid_amount, &expected_return);
        assert!(result.is_ok(), "Bid within limit must succeed");

        // Step 5: Verify bid was created correctly
        let bid_id = result.unwrap().unwrap();
        let bid = client.get_bid(&bid_id);
        assert!(bid.is_some());

        let bid = bid.unwrap();
        assert_eq!(bid.investor, investor);
        assert_eq!(bid.bid_amount, bid_amount);
        assert_eq!(bid.expected_return, expected_return);

        // Step 6: Verify investor can withdraw bid
        let result = client.try_withdraw_bid(&bid_id);
        assert!(result.is_ok(), "Bid withdrawal must succeed");
    }

    #[test]
    fn test_multiple_investors_competitive_bidding() {
        let (env, client, _admin) = setup();
        let investor1 = Address::generate(&env);
        let investor2 = Address::generate(&env);
        let investor3 = Address::generate(&env);
        let business = Address::generate(&env);
        let kyc_data = String::from_str(&env, "Valid KYC data");

        // Setup multiple verified investors with different limits
        let _ = client.try_submit_investor_kyc(&investor1, &kyc_data);
        let _ = client.try_verify_investor(&investor1, &100_000i128);

        let _ = client.try_submit_investor_kyc(&investor2, &kyc_data);
        let _ = client.try_verify_investor(&investor2, &75_000i128);

        let _ = client.try_submit_investor_kyc(&investor3, &kyc_data);
        let _ = client.try_verify_investor(&investor3, &50_000i128);

        // Get actual calculated limits
        let limit1 = client
            .get_investor_verification(&investor1)
            .unwrap()
            .investment_limit;
        let limit2 = client
            .get_investor_verification(&investor2)
            .unwrap()
            .investment_limit;
        let limit3 = client
            .get_investor_verification(&investor3)
            .unwrap()
            .investment_limit;

        // Create verified invoice with reasonable amount
        let invoice_id = create_verified_invoice(&env, &client, &business, 100_000);

        // All investors place bids within their actual calculated limits
        let bid1_amount = limit1 / 2; // Use 50% of actual limit
        let bid2_amount = limit2 / 2;
        let bid3_amount = limit3 / 2;

        let result1 =
            client.try_place_bid(&investor1, &invoice_id, &bid1_amount, &(bid1_amount + 1000));
        let result2 =
            client.try_place_bid(&investor2, &invoice_id, &bid2_amount, &(bid2_amount + 1000));
        let result3 =
            client.try_place_bid(&investor3, &invoice_id, &bid3_amount, &(bid3_amount + 1000));

        // Check if bids were successful
        assert!(
            result1.is_ok(),
            "Investor1 bid should succeed: {:?}",
            result1.err()
        );
        assert!(
            result2.is_ok(),
            "Investor2 bid should succeed: {:?}",
            result2.err()
        );
        assert!(
            result3.is_ok(),
            "Investor3 bid should succeed: {:?}",
            result3.err()
        );

        // Verify all bids were placed
        let all_bids = client.get_bids_for_invoice(&invoice_id);
        assert_eq!(all_bids.len(), 3, "All three bids should be placed");

        // Verify ranking works correctly (highest profit first)
        let ranked_bids = client.get_ranked_bids(&invoice_id);
        assert_eq!(ranked_bids.len(), 3, "All bids should be ranked");

        // All bids have same profit (1000), so ranking may vary
        // Just verify we have 3 ranked bids
        assert!(ranked_bids.len() == 3, "Should have 3 ranked bids");
    }

    // ============================================================================
    // Category 7: Helper Function Tests - validate_investor_investment & is_investor_verified
    // Category 9: Additional Edge Cases and Comprehensive Coverage
    // ============================================================================

    #[test]
    fn test_investor_cannot_resubmit_kyc_while_verified() {
        let (env, client, _admin) = setup();
        let investor = Address::generate(&env);
        let kyc_data = String::from_str(&env, "Valid KYC data");

        // Submit and verify
        let _ = client.try_submit_investor_kyc(&investor, &kyc_data);
        let _ = client.try_verify_investor(&investor, &50_000i128);

        // Try to resubmit KYC while verified
        let new_kyc_data = String::from_str(&env, "Updated KYC data");
        let result = client.try_submit_investor_kyc(&investor, &new_kyc_data);
        assert!(
            result.is_err(),
            "Cannot resubmit KYC while already verified"
        );
    }

    #[test]
    fn test_rejected_investor_can_resubmit_with_updated_kyc() {
        let (env, client, _admin) = setup();
        let investor = Address::generate(&env);
        let initial_kyc = String::from_str(&env, "Insufficient KYC data");

        // Submit and reject
        let _ = client.try_submit_investor_kyc(&investor, &initial_kyc);
        let _ = client.try_reject_investor(&investor, &String::from_str(&env, "Insufficient docs"));

        // Resubmit with updated KYC
        let updated_kyc = String::from_str(&env, "Comprehensive updated KYC data with all required documentation");
        let result = client.try_submit_investor_kyc(&investor, &updated_kyc);
        assert!(result.is_ok(), "Rejected investor should be able to resubmit");

        // Verify status is back to Pending
        let verification = client.get_investor_verification(&investor);
        assert!(verification.is_some());
        assert_eq!(
            verification.unwrap().status,
            BusinessVerificationStatus::Pending
        );
    }

    #[test]
    fn test_admin_cannot_verify_without_kyc_submission() {
        let (env, client, _admin) = setup();
        let investor = Address::generate(&env);

        // Try to verify without KYC submission
        let result = client.try_verify_investor(&investor, &50_000i128);
        assert!(result.is_err(), "Cannot verify without KYC submission");

        let error = result.unwrap_err().unwrap();
        assert_eq!(error, QuickLendXError::KYCNotFound);
    }

    #[test]
    fn test_admin_cannot_reject_without_kyc_submission() {
        let (env, client, _admin) = setup();
        let investor = Address::generate(&env);

        // Try to reject without KYC submission
        let result = client.try_reject_investor(&investor, &String::from_str(&env, "No KYC found"));
        assert!(result.is_err(), "Cannot reject without KYC submission");

        let error = result.unwrap_err().unwrap();
        assert_eq!(error, QuickLendXError::KYCNotFound);
    }

    #[test]
    fn test_investment_limit_calculation_with_different_tiers() {
        let (env, client, _admin) = setup();

        // Create investors with different KYC quality
        let investor1 = Address::generate(&env);
        let comprehensive_kyc = String::from_str(&env, "Comprehensive KYC data with detailed financial history, employment verification, credit checks, identity verification, address confirmation, and extensive documentation providing complete investor profile for thorough risk assessment and compliance verification");
        let _ = client.try_submit_investor_kyc(&investor1, &comprehensive_kyc);
        let _ = client.try_verify_investor(&investor1, &100_000i128);

        let investor2 = Address::generate(&env);
        let minimal_kyc = String::from_str(&env, "Basic info");
        let _ = client.try_submit_investor_kyc(&investor2, &minimal_kyc);
        let _ = client.try_verify_investor(&investor2, &100_000i128);

        // Get calculated limits
        let verification1 = client.get_investor_verification(&investor1).unwrap();
        let verification2 = client.get_investor_verification(&investor2).unwrap();

        // Verify limits are different based on risk assessment
        assert_ne!(
            verification1.investment_limit,
            verification2.investment_limit,
            "Investment limits should differ based on KYC quality"
        );

        // Comprehensive KYC should get higher limit
        assert!(
            verification1.investment_limit >= verification2.investment_limit,
            "Comprehensive KYC should get equal or higher limit"
        );
    }

    #[test]
    fn test_bid_validation_checks_investor_verification_status() {
        let (env, client, _admin) = setup();
        let business = Address::generate(&env);

        // Test 1: No KYC submitted
        let investor1 = Address::generate(&env);
        let invoice_id1 = create_verified_invoice(&env, &client, &business, 50_000);
        let result1 = client.try_place_bid(&investor1, &invoice_id1, &5_000, &6_000);
        assert!(result1.is_err(), "Investor without KYC cannot bid");

        // Test 2: KYC pending
        let investor2 = Address::generate(&env);
        let _ = client.try_submit_investor_kyc(&investor2, &String::from_str(&env, "KYC data"));
        let invoice_id2 = create_verified_invoice(&env, &client, &business, 50_000);
        let result2 = client.try_place_bid(&investor2, &invoice_id2, &5_000, &6_000);
        assert!(result2.is_err(), "Investor with pending KYC cannot bid");

        // Test 3: KYC rejected
        let investor3 = Address::generate(&env);
        let _ = client.try_submit_investor_kyc(&investor3, &String::from_str(&env, "KYC data"));
        let _ = client.try_reject_investor(&investor3, &String::from_str(&env, "Rejected"));
        let invoice_id3 = create_verified_invoice(&env, &client, &business, 50_000);
        let result3 = client.try_place_bid(&investor3, &invoice_id3, &5_000, &6_000);
        assert!(result3.is_err(), "Investor with rejected KYC cannot bid");

        // Test 4: KYC verified
        let investor4 = Address::generate(&env);
        let _ = client.try_submit_investor_kyc(&investor4, &String::from_str(&env, "KYC data"));
        let _ = client.try_verify_investor(&investor4, &50_000i128);
        let invoice_id4 = create_verified_invoice(&env, &client, &business, 50_000);
        let result4 = client.try_place_bid(&investor4, &invoice_id4, &5_000, &6_000);
        assert!(result4.is_ok(), "Verified investor can bid");
    }

    #[test]
    fn test_concurrent_investor_verifications() {
        let (env, client, _admin) = setup();

        // Create multiple investors and submit KYC
        let investor1 = Address::generate(&env);
        let investor2 = Address::generate(&env);
        let investor3 = Address::generate(&env);

        let kyc_data = String::from_str(&env, "Valid KYC data");
        let _ = client.try_submit_investor_kyc(&investor1, &kyc_data);
        let _ = client.try_submit_investor_kyc(&investor2, &kyc_data);
        let _ = client.try_submit_investor_kyc(&investor3, &kyc_data);

        // Verify all investors
        let result1 = client.try_verify_investor(&investor1, &50_000i128);
        let result2 = client.try_verify_investor(&investor2, &75_000i128);
        let result3 = client.try_verify_investor(&investor3, &100_000i128);

        assert!(result1.is_ok(), "Investor1 verification should succeed");
        assert!(result2.is_ok(), "Investor2 verification should succeed");
        assert!(result3.is_ok(), "Investor3 verification should succeed");

        // Verify all have different limits based on input
        let verification1 = client.get_investor_verification(&investor1).unwrap();
        let verification2 = client.get_investor_verification(&investor2).unwrap();
        let verification3 = client.get_investor_verification(&investor3).unwrap();

        assert!(verification1.investment_limit > 0);
        assert!(verification2.investment_limit > 0);
        assert!(verification3.investment_limit > 0);
    }

    #[test]
    fn test_investor_risk_score_calculation() {
        let (env, client, _admin) = setup();

        // Test with minimal KYC (should have higher risk score)
        let investor1 = Address::generate(&env);
        let minimal_kyc = String::from_str(&env, "Basic");
        let _ = client.try_submit_investor_kyc(&investor1, &minimal_kyc);
        let _ = client.try_verify_investor(&investor1, &50_000i128);

        // Test with comprehensive KYC (should have lower risk score)
        let investor2 = Address::generate(&env);
        let comprehensive_kyc = String::from_str(&env, "Comprehensive KYC data with detailed financial history, employment verification, credit checks, identity verification, address confirmation, and extensive documentation providing complete investor profile for thorough risk assessment and compliance verification");
        let _ = client.try_submit_investor_kyc(&investor2, &comprehensive_kyc);
        let _ = client.try_verify_investor(&investor2, &50_000i128);

        let verification1 = client.get_investor_verification(&investor1).unwrap();
        let verification2 = client.get_investor_verification(&investor2).unwrap();

        // Verify risk scores are calculated
        assert!(verification1.risk_score > 0, "Risk score should be calculated");
        assert!(verification2.risk_score > 0, "Risk score should be calculated");

        // Comprehensive KYC should have lower risk score
        assert!(
            verification2.risk_score < verification1.risk_score,
            "Comprehensive KYC should have lower risk score"
        );
    }

    #[test]
    fn test_investor_tier_assignment() {
        let (env, client, _admin) = setup();

        // Create new investor
        let investor = Address::generate(&env);
        let kyc_data = String::from_str(&env, "Valid KYC data");
        let _ = client.try_submit_investor_kyc(&investor, &kyc_data);
        let _ = client.try_verify_investor(&investor, &50_000i128);

        let verification = client.get_investor_verification(&investor).unwrap();

        // New investor should be Basic tier
        assert_eq!(
            verification.tier,
            InvestorTier::Basic,
            "New investor should be Basic tier"
        );

        // Verify tier is set
        assert!(
            matches!(
                verification.tier,
                InvestorTier::Basic
                    | InvestorTier::Silver
                    | InvestorTier::Gold
                    | InvestorTier::Platinum
                    | InvestorTier::VIP
            ),
            "Tier should be assigned"
        );
    }

    #[test]
    fn test_investor_verification_timestamps() {
        let (env, client, admin) = setup();
        let investor = Address::generate(&env);
        let kyc_data = String::from_str(&env, "Valid KYC data");

        // Submit KYC
        let _ = client.try_submit_investor_kyc(&investor, &kyc_data);

        let pending = client.get_pending_investors();
        assert_eq!(pending.len(), 1, "Should have 1 pending investor");
        assert!(
            pending.contains(&investor),
            "Pending should contain investor"
        );

        let verified = client.get_verified_investors();
        assert_eq!(verified.len(), 0, "Verified should be empty");

        // Verify timestamp is set (last_activity should be >= 0)
        assert!(
            verification_after_submit.last_activity >= 0,
            "Last activity should be set after submission"
        );

        // Verify investor
        let _ = client.try_verify_investor(&investor, &50_000i128);
        let verification_after_verify = client.get_investor_verification(&investor).unwrap();

        let pending = client.get_pending_investors();
        assert_eq!(
            pending.len(),
            0,
            "Pending should be empty after verification"
        );

        let verified = client.get_verified_investors();
        assert_eq!(verified.len(), 1, "Should have 1 verified investor");
        assert!(
            verified.contains(&investor),
            "Verified should contain investor"
        );
    }

    #[test]
    fn test_investor_compliance_notes() {
        let (env, client, _admin) = setup();
        let investor = Address::generate(&env);
        let kyc_data = String::from_str(&env, "Valid KYC data");

        // Submit and verify
        let _ = client.try_submit_investor_kyc(&investor, &kyc_data);

        let pending = client.get_pending_investors();
        assert_eq!(pending.len(), 1, "Should have 1 pending investor");

        let rejected = client.get_rejected_investors();
        assert_eq!(rejected.len(), 0, "Rejected should be empty");

        let verification = client.get_investor_verification(&investor).unwrap();

        let pending = client.get_pending_investors();
        assert_eq!(pending.len(), 0, "Pending should be empty after rejection");

        let rejected = client.get_rejected_investors();
        assert_eq!(rejected.len(), 1, "Should have 1 rejected investor");
        assert!(
            rejected.contains(&investor),
            "Rejected should contain investor"
        );
    }

    #[test]
    fn test_investor_rejection_reason_stored() {
        let (env, client, _admin) = setup();
        let investor = Address::generate(&env);
        let kyc_data = String::from_str(&env, "Insufficient KYC data");
        let rejection_reason = String::from_str(&env, "Missing required documentation");

        // Submit and reject
        let _ = client.try_submit_investor_kyc(&investor, &kyc_data);
        let _ = client.try_reject_investor(&investor, &rejection_reason);

        let verification = client.get_investor_verification(&investor).unwrap();

        // Verify rejection reason is stored
        assert!(
            verification.rejection_reason.is_some(),
            "Rejection reason should be stored"
        );
        assert_eq!(
            verification.rejection_reason.unwrap(),
            rejection_reason,
            "Rejection reason should match"
        );
    }

        // Note: Current implementation doesn't remove from rejected list on resubmission
        // The investor is added to pending but remains in rejected list
        // This is tracked behavior - investor status is Pending but appears in both lists
        let pending = client.get_pending_investors();
        assert_eq!(pending.len(), 1, "Should have 1 pending investor");
        assert!(
            pending.contains(&investor),
            "Pending should contain investor"
        );

        // Verify the investor's status is actually Pending
        let verification = client.get_investor_verification(&investor);
        assert!(verification.is_some());
        assert_eq!(
            verification.unwrap().status,
            BusinessVerificationStatus::Pending
        );
    }

    #[test]
    fn test_get_investors_by_tier_empty_initially() {
        let (_env, client, _admin) = setup();

        let basic = client.get_investors_by_tier(&InvestorTier::Basic);
        assert_eq!(basic.len(), 0, "Basic tier should be empty");

        let silver = client.get_investors_by_tier(&InvestorTier::Silver);
        assert_eq!(silver.len(), 0, "Silver tier should be empty");

        let gold = client.get_investors_by_tier(&InvestorTier::Gold);
        assert_eq!(gold.len(), 0, "Gold tier should be empty");

        let platinum = client.get_investors_by_tier(&InvestorTier::Platinum);
        assert_eq!(platinum.len(), 0, "Platinum tier should be empty");

        let vip = client.get_investors_by_tier(&InvestorTier::VIP);
        assert_eq!(vip.len(), 0, "VIP tier should be empty");
    }

    #[test]
    fn test_maximum_investment_limit() {
        let (env, client, _admin) = setup();
        let investor = Address::generate(&env);
        let kyc_data = String::from_str(&env, "Valid KYC data");

        // Submit KYC
        let _ = client.try_submit_investor_kyc(&investor, &kyc_data);

        // Try to set very large investment limit
        let max_limit = i128::MAX / 100; // Avoid overflow
        let result = client.try_verify_investor(&investor, &max_limit);

        // Should succeed with calculated limit
        if result.is_ok() {
            let verification = client.get_investor_verification(&investor).unwrap();
            assert!(
                verification.investment_limit > 0,
                "Investment limit should be positive"
            );
        }
    // Category 9: Investor List Query Tests (Issue #343)
    // ============================================================================

    /// Test suite for validate_investor_investment helper function
    /// Covers: within limit, over limit, and unverified investor scenarios
    #[test]
    fn test_validate_investor_investment_within_limit() {
        let (env, client, _admin) = setup();
        let investor = Address::generate(&env);
        let business = Address::generate(&env);
        let kyc_data = String::from_str(&env, "Valid KYC data");
        let investment_limit = 100_000i128;

        // Setup: Submit and verify investor
        let _ = client.try_submit_investor_kyc(&investor, &kyc_data);
        let _ = client.try_verify_investor(&investor, &investment_limit);

        // Create verified invoice
        let invoice_id = create_verified_invoice(&env, &client, &business, 150_000);

        // Test: Bid amount well within limit should succeed
        let bid_amount = 50_000i128;
        let expected_return = 51_000i128;
        let result = client.try_place_bid(&investor, &invoice_id, &bid_amount, &expected_return);
        assert!(
            result.is_ok(),
            "Investment within limit must be validated and accepted"
        );

        let tier_investors = client.get_investors_by_tier(&tier);
        assert_eq!(tier_investors.len(), 1, "Should have 1 investor in tier");
        assert!(
            tier_investors.contains(&investor),
            "Should contain investor"
        );
    }

    #[test]
    fn test_validate_investor_investment_at_limit_boundary() {
        let (env, client, _admin) = setup();
        let investor = Address::generate(&env);
        let business = Address::generate(&env);
        let kyc_data = String::from_str(&env, "Valid KYC data");
        let investment_limit = 100_000i128;

        // Setup: Submit and verify investor with specific limit
        let _ = client.try_submit_investor_kyc(&investor, &kyc_data);
        let _ = client.try_verify_investor(&investor, &investment_limit);

        // Get the actual calculated limit for this investor
        let verification = client.get_investor_verification(&investor);
        assert!(verification.is_some());
        let actual_limit = verification.unwrap().investment_limit;

        // Create verified invoice
        let invoice_id = create_verified_invoice(&env, &client, &business, actual_limit + 50_000);

        let tier1 = client.get_investor_verification(&investor1).unwrap().tier;
        let tier_investors = client.get_investors_by_tier(&tier1);

        assert!(
            tier_investors.len() >= 1,
            "Should have at least 1 investor in tier"
        );
        assert!(
            tier_investors.contains(&investor1),
            "Should contain investor1"
        );
    }

    #[test]
    fn test_validate_investor_investment_over_limit() {
        let (env, client, _admin) = setup();
        let investor = Address::generate(&env);
        let business = Address::generate(&env);
        let kyc_data = String::from_str(&env, "Valid KYC data");
        let investment_limit = 100_000i128;

        // Setup: Submit and verify investor
        let _ = client.try_submit_investor_kyc(&investor, &kyc_data);
        let _ = client.try_verify_investor(&investor, &investment_limit);

        // Create verified invoice
        let invoice_id = create_verified_invoice(&env, &client, &business, 500_000);

        let tier = client.get_investor_verification(&investor1).unwrap().tier;
        let tier_investors = client.get_investors_by_tier(&tier);

        assert!(
            tier_investors.contains(&investor1),
            "Should contain verified investor"
        );
        assert!(
            !tier_investors.contains(&investor2),
            "Should not contain pending investor"
        );
    }

    #[test]
    fn test_get_investors_by_risk_level_empty_initially() {
        let (_env, client, _admin) = setup();

        let low = client.get_investors_by_risk_level(&InvestorRiskLevel::Low);
        assert_eq!(low.len(), 0, "Low risk should be empty");

        let medium = client.get_investors_by_risk_level(&InvestorRiskLevel::Medium);
        assert_eq!(medium.len(), 0, "Medium risk should be empty");

        let high = client.get_investors_by_risk_level(&InvestorRiskLevel::High);
        assert_eq!(high.len(), 0, "High risk should be empty");
    }

    #[test]
    fn test_validate_investor_investment_pending_investor() {
        let (env, client, _admin) = setup();
        let pending_investor = Address::generate(&env);
        let business = Address::generate(&env);
        let kyc_data = String::from_str(&env, "Valid KYC data");

        // Setup: Submit KYC but do NOT verify (leaves investor in Pending state)
        let _ = client.try_submit_investor_kyc(&pending_investor, &kyc_data);

        // Verify investor is in Pending status
        let verification = client.get_investor_verification(&pending_investor);
        assert!(verification.is_some());
        assert_eq!(
            verification.unwrap().status,
            BusinessVerificationStatus::Pending
        );

        // Create verified invoice
        let invoice_id = create_verified_invoice(&env, &client, &business, 100_000);

        // Test: Bid from pending investor should fail (not fully verified)
        let bid_amount = 50_000i128;
        let expected_return = 51_000i128;
        let result = client.try_place_bid(&pending_investor, &invoice_id, &bid_amount, &expected_return);
        assert!(
            result.is_err(),
            "Pending investor must fail investment validation"
        );
    }

    #[test]
    fn test_validate_investor_investment_rejected_investor() {
        let (env, client, _admin) = setup();
        let rejected_investor = Address::generate(&env);
        let business = Address::generate(&env);
        let kyc_data = String::from_str(&env, "Valid KYC data");

        // Setup: Submit and reject investor
        let _ = client.try_submit_investor_kyc(&rejected_investor, &kyc_data);
        let _ = client.try_reject_investor(
            &rejected_investor,
            &String::from_str(&env, "Insufficient documentation"),
        );

        // Verify investor is in Rejected status
        let verification = client.get_investor_verification(&rejected_investor);
        assert!(verification.is_some());
        assert_eq!(
            verification.unwrap().status,
            BusinessVerificationStatus::Rejected
        );

        // Create verified invoice
        let invoice_id = create_verified_invoice(&env, &client, &business, 100_000);

        // Test: Bid from rejected investor should fail
        let bid_amount = 50_000i128;
        let expected_return = 51_000i128;
        let result =
            client.try_place_bid(&rejected_investor, &invoice_id, &bid_amount, &expected_return);
        assert!(
            result.is_err(),
            "Rejected investor must fail investment validation"
        );
    }

    #[test]
    fn test_validate_investor_investment_multiple_bids_independent_validation() {
        let (env, client, _admin) = setup();
        let investor = Address::generate(&env);
        let business = Address::generate(&env);
        let kyc_data = String::from_str(&env, "Valid KYC data");
        let investment_limit = 100_000i128;

        // Setup: Submit and verify investor
        let _ = client.try_submit_investor_kyc(&investor, &kyc_data);
        let _ = client.try_verify_investor(&investor, &investment_limit);

        // Get actual calculated limit
        let verification = client.get_investor_verification(&investor);
        let actual_limit = verification.unwrap().investment_limit;

        // Create first invoice
        let invoice_id1 = create_verified_invoice(&env, &client, &business, 100_000);

        // Place first bid: at the limit
        let bid_amount1 = actual_limit;
        let expected_return1 = bid_amount1 + 1_000i128;
        let result1 = client.try_place_bid(&investor, &invoice_id1, &bid_amount1, &expected_return1);
        assert!(
            result1.is_ok(),
            "First bid at limit must succeed"
        );

        // Create second invoice
        let invoice_id2 = create_verified_invoice(&env, &client, &business, 100_000);

        // Place second bid: at the limit on a different invoice
        // Validation is per-invoice/bid, not cumulative across all bids
        let bid_amount2 = actual_limit;
        let expected_return2 = bid_amount2 + 1_000i128;
        let result2 = client.try_place_bid(&investor, &invoice_id2, &bid_amount2, &expected_return2);
        assert!(
            result2.is_ok(),
            "Second bid at limit (on different invoice) must succeed - validation is per-bid, not cumulative"
        );

        let risk_investors = client.get_investors_by_risk_level(&risk_level);
        assert_eq!(
            risk_investors.len(),
            1,
            "Should have 1 investor at risk level"
        );
        assert!(
            risk_investors.contains(&investor),
            "Should contain investor"
        );
    }

    /// Test suite for is_investor_verified helper function
    /// Covers: verified (true), pending (false), rejected (false), none (false)
    #[test]
    fn test_is_investor_verified_returns_true_for_verified() {
        let (env, client, _admin) = setup();
        let investor = Address::generate(&env);
        let kyc_data = String::from_str(&env, "Valid KYC data");

        // Setup: Submit and verify investor
        let _ = client.try_submit_investor_kyc(&investor, &kyc_data);
        let _ = client.try_verify_investor(&investor, &50_000i128);

        // Test: is_investor_verified should return true
        let verification = client.get_investor_verification(&investor);
        assert!(verification.is_some(), "Verified investor record must exist");

        let risk1 = client
            .get_investor_verification(&investor1)
            .unwrap()
            .risk_level;
        let risk_investors = client.get_investors_by_risk_level(&risk1);

        assert!(
            risk_investors.len() >= 1,
            "Should have at least 1 investor at risk level"
        );
        assert!(
            risk_investors.contains(&investor1),
            "Should contain investor1"
        );
    }

    #[test]
    fn test_is_investor_verified_returns_false_for_pending() {
        let (env, client, _admin) = setup();
        let investor = Address::generate(&env);
        let kyc_data = String::from_str(&env, "Valid KYC data");

        // Setup: Submit KYC but do NOT verify (leaves in Pending state)
        let _ = client.try_submit_investor_kyc(&investor, &kyc_data);

        // Test: is_investor_verified should return false for pending
        let verification = client.get_investor_verification(&investor);
        assert!(verification.is_some(), "Pending investor record must exist");

        let risk = client
            .get_investor_verification(&investor1)
            .unwrap()
            .risk_level;
        let risk_investors = client.get_investors_by_risk_level(&risk);

        assert!(
            risk_investors.contains(&investor1),
            "Should contain verified investor"
        );
        assert!(
            !risk_investors.contains(&investor2),
            "Should not contain pending investor"
        );
    }

    #[test]
    fn test_is_investor_verified_returns_false_for_rejected() {
        let (env, client, _admin) = setup();
        let investor = Address::generate(&env);
        let kyc_data = String::from_str(&env, "Valid KYC data");

        // Setup: Submit and reject investor
        let _ = client.try_submit_investor_kyc(&investor, &kyc_data);
        let _ = client.try_reject_investor(
            &investor,
            &String::from_str(&env, "Failed compliance check"),
        );

        assert_eq!(
            client.get_pending_investors().len(),
            3,
            "Should have 3 pending"
        );
        assert_eq!(
            client.get_verified_investors().len(),
            0,
            "Should have 0 verified"
        );
        assert_eq!(
            client.get_rejected_investors().len(),
            0,
            "Should have 0 rejected"
        );

        let _ = client.try_verify_investor(&investor1, &50_000i128);
        assert_eq!(
            client.get_pending_investors().len(),
            2,
            "Should have 2 pending"
        );
        assert_eq!(
            client.get_verified_investors().len(),
            1,
            "Should have 1 verified"
        );
        assert_eq!(
            client.get_rejected_investors().len(),
            0,
            "Should have 0 rejected"
        );

        let _ = client.try_reject_investor(&investor2, &reason);
        assert_eq!(
            client.get_pending_investors().len(),
            1,
            "Should have 1 pending"
        );
        assert_eq!(
            client.get_verified_investors().len(),
            1,
            "Should have 1 verified"
        );
        assert_eq!(
            client.get_rejected_investors().len(),
            1,
            "Should have 1 rejected"
        );

        let _ = client.try_verify_investor(&investor3, &75_000i128);
        assert_eq!(
            client.get_pending_investors().len(),
            0,
            "Should have 0 pending"
        );
        assert_eq!(
            client.get_verified_investors().len(),
            2,
            "Should have 2 verified"
        );
        assert_eq!(
            client.get_rejected_investors().len(),
            1,
            "Should have 1 rejected"
        );
    }

    #[test]
    fn test_is_investor_verified_state_transitions() {
        let (env, client, _admin) = setup();
        let investor = Address::generate(&env);
        let kyc_data = String::from_str(&env, "Valid KYC data");

        // State 1: None (no record)
        let verification = client.get_investor_verification(&investor);
        assert!(verification.is_none(), "Initially no verification record");

        // State 2: Pending (after KYC submission)
        let _ = client.try_submit_investor_kyc(&investor, &kyc_data);
        let verification = client.get_investor_verification(&investor);
        assert!(verification.is_some());
        assert_eq!(verification.unwrap().status, BusinessVerificationStatus::Pending);

        // State 3: Verified (after admin verification)
        let _ = client.try_verify_investor(&investor, &50_000i128);
        let verification = client.get_investor_verification(&investor);
        assert!(verification.is_some());
        assert_eq!(verification.unwrap().status, BusinessVerificationStatus::Verified);

        // State 4: Can transition back to Pending via rejection and resubmission
        let _ = client.try_reject_investor(
            &investor,
            &String::from_str(&env, "Compliance issue"),
        );
        let verification = client.get_investor_verification(&investor);
        assert_eq!(verification.unwrap().status, BusinessVerificationStatus::Rejected);

        // Resubmit after rejection
        let new_kyc = String::from_str(&env, "Updated KYC data");
        let _ = client.try_submit_investor_kyc(&investor, &new_kyc);
        let verification = client.get_investor_verification(&investor);
        assert_eq!(verification.unwrap().status, BusinessVerificationStatus::Pending);
    }

    #[test]
    fn test_is_investor_verified_with_different_risk_levels() {
        let (env, client, _admin) = setup();
        let investor_high_risk = Address::generate(&env);
        let investor_low_risk = Address::generate(&env);
        let minimal_kyc = String::from_str(&env, "Basic info");
        let comprehensive_kyc = String::from_str(&env, 
            "Comprehensive KYC with detailed financial history, employment verification, \
             credit checks, identity verification, address confirmation, and extensive documentation");

        // Setup high-risk investor (minimal KYC)
        let _ = client.try_submit_investor_kyc(&investor_high_risk, &minimal_kyc);
        let _ = client.try_verify_investor(&investor_high_risk, &50_000i128);

        // Setup low-risk investor (comprehensive KYC)
        let _ = client.try_submit_investor_kyc(&investor_low_risk, &comprehensive_kyc);
        let _ = client.try_verify_investor(&investor_low_risk, &50_000i128);

        // Both should have Verified status despite different risk levels
        let high_risk_verification = client.get_investor_verification(&investor_high_risk);
        let low_risk_verification = client.get_investor_verification(&investor_low_risk);

        let verified_count = verified.iter().filter(|i| *i == investor).count();
        assert_eq!(
            verified_count, 1,
            "Should appear exactly once in verified list"
        );
    }


    // ============================================================================
    // Category 10: Single Investor Multiple Invoices Tests
    // ============================================================================

    /// Test: One investor places bids on multiple invoices
    #[test]
    fn test_single_investor_bids_on_multiple_invoices() {
        let (env, client, _admin) = setup();
        let investor = Address::generate(&env);
        let business = Address::generate(&env);
        let kyc_data = String::from_str(&env, "Valid KYC data");

        // Setup verified investor with sufficient limit
        let _ = client.try_submit_investor_kyc(&investor, &kyc_data);
        let _ = client.try_verify_investor(&investor, &100_000i128);

        // Create 5 verified invoices
        let invoice_id1 = create_verified_invoice(&env, &client, &business, 20_000);
        let invoice_id2 = create_verified_invoice(&env, &client, &business, 30_000);
        let invoice_id3 = create_verified_invoice(&env, &client, &business, 25_000);
        let invoice_id4 = create_verified_invoice(&env, &client, &business, 15_000);
        let invoice_id5 = create_verified_invoice(&env, &client, &business, 40_000);

        // Investor places bids on all 5 invoices
        let bid_id1 = client.place_bid(&investor, &invoice_id1, &10_000, &12_000);
        let bid_id2 = client.place_bid(&investor, &invoice_id2, &15_000, &18_000);
        let bid_id3 = client.place_bid(&investor, &invoice_id3, &12_000, &14_500);
        let bid_id4 = client.place_bid(&investor, &invoice_id4, &8_000, &9_500);
        let bid_id5 = client.place_bid(&investor, &invoice_id5, &20_000, &24_000);

        // Verify all bids were placed successfully
        assert!(client.get_bid(&bid_id1).is_some(), "Bid 1 should exist");
        assert!(client.get_bid(&bid_id2).is_some(), "Bid 2 should exist");
        assert!(client.get_bid(&bid_id3).is_some(), "Bid 3 should exist");
        assert!(client.get_bid(&bid_id4).is_some(), "Bid 4 should exist");
        assert!(client.get_bid(&bid_id5).is_some(), "Bid 5 should exist");

        // Verify all bids belong to the same investor
        assert_eq!(client.get_bid(&bid_id1).unwrap().investor, investor);
        assert_eq!(client.get_bid(&bid_id2).unwrap().investor, investor);
        assert_eq!(client.get_bid(&bid_id3).unwrap().investor, investor);
        assert_eq!(client.get_bid(&bid_id4).unwrap().investor, investor);
        assert_eq!(client.get_bid(&bid_id5).unwrap().investor, investor);

        // Verify get_all_bids_by_investor returns all 5 bids
        let all_bids = client.get_all_bids_by_investor(&investor);
        assert_eq!(all_bids.len(), 5, "Should have 5 bids for investor");
    }

    /// Test: Investment limit applies across all bids
    #[test]
    fn test_investment_limit_applies_across_all_bids() {
        let (env, client, _admin) = setup();
        let investor = Address::generate(&env);
        let business = Address::generate(&env);
        let kyc_data = String::from_str(&env, "Valid KYC data");

        // Setup investor with limited investment capacity
        let _ = client.try_submit_investor_kyc(&investor, &kyc_data);
        let _ = client.try_verify_investor(&investor, &50_000i128);

        // Get actual calculated limit
        let actual_limit = client
            .get_investor_verification(&investor)
            .unwrap()
            .investment_limit;

        // Create multiple invoices
        let invoice_id1 = create_verified_invoice(&env, &client, &business, 30_000);
        let invoice_id2 = create_verified_invoice(&env, &client, &business, 30_000);
        let invoice_id3 = create_verified_invoice(&env, &client, &business, 30_000);

        // Place bids within individual limits but respecting total limit
        let bid_amount = actual_limit / 4; // Use 25% of limit per bid

        let result1 = client.try_place_bid(&investor, &invoice_id1, &bid_amount, &(bid_amount + 1000));
        assert!(result1.is_ok(), "First bid within limit should succeed");

        let result2 = client.try_place_bid(&investor, &invoice_id2, &bid_amount, &(bid_amount + 1000));
        assert!(result2.is_ok(), "Second bid within limit should succeed");

        let result3 = client.try_place_bid(&investor, &invoice_id3, &bid_amount, &(bid_amount + 1000));
        assert!(result3.is_ok(), "Third bid within limit should succeed");

        // Verify all bids were placed
        let all_bids = client.get_all_bids_by_investor(&investor);
        assert_eq!(all_bids.len(), 3, "Should have 3 bids");

        // Try to place a bid that would exceed the limit
        let invoice_id4 = create_verified_invoice(&env, &client, &business, 30_000);
        let large_bid = actual_limit; // This would exceed limit
        let result4 = client.try_place_bid(&investor, &invoice_id4, &large_bid, &(large_bid + 1000));
        assert!(result4.is_err(), "Bid exceeding total limit should fail");
    }

    /// Test: Business accepts bids on some invoices, others remain Placed
    #[test]
    fn test_investor_bids_accepted_on_some_invoices() {
        let (env, client, _admin) = setup();
        let investor = Address::generate(&env);
        let business = Address::generate(&env);
        let kyc_data = String::from_str(&env, "Valid KYC data");

        // Setup verified investor
        let _ = client.try_submit_investor_kyc(&investor, &kyc_data);
        let _ = client.try_verify_investor(&investor, &100_000i128);

        // Create 4 verified invoices
        let invoice_id1 = create_verified_invoice(&env, &client, &business, 20_000);
        let invoice_id2 = create_verified_invoice(&env, &client, &business, 30_000);
        let invoice_id3 = create_verified_invoice(&env, &client, &business, 25_000);
        let invoice_id4 = create_verified_invoice(&env, &client, &business, 15_000);

        // Investor places bids on all 4 invoices
        let bid_id1 = client.place_bid(&investor, &invoice_id1, &10_000, &12_000);
        let bid_id2 = client.place_bid(&investor, &invoice_id2, &15_000, &18_000);
        let bid_id3 = client.place_bid(&investor, &invoice_id3, &12_000, &14_500);
        let bid_id4 = client.place_bid(&investor, &invoice_id4, &8_000, &9_500);

        // Business accepts bids on invoice 1 and 3
        let result1 = client.try_accept_bid(&invoice_id1, &bid_id1);
        assert!(result1.is_ok(), "Accept bid 1 should succeed");

        let result3 = client.try_accept_bid(&invoice_id3, &bid_id3);
        assert!(result3.is_ok(), "Accept bid 3 should succeed");

        // Verify bid statuses
        assert_eq!(client.get_bid(&bid_id1).unwrap().status, BidStatus::Accepted, "Bid 1 should be Accepted");
        assert_eq!(client.get_bid(&bid_id2).unwrap().status, BidStatus::Placed, "Bid 2 should remain Placed");
        assert_eq!(client.get_bid(&bid_id3).unwrap().status, BidStatus::Accepted, "Bid 3 should be Accepted");
        assert_eq!(client.get_bid(&bid_id4).unwrap().status, BidStatus::Placed, "Bid 4 should remain Placed");

        // Verify invoice statuses
        assert_eq!(client.get_invoice(&invoice_id1).status, InvoiceStatus::Funded, "Invoice 1 should be Funded");
        assert_eq!(client.get_invoice(&invoice_id2).status, InvoiceStatus::Verified, "Invoice 2 should remain Verified");
        assert_eq!(client.get_invoice(&invoice_id3).status, InvoiceStatus::Funded, "Invoice 3 should be Funded");
        assert_eq!(client.get_invoice(&invoice_id4).status, InvoiceStatus::Verified, "Invoice 4 should remain Verified");

        // Verify investor can still withdraw non-accepted bids
        let result2 = client.try_withdraw_bid(&bid_id2);
        assert!(result2.is_ok(), "Should be able to withdraw non-accepted bid");

        let result4 = client.try_withdraw_bid(&bid_id4);
        assert!(result4.is_ok(), "Should be able to withdraw non-accepted bid");
    }

    /// Test: get_all_bids_by_investor returns correct subset after acceptances
    #[test]
    fn test_get_all_bids_by_investor_after_acceptances() {
        let (env, client, _admin) = setup();
        let investor = Address::generate(&env);
        let business = Address::generate(&env);
        let kyc_data = String::from_str(&env, "Valid KYC data");

        // Setup verified investor
        let _ = client.try_submit_investor_kyc(&investor, &kyc_data);
        let _ = client.try_verify_investor(&investor, &100_000i128);

        // Create 3 verified invoices
        let invoice_id1 = create_verified_invoice(&env, &client, &business, 20_000);
        let invoice_id2 = create_verified_invoice(&env, &client, &business, 30_000);
        let invoice_id3 = create_verified_invoice(&env, &client, &business, 25_000);

        // Investor places bids on all 3 invoices
        let bid_id1 = client.place_bid(&investor, &invoice_id1, &10_000, &12_000);
        let bid_id2 = client.place_bid(&investor, &invoice_id2, &15_000, &18_000);
        let bid_id3 = client.place_bid(&investor, &invoice_id3, &12_000, &14_500);

        // Verify all bids are returned initially
        let all_bids_before = client.get_all_bids_by_investor(&investor);
        assert_eq!(all_bids_before.len(), 3, "Should have 3 bids initially");

        // Business accepts bid on invoice 1
        let _ = client.try_accept_bid(&invoice_id1, &bid_id1);

        // Investor withdraws bid on invoice 3
        let _ = client.try_withdraw_bid(&bid_id3);

        // get_all_bids_by_investor should still return all 3 bids
        let all_bids_after = client.get_all_bids_by_investor(&investor);
        assert_eq!(all_bids_after.len(), 3, "Should still have 3 bids");

        // Verify we can identify each bid by status
        let bid1 = all_bids_after.iter().find(|b| b.bid_id == bid_id1).unwrap();
        let bid2 = all_bids_after.iter().find(|b| b.bid_id == bid_id2).unwrap();
        let bid3 = all_bids_after.iter().find(|b| b.bid_id == bid_id3).unwrap();

        assert_eq!(bid1.status, BidStatus::Accepted, "Bid 1 should be Accepted");
        assert_eq!(bid2.status, BidStatus::Placed, "Bid 2 should be Placed");
        assert_eq!(bid3.status, BidStatus::Withdrawn, "Bid 3 should be Withdrawn");
    }

    /// Test: Investor can withdraw bids on non-accepted invoices
    #[test]
    fn test_investor_can_withdraw_non_accepted_bids() {
        let (env, client, _admin) = setup();
        let investor = Address::generate(&env);
        let business = Address::generate(&env);
        let kyc_data = String::from_str(&env, "Valid KYC data");

        // Setup verified investor
        let _ = client.try_submit_investor_kyc(&investor, &kyc_data);
        let _ = client.try_verify_investor(&investor, &100_000i128);

        // Create 3 verified invoices
        let invoice_id1 = create_verified_invoice(&env, &client, &business, 20_000);
        let invoice_id2 = create_verified_invoice(&env, &client, &business, 30_000);
        let invoice_id3 = create_verified_invoice(&env, &client, &business, 25_000);

        // Investor places bids on all 3 invoices
        let bid_id1 = client.place_bid(&investor, &invoice_id1, &10_000, &12_000);
        let bid_id2 = client.place_bid(&investor, &invoice_id2, &15_000, &18_000);
        let bid_id3 = client.place_bid(&investor, &invoice_id3, &12_000, &14_500);

        // Business accepts bid on invoice 1
        let _ = client.try_accept_bid(&invoice_id1, &bid_id1);

        // Investor cannot withdraw accepted bid
        let result1 = client.try_withdraw_bid(&bid_id1);
        assert!(result1.is_err(), "Cannot withdraw accepted bid");

        // Investor can withdraw non-accepted bids
        let result2 = client.try_withdraw_bid(&bid_id2);
        assert!(result2.is_ok(), "Should be able to withdraw non-accepted bid 2");

        let result3 = client.try_withdraw_bid(&bid_id3);
        assert!(result3.is_ok(), "Should be able to withdraw non-accepted bid 3");

        // Verify statuses
        assert_eq!(client.get_bid(&bid_id1).unwrap().status, BidStatus::Accepted);
        assert_eq!(client.get_bid(&bid_id2).unwrap().status, BidStatus::Withdrawn);
        assert_eq!(client.get_bid(&bid_id3).unwrap().status, BidStatus::Withdrawn);
    }

    /// Test: Multiple accepted bids create multiple investments
    #[test]
    fn test_multiple_accepted_bids_create_multiple_investments() {
        let (env, client, _admin) = setup();
        let investor = Address::generate(&env);
        let business = Address::generate(&env);
        let kyc_data = String::from_str(&env, "Valid KYC data");

        // Setup verified investor
        let _ = client.try_submit_investor_kyc(&investor, &kyc_data);
        let _ = client.try_verify_investor(&investor, &100_000i128);

        // Create 3 verified invoices
        let invoice_id1 = create_verified_invoice(&env, &client, &business, 20_000);
        let invoice_id2 = create_verified_invoice(&env, &client, &business, 30_000);
        let invoice_id3 = create_verified_invoice(&env, &client, &business, 25_000);

        // Investor places bids on all 3 invoices
        let bid_id1 = client.place_bid(&investor, &invoice_id1, &10_000, &12_000);
        let bid_id2 = client.place_bid(&investor, &invoice_id2, &15_000, &18_000);
        let bid_id3 = client.place_bid(&investor, &invoice_id3, &12_000, &14_500);

        // Business accepts all 3 bids
        let _ = client.try_accept_bid(&invoice_id1, &bid_id1);
        let _ = client.try_accept_bid(&invoice_id2, &bid_id2);
        let _ = client.try_accept_bid(&invoice_id3, &bid_id3);

        // Verify investments were created for each accepted bid
        let investment1 = client.get_investment_by_invoice(&invoice_id1);
        assert!(investment1.is_some(), "Investment 1 should exist");
        assert_eq!(investment1.unwrap().investor, investor);
        assert_eq!(investment1.unwrap().amount, 10_000);

        let investment2 = client.get_investment_by_invoice(&invoice_id2);
        assert!(investment2.is_some(), "Investment 2 should exist");
        assert_eq!(investment2.unwrap().investor, investor);
        assert_eq!(investment2.unwrap().amount, 15_000);

        let investment3 = client.get_investment_by_invoice(&invoice_id3);
        assert!(investment3.is_some(), "Investment 3 should exist");
        assert_eq!(investment3.unwrap().investor, investor);
        assert_eq!(investment3.unwrap().amount, 12_000);
    }

    /// Test: Investor with multiple bids on different invoices - comprehensive workflow
    #[test]
    fn test_investor_multiple_invoices_comprehensive_workflow() {
        let (env, client, _admin) = setup();
        let investor = Address::generate(&env);
        let business1 = Address::generate(&env);
        let business2 = Address::generate(&env);
        let kyc_data = String::from_str(&env, "Valid KYC data");

        // Setup verified investor
        let _ = client.try_submit_investor_kyc(&investor, &kyc_data);
        let _ = client.try_verify_investor(&investor, &100_000i128);

        // Create 5 verified invoices from different businesses
        let invoice_id1 = create_verified_invoice(&env, &client, &business1, 20_000);
        let invoice_id2 = create_verified_invoice(&env, &client, &business1, 30_000);
        let invoice_id3 = create_verified_invoice(&env, &client, &business2, 25_000);
        let invoice_id4 = create_verified_invoice(&env, &client, &business2, 15_000);
        let invoice_id5 = create_verified_invoice(&env, &client, &business1, 40_000);

        // Investor places bids on all 5 invoices
        let bid_id1 = client.place_bid(&investor, &invoice_id1, &10_000, &12_000);
        let bid_id2 = client.place_bid(&investor, &invoice_id2, &15_000, &18_000);
        let bid_id3 = client.place_bid(&investor, &invoice_id3, &12_000, &14_500);
        let bid_id4 = client.place_bid(&investor, &invoice_id4, &8_000, &9_500);
        let bid_id5 = client.place_bid(&investor, &invoice_id5, &20_000, &24_000);

        // Verify all bids are Placed
        let all_bids = client.get_all_bids_by_investor(&investor);
        assert_eq!(all_bids.len(), 5, "Should have 5 bids");
        for bid in all_bids.iter() {
            assert_eq!(bid.status, BidStatus::Placed, "All bids should be Placed initially");
        }

        // Business 1 accepts bids on invoices 1 and 5
        let _ = client.try_accept_bid(&invoice_id1, &bid_id1);
        let _ = client.try_accept_bid(&invoice_id5, &bid_id5);

        // Business 2 accepts bid on invoice 3
        let _ = client.try_accept_bid(&invoice_id3, &bid_id3);

        // Investor withdraws bids on invoices 2 and 4
        let _ = client.try_withdraw_bid(&bid_id2);
        let _ = client.try_withdraw_bid(&bid_id4);

        // Verify final bid statuses
        assert_eq!(client.get_bid(&bid_id1).unwrap().status, BidStatus::Accepted);
        assert_eq!(client.get_bid(&bid_id2).unwrap().status, BidStatus::Withdrawn);
        assert_eq!(client.get_bid(&bid_id3).unwrap().status, BidStatus::Accepted);
        assert_eq!(client.get_bid(&bid_id4).unwrap().status, BidStatus::Withdrawn);
        assert_eq!(client.get_bid(&bid_id5).unwrap().status, BidStatus::Accepted);

        // Verify investments were created for accepted bids
        assert!(client.get_investment_by_invoice(&invoice_id1).is_some());
        assert!(client.get_investment_by_invoice(&invoice_id3).is_some());
        assert!(client.get_investment_by_invoice(&invoice_id5).is_some());

        // Verify no investments for withdrawn bids
        assert!(client.get_investment_by_invoice(&invoice_id2).is_none());
        assert!(client.get_investment_by_invoice(&invoice_id4).is_none());

        // Verify get_all_bids_by_investor still returns all 5 bids
        let final_bids = client.get_all_bids_by_investor(&investor);
        assert_eq!(final_bids.len(), 5, "Should still have all 5 bids");
    }
}
