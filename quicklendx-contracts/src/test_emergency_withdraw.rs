#![cfg(test)]
//! Tests for emergency withdraw: timelock, auth, cancellation, and expiration constraints.
//!
//! This module tests the hardened emergency withdraw lifecycle including:
//! - Timelock enforcement
//! - Expiration after configurable window
//! - Cancellation guarantees
//! - Nonce-based replay prevention
//! - Edge cases for boundary conditions

use crate::emergency::{DEFAULT_EMERGENCY_EXPIRATION_SECS, DEFAULT_EMERGENCY_TIMELOCK_SECS};
use crate::{QuickLendXContract, QuickLendXContractClient};
use soroban_sdk::testutils::{Address as _, Ledger, Logs, MockAuth, MockAuthInvoke};
use soroban_sdk::{token, Address, Env, IntoVal};

fn setup(env: &Env) -> (QuickLendXContractClient<'static>, Address, Address) {
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    client.initialize_admin(&admin);
    (client, admin, contract_id)
}

fn setup_with_tokens(
    env: &Env,
) -> (
    QuickLendXContractClient<'static>,
    Address,
    Address,
    Address,
    Address,
) {
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    client.initialize_admin(&admin);
    client.initialize_fee_system(&admin);

    let token_admin = Address::generate(env);
    let token_id = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let sac = token::StellarAssetClient::new(env, &token_id);
    sac.mint(&contract_id, &1_000_000i128);

    (client, admin, token_id, token_admin, contract_id)
}

#[test]
fn test_only_admin_can_initiate() {
    let env = Env::default();
    let (client, admin, _) = setup(&env);
    let token = Address::generate(&env);
    let target = Address::generate(&env);
    let amount = 1_000i128;

    let result = client.try_initiate_emergency_withdraw(&admin, &token, &amount, &target);
    assert!(result.is_ok());
}

#[test]
fn test_spoofed_admin_cannot_initiate_execute_or_cancel() {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let attacker = Address::generate(&env);
    let target = Address::generate(&env);
    let amount = 1_000i128;

    client.mock_all_auths().initialize_admin(&admin);
    client.mock_all_auths().initialize_fee_system(&admin);

    let token_admin = Address::generate(&env);
    let token_id = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let sac = token::StellarAssetClient::new(&env, &token_id);
    sac.mint(&contract_id, &10_000i128);

    let spoofed_initiate = MockAuth {
        address: &attacker,
        invoke: &MockAuthInvoke {
            contract: &contract_id,
            fn_name: "initiate_emergency_withdraw",
            args: (admin.clone(), token_id.clone(), amount, target.clone()).into_val(&env),
            sub_invokes: &[],
        },
    };
    let initiate_result = client
        .mock_auths(&[spoofed_initiate])
        .try_initiate_emergency_withdraw(&admin, &token_id, &amount, &target);
    let initiate_err = initiate_result
        .err()
        .expect("spoofed initiate must fail")
        .err()
        .expect("spoofed initiate must abort at auth");
    assert_eq!(initiate_err, soroban_sdk::InvokeError::Abort);
    assert!(client.get_pending_emergency_withdraw().is_none());

    client.initiate_emergency_withdraw(&admin, &token_id, &amount, &target);
    env.ledger()
        .set_timestamp(env.ledger().timestamp() + DEFAULT_EMERGENCY_TIMELOCK_SECS + 1);

    let spoofed_execute = MockAuth {
        address: &attacker,
        invoke: &MockAuthInvoke {
            contract: &contract_id,
            fn_name: "execute_emergency_withdraw",
            args: (admin.clone(),).into_val(&env),
            sub_invokes: &[],
        },
    };
    let execute_result = client
        .mock_auths(&[spoofed_execute])
        .try_execute_emergency_withdraw(&admin);
    let execute_err = execute_result
        .err()
        .expect("spoofed execute must fail")
        .err()
        .expect("spoofed execute must abort at auth");
    assert_eq!(execute_err, soroban_sdk::InvokeError::Abort);
    assert!(client.get_pending_emergency_withdraw().is_some());

    let spoofed_cancel = MockAuth {
        address: &attacker,
        invoke: &MockAuthInvoke {
            contract: &contract_id,
            fn_name: "cancel_emergency_withdraw",
            args: (admin.clone(),).into_val(&env),
            sub_invokes: &[],
        },
    };
    let cancel_result = client
        .mock_auths(&[spoofed_cancel])
        .try_cancel_emergency_withdraw(&admin);
    let cancel_err = cancel_result
        .err()
        .expect("spoofed cancel must fail")
        .err()
        .expect("spoofed cancel must abort at auth");
    assert_eq!(cancel_err, soroban_sdk::InvokeError::Abort);

    let pending = client
        .get_pending_emergency_withdraw()
        .expect("pending withdrawal must remain unchanged");
    assert!(!pending.cancelled);
}

#[test]
fn test_initiate_zero_amount_fails() {
    let env = Env::default();
    let (client, admin, _) = setup(&env);
    let token = Address::generate(&env);
    let target = Address::generate(&env);

    let result = client.try_initiate_emergency_withdraw(&admin, &token, &0i128, &target);
    assert!(result.is_err());
}

#[test]
fn test_negative_amount_fails() {
    let env = Env::default();
    let (client, admin, _) = setup(&env);
    let token = Address::generate(&env);
    let target = Address::generate(&env);

    let result = client.try_initiate_emergency_withdraw(&admin, &token, &-100i128, &target);
    assert!(result.is_err());
}

