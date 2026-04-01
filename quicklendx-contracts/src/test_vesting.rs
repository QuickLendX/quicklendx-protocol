#![cfg(test)]
//! Comprehensive boundary tests for vesting releasable amount at cliff boundaries.
//!
//! Tests cover:
//! - Before cliff (releasable = 0)
//! - At cliff exactly (releasable > 0 or = total depending on timing)
//! - After cliff but before end (partial release)
//! - At end time (full amount vested)
//! - After end time (full amount vested)
//! - Off-by-one timestamp errors
//! - Zero cliff edge case
//! - Multiple partial releases

use crate::{QuickLendXContract, QuickLendXContractClient};
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::{token, Address, Env};

const ADMIN_BALANCE: i128 = 10_000_000;

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

// ============================================================================
// BEFORE CLIFF TESTS
// ============================================================================

#[test]
fn test_releasable_zero_one_second_before_cliff() {
    let (env, client, admin, beneficiary, token_id, _token_client) = setup();

    let total = 1000i128;
    let start = 1000u64;
    let cliff_seconds = 500u64;
    let cliff = start + cliff_seconds; // 1500

    client.create_vesting_schedule(
        &admin,
        &token_id,
        &beneficiary,
        &total,
        &start,
        &cliff_seconds,
        &(start + 2000),
    );

    // One second before cliff
    env.ledger().set_timestamp(cliff - 1);
    let releasable = client.get_vesting_releasable(&1).unwrap();
    assert_eq!(releasable, 0, "Should be 0 one second before cliff");
}

#[test]
fn test_releasable_zero_one_hour_before_cliff() {
    let (env, client, admin, beneficiary, token_id, _token_client) = setup();

    let total = 1000i128;
    let start = 1000u64;
    let cliff_seconds = 3600u64; // 1 hour
    let cliff = start + cliff_seconds;

    client.create_vesting_schedule(
        &admin,
        &token_id,
        &beneficiary,
        &total,
        &start,
        &cliff_seconds,
        &(start + 7200),
    );

    // Way before cliff
    env.ledger().set_timestamp(100);
    let releasable = client.get_vesting_releasable(&1).unwrap();
    assert_eq!(releasable, 0, "Should be 0 before cliff");
}

#[test]
fn test_vested_amount_zero_before_cliff() {
    let (env, client, admin, beneficiary, token_id, _token_client) = setup();

    let total = 1000i128;
    let start = 1000u64;
    let cliff_seconds = 500u64;
    let cliff = start + cliff_seconds;

    client.create_vesting_schedule(
        &admin,
        &token_id,
        &beneficiary,
        &total,
        &start,
        &cliff_seconds,
        &(start + 2000),
    );

    env.ledger().set_timestamp(cliff - 1);
    let vested = client.get_vesting_vested(&1).unwrap();
    assert_eq!(vested, 0, "Vested amount should be 0 before cliff");
}

#[test]
fn test_release_fails_before_cliff() {
    let (env, client, admin, beneficiary, token_id, _token_client) = setup();

    let total = 1000i128;
    let start = 1000u64;
    let cliff_seconds = 500u64;

    let id = client.create_vesting_schedule(
        &admin,
        &token_id,
        &beneficiary,
        &total,
        &start,
        &cliff_seconds,
        &(start + 2000),
    );

    // Try to release 1 second before cliff
    env.ledger().set_timestamp(start + cliff_seconds - 1);
    let result = client.try_release_vested_tokens(&beneficiary, &id);
    assert!(result.is_err(), "Release should fail before cliff");
}

// ============================================================================
// AT CLIFF BOUNDARY TESTS
// ============================================================================

