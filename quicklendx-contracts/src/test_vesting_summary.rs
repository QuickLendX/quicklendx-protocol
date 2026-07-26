#![cfg(test)]
//! Tests for `get_vesting_summary(user)` — boundary coverage for the
//! `for id in 1..=max_id` scan inside `Vesting::get_summary_for_user`.
//!
//! Specifically locks in:
//! - **Empty range**: counter is 0 → loop is `1..=0` (never executes) → zeroed summary
//! - **Single ledger**: exactly one schedule in storage → loop runs once → grant_count = 1
//! - **Wide range**: many schedules for mixed owners → only the caller's grants are counted
//!
//! Additional cases carried forward from the original file:
//! - Single-grant user before cliff (releasable = 0)
//! - Single-grant user after cliff (partial releasable)
//! - Single-grant user at end (fully releasable)
//! - Partial claim reflected in total_released
//! - Multi-grant aggregation
//! - Isolation: other users' grants are excluded

use crate::{QuickLendXContract, QuickLendXContractClient};
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::testutils::Address as _;
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

// ── Issue boundary: Empty range ───────────────────────────────────────────────
//
// When no vesting schedule has ever been created, the internal counter
// (`vest_cnt`) is absent / 0, so `get_summary_for_user` runs the range
// `1..=0` which is empty in Rust.  The result must be a fully-zeroed summary.

/// Empty range: counter absent (no schedules ever created) → zeroed summary.
///
/// This locks in the `1..=0` branch of `Vesting::get_summary_for_user` so
/// that a future refactor cannot accidentally panic or return stale data when
/// `max_id == 0`.
#[test]
fn returns_zeroed_summary_when_no_schedules_have_been_created() {
    let (env, client, _, _) = setup();
    let user = Address::generate(&env);

    // No `create_vesting_schedule` call → counter stays at 0 → loop `1..=0`
    // never executes.
    let summary = client.get_vesting_summary(&user);

    assert_eq!(summary.grant_count, 0, "grant_count must be 0 with empty storage");
    assert_eq!(summary.total_granted, 0, "total_granted must be 0 with empty storage");
    assert_eq!(summary.total_released, 0, "total_released must be 0 with empty storage");
    assert_eq!(summary.total_releasable, 0, "total_releasable must be 0 with empty storage");
}

// ── Issue boundary: Single ledger ─────────────────────────────────────────────
//
// Exactly one vesting schedule exists in storage.  The loop `1..=1` runs
// exactly once.  The summary must reflect that single schedule and nothing
// more.

/// Single ledger: one schedule, queried before cliff → grant_count = 1,
/// total_releasable = 0 (cliff not yet reached).
///
/// Pins the `1..=1` loop iteration so a refactoring that skips id 1 would
/// be caught immediately.
#[test]
fn returns_grant_count_one_for_single_schedule_before_cliff() {
    let (env, client, admin, token_id) = setup();
    let user = Address::generate(&env);

    // Timestamps: now = 1_000, start = 1_000, cliff at +500 = 1_500, end = 3_000
    client.create_vesting_schedule(&admin, &token_id, &user, &5_000, &1_000, &500, &3_000);

    // Still at timestamp 1_000 — before cliff — nothing releasable yet.
    let summary = client.get_vesting_summary(&user);

    assert_eq!(summary.grant_count, 1, "exactly one schedule must be counted");
    assert_eq!(summary.total_granted, 5_000, "total_granted must equal the schedule amount");
    assert_eq!(summary.total_released, 0);
    assert_eq!(summary.total_releasable, 0, "cliff not reached → nothing releasable");
}

