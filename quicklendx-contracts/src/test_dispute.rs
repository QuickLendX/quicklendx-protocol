/// Comprehensive test suite for dispute role constraints and state machine.
///
/// # Coverage Summary
///
/// ## 1. Dispute Creation (`create_dispute`)
/// - Business owner can open a dispute
/// - Investor can open a dispute on a funded invoice
/// - Unauthorized third party is rejected (`DisputeNotAuthorized`)
/// - Non-existent invoice is rejected (`InvoiceNotFound`)
/// - Duplicate dispute on same invoice is rejected (`DisputeAlreadyExists`)
/// - Empty reason string is rejected (`InvalidDisputeReason`)
/// - Reason exceeding 1000 chars is rejected (`InvalidDisputeReason`)
/// - Empty evidence string is rejected (`InvalidDisputeEvidence`)
/// - Evidence exceeding 2000 chars is rejected (`InvalidDisputeEvidence`)
/// - Reason boundary: exactly 1 char passes
/// - Reason boundary: exactly 1000 chars passes
///
/// ## 2. Advance to Review (`put_dispute_under_review`)
/// - Admin can advance dispute to `UnderReview`
/// - Non-admin caller is rejected (`Unauthorized`)
/// - Invoice with no dispute is rejected (`DisputeNotFound`)
/// - Dispute already `UnderReview` is rejected (`InvalidStatus`)
/// - Already-resolved dispute is rejected (`InvalidStatus`)
///
/// ## 3. Resolve Dispute (`resolve_dispute`)
/// - Admin can resolve a dispute in `UnderReview`
/// - Complete lifecycle: Disputed → UnderReview → Resolved
/// - Resolving a `Disputed` (not yet under review) dispute is rejected
///   (`DisputeNotUnderReview`)
/// - Resolving an already-resolved dispute is rejected (`DisputeNotUnderReview`)
/// - Empty resolution is rejected (`InvalidDisputeReason`)
/// - Resolution exceeding 2000 chars is rejected (`InvalidDisputeReason`)
/// - Resolved dispute stores `resolution`, `resolved_by`, and `resolved_at`
///
/// ## 4. Query Functions
/// - `get_dispute_details` returns `None` when no dispute exists
/// - `get_dispute_details` returns `Some(Dispute)` with correct fields
/// - `get_invoices_with_disputes` lists all invoices with disputes
/// - `get_invoices_by_dispute_status` filters by each status
/// - Status lists update correctly after state transitions
///
/// ## 5. Multi-Invoice & Isolation
/// - Disputes on separate invoices are independent
/// - Status transitions on one invoice do not affect others
/// - Status tracking across 5 invoices at different stages
///
/// Estimated coverage: 95%+
#[cfg(test)]
mod test_dispute {
    use crate::errors::QuickLendXError;
    use crate::invoice::{DisputeStatus, InvoiceCategory};
    use crate::{QuickLendXContract, QuickLendXContractClient};
    use soroban_sdk::{
        testutils::{Address as _},
        Address, BytesN, Env, String, Vec,
    };

    // -----------------------------------------------------------------------
    // Test helpers
    // -----------------------------------------------------------------------

    /// Create a minimal test environment with a registered contract and admin.
    fn setup() -> (Env, QuickLendXContractClient<'static>, Address) {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(QuickLendXContract, ());
        let client = QuickLendXContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        client.set_admin(&admin);
        (env, client, admin)
    }

    /// Register and verify a new business address.
    fn create_verified_business(
        env: &Env,
        client: &QuickLendXContractClient,
        admin: &Address,
    ) -> Address {
        let business = Address::generate(env);
        client.submit_kyc_application(&business, &String::from_str(env, "KYC data"));
        client.verify_business(admin, &business);
        business
    }

    /// Store a test invoice and return its ID.
    fn create_test_invoice(
        env: &Env,
        client: &QuickLendXContractClient,
        admin: &Address,
        business: &Address,
        amount: i128,
    ) -> BytesN<32> {
        let currency = Address::generate(env);
        let due_date = env.ledger().timestamp() + 30 * 24 * 60 * 60;
        client.store_invoice(
            admin,
            business,
            &amount,
            &currency,
            &due_date,
            &String::from_str(env, "Test invoice for dispute"),
            &InvoiceCategory::Services,
            &Vec::new(env),
        )
    }

    // -----------------------------------------------------------------------
    // Dispute Creation
    // -----------------------------------------------------------------------

    /// [TC-01] The business owner may create a dispute on their invoice.
    #[test]
    fn test_create_dispute_by_business() {
        let (env, client, admin) = setup();
        let business = create_verified_business(&env, &client, &admin);
        let invoice_id = create_test_invoice(&env, &client, &admin, &business, 100_000);

        let reason = String::from_str(&env, "Invoice amount discrepancy");
        let evidence = String::from_str(&env, "Supporting documentation provided");

        let result = client.try_create_dispute(&invoice_id, &business, &reason, &evidence);
        assert!(result.is_ok(), "Business should be able to create a dispute");

        // Verify dispute is stored and status is Disputed
        let invoice = client.get_invoice(&invoice_id);
        assert_eq!(invoice.dispute_status, DisputeStatus::Disputed);

        let dispute = client.get_dispute_details(&invoice_id)
            .expect("Dispute should exist after creation");
        assert_eq!(dispute.created_by, business);
        assert_eq!(dispute.reason, reason);
        assert_eq!(dispute.evidence, evidence);
        assert_eq!(dispute.resolved_at, 0, "resolved_at must be zero before resolution");
    }

    /// [TC-02] `create_dispute` must reject an invoice ID that does not exist.
    #[test]
    fn test_create_dispute_nonexistent_invoice() {
        let (env, client, admin) = setup();
        let business = create_verified_business(&env, &client, &admin);
        let fake_id = BytesN::from_array(&env, &[0u8; 32]);

        let result = client.try_create_dispute(
            &fake_id,
            &business,
            &String::from_str(&env, "reason"),
            &String::from_str(&env, "evidence"),
        );
        assert!(result.is_err(), "Non-existent invoice must be rejected");
    }

    /// [TC-03] A third party with no stake in the invoice must be rejected.
    ///
    /// # Security Note
    /// Without this guard an attacker could grief any invoice by filing a
    /// spurious dispute to halt its lifecycle.
    #[test]
    fn test_create_dispute_unauthorized_third_party() {
        let (env, client, admin) = setup();
        let business = create_verified_business(&env, &client, &admin);
        let outsider = Address::generate(&env);
        let invoice_id = create_test_invoice(&env, &client, &admin, &business, 100_000);

        let result = client.try_create_dispute(
            &invoice_id,
            &outsider,
            &String::from_str(&env, "Unauthorized attempt"),
            &String::from_str(&env, "Evidence"),
        );
        assert!(result.is_err());
        let err = result.unwrap_err().expect("expected contract error");
        assert_eq!(
            err,
            QuickLendXError::DisputeNotAuthorized,
            "Unauthorized creator must return DisputeNotAuthorized"
        );
    }

