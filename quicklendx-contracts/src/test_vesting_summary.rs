#![cfg(test)]
//! Tests for `get_vesting_summary(user)`.
//!
//! Covers:
//! - Empty user (no schedules) → zeroed summary
//! - Single-grant user → correct aggregation
//! - Multi-grant user → correct aggregation across schedules
//! - Other-user grants are excluded from the summary

use crate::{QuickLendXContract, QuickLendXContractClient};
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::{token, Address, Env};

const ADMIN_BALANCE: i128 = 100_000_000;

fn setup() -> (
    Env,
    QuickLendXContractClient<'static>,
    Address, // admin
    Address, // token_id
) {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);

    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
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

    (env, client, admin, token_id)
}

// ── Empty user ────────────────────────────────────────────────────────────────

#[test]
fn returns_zeroed_summary_for_user_with_no_grants() {
    let (env, client, _, _) = setup();
    let stranger = Address::generate(&env);

    let summary = client.get_vesting_summary(&stranger);

    assert_eq!(summary.grant_count, 0);
    assert_eq!(summary.total_granted, 0);
    assert_eq!(summary.total_released, 0);
    assert_eq!(summary.total_releasable, 0);
}

// ── Single-grant user ─────────────────────────────────────────────────────────

#[test]
fn returns_correct_summary_for_single_grant_before_cliff() {
    let (env, client, admin, token_id) = setup();
    let user = Address::generate(&env);

    let total = 5_000i128;
    let start = 1_000u64;
    let cliff_secs = 500u64;
    let end = 3_000u64;

    client.create_vesting_schedule(&admin, &token_id, &user, &total, &start, &cliff_secs, &end);

    // Still before cliff — releasable = 0
    let summary = client.get_vesting_summary(&user);
    assert_eq!(summary.grant_count, 1);
    assert_eq!(summary.total_granted, total);
    assert_eq!(summary.total_released, 0);
    assert_eq!(summary.total_releasable, 0);
}

#[test]
fn returns_correct_summary_for_single_grant_after_cliff() {
    let (env, client, admin, token_id) = setup();
    let user = Address::generate(&env);

    let total = 5_000i128;
    let start = 1_000u64;
    let cliff_secs = 500u64; // cliff at 1500
    let end = 3_000u64;

    client.create_vesting_schedule(&admin, &token_id, &user, &total, &start, &cliff_secs, &end);

    // Advance past cliff to midpoint: elapsed = 1000, duration = 2000 → vested = 2500
    env.ledger().set_timestamp(2_000);
    let summary = client.get_vesting_summary(&user);
    assert_eq!(summary.grant_count, 1);
    assert_eq!(summary.total_granted, total);
    assert_eq!(summary.total_released, 0);
    assert_eq!(summary.total_releasable, 2_500);
}

#[test]
fn returns_fully_releasable_for_single_grant_at_end() {
    let (env, client, admin, token_id) = setup();
    let user = Address::generate(&env);

    let total = 5_000i128;
    let start = 1_000u64;
    let end = 3_000u64;

    client.create_vesting_schedule(&admin, &token_id, &user, &total, &start, &0, &end);

    env.ledger().set_timestamp(end);
    let summary = client.get_vesting_summary(&user);
    assert_eq!(summary.grant_count, 1);
    assert_eq!(summary.total_granted, total);
    assert_eq!(summary.total_releasable, total);
}

#[test]
fn reflects_released_amount_after_partial_claim() {
    let (env, client, admin, token_id) = setup();
    let user = Address::generate(&env);

    let total = 4_000i128;
    let start = 1_000u64;
    let end = 3_000u64; // no cliff

    let id = client.create_vesting_schedule(&admin, &token_id, &user, &total, &start, &0, &end);

    // Midpoint: elapsed = 1000, duration = 2000 → vested = 2000
    env.ledger().set_timestamp(2_000);
    client.release_vesting(&user, &id);

    let summary = client.get_vesting_summary(&user);
    assert_eq!(summary.grant_count, 1);
    assert_eq!(summary.total_granted, total);
    assert_eq!(summary.total_released, 2_000);
    assert_eq!(summary.total_releasable, 0); // already claimed
}

// ── Multi-grant user ──────────────────────────────────────────────────────────

#[test]
fn aggregates_across_multiple_grants() {
    let (env, client, admin, token_id) = setup();
    let user = Address::generate(&env);

    // Grant A: 3000 total, no cliff, ends at 2000
    client.create_vesting_schedule(&admin, &token_id, &user, &3_000, &1_000, &0, &3_000);
    // Grant B: 7000 total, no cliff, ends at 2000
    client.create_vesting_schedule(&admin, &token_id, &user, &7_000, &1_000, &0, &3_000);

    // At midpoint: each is 50 % vested → releasable = 1500 + 3500 = 5000
    env.ledger().set_timestamp(2_000);
    let summary = client.get_vesting_summary(&user);
    assert_eq!(summary.grant_count, 2);
    assert_eq!(summary.total_granted, 10_000);
    assert_eq!(summary.total_released, 0);
    assert_eq!(summary.total_releasable, 5_000);
}

#[test]
fn excludes_other_users_grants_from_summary() {
    let (env, client, admin, token_id) = setup();
    let user = Address::generate(&env);
    let other = Address::generate(&env);

    client.create_vesting_schedule(&admin, &token_id, &user, &1_000, &1_000, &0, &3_000);
    client.create_vesting_schedule(&admin, &token_id, &other, &9_000, &1_000, &0, &3_000);

    let summary = client.get_vesting_summary(&user);
    assert_eq!(summary.grant_count, 1);
    assert_eq!(summary.total_granted, 1_000);
}
