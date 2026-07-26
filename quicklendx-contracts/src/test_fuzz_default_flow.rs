#![cfg(all(test, feature = "fuzz-tests"))]

use crate::contract::{QuickLendXContract, QuickLendXContractClient};
use crate::errors::QuickLendXError;
use crate::invoice::{InvoiceCategory, InvoiceStatus};
use crate::investment::InvestmentStatus;
use proptest::prelude::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, Env, String, Vec,
};

fn setup_env_and_invoice(
    env: &Env,
    due_date: u64,
) -> (QuickLendXContractClient<'static>, soroban_sdk::BytesN<32>, Address, Address) {
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    client.set_admin(&admin);
    client.initialize_fee_system(&admin);

    let business = Address::generate(env);
    client.submit_kyc_application(&business, &String::from_str(env, "KYC data"));
    client.verify_business(&admin, &business);

    let investor = Address::generate(env);
    client.submit_investor_kyc(&investor, &String::from_str(env, "KYC data"));
    client.verify_investor(&investor, &1000000);

    let token_admin = Address::generate(env);
    let currency = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let sac_client = token::StellarAssetClient::new(env, &currency);
    let token_client = token::Client::new(env, &currency);

    client.add_currency(&admin, &currency);
    let amount = 1000;
    sac_client.mint(&investor, &amount);
    let expiry = env.ledger().sequence() + 10_000;
    token_client.approve(&investor, &client.address, &amount, &expiry);

    let invoice_id = client.store_invoice(
        &business,
        &amount,
        &currency,
        &due_date,
        &String::from_str(env, "Test invoice"),
        &InvoiceCategory::Services,
        &Vec::new(env),
    );
    client.verify_invoice(&invoice_id);
    let bid_id = client.place_bid(&investor, &invoice_id, &amount, &(amount + 100));
    client.accept_bid(&invoice_id, &bid_id);
    
    (client, invoice_id, business, investor)
}

fn cfg_smoke() -> ProptestConfig {
    ProptestConfig {
        cases: 10,
        failure_persistence: None,
        ..ProptestConfig::default()
    }
}

proptest! {
    #![proptest_config(cfg_smoke())]

    #[test]
    fn test_fuzz_default_flow_transitions(
        due_date in 1_000_000u64..2_000_000u64,
        grace_period in 1u64..30 * 24 * 60 * 60,
    ) {
        let env = Env::default();
        let (client, invoice_id, _business, _investor) = setup_env_and_invoice(&env, due_date);
        
        let initial_invoice = client.get_invoice(&invoice_id);
        prop_assert_eq!(initial_invoice.status, InvoiceStatus::Funded);

        // 1. Past-due (after due date, before grace period ends)
        let past_due_time = due_date + grace_period / 2;
        env.ledger().set_timestamp(past_due_time);
        
        // Attempting to default should fail, it's in grace
        let res_grace = client.try_mark_invoice_defaulted(&invoice_id, &Some(grace_period));
        prop_assert!(
            matches!(res_grace, Err(Ok(QuickLendXError::OperationNotAllowed))),
            "Should not default during grace period"
        );
        let grace_invoice = client.get_invoice(&invoice_id);
        prop_assert_eq!(grace_invoice.status, InvoiceStatus::Funded);

        // 2. Default (after grace period)
        let default_time = due_date + grace_period + 1;
        env.ledger().set_timestamp(default_time);
        
        let res_default = client.try_mark_invoice_defaulted(&invoice_id, &Some(grace_period));
        prop_assert!(res_default.is_ok(), "Should successfully default after grace");
        
        let defaulted_invoice = client.get_invoice(&invoice_id);
        prop_assert_eq!(defaulted_invoice.status, InvoiceStatus::Defaulted);

        // Verify investment status also transitioned to Defaulted
        let investment = client.get_invoice_investment(&invoice_id);
        prop_assert_eq!(investment.status, InvestmentStatus::Defaulted);
        
        // 3. Recovery (insurance claim or similar would follow, but state is locked to Defaulted)
        // Ensure no further transition is possible
        let res_double_default = client.try_mark_invoice_defaulted(&invoice_id, &Some(grace_period));
        prop_assert!(
            matches!(res_double_default, Err(Ok(QuickLendXError::DuplicateDefaultTransition))),
            "Should not allow duplicate default transition"
        );
    }
}
