/// Comprehensive KYC Lifecycle Enforcement Tests for Issue #832
///
/// This test suite ensures KYC status properly gates all downstream operations
/// and that index queries remain consistent across Pending/Verified/Rejected
/// state transitions for both businesses and investors.
///
/// Test Coverage:
/// 1. Business Operations: invoice upload, cancellation, bid acceptance
/// 2. Investor Operations: bid placement, bid withdrawal
/// 3. Index Query Consistency: verification lists during state transitions
/// 4. State Transition Validation: proper error responses for each KYC state
///
/// Security Requirements:
/// - Unverified entities cannot perform privileged operations
/// - Pending entities get distinct KYCAlreadyPending errors
/// - Rejected entities get BusinessNotVerified errors
/// - Verified entities can perform all operations
/// - Index queries reflect current KYC state accurately

#[cfg(test)]
extern crate alloc;

mod test_kyc_lifecycle_enforcement {
    use crate::errors::QuickLendXError;
    use crate::invoice::InvoiceCategory;
    use crate::{QuickLendXContract, QuickLendXContractClient};
    use soroban_sdk::{
        testutils::Address as _,
        Address, Env, String, Vec,
    };

    // ============================================================================
    // SETUP HELPERS
    // ============================================================================

    fn setup() -> (Env, QuickLendXContractClient<'static>, Address) {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(QuickLendXContract, ());
        let client = QuickLendXContractClient::new(&env, &contract_id);

        // Set up admin
        let admin = Address::generate(&env);
        client.set_admin(&admin);

        // Initialize protocol limits
        client.initialize_protocol_limits(&admin, &1_000_000i128, &365u64, &86400u64);

        (env, client, admin)
    }

    fn create_test_kyc_data(env: &Env, name: &str) -> String {
        let kyc_json = alloc::format!(
            "{{\"name\":\"{}\",\"tax_id\":\"123456789\",\"address\":\"123 Test St\"}}",
            name
        );
        String::from_str(env, &kyc_json)
    }

    fn create_verified_business(
        env: &Env,
        client: &QuickLendXContractClient,
        admin: &Address,
        business: &Address,
    ) {
        let kyc_data = create_test_kyc_data(env, "TestBusiness");
        client.submit_kyc_application(business, &kyc_data);
        client.verify_business(&admin, business);
    }

    fn create_verified_investor(
        env: &Env,
        client: &QuickLendXContractClient,
        admin: &Address,
        investor: &Address,
    ) {
        let kyc_data = create_test_kyc_data(env, "TestInvestor");
        client.submit_investor_kyc(investor, &kyc_data);
        client.verify_investor(&investor, &500_000i128);
    }

    fn create_invoice(
        env: &Env,
        client: &QuickLendXContractClient,
        business: &Address,
    ) -> soroban_sdk::BytesN<32> {
        let currency = Address::generate(env);
        let due_date = env.ledger().timestamp() + 86400;
        let description = String::from_str(env, "Test invoice");
        let category = InvoiceCategory::Goods;
        let tags = Vec::new(env);

        client.store_invoice(
            business,
            &100_000i128,
            &currency,
            &due_date,
            &description,
            &category,
            &tags,
        )
    }

    fn create_and_verify_invoice(
        env: &Env,
        client: &QuickLendXContractClient,
        admin: &Address,
        business: &Address,
    ) -> soroban_sdk::BytesN<32> {
        let invoice_id = create_invoice(env, client, business);
        client.verify_invoice(&admin, &invoice_id);
        invoice_id
    }

    // ============================================================================
    // BUSINESS KYC LIFECYCLE ENFORCEMENT TESTS
    // ============================================================================

    #[test]
    fn test_unverified_business_cannot_upload_invoice() {
        let (env, client, _admin) = setup();
        let business = Address::generate(&env);
        let currency = Address::generate(&env);
        let due_date = env.ledger().timestamp() + 86400;
        let description = String::from_str(&env, "Test invoice");
        let category = InvoiceCategory::Goods;
        let tags = Vec::new(&env);

        let result = client.try_store_invoice(
            &business,
            &100_000i128,
            &currency,
            &due_date,
            &description,
            &category,
            &tags,
        );

        assert_eq!(result, Err(QuickLendXError::BusinessNotVerified));
    }

