//! Proves that `Invoice::total_paid` — not the inline `payment_history` window —
//! is the single source of truth for settlement accounting, across the
//! `MAX_INLINE_PAYMENT_HISTORY` (32) truncation boundary.
//!
//! # Why this exists
//! `record_payment()` maintains two parallel views of payment history:
//!
//! 1. **Inline view** (`Invoice.payment_history`): a `Vec<PaymentRecord>` capped at
//!    `MAX_INLINE_PAYMENT_HISTORY = 32` entries. Once full, `update_inline_payment_history`
//!    evicts the oldest entry (`remove(0)`) before pushing the newest. This is a
//!    **recent-window convenience for display/UX**, not an accounting ledger.
//! 2. **Durable view** (`SettlementDataKey::Payment(invoice_id, index)`): one record per
//!    payment, bounded only by `MAX_PAYMENT_COUNT = 1_000`, queryable in full via
//!    `get_payment_records`. This is the **complete, untruncated record set**.
//!
//! `invoice.total_paid` is updated unconditionally on every applied payment, strictly
//! before the inline view is touched, and is never derived from the inline view at
//! read time. Once payment count exceeds 32, summing the inline `payment_history`
//! alone under-reports total payments — any consumer that does this will silently
//! drift from the authoritative balance. This file pins the contract so that
//! regressions here fail loudly instead of surfacing later as a settlement dispute.
//!
//! # Invariant under test
//! At every step, for every invoice:
//! - `sum(get_payment_records(invoice_id, 0, count)) == invoice.total_paid` (exact)
//! - `invoice.total_paid <= invoice.amount` (capping invariant, never violated)
//! - `invoice.payment_history.len() <= MAX_INLINE_PAYMENT_HISTORY` (inline cap)
//!
//! **Consumers must read `total_paid` and/or `get_payment_records` for accounting.**
//! The inline `payment_history` field must never be summed or treated as a ledger;
//! it is documentation-only "what happened recently," not "what is owed."