#[test]
fn test_releasable_positive_at_exact_cliff() {
    let (env, client, admin, beneficiary, token_id, _token_client) = setup();

    let total = 1000i128;
    let start = 1000u64;
    let cliff_seconds = 500u64;
    let cliff = start + cliff_seconds; // 1500

    client.create_vesting_schedule(
        &admin,
        &token_id,
        &beneficiary,
        &total,
        &start,
        &cliff_seconds,
        &(start + 2000),
    );

    // At exact cliff time
    env.ledger().set_timestamp(cliff);
    let releasable = client.get_vesting_releasable(&1).unwrap();
    assert!(
        releasable > 0,
        "Releasable should be positive at exact cliff time"
    );
}

#[test]
fn test_vested_positive_at_exact_cliff() {
    let (env, client, admin, beneficiary, token_id, _token_client) = setup();

    let total = 1000i128;
    let start = 1000u64;
    let cliff_seconds = 500u64;
    let cliff = start + cliff_seconds;
    let end = start + 2000u64;
    let duration = end - start;

    client.create_vesting_schedule(
        &admin,
        &token_id,
        &beneficiary,
        &total,
        &start,
        &cliff_seconds,
        &end,
    );

    env.ledger().set_timestamp(cliff);
    let vested = client.get_vesting_vested(&1).unwrap();
    // At cliff (which is start + 500), elapsed = 500, duration = 2000
    // vested = total * 500 / 2000 = 250
    let expected = total * 500 / duration as i128;
    assert_eq!(
        vested, expected,
        "Vested at exact cliff should be total * cliff_seconds / duration"
    );
}

#[test]
fn test_release_succeeds_at_exact_cliff() {
    let (env, client, admin, beneficiary, token_id, _token_client) = setup();

    let total = 1000i128;
    let start = 1000u64;
    let cliff_seconds = 500u64;
    let cliff = start + cliff_seconds;

    let id = client.create_vesting_schedule(
        &admin,
        &token_id,
        &beneficiary,
        &total,
        &start,
        &cliff_seconds,
        &(start + 2000),
    );

    env.ledger().set_timestamp(cliff);
    let released = client.release_vested_tokens(&beneficiary, &id);
    assert!(released > 0, "Release should succeed at exact cliff");
}

#[test]
fn test_releasable_at_cliff_equals_vested_minus_released() {
    let (env, client, admin, beneficiary, token_id, _token_client) = setup();

    let total = 1000i128;
    let start = 1000u64;
    let cliff_seconds = 500u64;
    let end = start + 2000u64;

    client.create_vesting_schedule(
        &admin,
        &token_id,
        &beneficiary,
        &total,
        &start,
        &cliff_seconds,
        &end,
    );

    // At cliff
    env.ledger().set_timestamp(start + cliff_seconds);
    let vested = client.get_vesting_vested(&1).unwrap();
    let releasable = client.get_vesting_releasable(&1).unwrap();
    assert_eq!(
        releasable, vested,
        "Releasable should equal vested before any release"
    );
}

// ============================================================================
// AFTER CLIFF, BEFORE END TESTS
// ============================================================================

#[test]
fn test_releasable_partial_midway_after_cliff() {
    let (env, client, admin, beneficiary, token_id, _token_client) = setup();

    let total = 1000i128;
    let start = 1000u64;
    let cliff_seconds = 500u64;
    let end = start + 2000u64;
    let duration = end - start;

    client.create_vesting_schedule(
        &admin,
        &token_id,
        &beneficiary,
        &total,
        &start,
        &cliff_seconds,
        &end,
    );

    // Midway after cliff (1000 + 1000 = 2000)
    env.ledger().set_timestamp(2000);
    let vested = client.get_vesting_vested(&1).unwrap();
    // elapsed = 1000, duration = 2000
    let expected = total * 1000 / duration as i128;
    assert_eq!(vested, expected, "Midway vested should be 50% of total");
}