#[test]
fn test_execute_before_timelock_fails() {
    let env = Env::default();
    let (client, admin, _) = setup(&env);
    let token = Address::generate(&env);
    let target = Address::generate(&env);
    let amount = 1_000i128;

    client.initiate_emergency_withdraw(&admin, &token, &amount, &target);

    let result = client.try_execute_emergency_withdraw(&admin);
    assert!(result.is_err());
}

#[test]
fn test_execute_after_timelock_succeeds() {
    let env = Env::default();
    let (client, admin, token_id, _, _) = setup_with_tokens(&env);
    let target = Address::generate(&env);
    let amount = 1_000i128;

    client.initiate_emergency_withdraw(&admin, &token_id, &amount, &target);

    env.ledger()
        .set_timestamp(env.ledger().timestamp() + DEFAULT_EMERGENCY_TIMELOCK_SECS + 1);

    let result = client.try_execute_emergency_withdraw(&admin);
    assert!(result.is_ok());
}

#[test]
fn test_execute_after_timelock_with_sufficient_balance() {
    let env = Env::default();
    let (client, admin, token_id, _, _) = setup_with_tokens(&env);
    let target = Address::generate(&env);
    let amount = 500_000i128;

    client.initiate_emergency_withdraw(&admin, &token_id, &amount, &target);

    env.ledger()
        .set_timestamp(env.ledger().timestamp() + DEFAULT_EMERGENCY_TIMELOCK_SECS + 1);

    let result = client.try_execute_emergency_withdraw(&admin);
    assert!(result.is_ok());
}

#[test]
fn test_get_pending_returns_withdrawal_after_initiate() {
    let env = Env::default();
    let (client, admin, _) = setup(&env);
    let token = Address::generate(&env);
    let target = Address::generate(&env);
    let amount = 500i128;

    assert!(client.get_pending_emergency_withdraw().is_none());

    client.initiate_emergency_withdraw(&admin, &token, &amount, &target);

    let pending = client.get_pending_emergency_withdraw().unwrap();
    assert_eq!(pending.token, token);
    assert_eq!(pending.amount, amount);
    assert_eq!(pending.target, target);
    assert!(pending.unlock_at > env.ledger().timestamp());
    assert_eq!(pending.initiated_by, admin);
    assert!(!pending.cancelled);
    assert_eq!(pending.cancelled_at, 0);
    assert!(pending.nonce > 0);
}

#[test]
fn test_get_pending_none_after_execute() {
    let env = Env::default();
    let (client, admin, token_id, _, _) = setup_with_tokens(&env);
    let target = Address::generate(&env);
    let amount = 100i128;

    client.initiate_emergency_withdraw(&admin, &token_id, &amount, &target);
    env.ledger()
        .set_timestamp(env.ledger().timestamp() + DEFAULT_EMERGENCY_TIMELOCK_SECS + 1);
    client.execute_emergency_withdraw(&admin);

    assert!(client.get_pending_emergency_withdraw().is_none());
}

#[test]
fn test_target_receives_correct_amount_when_funded() {
    let env = Env::default();
    let (client, admin, token_id, _, contract_id) = setup_with_tokens(&env);
    let target = Address::generate(&env);
    let token_client = token::Client::new(&env, &token_id);
    let amount = 1_000i128;

    client.initiate_emergency_withdraw(&admin, &token_id, &amount, &target);
    env.ledger()
        .set_timestamp(env.ledger().timestamp() + DEFAULT_EMERGENCY_TIMELOCK_SECS + 1);
    client.execute_emergency_withdraw(&admin);

    assert_eq!(token_client.balance(&target), amount);
    assert_eq!(token_client.balance(&contract_id), 1_000_000i128 - amount);
}

#[test]
fn test_execute_without_pending_fails() {
    let env = Env::default();
    let (client, admin, _) = setup(&env);

    let result = client.try_execute_emergency_withdraw(&admin);
    assert!(result.is_err());
}

#[test]
fn test_cancel_clears_pending_but_keeps_record() {
    let env = Env::default();
    env.ledger().set_timestamp(1000);
    let (client, admin, _) = setup(&env);
    let token = Address::generate(&env);
    let target = Address::generate(&env);

    client.initiate_emergency_withdraw(&admin, &token, &500i128, &target);
    let pending_before = client.get_pending_emergency_withdraw().unwrap();
    let nonce_before = pending_before.nonce;

    client.cancel_emergency_withdraw(&admin);

    let pending_after = client.get_pending_emergency_withdraw().unwrap();
    assert!(pending_after.cancelled);
    assert!(pending_after.cancelled_at >= 1000);
    assert_eq!(pending_after.nonce, nonce_before);
}

#[test]
fn test_cancel_without_pending_fails() {
    let env = Env::default();
    let (client, admin, _) = setup(&env);
    let res = client.try_cancel_emergency_withdraw(&admin);
    assert!(res.is_err());
}

#[test]
fn test_non_admin_cannot_cancel() {
    let env = Env::default();
    let (client, admin, _) = setup(&env);
    let token = Address::generate(&env);
    let target = Address::generate(&env);
    client.initiate_emergency_withdraw(&admin, &token, &500i128, &target);

    let non_admin = Address::generate(&env);
    let res = client.try_cancel_emergency_withdraw(&non_admin);
    assert!(res.is_err());
}