#[cfg(test)]
mod tests {
    extern crate alloc;
    use crate::invoice::InvoiceCategory;
    use crate::settlement::{get_invoice_progress, get_payment_count, get_payment_records};
    use crate::{QuickLendXContract, QuickLendXContractClient};
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        token, Address, BytesN, Env, String, Vec,
    };

    /// Inline history cap, mirrored from `settlement.rs` (`MAX_INLINE_PAYMENT_HISTORY`).
    /// Kept as a local literal rather than importing the private constant, consistent
    /// with how sibling test files pin `MAX_PAYMENT_COUNT` in comments rather than
    /// importing it.
    const MAX_INLINE_PAYMENT_HISTORY: u32 = 32;

    fn setup_funded_invoice(
        env: &Env,
        client: &QuickLendXContractClient,
        contract_id: &Address,
        invoice_amount: i128,
    ) -> (BytesN<32>, Address, Address, Address) {
        let admin = Address::generate(env);
        let business = Address::generate(env);
        let investor = Address::generate(env);
        let token_admin = Address::generate(env);
        let currency = env
            .register_stellar_asset_contract_v2(token_admin.clone())
            .address();
        let token_client = token::Client::new(env, &currency);
        let sac_client = token::StellarAssetClient::new(env, &currency);
        let initial_balance = 1_000_000i128;
        sac_client.mint(&business, &initial_balance);
        sac_client.mint(&investor, &initial_balance);
        let expiration = env.ledger().sequence() + 10_000;
        token_client.approve(&business, contract_id, &initial_balance, &expiration);
        token_client.approve(&investor, contract_id, &initial_balance, &expiration);
        client.set_admin(&admin);
        client.submit_kyc_application(&business, &String::from_str(env, "business-kyc"));
        client.verify_business(&admin, &business);
        let due_date = env.ledger().timestamp() + 86_400;
        let invoice_id = client.store_invoice(
            &business,
            &invoice_amount,
            &currency,
            &due_date,
            &String::from_str(env, "Invoice for settlement history reconstruction tests"),
            &InvoiceCategory::Services,
            &Vec::new(env),
        );
        client.verify_invoice(&invoice_id);
        client.submit_investor_kyc(&investor, &String::from_str(env, "investor-kyc"));
        client.verify_investor(&investor, &initial_balance);
        let bid_id = client.place_bid(
            &investor,
            &invoice_id,
            &invoice_amount,
            &(invoice_amount + 100),
            &BytesN::from_array(&env, &[0u8; 32]),
        );
        client.accept_bid(&invoice_id, &bid_id);
        (invoice_id, business, investor, currency)
    }

    /// Sums the *full durable record set* for an invoice via `get_payment_records`,
    /// paging in case a future caller passes a small `limit`. This is the
    /// "consumer doing it right" reference implementation that the test asserts
    /// against `invoice.total_paid`.
    fn sum_full_records(env: &Env, contract_id: &Address, invoice_id: &BytesN<32>) -> i128 {
        let count = env.as_contract(contract_id, || get_payment_count(env, invoice_id).unwrap());
        let records = env.as_contract(contract_id, || {
            get_payment_records(env, invoice_id, 0, count).unwrap()
        });
        assert_eq!(
            records.len(),
            count,
            "get_payment_records must return the full record set when limit == count"
        );
        let mut sum: i128 = 0;
        for i in 0..records.len() {
            sum += records.get(i).unwrap().amount;
        }
        sum
    }

    /// Sums only the *inline, truncating* `payment_history` window on the invoice.
    /// This is the "consumer doing it wrong" reference: once payment count exceeds
    /// `MAX_INLINE_PAYMENT_HISTORY`, this undercounts relative to `total_paid`.
    fn sum_inline_history(client: &QuickLendXContractClient, invoice_id: &BytesN<32>) -> i128 {
        let invoice = client.get_invoice(invoice_id);
        let mut sum: i128 = 0;
        for i in 0..invoice.payment_history.len() {
            sum += invoice.payment_history.get(i).unwrap().amount;
        }
        sum
    }

    /// Asserts the three invariants under test hold at the current state of an invoice:
    /// 1. inline history length never exceeds the cap,
    /// 2. full durable record sum equals `total_paid` exactly,
    /// 3. `total_paid` never exceeds `total_due` (capping invariant).
    fn assert_reconstruction_invariants(
        env: &Env,
        client: &QuickLendXContractClient,
        contract_id: &Address,
        invoice_id: &BytesN<32>,
    ) {
        let invoice = client.get_invoice(invoice_id);

        assert!(
            invoice.payment_history.len() <= MAX_INLINE_PAYMENT_HISTORY,
            "inline payment_history must never exceed MAX_INLINE_PAYMENT_HISTORY (32); got {}",
            invoice.payment_history.len()
        );

        let full_sum = sum_full_records(env, contract_id, invoice_id);
        assert_eq!(
            full_sum, invoice.total_paid,
            "sum of full durable payment records must reconstruct total_paid exactly"
        );

        assert!(
            invoice.total_paid <= invoice.amount,
            "capping invariant violated: total_paid ({}) exceeds total_due ({})",
            invoice.total_paid,
            invoice.amount
        );
    }

    /// Drives `n` payments of `amount_each` onto `invoice_id`, asserting the
    /// reconstruction invariants after every single payment (not just at the end),
    /// so a regression at any specific step — not only the final state — is caught.
    /// Uses unique per-payment nonces, consistent with `test_partial_payments.rs`
    /// and `test_payment_history.rs`.
    fn drive_payments_with_invariant_checks(
        env: &Env,
        client: &QuickLendXContractClient,
        contract_id: &Address,
        invoice_id: &BytesN<32>,
        n: u32,
        amount_each: i128,
        nonce_prefix: &str,
        start_timestamp: u64,
    ) {
        for i in 0..n {
            env.ledger().set_timestamp(start_timestamp + i as u64);
            let nonce_str = alloc::format!("{}-{}", nonce_prefix, i);
            let nonce = String::from_str(env, &nonce_str);
            client.process_partial_payment(invoice_id, &amount_each, &nonce);
            assert_reconstruction_invariants(env, client, contract_id, invoice_id);
        }
    }

    // ========================================================================
    // Edge case 1: exactly 32 payments — no truncation has occurred yet.
    // ========================================================================

    #[test]
    fn test_exactly_32_payments_no_truncation() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(QuickLendXContract, ());
        let client = QuickLendXContractClient::new(&env, &contract_id);

        // Large invoice amount relative to per-payment size so settlement
        // does not trigger mid-sequence.
        let (invoice_id, _business, _investor, _currency) =
            setup_funded_invoice(&env, &client, &contract_id, 10_000);

        drive_payments_with_invariant_checks(
            &env,
            &client,
            &contract_id,
            &invoice_id,
            MAX_INLINE_PAYMENT_HISTORY,
            10,
            "exact32",
            100_000,
        );

        let invoice = client.get_invoice(&invoice_id);
        // No truncation should have occurred: inline history holds every payment.
        assert_eq!(
            invoice.payment_history.len(),
            MAX_INLINE_PAYMENT_HISTORY,
            "at exactly 32 payments, inline history should hold all of them untruncated"
        );
        assert_eq!(invoice.total_paid, 320);

        let full_sum = sum_full_records(&env, &contract_id, &invoice_id);
        let inline_sum = sum_inline_history(&client, &invoice_id);
        assert_eq!(
            full_sum, inline_sum,
            "below the truncation boundary, inline sum and full sum must still agree"
        );
        assert_eq!(full_sum, invoice.total_paid);
    }

    // ========================================================================
    // Edge case 2: 33 payments — first truncation. Inline history drops the
    // oldest payment; total_paid and the full record set must not.
    // ========================================================================

    #[test]
    fn test_33_payments_first_truncation_total_paid_remains_truth() {
        let env = Env::default();
        env.cost_estimate().budget().reset_unlimited();
        env.mock_all_auths();
        let contract_id = env.register(QuickLendXContract, ());
        let client = QuickLendXContractClient::new(&env, &contract_id);

        let (invoice_id, _business, _investor, _currency) =
            setup_funded_invoice(&env, &client, &contract_id, 10_000);

        drive_payments_with_invariant_checks(
            &env,
            &client,
            &contract_id,
            &invoice_id,
            MAX_INLINE_PAYMENT_HISTORY + 1, // 33
            10,
            "trunc33",
            200_000,
        );

        let invoice = client.get_invoice(&invoice_id);

        // Inline history is now truncated to exactly the cap.
        assert_eq!(
            invoice.payment_history.len(),
            MAX_INLINE_PAYMENT_HISTORY,
            "after the 33rd payment, inline history must be capped at 32"
        );

        // total_paid reflects all 33 payments, not just the 32 visible inline.
        assert_eq!(
            invoice.total_paid, 330,
            "total_paid must include the evicted first payment"
        );

        // The durable record count is NOT truncated.
        let count = env.as_contract(&contract_id, || {
            get_payment_count(&env, &invoice_id).unwrap()
        });
        assert_eq!(
            count, 33,
            "the full discrete record set must retain all 33 payments, unlike the inline view"
        );

        // Reconstruction from the full record set matches total_paid exactly.
        let full_sum = sum_full_records(&env, &contract_id, &invoice_id);
        assert_eq!(full_sum, invoice.total_paid);

        // Reconstruction from the inline view alone UNDER-REPORTS by exactly one
        // payment's worth. This is the failure mode this test exists to prevent
        // consumers from relying on.
        let inline_sum = sum_inline_history(&client, &invoice_id);
        assert_eq!(
            inline_sum,
            invoice.total_paid - 10,
            "inline-only sum is expected to under-report total_paid by the evicted payment \
             (amount=10) once past the 32-entry cap — this is exactly the bug this test pins"
        );
        assert_ne!(
            inline_sum, invoice.total_paid,
            "inline history sum must NOT be mistaken for total_paid past the truncation boundary"
        );

        // Verify it's specifically the oldest payment (index 0, nonce "trunc33-0")
        // that was evicted, and the inline view now starts at the second payment.
        let first_durable = env.as_contract(&contract_id, || {
            crate::settlement::get_payment_record(&env, &invoice_id, 0).unwrap()
        });
        assert_eq!(first_durable.nonce, String::from_str(&env, "trunc33-0"));

        let first_inline = invoice.payment_history.get(0).unwrap();
        assert_eq!(
            first_inline.transaction_id,
            String::from_str(&env, "trunc33-1"),
            "inline window should now start at the second payment after eviction of the first"
        );
    }

    // ========================================================================
    // Edge case 3: a final payment that closes the invoice after truncation
    // has already occurred. Confirms capping + settlement interact correctly
    // with an already-truncated inline view.
    // ========================================================================

    #[test]
    fn test_final_payment_closes_invoice_after_truncation() {
        let env = Env::default();
        env.cost_estimate().budget().reset_unlimited();
        env.mock_all_auths();
        let contract_id = env.register(QuickLendXContract, ());
        let client = QuickLendXContractClient::new(&env, &contract_id);

        // Invoice sized so that 40 payments of 10 (=400) leaves a remainder that
        // a final payment closes out exactly.
        let (invoice_id, _business, _investor, _currency) =
            setup_funded_invoice(&env, &client, &contract_id, 405);

        // 40 payments of 10 each = 400 total, well past the 32-cap truncation point.
        drive_payments_with_invariant_checks(
            &env,
            &client,
            &contract_id,
            &invoice_id,
            40,
            10,
            "close",
            300_000,
        );

        let invoice = client.get_invoice(&invoice_id);
        assert_eq!(invoice.total_paid, 400);
        assert_eq!(
            invoice.payment_history.len(),
            MAX_INLINE_PAYMENT_HISTORY,
            "inline history remains capped well past the truncation boundary"
        );

        // Final payment of 5 closes the invoice exactly (remaining_due == 5).
        env.ledger().set_timestamp(400_000);
        client.process_partial_payment(&invoice_id, &5, &String::from_str(&env, "close-final"));

        let invoice = client.get_invoice(&invoice_id);
        assert_eq!(invoice.total_paid, 405);
        assert_eq!(
            invoice.total_paid, invoice.amount,
            "invoice must be fully paid"
        );
        assert_eq!(
            invoice.status,
            crate::invoice::InvoiceStatus::Paid,
            "invoice must auto-settle to Paid on reaching total_due"
        );

        // Reconstruction invariants must still hold at the moment of closure,
        // even though the inline view never saw the early, evicted payments.
        assert_reconstruction_invariants(&env, &client, &contract_id, &invoice_id);

        // Full record set has all 41 payments; inline view has only the last 32.
        let count = env.as_contract(&contract_id, || {
            get_payment_count(&env, &invoice_id).unwrap()
        });
        assert_eq!(count, 41);
        assert_eq!(invoice.payment_history.len(), MAX_INLINE_PAYMENT_HISTORY);

        let full_sum = sum_full_records(&env, &contract_id, &invoice_id);
        assert_eq!(full_sum, invoice.total_paid);
        assert_eq!(full_sum, 405);
    }

    // ========================================================================
    // Edge case 4: reconstruction after auto-settlement. Once an invoice is
    // finalized (Paid), the full record set must still reconstruct total_paid,
    // and the invariants must continue to hold on the terminal state.
    // ========================================================================

    #[test]
    fn test_reconstruction_holds_after_auto_settlement() {
        let env = Env::default();
        // This test drives 50 payments through full contract invocations
        // (auth + storage + events), which exceeds the default simulated
        // resource budget. Lift it for this test only, consistent with how
        // Soroban test suites handle intentionally high payment-volume cases.
        env.cost_estimate().budget().reset_unlimited();
        env.mock_all_auths();
        let contract_id = env.register(QuickLendXContract, ());
        let client = QuickLendXContractClient::new(&env, &contract_id);

        let (invoice_id, _business, _investor, _currency) =
            setup_funded_invoice(&env, &client, &contract_id, 500);

        // 50 payments of 10 each = 500, exactly closing the invoice and
        // crossing the 32-cap truncation boundary along the way.
        drive_payments_with_invariant_checks(
            &env,
            &client,
            &contract_id,
            &invoice_id,
            50,
            10,
            "autoclose",
            500_000,
        );

        let invoice = client.get_invoice(&invoice_id);
        assert_eq!(invoice.total_paid, 500);
        assert_eq!(invoice.amount, 500);
        assert_eq!(
            invoice.status,
            crate::invoice::InvoiceStatus::Paid,
            "invoice must have auto-settled exactly at the 50th payment"
        );

        let finalized = env.as_contract(&contract_id, || {
            crate::settlement::is_invoice_finalized(&env, &invoice_id).unwrap()
        });
        assert!(
            finalized,
            "invoice must be marked finalized after auto-settlement"
        );

        // Post-settlement, the full durable record set still reconstructs
        // total_paid exactly — settlement finalization does not prune records.
        let full_sum = sum_full_records(&env, &contract_id, &invoice_id);
        assert_eq!(
            full_sum, invoice.total_paid,
            "full record set must reconstruct total_paid even after finalization"
        );

        // Progress view agrees: 100% complete, 0 remaining, payment_count intact.
        let progress = env.as_contract(&contract_id, || {
            get_invoice_progress(&env, &invoice_id).unwrap()
        });
        assert_eq!(progress.total_paid, 500);
        assert_eq!(progress.remaining_due, 0);
        assert_eq!(progress.progress_percent, 100);
        assert_eq!(progress.payment_count, 50);

        // The inline view, by contrast, only ever held the most recent 32 —
        // it is NOT a valid source for reconstructing the settled total.
        let inline_sum = sum_inline_history(&client, &invoice_id);
        assert_eq!(
            invoice.payment_history.len(),
            MAX_INLINE_PAYMENT_HISTORY,
            "inline view remains capped at 32 even on the terminal, settled invoice"
        );
        assert!(
            inline_sum < invoice.total_paid,
            "inline-only sum must under-report the settled total_paid once payments \
             exceeded the inline cap during the invoice's lifetime"
        );
    }

    // ========================================================================
    // Supplementary: capping invariant explicitly under overpayment pressure,
    // combined with having already crossed the truncation boundary.
    // ========================================================================

    #[test]
    fn test_capping_invariant_holds_with_overpayment_past_truncation() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(QuickLendXContract, ());
        let client = QuickLendXContractClient::new(&env, &contract_id);

        let (invoice_id, _business, _investor, _currency) =
            setup_funded_invoice(&env, &client, &contract_id, 350);

        // 34 payments of 10 each = 340, past truncation, invoice still open (remaining=10).
        drive_payments_with_invariant_checks(
            &env,
            &client,
            &contract_id,
            &invoice_id,
            34,
            10,
            "overpay-setup",
            600_000,
        );

        let invoice = client.get_invoice(&invoice_id);
        assert_eq!(invoice.total_paid, 340);
        assert_eq!(
            invoice.payment_history.len(),
            MAX_INLINE_PAYMENT_HISTORY,
            "already past the inline cap before the overpayment attempt"
        );

        // Attempt to overpay by requesting 100 when only 10 remains due.
        env.ledger().set_timestamp(700_000);
        client.process_partial_payment(&invoice_id, &100, &String::from_str(&env, "overpay-final"));

        let invoice = client.get_invoice(&invoice_id);
        // Capping invariant: applied amount is clamped to remaining_due (10),
        // not the requested 100. total_paid lands exactly at total_due, never above.
        assert_eq!(
            invoice.total_paid, 350,
            "overpayment must be capped at total_due, not the requested amount"
        );
        assert_eq!(invoice.total_paid, invoice.amount);
        assert!(
            invoice.total_paid <= invoice.amount,
            "capping invariant must never be violated"
        );

        // The durable record for the capped final payment reflects the applied
        // (capped) amount, not the requested amount — matching the existing
        // `test_capped_payment_record_reflects_applied_not_requested` convention.
        let count = env.as_contract(&contract_id, || {
            get_payment_count(&env, &invoice_id).unwrap()
        });
        let last_record = env.as_contract(&contract_id, || {
            crate::settlement::get_payment_record(&env, &invoice_id, count - 1).unwrap()
        });
        assert_eq!(
            last_record.amount, 10,
            "durable record must store the applied (capped) amount, not the requested 100"
        );

        // Full reconstruction still holds at the capped terminal state.
        let full_sum = sum_full_records(&env, &contract_id, &invoice_id);
        assert_eq!(full_sum, invoice.total_paid);
    }
}