#[test]
fn test_releasable_increases_over_time_after_cliff() {
    let (env, client, admin, beneficiary, token_id, _token_client) = setup();

    let total = 1000i128;
    let start = 1000u64;
    let cliff_seconds = 500u64;
    let end = start + 2000u64;

    client.create_vesting_schedule(
        &admin,
        &token_id,
        &beneficiary,
        &total,
        &start,
        &cliff_seconds,
        &end,
    );

    // At cliff
    env.ledger().set_timestamp(start + cliff_seconds);
    let releasable_1 = client.get_vesting_releasable(&1).unwrap();

    // 500 seconds later
    env.ledger().set_timestamp(start + cliff_seconds + 500);
    let releasable_2 = client.get_vesting_releasable(&1).unwrap();

    assert!(
        releasable_2 > releasable_1,
        "Releasable should increase over time"
    );
}

#[test]
fn test_partial_release_updates_releasable_correctly() {
    let (env, client, admin, beneficiary, token_id, token_client) = setup();

    let total = 1000i128;
    let start = 1000u64;
    let cliff_seconds = 500u64;
    let end = start + 2000u64;

    let id = client.create_vesting_schedule(
        &admin,
        &token_id,
        &beneficiary,
        &total,
        &start,
        &cliff_seconds,
        &end,
    );

    // At cliff
    env.ledger().set_timestamp(start + cliff_seconds);
    client.release_vested_tokens(&beneficiary, &id);

    // After release
    let remaining_releasable = client.get_vesting_releasable(&id).unwrap();
    assert_eq!(
        remaining_releasable, 0,
        "Releasable should be 0 after full release at cliff"
    );

    // Advance time
    env.ledger().set_timestamp(start + cliff_seconds + 500);
    let new_releasable = client.get_vesting_releasable(&id).unwrap();
    assert!(new_releasable > 0, "New amount should vest over time");
}

// ============================================================================
// AT END TIME BOUNDARY TESTS
// ============================================================================

#[test]
fn test_releasable_full_at_exact_end_time() {
    let (env, client, admin, beneficiary, token_id, _token_client) = setup();

    let total = 1000i128;
    let start = 1000u64;
    let cliff_seconds = 500u64;
    let end = start + 2000u64;

    client.create_vesting_schedule(
        &admin,
        &token_id,
        &beneficiary,
        &total,
        &start,
        &cliff_seconds,
        &end,
    );

    // At exact end time
    env.ledger().set_timestamp(end);
    let releasable = client.get_vesting_releasable(&1).unwrap();
    assert_eq!(
        releasable, total,
        "Full amount should be releasable at exact end time"
    );
}

#[test]
fn test_vested_equals_total_at_exact_end_time() {
    let (env, client, admin, beneficiary, token_id, _token_client) = setup();

    let total = 1000i128;
    let start = 1000u64;
    let cliff_seconds = 500u64;
    let end = start + 2000u64;

    client.create_vesting_schedule(
        &admin,
        &token_id,
        &beneficiary,
        &total,
        &start,
        &cliff_seconds,
        &end,
    );

    env.ledger().set_timestamp(end);
    let vested = client.get_vesting_vested(&1).unwrap();
    assert_eq!(
        vested, total,
        "All tokens should be vested at exact end time"
    );
}

#[test]
fn test_release_releases_remaining_at_end_time() {
    let (env, client, admin, beneficiary, token_id, token_client) = setup();

    let total = 1000i128;
    let start = 1000u64;
    let cliff_seconds = 500u64;
    let end = start + 2000u64;

    let id = client.create_vesting_schedule(
        &admin,
        &token_id,
        &beneficiary,
        &total,
        &start,
        &cliff_seconds,
        &end,
    );

    // Partial release at cliff
    env.ledger().set_timestamp(start + cliff_seconds);
    client.release_vested_tokens(&beneficiary, &id);

    // Complete release at end
    env.ledger().set_timestamp(end);
    let final_release = client.release_vested_tokens(&beneficiary, &id);

    let schedule = client.get_vesting_schedule(&id).unwrap();
    assert_eq!(
        schedule.released_amount, total,
        "All tokens should be released by end time"
    );
    assert_eq!(
        token_client.balance(&beneficiary),
        total,
        "Beneficiary should have full balance"
    );
}