    #[test]
    fn test_pending_business_cannot_upload_invoice() {
        let (env, client, _admin) = setup();
        let business = Address::generate(&env);
        let kyc_data = create_test_kyc_data(&env, "TestBusiness");

        // Submit KYC (becomes pending)
        client.submit_kyc_application(&business, &kyc_data);

        let currency = Address::generate(&env);
        let due_date = env.ledger().timestamp() + 86400;
        let description = String::from_str(&env, "Test invoice");
        let category = InvoiceCategory::Goods;
        let tags = Vec::new(&env);

        let result = client.try_store_invoice(
            &business,
            &100_000i128,
            &currency,
            &due_date,
            &description,
            &category,
            &tags,
        );

        assert_eq!(result, Err(QuickLendXError::KYCAlreadyPending));
    }

    #[test]
    fn test_rejected_business_cannot_upload_invoice() {
        let (env, client, admin) = setup();
        let business = Address::generate(&env);
        let kyc_data = create_test_kyc_data(&env, "TestBusiness");
        let rejection_reason = String::from_str(&env, "Invalid documents");

        // Submit and reject KYC
        client.submit_kyc_application(&business, &kyc_data);
        assert!(client.reject_business(&admin, &business, &rejection_reason).is_ok());

        let currency = Address::generate(&env);
        let due_date = env.ledger().timestamp() + 86400;
        let description = String::from_str(&env, "Test invoice");
        let category = InvoiceCategory::Goods;
        let tags = Vec::new(&env);

        let result = client.try_store_invoice(
            &business,
            &100_000i128,
            &currency,
            &due_date,
            &description,
            &category,
            &tags,
        );

        assert_eq!(result, Err(QuickLendXError::BusinessNotVerified));
    }

    #[test]
    fn test_verified_business_can_upload_invoice() {
        let (env, client, admin) = setup();
        let business = Address::generate(&env);

        create_verified_business(&env, &client, &admin, &business);

        let currency = Address::generate(&env);
        let due_date = env.ledger().timestamp() + 86400;
        let description = String::from_str(&env, "Test invoice");
        let category = InvoiceCategory::Goods;
        let tags = Vec::new(&env);

        let result = client.try_store_invoice(
            &business,
            &100_000i128,
            &currency,
            &due_date,
            &description,
            &category,
            &tags,
        );

        assert!(result.is_ok());
    }

    #[test]
    fn test_unverified_business_cannot_cancel_invoice() {
        let (env, client, admin) = setup();
        let business = Address::generate(&env);

        // Create and verify invoice with a different business
        let other_business = Address::generate(&env);
        create_verified_business(&env, &client, &admin, &other_business);
        let invoice_id = create_and_verify_invoice(&env, &client, &admin, &other_business);

        // Try to cancel with unverified business (should fail with auth error first)
        let result = client.try_cancel_invoice(&business, &invoice_id);
        assert!(result.is_err()); // Auth error takes precedence
    }

    #[test]
    fn test_pending_business_cannot_cancel_own_invoice() {
        let (env, client, admin) = setup();
        let business = Address::generate(&env);
        let kyc_data = create_test_kyc_data(&env, "TestBusiness");

        // Create verified business and invoice
        create_verified_business(&env, &client, &admin, &business);
        let invoice_id = create_and_verify_invoice(&env, &client, &admin, &business);

        // Submit new KYC application (makes business pending)
        let new_kyc_data = create_test_kyc_data(&env, "UpdatedBusiness");
        client.submit_kyc_application(&business, &new_kyc_data);

        // Try to cancel invoice while pending
        let result = client.try_cancel_invoice(&business, &invoice_id);
        assert_eq!(result, Err(QuickLendXError::KYCAlreadyPending));
    }

    #[test]
    fn test_rejected_business_cannot_cancel_own_invoice() {
        let (env, client, admin) = setup();
        let business = Address::generate(&env);
        let kyc_data = create_test_kyc_data(&env, "TestBusiness");
        let rejection_reason = String::from_str(&env, "Invalid documents");

        // Create verified business and invoice
        create_verified_business(&env, &client, &admin, &business);
        let invoice_id = create_and_verify_invoice(&env, &client, &admin, &business);

        // Submit and reject new KYC application
        let new_kyc_data = create_test_kyc_data(&env, "UpdatedBusiness");
        client.submit_kyc_application(&business, &new_kyc_data);
        assert!(client.reject_business(&admin, &business, &rejection_reason).is_ok());

        // Try to cancel invoice while rejected
        let result = client.try_cancel_invoice(&business, &invoice_id);
        assert_eq!(result, Err(QuickLendXError::BusinessNotVerified));
    }