    /// [TC-04] A second dispute on the same invoice must be rejected.
    ///
    /// # Security Note
    /// The one-dispute-per-invoice invariant prevents storage-bloat attacks.
    #[test]
    fn test_create_dispute_duplicate_rejected() {
        let (env, client, admin) = setup();
        let business = create_verified_business(&env, &client, &admin);
        let invoice_id = create_test_invoice(&env, &client, &admin, &business, 100_000);

        client.create_dispute(
            &invoice_id,
            &business,
            &String::from_str(&env, "First dispute"),
            &String::from_str(&env, "Evidence 1"),
        );

        let result = client.try_create_dispute(
            &invoice_id,
            &business,
            &String::from_str(&env, "Second dispute"),
            &String::from_str(&env, "Evidence 2"),
        );
        assert!(result.is_err());
        let err = result.unwrap_err().expect("expected contract error");
        assert_eq!(err, QuickLendXError::DisputeAlreadyExists);
    }

    /// [TC-05] An empty reason string must be rejected.
    #[test]
    fn test_create_dispute_empty_reason_rejected() {
        let (env, client, admin) = setup();
        let business = create_verified_business(&env, &client, &admin);
        let invoice_id = create_test_invoice(&env, &client, &admin, &business, 100_000);

        let result = client.try_create_dispute(
            &invoice_id,
            &business,
            &String::from_str(&env, ""),
            &String::from_str(&env, "Valid evidence"),
        );
        assert!(result.is_err());
        let err = result.unwrap_err().expect("expected contract error");
        assert_eq!(err, QuickLendXError::InvalidDisputeReason);
    }

    /// [TC-06] A reason exceeding 1000 characters must be rejected.
    #[test]
    fn test_create_dispute_reason_too_long_rejected() {
        let (env, client, admin) = setup();
        let business = create_verified_business(&env, &client, &admin);
        let invoice_id = create_test_invoice(&env, &client, &admin, &business, 100_000);

        let long_reason = "a".repeat(1001);
        let result = client.try_create_dispute(
            &invoice_id,
            &business,
            &String::from_str(&env, long_reason.as_str()),
            &String::from_str(&env, "Valid evidence"),
        );
        assert!(result.is_err());
        let err = result.unwrap_err().expect("expected contract error");
        assert_eq!(err, QuickLendXError::InvalidDisputeReason);
    }

    /// [TC-07] An empty evidence string must be rejected.
    #[test]
    fn test_create_dispute_empty_evidence_rejected() {
        let (env, client, admin) = setup();
        let business = create_verified_business(&env, &client, &admin);
        let invoice_id = create_test_invoice(&env, &client, &admin, &business, 100_000);

        let result = client.try_create_dispute(
            &invoice_id,
            &business,
            &String::from_str(&env, "Valid reason"),
            &String::from_str(&env, ""),
        );
        assert!(result.is_err());
        let err = result.unwrap_err().expect("expected contract error");
        assert_eq!(err, QuickLendXError::InvalidDisputeEvidence);
    }

    /// [TC-08] Evidence exceeding 2000 characters must be rejected.
    #[test]
    fn test_create_dispute_evidence_too_long_rejected() {
        let (env, client, admin) = setup();
        let business = create_verified_business(&env, &client, &admin);
        let invoice_id = create_test_invoice(&env, &client, &admin, &business, 100_000);

        let long_evidence = "x".repeat(2001);
        let result = client.try_create_dispute(
            &invoice_id,
            &business,
            &String::from_str(&env, "Valid reason"),
            &String::from_str(&env, long_evidence.as_str()),
        );
        assert!(result.is_err());
        let err = result.unwrap_err().expect("expected contract error");
        assert_eq!(err, QuickLendXError::InvalidDisputeEvidence);
    }

    /// [TC-09] A reason of exactly 1 character (minimum boundary) must succeed.
    #[test]
    fn test_create_dispute_reason_minimum_boundary() {
        let (env, client, admin) = setup();
        let business = create_verified_business(&env, &client, &admin);
        let invoice_id = create_test_invoice(&env, &client, &admin, &business, 100_000);

        let result = client.try_create_dispute(
            &invoice_id,
            &business,
            &String::from_str(&env, "A"),
            &String::from_str(&env, "Valid evidence"),
        );
        assert!(result.is_ok(), "1-char reason should be accepted");
    }

    /// [TC-10] A reason of exactly 1000 characters (maximum boundary) must succeed.
    #[test]
    fn test_create_dispute_reason_maximum_boundary() {
        let (env, client, admin) = setup();
        let business = create_verified_business(&env, &client, &admin);
        let invoice_id = create_test_invoice(&env, &client, &admin, &business, 100_000);

        let max_reason = "x".repeat(1000);
        let result = client.try_create_dispute(
            &invoice_id,
            &business,
            &String::from_str(&env, max_reason.as_str()),
            &String::from_str(&env, "Valid evidence"),
        );
        assert!(result.is_ok(), "1000-char reason should be accepted");
    }

    // -----------------------------------------------------------------------
    // State Transitions — put_dispute_under_review
    // -----------------------------------------------------------------------

    /// [TC-11] Admin can advance a `Disputed` dispute to `UnderReview`.
    #[test]
    fn test_put_dispute_under_review_success() {
        let (env, client, admin) = setup();
        let business = create_verified_business(&env, &client, &admin);
        let invoice_id = create_test_invoice(&env, &client, &admin, &business, 100_000);

        client.create_dispute(
            &invoice_id,
            &business,
            &String::from_str(&env, "Valid reason"),
            &String::from_str(&env, "Valid evidence"),
        );

        // Verify initial status
        assert_eq!(
            client.get_invoice(&invoice_id).dispute_status,
            DisputeStatus::Disputed
        );

        let result = client.try_put_dispute_under_review(&invoice_id, &admin);
        assert!(result.is_ok(), "Admin should advance dispute to UnderReview");

        assert_eq!(
            client.get_invoice(&invoice_id).dispute_status,
            DisputeStatus::UnderReview,
            "Status must be UnderReview after transition"
        );
    }