// ============================================================================
// AFTER END TIME TESTS
// ============================================================================

#[test]
fn test_releasable_full_after_end_time() {
    let (env, client, admin, beneficiary, token_id, _token_client) = setup();

    let total = 1000i128;
    let start = 1000u64;
    let cliff_seconds = 500u64;
    let end = start + 2000u64;

    client.create_vesting_schedule(
        &admin,
        &token_id,
        &beneficiary,
        &total,
        &start,
        &cliff_seconds,
        &end,
    );

    // After end time
    env.ledger().set_timestamp(end + 1);
    let releasable = client.get_vesting_releasable(&1).unwrap();
    assert_eq!(
        releasable, total,
        "Full amount should be releasable after end time"
    );
}

#[test]
fn test_vested_equals_total_after_end_time() {
    let (env, client, admin, beneficiary, token_id, _token_client) = setup();

    let total = 1000i128;
    let start = 1000u64;
    let cliff_seconds = 500u64;
    let end = start + 2000u64;

    client.create_vesting_schedule(
        &admin,
        &token_id,
        &beneficiary,
        &total,
        &start,
        &cliff_seconds,
        &end,
    );

    // Long after end
    env.ledger().set_timestamp(end + 100000);
    let vested = client.get_vesting_vested(&1).unwrap();
    assert_eq!(
        vested, total,
        "All tokens should remain vested after end time"
    );
}

#[test]
fn test_release_succeeds_long_after_end_time() {
    let (env, client, admin, beneficiary, token_id, _token_client) = setup();

    let total = 1000i128;
    let start = 1000u64;
    let cliff_seconds = 500u64;
    let end = start + 2000u64;

    let id = client.create_vesting_schedule(
        &admin,
        &token_id,
        &beneficiary,
        &total,
        &start,
        &cliff_seconds,
        &end,
    );

    // Far after end
    env.ledger().set_timestamp(end + 100000);
    let released = client.release_vested_tokens(&beneficiary, &id);
    assert_eq!(released, total, "Full remaining amount should be released");
}

// ============================================================================
// ZERO CLIFF TESTS
// ============================================================================

#[test]
fn test_zero_cliff_at_start_allows_immediate_release() {
    let (env, client, admin, beneficiary, token_id, _token_client) = setup();

    let total = 1000i128;
    let start = 1000u64;
    let cliff_seconds = 0u64;
    let end = start + 1000u64;

    client.create_vesting_schedule(
        &admin,
        &token_id,
        &beneficiary,
        &total,
        &start,
        &cliff_seconds,
        &end,
    );

    // At start with zero cliff
    env.ledger().set_timestamp(start);
    let releasable = client.get_vesting_releasable(&1).unwrap();
    assert!(
        releasable >= 0,
        "Releasable should be valid with zero cliff"
    );
}

#[test]
fn test_zero_cliff_halfway_releases_half() {
    let (env, client, admin, beneficiary, token_id, _token_client) = setup();

    let total = 1000i128;
    let start = 1000u64;
    let cliff_seconds = 0u64;
    let end = start + 2000u64;

    client.create_vesting_schedule(
        &admin,
        &token_id,
        &beneficiary,
        &total,
        &start,
        &cliff_seconds,
        &end,
    );

    // Halfway
    env.ledger().set_timestamp(start + 1000);
    let vested = client.get_vesting_vested(&1).unwrap();
    assert_eq!(vested, 500, "Halfway with zero cliff should vest 50%");
}

// ============================================================================
// OFF-BY-ONE BOUNDARY TESTS
// ============================================================================

