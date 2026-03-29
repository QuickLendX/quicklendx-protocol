#[cfg(test)]
mod tests {
    use crate::errors::QuickLendXError;
    use crate::invoice::{InvoiceCategory, InvoiceStatus};
    use crate::settlement::{
        get_invoice_progress, get_payment_count, get_payment_record, get_payment_records,
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
        let business = Address::generate(env);
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

    // Comprehensive tests for partial payments and settlement
    // ============================================================================
    // HELPER FUNCTIONS 
    // ============================================================================
    fn setup_env() -> (Env, QuickLendXContractClient<'static>, Address, Address) {
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

    #[test]
    fn test_duplicate_transaction_id_idempotency() {
        let (env, client, _admin, contract_id) = setup_env();
        let (invoice_id, _business, _investor, _currency) =
            setup_funded_invoice(&env, &client, &contract_id, 2_000);
        
        let tx_id = String::from_str(&env, "resubmit-me");
        
        // First submission
        client.process_partial_payment(&invoice_id, &500, &tx_id);
        let progress_1 = client.get_invoice_progress(&invoice_id);
        assert_eq!(progress_1.total_paid, 500);
        assert_eq!(progress_1.payment_count, 1);
        
        // Resubmission of SAME transaction_id
        client.process_partial_payment(&invoice_id, &500, &tx_id);
        let progress_2 = client.get_invoice_progress(&invoice_id);
        
        assert_eq!(progress_2.total_paid, 500);
        assert_eq!(progress_2.payment_count, 1);
    }

    #[test]
    fn test_payment_ordering_integrity() {
        let (env, client, _admin, contract_id) = setup_env();
        let (invoice_id, _business, _investor, _currency) =
            setup_funded_invoice(&env, &client, &contract_id, 5_000);
            
        for i in 0..10 {
            env.ledger().set_timestamp(10_000 + i as u64);
            let tx_id = String::from_str(&env, &format!("tx-{}", i));
            client.process_partial_payment(&invoice_id, &(100 + i as i128), &tx_id);
        }
        
        let count = client.get_payment_count(&invoice_id);
        assert_eq!(count, 10);
        
        let records = client.get_payment_records(&invoice_id, &0, &10);
        assert_eq!(records.len(), 10);
        
        for i in 0..10 {
            let record = records.get(i as u32).unwrap();
            assert_eq!(record.amount, 100 + i as i128);
            assert_eq!(record.timestamp, 10_000 + i as u64);
        }
    }

    #[test]
    fn test_pagination_and_limits() {
        let (env, client, _admin, contract_id) = setup_env();
        let (invoice_id, _business, _investor, _currency) =
            setup_funded_invoice(&env, &client, &contract_id, 10_000);
            
        for i in 0..15 {
            let tx_id = String::from_str(&env, &format!("p-{}", i));
            client.process_partial_payment(&invoice_id, &100, &tx_id);
        }
        
        let page_1 = client.get_payment_records(&invoice_id, &0, &5);
        assert_eq!(page_1.len(), 5);
        assert_eq!(page_1.get(0).unwrap().nonce, String::from_str(&env, "p-0"));
        
        let page_2 = client.get_payment_records(&invoice_id, &5, &5);
        assert_eq!(page_2.len(), 5);
        assert_eq!(page_2.get(0).unwrap().nonce, String::from_str(&env, "p-5"));
        
        let page_3 = client.get_payment_records(&invoice_id, &10, &100);
        assert_eq!(page_3.len(), 5);
        
        let page_empty = client.get_payment_records(&invoice_id, &15, &5);
        assert_eq!(page_empty.len(), 0);
    }

    #[test]
    fn test_overpayment_capping_order() {
        let (env, client, _admin, contract_id) = setup_env();
        let (invoice_id, _business, _investor, _currency) =
            setup_funded_invoice(&env, &client, &contract_id, 1_000);
            
        client.process_partial_payment(&invoice_id, &600, &String::from_str(&env, "tx-a"));
        client.process_partial_payment(&invoice_id, &1000, &String::from_str(&env, "tx-b"));
        
        let records = client.get_payment_records(&invoice_id, &0, &10);
        assert_eq!(records.len(), 2);
        assert_eq!(records.get(1).unwrap().amount, 400); 
        
        let invoice = client.get_invoice(&invoice_id);
        assert_eq!(invoice.status, InvoiceStatus::Paid);
    }
}
