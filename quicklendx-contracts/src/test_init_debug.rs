#![cfg(test)]

use super::*;
use crate::admin::{ADMIN_INITIALIZED_KEY, ADMIN_KEY};
use crate::init::InitializationParams;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{symbol_short, Address, Env, Vec};

fn setup_debug() -> (Env, QuickLendXContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    (env, client, contract_id)
}

fn base_params(env: &Env) -> InitializationParams {
    InitializationParams {
        admin: Address::generate(env),
        treasury: Address::generate(env),
        fee_bps: 200,
        min_invoice_amount: 1_000_000,
        max_due_date_days: 365,
        grace_period_seconds: 604800,
        initial_currencies: Vec::new(env),
    }
}

#[test]
fn test_rejects_partial_admin_state() {
    let (env, client, contract_id) = setup_debug();
    let params = base_params(&env);

    env.as_contract(&contract_id, || {
        env.storage().instance().set(&ADMIN_INITIALIZED_KEY, &true);
        env.storage().instance().set(&ADMIN_KEY, &params.admin);
    });

    let result = client.try_initialize(&params);
    assert_eq!(result, Err(Ok(QuickLendXError::OperationNotAllowed)));
}

#[test]
fn test_rejects_partial_currency_state() {
    let (env, client, contract_id) = setup_debug();
    let params = base_params(&env);

    let whitelist_key = symbol_short!("curr_wl");
    let currencies = Vec::from_array(&env, [Address::generate(&env)]);
    env.as_contract(&contract_id, || {
        env.storage().instance().set(&whitelist_key, &currencies);
    });

    let result = client.try_initialize(&params);
    assert_eq!(result, Err(Ok(QuickLendXError::OperationNotAllowed)));
}

#[test]
fn test_rejects_partial_protocol_limits_state() {
    let (env, client, contract_id) = setup_debug();
    let params = base_params(&env);

    let limits_key = "protocol_limits";
    env.as_contract(&contract_id, || {
        env.storage().instance().set(&limits_key, &true);
    });

    let result = client.try_initialize(&params);
    assert_eq!(result, Err(Ok(QuickLendXError::OperationNotAllowed)));
}

#[test]
fn test_initialized_flag_without_state_returns_storage_error() {
    let (env, client, contract_id) = setup_debug();
    let params = base_params(&env);

    let init_key = symbol_short!("proto_in");
    env.as_contract(&contract_id, || {
        env.storage().instance().set(&init_key, &true);
    });

    let result = client.try_initialize(&params);
    assert_eq!(result, Err(Ok(QuickLendXError::StorageKeyNotFound)));
}