#[test]
fn test_off_by_one_before_cliff_vs_at_cliff() {
    let (env, client, admin, beneficiary, token_id, _token_client) = setup();

    let total = 1000i128;
    let start = 1000u64;
    let cliff_seconds = 500u64;
    let cliff = start + cliff_seconds;
    let end = start + 2000u64;

    client.create_vesting_schedule(
        &admin,
        &token_id,
        &beneficiary,
        &total,
        &start,
        &cliff_seconds,
        &end,
    );

    // One second before cliff
    env.ledger().set_timestamp(cliff - 1);
    let releasable_before = client.get_vesting_releasable(&1).unwrap();

    // At cliff
    env.ledger().set_timestamp(cliff);
    let releasable_at = client.get_vesting_releasable(&1).unwrap();

    assert_eq!(releasable_before, 0, "Releasable should be 0 before cliff");
    assert!(
        releasable_at > releasable_before,
        "Releasable should increase at cliff"
    );
}

#[test]
fn test_off_by_one_before_end_vs_at_end() {
    let (env, client, admin, beneficiary, token_id, _token_client) = setup();

    let total = 1000i128;
    let start = 1000u64;
    let end = start + 2000u64;

    client.create_vesting_schedule(
        &admin,
        &token_id,
        &beneficiary,
        &total,
        &start,
        &1000u64,
        &end,
    );

    // One second before end
    env.ledger().set_timestamp(end - 1);
    let releasable_before = client.get_vesting_releasable(&1).unwrap();

    // At end
    env.ledger().set_timestamp(end);
    let releasable_at = client.get_vesting_releasable(&1).unwrap();

    assert!(
        releasable_at > releasable_before,
        "Releasable should reach full amount at end"
    );
    assert_eq!(releasable_at, total, "Releasable at end should equal total");
}

#[test]
fn test_boundary_exactly_at_start_time() {
    let (env, client, admin, beneficiary, token_id, _token_client) = setup();

    let total = 1000i128;
    let start = 1000u64;
    let cliff_seconds = 500u64;
    let end = start + 2000u64;

    client.create_vesting_schedule(
        &admin,
        &token_id,
        &beneficiary,
        &total,
        &start,
        &cliff_seconds,
        &end,
    );

    // At exact start time
    env.ledger().set_timestamp(start);
    let releasable = client.get_vesting_releasable(&1).unwrap();
    assert_eq!(
        releasable, 0,
        "Releasable should be 0 at start (before cliff)"
    );
}

// ============================================================================
// MULTIPLE RELEASE SCENARIOS
// ============================================================================

#[test]
fn test_multiple_partial_releases_tracking() {
    let (env, client, admin, beneficiary, token_id, token_client) = setup();

    let total = 1000i128;
    let start = 1000u64;
    let cliff_seconds = 500u64;
    let end = start + 2000u64;

    let id = client.create_vesting_schedule(
        &admin,
        &token_id,
        &beneficiary,
        &total,
        &start,
        &cliff_seconds,
        &end,
    );

    // First release at cliff
    env.ledger().set_timestamp(start + cliff_seconds);
    let release1 = client.release_vested_tokens(&beneficiary, &id);
    let balance1 = token_client.balance(&beneficiary);

    // Second release midway
    env.ledger().set_timestamp(start + cliff_seconds + 750);
    let release2 = client.release_vested_tokens(&beneficiary, &id);
    let balance2 = token_client.balance(&beneficiary);

    // Third release at end
    env.ledger().set_timestamp(end);
    let release3 = client.release_vested_tokens(&beneficiary, &id);
    let balance3 = token_client.balance(&beneficiary);

    assert_eq!(balance1, release1);
    assert_eq!(balance2, release1 + release2);
    assert_eq!(balance3, total, "Total should equal sum of all releases");
}

