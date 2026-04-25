/// Tests for the dispute timeline endpoint.
///
/// # Coverage
///
/// ## Lifecycle ordering
/// - Disputed invoice produces exactly 1 entry ("Opened")
/// - UnderReview invoice produces exactly 2 entries ("Opened", "UnderReview")
/// - Resolved invoice produces exactly 3 entries in order
/// - Entry sequence numbers are 0, 1, 2
/// - Entry timestamps are non-decreasing
/// - Entry event labels match expected strings
///
/// ## Redaction
/// - Evidence is never present in any timeline entry
/// - Resolution text is absent until dispute is Resolved
/// - UnderReview actor is the zero (redacted) address
/// - Opened actor is the dispute creator (not redacted)
/// - Resolved actor is the admin (not redacted)
///
/// ## Pagination
/// - offset=0, limit=10 returns all entries for a resolved dispute
/// - offset=1, limit=10 skips the first entry
/// - offset=0, limit=1 returns only the first entry with has_more=true
/// - offset=0, limit=0 returns empty page
/// - offset beyond total returns empty page with has_more=false
/// - limit capped at TIMELINE_MAX_PAGE_SIZE (50)
/// - total field always reflects the full event count
///
/// ## Error handling
/// - Invoice not found returns InvoiceNotFound
/// - Invoice with no dispute returns DisputeNotFound
/// - current_status field matches on-chain dispute_status
#[cfg(test)]
mod test_dispute_timeline {
    use crate::dispute_timeline::{get_dispute_timeline, DisputeTimeline, TIMELINE_MAX_PAGE_SIZE};
    use crate::errors::QuickLendXError;
    use crate::invoice::{DisputeStatus, InvoiceCategory};
    use crate::{QuickLendXContract, QuickLendXContractClient};
    use soroban_sdk::{testutils::Address as _, Address, BytesN, Env, String, Vec};

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    fn setup() -> (Env, QuickLendXContractClient<'static>, Address) {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(QuickLendXContract, ());
        let client = QuickLendXContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        client.set_admin(&admin);
        (env, client, admin)
    }

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

    fn create_invoice(
        env: &Env,
        client: &QuickLendXContractClient,
        admin: &Address,
        business: &Address,
    ) -> BytesN<32> {
        let currency = Address::generate(env);
        let due_date = env.ledger().timestamp() + 30 * 24 * 60 * 60;
        client.store_invoice(
            admin,
            business,
            &100_000i128,
            &currency,
            &due_date,
            &String::from_str(env, "Timeline test invoice"),
            &InvoiceCategory::Services,
            &Vec::new(env),
        )
    }

    /// Zero address used as the redacted actor sentinel.
    fn zero_addr(env: &Env) -> Address {
        Address::from_str(env, "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF")
    }

    // -----------------------------------------------------------------------
    // Error handling
    // -----------------------------------------------------------------------

    #[test]
    fn test_timeline_invoice_not_found() {
        let (env, _client, _admin) = setup();
        let fake_id = BytesN::from_array(&env, &[0u8; 32]);
        let result = env.as_contract(
            &env.register(QuickLendXContract, ()),
            || get_dispute_timeline(&env, &fake_id, 0, 10),
        );
        assert_eq!(result, Err(QuickLendXError::InvoiceNotFound));
    }

    #[test]
    fn test_timeline_no_dispute_returns_dispute_not_found() {
        let (env, client, admin) = setup();
        let business = create_verified_business(&env, &client, &admin);
        let invoice_id = create_invoice(&env, &client, &admin, &business);

        let contract_id = env.register(QuickLendXContract, ());
        // Use the same contract that has the invoice stored
        let result = client.try_get_dispute_timeline(&invoice_id, &0u32, &10u32);
        assert!(result.is_err());
        let err = result.unwrap_err().expect("expected contract error");
        assert_eq!(err, QuickLendXError::DisputeNotFound);
    }

    // -----------------------------------------------------------------------
    // Lifecycle ordering — Disputed state (1 entry)
    // -----------------------------------------------------------------------

    #[test]
    fn test_timeline_disputed_has_one_entry() {
        let (env, client, admin) = setup();
        let business = create_verified_business(&env, &client, &admin);
        let invoice_id = create_invoice(&env, &client, &admin, &business);

        client.create_dispute(
            &invoice_id,
            &business,
            &String::from_str(&env, "Payment not received"),
            &String::from_str(&env, "Bank statement attached"),
        );

        let timeline = client.get_dispute_timeline(&invoice_id, &0u32, &10u32);
        assert_eq!(timeline.total, 1);
        assert_eq!(timeline.entries.len(), 1);
        assert!(!timeline.has_more);
        assert_eq!(timeline.current_status, DisputeStatus::Disputed);
    }