    #[test]
    fn test_verified_business_can_cancel_own_invoice() {
        let (env, client, admin) = setup();
        let business = Address::generate(&env);

        create_verified_business(&env, &client, &admin, &business);
        let invoice_id = create_and_verify_invoice(&env, &client, &admin, &business);

        let result = client.try_cancel_invoice(&business, &invoice_id);
        assert!(result.is_ok());
    }

    #[test]
    fn test_pending_business_cannot_accept_bid() {
        let (env, client, admin) = setup();
        let business = Address::generate(&env);
        let investor = Address::generate(&env);

        // Setup verified entities
        create_verified_business(&env, &client, &admin, &business);
        create_verified_investor(&env, &client, &admin, &investor);

        // Create invoice and place bid
        let invoice_id = create_and_verify_invoice(&env, &client, &admin, &business);
        let bid_id = client.place_bid(&investor, &invoice_id, &50_000i128, &55_000i128);

        // Make business pending by submitting new KYC
        let new_kyc_data = create_test_kyc_data(&env, "UpdatedBusiness");
        client.submit_kyc_application(&business, &new_kyc_data);

        // Try to accept bid while pending
        let result = client.try_accept_bid(&business, &bid_id);
        assert_eq!(result, Err(QuickLendXError::KYCAlreadyPending));
    }

    #[test]
    fn test_rejected_business_cannot_accept_bid() {
        let (env, client, admin) = setup();
        let business = Address::generate(&env);
        let investor = Address::generate(&env);
        let rejection_reason = String::from_str(&env, "Invalid documents");

        // Setup verified entities
        create_verified_business(&env, &client, &admin, &business);
        create_verified_investor(&env, &client, &admin, &investor);

        // Create invoice and place bid
        let invoice_id = create_and_verify_invoice(&env, &client, &admin, &business);
        let bid_id = client.place_bid(&investor, &invoice_id, &50_000i128, &55_000i128);

        // Make business rejected by submitting and rejecting new KYC
        let new_kyc_data = create_test_kyc_data(&env, "UpdatedBusiness");
        client.submit_kyc_application(&business, &new_kyc_data);
        assert!(client.reject_business(&admin, &business, &rejection_reason).is_ok());

        // Try to accept bid while rejected
        let result = client.try_accept_bid(&business, &bid_id);
        assert_eq!(result, Err(QuickLendXError::BusinessNotVerified));
    }

    #[test]
    fn test_verified_business_can_accept_bid() {
        let (env, client, admin) = setup();
        let business = Address::generate(&env);
        let investor = Address::generate(&env);

        create_verified_business(&env, &client, &admin, &business);
        create_verified_investor(&env, &client, &admin, &investor);

        let invoice_id = create_and_verify_invoice(&env, &client, &admin, &business);
        let bid_id = client.place_bid(&investor, &invoice_id, &50_000i128, &55_000i128);

        let result = client.try_accept_bid(&business, &bid_id);
        assert!(result.is_ok());
    }

    // ============================================================================
    // INVESTOR KYC LIFECYCLE ENFORCEMENT TESTS
    // ============================================================================

    #[test]
    fn test_unverified_investor_cannot_place_bid() {
        let (env, client, admin) = setup();
        let business = Address::generate(&env);
        let investor = Address::generate(&env);

        create_verified_business(&env, &client, &admin, &business);
        let invoice_id = create_and_verify_invoice(&env, &client, &admin, &business);

        let result = client.try_place_bid(&investor, &invoice_id, &50_000i128, &55_000i128);
        assert_eq!(result, Err(QuickLendXError::BusinessNotVerified));
    }

    #[test]
    fn test_pending_investor_cannot_place_bid() {
        let (env, client, admin) = setup();
        let business = Address::generate(&env);
        let investor = Address::generate(&env);
        let kyc_data = create_test_kyc_data(&env, "TestInvestor");

        create_verified_business(&env, &client, &admin, &business);
        let invoice_id = create_and_verify_invoice(&env, &client, &admin, &business);

        // Submit KYC (becomes pending)
        client.submit_investor_kyc(&investor, &kyc_data);

        let result = client.try_place_bid(&investor, &invoice_id, &50_000i128, &55_000i128);
        assert_eq!(result, Err(QuickLendXError::KYCAlreadyPending));
    }

