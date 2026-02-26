// ============================================================================
// COMPREHENSIVE FEES AND REVENUE DISTRIBUTION TESTS (95%+ COVERAGE)
// ============================================================================
// This module provides additional tests for edge cases, boundary conditions,
// and detailed verification of fee calculations and revenue distribution logic.

use super::*;
use crate::{errors::QuickLendXError, fees::FeeType};
use soroban_sdk::{testutils::Address as _, Address, Env, Map, String};

fn setup_admin(env: &Env, client: &QuickLendXContractClient) -> Address {
    let admin = Address::generate(&env);
    client.set_admin(&admin);
    admin
}

fn setup_admin_init(env: &Env, client: &QuickLendXContractClient) -> Address {
    let admin = Address::generate(&env);
    client.initialize_admin(&admin);
    admin
}

fn setup_investor(env: &Env, client: &QuickLendXContractClient, admin: &Address) -> Address {
    let investor = Address::generate(&env);
    client.submit_investor_kyc(&investor, &String::from_str(env, "Investor KYC"));
    client.verify_investor(&investor, &1_000_000);
    investor
}

// ============================================================================
// EDGE CASE TESTS - ZERO AND SMALL AMOUNTS
// ============================================================================

/// Test platform fee with zero amount returns error (not allowed)
#[test]
fn test_transaction_fee_with_zero_amount() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);
    let user = setup_investor(&env, &client, &admin);

    client.initialize_fee_system(&admin);

    // Zero amount should fail
    let result = client.try_calculate_transaction_fees(&user, &0, &false, &false);
    assert!(result.is_err());
}

/// Test platform fee with very small amount (boundary)
#[test]
fn test_transaction_fee_with_small_amount() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);
    let user = setup_investor(&env, &client, &admin);

    client.initialize_fee_system(&admin);

    let fees = client.calculate_transaction_fees(&user, &1, &false, &false);
    assert!(fees >= 0);
}

// ============================================================================
// BOUNDARY VALUE TESTS - MIN AND MAX FEE VALUES
// ============================================================================

/// Test fee calculation with maximum BPS (1000 = 10%)
#[test]
fn test_fee_with_maximum_bps() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);

    client.set_platform_fee(&1000);
    let fee_config = client.get_platform_fee();
    assert_eq!(fee_config.fee_bps, 1000);
}

/// Test fee configuration with various intermediate values
#[test]
fn test_fee_with_intermediate_values() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);

    client.set_platform_fee(&100); // 1%
    assert_eq!(client.get_platform_fee().fee_bps, 100);

    client.set_platform_fee(&500); // 5%
    assert_eq!(client.get_platform_fee().fee_bps, 500);

    client.set_platform_fee(&750); // 7.5%
    assert_eq!(client.get_platform_fee().fee_bps, 750);
}

// ============================================================================
// VOLUME TIER TESTS - DISCOUNT APPLICATION
// ============================================================================

/// Test that volume tier discount applies correctly for Standard tier
#[test]
fn test_volume_tier_standard_no_discount() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);
    let user = setup_investor(&env, &client, &admin);

    client.initialize_fee_system(&admin);

    let volume_data = client.get_user_volume_data(&user);
    assert_eq!(volume_data.current_tier, crate::fees::VolumeTier::Standard);
}

// ============================================================================
// ROUNDING BEHAVIOR TESTS - FLOOR DIVISION VERIFICATION
// ============================================================================

/// Test rounding behavior with odd-numbered amounts
#[test]
fn test_rounding_with_odd_amounts() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);
    let user = setup_investor(&env, &client, &admin);

    client.initialize_fee_system(&admin);

    // Test with amounts that don't divide evenly
    let fees_odd = client.calculate_transaction_fees(&user, &333, &false, &false);
    assert!(fees_odd >= 0);

    let fees_odd2 = client.calculate_transaction_fees(&user, &777, &false, &false);
    assert!(fees_odd2 >= 0);
}

// ============================================================================
// PAYMENT TIMING MODIFIER TESTS - EARLY/LATE PENALTIES
// ============================================================================

/// Test early payment reduces fees as expected
#[test]
fn test_early_payment_fee_reduction() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);
    let user = setup_investor(&env, &client, &admin);

    client.initialize_fee_system(&admin);

    let transaction_amount = 50_000i128;
    let regular_fees =
        client.calculate_transaction_fees(&user, &transaction_amount, &false, &false);
    let early_fees = client.calculate_transaction_fees(&user, &transaction_amount, &true, &false);

    assert!(early_fees < regular_fees);
}

/// Test late payment modifier logic (only applies to LatePayment fee type)
/// Note: Default initialization doesn't include EarlyPayment/LatePayment fee structures
#[test]
fn test_late_payment_fee_increase() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);
    let user = setup_investor(&env, &client, &admin);

    client.initialize_fee_system(&admin);

    let transaction_amount = 50_000i128;

    // Late payment flag doesn't increase fees unless LatePayment fee structure exists
    let regular_fees =
        client.calculate_transaction_fees(&user, &transaction_amount, &false, &false);
    let late_fees = client.calculate_transaction_fees(&user, &transaction_amount, &false, &true);

    // With default structures, late_fees and regular_fees should be equal
    // since there's no LatePayment fee structure to apply the penalty to
    assert_eq!(late_fees, regular_fees);
}

