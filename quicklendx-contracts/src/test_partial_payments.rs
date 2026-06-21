#[cfg(test)]
mod tests {
    use crate::errors::QuickLendXError;
    use crate::invoice::{InvoiceCategory, InvoiceStatus};
    use crate::settlement::{
        get_invoice_progress, get_payment_count, get_payment_record, get_payment_records,
        is_invoice_finalized, record_payment,
    };
    use crate::{QuickLendXContract, QuickLendXContractClient};
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        token, Address, BytesN, Env, String, Vec,
    };

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
        let initial_balance = 50_000i128;
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
            &String::from_str(env, "Invoice for settlement tests"),
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
        );
        client.accept_bid(&invoice_id, &bid_id);
        (invoice_id, business, investor, currency)
    }

    fn setup_cancelled_invoice(
        env: &Env,
        client: &QuickLendXContractClient,
    ) -> (BytesN<32>, Address) {
        let admin = Address::generate(env);
        client.set_admin(&admin);

        let business = Address::generate(env);
        client.submit_kyc_application(&business, &String::from_str(env, "business-kyc"));
        client.verify_business(&admin, &business);

        let currency = Address::generate(env);
        let due_date = env.ledger().timestamp() + 86_400;
        let invoice_id = client.store_invoice(
            &business,
            &1_000,
            &currency,
            &due_date,
            &String::from_str(env, "Cancelled invoice"),
            &InvoiceCategory::Services,
            &Vec::new(env),
        );
        client.cancel_invoice(&invoice_id);
        (invoice_id, business)
    }

    // ========================================================================
    // Existing tests
    // ========================================================================


    #[test]
    fn test_partial_payment_accumulates_correctly() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(QuickLendXContract, ());
        let client = QuickLendXContractClient::new(&env, &contract_id);
        let (invoice_id, _business, _investor, _currency) =
            setup_funded_invoice(&env, &client, &contract_id, 1_000);
        env.ledger().set_timestamp(1_000);
        client.process_partial_payment(&invoice_id, &300, &String::from_str(&env, "tx-1"));
        env.ledger().set_timestamp(1_100);
        client.process_partial_payment(&invoice_id, &200, &String::from_str(&env, "tx-2"));
        let invoice = client.get_invoice(&invoice_id);
        assert_eq!(invoice.total_paid, 500);
        assert_eq!(invoice.status, InvoiceStatus::Funded);
        let progress = env.as_contract(&contract_id, || {
            get_invoice_progress(&env, &invoice_id).unwrap()
        });
        assert_eq!(progress.total_due, 1_000);
        assert_eq!(progress.total_paid, 500);
        assert_eq!(progress.remaining_due, 500);
        assert_eq!(progress.progress_percent, 50);
        assert_eq!(progress.payment_count, 2);
    }

    #[test]
    fn test_transaction_id_is_stored_in_records() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(QuickLendXContract, ());
        let client = QuickLendXContractClient::new(&env, &contract_id);
        let (invoice_id, business, _investor, _currency) =
            setup_funded_invoice(&env, &client, &contract_id, 1_000);
        let tx_id = String::from_str(&env, "tx-store-001");
        env.ledger().set_timestamp(1_250);
        client.process_partial_payment(&invoice_id, &275, &tx_id);
        let durable_record = env.as_contract(&contract_id, || {
            get_payment_record(&env, &invoice_id, 0).unwrap()
        });
        assert_eq!(durable_record.payer, business);
        assert_eq!(durable_record.amount, 275);
        assert_eq!(durable_record.timestamp, 1_250);
        assert_eq!(durable_record.nonce, tx_id);
        let invoice = client.get_invoice(&invoice_id);
        assert_eq!(invoice.total_paid, 275);
        assert_eq!(invoice.payment_history.len(), 1);
        let inline_record = invoice.payment_history.get(0).unwrap();
        assert_eq!(inline_record.amount, 275);
        assert_eq!(inline_record.timestamp, 1_250);
        assert_eq!(
            inline_record.transaction_id,
            String::from_str(&env, "tx-store-001")
        );
    }

    #[test]
    fn test_duplicate_transaction_id_is_deduplicated() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(QuickLendXContract, ());
        let client = QuickLendXContractClient::new(&env, &contract_id);
        let (invoice_id, _business, _investor, _currency) =
            setup_funded_invoice(&env, &client, &contract_id, 1_000);
        let duplicate_tx = String::from_str(&env, "dup-tx");
        env.ledger().set_timestamp(1_300);
        client.process_partial_payment(&invoice_id, &100, &duplicate_tx);
        
        // This should not fail, but effectively do nothing (deduplicated)
        client.process_partial_payment(&invoice_id, &150, &duplicate_tx);
        
        let invoice = client.get_invoice(&invoice_id);
        assert_eq!(invoice.total_paid, 100);
        let count = env.as_contract(&contract_id, || {
            get_payment_count(&env, &invoice_id).unwrap()
        });
        assert_eq!(count, 1);
    }

    #[test]
    fn test_empty_transaction_id_is_allowed_and_recorded() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(QuickLendXContract, ());
        let client = QuickLendXContractClient::new(&env, &contract_id);
        let (invoice_id, _business, _investor, _currency) =
            setup_funded_invoice(&env, &client, &contract_id, 1_000);
        let empty_tx = String::from_str(&env, "");
        env.ledger().set_timestamp(1_400);
        client.process_partial_payment(&invoice_id, &125, &empty_tx);
        env.ledger().set_timestamp(1_500);
        client.process_partial_payment(&invoice_id, &125, &empty_tx);
        let count = env.as_contract(&contract_id, || {
            get_payment_count(&env, &invoice_id).unwrap()
        });
        assert_eq!(count, 2);
        let first = env.as_contract(&contract_id, || {
            get_payment_record(&env, &invoice_id, 0).unwrap()
        });
        let second = env.as_contract(&contract_id, || {
            get_payment_record(&env, &invoice_id, 1).unwrap()
        });
        assert_eq!(first.nonce, String::from_str(&env, ""));
        assert_eq!(second.nonce, String::from_str(&env, ""));
    }

    #[test]
    fn test_final_payment_marks_invoice_paid() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(QuickLendXContract, ());
        let client = QuickLendXContractClient::new(&env, &contract_id);
        let (invoice_id, _business, _investor, _currency) =
            setup_funded_invoice(&env, &client, &contract_id, 1_000);
        env.ledger().set_timestamp(2_000);
        client.process_partial_payment(&invoice_id, &400, &String::from_str(&env, "pay-1"));
        env.ledger().set_timestamp(2_100);
        client.process_partial_payment(&invoice_id, &600, &String::from_str(&env, "pay-2"));
        let invoice = client.get_invoice(&invoice_id);
        assert_eq!(invoice.total_paid, 1_000);
        assert_eq!(invoice.status, InvoiceStatus::Paid);
    }

    #[test]
    fn test_overpayment_is_capped_at_total_due() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(QuickLendXContract, ());
        let client = QuickLendXContractClient::new(&env, &contract_id);
        let (invoice_id, _business, _investor, _currency) =
            setup_funded_invoice(&env, &client, &contract_id, 1_000);
        env.ledger().set_timestamp(3_000);
        client.process_partial_payment(&invoice_id, &800, &String::from_str(&env, "cap-1"));
        env.ledger().set_timestamp(3_100);
        client.process_partial_payment(&invoice_id, &500, &String::from_str(&env, "cap-2"));
        let invoice = client.get_invoice(&invoice_id);
        assert_eq!(invoice.total_paid, 1_000);
        assert_eq!(invoice.status, InvoiceStatus::Paid);
        let second_record = env.as_contract(&contract_id, || {
            get_payment_record(&env, &invoice_id, 1).unwrap()
        });
        assert_eq!(second_record.amount, 200);
    }

    #[test]
    fn test_zero_amount_rejected() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(QuickLendXContract, ());
        let client = QuickLendXContractClient::new(&env, &contract_id);
        let (invoice_id, _business, _investor, _currency) =
            setup_funded_invoice(&env, &client, &contract_id, 1_000);
        let result =
            client.try_process_partial_payment(&invoice_id, &0, &String::from_str(&env, "zero"));
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().unwrap(), QuickLendXError::InvalidAmount);
    }

    #[test]
    fn test_missing_invoice_is_rejected() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(QuickLendXContract, ());
        let client = QuickLendXContractClient::new(&env, &contract_id);
        let missing_id = BytesN::from_array(&env, &[7u8; 32]);
        let result = client.try_process_partial_payment(
            &missing_id,
            &100,
            &String::from_str(&env, "missing"),
        );
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().unwrap(),
            QuickLendXError::InvoiceNotFound
        );
    }

    #[test]
    fn test_payment_records_are_queryable_and_ordered() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(QuickLendXContract, ());
        let client = QuickLendXContractClient::new(&env, &contract_id);
        let (invoice_id, business, _investor, _currency) =
            setup_funded_invoice(&env, &client, &contract_id, 1_000);
        env.ledger().set_timestamp(5_001);
        client.process_partial_payment(&invoice_id, &100, &String::from_str(&env, "ord-1"));
        env.ledger().set_timestamp(5_002);
        client.process_partial_payment(&invoice_id, &200, &String::from_str(&env, "ord-2"));
        env.ledger().set_timestamp(5_003);
        client.process_partial_payment(&invoice_id, &300, &String::from_str(&env, "ord-3"));
        
        let records = env.as_contract(&contract_id, || {
            get_payment_records(&env, &invoice_id, 0, 10).unwrap()
        });
        assert_eq!(records.len(), 3);
        assert_eq!(records.get(0).unwrap().amount, 100);
        assert_eq!(records.get(1).unwrap().amount, 200);
        assert_eq!(records.get(2).unwrap().amount, 300);
    }

    // ========================================================================
    // Helper functions (second set)
    // ========================================================================

    fn setup_env() -> (Env, QuickLendXContractClient<'static>, Address) {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(QuickLendXContract, ());
        let client = QuickLendXContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        client.set_admin(&admin);
        (env, client, admin, contract_id)
    }

    fn create_verified_business(
        env: &Env,
        client: &QuickLendXContractClient,
        admin: &Address,
    ) -> Address {
        let business = Address::generate(env);
        client.submit_kyc_application(&business, &String::from_str(env, "Business KYC"));
        client.verify_business(admin, &business);
        business
    }

    fn create_verified_investor(
        env: &Env,
        client: &QuickLendXContractClient,
        limit: i128,
    ) -> Address {
        let investor = Address::generate(env);
        client.submit_investor_kyc(&investor, &String::from_str(env, "Investor KYC"));
        client.verify_investor(&investor, &limit);
        investor
    }

    // ========================================================================
    // Hardening tests for partial payments
    // ========================================================================

    /// Single payment of exact invoice amount triggers auto-settlement and
    /// sets finalization flag.
    #[test]
    fn test_single_full_partial_payment_finalizes() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(QuickLendXContract, ());
        let client = QuickLendXContractClient::new(&env, &contract_id);
        let (invoice_id, _business, _investor, _currency) =
            setup_funded_invoice(&env, &client, &contract_id, 1_000);

        env.ledger().set_timestamp(7_000);
        client.process_partial_payment(&invoice_id, &1_000, &String::from_str(&env, "full"));

        let invoice = client.get_invoice(&invoice_id);
        assert_eq!(invoice.total_paid, 1_000);
        assert_eq!(invoice.status, InvoiceStatus::Paid);

        let finalized = env.as_contract(&contract_id, || {
            is_invoice_finalized(&env, &invoice_id).unwrap()
        });
        assert!(finalized);
    }

    /// Payment record sum must always equal invoice.total_paid.
    #[test]
    fn test_payment_record_sum_equals_total_paid() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(QuickLendXContract, ());
        let client = QuickLendXContractClient::new(&env, &contract_id);
        let (invoice_id, _business, _investor, _currency) =
            setup_funded_invoice(&env, &client, &contract_id, 1_000);

        let amounts: [i128; 4] = [150, 250, 350, 250];
        for (i, &amt) in amounts.iter().enumerate() {
            env.ledger().set_timestamp(8_000 + i as u64 * 100);
            let nonce = String::from_str(&env, &format!("sum-{}", i));
            client.process_partial_payment(&invoice_id, &amt, &nonce);
        }

        let invoice = client.get_invoice(&invoice_id);
        let count = env.as_contract(&contract_id, || {
            get_payment_count(&env, &invoice_id).unwrap()
        });
        let records = env.as_contract(&contract_id, || {
            get_payment_records(&env, &invoice_id, 0, count).unwrap()
        });
        let sum: i128 = (0..records.len())
            .map(|i| records.get(i as u32).unwrap().amount)
            .sum();
        assert_eq!(
            sum, invoice.total_paid,
            "sum of durable records must equal invoice.total_paid"
        );
    }

    /// Minimum payment of 1 unit is accepted.
    #[test]
    fn test_minimum_payment_accepted() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(QuickLendXContract, ());
        let client = QuickLendXContractClient::new(&env, &contract_id);
        let (invoice_id, _business, _investor, _currency) =
            setup_funded_invoice(&env, &client, &contract_id, 1_000);

        env.ledger().set_timestamp(9_000);
        client.process_partial_payment(&invoice_id, &1, &String::from_str(&env, "min-pay"));

        let invoice = client.get_invoice(&invoice_id);
        assert_eq!(invoice.total_paid, 1);
        assert_eq!(invoice.status, InvoiceStatus::Funded);
    }

    /// Many small payments accumulate correctly to full settlement.
    #[test]
    fn test_many_small_payments_accumulate_to_full() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(QuickLendXContract, ());
        let client = QuickLendXContractClient::new(&env, &contract_id);
        let (invoice_id, _business, _investor, _currency) =
            setup_funded_invoice(&env, &client, &contract_id, 100);

        // 100 payments of 1 each.
        for i in 0..100u32 {
            env.ledger().set_timestamp(10_000 + i as u64);
            let nonce = String::from_str(&env, &format!("small-{}", i));
            client.process_partial_payment(&invoice_id, &1, &nonce);
        }

        let invoice = client.get_invoice(&invoice_id);
        assert_eq!(invoice.total_paid, 100);
        assert_eq!(invoice.status, InvoiceStatus::Paid);

        let count = env.as_contract(&contract_id, || {
            get_payment_count(&env, &invoice_id).unwrap()
        });
        assert_eq!(count, 100);
    }

    /// Overpayment attempt on the final payment: capped amount is recorded,
    /// not the requested amount.
    #[test]
    fn test_capped_payment_record_reflects_applied_not_requested() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(QuickLendXContract, ());
        let client = QuickLendXContractClient::new(&env, &contract_id);
        let (invoice_id, _business, _investor, _currency) =
            setup_funded_invoice(&env, &client, &contract_id, 500);

        env.ledger().set_timestamp(11_000);
        client.process_partial_payment(&invoice_id, &400, &String::from_str(&env, "pre"));

        env.ledger().set_timestamp(11_100);
        // Request 300, but only 100 remains.
        client.process_partial_payment(&invoice_id, &300, &String::from_str(&env, "over"));

        let record = env.as_contract(&contract_id, || {
            get_payment_record(&env, &invoice_id, 1).unwrap()
        });
        assert_eq!(record.amount, 100, "recorded amount must be capped at remaining_due");

        let invoice = client.get_invoice(&invoice_id);
        assert_eq!(invoice.total_paid, 500);
    }

    /// Progress for a non-existent invoice returns InvoiceNotFound.
    #[test]
    fn test_progress_for_nonexistent_invoice() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(QuickLendXContract, ());
        let missing_id = BytesN::from_array(&env, &[99u8; 32]);
        let result = env.as_contract(&contract_id, || {
            get_invoice_progress(&env, &missing_id)
        });
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), QuickLendXError::InvoiceNotFound);
    }

    /// Payment count for a non-existent invoice returns InvoiceNotFound.
    #[test]
    fn test_payment_count_for_nonexistent_invoice() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(QuickLendXContract, ());
        let missing_id = BytesN::from_array(&env, &[98u8; 32]);
        let result = env.as_contract(&contract_id, || {
            get_payment_count(&env, &missing_id)
        });
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), QuickLendXError::InvoiceNotFound);
    }

    /// Querying payment record at invalid index returns StorageKeyNotFound.
    #[test]
    fn test_payment_record_at_invalid_index() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(QuickLendXContract, ());
        let client = QuickLendXContractClient::new(&env, &contract_id);
        let (invoice_id, _business, _investor, _currency) =
            setup_funded_invoice(&env, &client, &contract_id, 1_000);

        // No payments made yet; index 0 should not exist.
        let result = env.as_contract(&contract_id, || {
            get_payment_record(&env, &invoice_id, 0)
        });
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), QuickLendXError::StorageKeyNotFound);
    }

    /// Unique nonces are accepted; the same nonce across different invoices
    /// should be fine (nonce is scoped per invoice).
    #[test]
    fn test_same_nonce_different_invoices_accepted() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(QuickLendXContract, ());
        let client = QuickLendXContractClient::new(&env, &contract_id);

        let (invoice_id_a, _biz_a, _inv_a, _cur_a) =
            setup_funded_invoice(&env, &client, &contract_id, 1_000);
        let (invoice_id_b, _biz_b, _inv_b, _cur_b) =
            setup_funded_invoice(&env, &client, &contract_id, 1_000);

        let shared_nonce = String::from_str(&env, "shared-nonce");
        env.ledger().set_timestamp(12_000);
        client.process_partial_payment(&invoice_id_a, &100, &shared_nonce);
        // Same nonce on different invoice should succeed.
        client.process_partial_payment(&invoice_id_b, &100, &shared_nonce);

        let a = client.get_invoice(&invoice_id_a);
        let b = client.get_invoice(&invoice_id_b);
        assert_eq!(a.total_paid, 100);
        assert_eq!(b.total_paid, 100);
    }

    /// After full settlement via partial payments, progress shows 100% and 0 remaining.
    #[test]
    fn test_progress_at_100_percent_after_full_partial_payment() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(QuickLendXContract, ());
        let client = QuickLendXContractClient::new(&env, &contract_id);
        let (invoice_id, _business, _investor, _currency) =
            setup_funded_invoice(&env, &client, &contract_id, 1_000);

        env.ledger().set_timestamp(13_000);
        client.process_partial_payment(&invoice_id, &1_000, &String::from_str(&env, "full-prog"));

        let progress = env.as_contract(&contract_id, || {
            get_invoice_progress(&env, &invoice_id).unwrap()
        });
        assert_eq!(progress.progress_percent, 100);
        assert_eq!(progress.remaining_due, 0);
        assert_eq!(progress.total_paid, progress.total_due);
        assert_eq!(progress.status, InvoiceStatus::Paid);
    }

    // ========================================================================
    // Payment Count Cap Enforcement Tests
    // ========================================================================
    // These tests validate that payment record storage is bounded (cap enforced),
    // behavior at the cap is documented, and queries remain stable near the limit.
    // MAX_PAYMENT_COUNT = 1_000 in settlement.rs

    /// Helper to make many small payments up to a target count without triggering settlement.
    /// Uses a large invoice amount to avoid early settlement.
    fn make_n_payments(
        env: &Env,
        client: &QuickLendXContractClient,
        invoice_id: &BytesN<32>,
        count: u32,
    ) {
        // Use a very large invoice amount to avoid settlement
        for i in 0..count {
            env.ledger().set_timestamp(20_000 + i as u64);
            let nonce = String::from_str(env, &format!("cap-test-{}", i));
            // Use record_payment directly to avoid auto-settlement logic
            let business = Address::generate(env);
            // We need to use the client which handles auth
            client.process_partial_payment(invoice_id, &1, &nonce);
        }
    }

    /// Test that the payment count cap (MAX_PAYMENT_COUNT = 1000) is enforced.
    /// After reaching the cap, additional payments should be rejected with OperationNotAllowed.
    #[test]
    fn test_payment_count_cap_is_enforced() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(QuickLendXContract, ());
        let client = QuickLendXContractClient::new(&env, &contract_id);
        
        // Use a very large invoice amount (100,000) so we can make 1000 payments of 1 each
        let (invoice_id, _business, _investor, _currency) =
            setup_funded_invoice(&env, &client, &contract_id, 100_000);

        // Make 1000 payments of 1 each (reaching the cap)
        for i in 0..1000u32 {
            env.ledger().set_timestamp(30_000 + i as u64);
            let nonce = String::from_str(&env, &format!("cap-enforce-{}", i));
            let result = client.try_process_partial_payment(&invoice_id, &1, &nonce);
            assert!(result.is_ok(), "Payment {} should succeed before cap", i);
        }

        // Verify payment count is exactly 1000
        let count = env.as_contract(&contract_id, || {
            get_payment_count(&env, &invoice_id).unwrap()
        });
        assert_eq!(count, 1000, "Payment count should be exactly 1000 at cap");

        // The 1001st payment should be rejected
        env.ledger().set_timestamp(31_000);
        let result = client.try_process_partial_payment(
            &invoice_id,
            &1,
            &String::from_str(&env, "cap-enforce-1000"),
        );
        assert!(result.is_err(), "Payment beyond cap should be rejected");
        assert_eq!(
            result.unwrap_err().unwrap(),
            QuickLendXError::OperationNotAllowed,
            "Error should be OperationNotAllowed when cap is reached"
        );

        // Verify count hasn't changed
        let count_after = env.as_contract(&contract_id, || {
            get_payment_count(&env, &invoice_id).unwrap()
        });
        assert_eq!(count_after, 1000, "Payment count should remain 1000 after rejected payment");
    }

    /// Test that the 999th payment (just before cap) succeeds.
    #[test]
    fn test_payment_just_before_cap_succeeds() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(QuickLendXContract, ());
        let client = QuickLendXContractClient::new(&env, &contract_id);
        
        // Use a very large invoice amount
        let (invoice_id, _business, _investor, _currency) =
            setup_funded_invoice(&env, &client, &contract_id, 100_000);

        // Make 999 payments
        for i in 0..999u32 {
            env.ledger().set_timestamp(32_000 + i as u64);
            let nonce = String::from_str(&env, &format!("before-cap-{}", i));
            let result = client.try_process_partial_payment(&invoice_id, &1, &nonce);
            assert!(result.is_ok(), "Payment {} should succeed", i);
        }

        // Verify payment count is 999
        let count = env.as_contract(&contract_id, || {
            get_payment_count(&env, &invoice_id).unwrap()
        });
        assert_eq!(count, 999, "Payment count should be 999");

        // The 1000th payment should still succeed (at the boundary)
        env.ledger().set_timestamp(33_000);
        let result = client.try_process_partial_payment(
            &invoice_id,
            &1,
            &String::from_str(&env, "before-cap-999"),
        );
        assert!(result.is_ok(), "1000th payment should succeed at cap boundary");

        let count_after = env.as_contract(&contract_id, || {
            get_payment_count(&env, &invoice_id).unwrap()
        });
        assert_eq!(count_after, 1000, "Payment count should be 1000 after boundary payment");
    }

    /// Test that queries remain stable and return correct data when near the cap.
    #[test]
    fn test_queries_stable_near_cap() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(QuickLendXContract, ());
        let client = QuickLendXContractClient::new(&env, &contract_id);
        
        // Use a very large invoice amount
        let (invoice_id, _business, _investor, _currency) =
            setup_funded_invoice(&env, &client, &contract_id, 100_000);

        // Make 999 payments (near cap)
        for i in 0..999u32 {
            env.ledger().set_timestamp(34_000 + i as u64);
            let nonce = String::from_str(&env, &format!("query-test-{}", i));
            client.process_partial_payment(&invoice_id, &1, &nonce);
        }

        // Test get_payment_count
        let count = env.as_contract(&contract_id, || {
            get_payment_count(&env, &invoice_id).unwrap()
        });
        assert_eq!(count, 999, "Payment count should be accurate near cap");

        // Test get_payment_record for first, middle, and last records
        let first = env.as_contract(&contract_id, || {
            get_payment_record(&env, &invoice_id, 0).unwrap()
        });
        assert_eq!(first.amount, 1, "First payment record should be accessible");

        let middle = env.as_contract(&contract_id, || {
            get_payment_record(&env, &invoice_id, 500).unwrap()
        });
        assert_eq!(middle.amount, 1, "Middle payment record should be accessible");

        let last = env.as_contract(&contract_id, || {
            get_payment_record(&env, &invoice_id, 998).unwrap()
        });
        assert_eq!(last.amount, 1, "Last payment record should be accessible");

        // Test get_payment_records pagination
        let page1 = env.as_contract(&contract_id, || {
            get_payment_records(&env, &invoice_id, 0, 100).unwrap()
        });
        assert_eq!(page1.len(), 100, "First page should return 100 records");

        let page2 = env.as_contract(&contract_id, || {
            get_payment_records(&env, &invoice_id, 900, 100).unwrap()
        });
        assert_eq!(page2.len(), 99, "Last page should return remaining 99 records");

        // Test get_invoice_progress
        let progress = env.as_contract(&contract_id, || {
            get_invoice_progress(&env, &invoice_id).unwrap()
        });
        assert_eq!(progress.payment_count, 999, "Progress should show correct payment count");
        assert_eq!(progress.total_paid, 999, "Progress should show correct total paid");
        assert_eq!(progress.progress_percent, 0, "Progress percent should be 0 for large invoice");
    }

    /// Test pagination returns correct results at cap boundary.
    #[test]
    fn test_pagination_at_cap_boundary() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(QuickLendXContract, ());
        let client = QuickLendXContractClient::new(&env, &contract_id);
        
        // Use a very large invoice amount
        let (invoice_id, _business, _investor, _currency) =
            setup_funded_invoice(&env, &client, &contract_id, 100_000);

        // Make exactly 1000 payments (at cap)
        for i in 0..1000u32 {
            env.ledger().set_timestamp(35_000 + i as u64);
            let nonce = String::from_str(&env, &format!("pagination-{}", i));
            client.process_partial_payment(&invoice_id, &1, &nonce);
        }

        // Test various pagination scenarios
        // Full range query
        let all_records = env.as_contract(&contract_id, || {
            get_payment_records(&env, &invoice_id, 0, 1000).unwrap()
        });
        assert_eq!(all_records.len(), 1000, "Should return all 1000 records");

        // Query beyond available records should return empty
        let beyond = env.as_contract(&contract_id, || {
            get_payment_records(&env, &invoice_id, 1000, 100).unwrap()
        });
        assert_eq!(beyond.len(), 0, "Query beyond records should return empty");

        // Query starting near end
        let near_end = env.as_contract(&contract_id, || {
            get_payment_records(&env, &invoice_id, 990, 20).unwrap()
        });
        assert_eq!(near_end.len(), 10, "Should return only available 10 records");

        // Verify records are in order
        for i in 0..1000u32 {
            let record = env.as_contract(&contract_id, || {
                get_payment_record(&env, &invoice_id, i).unwrap()
            });
            assert_eq!(record.amount, 1, "Record {} should have amount 1", i);
        }
    }

    /// Test that cap cannot be bypassed by using different invoice IDs.
    /// Each invoice has its own independent payment count.
    #[test]
    fn test_cap_is_per_invoice_not_global() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(QuickLendXContract, ());
        let client = QuickLendXContractClient::new(&env, &contract_id);

        // Create two invoices
        let (invoice_id_a, _biz_a, _inv_a, _cur_a) =
            setup_funded_invoice(&env, &client, &contract_id, 100_000);
        let (invoice_id_b, _biz_b, _inv_b, _cur_b) =
            setup_funded_invoice(&env, &client, &contract_id, 100_000);

        // Make 1000 payments on invoice A (reaching cap)
        for i in 0..1000u32 {
            env.ledger().set_timestamp(36_000 + i as u64);
            let nonce = String::from_str(&env, &format!("invoice-a-{}", i));
            client.process_partial_payment(&invoice_id_a, &1, &nonce);
        }

        // Invoice A should be at cap
        let count_a = env.as_contract(&contract_id, || {
            get_payment_count(&env, &invoice_id_a).unwrap()
        });
        assert_eq!(count_a, 1000, "Invoice A should be at cap");

        // Invoice B should still be at 0 (independent)
        let count_b = env.as_contract(&contract_id, || {
            get_payment_count(&env, &invoice_id_b).unwrap()
        });
        assert_eq!(count_b, 0, "Invoice B should have 0 payments");

        // Invoice B should still accept payments
        env.ledger().set_timestamp(37_000);
        client.process_partial_payment(&invoice_id_b, &1, &String::from_str(&env, "invoice-b-0"));
        
        let count_b_after = env.as_contract(&contract_id, || {
            get_payment_count(&env, &invoice_id_b).unwrap()
        });
        assert_eq!(count_b_after, 1, "Invoice B should accept payments independently");
    }

    /// Test that settled invoices reject additional payments (status guard).
    /// This is a secondary defense after the invoice is marked as Paid.
    #[test]
    fn test_settled_invoice_rejects_additional_payments() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(QuickLendXContract, ());
        let client = QuickLendXContractClient::new(&env, &contract_id);
        
        let (invoice_id, _business, _investor, _currency) =
            setup_funded_invoice(&env, &client, &contract_id, 1_000);

        // Make a payment that completes the invoice
        env.ledger().set_timestamp(38_000);
        client.process_partial_payment(&invoice_id, &1_000, &String::from_str(&env, "full-pay"));

        // Verify invoice is paid
        let invoice = client.get_invoice(&invoice_id);
        assert_eq!(invoice.status, InvoiceStatus::Paid);

        // Try to make another payment on the settled invoice
        env.ledger().set_timestamp(38_100);
        let result = client.try_process_partial_payment(
            &invoice_id,
            &100,
            &String::from_str(&env, "after-settle"),
        );
        assert!(result.is_err(), "Payment on settled invoice should be rejected");
        assert_eq!(
            result.unwrap_err().unwrap(),
            QuickLendXError::InvalidStatus,
            "Error should be InvalidStatus for settled invoice"
        );
    }

    /// Test that the finalization flag is set correctly after settlement.
    #[test]
    fn test_finalization_flag_after_settlement() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(QuickLendXContract, ());
        let client = QuickLendXContractClient::new(&env, &contract_id);
        
        let (invoice_id, _business, _investor, _currency) =
            setup_funded_invoice(&env, &client, &contract_id, 1_000);

        // Before settlement, should not be finalized
        let finalized_before = env.as_contract(&contract_id, || {
            is_invoice_finalized(&env, &invoice_id).unwrap()
        });
        assert!(!finalized_before, "Invoice should not be finalized before payment");

        // Settle the invoice
        env.ledger().set_timestamp(39_000);
        client.process_partial_payment(&invoice_id, &1_000, &String::from_str(&env, "settle"));

        // After settlement, should be finalized
        let finalized_after = env.as_contract(&contract_id, || {
            is_invoice_finalized(&env, &invoice_id).unwrap()
        });
        assert!(finalized_after, "Invoice should be finalized after settlement");
    }

    /// Test that payment records are correctly stored with all fields populated.
    #[test]
    fn test_payment_record_fields_are_complete() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(QuickLendXContract, ());
        let client = QuickLendXContractClient::new(&env, &contract_id);
        
        let (invoice_id, business, _investor, _currency) =
            setup_funded_invoice(&env, &client, &contract_id, 100_000);

        let test_nonce = String::from_str(&env, "complete-record-test");
        env.ledger().set_timestamp(40_000);
        client.process_partial_payment(&invoice_id, &500, &test_nonce);

        let record = env.as_contract(&contract_id, || {
            get_payment_record(&env, &invoice_id, 0).unwrap()
        });

        // Verify all fields are correctly populated
        assert_eq!(record.payer, business, "Payer should match business");
        assert_eq!(record.amount, 500, "Amount should match payment");
        assert_eq!(record.timestamp, 40_000, "Timestamp should match ledger timestamp");
        assert_eq!(record.nonce, test_nonce, "Nonce should match transaction_id");
    }

    /// Test that duplicate nonces are properly deduplicated and don't increment count.
    #[test]
    fn test_duplicate_nonce_does_not_increment_count() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(QuickLendXContract, ());
        let client = QuickLendXContractClient::new(&env, &contract_id);
        
        let (invoice_id, _business, _investor, _currency) =
            setup_funded_invoice(&env, &client, &contract_id, 100_000);

        let duplicate_nonce = String::from_str(&env, "duplicate-nonce");
        
        // First payment with nonce
        env.ledger().set_timestamp(41_000);
        client.process_partial_payment(&invoice_id, &100, &duplicate_nonce);

        // Try duplicate - should be deduplicated (return current progress, not error)
        env.ledger().set_timestamp(41_100);
        let result = client.try_process_partial_payment(&invoice_id, &100, &duplicate_nonce);
        assert!(result.is_ok(), "Duplicate nonce should not error, but be deduplicated");

        // Count should still be 1
        let count = env.as_contract(&contract_id, || {
            get_payment_count(&env, &invoice_id).unwrap()
        });
        assert_eq!(count, 1, "Duplicate nonce should not increment payment count");

        // Total paid should still be 100
        let invoice = client.get_invoice(&invoice_id);
        assert_eq!(invoice.total_paid, 100, "Total paid should not change on duplicate");
    }

    /// Test that empty nonces are allowed and each creates a separate record.
    /// This is important because empty nonces skip the deduplication check.
    #[test]
    fn test_empty_nonce_each_creates_separate_record() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(QuickLendXContract, ());
        let client = QuickLendXContractClient::new(&env, &contract_id);
        
        let (invoice_id, _business, _investor, _currency) =
            setup_funded_invoice(&env, &client, &contract_id, 100_000);

        let empty_nonce = String::from_str(&env, "");
        
        // Make multiple payments with empty nonce
        for i in 0..5u32 {
            env.ledger().set_timestamp(42_000 + i as u64);
            client.process_partial_payment(&invoice_id, &10, &empty_nonce);
        }

        // All 5 should be recorded
        let count = env.as_contract(&contract_id, || {
            get_payment_count(&env, &invoice_id).unwrap()
        });
        assert_eq!(count, 5, "Empty nonces should each create separate records");

        let total = env.as_contract(&contract_id, || {
            get_payment_records(&env, &invoice_id, 0, 10).unwrap()
        });
        assert_eq!(total.len(), 5, "Should have 5 payment records");
        
        // Verify all have empty nonce
        for record in total.iter() {
            assert_eq!(record.nonce, empty_nonce, "All records should have empty nonce");
        }
    }
}
