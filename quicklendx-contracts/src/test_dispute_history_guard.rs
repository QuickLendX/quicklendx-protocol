/// Boundary tests for the dispute-history guard (`require_no_active_dispute_snapshot`).
///
/// # Coverage
///
/// These tests lock in the `require_no_active_dispute_snapshot` guard's behaviour at
/// the boundary between "no active disputes" and "at least one active dispute".
/// The guard blocks analytics-snapshot generation while any invoice has an
/// unresolved (`Disputed` or `UnderReview`) dispute.
///
/// | Test name                                    | Disputes in index              | Expected outcome       |
/// |----------------------------------------------|--------------------------------|------------------------|
/// | `guard_passes_when_no_disputes_exist`        | 0 disputes                     | Ok(())  ← at threshold |
/// | `guard_passes_when_only_resolved_disputes`   | 1 resolved dispute             | Ok(())  ← one below    |
/// | `guard_fails_when_one_dispute_is_active`     | 1 `Disputed` dispute           | Err(ActiveDisputeExists) ← one above |
/// | `guard_fails_when_one_dispute_is_under_review` | 1 `UnderReview` dispute     | Err(ActiveDisputeExists) ← one above |
/// | `guard_passes_after_dispute_resolves`        | 1 dispute → resolved           | Ok(())  ← recovery     |
///
/// All tests are deterministic and `#![no_std]`-clean: they use only
/// `soroban_sdk` primitives and the contract's public API.
#[cfg(test)]
mod test_dispute_history_guard {
    use crate::errors::QuickLendXError;
    use crate::invoice::InvoiceCategory;
    use crate::{QuickLendXContract, QuickLendXContractClient};
    use soroban_sdk::{testutils::Address as _, Address, BytesN, Env, String, Vec};

    // ------------------------------------------------------------------
    // Helpers
    // ------------------------------------------------------------------

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

    fn create_test_invoice(
        env: &Env,
        client: &QuickLendXContractClient,
        _admin: &Address,
        business: &Address,
        amount: i128,
    ) -> BytesN<32> {
        let currency = Address::generate(env);
        let due_date = env.ledger().timestamp() + 86_400;
        client.store_invoice(
            business,
            &amount,
            &currency,
            &due_date,
            &String::from_str(env, "Test invoice for dispute-history guard"),
            &InvoiceCategory::Services,
            &Vec::new(env),
        )
    }

    // ------------------------------------------------------------------
    // At threshold: zero disputes in the index → guard passes
    // ------------------------------------------------------------------

    /// Guard passes when no disputes exist in the index.
    ///
    /// This is the baseline "at threshold" case: the dispute index is empty,
    /// so zero invoices have an active dispute and the guard must succeed.
    #[test]
    fn guard_passes_when_no_disputes_exist() {
        let env = Env::default();
        let (client, _admin, _business) = setup_with_client(&env);

        // No disputes created → snapshot must succeed.
        assert!(
            client.try_export_analytics_snapshot().is_ok(),
            "guard must pass when no disputes exist (at threshold = 0 active disputes)"
        );
    }

    // ------------------------------------------------------------------
    // One below: only resolved disputes → guard passes
    // ------------------------------------------------------------------

    /// Guard passes when the only dispute in the index is resolved.
    ///
    /// A `Resolved` dispute is "below" the active-dispute threshold — the
    /// admin has already ruled, so the guard must not block snapshots.
    #[test]
    fn guard_passes_when_only_resolved_disputes() {
        let (env, client, admin) = setup();
        let business = create_verified_business(&env, &client, &admin);
        let invoice_id = create_test_invoice(&env, &client, &admin, &business, 100_000);

        // Open, review, and resolve a dispute — this enters the dispute index
        // but ends in the terminal `Resolved` state.
        let reason = String::from_str(&env, "Dispute reason");
        let evidence = String::from_str(&env, "Evidence");
        client.create_dispute(&invoice_id, &business, &reason, &evidence);
        client.put_dispute_under_review(&invoice_id, &admin);
        client.resolve_dispute(
            &invoice_id,
            &admin,
            &String::from_str(&env, "Resolved in favor of business"),
        );

        // The index contains one invoice, but its dispute is Resolved → guard passes.
        assert!(
            client.try_export_analytics_snapshot().is_ok(),
            "guard must pass when the only dispute is resolved (one below threshold)"
        );
    }