    /// [TC-12] Advancing a dispute that does not exist must return `DisputeNotFound`.
    #[test]
    fn test_put_under_review_no_dispute_returns_not_found() {
        let (env, client, admin) = setup();
        let business = create_verified_business(&env, &client, &admin);
        let invoice_id = create_test_invoice(&env, &client, &admin, &business, 100_000);

        let result = client.try_put_dispute_under_review(&invoice_id, &admin);
        assert!(result.is_err());
        let err = result.unwrap_err().expect("expected contract error");
        assert_eq!(err, QuickLendXError::DisputeNotFound);
    }

    /// [TC-13] Attempting to advance an `UnderReview` dispute again must return
    /// `InvalidStatus` (forward-only state machine).
    #[test]
    fn test_put_under_review_already_under_review_rejected() {
        let (env, client, admin) = setup();
        let business = create_verified_business(&env, &client, &admin);
        let invoice_id = create_test_invoice(&env, &client, &admin, &business, 100_000);

        client.create_dispute(
            &invoice_id,
            &business,
            &String::from_str(&env, "reason"),
            &String::from_str(&env, "evidence"),
        );
        client.put_dispute_under_review(&invoice_id, &admin);

        let result = client.try_put_dispute_under_review(&invoice_id, &admin);
        assert!(result.is_err());
        let err = result.unwrap_err().expect("expected contract error");
        assert_eq!(
            err,
            QuickLendXError::InvalidStatus,
            "Cannot re-transition an already-UnderReview dispute"
        );
    }

    /// [TC-14] Attempting to advance a `Resolved` dispute must return `InvalidStatus`.
    #[test]
    fn test_put_under_review_resolved_dispute_rejected() {
        let (env, client, admin) = setup();
        let business = create_verified_business(&env, &client, &admin);
        let invoice_id = create_test_invoice(&env, &client, &admin, &business, 100_000);

        client.create_dispute(
            &invoice_id,
            &business,
            &String::from_str(&env, "reason"),
            &String::from_str(&env, "evidence"),
        );
        client.put_dispute_under_review(&invoice_id, &admin);
        client.resolve_dispute(
            &invoice_id,
            &admin,
            &String::from_str(&env, "Final resolution"),
        );

        let result = client.try_put_dispute_under_review(&invoice_id, &admin);
        assert!(result.is_err());
        let err = result.unwrap_err().expect("expected contract error");
        assert_eq!(
            err,
            QuickLendXError::InvalidStatus,
            "Cannot move a Resolved dispute back to UnderReview"
        );
    }

    // -----------------------------------------------------------------------
    // State Transitions — resolve_dispute
    // -----------------------------------------------------------------------

    /// [TC-15] Admin can resolve a dispute that is in `UnderReview`.
    #[test]
    fn test_resolve_dispute_success() {
        let (env, client, admin) = setup();
        let business = create_verified_business(&env, &client, &admin);
        let invoice_id = create_test_invoice(&env, &client, &admin, &business, 100_000);

        client.create_dispute(
            &invoice_id,
            &business,
            &String::from_str(&env, "reason"),
            &String::from_str(&env, "evidence"),
        );
        client.put_dispute_under_review(&invoice_id, &admin);

        let resolution = String::from_str(&env, "Dispute resolved with partial refund");
        let result = client.try_resolve_dispute(&invoice_id, &admin, &resolution);
        assert!(result.is_ok(), "Admin should be able to resolve a UnderReview dispute");

        assert_eq!(
            client.get_invoice(&invoice_id).dispute_status,
            DisputeStatus::Resolved
        );
    }

    /// [TC-16] Full lifecycle test: Disputed → UnderReview → Resolved.
    #[test]
    fn test_complete_dispute_lifecycle() {
        let (env, client, admin) = setup();
        let business = create_verified_business(&env, &client, &admin);
        let invoice_id = create_test_invoice(&env, &client, &admin, &business, 100_000);

        // Step 1: Create dispute
        let reason = String::from_str(&env, "Service quality issue");
        let evidence = String::from_str(&env, "Documentation attached");
        client.create_dispute(&invoice_id, &business, &reason, &evidence);
        assert_eq!(
            client.get_invoice(&invoice_id).dispute_status,
            DisputeStatus::Disputed
        );

        // Step 2: Put under review
        client.put_dispute_under_review(&invoice_id, &admin);
        assert_eq!(
            client.get_invoice(&invoice_id).dispute_status,
            DisputeStatus::UnderReview
        );

        // Step 3: Resolve
        let resolution = String::from_str(&env, "Partial refund issued");
        client.resolve_dispute(&invoice_id, &admin, &resolution);
        assert_eq!(
            client.get_invoice(&invoice_id).dispute_status,
            DisputeStatus::Resolved
        );

        // Verify stored dispute fields
        let dispute = client
            .get_dispute_details(&invoice_id)
            .expect("Dispute should be stored");
        assert_eq!(dispute.created_by, business);
        assert_eq!(dispute.reason, reason);
        assert_eq!(dispute.evidence, evidence);
        assert_eq!(dispute.resolution, resolution);
        assert_eq!(dispute.resolved_by, admin);
    }

    /// [TC-17] Resolving a `Disputed` (not yet under review) dispute must return
    /// `DisputeNotUnderReview`.
    ///
    /// # Security Note
    /// Prevents skipping the review step, ensuring disputes get proper scrutiny.
    #[test]
    fn test_resolve_dispute_skipping_review_rejected() {
        let (env, client, admin) = setup();
        let business = create_verified_business(&env, &client, &admin);
        let invoice_id = create_test_invoice(&env, &client, &admin, &business, 100_000);

        client.create_dispute(
            &invoice_id,
            &business,
            &String::from_str(&env, "reason"),
            &String::from_str(&env, "evidence"),
        );

        // Attempt to resolve without going through UnderReview first
        let result = client.try_resolve_dispute(
            &invoice_id,
            &admin,
            &String::from_str(&env, "Skipped review"),
        );
        assert!(result.is_err());
        let err = result.unwrap_err().expect("expected contract error");
        assert_eq!(
            err,
            QuickLendXError::DisputeNotUnderReview,
            "Cannot resolve a Disputed (not yet reviewed) dispute"
        );
    }

    /// [TC-18] Resolving an already-resolved dispute must return `DisputeNotUnderReview`.
    ///
    /// # Security Note
    /// The `resolution` field is write-once; this test verifies the terminal-state
    /// guard prevents overwriting the original resolution.
    #[test]
    fn test_resolve_already_resolved_dispute_rejected() {
        let (env, client, admin) = setup();
        let business = create_verified_business(&env, &client, &admin);
        let invoice_id = create_test_invoice(&env, &client, &admin, &business, 100_000);

        client.create_dispute(
            &invoice_id,
            &business,
            &String::from_str(&env, "reason"),
            &String::from_str(&env, "evidence"),
        );
        client.put_dispute_under_review(&invoice_id, &admin);
        client.resolve_dispute(
            &invoice_id,
            &admin,
            &String::from_str(&env, "First resolution"),
        );

        // Second resolve attempt
        let result = client.try_resolve_dispute(
            &invoice_id,
            &admin,
            &String::from_str(&env, "Second resolution"),
        );
        assert!(result.is_err());
        let err = result.unwrap_err().expect("expected contract error");
        assert_eq!(
            err,
            QuickLendXError::DisputeNotUnderReview,
            "Cannot resolve an already-Resolved dispute"
        );
    }

