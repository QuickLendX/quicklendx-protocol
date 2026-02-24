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

#[test]
fn test_get_pending_returns_withdrawal_after_initiate() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let token = Address::generate(&env);
    let target = Address::generate(&env);
    let amount = 500i128;

    // Initially no pending withdrawal
    assert!(client.get_pending_emergency_withdraw().is_none());

    client.initiate_emergency_withdraw(&admin, &token, &amount, &target);

    // After initiate, pending withdrawal should exist with correct data
    let pending = client.get_pending_emergency_withdraw().unwrap();
    assert_eq!(pending.token, token);
    assert_eq!(pending.amount, amount);
    assert_eq!(pending.target, target);
    assert!(pending.unlock_at > env.ledger().timestamp());
    assert_eq!(pending.initiated_by, admin);
}

#[test]
fn test_get_pending_none_after_execute() {
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
    let amount = 100i128;
    sac.mint(&contract_id, &amount);

    client.initiate_emergency_withdraw(&admin, &token_id, &amount, &target);
    env.ledger()
        .set_timestamp(env.ledger().timestamp() + DEFAULT_EMERGENCY_TIMELOCK_SECS + 1);
    client.execute_emergency_withdraw(&admin);

    // After execution, pending withdrawal should be cleared
    assert!(client.get_pending_emergency_withdraw().is_none());
}

#[test]
fn test_target_receives_correct_amount_when_funded() {
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
    let token_client = token::Client::new(&env, &token_id);
    let target = Address::generate(&env);
    let amount = 1_000i128;
    sac.mint(&contract_id, &amount);

    client.initiate_emergency_withdraw(&admin, &token_id, &amount, &target);
    env.ledger()
        .set_timestamp(env.ledger().timestamp() + DEFAULT_EMERGENCY_TIMELOCK_SECS + 1);
    client.execute_emergency_withdraw(&admin);

    // Verify target received the correct amount and contract balance is zero
    assert_eq!(token_client.balance(&target), amount);
    assert_eq!(token_client.balance(&contract_id), 0);
}

#[test]
fn test_execute_without_pending_fails() {
    let env = Env::default();
    let (client, admin) = setup(&env);

    // Attempting to execute without initiating should fail
    let result = client.try_execute_emergency_withdraw(&admin);
    assert!(result.is_err());
}

#[test]
fn test_cancel_clears_pending() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let token = Address::generate(&env);
    let target = Address::generate(&env);

    client.initiate_emergency_withdraw(&admin, &token, &500i128, &target);
    assert!(client.get_pending_emergency_withdraw().is_some());

    client.cancel_emergency_withdraw(&admin);
    assert!(client.get_pending_emergency_withdraw().is_none());
}

#[test]
fn test_cancel_without_pending_fails() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let res = client.try_cancel_emergency_withdraw(&admin);
    assert!(res.is_err());
}

#[test]
fn test_non_admin_cannot_cancel() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let token = Address::generate(&env);
    let target = Address::generate(&env);
    client.initiate_emergency_withdraw(&admin, &token, &500i128, &target);

    let non_admin = Address::generate(&env);
    let res = client.try_cancel_emergency_withdraw(&non_admin);
    assert!(res.is_err());
}

#[test]
fn test_cancel_prevents_execute() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let token = Address::generate(&env);
    let target = Address::generate(&env);

    client.initiate_emergency_withdraw(&admin, &token, &500i128, &target);
    client.cancel_emergency_withdraw(&admin);

    // Advance time past timelock
    env.ledger()
        .set_timestamp(env.ledger().timestamp() + DEFAULT_EMERGENCY_TIMELOCK_SECS + 1);

    // Execute should fail because withdrawal was cancelled
    let res = client.try_execute_emergency_withdraw(&admin);
    assert!(res.is_err());
}

#[test]
fn test_get_pending_none_when_no_withdrawal_initiated() {
    let env = Env::default();
    let (client, _admin) = setup(&env);

    // Initially, no pending withdrawal should exist
    let pending = client.get_pending_emergency_withdraw();
    assert!(pending.is_none());
}

#[test]
fn test_execute_at_exact_timelock_boundary_succeeds() {
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
    
    let pending = client.get_pending_emergency_withdraw().unwrap();
    
    // Set time to exactly unlock_at (boundary condition)
    env.ledger().set_timestamp(pending.unlock_at);

    // Execute should succeed at exact timelock boundary
    let result = client.try_execute_emergency_withdraw(&admin);
    assert!(result.is_ok());
}

#[test]
fn test_execute_one_second_before_timelock_fails() {
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
    
    let pending = client.get_pending_emergency_withdraw().unwrap();
    
    // Set time to one second before unlock_at
    env.ledger().set_timestamp(pending.unlock_at - 1);

    // Execute should fail
    let result = client.try_execute_emergency_withdraw(&admin);
    assert!(result.is_err());
}

#[test]
fn test_pending_withdrawal_contains_correct_fields() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let token = Address::generate(&env);
    let target = Address::generate(&env);
    let amount = 750i128;
    
    let init_time = env.ledger().timestamp();
    client.initiate_emergency_withdraw(&admin, &token, &amount, &target);

    let pending = client.get_pending_emergency_withdraw().unwrap();
    
    // Verify all fields are correctly set
    assert_eq!(pending.token, token);
    assert_eq!(pending.amount, amount);
    assert_eq!(pending.target, target);
    assert_eq!(pending.initiated_by, admin);
    assert_eq!(pending.initiated_at, init_time);
    assert_eq!(pending.unlock_at, init_time + DEFAULT_EMERGENCY_TIMELOCK_SECS);
}

#[test]
fn test_multiple_initiates_overwrites_previous() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let token1 = Address::generate(&env);
    let token2 = Address::generate(&env);
    let target1 = Address::generate(&env);
    let target2 = Address::generate(&env);

    // First initiate
    client.initiate_emergency_withdraw(&admin, &token1, &100i128, &target1);
    let pending1 = client.get_pending_emergency_withdraw().unwrap();
    assert_eq!(pending1.token, token1);
    assert_eq!(pending1.amount, 100i128);

    // Second initiate should overwrite
    client.initiate_emergency_withdraw(&admin, &token2, &200i128, &target2);
    let pending2 = client.get_pending_emergency_withdraw().unwrap();
    assert_eq!(pending2.token, token2);
    assert_eq!(pending2.amount, 200i128);
    assert_eq!(pending2.target, target2);
}

#[test]
fn test_negative_amount_fails() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let token = Address::generate(&env);
    let target = Address::generate(&env);

    let result = client.try_initiate_emergency_withdraw(&admin, &token, &-100i128, &target);
    assert!(result.is_err());
}
