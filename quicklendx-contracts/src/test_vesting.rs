#![cfg(test)]
//! Tests for vesting schedules: timelock, release, and authorization.

use crate::{QuickLendXContract, QuickLendXContractClient};
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::{token, Address, Env};

const ADMIN_BALANCE: i128 = 10_000;

fn setup() -> (
    Env,
    QuickLendXContractClient<'static>,
    Address,
    Address,
    Address,
    token::Client<'static>,
) {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);

    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let beneficiary = Address::generate(&env);
    client.initialize_admin(&admin);

    let token_admin = Address::generate(&env);
    let token_id = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let sac = token::StellarAssetClient::new(&env, &token_id);
    let token_client = token::Client::new(&env, &token_id);

    sac.mint(&admin, &ADMIN_BALANCE);
    let exp = env.ledger().sequence() + 10_000;
    token_client.approve(&admin, &contract_id, &ADMIN_BALANCE, &exp);

    (env, client, admin, beneficiary, token_id, token_client)
}

#[test]
fn test_create_schedule_transfers_funds() {
    let (_env, client, admin, beneficiary, token_id, token_client) = setup();

    let total = 1_000i128;
    let id = client.create_vesting_schedule(
        &admin,
        &token_id,
        &beneficiary,
        &total,
        &1_000u64,
        &100u64,
        &2_000u64,
    );

    let schedule = client.get_vesting_schedule(&id).unwrap();
    assert_eq!(schedule.total_amount, total);
    assert_eq!(schedule.released_amount, 0);
    assert_eq!(schedule.beneficiary, beneficiary);
    assert_eq!(schedule.token, token_id);

    let contract_id = client.address;
    assert_eq!(token_client.balance(&contract_id), total);
    assert_eq!(token_client.balance(&admin), ADMIN_BALANCE - total);
}

#[test]
fn test_zero_amount_fails() {
    let (_env, client, admin, beneficiary, token_id, _token_client) = setup();

    let result = client.try_create_vesting_schedule(
        &admin,
        &token_id,
        &beneficiary,
        &0i128,
        &1_000u64,
        &0u64,
        &2_000u64,
    );

    assert!(result.is_err());
}

#[test]
fn test_invalid_timestamps_fail() {
    let (env, client, admin, beneficiary, token_id, _token_client) = setup();

    let res_end_before_start = client.try_create_vesting_schedule(
        &admin,
        &token_id,
        &beneficiary,
        &100i128,
        &2_000u64,
        &0u64,
        &1_000u64,
    );
    assert!(res_end_before_start.is_err());

    let res_start_in_past = client.try_create_vesting_schedule(
        &admin,
        &token_id,
        &beneficiary,
        &100i128,
        &999u64,
        &0u64,
        &1_500u64,
    );
    assert!(res_start_in_past.is_err());

    let res_start_equals_end = client.try_create_vesting_schedule(
        &admin,
        &token_id,
        &beneficiary,
        &100i128,
        &1_500u64,
        &0u64,
        &1_500u64,
    );
    assert!(res_start_equals_end.is_err());

    let res_cliff_after_end = client.try_create_vesting_schedule(
        &admin,
        &token_id,
        &beneficiary,
        &100i128,
        &1_000u64,
        &2_000u64,
        &1_500u64,
    );
    assert!(res_cliff_after_end.is_err());

    let res_cliff_at_end = client.try_create_vesting_schedule(
        &admin,
        &token_id,
        &beneficiary,
        &100i128,
        &1_000u64,
        &500u64,
        &1_500u64,
    );
    assert!(res_cliff_at_end.is_err());

    let res_cliff_overflow = client.try_create_vesting_schedule(
        &admin,
        &token_id,
        &beneficiary,
        &100i128,
        &env.ledger().timestamp(),
        &u64::MAX,
        &(u64::MAX - 1),
    );
    assert!(res_cliff_overflow.is_err());
}

#[test]
fn test_release_before_cliff_fails() {
    let (_env, client, admin, beneficiary, token_id, _token_client) = setup();

    let id = client.create_vesting_schedule(
        &admin,
        &token_id,
        &beneficiary,
        &1_000i128,
        &1_000u64,
        &500u64,
        &3_000u64,
    );

    let result = client.try_release_vested_tokens(&beneficiary, &id);
    assert!(result.is_err());
}