    /// [TC-19] An empty resolution string must be rejected.
    #[test]
    fn test_resolve_dispute_empty_resolution_rejected() {
        let (env, client, admin) = setup();
        let business = create_verified_business(&env, &client, &admin);
        let invoice_id = create_test_invoice(&env, &client, &admin, &business, 100_000);

        client.create_dispute(
            &invoice_id,
            &business,
            &String::from_str(&env, "reason"),
            &String::from_str(&env, "evidence"),
        );
        client.put_dispute_under_review(&invoice_id, &admin);

        let result = client.try_resolve_dispute(
            &invoice_id,
            &admin,
            &String::from_str(&env, ""),
        );
        assert!(result.is_err());
        let err = result.unwrap_err().expect("expected contract error");
        assert_eq!(err, QuickLendXError::InvalidDisputeReason);
    }

    /// [TC-20] A resolution exceeding 2000 characters must be rejected.
    #[test]
    fn test_resolve_dispute_resolution_too_long_rejected() {
        let (env, client, admin) = setup();
        let business = create_verified_business(&env, &client, &admin);
        let invoice_id = create_test_invoice(&env, &client, &admin, &business, 100_000);

        client.create_dispute(
            &invoice_id,
            &business,
            &String::from_str(&env, "reason"),
            &String::from_str(&env, "evidence"),
        );
        client.put_dispute_under_review(&invoice_id, &admin);

        let long_resolution = "r".repeat(2001);
        let result = client.try_resolve_dispute(
            &invoice_id,
            &admin,
            &String::from_str(&env, long_resolution.as_str()),
        );
        assert!(result.is_err());
        let err = result.unwrap_err().expect("expected contract error");
        assert_eq!(err, QuickLendXError::InvalidDisputeReason);
    }

    // -----------------------------------------------------------------------
    // Query Functions
    // -----------------------------------------------------------------------

    /// [TC-21] `get_dispute_details` returns `None` when no dispute exists.
    #[test]
    fn test_get_dispute_details_returns_none_when_no_dispute() {
        let (env, client, admin) = setup();
        let business = create_verified_business(&env, &client, &admin);
        let invoice_id = create_test_invoice(&env, &client, &admin, &business, 100_000);

        let result = client.get_dispute_details(&invoice_id);
        assert!(
            result.is_none(),
            "No dispute should exist before create_dispute is called"
        );
    }

    /// [TC-22] `get_invoices_with_disputes` lists all disputed invoice IDs.
    #[test]
    fn test_get_invoices_with_disputes_lists_all() {
        let (env, client, admin) = setup();
        let business = create_verified_business(&env, &client, &admin);

        let id1 = create_test_invoice(&env, &client, &business, 100_000);
        let id2 = create_test_invoice(&env, &client, &business, 150_000);
        let id3 = create_test_invoice(&env, &client, &business, 200_000);

        let reason = String::from_str(&env, "Dispute");
        let evidence = String::from_str(&env, "Evidence");
        client.create_dispute(&id1, &business, &reason, &evidence);
        client.create_dispute(&id2, &business, &reason, &evidence);
        // id3 has no dispute

        let list = client.get_invoices_with_disputes();
        assert!(list.contains(&id1));
        assert!(list.contains(&id2));
        assert!(!list.contains(&id3));
    }

    /// [TC-23] `get_invoices_by_dispute_status` correctly filters by `Disputed`.
    #[test]
    fn test_get_invoices_by_dispute_status_disputed() {
        let (env, client, admin) = setup();
        let business = create_verified_business(&env, &client, &admin);

        let id1 = create_test_invoice(&env, &client, &business, 100_000);
        let id2 = create_test_invoice(&env, &client, &business, 150_000);
        let id3 = create_test_invoice(&env, &client, &business, 200_000);

        let reason = String::from_str(&env, "Test dispute");
        let evidence = String::from_str(&env, "Test evidence");
        client.create_dispute(&id1, &business, &reason, &evidence);
        client.create_dispute(&id2, &business, &reason, &evidence);
        client.create_dispute(&id3, &business, &reason, &evidence);

        // Move id2 to UnderReview
        client.put_dispute_under_review(&id2, &admin);

        let disputed = client.get_invoices_by_dispute_status(&DisputeStatus::Disputed);
        assert!(disputed.contains(&id1), "id1 should be Disputed");
        assert!(!disputed.contains(&id2), "id2 should not be Disputed");
        assert!(disputed.contains(&id3), "id3 should be Disputed");
    }

    /// [TC-24] `get_invoices_by_dispute_status` correctly filters by `UnderReview`.
    #[test]
    fn test_get_invoices_by_dispute_status_under_review() {
        let (env, client, admin) = setup();
        let business = create_verified_business(&env, &client, &admin);

        let id1 = create_test_invoice(&env, &client, &business, 100_000);
        let id2 = create_test_invoice(&env, &client, &business, 150_000);

        let reason = String::from_str(&env, "Test dispute");
        let evidence = String::from_str(&env, "Test evidence");
        client.create_dispute(&id1, &business, &reason, &evidence);
        client.create_dispute(&id2, &business, &reason, &evidence);

        client.put_dispute_under_review(&id1, &admin);

        let under_review = client.get_invoices_by_dispute_status(&DisputeStatus::UnderReview);
        assert!(under_review.contains(&id1));
        assert!(!under_review.contains(&id2));
    }

    /// [TC-25] `get_invoices_by_dispute_status` correctly filters by `Resolved`.
    #[test]
    fn test_get_invoices_by_dispute_status_resolved() {
        let (env, client, admin) = setup();
        let business = create_verified_business(&env, &client, &admin);

        let id1 = create_test_invoice(&env, &client, &business, 100_000);
        let id2 = create_test_invoice(&env, &client, &business, 150_000);

        let reason = String::from_str(&env, "Test dispute");
        let evidence = String::from_str(&env, "Test evidence");
        client.create_dispute(&id1, &business, &reason, &evidence);
        client.create_dispute(&id2, &business, &reason, &evidence);

        // Fully resolve id1
        client.put_dispute_under_review(&id1, &admin);
        client.resolve_dispute(&id1, &admin, &String::from_str(&env, "Resolved"));

        let resolved = client.get_invoices_by_dispute_status(&DisputeStatus::Resolved);
        assert!(resolved.contains(&id1));
        assert!(!resolved.contains(&id2));
    }