    #[test]
    fn test_rejected_investor_cannot_place_bid() {
        let (env, client, admin) = setup();
        let business = Address::generate(&env);
        let investor = Address::generate(&env);
        let kyc_data = create_test_kyc_data(&env, "TestInvestor");
        let rejection_reason = String::from_str(&env, "Invalid documents");

        create_verified_business(&env, &client, &admin, &business);
        let invoice_id = create_and_verify_invoice(&env, &client, &admin, &business);

        // Submit and reject KYC
        client.submit_investor_kyc(&investor, &kyc_data);
        assert!(client.try_reject_investor(&investor, &rejection_reason).is_ok());

        let result = client.try_place_bid(&investor, &invoice_id, &50_000i128, &55_000i128);
        assert_eq!(result, Err(QuickLendXError::BusinessNotVerified));
    }

    #[test]
    fn test_verified_investor_can_place_bid() {
        let (env, client, admin) = setup();
        let business = Address::generate(&env);
        let investor = Address::generate(&env);

        create_verified_business(&env, &client, &admin, &business);
        create_verified_investor(&env, &client, &admin, &investor);

        let invoice_id = create_and_verify_invoice(&env, &client, &admin, &business);

        let result = client.try_place_bid(&investor, &invoice_id, &50_000i128, &55_000i128);
        assert!(result.is_ok());
    }

    #[test]
    fn test_pending_investor_cannot_withdraw_bid() {
        let (env, client, admin) = setup();
        let business = Address::generate(&env);
        let investor = Address::generate(&env);
        let kyc_data = create_test_kyc_data(&env, "TestInvestor");

        // Setup verified entities and place bid
        create_verified_business(&env, &client, &admin, &business);
        create_verified_investor(&env, &client, &admin, &investor);
        let invoice_id = create_and_verify_invoice(&env, &client, &admin, &business);
        let bid_id = client.place_bid(&investor, &invoice_id, &50_000i128, &55_000i128);

        // Make investor pending by submitting new KYC
        let new_kyc_data = create_test_kyc_data(&env, "UpdatedInvestor");
        client.submit_investor_kyc(&investor, &new_kyc_data);

        // Try to withdraw bid while pending
        let result = client.try_withdraw_bid(&bid_id);
        assert_eq!(result, Err(QuickLendXError::KYCAlreadyPending));
    }

    #[test]
    fn test_rejected_investor_cannot_withdraw_bid() {
        let (env, client, admin) = setup();
        let business = Address::generate(&env);
        let investor = Address::generate(&env);
        let kyc_data = create_test_kyc_data(&env, "TestInvestor");
        let rejection_reason = String::from_str(&env, "Invalid documents");

        // Setup verified entities and place bid
        create_verified_business(&env, &client, &admin, &business);
        create_verified_investor(&env, &client, &admin, &investor);
        let invoice_id = create_and_verify_invoice(&env, &client, &admin, &business);
        let bid_id = client.place_bid(&investor, &invoice_id, &50_000i128, &55_000i128);

        // Make investor rejected by submitting and rejecting new KYC
        let new_kyc_data = create_test_kyc_data(&env, "UpdatedInvestor");
        client.submit_investor_kyc(&investor, &new_kyc_data);
        assert!(client.try_reject_investor(&investor, &rejection_reason).is_ok());

        // Try to withdraw bid while rejected
        let result = client.try_withdraw_bid(&bid_id);
        assert_eq!(result, Err(QuickLendXError::BusinessNotVerified));
    }

    #[test]
    fn test_verified_investor_can_withdraw_bid() {
        let (env, client, admin) = setup();
        let business = Address::generate(&env);
        let investor = Address::generate(&env);

        create_verified_business(&env, &client, &admin, &business);
        create_verified_investor(&env, &client, &admin, &investor);

        let invoice_id = create_and_verify_invoice(&env, &client, &admin, &business);
        let bid_id = client.place_bid(&investor, &invoice_id, &50_000i128, &55_000i128);

        let result = client.try_withdraw_bid(&bid_id);
        assert!(result.is_ok());
    }

    // ============================================================================
    // INDEX QUERY CONSISTENCY TESTS
    // ============================================================================

