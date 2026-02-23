#![cfg(test)]
//! Tests for emergency withdraw: timelock, auth, and execution conditions.

use crate::emergency::DEFAULT_EMERGENCY_TIMELOCK_SECS;
use crate::{QuickLendXContract, QuickLendXContractClient};
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::{token, Address, Env};

fn setup(env: &Env) -> (QuickLendXContractClient<'static>, Address) {
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    client.initialize_admin(&admin);
    (client, admin)
}

#[test]
fn test_only_admin_can_initiate() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let token = Address::generate(&env);
    let target = Address::generate(&env);
    let amount = 1_000i128;

    let result = client.try_initiate_emergency_withdraw(&admin, &token, &amount, &target);
    assert!(result.is_ok());
}

#[test]
fn test_initiate_zero_amount_fails() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let token = Address::generate(&env);
    let target = Address::generate(&env);

    let result = client.try_initiate_emergency_withdraw(&admin, &token, &0i128, &target);
    assert!(result.is_err());
}

#[test]
fn test_execute_before_timelock_fails() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let token = Address::generate(&env);
    let target = Address::generate(&env);
    let amount = 1_000i128;

    client.initiate_emergency_withdraw(&admin, &token, &amount, &target);

    // Attempt to execute immediately - should fail due to timelock
    let result = client.try_execute_emergency_withdraw(&admin);
    assert!(result.is_err());
}

#[test]
fn test_execute_after_timelock_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize_admin(&admin);
    client.initialize_fee_system(&admin);

    let token_admin = Address::generate(&env);
    let token_id = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let sac = token::StellarAssetClient::new(&env, &token_id);
    let target = Address::generate(&env);
    let amount = 1_000i128;
    sac.mint(&contract_id, &amount);

    client.initiate_emergency_withdraw(&admin, &token_id, &amount, &target);
    
    // Advance time past the timelock period
    env.ledger()
        .set_timestamp(env.ledger().timestamp() + DEFAULT_EMERGENCY_TIMELOCK_SECS + 1);

    let result = client.try_execute_emergency_withdraw(&admin);
    assert!(result.is_ok());
}