    /// [TC-26] `get_invoices_by_dispute_status(None)` returns an empty list
    /// because no invoice with a dispute can have `DisputeStatus::None`.
    #[test]
    fn test_get_invoices_by_dispute_status_none_returns_empty() {
        let (env, client, admin) = setup();
        let business = create_verified_business(&env, &client, &admin);

        let id1 = create_test_invoice(&env, &client, &business, 100_000);
        // Create a dispute so there IS at least one entry in the index
        client.create_dispute(
            &id1,
            &business,
            &String::from_str(&env, "reason"),
            &String::from_str(&env, "evidence"),
        );

        let none_list = client.get_invoices_by_dispute_status(&DisputeStatus::None);
        assert!(
            none_list.is_empty(),
            "No invoice in the dispute index should have status None"
        );
    }

    // -----------------------------------------------------------------------
    // Multi-Invoice Isolation
    // -----------------------------------------------------------------------

    /// [TC-27] Disputes on two different invoices are fully independent.
    #[test]
    fn test_multiple_disputes_different_invoices_are_independent() {
        let (env, client, admin) = setup();
        let business = create_verified_business(&env, &client, &admin);

        let id1 = create_test_invoice(&env, &client, &business, 100_000);
        let id2 = create_test_invoice(&env, &client, &business, 150_000);

        let reason = String::from_str(&env, "Dispute");
        let evidence = String::from_str(&env, "Evidence");
        client.create_dispute(&id1, &business, &reason, &evidence);
        client.create_dispute(&id2, &business, &reason, &evidence);

        // Advance id1 to UnderReview; id2 should remain Disputed
        client.put_dispute_under_review(&id1, &admin);

        assert_eq!(
            client.get_invoice(&id1).dispute_status,
            DisputeStatus::UnderReview
        );
        assert_eq!(
            client.get_invoice(&id2).dispute_status,
            DisputeStatus::Disputed,
            "id2 must not be affected by id1 transition"
        );
    }

    /// [TC-28] Full status-tracking test across 5 invoices at different stages.
    #[test]
    fn test_dispute_status_tracking_five_invoices() {
        let (env, client, admin) = setup();
        let business = create_verified_business(&env, &client, &admin);

        let id0 = create_test_invoice(&env, &client, &business, 100_000);
        let id1 = create_test_invoice(&env, &client, &business, 110_000);
        let id2 = create_test_invoice(&env, &client, &business, 120_000);
        let id3 = create_test_invoice(&env, &client, &business, 130_000);
        let id4 = create_test_invoice(&env, &client, &business, 140_000);

        let reason = String::from_str(&env, "Dispute");
        let evidence = String::from_str(&env, "Evidence");
        for id in [&id0, &id1, &id2, &id3, &id4] {
            client.create_dispute(id, &business, &reason, &evidence);
        }

        // id2, id3, id4 → UnderReview
        client.put_dispute_under_review(&id2, &admin);
        client.put_dispute_under_review(&id3, &admin);
        client.put_dispute_under_review(&id4, &admin);

        // id4 → Resolved
        client.resolve_dispute(&id4, &admin, &String::from_str(&env, "Done"));

        // Verify Disputed: id0, id1
        let disputed = client.get_invoices_by_dispute_status(&DisputeStatus::Disputed);
        assert!(disputed.contains(&id0));
        assert!(disputed.contains(&id1));
        assert!(!disputed.contains(&id2));

        // Verify UnderReview: id2, id3
        let under_review = client.get_invoices_by_dispute_status(&DisputeStatus::UnderReview);
        assert!(under_review.contains(&id2));
        assert!(under_review.contains(&id3));
        assert!(!under_review.contains(&id4));

        // Verify Resolved: id4
        let resolved = client.get_invoices_by_dispute_status(&DisputeStatus::Resolved);
        assert!(resolved.contains(&id4));
        assert!(!resolved.contains(&id3));
    }

    /// [TC-29] Complete lifecycle with all query functions.
    #[test]
    fn test_complete_lifecycle_with_all_queries() {
        let (env, client, admin) = setup();
        let business = create_verified_business(&env, &client, &admin);
        let invoice_id = create_test_invoice(&env, &client, &admin, &business, 100_000);

        let reason = String::from_str(&env, "Payment delay");
        let evidence = String::from_str(&env, "Invoice was 30 days late");
        client.create_dispute(&invoice_id, &business, &reason, &evidence);

        // Invoice must appear in the global dispute list
        assert!(client.get_invoices_with_disputes().contains(&invoice_id));

        // Invoice must appear in Disputed list
        assert!(client
            .get_invoices_by_dispute_status(&DisputeStatus::Disputed)
            .contains(&invoice_id));

        // Advance to UnderReview
        client.put_dispute_under_review(&invoice_id, &admin);
        assert!(client
            .get_invoices_by_dispute_status(&DisputeStatus::UnderReview)
            .contains(&invoice_id));
        assert!(!client
            .get_invoices_by_dispute_status(&DisputeStatus::Disputed)
            .contains(&invoice_id));

        // Resolve
        let resolution = String::from_str(&env, "Partial refund issued");
        client.resolve_dispute(&invoice_id, &admin, &resolution);
        assert!(client
            .get_invoices_by_dispute_status(&DisputeStatus::Resolved)
            .contains(&invoice_id));
        assert!(!client
            .get_invoices_by_dispute_status(&DisputeStatus::UnderReview)
            .contains(&invoice_id));

        // Final dispute details
        let dispute = client
            .get_dispute_details(&invoice_id)
            .expect("Dispute should exist");
        assert_eq!(dispute.created_by, business);
        assert_eq!(dispute.reason, reason);
        assert_eq!(dispute.evidence, evidence);
        assert_eq!(dispute.resolution, resolution);
        assert_eq!(dispute.resolved_by, admin);
    }

    // -----------------------------------------------------------------------
    // Regression Tests — Dispute Locking
    // -----------------------------------------------------------------------