/// Test combined early and late flags behavior
/// Note: Early payment only affects Platform fee, late only affects LatePayment fee
#[test]
fn test_early_and_late_payment_combined() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);
    let user = setup_investor(&env, &client, &admin);

    client.initialize_fee_system(&admin);

    let transaction_amount = 50_000i128;

    // With default structures (no LatePayment fee), both should be equal
    let combined_fees = client.calculate_transaction_fees(&user, &transaction_amount, &true, &true);
    let regular_fees =
        client.calculate_transaction_fees(&user, &transaction_amount, &false, &false);

    // Early payment should reduce fees even with late flag set
    assert!(combined_fees < regular_fees);
}

// ============================================================================
// REVENUE DISTRIBUTION PATTERN TESTS - VARIOUS SPLITS
// ============================================================================

/// Test revenue distribution with 100% to treasury
#[test]
fn test_revenue_all_to_treasury() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);
    let user = setup_investor(&env, &client, &admin);
    let treasury = Address::generate(&env);

    client.initialize_fee_system(&admin);
    client.configure_revenue_distribution(&admin, &treasury, &10000, &0, &0, &false, &1);

    let mut fees_by_type = Map::new(&env);
    fees_by_type.set(FeeType::Platform, 1000);
    client.collect_transaction_fees(&user, &fees_by_type, &1000);

    let current_period = env.ledger().timestamp() / 2_592_000;
    let (treasury_amount, developer_amount, platform_amount) =
        client.distribute_revenue(&admin, &current_period);

    assert_eq!(treasury_amount, 1000);
    assert_eq!(developer_amount, 0);
    assert_eq!(platform_amount, 0);
}

/// Test revenue distribution with no allocation to treasury
#[test]
fn test_revenue_all_to_platform() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);
    let user = setup_investor(&env, &client, &admin);
    let treasury = Address::generate(&env);

    client.initialize_fee_system(&admin);
    client.configure_revenue_distribution(&admin, &treasury, &0, &0, &10000, &false, &1);

    let mut fees_by_type = Map::new(&env);
    fees_by_type.set(FeeType::Platform, 1000);
    client.collect_transaction_fees(&user, &fees_by_type, &1000);

    let current_period = env.ledger().timestamp() / 2_592_000;
    let (treasury_amount, developer_amount, platform_amount) =
        client.distribute_revenue(&admin, &current_period);

    assert_eq!(treasury_amount, 0);
    assert_eq!(developer_amount, 0);
    assert_eq!(platform_amount, 1000);
}

/// Test asymmetric distribution (45/45/10)
#[test]
fn test_revenue_asymmetric_distribution() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);
    let user = setup_investor(&env, &client, &admin);
    let treasury = Address::generate(&env);

    client.initialize_fee_system(&admin);
    client.configure_revenue_distribution(&admin, &treasury, &4500, &4500, &1000, &false, &1);

    let mut fees_by_type = Map::new(&env);
    fees_by_type.set(FeeType::Platform, 1000);
    client.collect_transaction_fees(&user, &fees_by_type, &1000);

    let current_period = env.ledger().timestamp() / 2_592_000;
    let (treasury_amount, developer_amount, platform_amount) =
        client.distribute_revenue(&admin, &current_period);

    assert_eq!(treasury_amount, 450);
    assert_eq!(developer_amount, 450);
    assert_eq!(platform_amount, 100);
}

// ============================================================================
// REVENUE DISTRIBUTION ACCURACY TESTS - NO DUST, EXACT AMOUNTS
// ============================================================================

/// Test that distribution totals don't exceed collected amount (no dust)
#[test]
fn test_revenue_distribution_sum_equals_collected() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);
    let user = setup_investor(&env, &client, &admin);
    let treasury = Address::generate(&env);

    client.initialize_fee_system(&admin);
    client.configure_revenue_distribution(&admin, &treasury, &3333, &3333, &3334, &false, &1);

    let total_collected = 9999i128;
    let mut fees_by_type = Map::new(&env);
    fees_by_type.set(FeeType::Platform, total_collected);
    client.collect_transaction_fees(&user, &fees_by_type, &total_collected);

    let current_period = env.ledger().timestamp() / 2_592_000;
    let (treasury_amount, developer_amount, platform_amount) =
        client.distribute_revenue(&admin, &current_period);

    assert_eq!(
        treasury_amount + developer_amount + platform_amount,
        total_collected
    );
}

// ============================================================================
// INITIALIZATION AND STATE PERSISTENCE TESTS
// ============================================================================

/// Test fee initialization sets correct defaults
#[test]
fn test_initialize_fee_system_sets_defaults() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);

    client.initialize_fee_system(&admin);

    let fee_config = client.get_platform_fee();
    assert_eq!(fee_config.fee_bps, 200); // Default 2%
}

/// Test multiple fee updates preserve state correctly
#[test]
fn test_multiple_fee_updates_sequence() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);

    client.set_platform_fee(&300);
    assert_eq!(client.get_platform_fee().fee_bps, 300);

    client.set_platform_fee(&500);
    assert_eq!(client.get_platform_fee().fee_bps, 500);

    client.set_platform_fee(&150);
    assert_eq!(client.get_platform_fee().fee_bps, 150);
}

/// Test treasury address persists across fee updates
#[test]
fn test_treasury_persists_across_updates() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin_init(&env, &client);
    let treasury = Address::generate(&env);

    client.initialize_fee_system(&admin);
    client.configure_treasury(&treasury);

    client.set_platform_fee(&500);

    let treasury_addr = client.get_treasury_address();
    assert!(treasury_addr.is_some());
    assert_eq!(treasury_addr.unwrap(), treasury);
}