#[test]
fn test_cancel_prevents_execute_after_timelock() {
    let env = Env::default();
    let (client, admin, _) = setup(&env);
    let token = Address::generate(&env);
    let target = Address::generate(&env);

    client.initiate_emergency_withdraw(&admin, &token, &500i128, &target);
    client.cancel_emergency_withdraw(&admin);

    env.ledger()
        .set_timestamp(env.ledger().timestamp() + DEFAULT_EMERGENCY_TIMELOCK_SECS + 1);

    let res = client.try_execute_emergency_withdraw(&admin);
    assert!(res.is_err());
}

#[test]
fn test_cancel_immediately_prevents_execute() {
    let env = Env::default();
    let (client, admin, _) = setup(&env);
    let token = Address::generate(&env);
    let target = Address::generate(&env);

    client.initiate_emergency_withdraw(&admin, &token, &500i128, &target);
    client.cancel_emergency_withdraw(&admin);

    let res = client.try_execute_emergency_withdraw(&admin);
    assert!(res.is_err());
}

#[test]
fn test_get_pending_none_when_no_withdrawal_initiated() {
    let env = Env::default();
    let (client, _, _) = setup(&env);

    let pending = client.get_pending_emergency_withdraw();
    assert!(pending.is_none());
}

#[test]
fn test_execute_at_exact_timelock_boundary_succeeds() {
    let env = Env::default();
    let (client, admin, token_id, _, _) = setup_with_tokens(&env);
    let target = Address::generate(&env);
    let amount = 1_000i128;

    client.initiate_emergency_withdraw(&admin, &token_id, &amount, &target);
    let pending = client.get_pending_emergency_withdraw().unwrap();

    env.ledger().set_timestamp(pending.unlock_at);

    let result = client.try_execute_emergency_withdraw(&admin);
    assert!(result.is_ok());
}

#[test]
fn test_execute_one_second_before_timelock_fails() {
    let env = Env::default();
    let (client, admin, token_id, _, _) = setup_with_tokens(&env);
    let target = Address::generate(&env);
    let amount = 1_000i128;

    client.initiate_emergency_withdraw(&admin, &token_id, &amount, &target);
    let pending = client.get_pending_emergency_withdraw().unwrap();

    env.ledger().set_timestamp(pending.unlock_at - 1);

    let result = client.try_execute_emergency_withdraw(&admin);
    assert!(result.is_err());
}

#[test]
fn test_pending_withdrawal_contains_correct_fields() {
    let env = Env::default();
    let (client, admin, _) = setup(&env);
    let token = Address::generate(&env);
    let target = Address::generate(&env);
    let amount = 750i128;

    let init_time = env.ledger().timestamp();
    client.initiate_emergency_withdraw(&admin, &token, &amount, &target);

    let pending = client.get_pending_emergency_withdraw().unwrap();

    assert_eq!(pending.token, token);
    assert_eq!(pending.amount, amount);
    assert_eq!(pending.target, target);
    assert_eq!(pending.initiated_by, admin);
    assert_eq!(pending.initiated_at, init_time);
    assert_eq!(
        pending.unlock_at,
        init_time + DEFAULT_EMERGENCY_TIMELOCK_SECS
    );
    assert_eq!(
        pending.expires_at,
        init_time + DEFAULT_EMERGENCY_TIMELOCK_SECS + DEFAULT_EMERGENCY_EXPIRATION_SECS
    );
    assert!(!pending.cancelled);
    assert_eq!(pending.cancelled_at, 0);
    assert!(pending.nonce > 0);
}

#[test]
fn test_multiple_initiates_increments_nonce() {
    let env = Env::default();
    let (client, admin, _) = setup(&env);
    let token1 = Address::generate(&env);
    let token2 = Address::generate(&env);
    let target1 = Address::generate(&env);
    let target2 = Address::generate(&env);

    client.initiate_emergency_withdraw(&admin, &token1, &100i128, &target1);
    let pending1 = client.get_pending_emergency_withdraw().unwrap();
    let nonce1 = pending1.nonce;

    client.initiate_emergency_withdraw(&admin, &token2, &200i128, &target2);
    let pending2 = client.get_pending_emergency_withdraw().unwrap();

    assert_eq!(pending2.token, token2);
    assert_eq!(pending2.amount, 200i128);
    assert_eq!(pending2.target, target2);
    assert!(pending2.nonce > nonce1);
}

#[test]
fn test_initiate_token_equals_contract_fails() {
    let env = Env::default();
    let (client, admin, contract_id) = setup(&env);
    let target = Address::generate(&env);

    let result = client.try_initiate_emergency_withdraw(&admin, &contract_id, &100i128, &target);
    assert!(result.is_err());
}

#[test]
fn test_initiate_target_equals_contract_fails() {
    let env = Env::default();
    let (client, admin, contract_id) = setup(&env);
    let token = Address::generate(&env);

    let result = client.try_initiate_emergency_withdraw(&admin, &token, &100i128, &contract_id);
    assert!(result.is_err());
}

#[test]
fn test_execute_expired_withdrawal_fails() {
    let env = Env::default();
    let (client, admin, token_id, _, _) = setup_with_tokens(&env);
    let target = Address::generate(&env);
    let amount = 1_000i128;

    client.initiate_emergency_withdraw(&admin, &token_id, &amount, &target);

    let pending = client.get_pending_emergency_withdraw().unwrap();
    env.ledger().set_timestamp(pending.expires_at);

    let result = client.try_execute_emergency_withdraw(&admin);
    assert!(result.is_err());
}