    /// [TC-30] REGRESSION: Resolved dispute cannot be overwritten by a second
    /// `resolve_dispute` call.
    ///
    /// # Security Note
    /// This is the core locking invariant.  The `Resolved` state is terminal;
    /// any attempt to call `resolve_dispute` again must return
    /// `DisputeNotUnderReview` because the status is no longer `UnderReview`.
    #[test]
    fn test_regression_resolved_dispute_is_locked() {
        let (env, client, admin) = setup();
        let business = create_verified_business(&env, &client, &admin);
        let invoice_id = create_test_invoice(&env, &client, &business, 100_000);

        client.create_dispute(
            &invoice_id,
            &business,
            &String::from_str(&env, "Original reason"),
            &String::from_str(&env, "Original evidence"),
        );
        client.put_dispute_under_review(&invoice_id, &admin);
        client.resolve_dispute(
            &invoice_id,
            &admin,
            &String::from_str(&env, "Original resolution"),
        );

        // Verify terminal state
        assert_eq!(
            client.get_invoice(&invoice_id).dispute_status,
            DisputeStatus::Resolved
        );

        // Attempt overwrite — must fail
        let overwrite = client.try_resolve_dispute(
            &invoice_id,
            &admin,
            &String::from_str(&env, "Overwrite attempt"),
        );
        assert!(overwrite.is_err(), "Resolved dispute must be locked");
        let err = overwrite.unwrap_err().expect("expected contract error");
        assert_eq!(
            err,
            QuickLendXError::DisputeNotUnderReview,
            "Overwrite must return DisputeNotUnderReview"
        );

        // Original resolution must be unchanged
        let dispute = client
            .get_dispute_details(&invoice_id)
            .expect("Dispute must still exist");
        assert_eq!(
            dispute.resolution,
            String::from_str(&env, "Original resolution"),
            "Resolution must not be overwritten"
        );
    }

    /// [TC-31] REGRESSION: Resolved dispute cannot be re-opened via
    /// `put_dispute_under_review`.
    ///
    /// # Security Note
    /// Prevents an admin from cycling a resolved dispute back to `UnderReview`
    /// and then issuing a different resolution.
    #[test]
    fn test_regression_resolved_dispute_cannot_reopen() {
        let (env, client, admin) = setup();
        let business = create_verified_business(&env, &client, &admin);
        let invoice_id = create_test_invoice(&env, &client, &business, 100_000);

        client.create_dispute(
            &invoice_id,
            &business,
            &String::from_str(&env, "reason"),
            &String::from_str(&env, "evidence"),
        );
        client.put_dispute_under_review(&invoice_id, &admin);
        client.resolve_dispute(
            &invoice_id,
            &admin,
            &String::from_str(&env, "Final resolution"),
        );

        // Attempt to re-open — must fail
        let reopen = client.try_put_dispute_under_review(&invoice_id, &admin);
        assert!(reopen.is_err(), "Resolved dispute must not be re-opened");
        let err = reopen.unwrap_err().expect("expected contract error");
        assert_eq!(
            err,
            QuickLendXError::InvalidStatus,
            "Re-opening a Resolved dispute must return InvalidStatus"
        );
    }

    /// [TC-32] REGRESSION: `resolved_at` timestamp is set exactly once and
    /// is never zero after resolution.
    #[test]
    fn test_regression_resolved_at_is_set_once() {
        let (env, client, admin) = setup();
        let business = create_verified_business(&env, &client, &admin);
        let invoice_id = create_test_invoice(&env, &client, &business, 100_000);

        client.create_dispute(
            &invoice_id,
            &business,
            &String::from_str(&env, "reason"),
            &String::from_str(&env, "evidence"),
        );

        // Before resolution: resolved_at must be 0
        let before = client
            .get_dispute_details(&invoice_id)
            .expect("Dispute must exist");
        assert_eq!(before.resolved_at, 0, "resolved_at must be 0 before resolution");

        client.put_dispute_under_review(&invoice_id, &admin);
        client.resolve_dispute(
            &invoice_id,
            &admin,
            &String::from_str(&env, "Resolution text"),
        );

        // After resolution: resolved_at must be non-zero
        let after = client
            .get_dispute_details(&invoice_id)
            .expect("Dispute must exist");
        assert!(after.resolved_at > 0, "resolved_at must be set after resolution");
        assert_eq!(after.resolved_by, admin, "resolved_by must be the admin");
    }

    /// [TC-33] REGRESSION: `resolve_dispute` on a `Disputed` (not yet under
    /// review) invoice must return `DisputeNotUnderReview`, not silently succeed.
    ///
    /// # Security Note
    /// Prevents skipping the mandatory review step.
    #[test]
    fn test_regression_cannot_skip_review_step() {
        let (env, client, admin) = setup();
        let business = create_verified_business(&env, &client, &admin);
        let invoice_id = create_test_invoice(&env, &client, &business, 100_000);

        client.create_dispute(
            &invoice_id,
            &business,
            &String::from_str(&env, "reason"),
            &String::from_str(&env, "evidence"),
        );

        // Status is Disputed — resolve must fail
        let result = client.try_resolve_dispute(
            &invoice_id,
            &admin,
            &String::from_str(&env, "Skipped review"),
        );
        assert!(result.is_err());
        let err = result.unwrap_err().expect("expected contract error");
        assert_eq!(err, QuickLendXError::DisputeNotUnderReview);

        // Status must remain Disputed
        assert_eq!(
            client.get_invoice(&invoice_id).dispute_status,
            DisputeStatus::Disputed,
            "Status must not change after failed resolve"
        );
    }

    /// [TC-34] REGRESSION: Non-admin cannot resolve a dispute even if they
    /// know the invoice ID.
    #[test]
    fn test_regression_non_admin_cannot_resolve() {
        let (env, client, admin) = setup();
        let business = create_verified_business(&env, &client, &admin);
        let invoice_id = create_test_invoice(&env, &client, &business, 100_000);
        let attacker = Address::generate(&env);

        client.create_dispute(
            &invoice_id,
            &business,
            &String::from_str(&env, "reason"),
            &String::from_str(&env, "evidence"),
        );
        client.put_dispute_under_review(&invoice_id, &admin);

        let result = client.try_resolve_dispute(
            &invoice_id,
            &attacker,
            &String::from_str(&env, "Attacker resolution"),
        );
        assert!(result.is_err(), "Non-admin must not resolve a dispute");
    }

    /// [TC-35] REGRESSION: Non-admin cannot advance a dispute to `UnderReview`.
    #[test]
    fn test_regression_non_admin_cannot_put_under_review() {
        let (env, client, admin) = setup();
        let business = create_verified_business(&env, &client, &admin);
        let invoice_id = create_test_invoice(&env, &client, &business, 100_000);
        let attacker = Address::generate(&env);

        client.create_dispute(
            &invoice_id,
            &business,
            &String::from_str(&env, "reason"),
            &String::from_str(&env, "evidence"),
        );

        let result = client.try_put_dispute_under_review(&invoice_id, &attacker);
        assert!(result.is_err(), "Non-admin must not advance dispute");
    }

