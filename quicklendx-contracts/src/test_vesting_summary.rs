#![cfg(test)]
//! Tests for `get_vesting_summary(user)`.
//!
//! Covers: empty user, single-grant user, multi-grant user, and a user whose
//! address appears nowhere in the vesting ledger (sad path / isolation).

use crate::{QuickLendXContract, QuickLendXContractClient};
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::{token, Address, Env};

const ADMIN_BALANCE: i128 = 10_000_000;

fn setup() -> (
    Env,
    QuickLendXContractClient<'static>,
    Address, // admin
    Address, // beneficiary
    Address, // token_id
    token::Client<'static>,
) {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1000);

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
fn summary_returns_zeros_for_user_with_no_grants() {
    let (env, client, _admin, _beneficiary, _token_id, _token_client) = setup();
    let stranger = Address::generate(&env);
    let summary = client.get_vesting_summary(&stranger);
    assert_eq!(summary.grant_count, 0);
    assert_eq!(summary.total_granted, 0);
    assert_eq!(summary.total_released, 0);
    assert_eq!(summary.total_releasable, 0);
}

#[test]
fn summary_reflects_single_grant_before_cliff() {
    let (env, client, admin, beneficiary, token_id, _token_client) = setup();

    let total = 1_000i128;
    let start = 1000u64;
    let cliff_seconds = 500u64;
    let end = start + 2_000;

    client.create_vesting_schedule(
        &admin,
        &token_id,
        &beneficiary,
        &total,
        &start,
        &cliff_seconds,
        &end,
    );

    // Still before cliff — releasable must be 0.
    env.ledger().set_timestamp(start + cliff_seconds - 1);
    let summary = client.get_vesting_summary(&beneficiary);
    assert_eq!(summary.grant_count, 1);
    assert_eq!(summary.total_granted, total);
    assert_eq!(summary.total_released, 0);
    assert_eq!(summary.total_releasable, 0);
}

#[test]
fn summary_reflects_multiple_grants_for_same_user() {
    let (env, client, admin, beneficiary, token_id, _token_client) = setup();

    let start = 1000u64;
    let cliff_seconds = 0u64;
    let end = start + 2_000;

    client.create_vesting_schedule(
        &admin,
        &token_id,
        &beneficiary,
        &600i128,
        &start,
        &cliff_seconds,
        &end,
    );
    client.create_vesting_schedule(
        &admin,
        &token_id,
        &beneficiary,
        &400i128,
        &start,
        &cliff_seconds,
        &end,
    );

    // Past end — everything fully vested, nothing released yet.
    env.ledger().set_timestamp(end + 1);
    let summary = client.get_vesting_summary(&beneficiary);
    assert_eq!(summary.grant_count, 2);
    assert_eq!(summary.total_granted, 1_000);
    assert_eq!(summary.total_released, 0);
    assert_eq!(summary.total_releasable, 1_000);
}

#[test]
fn summary_excludes_grants_belonging_to_other_users() {
    let (env, client, admin, beneficiary, _token_id, _token_client) = setup();
    let other = Address::generate(&env);

    let start = 1000u64;
    let end = start + 2_000;

    // Mint and approve a second token so the other user's grant goes through.
    let token_admin2 = Address::generate(&env);
    let other_token = env
        .register_stellar_asset_contract_v2(token_admin2.clone())
        .address();
    let sac2 = token::StellarAssetClient::new(&env, &other_token);
    let other_tc = token::Client::new(&env, &other_token);
    sac2.mint(&admin, &500i128);
    let exp = env.ledger().sequence() + 10_000;
    other_tc.approve(&admin, &client.address, &500i128, &exp);

    client.create_vesting_schedule(
        &admin,
        &other_token,
        &other,
        &500i128,
        &start,
        &0u64,
        &end,
    );

    // `beneficiary` should see an empty summary.
    let summary = client.get_vesting_summary(&beneficiary);
    assert_eq!(summary.grant_count, 0);
    assert_eq!(summary.total_granted, 0);
}