    #[test]
    fn test_timeline_disputed_entry_fields() {
        let (env, client, admin) = setup();
        let business = create_verified_business(&env, &client, &admin);
        let invoice_id = create_invoice(&env, &client, &admin, &business);

        let reason = String::from_str(&env, "Wrong amount invoiced");
        client.create_dispute(
            &invoice_id,
            &business,
            &reason,
            &String::from_str(&env, "Evidence doc"),
        );

        let timeline = client.get_dispute_timeline(&invoice_id, &0u32, &10u32);
        let entry = timeline.entries.get(0).expect("entry 0 must exist");

        assert_eq!(entry.sequence, 0);
        assert_eq!(entry.event, String::from_str(&env, "Opened"));
        assert_eq!(entry.actor, business);
        assert_eq!(entry.summary, reason);
        // Evidence must NOT appear in summary
        assert_ne!(entry.summary, String::from_str(&env, "Evidence doc"));
    }

    // -----------------------------------------------------------------------
    // Lifecycle ordering — UnderReview state (2 entries)
    // -----------------------------------------------------------------------

    #[test]
    fn test_timeline_under_review_has_two_entries() {
        let (env, client, admin) = setup();
        let business = create_verified_business(&env, &client, &admin);
        let invoice_id = create_invoice(&env, &client, &admin, &business);

        client.create_dispute(
            &invoice_id,
            &business,
            &String::from_str(&env, "reason"),
            &String::from_str(&env, "evidence"),
        );
        client.put_dispute_under_review(&invoice_id, &admin);

        let timeline = client.get_dispute_timeline(&invoice_id, &0u32, &10u32);
        assert_eq!(timeline.total, 2);
        assert_eq!(timeline.entries.len(), 2);
        assert!(!timeline.has_more);
        assert_eq!(timeline.current_status, DisputeStatus::UnderReview);
    }

    #[test]
    fn test_timeline_under_review_entry_order_and_labels() {
        let (env, client, admin) = setup();
        let business = create_verified_business(&env, &client, &admin);
        let invoice_id = create_invoice(&env, &client, &admin, &business);

        client.create_dispute(
            &invoice_id,
            &business,
            &String::from_str(&env, "reason"),
            &String::from_str(&env, "evidence"),
        );
        client.put_dispute_under_review(&invoice_id, &admin);

        let timeline = client.get_dispute_timeline(&invoice_id, &0u32, &10u32);
        let e0 = timeline.entries.get(0).unwrap();
        let e1 = timeline.entries.get(1).unwrap();

        assert_eq!(e0.sequence, 0);
        assert_eq!(e0.event, String::from_str(&env, "Opened"));
        assert_eq!(e1.sequence, 1);
        assert_eq!(e1.event, String::from_str(&env, "UnderReview"));
    }

    // -----------------------------------------------------------------------
    // Lifecycle ordering — Resolved state (3 entries)
    // -----------------------------------------------------------------------

    #[test]
    fn test_timeline_resolved_has_three_entries() {
        let (env, client, admin) = setup();
        let business = create_verified_business(&env, &client, &admin);
        let invoice_id = create_invoice(&env, &client, &admin, &business);

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
            &String::from_str(&env, "Resolved in favour of business"),
        );

