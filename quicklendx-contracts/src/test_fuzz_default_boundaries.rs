#![cfg(all(test, feature = "fuzz-tests"))]

use crate::contract::{QuickLendXContract, QuickLendXContractClient};
use crate::errors::QuickLendXError;
use crate::invoice::InvoiceCategory;
use proptest::prelude::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, Env, String, Vec,
};

fn setup_env_and_invoice(
    env: &Env,
    due_date: u64,
) -> (QuickLendXContractClient<'static>, soroban_sdk::BytesN<32>) {
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
    
    (client, invoice_id)
}

fn cfg_smoke() -> ProptestConfig {
    ProptestConfig {
        cases: 10,
        failure_persistence: None,
        ..ProptestConfig::default()
    }
}

fn cfg_full() -> ProptestConfig {
    ProptestConfig {
        cases: 1000,
        failure_persistence: None,
        ..ProptestConfig::default()
    }
}

proptest! {
    #![proptest_config(cfg_full())]

    #[test]
    fn test_fuzz_default_boundary(
        due_date in 1_000_000u64..2_000_000u64,
        grace_period in 0u64..30 * 24 * 60 * 60, // up to MAX_GRACE_PERIOD
        time_offset in -86400i64..86400i64 // check times around the boundary
    ) {
        let env = Env::default();
        let (client, invoice_id) = setup_env_and_invoice(&env, due_date);
        
        let target_timestamp = (due_date + grace_period) as i64 + time_offset;
        prop_assume!(target_timestamp >= 0);
        
        env.ledger().set_timestamp(target_timestamp as u64);
        
        let res = client.try_mark_invoice_defaulted(&invoice_id, &Some(grace_period));
        
        // The default boundary strictly requires current_timestamp > due_date + grace_period
        if target_timestamp as u64 > due_date + grace_period {
            // Should successfully transition to defaulted
            prop_assert!(res.is_ok(), "Expected success when past grace deadline, got {:?}", res);
        } else {
            // Should fail with OperationNotAllowed
            prop_assert!(
                matches!(res, Err(Ok(QuickLendXError::OperationNotAllowed))),
                "Expected OperationNotAllowed when at or before grace deadline, got {:?}", res
            );
        }
    }
}

proptest! {
    #![proptest_config(cfg_smoke())]

    #[test]
    fn test_fuzz_default_boundary_smoke(
        due_date in 1_000_000u64..2_000_000u64,
        grace_period in 0u64..30 * 24 * 60 * 60,
        time_offset in -86400i64..86400i64
    ) {
        let env = Env::default();
        let (client, invoice_id) = setup_env_and_invoice(&env, due_date);
        
        let target_timestamp = (due_date + grace_period) as i64 + time_offset;
        prop_assume!(target_timestamp >= 0);
        
        env.ledger().set_timestamp(target_timestamp as u64);
        
        let res = client.try_mark_invoice_defaulted(&invoice_id, &Some(grace_period));
        
        if target_timestamp as u64 > due_date + grace_period {
            prop_assert!(res.is_ok());
        } else {
            prop_assert!(matches!(res, Err(Ok(QuickLendXError::OperationNotAllowed))));
        }
    }
}