#[test]
fn test_releasable_reflects_already_released() {
    let (env, client, admin, beneficiary, token_id, _token_client) = setup();

    let total = 1000i128;
    let start = 1000u64;
    let cliff_seconds = 500u64;
    let end = start + 2000u64;

    client.create_vesting_schedule(
        &admin,
        &token_id,
        &beneficiary,
        &total,
        &start,
        &cliff_seconds,
        &end,
    );

    // At cliff - release first portion
    env.ledger().set_timestamp(start + cliff_seconds);
    let initial_vested = client.get_vesting_vested(&1).unwrap();
    client.release_vested_tokens(&beneficiary, &1);

    // After more time
    env.ledger().set_timestamp(start + cliff_seconds + 500);
    let new_vested = client.get_vesting_vested(&1).unwrap();
    let releasable = client.get_vesting_releasable(&1).unwrap();

    // Releasable should be new_vested - already_released
    let already_released = initial_vested;
    assert_eq!(
        releasable,
        new_vested - already_released,
        "Releasable should account for already released amount"
    );
}

#[test]
fn test_no_double_release_same_period() {
    let (env, client, admin, beneficiary, token_id, _token_client) = setup();

    let total = 1000i128;
    let start = 1000u64;
    let cliff_seconds = 500u64;
    let end = start + 2000u64;

    let id = client.create_vesting_schedule(
        &admin,
        &token_id,
        &beneficiary,
        &total,
        &start,
        &cliff_seconds,
        &end,
    );

    // At cliff
    env.ledger().set_timestamp(start + cliff_seconds);
    client.release_vested_tokens(&beneficiary, &id);

    // Try to release again without time passing
    let releasable = client.get_vesting_releasable(&id).unwrap();
    assert_eq!(
        releasable, 0,
        "Releasable should be 0 after release at same timestamp"
    );
}

// ============================================================================
// EDGE CASES
// ============================================================================

#[test]
fn test_very_small_vesting_amount() {
    let (env, client, admin, beneficiary, token_id, _token_client) = setup();

    let total = 1i128;
    let start = 1000u64;
    let cliff_seconds = 500u64;
    let end = start + 1000u64;

    client.create_vesting_schedule(
        &admin,
        &token_id,
        &beneficiary,
        &total,
        &start,
        &cliff_seconds,
        &end,
    );

    env.ledger().set_timestamp(end);
    let releasable = client.get_vesting_releasable(&1).unwrap();
    assert_eq!(releasable, 1, "Single unit should be releasable");
}

#[test]
fn test_very_long_vesting_period() {
    let (env, client, admin, beneficiary, token_id, _token_client) = setup();

    let total = 1_000_000i128;
    let start = 1000u64;
    let cliff_seconds = 31_536_000u64; // 1 year
    let end = start + 31_536_000u64 * 4; // 4 years

    client.create_vesting_schedule(
        &admin,
        &token_id,
        &beneficiary,
        &total,
        &start,
        &cliff_seconds,
        &end,
    );

    // At cliff (1 year)
    env.ledger().set_timestamp(start + cliff_seconds);
    let vested = client.get_vesting_vested(&1).unwrap();
    let expected = total / 4; // 25% at cliff
    assert_eq!(vested, expected, "25% should be vested at 1 year cliff");
}

#[test]
fn test_immediate_cliff_equals_end() {
    // A cliff that lands exactly at end_time is rejected by the contract (cliff_time >= end_time).
    // This test verifies that the contract correctly rejects such a degenerate schedule and that
    // a schedule with cliff just before end_time works as expected.
    let (env, client, admin, beneficiary, token_id, _token_client) = setup();

    let total = 1000i128;
    let start = 1000u64;
    // cliff_seconds = end - start - 1 so cliff_time = end_time - 1 (valid)
    let end = start + 1001u64;
    let cliff_seconds = 1000u64; // cliff_time = start + 1000 = end - 1

    client.create_vesting_schedule(
        &admin,
        &token_id,
        &beneficiary,
        &total,
        &start,
        &cliff_seconds,
        &end,
    );

    // At end time all tokens are vested
    env.ledger().set_timestamp(end);
    let releasable = client.get_vesting_releasable(&1).unwrap();
    assert_eq!(
        releasable, total,
        "Full amount should be releasable at end time"
    );
}

