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

        let progress = env.as_contract(&contract_id, || {
            get_invoice_progress(&env, &invoice_id).unwrap()
        });
        assert_eq!(progress.progress_percent, 100);
        assert_eq!(progress.remaining_due, 0);
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

        let progress = env.as_contract(&contract_id, || {
            get_invoice_progress(&env, &invoice_id).unwrap()
        });
        assert_eq!(progress.total_paid, progress.total_due);

        let second_record = env.as_contract(&contract_id, || {
            get_payment_record(&env, &invoice_id, 1).unwrap()
        });
        assert_eq!(second_record.amount, 200);
        assert_eq!(second_record.timestamp, 3_100);
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
    fn test_negative_amount_rejected() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(QuickLendXContract, ());
        let client = QuickLendXContractClient::new(&env, &contract_id);

        let (invoice_id, _business, _investor, _currency) =
            setup_funded_invoice(&env, &client, &contract_id, 1_000);

        let result =
            client.try_process_partial_payment(&invoice_id, &-50, &String::from_str(&env, "neg"));
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().unwrap(), QuickLendXError::InvalidAmount);
    }

    #[test]
    fn test_payment_after_invoice_paid_is_rejected() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(QuickLendXContract, ());
        let client = QuickLendXContractClient::new(&env, &contract_id);

        let (invoice_id, _business, _investor, _currency) =
            setup_funded_invoice(&env, &client, &contract_id, 1_000);

        env.ledger().set_timestamp(4_000);
        client.process_partial_payment(&invoice_id, &1_000, &String::from_str(&env, "full"));

        let result = client.try_process_partial_payment(
            &invoice_id,
            &1,
            &String::from_str(&env, "after-paid"),
        );
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().unwrap(), QuickLendXError::InvalidStatus);
    }

    #[test]
    fn test_payment_to_cancelled_invoice_is_rejected() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(QuickLendXContract, ());
        let client = QuickLendXContractClient::new(&env, &contract_id);

        let (invoice_id, _business) = setup_cancelled_invoice(&env, &client);

        let result = client.try_process_partial_payment(
            &invoice_id,
            &100,
            &String::from_str(&env, "cancelled"),
        );
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().unwrap(), QuickLendXError::InvalidStatus);
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

        let count = env.as_contract(&contract_id, || {
            get_payment_count(&env, &invoice_id).unwrap()
        });
        assert_eq!(count, 3);

        let records = env.as_contract(&contract_id, || {
            get_payment_records(&env, &invoice_id, 0, 10).unwrap()
        });
        assert_eq!(records.len(), 3);

        let first = records.get(0).unwrap();
        let second = records.get(1).unwrap();
        let third = records.get(2).unwrap();

        assert_eq!(first.payer, business);
        assert_eq!(first.amount, 100);
        assert_eq!(first.timestamp, 5_001);

        assert_eq!(second.payer, business);
        assert_eq!(second.amount, 200);
        assert_eq!(second.timestamp, 5_002);

        assert_eq!(third.payer, business);
        assert_eq!(third.amount, 300);
        assert_eq!(third.timestamp, 5_003);
    }

    #[test]
    fn test_lifecycle_create_invoice_to_paid_with_multiple_payments() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(QuickLendXContract, ());
        let client = QuickLendXContractClient::new(&env, &contract_id);

        let (invoice_id, _business, _investor, _currency) =
            setup_funded_invoice(&env, &client, &contract_id, 1_000);

        env.ledger().set_timestamp(6_000);
        client.process_partial_payment(&invoice_id, &250, &String::from_str(&env, "life-1"));

        env.ledger().set_timestamp(6_100);
        client.process_partial_payment(&invoice_id, &250, &String::from_str(&env, "life-2"));

        env.ledger().set_timestamp(6_200);
        client.process_partial_payment(&invoice_id, &500, &String::from_str(&env, "life-3"));

        let invoice = client.get_invoice(&invoice_id);
        assert_eq!(invoice.total_paid, 1_000);
        assert_eq!(invoice.status, InvoiceStatus::Paid);

        let progress = env.as_contract(&contract_id, || {
            get_invoice_progress(&env, &invoice_id).unwrap()
        });
        assert_eq!(progress.payment_count, 3);
        assert_eq!(progress.progress_percent, 100);
        assert_eq!(progress.remaining_due, 0);
    }
}