#[test]
fn test_release_partial_after_cliff() {
    let (env, client, admin, beneficiary, token_id, token_client) = setup();

    let total = 1_000i128;
    let id = client.create_vesting_schedule(
        &admin,
        &token_id,
        &beneficiary,
        &total,
        &1_000u64,
        &100u64,
        &2_000u64,
    );

    env.ledger().set_timestamp(1_500);

    let released = client.release_vested_tokens(&beneficiary, &id);
    assert_eq!(released, 500);

    let schedule = client.get_vesting_schedule(&id).unwrap();
    assert_eq!(schedule.released_amount, 500);
    assert_eq!(token_client.balance(&beneficiary), 500);

    // Cannot release again without advancing time
    let result = client.try_release_vested_tokens(&beneficiary, &id);
    assert!(result.is_err());
}

#[test]
fn test_release_after_end_releases_remaining() {
    let (env, client, admin, beneficiary, token_id, token_client) = setup();

    let total = 1_000i128;
    let id = client.create_vesting_schedule(
        &admin,
        &token_id,
        &beneficiary,
        &total,
        &1_000u64,
        &100u64,
        &2_000u64,
    );

    env.ledger().set_timestamp(1_500);
    let released_partial = client.release_vested_tokens(&beneficiary, &id);
    assert_eq!(released_partial, 500);

    env.ledger().set_timestamp(2_100);
    let released_final = client.release_vested_tokens(&beneficiary, &id);
    assert_eq!(released_final, 500);

    let schedule = client.get_vesting_schedule(&id).unwrap();
    assert_eq!(schedule.released_amount, total);
    assert_eq!(token_client.balance(&beneficiary), total);
}

#[test]
fn test_start_time_equal_to_now_is_allowed() {
    let (env, client, admin, beneficiary, token_id, token_client) = setup();

    let total = 2_000i128;
    let id = client.create_vesting_schedule(
        &admin,
        &token_id,
        &beneficiary,
        &total,
        &2_000u64,
        &0u64,
        &3_000u64,
    );

    let releasable_at_start = client.get_vesting_releasable(&id).unwrap();
    assert_eq!(releasable_at_start, 0);

    env.ledger().set_timestamp(2_500);
    let released = client.release_vested_tokens(&beneficiary, &id);
    assert_eq!(released, 1_000);
    assert_eq!(token_client.balance(&beneficiary), 1_000);

    let releasable = client.get_vesting_releasable(&id).unwrap();
    assert_eq!(releasable, 0);

    env.ledger().set_timestamp(3_000);
    let released_final = client.release_vested_tokens(&beneficiary, &id);
    assert_eq!(released_final, 1_000);
}

#[test]
fn test_release_at_exact_cliff_uses_elapsed_vesting() {
    let (env, client, admin, beneficiary, token_id, token_client) = setup();

    let id = client.create_vesting_schedule(
        &admin,
        &token_id,
        &beneficiary,
        &1_000i128,
        &1_000u64,
        &500u64,
        &2_000u64,
    );

    env.ledger().set_timestamp(1_500);

    let released = client.release_vested_tokens(&beneficiary, &id);
    assert_eq!(released, 500);
    assert_eq!(token_client.balance(&beneficiary), 500);
}

#[test]
fn test_only_beneficiary_can_release() {
    let (env, client, admin, beneficiary, token_id, _token_client) = setup();
    let intruder = Address::generate(&env);

    let id = client.create_vesting_schedule(
        &admin,
        &token_id,
        &beneficiary,
        &1_000i128,
        &1_000u64,
        &0u64,
        &2_000u64,
    );

    let result = client.try_release_vested_tokens(&intruder, &id);
    assert!(result.is_err());
}

#[test]
fn test_only_admin_can_create_schedule() {
    let (env, client, _admin, beneficiary, token_id, _token_client) = setup();
    let attacker = Address::generate(&env);

    let result = client.try_create_vesting_schedule(
        &attacker,
        &token_id,
        &beneficiary,
        &1_000i128,
        &1_500u64,
        &0u64,
        &2_000u64,
    );

    assert!(result.is_err());
}