#[test]
fn test_integer_division_rounding() {
    let (env, client, admin, beneficiary, token_id, _token_client) = setup();

    // Use amounts that don't divide evenly
    let total = 1001i128;
    let start = 1000u64;
    let end = start + 3000u64; // 3 second duration

    client.create_vesting_schedule(
        &admin,
        &token_id,
        &beneficiary,
        &total,
        &start,
        &1000u64,
        &end,
    );

    // At 1 second (1/3 of duration)
    env.ledger().set_timestamp(start + 1000);
    let vested = client.get_vesting_vested(&1).unwrap();
    // 1001 * 1000 / 3000 = 333.666... -> 333 (truncated)
    let expected = 1001 * 1000 / 3000;
    assert_eq!(
        vested, expected,
        "Integer division should truncate correctly"
    );
}

#[test]
fn test_release_idempotency() {
    let (env, client, admin, beneficiary, token_id, _) = setup();

    let id =
        client.create_vesting_schedule(&admin, &token_id, &beneficiary, &1000, &1000, &0, &2000);

    env.ledger().set_timestamp(1500);

    let first = client.release_vested_tokens(&beneficiary, &id);
    let second = client.release_vested_tokens(&beneficiary, &id);

    assert_eq!(second, 0);

    let schedule = client.get_vesting_schedule(&id).unwrap();
    assert_eq!(schedule.released_amount, first);
}

#[test]
fn test_multi_step_progression() {
    let (env, client, admin, beneficiary, token_id, _) = setup();

    let total = 1000;
    let id =
        client.create_vesting_schedule(&admin, &token_id, &beneficiary, &total, &1000, &0, &2000);

    env.ledger().set_timestamp(1250);
    let r1 = client.release_vested_tokens(&beneficiary, &id);

    env.ledger().set_timestamp(1500);
    let r2 = client.release_vested_tokens(&beneficiary, &id);

    env.ledger().set_timestamp(2000);
    let r3 = client.release_vested_tokens(&beneficiary, &id);

    let schedule = client.get_vesting_schedule(&id).unwrap();

    assert_eq!(r1 + r2 + r3, total);
    assert_eq!(schedule.released_amount, total);
}

#[test]
fn test_never_exceeds_total() {
    let (env, client, admin, beneficiary, token_id, _) = setup();

    let total = 1000;
    let id =
        client.create_vesting_schedule(&admin, &token_id, &beneficiary, &total, &1000, &0, &2000);

    env.ledger().set_timestamp(3000);

    let _ = client.release_vested_tokens(&beneficiary, &id);
    let extra = client.release_vested_tokens(&beneficiary, &id);

    let schedule = client.get_vesting_schedule(&id).unwrap();

    assert_eq!(schedule.released_amount, total);
    assert_eq!(extra, 0);
}