    #[test]
    fn test_business_kyc_state_transitions_maintain_index_consistency() {
        let (env, client, admin) = setup();
        let business = Address::generate(&env);
        let kyc_data = create_test_kyc_data(&env, "TestBusiness");
        let rejection_reason = String::from_str(&env, "Invalid documents");

        // Initial state: no KYC
        let pending_list = client.get_pending_businesses();
        let verified_list = client.get_verified_businesses();
        let rejected_list = client.get_rejected_businesses();
        assert!(!pending_list.contains(&business));
        assert!(!verified_list.contains(&business));
        assert!(!rejected_list.contains(&business));

        // Submit KYC: becomes pending
        client.submit_kyc_application(&business, &kyc_data);
        let pending_list = client.get_pending_businesses();
        let verified_list = client.get_verified_businesses();
        let rejected_list = client.get_rejected_businesses();
        assert!(pending_list.contains(&business));
        assert!(!verified_list.contains(&business));
        assert!(!rejected_list.contains(&business));

        // Verify: moves to verified
        client.verify_business(&admin, &business);
        let pending_list = client.get_pending_businesses();
        let verified_list = client.get_verified_businesses();
        let rejected_list = client.get_rejected_businesses();
        assert!(!pending_list.contains(&business));
        assert!(verified_list.contains(&business));
        assert!(!rejected_list.contains(&business));

        // Submit new KYC: becomes pending again
        let new_kyc_data = create_test_kyc_data(&env, "UpdatedBusiness");
        client.submit_kyc_application(&business, &new_kyc_data);
        let pending_list = client.get_pending_businesses();
        let verified_list = client.get_verified_businesses();
        let rejected_list = client.get_rejected_businesses();
        assert!(pending_list.contains(&business));
        assert!(!verified_list.contains(&business)); // Should be removed from verified
        assert!(!rejected_list.contains(&business));

        // Reject: moves to rejected
        client.reject_business(&admin, &business, &rejection_reason);
        let pending_list = client.get_pending_businesses();
        let verified_list = client.get_verified_businesses();
        let rejected_list = client.get_rejected_businesses();
        assert!(!pending_list.contains(&business));
        assert!(!verified_list.contains(&business));
        assert!(rejected_list.contains(&business));

        // Resubmit after rejection: becomes pending
        let resubmit_kyc_data = create_test_kyc_data(&env, "ResubmittedBusiness");
        client.submit_kyc_application(&business, &resubmit_kyc_data);
        let pending_list = client.get_pending_businesses();
        let verified_list = client.get_verified_businesses();
        let rejected_list = client.get_rejected_businesses();
        assert!(pending_list.contains(&business));
        assert!(!verified_list.contains(&business));
        assert!(!rejected_list.contains(&business)); // Should be removed from rejected
    }

    #[test]
    fn test_investor_kyc_state_transitions_maintain_index_consistency() {
        let (env, client, admin) = setup();
        let investor = Address::generate(&env);
        let kyc_data = create_test_kyc_data(&env, "TestInvestor");
        let rejection_reason = String::from_str(&env, "Invalid documents");

        // Initial state: no KYC
        let pending_list = client.get_pending_investors();
        let verified_list = client.get_verified_investors();
        let rejected_list = client.get_rejected_investors();
        assert!(!pending_list.contains(&investor));
        assert!(!verified_list.contains(&investor));
        assert!(!rejected_list.contains(&investor));

        // Submit KYC: becomes pending
        client.submit_investor_kyc(&investor, &kyc_data);
        let pending_list = client.get_pending_investors();
        let verified_list = client.get_verified_investors();
        let rejected_list = client.get_rejected_investors();
        assert!(pending_list.contains(&investor));
        assert!(!verified_list.contains(&investor));
        assert!(!rejected_list.contains(&investor));

        // Verify: moves to verified
        client.verify_investor(&admin, &investor, &500_000i128);
        let pending_list = client.get_pending_investors();
        let verified_list = client.get_verified_investors();
        let rejected_list = client.get_rejected_investors();
        assert!(!pending_list.contains(&investor));
        assert!(verified_list.contains(&investor));
        assert!(!rejected_list.contains(&investor));

        // Submit new KYC: becomes pending again
        let new_kyc_data = create_test_kyc_data(&env, "UpdatedInvestor");
        client.submit_investor_kyc(&investor, &new_kyc_data);
        let pending_list = client.get_pending_investors();
        let verified_list = client.get_verified_investors();
        let rejected_list = client.get_rejected_investors();
        assert!(pending_list.contains(&investor));
        assert!(!verified_list.contains(&investor)); // Should be removed from verified
        assert!(!rejected_list.contains(&investor));

        // Reject: moves to rejected
        assert!(client.try_reject_investor(&investor, &rejection_reason).is_ok());
        let pending_list = client.get_pending_investors();
        let verified_list = client.get_verified_investors();
        let rejected_list = client.get_rejected_investors();
        assert!(!pending_list.contains(&investor));
        assert!(!verified_list.contains(&investor));
        assert!(rejected_list.contains(&investor));

        // Resubmit after rejection: becomes pending
        let resubmit_kyc_data = create_test_kyc_data(&env, "ResubmittedInvestor");
        client.submit_investor_kyc(&investor, &resubmit_kyc_data);
        let pending_list = client.get_pending_investors();
        let verified_list = client.get_verified_investors();
        let rejected_list = client.get_rejected_investors();
        assert!(pending_list.contains(&investor));
        assert!(!verified_list.contains(&investor));
        assert!(!rejected_list.contains(&investor)); // Should be removed from rejected
    }