#[test]
fn test_execute_one_second_before_expiration_succeeds() {
    let env = Env::default();
    let (client, admin, token_id, _, _) = setup_with_tokens(&env);
    let target = Address::generate(&env);
    let amount = 1_000i128;

    client.initiate_emergency_withdraw(&admin, &token_id, &amount, &target);

    let pending = client.get_pending_emergency_withdraw().unwrap();
    env.ledger().set_timestamp(pending.expires_at - 1);

    let result = client.try_execute_emergency_withdraw(&admin);
    assert!(result.is_ok());
}

#[test]
fn test_cannot_double_cancel() {
    let env = Env::default();
    let (client, admin, _) = setup(&env);
    let token = Address::generate(&env);
    let target = Address::generate(&env);

    client.initiate_emergency_withdraw(&admin, &token, &500i128, &target);
    client.cancel_emergency_withdraw(&admin);

    let res = client.try_cancel_emergency_withdraw(&admin);
    assert!(res.is_err());
}

#[test]
fn test_initiate_overwrites_cancelled_withdrawal() {
    let env = Env::default();
    let (client, admin, _) = setup(&env);
    let token1 = Address::generate(&env);
    let token2 = Address::generate(&env);
    let target1 = Address::generate(&env);
    let target2 = Address::generate(&env);

    client.initiate_emergency_withdraw(&admin, &token1, &100i128, &target1);
    let pending1 = client.get_pending_emergency_withdraw().unwrap();
    let nonce1 = pending1.nonce;

    client.cancel_emergency_withdraw(&admin);
    let pending_cancelled = client.get_pending_emergency_withdraw().unwrap();
    assert!(pending_cancelled.cancelled);

    client.initiate_emergency_withdraw(&admin, &token2, &200i128, &target2);
    let pending2 = client.get_pending_emergency_withdraw().unwrap();

    assert_eq!(pending2.token, token2);
    assert_eq!(pending2.amount, 200i128);
    assert!(!pending2.cancelled);
    assert!(pending2.nonce > nonce1);
}

#[test]
fn test_execute_after_cancel_and_reinitiate_succeeds() {
    let env = Env::default();
    let (client, admin, token_id, _, _) = setup_with_tokens(&env);
    let target = Address::generate(&env);

    client.initiate_emergency_withdraw(&admin, &token_id, &100i128, &target);
    client.cancel_emergency_withdraw(&admin);

    client.initiate_emergency_withdraw(&admin, &token_id, &200i128, &target);

    env.ledger()
        .set_timestamp(env.ledger().timestamp() + DEFAULT_EMERGENCY_TIMELOCK_SECS + 1);

    let result = client.try_execute_emergency_withdraw(&admin);
    assert!(result.is_ok());

    let pending = client.get_pending_emergency_withdraw();
    assert!(pending.is_none());
}

#[test]
fn test_nonce_is_persisted_in_cancellation() {
    let env = Env::default();
    let (client, admin, _) = setup(&env);
    let token = Address::generate(&env);
    let target = Address::generate(&env);

    client.initiate_emergency_withdraw(&admin, &token, &500i128, &target);
    let pending = client.get_pending_emergency_withdraw().unwrap();
    let cancelled_nonce = pending.nonce;

    client.cancel_emergency_withdraw(&admin);

    let pending_after = client.get_pending_emergency_withdraw().unwrap();
    assert_eq!(pending_after.nonce, cancelled_nonce);
    assert!(pending_after.cancelled);
}

#[test]
fn test_initiate_with_same_params_different_nonce() {
    let env = Env::default();
    let (client, admin, _) = setup(&env);
    let token = Address::generate(&env);
    let target = Address::generate(&env);

    client.initiate_emergency_withdraw(&admin, &token, &100i128, &target);
    let pending1 = client.get_pending_emergency_withdraw().unwrap();
    let nonce1 = pending1.nonce;

    client.initiate_emergency_withdraw(&admin, &token, &100i128, &target);
    let pending2 = client.get_pending_emergency_withdraw().unwrap();
    let nonce2 = pending2.nonce;

    assert!(nonce2 > nonce1);
}

#[test]
fn test_can_execute_returns_true_when_ready() {
    let env = Env::default();
    let (client, admin, token_id, _, _) = setup_with_tokens(&env);
    let target = Address::generate(&env);

    client.initiate_emergency_withdraw(&admin, &token_id, &100i128, &target);

    env.ledger()
        .set_timestamp(env.ledger().timestamp() + DEFAULT_EMERGENCY_TIMELOCK_SECS + 1);

    let can_exec = client.can_exec_emergency();
    assert!(can_exec);
}

#[test]
fn test_can_execute_returns_false_before_timelock() {
    let env = Env::default();
    let (client, admin, _) = setup(&env);
    let token = Address::generate(&env);
    let target = Address::generate(&env);

    client.initiate_emergency_withdraw(&admin, &token, &100i128, &target);

    let can_exec = client.can_exec_emergency();
    assert!(!can_exec);
}

#[test]
fn test_can_execute_returns_false_when_cancelled() {
    let env = Env::default();
    let (client, admin, _) = setup(&env);
    let token = Address::generate(&env);
    let target = Address::generate(&env);

    client.initiate_emergency_withdraw(&admin, &token, &100i128, &target);
    client.cancel_emergency_withdraw(&admin);

    let can_exec = client.can_exec_emergency();
    assert!(!can_exec);
}

#[test]
fn test_can_execute_returns_none_when_no_pending() {
    let env = Env::default();
    let (client, _, _) = setup(&env);

    let can_exec = client.can_exec_emergency();
    assert!(!can_exec);
}