/// Single ledger: one schedule, queried at midpoint after cliff →
/// total_releasable is the pro-rata vested amount.
#[test]
fn returns_correct_releasable_for_single_schedule_at_midpoint() {
    let (env, client, admin, token_id) = setup();
    let user = Address::generate(&env);

    // start = 1_000, no cliff (cliff_seconds = 0), end = 3_000, total = 4_000
    // At t = 2_000: elapsed = 1_000, duration = 2_000 → vested = 2_000
    client.create_vesting_schedule(&admin, &token_id, &user, &4_000, &1_000, &0, &3_000);

    env.ledger().set_timestamp(2_000);
    let summary = client.get_vesting_summary(&user);

    assert_eq!(summary.grant_count, 1);
    assert_eq!(summary.total_granted, 4_000);
    assert_eq!(summary.total_released, 0);
    assert_eq!(summary.total_releasable, 2_000);
}

// ── Issue boundary: Wide range ────────────────────────────────────────────────
//
// Many schedules in storage belonging to multiple users.  The loop
// `1..=max_id` must iterate all IDs but only accumulate the queried user's
// grants.  Schedules belonging to other users must be silently skipped.

/// Wide range: N schedules for two users interleaved by creation order.
/// Each user's summary must reflect only their own grants.
///
/// This ensures the `beneficiary` filter inside the scan loop is applied
/// correctly across a wide (> 1) counter range.
#[test]
fn counts_only_own_grants_across_wide_range_of_schedule_ids() {
    let (env, client, admin, token_id) = setup();
    let user_a = Address::generate(&env);
    let user_b = Address::generate(&env);

    // Create schedules interleaved: A, B, A, B, A  → IDs 1..=5
    // A gets 3 schedules of 1_000 each → total 3_000
    // B gets 2 schedules of 2_000 each → total 4_000
    client.create_vesting_schedule(&admin, &token_id, &user_a, &1_000, &1_000, &0, &5_000); // id 1
    client.create_vesting_schedule(&admin, &token_id, &user_b, &2_000, &1_000, &0, &5_000); // id 2
    client.create_vesting_schedule(&admin, &token_id, &user_a, &1_000, &1_000, &0, &5_000); // id 3
    client.create_vesting_schedule(&admin, &token_id, &user_b, &2_000, &1_000, &0, &5_000); // id 4
    client.create_vesting_schedule(&admin, &token_id, &user_a, &1_000, &1_000, &0, &5_000); // id 5

    let summary_a = client.get_vesting_summary(&user_a);
    let summary_b = client.get_vesting_summary(&user_b);

    // User A: 3 grants, total 3_000
    assert_eq!(summary_a.grant_count, 3, "user_a must have exactly 3 grants");
    assert_eq!(summary_a.total_granted, 3_000, "user_a total_granted mismatch");

    // User B: 2 grants, total 4_000
    assert_eq!(summary_b.grant_count, 2, "user_b must have exactly 2 grants");
    assert_eq!(summary_b.total_granted, 4_000, "user_b total_granted mismatch");
}

/// Wide range: a user with no grants in a populated schedule registry
/// still receives a zeroed summary (no false positives from other users'
/// schedules).
#[test]
fn returns_zeroed_summary_for_user_with_no_grants_in_populated_registry() {
    let (env, client, admin, token_id) = setup();
    let owner = Address::generate(&env);
    let stranger = Address::generate(&env);

    // Create several schedules for `owner`; `stranger` owns none.
    for _ in 0..5 {
        client.create_vesting_schedule(&admin, &token_id, &owner, &1_000, &1_000, &0, &5_000);
    }

    // Querying `stranger` must return a fully-zeroed summary even though the
    // counter is now at 5.
    let summary = client.get_vesting_summary(&stranger);

    assert_eq!(summary.grant_count, 0);
    assert_eq!(summary.total_granted, 0);
    assert_eq!(summary.total_released, 0);
    assert_eq!(summary.total_releasable, 0);
}

// ── Carried-forward cases (unchanged from the original test file) ─────────────

/// Empty user (alias for the "no schedules created" case, kept for symmetry
/// with the original test name expected by reviewers).
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
    client.release_vested_tokens(&user, &id);

    let summary = client.get_vesting_summary(&user);
    assert_eq!(summary.grant_count, 1);
    assert_eq!(summary.total_granted, total);
    assert_eq!(summary.total_released, 2_000);
    assert_eq!(summary.total_releasable, 0); // already claimed
}

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
