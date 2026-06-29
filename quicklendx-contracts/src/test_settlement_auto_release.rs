#[cfg(test)]
mod tests {
    use crate::errors::QuickLendXError;
    use crate::invoice::{InvoiceCategory, InvoiceStatus};
    use crate::settlement::{get_invoice_progress, is_invoice_finalized};
    use crate::{QuickLendXContract, QuickLendXContractClient};
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        token, Address, BytesN, Env, String, Vec,
    };

    // Reuse setup from test_partial_payments.rs
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
            &BytesN::from_array(&env, &[0u8; 32]),
        );
        client.accept_bid(&invoice_id, &bid_id);
        (invoice_id, business, investor, currency)
    }

    #[test]
    fn test_settlement_auto_release_on_final_payment() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(QuickLendXContract, ());
        let client = QuickLendXContractClient::new(&env, &contract_id);

        let invoice_amount = 1_000;
        let (invoice_id, _business, investor, currency) =
            setup_funded_invoice(&env, &client, &contract_id, invoice_amount);

        let token_client = token::Client::new(&env, &currency);
        let investor_balance_before = token_client.balance(&investor);

        // Make partial payments
        env.ledger().set_timestamp(2_000);
        client.process_partial_payment(&invoice_id, &400, &String::from_str(&env, "pay-1"));

        env.ledger().set_timestamp(2_100);
        client.process_partial_payment(&invoice_id, &300, &String::from_str(&env, "pay-2"));

        let progress = env.as_contract(&contract_id, || {
            get_invoice_progress(&env, &invoice_id).unwrap()
        });
        assert_eq!(progress.total_paid, 700);
        assert_eq!(progress.status, InvoiceStatus::Funded);

        // Final partial payment
        env.ledger().set_timestamp(2_200);
        client.process_partial_payment(&invoice_id, &300, &String::from_str(&env, "final-pay"));

        // Verify finalization
        let invoice = client.get_invoice(&invoice_id);
        assert_eq!(invoice.status, InvoiceStatus::Paid);

        let finalized = env.as_contract(&contract_id, || {
            is_invoice_finalized(&env, &invoice_id).unwrap()
        });
        assert!(finalized);

        // Assert balances (investor should have received back their investment)
        let investor_balance_after = token_client.balance(&investor);
        assert!(
            investor_balance_after > investor_balance_before,
            "Investor should have received funds"
        );

        // Assert no further payment
        let result = client.try_process_partial_payment(
            &invoice_id,
            &100,
            &String::from_str(&env, "extra-pay"),
        );
        assert!(result.is_err(), "Further payments should be rejected");
        assert_eq!(result.unwrap_err().unwrap(), QuickLendXError::InvalidStatus);
    }
}