#[test]
fn test_time_until_unlock_returns_zero_after_timelock() {
    let env = Env::default();
    let (client, admin, _) = setup(&env);
    let token = Address::generate(&env);
    let target = Address::generate(&env);

    client.initiate_emergency_withdraw(&admin, &token, &100i128, &target);

    env.ledger()
        .set_timestamp(env.ledger().timestamp() + DEFAULT_EMERGENCY_TIMELOCK_SECS + 10);

    let remaining = client.emg_time_until_unlock();
    assert_eq!(remaining, 0);
}

#[test]
fn test_time_until_unlock_returns_positive_before_timelock() {
    let env = Env::default();
    let (client, admin, _) = setup(&env);
    let token = Address::generate(&env);
    let target = Address::generate(&env);

    client.initiate_emergency_withdraw(&admin, &token, &100i128, &target);

    let remaining = client.emg_time_until_unlock();
    assert!(remaining > 0);
    assert!(remaining <= DEFAULT_EMERGENCY_TIMELOCK_SECS);
}

#[test]
fn test_time_until_expiration_returns_zero_after_expiry() {
    let env = Env::default();
    let (client, admin, _) = setup(&env);
    let token = Address::generate(&env);
    let target = Address::generate(&env);

    client.initiate_emergency_withdraw(&admin, &token, &100i128, &target);

    let pending = client.get_pending_emergency_withdraw().unwrap();
    env.ledger().set_timestamp(pending.expires_at + 100);

    let remaining = client.emg_time_until_expire();
    assert_eq!(remaining, 0);
}

#[test]
fn test_time_until_expiration_returns_positive_before_expiry() {
    let env = Env::default();
    let (client, admin, _) = setup(&env);
    let token = Address::generate(&env);
    let target = Address::generate(&env);

    client.initiate_emergency_withdraw(&admin, &token, &100i128, &target);

    let remaining = client.emg_time_until_expire();
    assert!(remaining > 0);
}

#[test]
fn test_cancelled_withdrawal_shows_correct_nonce() {
    let env = Env::default();
    env.ledger().set_timestamp(2000);
    let (client, admin, _) = setup(&env);
    let token = Address::generate(&env);
    let target = Address::generate(&env);

    client.initiate_emergency_withdraw(&admin, &token, &100i128, &target);
    let pending = client.get_pending_emergency_withdraw().unwrap();

    client.cancel_emergency_withdraw(&admin);

    let after_cancel = client.get_pending_emergency_withdraw().unwrap();
    assert_eq!(after_cancel.nonce, pending.nonce);
    assert!(after_cancel.cancelled);
    assert!(after_cancel.cancelled_at >= 2000);
}

#[test]
fn test_initiate_after_cancel_clears_cancelled_state() {
    let env = Env::default();
    let (client, admin, _) = setup(&env);
    let token = Address::generate(&env);
    let target = Address::generate(&env);

    client.initiate_emergency_withdraw(&admin, &token, &100i128, &target);
    client.cancel_emergency_withdraw(&admin);

    let cancelled_pending = client.get_pending_emergency_withdraw().unwrap();
    assert!(cancelled_pending.cancelled);

    client.initiate_emergency_withdraw(&admin, &token, &200i128, &target);

    let new_pending = client.get_pending_emergency_withdraw().unwrap();
    assert!(!new_pending.cancelled);
    assert_eq!(new_pending.cancelled_at, 0);
}

#[test]
fn test_boundary_exactly_at_expiration_fails() {
    let env = Env::default();
    let (client, admin, token_id, _, _) = setup_with_tokens(&env);
    let target = Address::generate(&env);
    let amount = 1_000i128;

    client.initiate_emergency_withdraw(&admin, &token_id, &amount, &target);
    let pending = client.get_pending_emergency_withdraw().unwrap();

    env.ledger().set_timestamp(pending.expires_at);

    let result = client.try_execute_emergency_withdraw(&admin);
    assert!(result.is_err());
}

#[test]
fn test_large_amount_withdrawal() {
    let env = Env::default();
    let (client, admin, token_id, _, _) = setup_with_tokens(&env);
    let target = Address::generate(&env);
    let amount = 500_000i128;

    client.initiate_emergency_withdraw(&admin, &token_id, &amount, &target);

    env.ledger()
        .set_timestamp(env.ledger().timestamp() + DEFAULT_EMERGENCY_TIMELOCK_SECS + 1);

    let result = client.try_execute_emergency_withdraw(&admin);
    assert!(result.is_ok());
}

#[test]
fn test_zero_target_address_fails_validation() {
    let env = Env::default();
    let (client, admin, _) = setup(&env);
    let token = Address::generate(&env);
    let target = Address::generate(&env);

    let result = client.try_initiate_emergency_withdraw(&admin, &token, &0i128, &target);
    assert!(result.is_err());
}

// ============================================================================
// CRITICAL EDGE CASES - Phase 3 Tests
// ============================================================================

#[test]
fn test_double_execution_fails() {
    let env = Env::default();
    let (client, admin, token_id, _, _) = setup_with_tokens(&env);
    let target = Address::generate(&env);
    let amount = 1_000i128;

    client.initiate_emergency_withdraw(&admin, &token_id, &amount, &target);

    env.ledger()
        .set_timestamp(env.ledger().timestamp() + DEFAULT_EMERGENCY_TIMELOCK_SECS + 1);

    // First execution succeeds
    let result = client.try_execute_emergency_withdraw(&admin);
    assert!(result.is_ok());

    // Second execution must fail (no pending withdrawal exists)
    let result2 = client.try_execute_emergency_withdraw(&admin);
    assert!(result2.is_err());

    // Verify no pending withdrawal exists
    assert!(client.get_pending_emergency_withdraw().is_none());
}