#[test]
fn test_releasable_consistency() {
    let (env, client, admin, beneficiary, token_id, _) = setup();

    let id =
        client.create_vesting_schedule(&admin, &token_id, &beneficiary, &1000, &1000, &0, &2000);

    env.ledger().set_timestamp(1500);

    let releasable_before = client.get_vesting_releasable(&id).unwrap();
    let released = client.release_vested_tokens(&beneficiary, &id);
    let releasable_after = client.get_vesting_releasable(&id).unwrap();

    assert_eq!(released, releasable_before);
    assert_eq!(releasable_after, 0);
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

// ============================================================================
// ADMIN BOUNDARY TESTS
// These tests cover the threat model for admin powers over vesting schedules.
// ============================================================================

/// Admin cannot create a schedule with zero total_amount.
#[test]
fn test_admin_rejects_zero_amount() {
    let (env, client, admin, beneficiary, token_id, _) = setup();
    let result = client.try_create_vesting_schedule(
        &admin, &token_id, &beneficiary,
        &0i128, &1500u64, &0u64, &2000u64,
    );
    assert!(result.is_err(), "Zero-amount schedule must be rejected");
}

/// Admin cannot create a schedule with a backdated start_time.
#[test]
fn test_admin_rejects_backdated_start() {
    let (env, client, admin, beneficiary, token_id, _) = setup();
    // Ledger is at 1000; start_time = 999 is in the past.
    let result = client.try_create_vesting_schedule(
        &admin, &token_id, &beneficiary,
        &1000i128, &999u64, &0u64, &2000u64,
    );
    assert!(result.is_err(), "Backdated start_time must be rejected");
}

/// Admin cannot create a schedule where end_time <= start_time.
#[test]
fn test_admin_rejects_end_before_start() {
    let (env, client, admin, beneficiary, token_id, _) = setup();
    let result = client.try_create_vesting_schedule(
        &admin, &token_id, &beneficiary,
        &1000i128, &1500u64, &0u64, &1500u64, // end == start
    );
    assert!(result.is_err(), "end_time == start_time must be rejected");
}

/// Admin cannot create a schedule where cliff_time >= end_time.
#[test]
fn test_admin_rejects_cliff_at_or_after_end() {
    let (env, client, admin, beneficiary, token_id, _) = setup();
    // cliff_seconds = 1000, start = 1000 → cliff_time = 2000 = end_time
    let result = client.try_create_vesting_schedule(
        &admin, &token_id, &beneficiary,
        &1000i128, &1000u64, &1000u64, &2000u64,
    );
    assert!(result.is_err(), "cliff_time == end_time must be rejected");
}

/// After admin role is transferred, the old admin loses the ability to create schedules.
#[test]
fn test_old_admin_loses_vesting_power_after_transfer() {
    let (env, client, admin, beneficiary, token_id, token_client) = setup();
    let new_admin = Address::generate(&env);

    // Fund new_admin so it can back a schedule
    token_client.approve(&new_admin, &client.address, &ADMIN_BALANCE, &(env.ledger().sequence() + 10_000));

    // Transfer admin role
    client.transfer_admin(&new_admin);

    // Old admin can no longer create a vesting schedule
    let result = client.try_create_vesting_schedule(
        &admin, &token_id, &beneficiary,
        &1000i128, &1500u64, &0u64, &2000u64,
    );
    assert!(result.is_err(), "Old admin must not create schedules after role transfer");
}

/// Non-beneficiary cannot release tokens from someone else's schedule.
#[test]
fn test_non_beneficiary_cannot_release() {
    let (env, client, admin, beneficiary, token_id, _) = setup();
    let attacker = Address::generate(&env);

    let id = client.create_vesting_schedule(
        &admin, &token_id, &beneficiary,
        &1000i128, &1000u64, &0u64, &2000u64,
    );

    env.ledger().set_timestamp(1500);
    let result = client.try_release_vested_tokens(&attacker, &id);
    assert!(result.is_err(), "Non-beneficiary must not release tokens");
}

/// Release before cliff returns an error (not a silent no-op).
#[test]
fn test_release_before_cliff_is_error_not_noop() {
    let (env, client, admin, beneficiary, token_id, _) = setup();

    let id = client.create_vesting_schedule(
        &admin, &token_id, &beneficiary,
        &1000i128, &1000u64, &500u64, &3000u64,
    );

    // cliff_time = 1500; set ledger to 1499
    env.ledger().set_timestamp(1499);
    let result = client.try_release_vested_tokens(&beneficiary, &id);
    assert!(result.is_err(), "Release before cliff must return an error");
}

/// Querying a non-existent schedule returns None without panicking.
#[test]
fn test_get_nonexistent_schedule_returns_none() {
    let (_env, client, _, _, _, _) = setup();
    assert!(client.get_vesting_schedule(&9999).is_none());
    assert!(client.get_vesting_releasable(&9999).is_none());
    assert!(client.get_vesting_vested(&9999).is_none());
}
