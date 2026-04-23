#[cfg(test)]
mod test_settlement_accounting_identity {
    use crate::errors::QuickLendXError;
    use crate::invoice::{InvoiceCategory, InvoiceStatus};
    use crate::profits::calculate_profit;
    use crate::settlement::{get_invoice_progress, get_payment_count, get_payment_records};
    use crate::{QuickLendXContract, QuickLendXContractClient};
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        token, Address, BytesN, Env, String, Vec,
    };

    fn setup_test_invoice(
        env: &Env,
        client: &QuickLendXContractClient,
        contract_id: &Address,
        invoice_amount: i128,
        investment_amount: i128,
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
            &String::from_str(env, "Invoice for accounting identity tests"),
            &InvoiceCategory::Services,
            &Vec::new(env),
        );
        client.verify_invoice(&invoice_id);
        client.submit_investor_kyc(&investor, &String::from_str(env, "investor-kyc"));
        client.verify_investor(&investor, &initial_balance);
        let bid_id = client.place_bid(
            &investor,
            &invoice_id,
            &investment_amount,
            &(invoice_amount + 100),
        );
        client.accept_bid(&invoice_id, &bid_id);
        (invoice_id, business, investor, currency)
    }

    fn setup_test_invoice_with_fee(
        env: &Env,
        client: &QuickLendXContractClient,
        contract_id: &Address,
        invoice_amount: i128,
        investment_amount: i128,
        fee_bps: u32,
    ) -> (BytesN<32>, Address, Address, Address) {
        let admin = Address::generate(env);
        client.set_admin(&admin);
        client.initialize_fee_system(&admin);
        client.update_platform_fee_bps(&fee_bps);
        setup_test_invoice(env, client, contract_id, invoice_amount, investment_amount)
    }

    #[derive(Clone, Debug)]
    struct AccountingIdentityVector {
        investment_amount: i128,
        payment_amount: i128,
        fee_bps: u32,
        expected_investor_return: i128,
        expected_platform_fee: i128,
    }

    impl AccountingIdentityVector {
        fn test_all() -> Vec<Self> {
            vec![
                AccountingIdentityVector {
                    investment_amount: 1000,
                    payment_amount: 1000,
                    fee_bps: 200,
                    expected_investor_return: 1000,
                    expected_platform_fee: 0,
                },
                AccountingIdentityVector {
                    investment_amount: 1000,
                    payment_amount: 1100,
                    fee_bps: 200,
                    expected_investor_return: 1098,
                    expected_platform_fee: 2,
                },
                AccountingIdentityVector {
                    investment_amount: 1000,
                    payment_amount: 1101,
                    fee_bps: 200,
                    expected_investor_return: 1099,
                    expected_platform_fee: 2,
                },
                AccountingIdentityVector {
                    investment_amount: 1000,
                    payment_amount: 1200,
                    fee_bps: 500,
                    expected_investor_return: 1190,
                    expected_platform_fee: 10,
                },
                AccountingIdentityVector {
                    investment_amount: 1000,
                    payment_amount: 2000,
                    fee_bps: 1000,
                    expected_investor_return: 1900,
                    expected_platform_fee: 100,
                },
                AccountingIdentityVector {
                    investment_amount: 0,
                    payment_amount: 100,
                    fee_bps: 200,
                    expected_investor_return: 100,
                    expected_platform_fee: 0,
                },
                AccountingIdentityVector {
                    investment_amount: 500,
                    payment_amount: 500,
                    fee_bps: 200,
                    expected_investor_return: 500,
                    expected_platform_fee: 0,
                },
                AccountingIdentityVector {
                    investment_amount: 500,
                    payment_amount: 600,
                    fee_bps: 200,
                    expected_investor_return: 598,
                    expected_platform_fee: 2,
                },
                AccountingIdentityVector {
                    investment_amount: 100,
                    payment_amount: 200,
                    fee_bps: 500,
                    expected_investor_return: 195,
                    expected_platform_fee: 5,
                },
                AccountingIdentityVector {
                    investment_amount: 2000,
                    payment_amount: 2100,
                    fee_bps: 100,
                    expected_investor_return: 2099,
                    expected_platform_fee: 1,
                },
                AccountingIdentityVector {
                    investment_amount: 5000,
                    payment_amount: 5500,
                    fee_bps: 50,
                    expected_investor_return: 5498,
                    expected_platform_fee: 2,
                },
                AccountingIdentityVector {
                    investment_amount: 10000,
                    payment_amount: 10500,
                    fee_bps: 0,
                    expected_investor_return: 10500,
                    expected_platform_fee: 0,
                },
            ]
        }
    }

    #[test]
    fn test_accounting_identity_no_profit() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(QuickLendXContract, ());
        let client = QuickLendXContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        client.set_admin(&admin);
        client.initialize_fee_system(&admin);
        client.update_platform_fee_bps(&200u32);

        let (invoice_id, _business, investor, currency) = setup_test_invoice(
            &env,
            &client,
            &contract_id,
            1000,
            1000,
        );
        let token_client = token::Client::new(&env, &currency);
        let initial_investor = token_client.balance(&investor);
        let initial_platform = token_client.balance(&contract_id);

        client.settle_invoice(&invoice_id, &1000);

        let investor_received = token_client.balance(&investor) - initial_investor;
        let platform_received = token_client.balance(&contract_id) - initial_platform;

        assert_eq!(investor_received, 1000);
        assert_eq!(platform_received, 0);
        assert_eq!(investor_received + platform_received, 1000);
    }

    #[test]
    fn test_accounting_identity_exact_profit_2pct() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(QuickLendXContract, ());
        let client = QuickLendXContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        client.set_admin(&admin);
        client.initialize_fee_system(&admin);
        client.update_platform_fee_bps(&200u32);

        let (invoice_id, _business, investor, currency) = setup_test_invoice(
            &env,
            &client,
            &contract_id,
            1100,
            1000,
        );
        let token_client = token::Client::new(&env, &currency);
        let initial_investor = token_client.balance(&investor);
        let initial_platform = token_client.balance(&contract_id);

        client.settle_invoice(&invoice_id, &1100);

        let investor_received = token_client.balance(&investor) - initial_investor;
        let platform_received = token_client.balance(&contract_id) - initial_platform;

        assert_eq!(investor_received, 1098);
        assert_eq!(platform_received, 2);
        assert_eq!(investor_received + platform_received, 1100);
    }

    #[test]
    fn test_accounting_identity_rounding_floor() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(QuickLendXContract, ());
        let client = QuickLendXContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        client.set_admin(&admin);
        client.initialize_fee_system(&admin);
        client.update_platform_fee_bps(&200u32);

        let (invoice_id, _business, investor, currency) = setup_test_invoice(
            &env,
            &client,
            &contract_id,
            1101,
            1000,
        );
        let token_client = token::Client::new(&env, &currency);
        let initial_investor = token_client.balance(&investor);
        let initial_platform = token_client.balance(&contract_id);

        client.settle_invoice(&invoice_id, &1101);

        let investor_received = token_client.balance(&investor) - initial_investor;
        let platform_received = token_client.balance(&contract_id) - initial_platform;

        assert_eq!(platform_received, 2);
        assert_eq!(investor_received, 1099);
        assert_eq!(
            investor_received + platform_received,
            1101,
            "investor_return + platform_fee must equal total_paid (no dust)"
        );
    }

    #[test]
    fn test_accounting_identity_5pct_fee() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(QuickLendXContract, ());
        let client = QuickLendXContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        client.set_admin(&admin);
        client.initialize_fee_system(&admin);
        client.update_platform_fee_bps(&500u32);

        let (invoice_id, _business, investor, currency) = setup_test_invoice(
            &env,
            &client,
            &contract_id,
            1200,
            1000,
        );
        let token_client = token::Client::new(&env, &currency);
        let initial_investor = token_client.balance(&investor);
        let initial_platform = token_client.balance(&contract_id);

        client.settle_invoice(&invoice_id, &1200);

        let investor_received = token_client.balance(&investor) - initial_investor;
        let platform_received = token_client.balance(&contract_id) - initial_platform;

        assert_eq!(platform_received, 10);
        assert_eq!(investor_received, 1190);
        assert_eq!(investor_received + platform_received, 1200);
    }

    #[test]
    fn test_accounting_identity_max_fee_10pct() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(QuickLendXContract, ());
        let client = QuickLendXContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        client.set_admin(&admin);
        client.initialize_fee_system(&admin);
        client.update_platform_fee_bps(&1000u32);

        let (invoice_id, _business, investor, currency) = setup_test_invoice(
            &env,
            &client,
            &contract_id,
            2000,
            1000,
        );
        let token_client = token::Client::new(&env, &currency);
        let initial_investor = token_client.balance(&investor);
        let initial_platform = token_client.balance(&contract_id);

        client.settle_invoice(&invoice_id, &2000);

        let investor_received = token_client.balance(&investor) - initial_investor;
        let platform_received = token_client.balance(&contract_id) - initial_platform;

        assert_eq!(platform_received, 100);
        assert_eq!(investor_received, 1900);
        assert_eq!(investor_received + platform_received, 2000);
    }

    #[test]
    fn test_accounting_identity_zero_investment() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(QuickLendXContract, ());
        let client = QuickLendXContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        client.set_admin(&admin);
        client.initialize_fee_system(&admin);
        client.update_platform_fee_bps(&200u32);

        let (invoice_id, business, investor, currency) =
            setup_test_invoice(&env, &client, &contract_id, 100, 0);

        let token_client = token::Client::new(&env, &currency);
        let initial_investor = token_client.balance(&investor);
        let initial_platform = token_client.balance(&contract_id);

        client.settle_invoice(&invoice_id, &100);

        let investor_received = token_client.balance(&investor) - initial_investor;
        let platform_received = token_client.balance(&contract_id) - initial_platform;

        assert_eq!(platform_received, 0);
        assert_eq!(investor_received, 100);
        assert_eq!(investor_received + platform_received, 100);
    }

    #[test]
    fn test_accounting_identity_small_profit() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(QuickLendXContract, ());
        let client = QuickLendXContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        client.set_admin(&admin);
        client.initialize_fee_system(&admin);
        client.update_platform_fee_bps(&100u32);

        let (invoice_id, _business, investor, currency) = setup_test_invoice(
            &env,
            &client,
            &contract_id,
            2100,
            2000,
        );
        let token_client = token::Client::new(&env, &currency);
        let initial_investor = token_client.balance(&investor);
        let initial_platform = token_client.balance(&contract_id);

        client.settle_invoice(&invoice_id, &2100);

        let investor_received = token_client.balance(&investor) - initial_investor;
        let platform_received = token_client.balance(&contract_id) - initial_platform;

        assert_eq!(platform_received, 1);
        assert_eq!(investor_received, 2099);
        assert_eq!(investor_received + platform_received, 2100);
    }

    #[test]
    fn test_accounting_identity_partial_then_settle() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(QuickLendXContract, ());
        let client = QuickLendXContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        client.set_admin(&admin);
        client.initialize_fee_system(&admin);
        client.update_platform_fee_bps(&200u32);

        let invoice_amount = 1000i128;
        let investment_amount = 900i128;
        let (invoice_id, _business, investor, currency) = setup_test_invoice(
            &env,
            &client,
            &contract_id,
            invoice_amount,
            investment_amount,
        );
        let token_client = token::Client::new(&env, &currency);
        let initial_investor = token_client.balance(&investor);
        let initial_platform = token_client.balance(&contract_id);

        client.process_partial_payment(&invoice_id, &300, &String::from_str(&env, "partial-1"));

        let progress1 = env.as_contract(&contract_id, || {
            get_invoice_progress(&env, &invoice_id).unwrap()
        });
        assert_eq!(progress1.total_paid, 300);
        assert_eq!(progress1.status, InvoiceStatus::Funded);

        client.settle_invoice(&invoice_id, &700);

        let investor_received = token_client.balance(&investor) - initial_investor;
        let platform_received = token_client.balance(&contract_id) - initial_platform;

        let (expected_investor, expected_fee) =
            calculate_profit(&env, investment_amount, invoice_amount);
        assert_eq!(platform_received, expected_fee);
        assert_eq!(investor_received, expected_investor);
        assert_eq!(
            investor_received + platform_received,
            invoice_amount,
            "identity must hold after partial + settle"
        );
    }

    #[test]
    fn test_accounting_identity_multiple_partials_then_settle() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(QuickLendXContract, ());
        let client = QuickLendXContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        client.set_admin(&admin);
        client.initialize_fee_system(&admin);
        client.update_platform_fee_bps(&200u32);

        let invoice_amount = 600i128;
        let investment_amount = 500i128;
        let (invoice_id, _business, investor, currency) = setup_test_invoice(
            &env,
            &client,
            &contract_id,
            invoice_amount,
            investment_amount,
        );
        let token_client = token::Client::new(&env, &currency);
        let initial_investor = token_client.balance(&investor);
        let initial_platform = token_client.balance(&contract_id);

        client.process_partial_payment(&invoice_id, &100, &String::from_str(&env, "mp-1"));
        client.process_partial_payment(&invoice_id, &200, &String::from_str(&env, "mp-2"));
        client.process_partial_payment(&invoice_id, &100, &String::from_str(&env, "mp-3"));

        let progress = env.as_contract(&contract_id, || {
            get_invoice_progress(&env, &invoice_id).unwrap()
        });
        assert_eq!(progress.total_paid, 400);
        assert_eq!(progress.status, InvoiceStatus::Funded);

        client.settle_invoice(&invoice_id, &200);

        let investor_received = token_client.balance(&investor) - initial_investor;
        let platform_received = token_client.balance(&contract_id) - initial_platform;

        let (expected_investor, expected_fee) =
            calculate_profit(&env, investment_amount, invoice_amount);
        assert_eq!(platform_received, expected_fee);
        assert_eq!(investor_received, expected_investor);
        assert_eq!(
            investor_received + platform_received,
            invoice_amount
        );
    }

    #[test]
    fn test_accounting_identity_full_partial_auto_settle() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(QuickLendXContract, ());
        let client = QuickLendXContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        client.set_admin(&admin);
        client.initialize_fee_system(&admin);
        client.update_platform_fee_bps(&200u32);

        let invoice_amount = 500i128;
        let investment_amount = 400i128;
        let (invoice_id, _business, investor, currency) = setup_test_invoice(
            &env,
            &client,
            &contract_id,
            invoice_amount,
            investment_amount,
        );
        let token_client = token::Client::new(&env, &currency);
        let initial_investor = token_client.balance(&investor);
        let initial_platform = token_client.balance(&contract_id);

        client.process_partial_payment(&invoice_id, &200, &String::from_str(&env, "auto-1"));
        client.process_partial_payment(&invoice_id, &300, &String::from_str(&env, "auto-2"));

        let invoice = client.get_invoice(&invoice_id);
        assert_eq!(invoice.status, InvoiceStatus::Paid);

        let investor_received = token_client.balance(&investor) - initial_investor;
        let platform_received = token_client.balance(&contract_id) - initial_platform;

        let (expected_investor, expected_fee) =
            calculate_profit(&env, investment_amount, invoice_amount);
        assert_eq!(platform_received, expected_fee);
        assert_eq!(investor_received, expected_investor);
        assert_eq!(
            investor_received + platform_received,
            invoice_amount
        );
    }

    #[test]
    fn test_rounding_no_dust_across_fee_bps() {
        let vectors = AccountingIdentityVector::test_all();

        for vector in vectors {
            let env = Env::default();
            env.mock_all_auths();
            let contract_id = env.register(QuickLendXContract, ());
            let client = QuickLendXContractClient::new(&env, &contract_id);
            let admin = Address::generate(&env);
            client.set_admin(&admin);
            client.initialize_fee_system(&admin);
            client.update_platform_fee_bps(&vector.fee_bps);

            let (invoice_id, _business, investor, currency) = if vector.investment_amount == 0 {
                let admin = Address::generate(&env);
                let business = Address::generate(&env);
                let token_admin = Address::generate(&env);
                let curr = env
                    .register_stellar_asset_contract_v2(token_admin.clone())
                    .address();
                let sac = token::StellarAssetClient::new(&env, &curr);
                sac.mint(&business, &50_000i128);
                let exp = env.ledger().sequence() + 10_000;
                token::Client::new(&env, &curr)
                    .approve(&business, &contract_id, &50_000i128, &exp);
                client.set_admin(&admin);
                client.submit_kyc_application(&business, &String::from_str(&env, "kyc"));
                client.verify_business(&admin, &business);
                let due_date = env.ledger().timestamp() + 86_400;
                let inv_id = client.store_invoice(
                    &business,
                    &vector.payment_amount,
                    &curr,
                    &due_date,
                    &String::from_str(&env, "test"),
                    &InvoiceCategory::Services,
                    &Vec::new(&env),
                );
                client.verify_invoice(&inv_id);
                (inv_id, business, business, curr)
            } else {
                setup_test_invoice(
                    &env,
                    &client,
                    &contract_id,
                    vector.payment_amount,
                    vector.investment_amount,
                )
            };

            let token_client = token::Client::new(&env, &currency);
            let initial_investor = token_client.balance(&investor);
            let initial_platform = token_client.balance(&contract_id);

            client.settle_invoice(&invoice_id, &vector.payment_amount);

            let investor_received = token_client.balance(&investor) - initial_investor;
            let platform_received = token_client.balance(&contract_id) - initial_platform;

            assert_eq!(
                investor_received + platform_received,
                vector.payment_amount,
                "no dust: investor_return + platform_fee == payment for investment={}, payment={}, fee_bps={}",
                vector.investment_amount,
                vector.payment_amount,
                vector.fee_bps
            );
        }
    }

    #[test]
    fn test_rounding_boundary_minimal_profit() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(QuickLendXContract, ());
        let client = QuickLendXContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        client.set_admin(&admin);
        client.initialize_fee_system(&admin);
        client.update_platform_fee_bps(&50u32);

        let (invoice_id, _business, investor, currency) = setup_test_invoice(
            &env,
            &client,
            &contract_id,
            5001,
            5000,
        );
        let token_client = token::Client::new(&env, &currency);
        let initial_investor = token_client.balance(&investor);
        let initial_platform = token_client.balance(&contract_id);

        client.settle_invoice(&invoice_id, &5001);

        let investor_received = token_client.balance(&investor) - initial_investor;
        let platform_received = token_client.balance(&contract_id) - initial_platform;

        assert_eq!(platform_received, 0);
        assert_eq!(investor_received, 5001);
        assert_eq!(
            investor_received + platform_received,
            5001,
            "minimal profit rounds to zero fee"
        );
    }

    #[test]
    fn test_no_value_leakage_via_rounding() {
        let vectors = AccountingIdentityVector::test_all();

        for vector in vectors {
            let (calc_investor, calc_fee) =
                calculate_profit(&Env::default(), vector.investor_return, vector.payment_amount);
            assert_eq!(
                calc_investor + calc_fee,
                vector.payment_amount,
                "calculate_profit must produce no dust for payment={}",
                vector.payment_amount
            );
        }
    }

    #[test]
    fn test_deterministic_fee_routing() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(QuickLendXContract, ());
        let client = QuickLendXContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        client.set_admin(&admin);
        client.initialize_fee_system(&admin);
        client.update_platform_fee_bps(&200u32);

        let (invoice_id, _business, investor, currency) = setup_test_invoice(
            &env,
            &client,
            &contract_id,
            1500,
            1000,
        );
        let token_client = token::Client::new(&env, &currency);
        let initial_investor = token_client.balance(&investor);
        let initial_platform = token_client.balance(&contract_id);

        client.settle_invoice(&invoice_id, &1500);

        let investor_received = token_client.balance(&investor) - initial_investor;
        let platform_received = token_client.balance(&contract_id) - initial_platform;

        let (expected_investor, expected_fee) =
            calculate_profit(&env, 1000, 1500);
        assert_eq!(investor_received, expected_investor);
        assert_eq!(platform_received, expected_fee);
        assert_eq!(
            investor_received + platform_received,
            1500
        );
    }

    #[test]
    fn test_verify_no_dust_function() {
        use crate::profits::verify_no_dust;

        assert!(verify_no_dust(1000, 0, 1000));
        assert!(verify_no_dust(1098, 2, 1100));
        assert!(verify_no_dust(1099, 2, 1101));
        assert!(verify_no_dust(1190, 10, 1200));
        assert!(verify_no_dust(1900, 100, 2000));
        assert!(verify_no_dust(100, 0, 100));
        assert!(verify_no_dust(598, 2, 600));
        assert!(!verify_no_dust(598, 1, 600));
        assert!(!verify_no_dust(1000, 1, 1000));
    }

    #[test]
    fn test_accounting_identity_zero_fee_bps() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(QuickLendXContract, ());
        let client = QuickLendXContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        client.set_admin(&admin);
        client.initialize_fee_system(&admin);
        client.update_platform_fee_bps(&0u32);

        let (invoice_id, _business, investor, currency) = setup_test_invoice(
            &env,
            &client,
            &contract_id,
            5000,
            1000,
        );
        let token_client = token::Client::new(&env, &currency);
        let initial_investor = token_client.balance(&investor);
        let initial_platform = token_client.balance(&contract_id);

        client.settle_invoice(&invoice_id, &5000);

        let investor_received = token_client.balance(&investor) - initial_investor;
        let platform_received = token_client.balance(&contract_id) - initial_platform;

        assert_eq!(platform_received, 0);
        assert_eq!(investor_received, 5000);
        assert_eq!(investor_received + platform_received, 5000);
    }

    #[test]
    fn test_accounting_identity_max_profit_scenario() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(QuickLendXContract, ());
        let client = QuickLendXContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        client.set_admin(&admin);
        client.initialize_fee_system(&admin);
        client.update_platform_fee_bps(&1000u32);

        let (invoice_id, _business, investor, currency) = setup_test_invoice(
            &env,
            &client,
            &contract_id,
            100_000,
            10_000,
        );
        let token_client = token::Client::new(&env, &currency);
        let initial_investor = token_client.balance(&investor);
        let initial_platform = token_client.balance(&contract_id);

        client.settle_invoice(&invoice_id, &100_000);

        let investor_received = token_client.balance(&investor) - initial_investor;
        let platform_received = token_client.balance(&contract_id) - initial_platform;

        assert_eq!(platform_received, 90_000);
        assert_eq!(investor_received, 10_000);
        assert_eq!(investor_received + platform_received, 100_000);
    }

    #[test]
    fn test_accounting_identity_payment_equal_investment() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(QuickLendXContract, ());
        let client = QuickLendXContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        client.set_admin(&admin);
        client.initialize_fee_system(&admin);
        client.update_platform_fee_bps(&200u32);

        let (invoice_id, _business, investor, currency) = setup_test_invoice(
            &env,
            &client,
            &contract_id,
            500,
            500,
        );
        let token_client = token::Client::new(&env, &currency);
        let initial_investor = token_client.balance(&investor);
        let initial_platform = token_client.balance(&contract_id);

        client.settle_invoice(&invoice_id, &500);

        let investor_received = token_client.balance(&investor) - initial_investor;
        let platform_received = token_client.balance(&contract_id) - initial_platform;

        assert_eq!(platform_received, 0);
        assert_eq!(investor_received, 500);
        assert_eq!(investor_received + platform_received, 500);
    }

    #[test]
    fn test_accounting_identity_partials_equal_payment_total() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(QuickLendXContract, ());
        let client = QuickLendXContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        client.set_admin(&admin);
        client.initialize_fee_system(&admin);
        client.update_platform_fee_bps(&200u32);

        let invoice_amount = 1000i128;
        let investment_amount = 900i128;
        let (invoice_id, _business, investor, currency) = setup_test_invoice(
            &env,
            &client,
            &contract_id,
            invoice_amount,
            investment_amount,
        );
        let token_client = token::Client::new(&env, &currency);
        let initial_investor = token_client.balance(&investor);
        let initial_platform = token_client.balance(&contract_id);

        let partial_amounts = [100, 200, 300, 400];
        for (i, &amt) in partial_amounts.iter().enumerate() {
            env.ledger().set_timestamp(1_000 + i as u64 * 100);
            client.process_partial_payment(
                &invoice_id,
                &amt,
                &String::from_str(&env, &format!("pt-{}", i)),
            );
        }

        let invoice = client.get_invoice(&invoice_id);
        assert_eq!(invoice.status, InvoiceStatus::Paid);

        let investor_received = token_client.balance(&investor) - initial_investor;
        let platform_received = token_client.balance(&contract_id) - initial_platform;

        assert_eq!(
            investor_received + platform_received,
            invoice_amount
        );
    }
}