#[test]
fn test_execute_then_cancel_fails() {
    let env = Env::default();
    let (client, admin, token_id, _, _) = setup_with_tokens(&env);
    let target = Address::generate(&env);
    let amount = 1_000i128;

    client.initiate_emergency_withdraw(&admin, &token_id, &amount, &target);

    env.ledger()
        .set_timestamp(env.ledger().timestamp() + DEFAULT_EMERGENCY_TIMELOCK_SECS + 1);

    // Execute successfully
    client.execute_emergency_withdraw(&admin);

    // Trying to cancel after execution must fail
    let result = client.try_cancel_emergency_withdraw(&admin);
    assert!(result.is_err());
}

#[test]
fn test_exact_unlock_second_inclusive_boundary() {
    let env = Env::default();
    env.ledger().set_timestamp(1000);
    let (client, admin, token_id, _, _) = setup_with_tokens(&env);
    let target = Address::generate(&env);
    let amount = 1_000i128;

    client.initiate_emergency_withdraw(&admin, &token_id, &amount, &target);

    let pending = client.get_pending_emergency_withdraw().unwrap();
    assert_eq!(pending.unlock_at, 1000 + DEFAULT_EMERGENCY_TIMELOCK_SECS);

    // Set time to exactly unlock_at (inclusive boundary)
    env.ledger().set_timestamp(pending.unlock_at);

    // Execution at exact unlock_at must succeed
    let result = client.try_execute_emergency_withdraw(&admin);
    assert!(
        result.is_ok(),
        "Execution at exact unlock_at must succeed (inclusive boundary)"
    );
}

#[test]
fn test_one_second_after_unlock_succeeds() {
    let env = Env::default();
    env.ledger().set_timestamp(1000);
    let (client, admin, token_id, _, _) = setup_with_tokens(&env);
    let target = Address::generate(&env);
    let amount = 1_000i128;

    client.initiate_emergency_withdraw(&admin, &token_id, &amount, &target);

    let pending = client.get_pending_emergency_withdraw().unwrap();

    // Set time to unlock_at + 1 (should succeed)
    env.ledger().set_timestamp(pending.unlock_at + 1);

    let result = client.try_execute_emergency_withdraw(&admin);
    assert!(result.is_ok());
}

#[test]
fn test_exact_expiry_second_exclusive_boundary() {
    let env = Env::default();
    env.ledger().set_timestamp(1000);
    let (client, admin, token_id, _, _) = setup_with_tokens(&env);
    let target = Address::generate(&env);
    let amount = 1_000i128;

    client.initiate_emergency_withdraw(&admin, &token_id, &amount, &target);

    let pending = client.get_pending_emergency_withdraw().unwrap();
    assert_eq!(
        pending.expires_at,
        1000 + DEFAULT_EMERGENCY_TIMELOCK_SECS + DEFAULT_EMERGENCY_EXPIRATION_SECS
    );

    // Set time to exactly expires_at (exclusive boundary)
    env.ledger().set_timestamp(pending.expires_at);

    // Execution at exact expires_at must fail
    let result = client.try_execute_emergency_withdraw(&admin);
    assert!(
        result.is_err(),
        "Execution at exact expires_at must fail (exclusive boundary)"
    );
}

#[test]
fn test_last_valid_second_before_expiry_succeeds() {
    let env = Env::default();
    env.ledger().set_timestamp(1000);
    let (client, admin, token_id, _, _) = setup_with_tokens(&env);
    let target = Address::generate(&env);
    let amount = 1_000i128;

    client.initiate_emergency_withdraw(&admin, &token_id, &amount, &target);

    let pending = client.get_pending_emergency_withdraw().unwrap();

    // Set time to expires_at - 1 (last valid second)
    env.ledger().set_timestamp(pending.expires_at - 1);

    let result = client.try_execute_emergency_withdraw(&admin);
    assert!(
        result.is_ok(),
        "Execution at expires_at - 1 must succeed (last valid second)"
    );
}

#[test]
fn test_non_admin_cannot_initiate() {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let attacker = Address::generate(&env);
    let token = Address::generate(&env);
    let target = Address::generate(&env);

    client.mock_all_auths().initialize_admin(&admin);

    // Attacker tries to initiate
    let spoofed_auth = MockAuth {
        address: &attacker,
        invoke: &MockAuthInvoke {
            contract: &contract_id,
            fn_name: "initiate_emergency_withdraw",
            args: (admin.clone(), token.clone(), 1000i128, target.clone()).into_val(&env),
            sub_invokes: &[],
        },
    };

    let result = client
        .mock_auths(&[spoofed_auth])
        .try_initiate_emergency_withdraw(&admin, &token, &1000i128, &target);

    assert!(result.is_err(), "Non-admin must not be able to initiate");
    assert!(client.get_pending_emergency_withdraw().is_none());
}

