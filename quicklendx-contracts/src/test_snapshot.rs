/// Analytics snapshot tests
///
/// # Coverage
///
/// ## 1. Clean snapshot (happy path)
/// - `schema_version` equals `ANALYTICS_SCHEMA_VERSION` (currently 1).
/// - `ledger_timestamp` and `platform_metrics.timestamp` both reflect the
///   ledger timestamp set before the call.
/// - After storing two invoices, `platform_metrics.total_invoices == 2` and
///   `total_volume` equals the sum of the two invoice amounts.
/// - `performance_metrics.dispute_resolution_time == 0` when no disputes
///   exist.
/// - `performance_metrics.transaction_success_rate == 0` when no invoice is
///   in `Paid` status yet.
///
/// ## 2. Snapshot with open dispute (boundary path)
/// - After opening a dispute on a `Pending` invoice the invoice **stays in
///   `Pending` status** for analytics purposes, so
///   `platform_metrics.total_invoices` still counts it.
/// - `performance_metrics.dispute_resolution_time == 0` because only
///   disputes whose `resolved_at > 0` (i.e. fully resolved disputes)
///   contribute to the average — an open `Disputed` invoice does not.
/// - The snapshot output is internally consistent: identical values are
///   returned regardless of whether the snapshot or the individual
///   `get_platform_metrics` / `get_performance_metrics` calls are used.
///
/// These tests carry no feature gate so they run on every CI matrix entry.
#[cfg(test)]
mod test_snapshot {
    use crate::analytics::ANALYTICS_SCHEMA_VERSION;
    use crate::invoice::InvoiceCategory;
    use crate::{QuickLendXContract, QuickLendXContractClient};
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        Address, Env, String, Vec,
    };

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    /// Register the contract, set an admin, and enable auth mocking.
    fn setup(env: &Env) -> (QuickLendXContractClient<'_>, Address, Address) {
        let contract_id = env.register(QuickLendXContract, ());
        let client = QuickLendXContractClient::new(env, &contract_id);
        let admin = Address::generate(env);
        let business = Address::generate(env);
        env.mock_all_auths();
        client.set_admin(&admin);
        (client, admin, business)
    }

    /// Register and KYC-verify a business so it can store invoices.
    fn verify_business(
        env: &Env,
        client: &QuickLendXContractClient<'_>,
        admin: &Address,
        business: &Address,
    ) {
        client.submit_kyc_application(business, &String::from_str(env, "KYC data"));
        client.verify_business(admin, business);
    }

    /// Store one invoice owned by `business` and return its ID.
    fn store_invoice(
        env: &Env,
        client: &QuickLendXContractClient<'_>,
        business: &Address,
        amount: i128,
        description: &str,
    ) -> soroban_sdk::BytesN<32> {
        let currency = Address::generate(env);
        // due_date must be strictly after ledger timestamp
        let due_date = env.ledger().timestamp() + 86_400;
        client.store_invoice(
            business,
            &amount,
            &currency,
            &due_date,
            &String::from_str(env, description),
            &InvoiceCategory::Services,
            &Vec::new(env),
        )
    }

    // -----------------------------------------------------------------------
    // Test 1 — clean snapshot (happy path)
    // -----------------------------------------------------------------------

    /// A freshly-initialised platform with two invoices and no disputes
    /// produces a snapshot whose fields are all consistent and whose
    /// dispute-related performance metric is zero.
    #[test]
    fn snapshot_clean_state_has_zero_dispute_resolution_time() {
        let env = Env::default();
        let ts = 1_710_000_000u64;
        env.ledger().set_timestamp(ts);

        let (client, admin, business) = setup(&env);
        verify_business(&env, &client, &admin, &business);

        let amount_a = 1_000_000i128;
        let amount_b = 2_500_000i128;
        store_invoice(&env, &client, &business, amount_a, "clean-inv-a");
        store_invoice(&env, &client, &business, amount_b, "clean-inv-b");

        let snapshot = client.export_analytics_snapshot();

        // Schema version is the well-known constant.
        assert_eq!(
            snapshot.schema_version, ANALYTICS_SCHEMA_VERSION,
            "schema_version must equal ANALYTICS_SCHEMA_VERSION"
        );

        // Timestamps are anchored to the ledger clock set above.
        assert_eq!(
            snapshot.ledger_timestamp, ts,
            "ledger_timestamp must reflect the ledger clock"
        );
        assert_eq!(
            snapshot.platform_metrics.timestamp, ts,
            "platform_metrics.timestamp must reflect the ledger clock"
        );
        assert_eq!(
            snapshot.performance_metrics.platform_uptime, ts,
            "platform_uptime must reflect the ledger clock"
        );

        // Two invoices stored → both counted.
        assert_eq!(
            snapshot.platform_metrics.total_invoices, 2,
            "total_invoices must count all stored invoices"
        );

        // Total volume equals the sum of the two invoice amounts.
        assert_eq!(
            snapshot.platform_metrics.total_volume,
            amount_a + amount_b,
            "total_volume must equal the sum of all invoice amounts"
        );

        // No dispute has ever been resolved → resolution time is zero.
        assert_eq!(
            snapshot.performance_metrics.dispute_resolution_time, 0,
            "dispute_resolution_time must be 0 when no disputes exist"
        );

        // No invoice is Paid yet → success rate is zero.
        assert_eq!(
            snapshot.performance_metrics.transaction_success_rate, 0,
            "transaction_success_rate must be 0 when no invoice is Paid"
        );

        // Snapshot must be internally consistent with individual metric calls.
        assert_eq!(
            snapshot.platform_metrics,
            client.get_platform_metrics(),
            "snapshot.platform_metrics must match get_platform_metrics()"
        );
        assert_eq!(
            snapshot.performance_metrics,
            client.get_performance_metrics(),
            "snapshot.performance_metrics must match get_performance_metrics()"
        );
    }

    // -----------------------------------------------------------------------
    // Test 2 — snapshot with an open dispute (boundary path)
    // -----------------------------------------------------------------------

    /// An open (unresolved) dispute must not contribute to
    /// `dispute_resolution_time`.  Only disputes whose `resolved_at > 0` are
    /// counted; a `Disputed` invoice is still unresolved, so it is excluded
    /// from the time average.
    ///
    /// Additionally, because `create_dispute` leaves the invoice in `Pending`
    /// status, `total_invoices` must still count it.
    #[test]
    fn snapshot_with_open_dispute_excludes_unresolved_from_resolution_time() {
        let env = Env::default();
        let ts = 1_720_000_000u64;
        env.ledger().set_timestamp(ts);

        let (client, admin, business) = setup(&env);
        verify_business(&env, &client, &admin, &business);

        // Store one invoice, then open a dispute on it.
        let invoice_amount = 5_000_000i128;
        let invoice_id = store_invoice(&env, &client, &business, invoice_amount, "disputed-inv");

        let reason = String::from_str(&env, "Payment terms not honoured");
        let evidence = String::from_str(&env, "Signed contract attached as reference");
        client.create_dispute(&invoice_id, &business, &reason, &evidence);

        let snapshot = client.export_analytics_snapshot();

        // The invoice is still counted in total_invoices (it remains Pending).
        assert_eq!(
            snapshot.platform_metrics.total_invoices, 1,
            "disputed Pending invoice must still be counted in total_invoices"
        );

        // Volume is unaffected by the dispute.
        assert_eq!(
            snapshot.platform_metrics.total_volume, invoice_amount,
            "total_volume must be unaffected by an open dispute"
        );

        // An open dispute has resolved_at == 0 and therefore must NOT
        // contribute to dispute_resolution_time.
        assert_eq!(
            snapshot.performance_metrics.dispute_resolution_time, 0,
            "dispute_resolution_time must be 0 for an unresolved (open) dispute"
        );

        // Snapshot version and timestamp fields are still correct.
        assert_eq!(
            snapshot.schema_version, ANALYTICS_SCHEMA_VERSION,
            "schema_version must equal ANALYTICS_SCHEMA_VERSION after dispute"
        );
        assert_eq!(
            snapshot.ledger_timestamp, ts,
            "ledger_timestamp must be unchanged by a dispute"
        );

        // Snapshot is internally consistent.
        assert_eq!(
            snapshot.platform_metrics,
            client.get_platform_metrics(),
            "snapshot.platform_metrics must match get_platform_metrics() after open dispute"
        );
        assert_eq!(
            snapshot.performance_metrics,
            client.get_performance_metrics(),
            "snapshot.performance_metrics must match get_performance_metrics() after open dispute"
        );
    }
}