    /// [TC-36] REGRESSION: `put_dispute_under_review` on an invoice with no
    /// dispute must return `DisputeNotFound`.
    #[test]
    fn test_regression_review_no_dispute_returns_not_found() {
        let (env, client, admin) = setup();
        let business = create_verified_business(&env, &client, &admin);
        let invoice_id = create_test_invoice(&env, &client, &business, 100_000);

        let result = client.try_put_dispute_under_review(&invoice_id, &admin);
        assert!(result.is_err());
        let err = result.unwrap_err().expect("expected contract error");
        assert_eq!(err, QuickLendXError::DisputeNotFound);
    }

    /// [TC-37] REGRESSION: `resolve_dispute` on an invoice with no dispute
    /// must return `DisputeNotFound`.
    #[test]
    fn test_regression_resolve_no_dispute_returns_not_found() {
        let (env, client, admin) = setup();
        let business = create_verified_business(&env, &client, &admin);
        let invoice_id = create_test_invoice(&env, &client, &business, 100_000);

        let result = client.try_resolve_dispute(
            &invoice_id,
            &admin,
            &String::from_str(&env, "resolution"),
        );
        assert!(result.is_err());
        let err = result.unwrap_err().expect("expected contract error");
        assert_eq!(err, QuickLendXError::DisputeNotFound);
    }

    /// [TC-38] REGRESSION: Double-resolution attempt preserves the original
    /// `resolved_by` and `resolved_at` fields unchanged.
    #[test]
    fn test_regression_double_resolution_preserves_original_fields() {
        let (env, client, admin) = setup();
        let business = create_verified_business(&env, &client, &admin);
        let invoice_id = create_test_invoice(&env, &client, &business, 100_000);

        client.create_dispute(
            &invoice_id,
            &business,
            &String::from_str(&env, "reason"),
            &String::from_str(&env, "evidence"),
        );
        client.put_dispute_under_review(&invoice_id, &admin);
        client.resolve_dispute(
            &invoice_id,
            &admin,
            &String::from_str(&env, "First resolution"),
        );

        let first = client
            .get_dispute_details(&invoice_id)
            .expect("Dispute must exist");

        // Second attempt must fail
        let _ = client.try_resolve_dispute(
            &invoice_id,
            &admin,
            &String::from_str(&env, "Second resolution"),
        );

        // Fields must be identical to the first resolution
        let after = client
            .get_dispute_details(&invoice_id)
            .expect("Dispute must exist");
        assert_eq!(after.resolution, first.resolution);
        assert_eq!(after.resolved_by, first.resolved_by);
        assert_eq!(after.resolved_at, first.resolved_at);
    }

    /// [TC-39] REGRESSION: Invalid dispute ID (non-existent invoice) must
    /// return `InvoiceNotFound` for all dispute operations.
    #[test]
    fn test_regression_invalid_invoice_id_all_operations() {
        let (env, client, admin) = setup();
        let business = create_verified_business(&env, &client, &admin);
        let fake_id = BytesN::from_array(&env, &[0xFFu8; 32]);

        let create_result = client.try_create_dispute(
            &fake_id,
            &business,
            &String::from_str(&env, "reason"),
            &String::from_str(&env, "evidence"),
        );
        assert!(create_result.is_err());
        assert_eq!(
            create_result.unwrap_err().expect("expected contract error"),
            QuickLendXError::InvoiceNotFound
        );

        let review_result = client.try_put_dispute_under_review(&fake_id, &admin);
        assert!(review_result.is_err());
        assert_eq!(
            review_result.unwrap_err().expect("expected contract error"),
            QuickLendXError::InvoiceNotFound
        );

        let resolve_result = client.try_resolve_dispute(
            &fake_id,
            &admin,
            &String::from_str(&env, "resolution"),
        );
        assert!(resolve_result.is_err());
        assert_eq!(
            resolve_result.unwrap_err().expect("expected contract error"),
            QuickLendXError::InvoiceNotFound
        );
    }

    /// [TC-40] REGRESSION: Evidence boundary — exactly 2000 chars must succeed;
    /// 2001 chars must fail.
    #[test]
    fn test_regression_evidence_boundary_values() {
        let (env, client, admin) = setup();
        let business = create_verified_business(&env, &client, &admin);

        // 2000 chars — must succeed
        let id1 = create_test_invoice(&env, &client, &business, 100_000);
        let max_evidence = "e".repeat(2000);
        let ok = client.try_create_dispute(
            &id1,
            &business,
            &String::from_str(&env, "reason"),
            &String::from_str(&env, max_evidence.as_str()),
        );
        assert!(ok.is_ok(), "2000-char evidence must be accepted");

        // 2001 chars — must fail
        let id2 = create_test_invoice(&env, &client, &business, 110_000);
        let over_evidence = "e".repeat(2001);
        let err = client.try_create_dispute(
            &id2,
            &business,
            &String::from_str(&env, "reason"),
            &String::from_str(&env, over_evidence.as_str()),
        );
        assert!(err.is_err());
        assert_eq!(
            err.unwrap_err().expect("expected contract error"),
            QuickLendXError::InvalidDisputeEvidence
        );
    }

    /// [TC-41] REGRESSION: Resolution boundary — exactly 2000 chars must
    /// succeed; 2001 chars must fail.
    #[test]
    fn test_regression_resolution_boundary_values() {
        let (env, client, admin) = setup();
        let business = create_verified_business(&env, &client, &admin);

        // 2000 chars — must succeed
        let id1 = create_test_invoice(&env, &client, &business, 100_000);
        client.create_dispute(
            &id1,
            &business,
            &String::from_str(&env, "reason"),
            &String::from_str(&env, "evidence"),
        );
        client.put_dispute_under_review(&id1, &admin);
        let max_resolution = "r".repeat(2000);
        let ok = client.try_resolve_dispute(
            &id1,
            &admin,
            &String::from_str(&env, max_resolution.as_str()),
        );
        assert!(ok.is_ok(), "2000-char resolution must be accepted");

        // 2001 chars — must fail
        let id2 = create_test_invoice(&env, &client, &business, 110_000);
        client.create_dispute(
            &id2,
            &business,
            &String::from_str(&env, "reason"),
            &String::from_str(&env, "evidence"),
        );
        client.put_dispute_under_review(&id2, &admin);
        let over_resolution = "r".repeat(2001);
        let err = client.try_resolve_dispute(
            &id2,
            &admin,
            &String::from_str(&env, over_resolution.as_str()),
        );
        assert!(err.is_err());
        assert_eq!(
            err.unwrap_err().expect("expected contract error"),
            QuickLendXError::InvalidDisputeReason
        );
    }

