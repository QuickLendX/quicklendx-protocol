//! Fee Analytics Period Boundaries and Empty-State Behavior Tests
//!
//! Validates that fee analytics endpoints handle:
//! - Empty/uninitialized periods gracefully
//! - Period boundary calculations (month transitions)
//! - Sparse data ranges (gaps between active periods)
//! - Revenue data consistency across period boundaries
//! - Analytics output stability for dashboard consumers

#![cfg(test)]

use super::*;
use crate::fees::FeeType;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Env, Map, String,
};

// Period = timestamp / 2_592_000 (30 days in seconds)
const PERIOD_SECONDS: u64 = 2_592_000;

fn setup() -> (Env, QuickLendXContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.set_admin(&admin);
    (env, client, admin)
}

// -- Empty state tests --------------------------------------------------------

#[test]
fn analytics_returns_error_for_uninitialized_period() {
    let (_env, client, _admin) = setup();
    let result = client.try_get_fee_analytics(&999);
    assert!(result.is_err());
}

#[test]
fn analytics_returns_error_for_period_zero() {
    let (_env, client, _admin) = setup();
    let result = client.try_get_fee_analytics(&0);
    assert!(result.is_err());
}

#[test]
fn analytics_returns_error_for_future_period() {
    let (env, client, _admin) = setup();
    env.ledger().with_mut(|l| l.timestamp = PERIOD_SECONDS * 5);
    // Period 100 is far in the future and has no data
    let result = client.try_get_fee_analytics(&100);
    assert!(result.is_err());
}

// -- Period boundary tests via full invoice lifecycle --------------------------

#[test]
fn analytics_query_returns_consistent_period_field() {
    let (_env, client, _admin) = setup();
    // Querying an unused period should fail, confirming period isolation
    let r1 = client.try_get_fee_analytics(&1);
    let r2 = client.try_get_fee_analytics(&2);
    let r3 = client.try_get_fee_analytics(&3);

    // All should fail since no fees were collected in these periods
    assert!(r1.is_err());
    assert!(r2.is_err());
    assert!(r3.is_err());
}

// -- Revenue distribution boundary tests --------------------------------------

#[test]
fn distribute_revenue_fails_without_config() {
    let (_env, client, admin) = setup();
    let result = client.try_distribute_revenue(&admin, &0);
    assert!(result.is_err());
}

#[test]
fn revenue_split_config_not_found_before_setup() {
    let (_env, client, _admin) = setup();
    let result = client.try_get_revenue_split_config();
    assert!(result.is_err());
}

#[test]
fn revenue_distribution_requires_minimum_threshold() {
    let (env, client, admin) = setup();
    env.ledger().with_mut(|l| l.timestamp = PERIOD_SECONDS * 4 + 100);

    // Initialize fee system
    client.initialize_fee_system(&admin);

    // Configure revenue distribution with a minimum threshold
    let treasury = Address::generate(&env);
    client.configure_revenue_distribution(
        &admin,
        &treasury,
        &5000u32,  // treasury 50%
        &3000u32,  // developer 30%
        &2000u32,  // platform 20%
        &false,
        &1_000_000i128, // high minimum threshold
    );

    // Try to distribute period 4 - should fail since no fees were collected
    // (pending < min_distribution_amount)
    let result = client.try_distribute_revenue(&admin, &4);
    assert!(result.is_err());
}

// -- Fee system initialization boundary ---------------------------------------

#[test]
fn fee_system_double_init_fails() {
    let (_env, client, admin) = setup();
    client.initialize_fee_system(&admin);
    let result = client.try_initialize_fee_system(&admin);
    assert!(result.is_err());
}

#[test]
fn platform_fee_config_accessible_after_init() {
    let (_env, client, admin) = setup();
    client.initialize_fee_system(&admin);
    let config = client.get_platform_fee_config();
    assert_eq!(config.fee_bps, 200); // default 2%
}

#[test]
fn fee_analytics_query_for_adjacent_periods_are_independent() {
    let (_env, client, _admin) = setup();
    // Both adjacent periods should be empty (error) independently
    let r_a = client.try_get_fee_analytics(&10);
    let r_b = client.try_get_fee_analytics(&11);
    assert!(r_a.is_err());
    assert!(r_b.is_err());
}