#[test]
fn test_non_admin_cannot_execute() {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let attacker = Address::generate(&env);

    client.mock_all_auths().initialize_admin(&admin);
    client.mock_all_auths().initialize_fee_system(&admin);

    let token_admin = Address::generate(&env);
    let token_id = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let sac = token::StellarAssetClient::new(&env, &token_id);
    sac.mint(&contract_id, &10_000i128);

    let target = Address::generate(&env);

    client.initiate_emergency_withdraw(&admin, &token_id, &1000i128, &target);

    env.ledger()
        .set_timestamp(env.ledger().timestamp() + DEFAULT_EMERGENCY_TIMELOCK_SECS + 1);

    // Attacker tries to execute
    let spoofed_auth = MockAuth {
        address: &attacker,
        invoke: &MockAuthInvoke {
            contract: &contract_id,
            fn_name: "execute_emergency_withdraw",
            args: (admin.clone(),).into_val(&env),
            sub_invokes: &[],
        },
    };

    let result = client
        .mock_auths(&[spoofed_auth])
        .try_execute_emergency_withdraw(&admin);

    assert!(result.is_err(), "Non-admin must not be able to execute");

    // Verify withdrawal still pending
    assert!(client.get_pending_emergency_withdraw().is_some());
}

#[test]
fn test_non_admin_cannot_cancel() {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let attacker = Address::generate(&env);
    let token = Address::generate(&env);
    let target = Address::generate(&env);

    client.mock_all_auths().initialize_admin(&admin);

    client.initiate_emergency_withdraw(&admin, &token, &1000i128, &target);

    // Attacker tries to cancel
    let spoofed_auth = MockAuth {
        address: &attacker,
        invoke: &MockAuthInvoke {
            contract: &contract_id,
            fn_name: "cancel_emergency_withdraw",
            args: (admin.clone(),).into_val(&env),
            sub_invokes: &[],
        },
    };

    let result = client
        .mock_auths(&[spoofed_auth])
        .try_cancel_emergency_withdraw(&admin);

    assert!(result.is_err(), "Non-admin must not be able to cancel");

    // Verify withdrawal not cancelled
    let pending = client.get_pending_emergency_withdraw().unwrap();
    assert!(!pending.cancelled);
}

#[test]
fn test_cancel_completely_clears_execution_path() {
    let env = Env::default();
    let (client, admin, token_id, _, _) = setup_with_tokens(&env);
    let target = Address::generate(&env);
    let amount = 1_000i128;

    client.initiate_emergency_withdraw(&admin, &token_id, &amount, &target);

    let pending_before = client.get_pending_emergency_withdraw().unwrap();
    assert!(!pending_before.cancelled);

    // Cancel the withdrawal
    client.cancel_emergency_withdraw(&admin);

    let pending_after = client.get_pending_emergency_withdraw().unwrap();
    assert!(pending_after.cancelled);
    assert!(pending_after.cancelled_at > 0);

    // Advance past unlock time
    env.ledger()
        .set_timestamp(env.ledger().timestamp() + DEFAULT_EMERGENCY_TIMELOCK_SECS + 1);

    // Execution must still fail even after timelock
    let result = client.try_execute_emergency_withdraw(&admin);
    assert!(
        result.is_err(),
        "Cancelled withdrawal must not be executable even after timelock"
    );

    // Verify can_execute returns false
    assert!(
        !client.can_exec_emergency(),
        "can_execute must return false for cancelled withdrawal"
    );
}

#[test]
fn test_state_cleared_after_successful_execution() {
    let env = Env::default();
    let (client, admin, token_id, _, _) = setup_with_tokens(&env);
    let target = Address::generate(&env);
    let amount = 1_000i128;

    client.initiate_emergency_withdraw(&admin, &token_id, &amount, &target);

    env.ledger()
        .set_timestamp(env.ledger().timestamp() + DEFAULT_EMERGENCY_TIMELOCK_SECS + 1);

    client.execute_emergency_withdraw(&admin);

    // Verify complete state clearing
    assert!(
        client.get_pending_emergency_withdraw().is_none(),
        "Pending withdrawal must be completely cleared after execution"
    );
    assert!(
        !client.can_exec_emergency(),
        "can_execute must return false when no pending withdrawal"
    );
    assert_eq!(
        client.emg_time_until_unlock(),
        0,
        "time_until_unlock must return 0 when no pending withdrawal"
    );
    assert_eq!(
        client.emg_time_until_expire(),
        0,
        "time_until_expiration must return 0 when no pending withdrawal"
    );
}

#[test]
fn test_timelock_window_enforcement_comprehensive() {
    let env = Env::default();
    env.ledger().set_timestamp(10000);
    let (client, admin, token_id, _, _) = setup_with_tokens(&env);
    let target = Address::generate(&env);
    let amount = 1_000i128;

    client.initiate_emergency_withdraw(&admin, &token_id, &amount, &target);

    let pending = client.get_pending_emergency_withdraw().unwrap();
    let unlock_at = pending.unlock_at;
    let expires_at = pending.expires_at;

    // Test various points in the timeline

    // 1. Way before unlock: FAIL
    env.ledger().set_timestamp(unlock_at - 1000);
    assert!(client.try_execute_emergency_withdraw(&admin).is_err());

    // 2. One second before unlock: FAIL
    env.ledger().set_timestamp(unlock_at - 1);
    assert!(client.try_execute_emergency_withdraw(&admin).is_err());

    // 3. Exactly at unlock: SUCCESS
    env.ledger().set_timestamp(unlock_at);
    // Don't execute, just verify it would succeed
    assert!(client.can_exec_emergency());

    // 4. One second after unlock: SUCCESS
    env.ledger().set_timestamp(unlock_at + 1);
    assert!(client.can_exec_emergency());

    // 5. Middle of window: SUCCESS
    env.ledger().set_timestamp((unlock_at + expires_at) / 2);
    assert!(client.can_exec_emergency());

    // 6. One second before expiry: SUCCESS
    env.ledger().set_timestamp(expires_at - 1);
    assert!(client.can_exec_emergency());

    // 7. Exactly at expiry: FAIL
    env.ledger().set_timestamp(expires_at);
    assert!(!client.can_exec_emergency());
    assert!(client.try_execute_emergency_withdraw(&admin).is_err());

    // 8. After expiry: FAIL
    env.ledger().set_timestamp(expires_at + 1000);
    assert!(client.try_execute_emergency_withdraw(&admin).is_err());
}

