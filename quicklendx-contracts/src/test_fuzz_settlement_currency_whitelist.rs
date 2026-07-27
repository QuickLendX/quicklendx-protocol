//! Property tests for settlement currency whitelist enforcement.
//!
//! Validates that an invoice can only be settled if the invoice currency
//! is present in the settlement currency whitelist (when non-empty).

#[cfg(test)]
mod test_fuzz_settlement_currency_whitelist {
    use crate::errors::QuickLendXError;
    use crate::types::InvoiceCategory;
    use crate::{QuickLendXContract, QuickLendXContractClient};
    use proptest::prelude::*;
    use soroban_sdk::{testutils::Address as _, token, Address, BytesN, Env, String, Vec};

    fn setup_funded_invoice(
        env: &Env,
    ) -> (
        QuickLendXContractClient,
        BytesN<32>,
        Address, // invoice currency
        Address, // other token
    ) {
        let contract_id = env.register(QuickLendXContract, ());
        let client = QuickLendXContractClient::new(env, &contract_id);

        let admin = Address::generate(env);
        let business = Address::generate(env);
        let investor = Address::generate(env);

        client.set_admin(&admin);
        client.initialize_fee_system(&admin);

        let token_admin = Address::generate(env);
        let currency = env.register_stellar_asset_contract_v2(token_admin.clone()).address();
        let token_client = token::Client::new(env, &currency);
        let sac = token::StellarAssetClient::new(env, &currency);

        let other_token_admin = Address::generate(env);
        let other_token = env.register_stellar_asset_contract_v2(other_token_admin.clone()).address();

        let balance: i128 = 500_000;
        sac.mint(&business, &balance);
        sac.mint(&investor, &balance);

        let expiry = env.ledger().sequence() + 10_000;
        token_client.approve(&business, &contract_id, &balance, &expiry);
        token_client.approve(&investor, &contract_id, &balance, &expiry);

        client.submit_kyc_application(&business, &String::from_str(env, "business-kyc"));
        client.verify_business(&admin, &business);
        client.submit_investor_kyc(&investor, &String::from_str(env, "investor-kyc"));
        client.verify_investor(&investor, &balance);

        let amount: i128 = 100_000;
        let due_date = env.ledger().timestamp() + 86_400;
        let invoice_id = client.store_invoice(
            &business,
            &amount,
            &currency,
            &due_date,
            &String::from_str(env, "Fuzz test invoice"),
            &InvoiceCategory::Services,
            &Vec::new(env),
        );
        client.verify_invoice(&invoice_id);
        let bid_id = client.place_bid(&investor, &invoice_id, &amount, &amount);
        client.accept_bid(&invoice_id, &bid_id);

        (client, invoice_id, currency, other_token)
    }

    proptest! {
        #![proptest_config({
            let mut config = ProptestConfig::with_cases(256);
            if let Some(_seed_array) = crate::test_seed::seed() {
                config.rng_algorithm = proptest::test_runner::RngAlgorithm::ChaCha;
            }
            config
        })]

        #[test]
        fn fuzz_settlement_whitelist_enforcement(
            include_invoice_currency in any::<bool>(),
            random_tokens in 0..5usize,
        ) {
            let env = Env::default();
            env.mock_all_auths();

            let (client, invoice_id, currency, other_token) = setup_funded_invoice(&env);

            let mut whitelist: Vec<Address> = Vec::new(&env);
            
            for _ in 0..random_tokens {
                whitelist.push_back(Address::generate(&env));
            }
            
            whitelist.push_back(other_token);

            if include_invoice_currency {
                whitelist.push_back(currency.clone());
            }

            crate::settlement::store_settlement_currencies(&env, &invoice_id, &whitelist);

            let result = client.try_settle_invoice(&invoice_id, &100_000i128);

            if include_invoice_currency {
                prop_assert!(result.is_ok(), "Settlement should succeed if currency is whitelisted");
            } else {
                prop_assert_eq!(
                    result.unwrap_err().unwrap(),
                    QuickLendXError::SettlementCurrencyNotAllowed,
                    "Settlement should fail with SettlementCurrencyNotAllowed if currency is not whitelisted"
                );
            }
        }
    }
}