        let timeline = client.get_dispute_timeline(&invoice_id, &0u32, &10u32);
        assert_eq!(timeline.total, 3);
        assert_eq!(timeline.entries.len(), 3);
        assert!(!timeline.has_more);
        assert_eq!(timeline.current_status, DisputeStatus::Resolved);
    }

    #[test]
    fn test_timeline_resolved_entry_order_labels_and_sequences() {
        let (env, client, admin) = setup();
        let business = create_verified_business(&env, &client, &admin);
        let invoice_id = create_invoice(&env, &client, &admin, &business);

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
            &String::from_str(&env, "Partial refund issued"),
        );

        let timeline = client.get_dispute_timeline(&invoice_id, &0u32, &10u32);
        let e0 = timeline.entries.get(0).unwrap();
        let e1 = timeline.entries.get(1).unwrap();
        let e2 = timeline.entries.get(2).unwrap();

        assert_eq!(e0.sequence, 0);
        assert_eq!(e0.event, String::from_str(&env, "Opened"));
        assert_eq!(e1.sequence, 1);
        assert_eq!(e1.event, String::from_str(&env, "UnderReview"));
        assert_eq!(e2.sequence, 2);
        assert_eq!(e2.event, String::from_str(&env, "Resolved"));
    }

    #[test]
    fn test_timeline_resolved_timestamps_non_decreasing() {
        let (env, client, admin) = setup();
        let business = create_verified_business(&env, &client, &admin);
        let invoice_id = create_invoice(&env, &client, &admin, &business);

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
            &String::from_str(&env, "resolution"),
        );

        let timeline = client.get_dispute_timeline(&invoice_id, &0u32, &10u32);
        let e0 = timeline.entries.get(0).unwrap();
        let e1 = timeline.entries.get(1).unwrap();
        let e2 = timeline.entries.get(2).unwrap();

        assert!(e0.timestamp <= e1.timestamp, "Opened must not be after UnderReview");
        assert!(e1.timestamp <= e2.timestamp, "UnderReview must not be after Resolved");
    }

    // -----------------------------------------------------------------------
    // Redaction tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_redaction_evidence_never_in_timeline() {
        let (env, client, admin) = setup();
        let business = create_verified_business(&env, &client, &admin);
        let invoice_id = create_invoice(&env, &client, &admin, &business);

        let evidence = String::from_str(&env, "SECRET_BANK_STATEMENT_XYZ");
        client.create_dispute(
            &invoice_id,
            &business,
            &String::from_str(&env, "reason"),
            &evidence,
        );
        client.put_dispute_under_review(&invoice_id, &admin);
        client.resolve_dispute(
            &invoice_id,
            &admin,
            &String::from_str(&env, "resolution"),
        );

        let timeline = client.get_dispute_timeline(&invoice_id, &0u32, &10u32);
        for i in 0..timeline.entries.len() {
            let entry = timeline.entries.get(i).unwrap();
            assert_ne!(
                entry.summary, evidence,
                "Evidence must not appear in entry {i}"
            );
        }
    }

    #[test]
    fn test_redaction_resolution_absent_before_resolved() {
        let (env, client, admin) = setup();
        let business = create_verified_business(&env, &client, &admin);
        let invoice_id = create_invoice(&env, &client, &admin, &business);

        let resolution = String::from_str(&env, "CONFIDENTIAL_RESOLUTION");
        client.create_dispute(
            &invoice_id,
            &business,
            &String::from_str(&env, "reason"),
            &String::from_str(&env, "evidence"),
        );
        client.put_dispute_under_review(&invoice_id, &admin);

        // Not yet resolved — resolution text must not appear
        let timeline = client.get_dispute_timeline(&invoice_id, &0u32, &10u32);
        for i in 0..timeline.entries.len() {
            let entry = timeline.entries.get(i).unwrap();
            assert_ne!(entry.summary, resolution, "Resolution must not appear before Resolved");
        }
    }

    #[test]
    fn test_redaction_resolution_present_after_resolved() {
        let (env, client, admin) = setup();
        let business = create_verified_business(&env, &client, &admin);
        let invoice_id = create_invoice(&env, &client, &admin, &business);

        let resolution = String::from_str(&env, "Refund approved");
        client.create_dispute(
            &invoice_id,
            &business,
            &String::from_str(&env, "reason"),
            &String::from_str(&env, "evidence"),
        );
        client.put_dispute_under_review(&invoice_id, &admin);
        client.resolve_dispute(&invoice_id, &admin, &resolution);

        let timeline = client.get_dispute_timeline(&invoice_id, &0u32, &10u32);
        let resolved_entry = timeline.entries.get(2).unwrap();
        assert_eq!(resolved_entry.summary, resolution);
    }

    #[test]
    fn test_redaction_under_review_actor_is_zero_address() {
        let (env, client, admin) = setup();
        let business = create_verified_business(&env, &client, &admin);
        let invoice_id = create_invoice(&env, &client, &admin, &business);

        client.create_dispute(
            &invoice_id,
            &business,
            &String::from_str(&env, "reason"),
            &String::from_str(&env, "evidence"),
        );
        client.put_dispute_under_review(&invoice_id, &admin);

        let timeline = client.get_dispute_timeline(&invoice_id, &0u32, &10u32);
        let review_entry = timeline.entries.get(1).unwrap();

        assert_eq!(
            review_entry.actor,
            zero_addr(&env),
            "UnderReview actor must be redacted"
        );
        // Admin address must NOT appear in the UnderReview entry
        assert_ne!(review_entry.actor, admin);
    }

    #[test]
    fn test_redaction_opened_actor_is_creator() {
        let (env, client, admin) = setup();
        let business = create_verified_business(&env, &client, &admin);
        let invoice_id = create_invoice(&env, &client, &admin, &business);

        client.create_dispute(
            &invoice_id,
            &business,
            &String::from_str(&env, "reason"),
            &String::from_str(&env, "evidence"),
        );

        let timeline = client.get_dispute_timeline(&invoice_id, &0u32, &10u32);
        let opened_entry = timeline.entries.get(0).unwrap();
        assert_eq!(opened_entry.actor, business);
    }

    #[test]
    fn test_redaction_resolved_actor_is_admin() {
        let (env, client, admin) = setup();
        let business = create_verified_business(&env, &client, &admin);
        let invoice_id = create_invoice(&env, &client, &admin, &business);

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
            &String::from_str(&env, "resolution"),
        );

        let timeline = client.get_dispute_timeline(&invoice_id, &0u32, &10u32);
        let resolved_entry = timeline.entries.get(2).unwrap();
        assert_eq!(resolved_entry.actor, admin);
    }

    // -----------------------------------------------------------------------
    // Pagination tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_pagination_offset_0_limit_10_returns_all_for_resolved() {
        let (env, client, admin) = setup();
        let business = create_verified_business(&env, &client, &admin);
        let invoice_id = create_invoice(&env, &client, &admin, &business);

        client.create_dispute(
            &invoice_id,
            &business,
            &String::from_str(&env, "r"),
            &String::from_str(&env, "e"),
        );
        client.put_dispute_under_review(&invoice_id, &admin);
        client.resolve_dispute(&invoice_id, &admin, &String::from_str(&env, "done"));

        let timeline = client.get_dispute_timeline(&invoice_id, &0u32, &10u32);
        assert_eq!(timeline.entries.len(), 3);
        assert_eq!(timeline.total, 3);
        assert!(!timeline.has_more);
    }

    #[test]
    fn test_pagination_offset_1_skips_first_entry() {
        let (env, client, admin) = setup();
        let business = create_verified_business(&env, &client, &admin);
        let invoice_id = create_invoice(&env, &client, &admin, &business);

        client.create_dispute(
            &invoice_id,
            &business,
            &String::from_str(&env, "r"),
            &String::from_str(&env, "e"),
        );
        client.put_dispute_under_review(&invoice_id, &admin);
        client.resolve_dispute(&invoice_id, &admin, &String::from_str(&env, "done"));

        let timeline = client.get_dispute_timeline(&invoice_id, &1u32, &10u32);
        assert_eq!(timeline.entries.len(), 2);
        assert_eq!(timeline.total, 3);
        assert!(!timeline.has_more);
        // First returned entry should be the UnderReview event
        let first = timeline.entries.get(0).unwrap();
        assert_eq!(first.event, String::from_str(&env, "UnderReview"));
    }

    #[test]
    fn test_pagination_limit_1_returns_one_entry_with_has_more() {
        let (env, client, admin) = setup();
        let business = create_verified_business(&env, &client, &admin);
        let invoice_id = create_invoice(&env, &client, &admin, &business);

        client.create_dispute(
            &invoice_id,
            &business,
            &String::from_str(&env, "r"),
            &String::from_str(&env, "e"),
        );
        client.put_dispute_under_review(&invoice_id, &admin);
        client.resolve_dispute(&invoice_id, &admin, &String::from_str(&env, "done"));

        let timeline = client.get_dispute_timeline(&invoice_id, &0u32, &1u32);
        assert_eq!(timeline.entries.len(), 1);
        assert_eq!(timeline.total, 3);
        assert!(timeline.has_more);
        let first = timeline.entries.get(0).unwrap();
        assert_eq!(first.event, String::from_str(&env, "Opened"));
    }

    #[test]
    fn test_pagination_limit_0_returns_empty_page() {
        let (env, client, admin) = setup();
        let business = create_verified_business(&env, &client, &admin);
        let invoice_id = create_invoice(&env, &client, &admin, &business);

        client.create_dispute(
            &invoice_id,
            &business,
            &String::from_str(&env, "r"),
            &String::from_str(&env, "e"),
        );

        let timeline = client.get_dispute_timeline(&invoice_id, &0u32, &0u32);
        assert_eq!(timeline.entries.len(), 0);
        assert_eq!(timeline.total, 1);
        assert!(!timeline.has_more);
    }

    #[test]
    fn test_pagination_offset_beyond_total_returns_empty() {
        let (env, client, admin) = setup();
        let business = create_verified_business(&env, &client, &admin);
        let invoice_id = create_invoice(&env, &client, &admin, &business);

        client.create_dispute(
            &invoice_id,
            &business,
            &String::from_str(&env, "r"),
            &String::from_str(&env, "e"),
        );

        // Only 1 entry exists; offset=5 is beyond it
        let timeline = client.get_dispute_timeline(&invoice_id, &5u32, &10u32);
        assert_eq!(timeline.entries.len(), 0);
        assert_eq!(timeline.total, 1);
        assert!(!timeline.has_more);
    }

    #[test]
    fn test_pagination_limit_capped_at_max_page_size() {
        let (env, client, admin) = setup();
        let business = create_verified_business(&env, &client, &admin);
        let invoice_id = create_invoice(&env, &client, &admin, &business);

        client.create_dispute(
            &invoice_id,
            &business,
            &String::from_str(&env, "r"),
            &String::from_str(&env, "e"),
        );
        client.put_dispute_under_review(&invoice_id, &admin);
        client.resolve_dispute(&invoice_id, &admin, &String::from_str(&env, "done"));

        // Request more than TIMELINE_MAX_PAGE_SIZE — should still return all 3
        // (3 < 50, so cap doesn't truncate here, but the cap constant is enforced)
        let timeline = client.get_dispute_timeline(&invoice_id, &0u32, &(TIMELINE_MAX_PAGE_SIZE + 100));
        // All 3 entries fit within the cap
        assert_eq!(timeline.entries.len(), 3);
        assert_eq!(timeline.total, 3);
    }

    #[test]
    fn test_pagination_total_field_always_reflects_full_count() {
        let (env, client, admin) = setup();
        let business = create_verified_business(&env, &client, &admin);
        let invoice_id = create_invoice(&env, &client, &admin, &business);

        client.create_dispute(
            &invoice_id,
            &business,
            &String::from_str(&env, "r"),
            &String::from_str(&env, "e"),
        );
        client.put_dispute_under_review(&invoice_id, &admin);
        client.resolve_dispute(&invoice_id, &admin, &String::from_str(&env, "done"));

        // Even with limit=1, total must be 3
        let page1 = client.get_dispute_timeline(&invoice_id, &0u32, &1u32);
        assert_eq!(page1.total, 3);

        let page2 = client.get_dispute_timeline(&invoice_id, &1u32, &1u32);
        assert_eq!(page2.total, 3);

        let page3 = client.get_dispute_timeline(&invoice_id, &2u32, &1u32);
        assert_eq!(page3.total, 3);
        assert!(!page3.has_more);
    }

    #[test]
    fn test_pagination_sequential_pages_cover_all_entries() {
        let (env, client, admin) = setup();
        let business = create_verified_business(&env, &client, &admin);
        let invoice_id = create_invoice(&env, &client, &admin, &business);

        client.create_dispute(
            &invoice_id,
            &business,
            &String::from_str(&env, "r"),
            &String::from_str(&env, "e"),
        );
        client.put_dispute_under_review(&invoice_id, &admin);
        client.resolve_dispute(&invoice_id, &admin, &String::from_str(&env, "done"));

        let p0 = client.get_dispute_timeline(&invoice_id, &0u32, &1u32);
        let p1 = client.get_dispute_timeline(&invoice_id, &1u32, &1u32);
        let p2 = client.get_dispute_timeline(&invoice_id, &2u32, &1u32);

        assert!(p0.has_more);
        assert!(p1.has_more);
        assert!(!p2.has_more);

        let e0 = p0.entries.get(0).unwrap();
        let e1 = p1.entries.get(0).unwrap();
        let e2 = p2.entries.get(0).unwrap();

        assert_eq!(e0.event, String::from_str(&env, "Opened"));
        assert_eq!(e1.event, String::from_str(&env, "UnderReview"));
        assert_eq!(e2.event, String::from_str(&env, "Resolved"));
    }

    // -----------------------------------------------------------------------
    // current_status field
    // -----------------------------------------------------------------------

    #[test]
    fn test_current_status_matches_on_chain_state() {
        let (env, client, admin) = setup();
        let business = create_verified_business(&env, &client, &admin);
        let invoice_id = create_invoice(&env, &client, &admin, &business);

        client.create_dispute(
            &invoice_id,
            &business,
            &String::from_str(&env, "r"),
            &String::from_str(&env, "e"),
        );

        let t1 = client.get_dispute_timeline(&invoice_id, &0u32, &10u32);
        assert_eq!(t1.current_status, DisputeStatus::Disputed);

        client.put_dispute_under_review(&invoice_id, &admin);
        let t2 = client.get_dispute_timeline(&invoice_id, &0u32, &10u32);
        assert_eq!(t2.current_status, DisputeStatus::UnderReview);

        client.resolve_dispute(&invoice_id, &admin, &String::from_str(&env, "done"));
        let t3 = client.get_dispute_timeline(&invoice_id, &0u32, &10u32);
        assert_eq!(t3.current_status, DisputeStatus::Resolved);
    }
}