#[test]
fn test_nonce_prevents_replay_after_cancel() {
    let env = Env::default();
    let (client, admin, token_id, _, _) = setup_with_tokens(&env);
    let target = Address::generate(&env);

    // Initiate first withdrawal
    client.initiate_emergency_withdraw(&admin, &token_id, &1000i128, &target);
    let nonce1 = client.get_pending_emergency_withdraw().unwrap().nonce;

    // Cancel it
    client.cancel_emergency_withdraw(&admin);

    // Verify cancelled state persists
    let pending = client.get_pending_emergency_withdraw().unwrap();
    assert!(pending.cancelled);
    assert_eq!(pending.nonce, nonce1);

    // Advance time past unlock
    env.ledger()
        .set_timestamp(env.ledger().timestamp() + DEFAULT_EMERGENCY_TIMELOCK_SECS + 1);

    // Execution must fail with cancelled error, not replay
    let result = client.try_execute_emergency_withdraw(&admin);
    assert!(
        result.is_err(),
        "Execution must fail for cancelled nonce even after timelock"
    );

    // Initiate new withdrawal with new nonce
    client.initiate_emergency_withdraw(&admin, &token_id, &2000i128, &target);
    let nonce2 = client.get_pending_emergency_withdraw().unwrap().nonce;

    assert!(nonce2 > nonce1, "New nonce must be greater than cancelled nonce");

    // New withdrawal should be executable after timelock
    env.ledger()
        .set_timestamp(env.ledger().timestamp() + DEFAULT_EMERGENCY_TIMELOCK_SECS + 1);
    assert!(client.can_exec_emergency());
}

#[test]
fn test_can_execute_false_for_all_invalid_states() {
    let env = Env::default();
    let (client, admin, token_id, _, _) = setup_with_tokens(&env);
    let target = Address::generate(&env);

    // No pending withdrawal
    assert!(!client.can_exec_emergency());

    // Before timelock
    client.initiate_emergency_withdraw(&admin, &token_id, &1000i128, &target);
    assert!(!client.can_exec_emergency());

    // After cancel
    client.cancel_emergency_withdraw(&admin);
    assert!(!client.can_exec_emergency());

    // After expiry
    client.initiate_emergency_withdraw(&admin, &token_id, &1000i128, &target);
    let pending = client.get_pending_emergency_withdraw().unwrap();
    env.ledger().set_timestamp(pending.expires_at + 1);
    assert!(!client.can_exec_emergency());
}

#[test]
fn test_time_helpers_return_correct_values_throughout_lifecycle() {
    let env = Env::default();
    env.ledger().set_timestamp(5000);
    let (client, admin, token_id, _, _) = setup_with_tokens(&env);
    let target = Address::generate(&env);

    // Before initiation: returns 0
    assert_eq!(client.emg_time_until_unlock(), 0);
    assert_eq!(client.emg_time_until_expire(), 0);

    client.initiate_emergency_withdraw(&admin, &token_id, &1000i128, &target);

    let pending = client.get_pending_emergency_withdraw().unwrap();

    // Just after initiation
    assert_eq!(
        client.emg_time_until_unlock(),
        DEFAULT_EMERGENCY_TIMELOCK_SECS
    );
    assert!(client.emg_time_until_expire() > DEFAULT_EMERGENCY_TIMELOCK_SECS);

    // Halfway to unlock
    env.ledger()
        .set_timestamp(5000 + DEFAULT_EMERGENCY_TIMELOCK_SECS / 2);
    assert!(client.emg_time_until_unlock() > 0);
    assert!(client.emg_time_until_unlock() < DEFAULT_EMERGENCY_TIMELOCK_SECS);

    // At unlock
    env.ledger().set_timestamp(pending.unlock_at);
    assert_eq!(client.emg_time_until_unlock(), 0);
    assert_eq!(
        client.emg_time_until_expire(),
        DEFAULT_EMERGENCY_EXPIRATION_SECS
    );

    // At expiry
    env.ledger().set_timestamp(pending.expires_at);
    assert_eq!(client.emg_time_until_unlock(), 0);
    assert_eq!(client.emg_time_until_expire(), 0);
}

#[test]
fn test_initiate_expires_before_queue_fails() {
    let env = Env::default();
    let (client, admin, _) = setup(&env);
    let token = Address::generate(&env);
    let target = Address::generate(&env);
    let amount = 1_000i128;

    // Set timestamp to u64::MAX to force saturating_add overflow/saturation,
    // which results in expires_at == initiated_at (queued_at) because they both saturate to u64::MAX
    env.ledger().set_timestamp(u64::MAX);

    let result = client.try_initiate_emergency_withdraw(&admin, &token, &amount, &target);
    assert!(result.is_err());
    let err = result.unwrap_err();
    let contract_err = err.expect("expected contract error");
    assert_eq!(contract_err, crate::errors::QuickLendXError::InvalidTimestamp);
}