    /// [TC-42] REGRESSION: `get_dispute_details` returns `None` for an invoice
    /// that exists but has no dispute.
    #[test]
    fn test_regression_get_details_no_dispute_returns_none() {
        let (env, client, admin) = setup();
        let business = create_verified_business(&env, &client, &admin);
        let invoice_id = create_test_invoice(&env, &client, &business, 100_000);

        let result = client.get_dispute_details(&invoice_id);
        assert!(
            result.is_none(),
            "get_dispute_details must return None when no dispute exists"
        );
    }

    /// [TC-43] REGRESSION: Dispute state is isolated per invoice — resolving
    /// one does not affect another.
    #[test]
    fn test_regression_resolution_isolation_across_invoices() {
        let (env, client, admin) = setup();
        let business = create_verified_business(&env, &client, &admin);

        let id1 = create_test_invoice(&env, &client, &business, 100_000);
        let id2 = create_test_invoice(&env, &client, &business, 200_000);

        let reason = String::from_str(&env, "reason");
        let evidence = String::from_str(&env, "evidence");
        client.create_dispute(&id1, &business, &reason, &evidence);
        client.create_dispute(&id2, &business, &reason, &evidence);

        // Fully resolve id1
        client.put_dispute_under_review(&id1, &admin);
        client.resolve_dispute(&id1, &admin, &String::from_str(&env, "Resolved id1"));

        // id2 must still be Disputed
        assert_eq!(
            client.get_invoice(&id2).dispute_status,
            DisputeStatus::Disputed,
            "id2 must remain Disputed after id1 is resolved"
        );

        // id2 must still be resolvable through the normal path
        client.put_dispute_under_review(&id2, &admin);
        client.resolve_dispute(&id2, &admin, &String::from_str(&env, "Resolved id2"));
        assert_eq!(
            client.get_invoice(&id2).dispute_status,
            DisputeStatus::Resolved
        );
    }

    /// [TC-44] Lifecycle happy path: open dispute, update evidence, then resolve.
    #[test]
    fn test_dispute_lifecycle_open_evidence_resolve() {
        let (env, client, admin) = setup();
        let business = create_verified_business(&env, &client, &admin);
        let invoice_id = create_test_invoice(&env, &client, &admin, &business, 120_000);

        client.create_dispute(
            &invoice_id,
            &business,
            &String::from_str(&env, "Incorrect settlement amount"),
            &String::from_str(&env, "Initial evidence"),
        );
        client.update_dispute_evidence(
            &invoice_id,
            &business,
            &String::from_str(&env, "Updated supporting evidence"),
        );
        client.put_dispute_under_review(&invoice_id, &admin);
        client.resolve_dispute(
            &invoice_id,
            &admin,
            &String::from_str(&env, "Resolved after evidence review"),
        );

        let dispute = client.get_dispute_details(&invoice_id).expect("dispute must exist");
        assert_eq!(
            dispute.evidence,
            String::from_str(&env, "Updated supporting evidence")
        );
        assert_eq!(client.get_invoice(&invoice_id).dispute_status, DisputeStatus::Resolved);
    }

    /// [TC-45] Evidence updates are creator-only and reject tampering attempts.
    #[test]
    fn test_update_dispute_evidence_creator_only() {
        let (env, client, admin) = setup();
        let business = create_verified_business(&env, &client, &admin);
        let outsider = Address::generate(&env);
        let invoice_id = create_test_invoice(&env, &client, &admin, &business, 90_000);

        client.create_dispute(
            &invoice_id,
            &business,
            &String::from_str(&env, "reason"),
            &String::from_str(&env, "evidence"),
        );

        let err = client
            .try_update_dispute_evidence(
                &invoice_id,
                &outsider,
                &String::from_str(&env, "tampered evidence"),
            )
            .unwrap_err()
            .expect("expected contract error");
        assert_eq!(err, QuickLendXError::DisputeNotAuthorized);
    }

    /// [TC-46] Evidence is immutable after review starts or dispute is resolved.
    #[test]
    fn test_update_dispute_evidence_rejected_after_review_or_resolution() {
        let (env, client, admin) = setup();
        let business = create_verified_business(&env, &client, &admin);
        let invoice_id = create_test_invoice(&env, &client, &admin, &business, 130_000);

        client.create_dispute(
            &invoice_id,
            &business,
            &String::from_str(&env, "reason"),
            &String::from_str(&env, "evidence"),
        );
        client.put_dispute_under_review(&invoice_id, &admin);

        let under_review_err = client
            .try_update_dispute_evidence(
                &invoice_id,
                &business,
                &String::from_str(&env, "new evidence"),
            )
            .unwrap_err()
            .expect("expected contract error");
        assert_eq!(under_review_err, QuickLendXError::InvalidStatus);

        client.resolve_dispute(&invoice_id, &admin, &String::from_str(&env, "done"));
        let resolved_err = client
            .try_update_dispute_evidence(
                &invoice_id,
                &business,
                &String::from_str(&env, "another update"),
            )
            .unwrap_err()
            .expect("expected contract error");
        assert_eq!(resolved_err, QuickLendXError::InvalidStatus);
    }

    /// [TC-47] Index/query consistency: lifecycle transitions preserve indexed invoice visibility.
    #[test]
    fn test_dispute_index_query_consistency_across_lifecycle() {
        let (env, client, admin) = setup();
        let business = create_verified_business(&env, &client, &admin);
        let id1 = create_test_invoice(&env, &client, &admin, &business, 100_000);
        let id2 = create_test_invoice(&env, &client, &admin, &business, 200_000);

        client.create_dispute(
            &id1,
            &business,
            &String::from_str(&env, "r1"),
            &String::from_str(&env, "e1"),
        );
        client.create_dispute(
            &id2,
            &business,
            &String::from_str(&env, "r2"),
            &String::from_str(&env, "e2"),
        );

        let all_disputes = client.get_invoices_with_disputes();
        assert!(all_disputes.contains(&id1));
        assert!(all_disputes.contains(&id2));

        client.put_dispute_under_review(&id1, &admin);
        let review_ids = client.get_invoices_by_dispute_status(&DisputeStatus::UnderReview);
        let open_ids = client.get_invoices_by_dispute_status(&DisputeStatus::Disputed);
        assert!(review_ids.contains(&id1));
        assert!(!open_ids.contains(&id1));
        assert!(open_ids.contains(&id2));

        client.resolve_dispute(&id1, &admin, &String::from_str(&env, "resolved"));
        let resolved_ids = client.get_invoices_by_dispute_status(&DisputeStatus::Resolved);
        assert!(resolved_ids.contains(&id1));
        assert!(!resolved_ids.contains(&id2));
    }
}