    // ------------------------------------------------------------------
    // One above: one active (Disputed) dispute → guard fails
    // ------------------------------------------------------------------

    /// Guard fails when one dispute is in the `Disputed` state.
    ///
    /// This is the "+1" case: a single active dispute must cause the guard
    /// to reject snapshot generation with `ActiveDisputeExists`.
    #[test]
    fn guard_fails_when_one_dispute_is_active() {
        let (env, client, _admin) = setup();
        let business = create_verified_business(&env, &client, &_admin);
        let invoice_id = create_test_invoice(&env, &client, &_admin, &business, 100_000);

        let reason = String::from_str(&env, "Dispute reason");
        let evidence = String::from_str(&env, "Evidence");
        client.create_dispute(&invoice_id, &business, &reason, &evidence);

        let result = client.try_export_analytics_snapshot();
        assert!(
            result.is_err(),
            "guard must fail when one dispute is active (one above threshold)"
        );
        let err = result.unwrap_err().expect("expected contract error");
        assert_eq!(
            err,
            QuickLendXError::ActiveDisputeExists,
            "expected ActiveDisputeExists for one active dispute"
        );
    }

    // ------------------------------------------------------------------
    // One above: one active (UnderReview) dispute → guard fails
    // ------------------------------------------------------------------

    /// Guard fails when one dispute is in the `UnderReview` state.
    ///
    /// `UnderReview` is also an active state — the admin has acknowledged
    /// the dispute but has not yet ruled. The guard must still block.
    #[test]
    fn guard_fails_when_one_dispute_is_under_review() {
        let (env, client, admin) = setup();
        let business = create_verified_business(&env, &client, &admin);
        let invoice_id = create_test_invoice(&env, &client, &admin, &business, 100_000);

        let reason = String::from_str(&env, "Dispute reason");
        let evidence = String::from_str(&env, "Evidence");
        client.create_dispute(&invoice_id, &business, &reason, &evidence);
        client.put_dispute_under_review(&invoice_id, &admin);

        let result = client.try_export_analytics_snapshot();
        assert!(
            result.is_err(),
            "guard must fail when one dispute is UnderReview (one above threshold)"
        );
        let err = result.unwrap_err().expect("expected contract error");
        assert_eq!(
            err,
            QuickLendXError::ActiveDisputeExists,
            "expected ActiveDisputeExists for UnderReview dispute"
        );
    }

    // ------------------------------------------------------------------
    // Recovery: guard passes after dispute resolves
    // ------------------------------------------------------------------

    /// Guard passes after an active dispute is resolved.
    ///
    /// Locks in that the guard is not stateful — once the active dispute
    /// transitions to `Resolved`, snapshots resume immediately.
    #[test]
    fn guard_passes_after_dispute_resolves() {
        let (env, client, admin) = setup();
        let business = create_verified_business(&env, &client, &admin);
        let invoice_id = create_test_invoice(&env, &client, &admin, &business, 100_000);

        let reason = String::from_str(&env, "Dispute reason");
        let evidence = String::from_str(&env, "Evidence");
        client.create_dispute(&invoice_id, &business, &reason, &evidence);

        // Guard blocks while dispute is active.
        assert!(client.try_export_analytics_snapshot().is_err());

        // Resolve the dispute.
        client.put_dispute_under_review(&invoice_id, &admin);
        client.resolve_dispute(&invoice_id, &admin, &String::from_str(&env, "Resolved"));

        // Guard must now pass.
        assert!(
            client.try_export_analytics_snapshot().is_ok(),
            "guard must pass after the active dispute is resolved"
        );
    }

    // ------------------------------------------------------------------
    // Helper to create a client directly (for the at-threshold test)
    // ------------------------------------------------------------------

    fn setup_with_client(env: &Env) -> (QuickLendXContractClient<'_>, Address, Address) {
        env.mock_all_auths();
        let contract_id = env.register(QuickLendXContract, ());
        let client = QuickLendXContractClient::new(env, &contract_id);
        let admin = Address::generate(env);
        let business = Address::generate(env);
        client.set_admin(&admin);
        client.submit_kyc_application(&business, &String::from_str(env, "KYC data"));
        client.verify_business(&admin, &business);
        (client, admin, business)
    }
}