    #[test]
    fn test_multiple_entities_kyc_transitions_maintain_separate_indexes() {
        let (env, client, admin) = setup();
        let business1 = Address::generate(&env);
        let business2 = Address::generate(&env);
        let investor1 = Address::generate(&env);
        let investor2 = Address::generate(&env);

        // Setup initial KYC states
        let kyc1 = create_test_kyc_data(&env, "Business1");
        let kyc2 = create_test_kyc_data(&env, "Business2");
        let inv_kyc1 = create_test_kyc_data(&env, "Investor1");
        let inv_kyc2 = create_test_kyc_data(&env, "Investor2");

        // Business1: verified
        client.submit_kyc_application(&business1, &kyc1);
        client.verify_business(&admin, &business1);

        // Business2: pending
        client.submit_kyc_application(&business2, &kyc2);

        // Investor1: verified
        client.submit_investor_kyc(&investor1, &inv_kyc1);
        client.verify_investor(&investor1, &500_000i128);

        // Investor2: rejected
        client.submit_investor_kyc(&investor2, &inv_kyc2);
        let rejection_reason = String::from_str(&env, "Invalid docs");
        client.reject_investor(&admin, &investor2, &rejection_reason);

        // Verify business indexes
        let business_pending = client.get_pending_businesses();
        let business_verified = client.get_verified_businesses();
        let business_rejected = client.get_rejected_businesses();

        assert!(!business_pending.contains(&business1));
        assert!(business_verified.contains(&business1));
        assert!(!business_rejected.contains(&business1));

        assert!(business_pending.contains(&business2));
        assert!(!business_verified.contains(&business2));
        assert!(!business_rejected.contains(&business2));

        // Verify investor indexes
        let investor_pending = client.get_pending_investors();
        let investor_verified = client.get_verified_investors();
        let investor_rejected = client.get_rejected_investors();

        assert!(!investor_pending.contains(&investor1));
        assert!(investor_verified.contains(&investor1));
        assert!(!investor_rejected.contains(&investor1));

        assert!(!investor_pending.contains(&investor2));
        assert!(!investor_verified.contains(&investor2));
        assert!(investor_rejected.contains(&investor2));
    }

    // ============================================================================
    // COMPREHENSIVE LIFECYCLE VALIDATION TESTS
    // ============================================================================

    #[test]
    fn test_complete_business_kyc_lifecycle_with_operations() {
        let (env, client, admin) = setup();
        let business = Address::generate(&env);
        let investor = Address::generate(&env);
        let kyc_data = create_test_kyc_data(&env, "TestBusiness");
        let rejection_reason = String::from_str(&env, "Invalid documents");

        create_verified_investor(&env, &client, &admin, &investor);

        // Phase 1: Unverified - cannot upload invoice
        let currency = Address::generate(&env);
        let due_date = env.ledger().timestamp() + 86400;
        let description = String::from_str(&env, "Test invoice");
        let category = InvoiceCategory::Goods;
        let tags = Vec::new(&env);

        let result = client.try_store_invoice(
            &business,
            &100_000i128,
            &currency,
            &due_date,
            &description,
            &category,
            &tags,
        );
        assert_eq!(result, Err(QuickLendXError::BusinessNotVerified));

        // Phase 2: Submit KYC - becomes pending, still cannot upload
        client.submit_kyc_application(&business, &kyc_data);

        let result = client.try_store_invoice(
            &business,
            &100_000i128,
            &currency,
            &due_date,
            &description,
            &category,
            &tags,
        );
        assert_eq!(result, Err(QuickLendXError::KYCAlreadyPending));

        // Phase 3: Get verified - now can upload invoice
        client.verify_business(&admin, &business, &1_000_000i128);

        let invoice_id = client.store_invoice(
            &business,
            &100_000i128,
            &currency,
            &due_date,
            &description,
            &category,
            &tags,
        );

        // Phase 4: Verify invoice and place bid
        client.verify_invoice(&admin, &invoice_id);
        let bid_id = client.place_bid(&investor, &invoice_id, &50_000i128, &55_000i128);

        // Phase 5: Submit new KYC - becomes pending, cannot accept bid
        let new_kyc_data = create_test_kyc_data(&env, "UpdatedBusiness");
        client.submit_kyc_application(&business, &new_kyc_data);

        let result = client.try_accept_bid(&business, &bid_id);
        assert_eq!(result, Err(QuickLendXError::KYCAlreadyPending));

        // Phase 6: Get rejected - cannot accept bid
        client.reject_business(&admin, &business, &rejection_reason);

        let result = client.try_accept_bid(&business, &bid_id);
        assert_eq!(result, Err(QuickLendXError::BusinessNotVerified));

        // Phase 7: Resubmit KYC - becomes pending again
        let resubmit_kyc_data = create_test_kyc_data(&env, "ResubmittedBusiness");
        client.submit_kyc_application(&business, &resubmit_kyc_data);

        let result = client.try_accept_bid(&business, &bid_id);
        assert_eq!(result, Err(QuickLendXError::KYCAlreadyPending));

        // Phase 8: Final verification - can now accept bid
        client.verify_business(&admin, &business, &1_000_000i128);

        let result = client.try_accept_bid(&business, &bid_id);
        assert!(result.is_ok());
    }

    #[test]
    fn test_complete_investor_kyc_lifecycle_with_operations() {
        let (env, client, admin) = setup();
        let business = Address::generate(&env);
        let investor = Address::generate(&env);
        let kyc_data = create_test_kyc_data(&env, "TestInvestor");
        let rejection_reason = String::from_str(&env, "Invalid documents");

        create_verified_business(&env, &client, &admin, &business);
        let invoice_id = create_and_verify_invoice(&env, &client, &admin, &business);

        // Phase 1: Unverified - cannot place bid
        let result = client.try_place_bid(&investor, &invoice_id, &50_000i128, &55_000i128);
        assert_eq!(result, Err(QuickLendXError::BusinessNotVerified));

        // Phase 2: Submit KYC - becomes pending, still cannot place bid
        client.submit_investor_kyc(&investor, &kyc_data);

        let result = client.try_place_bid(&investor, &invoice_id, &50_000i128, &55_000i128);
        assert_eq!(result, Err(QuickLendXError::KYCAlreadyPending));

        // Phase 3: Get verified - now can place bid
        client.verify_investor(&admin, &investor, &500_000i128);

        let bid_id = client.place_bid(&investor, &invoice_id, &50_000i128, &55_000i128);

        // Phase 4: Submit new KYC - becomes pending, cannot withdraw bid
        let new_kyc_data = create_test_kyc_data(&env, "UpdatedInvestor");
        client.submit_investor_kyc(&investor, &new_kyc_data);

        let result = client.try_withdraw_bid(&bid_id);
        assert_eq!(result, Err(QuickLendXError::KYCAlreadyPending));

        // Phase 5: Get rejected - cannot withdraw bid
        client.reject_investor(&admin, &investor, &rejection_reason);

        let result = client.try_withdraw_bid(&bid_id);
        assert_eq!(result, Err(QuickLendXError::BusinessNotVerified));

        // Phase 6: Resubmit KYC - becomes pending again
        let resubmit_kyc_data = create_test_kyc_data(&env, "ResubmittedInvestor");
        client.submit_investor_kyc(&investor, &resubmit_kyc_data);

        let result = client.try_withdraw_bid(&bid_id);
        assert_eq!(result, Err(QuickLendXError::KYCAlreadyPending));

        // Phase 7: Final verification - can now withdraw bid
        client.verify_investor(&admin, &investor, &500_000i128);

        let result = client.try_withdraw_bid(&bid_id);
        assert!(result.is_ok());
    }
}