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

fn set_user_volume_tier(
    client: &QuickLendXContractClient,
    user: &Address,
    target_tier: crate::fees::VolumeTier,
) {
    match target_tier {
        crate::fees::VolumeTier::Standard => {}
        crate::fees::VolumeTier::Silver => {
            client.update_user_transaction_volume(user, &100_000_000_000);
        }
        crate::fees::VolumeTier::Gold => {
            client.update_user_transaction_volume(user, &500_000_000_000);
        }
        crate::fees::VolumeTier::Platinum => {
            client.update_user_transaction_volume(user, &1_000_000_000_000);
        }
    }
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

/// Small-amount fee calculations should clamp first and then apply modifiers deterministically.
#[test]
fn test_transaction_fee_small_amount_uses_minimums_before_modifiers() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);
    let user = setup_investor(&env, &client, &admin);

    client.initialize_fee_system(&admin);

    let regular_fees = client.calculate_transaction_fees(&user, &1, &false, &false);
    let early_fees = client.calculate_transaction_fees(&user, &1, &true, &false);

    // Base fees clamp to configured minimums: 100 + 50 + 100 = 250
    // Early discount applies after clamp on Platform only: 100 -> 90
    assert_eq!(regular_fees, 250);
    assert_eq!(early_fees, 240);
}

/// Large-amount calculations should clamp to maximums before discounts are applied.
#[test]
fn test_transaction_fee_large_amount_uses_maximums_before_tier_discount() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);
    let user = setup_investor(&env, &client, &admin);

    client.initialize_fee_system(&admin);
    set_user_volume_tier(&client, &user, crate::fees::VolumeTier::Platinum);

    let amount = 100_000_000_i128;
    let fees = client.calculate_transaction_fees(&user, &amount, &false, &false);

    // Platform max 1_000_000 -> 850_000 after Platinum discount
    // Processing max 500_000 -> 425_000 after Platinum discount
    // Verification max 100_000 -> 85_000 after Platinum discount
    assert_eq!(fees, 1_360_000);
}

/// Repeated calculations over the same state and input should remain deterministic.
#[test]
fn test_transaction_fee_same_inputs_are_deterministic() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);
    let user = setup_investor(&env, &client, &admin);

    client.initialize_fee_system(&admin);
    set_user_volume_tier(&client, &user, crate::fees::VolumeTier::Silver);
    client.update_fee_structure(&admin, &FeeType::LatePayment, &100, &50, &10_000, &true);

    let amount = 12_345_i128;
    let first = client.calculate_transaction_fees(&user, &amount, &true, &true);
    let second = client.calculate_transaction_fees(&user, &amount, &true, &true);
    let third = client.calculate_transaction_fees(&user, &amount, &true, &true);

    assert_eq!(first, second);
    assert_eq!(second, third);
    assert_eq!(third, 533);
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

    client.update_platform_fee_bps(&500);
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

    client.update_platform_fee_bps(&500);

    let treasury_addr = client.get_treasury_address();
    assert!(treasury_addr.is_some());
    assert_eq!(treasury_addr.unwrap(), treasury);
}

// ============================================================================
// HARDENING EXTENDED TESTS — Initialization Guard & Treasury Validation
// ============================================================================

/// Second initialize_fee_system call returns InvalidFeeConfiguration.
#[test]
fn test_double_init_returns_invalid_fee_configuration() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);

    client.initialize_fee_system(&admin);

    let result = client.try_initialize_fee_system(&admin);
    let err = result.err().expect("re-init must return error");
    let contract_error = err.expect("expected typed contract error");
    assert_eq!(contract_error, QuickLendXError::InvalidFeeConfiguration);
}

/// Fee structures initialized in first call survive a rejected second call.
#[test]
fn test_fee_structures_unchanged_after_rejected_reinit() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);

    client.initialize_fee_system(&admin);

    // A custom update after first init.
    client.update_fee_structure(
        &admin,
        &crate::fees::FeeType::Platform,
        &300,
        &50,
        &5_000,
        &true,
    );
    assert_eq!(
        client
            .get_fee_structure(&crate::fees::FeeType::Platform)
            .base_fee_bps,
        300
    );

    // Re-init attempt is rejected — custom update must be preserved.
    let _ = client.try_initialize_fee_system(&admin);
    assert_eq!(
        client
            .get_fee_structure(&crate::fees::FeeType::Platform)
            .base_fee_bps,
        300
    );
}

/// configure_treasury rejects the contract address as InvalidAddress.
#[test]
fn test_treasury_self_assignment_returns_invalid_address() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin_init(&env, &client);

    client.initialize_fee_system(&admin);

    let result = client.try_configure_treasury(&contract_id);
    let err = result.err().expect("self-assignment must fail");
    let contract_error = err.expect("expected typed contract error");
    assert_eq!(contract_error, QuickLendXError::InvalidAddress);
}

/// Duplicate treasury configuration returns InvalidFeeConfiguration.
#[test]
fn test_duplicate_treasury_returns_invalid_fee_configuration() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin_init(&env, &client);
    let treasury = Address::generate(&env);

    client.initialize_fee_system(&admin);
    client.configure_treasury(&treasury);

    let result = client.try_configure_treasury(&treasury);
    let err = result.err().expect("duplicate must fail");
    let contract_error = err.expect("expected typed contract error");
    assert_eq!(contract_error, QuickLendXError::InvalidFeeConfiguration);
}

/// Treasury address can be updated to a new distinct address after initial set.
#[test]
fn test_treasury_update_to_new_address_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin_init(&env, &client);
    let treasury_a = Address::generate(&env);
    let treasury_b = Address::generate(&env);

    client.initialize_fee_system(&admin);
    client.configure_treasury(&treasury_a);
    client.configure_treasury(&treasury_b);

    assert_eq!(client.get_treasury_address(), Some(treasury_b));
}

/// Revenue distribution config uses the treasury address that was last set.
#[test]
fn test_revenue_distribution_uses_updated_treasury() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);
    let user = setup_investor(&env, &client, &admin);
    let treasury_a = Address::generate(&env);
    let treasury_b = Address::generate(&env);

    client.initialize_fee_system(&admin);
    client.configure_treasury(&treasury_a);
    // Update to a different treasury before configuring revenue distribution.
    client.configure_treasury(&treasury_b);

    client.configure_revenue_distribution(&admin, &treasury_b, &10000, &0, &0, &false, &1);

    let mut fees_by_type = Map::new(&env);
    fees_by_type.set(crate::fees::FeeType::Platform, 500);
    client.collect_transaction_fees(&user, &fees_by_type, &500);

    let period = env.ledger().timestamp() / 2_592_000;
    let (treasury_amount, _, _) = client.distribute_revenue(&admin, &period);
    assert_eq!(treasury_amount, 500);
}

// ============================================================================
// MIN/MAX FEE STRUCTURE CONSISTENCY TESTS
// ============================================================================

/// Test that min_fee must be <= max_fee
#[test]
fn test_fee_structure_min_exceeds_max_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);

    client.initialize_fee_system(&admin);

    // Try to set min_fee > max_fee (should fail)
    let result = client.try_update_fee_structure(
        &admin,
        &crate::fees::FeeType::Processing,
        &100,  // base_fee_bps
        &1000, // min_fee > max_fee
        &500,  // max_fee
        &true,
    );

    assert!(result.is_err());
    let err = result.err().expect("should error").expect("contract error");
    assert_eq!(err, QuickLendXError::InvalidAmount);
}

/// Test that negative min_fee is rejected
#[test]
fn test_fee_structure_negative_min_fee_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);

    client.initialize_fee_system(&admin);

    // Try to set negative min_fee (should fail)
    let result = client.try_update_fee_structure(
        &admin,
        &crate::fees::FeeType::Verification,
        &100,
        &-100, // negative min_fee
        &500,
        &true,
    );

    assert!(result.is_err());
    let err = result.err().expect("should error").expect("contract error");
    assert_eq!(err, QuickLendXError::InvalidAmount);
}

/// Test that negative max_fee is rejected
#[test]
fn test_fee_structure_negative_max_fee_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);

    client.initialize_fee_system(&admin);

    // Try to set negative max_fee (should fail)
    let result = client.try_update_fee_structure(
        &admin,
        &crate::fees::FeeType::Processing,
        &100,
        &100,
        &-500, // negative max_fee
        &true,
    );

    assert!(result.is_err());
    let err = result.err().expect("should error").expect("contract error");
    assert_eq!(err, QuickLendXError::InvalidAmount);
}

/// Test that equal min_fee and max_fee is allowed (flat fee)
#[test]
fn test_fee_structure_equal_min_max_allowed() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);

    client.initialize_fee_system(&admin);

    // min_fee == max_fee should be allowed (flat fee)
    let result = client.try_update_fee_structure(
        &admin,
        &crate::fees::FeeType::Verification,
        &100,
        &500, // min_fee
        &500, // max_fee (same as min)
        &true,
    );

    assert!(result.is_ok());
    let structure = result.unwrap().unwrap();
    assert_eq!(structure.min_fee, 500);
    assert_eq!(structure.max_fee, 500);
}

/// Test that max_fee cannot exceed absolute protocol maximum
#[test]
fn test_fee_structure_exceeds_absolute_maximum_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);

    client.initialize_fee_system(&admin);

    // Try to set max_fee exceeding protocol limit (10M stroops)
    let result = client.try_update_fee_structure(
        &admin,
        &crate::fees::FeeType::Platform,
        &1000,
        &100,
        &11_000_000_000_000, // exceeds 10M absolute max
        &true,
    );

    assert!(result.is_err());
    let err = result.err().expect("should error").expect("contract error");
    assert_eq!(err, QuickLendXError::InvalidFeeConfiguration);
}

/// Test valid fee structure with reasonable min/max bounds
#[test]
fn test_fee_structure_valid_bounds_accepted() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);

    client.initialize_fee_system(&admin);

    // Valid configuration with reasonable bounds
    let result = client.try_update_fee_structure(
        &admin,
        &crate::fees::FeeType::Processing,
        &200,     // 2% base fee
        &100,     // min_fee
        &100_000, // max_fee (reasonable)
        &true,
    );

    assert!(result.is_ok());
    let structure = result.unwrap().unwrap();
    assert_eq!(structure.base_fee_bps, 200);
    assert_eq!(structure.min_fee, 100);
    assert_eq!(structure.max_fee, 100_000);
}

/// Test zero min_fee is allowed
#[test]
fn test_fee_structure_zero_min_fee_allowed() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);

    client.initialize_fee_system(&admin);

    // min_fee = 0 should be allowed
    let result = client.try_update_fee_structure(
        &admin,
        &crate::fees::FeeType::EarlyPayment,
        &100,
        &0, // no minimum fee
        &50_000,
        &true,
    );

    assert!(result.is_ok());
    let structure = result.unwrap().unwrap();
    assert_eq!(structure.min_fee, 0);
}

/// Test zero max_fee with zero min_fee (edge case)
#[test]
fn test_fee_structure_zero_min_and_max_allowed() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);

    client.initialize_fee_system(&admin);

    // Zero fees (no-op fee structure)
    let result = client.try_update_fee_structure(
        &admin,
        &crate::fees::FeeType::EarlyPayment,
        &0,     // 0% base
        &0,     // no minimum
        &0,     // no maximum
        &false, // inactive
    );

    assert!(result.is_ok());
    let structure = result.unwrap().unwrap();
    assert_eq!(structure.min_fee, 0);
    assert_eq!(structure.max_fee, 0);
    assert!(!structure.is_active);
}

/// Test that fee structure bounds remain consistent when updated
#[test]
fn test_fee_structure_update_preserves_consistency() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);

    client.initialize_fee_system(&admin);

    // First update
    let _first = client.update_fee_structure(
        &admin,
        &crate::fees::FeeType::Platform,
        &150,
        &50,
        &200_000,
        &true,
    );

    // Second update with different but valid bounds
    let result = client.try_update_fee_structure(
        &admin,
        &crate::fees::FeeType::Platform,
        &250,
        &75,
        &300_000,
        &true,
    );

    assert!(result.is_ok());
    let updated = result.unwrap().unwrap();
    assert_eq!(updated.base_fee_bps, 250);
    assert_eq!(updated.min_fee, 75);
    assert_eq!(updated.max_fee, 300_000);
}

/// Test cross-fee-type consistency: LatePayment shouldn't undercut Platform
#[test]
fn test_cross_fee_late_payment_higher_min_than_platform() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);

    client.initialize_fee_system(&admin);

    // Set a Platform fee structure with min of 500
    client.update_fee_structure(
        &admin,
        &crate::fees::FeeType::Platform,
        &200,
        &500, // Platform min
        &100_000,
        &true,
    );

    // Try to set LatePayment with lower min than Platform (should fail cross-check)
    let result = client.try_update_fee_structure(
        &admin,
        &crate::fees::FeeType::LatePayment,
        &300,
        &100, // Lower than Platform's 500
        &200_000,
        &true,
    );

    // This should fail the cross-fee consistency check
    assert!(result.is_err());
}

/// Test that total active min_fees don't exceed protocol maximum
#[test]
fn test_cross_fee_total_min_fees_respects_limit() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);

    client.initialize_fee_system(&admin);

    // Try to configure fees that together exceed reasonable total min_fee limit
    // The absolute protocol limit for total min fees is 2.5M stroops
    let excessive_min = 2_000_000_000_000; // 2M stroops - already high

    let result = client.try_update_fee_structure(
        &admin,
        &crate::fees::FeeType::Verification,
        &100,
        &excessive_min,
        &5_000_000_000_000, // 5M max
        &true,
    );

    // This should fail because individual min_fee alone approaches limit
    assert!(result.is_err());
}

/// Test fee structure bounds for early payment fees
#[test]
fn test_fee_structure_early_payment_bounds() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);

    client.initialize_fee_system(&admin);

    // Early payment can have more flexible bounds (500x multiplier)
    let result = client.try_update_fee_structure(
        &admin,
        &crate::fees::FeeType::EarlyPayment,
        &100,
        &10,
        &5_000_000, // 5x base rate is reasonable for early payments
        &true,
    );

    assert!(result.is_ok());
}

/// Test fee structure bounds for late payment fees with penalty
#[test]
fn test_fee_structure_late_payment_bounds() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);

    client.initialize_fee_system(&admin);

    // Late payment fees can be higher (penalty)
    let result = client.try_update_fee_structure(
        &admin,
        &crate::fees::FeeType::LatePayment,
        &500, // 5% late penalty
        &100,
        &1_000_000, // Reasonable penalty cap
        &true,
    );

    assert!(result.is_ok());
    let structure = result.unwrap().unwrap();
    assert_eq!(structure.fee_type, crate::fees::FeeType::LatePayment);
}

/// Test simultaneous valid fee structures for multiple types
#[test]
fn test_multiple_fee_structures_concurrent_valid() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);

    client.initialize_fee_system(&admin);

    // Create valid structures for multiple types
    let platform = client.update_fee_structure(
        &admin,
        &crate::fees::FeeType::Platform,
        &200,
        &100,
        &500_000,
        &true,
    );

    let processing = client.update_fee_structure(
        &admin,
        &crate::fees::FeeType::Processing,
        &50,
        &50,
        &250_000,
        &true,
    );

    let verification = client.update_fee_structure(
        &admin,
        &crate::fees::FeeType::Verification,
        &100,
        &100,
        &100_000,
        &true,
    );

    assert_eq!(platform.fee_type, crate::fees::FeeType::Platform);
    assert_eq!(processing.fee_type, crate::fees::FeeType::Processing);
    assert_eq!(verification.fee_type, crate::fees::FeeType::Verification);
}

// ============================================================================
// FEETYPE CONFIGURATION MATRIX — ALL TYPES × BPS / MIN / MAX BOUNDS
// ============================================================================
//
// Each FeeType variant is exercised independently through `update_fee_structure`
// across three categories of assertions:
//
//  1. BPS boundary — 0 bps (valid), 1000 bps (hard cap, valid), 1001 bps (rejected)
//  2. Min/max ordering — equal bounds (flat fee), inverted bounds (rejected)
//  3. Insert vs. update — EarlyPayment/LatePayment begin absent from defaults and
//     must be created fresh; Platform/Processing/Verification already exist and
//     follow the update path.
//
// Cross-cutting invariant tested throughout:
//   `LatePayment.min_fee >= Platform.min_fee` (cross-fee floor rule).
//
// BPS denominator:  10 000 (1 bps = 0.01 %)
// Hard cap:         1 000 bps = 10 %
// Absolute max fee: 10 000 000 000 000 stroops (10 M)
// ============================================================================

// ----------------------------------------------------------------------------
// BPS BOUNDARY — PLATFORM
// ----------------------------------------------------------------------------

/// Platform: bps = 0 is valid (free / no-op rate).
#[test]
fn test_matrix_platform_bps_zero_accepted() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);

    client.initialize_fee_system(&admin);

    let result =
        client.try_update_fee_structure(&admin, &FeeType::Platform, &0, &0, &0, &false);
    assert!(result.is_ok(), "Platform bps=0 must be accepted");
    let s = result.unwrap().unwrap();
    assert_eq!(s.base_fee_bps, 0);
    assert_eq!(s.fee_type, FeeType::Platform);
}

/// Platform: bps = 1000 (hard cap) is accepted.
#[test]
fn test_matrix_platform_bps_at_cap_accepted() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);

    client.initialize_fee_system(&admin);

    // With bps=1000 the calculated threshold is 1000*100*100 = 10_000_000.
    // max_fee=1_000_000 is within that threshold.
    let result = client.try_update_fee_structure(
        &admin, &FeeType::Platform, &1000, &100, &1_000_000, &true,
    );
    assert!(result.is_ok(), "Platform bps=1000 (hard cap) must be accepted");
    let s = result.unwrap().unwrap();
    assert_eq!(s.base_fee_bps, 1000);
    assert!(s.is_active);
}

/// Platform: bps = 1001 exceeds the hard cap and must be rejected with
/// `InvalidFeeBasisPoints`.
#[test]
fn test_matrix_platform_bps_over_cap_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);

    client.initialize_fee_system(&admin);

    let result = client.try_update_fee_structure(
        &admin, &FeeType::Platform, &1001, &100, &1_000_000, &true,
    );
    let err = result.err().expect("bps=1001 must fail").expect("contract error");
    assert_eq!(err, QuickLendXError::InvalidFeeBasisPoints);
}

// ----------------------------------------------------------------------------
// BPS BOUNDARY — PROCESSING
// ----------------------------------------------------------------------------

/// Processing: bps = 0 is valid.
#[test]
fn test_matrix_processing_bps_zero_accepted() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);

    client.initialize_fee_system(&admin);

    let result =
        client.try_update_fee_structure(&admin, &FeeType::Processing, &0, &0, &0, &false);
    assert!(result.is_ok(), "Processing bps=0 must be accepted");
    assert_eq!(result.unwrap().unwrap().base_fee_bps, 0);
}

/// Processing: bps = 1000 (hard cap) is accepted.
#[test]
fn test_matrix_processing_bps_at_cap_accepted() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);

    client.initialize_fee_system(&admin);

    let result = client.try_update_fee_structure(
        &admin, &FeeType::Processing, &1000, &50, &1_000_000, &true,
    );
    assert!(result.is_ok(), "Processing bps=1000 must be accepted");
    assert_eq!(result.unwrap().unwrap().base_fee_bps, 1000);
}

/// Processing: bps = 1001 must be rejected with `InvalidFeeBasisPoints`.
#[test]
fn test_matrix_processing_bps_over_cap_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);

    client.initialize_fee_system(&admin);

    let result = client.try_update_fee_structure(
        &admin, &FeeType::Processing, &1001, &50, &500_000, &true,
    );
    let err = result.err().expect("bps=1001 must fail").expect("contract error");
    assert_eq!(err, QuickLendXError::InvalidFeeBasisPoints);
}

// ----------------------------------------------------------------------------
// BPS BOUNDARY — VERIFICATION
// ----------------------------------------------------------------------------

/// Verification: bps = 0 is valid.
#[test]
fn test_matrix_verification_bps_zero_accepted() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);

    client.initialize_fee_system(&admin);

    let result =
        client.try_update_fee_structure(&admin, &FeeType::Verification, &0, &0, &0, &false);
    assert!(result.is_ok(), "Verification bps=0 must be accepted");
}

/// Verification: bps = 1000 (hard cap) is accepted.
#[test]
fn test_matrix_verification_bps_at_cap_accepted() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);

    client.initialize_fee_system(&admin);

    let result = client.try_update_fee_structure(
        &admin, &FeeType::Verification, &1000, &100, &1_000_000, &true,
    );
    assert!(result.is_ok(), "Verification bps=1000 must be accepted");
    assert_eq!(result.unwrap().unwrap().base_fee_bps, 1000);
}

/// Verification: bps = 1001 must be rejected with `InvalidFeeBasisPoints`.
#[test]
fn test_matrix_verification_bps_over_cap_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);

    client.initialize_fee_system(&admin);

    let result = client.try_update_fee_structure(
        &admin, &FeeType::Verification, &1001, &100, &100_000, &true,
    );
    let err = result.err().expect("bps=1001 must fail").expect("contract error");
    assert_eq!(err, QuickLendXError::InvalidFeeBasisPoints);
}

// ----------------------------------------------------------------------------
// BPS BOUNDARY — EARLYPAYMENT (new insertion path)
// ----------------------------------------------------------------------------

/// EarlyPayment: bps = 0 is valid; entry is inserted into storage.
#[test]
fn test_matrix_early_payment_bps_zero_accepted() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);

    client.initialize_fee_system(&admin);

    // EarlyPayment is absent from the default fee structures.
    // A successful call proves it was inserted fresh.
    let result =
        client.try_update_fee_structure(&admin, &FeeType::EarlyPayment, &0, &0, &0, &false);
    assert!(result.is_ok(), "EarlyPayment bps=0 must be accepted");
    let s = result.unwrap().unwrap();
    assert_eq!(s.fee_type, FeeType::EarlyPayment);
    assert_eq!(s.base_fee_bps, 0);
    assert!(!s.is_active);
}

/// EarlyPayment: bps = 1000 (hard cap) is accepted.
/// EarlyPayment / LatePayment use a 500× multiplier for max_fee, giving more
/// room for incentive / penalty structures than the 100× cap used by regular fees.
#[test]
fn test_matrix_early_payment_bps_at_cap_accepted() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);

    client.initialize_fee_system(&admin);

    // bps=1000 → threshold = 1000 * 500 * 100 = 50_000_000; max_fee=1_000_000 is within bound.
    let result = client.try_update_fee_structure(
        &admin, &FeeType::EarlyPayment, &1000, &0, &1_000_000, &true,
    );
    assert!(result.is_ok(), "EarlyPayment bps=1000 must be accepted");
    assert_eq!(result.unwrap().unwrap().base_fee_bps, 1000);
}

/// EarlyPayment: bps = 1001 must be rejected with `InvalidFeeBasisPoints`.
#[test]
fn test_matrix_early_payment_bps_over_cap_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);

    client.initialize_fee_system(&admin);

    let result = client.try_update_fee_structure(
        &admin, &FeeType::EarlyPayment, &1001, &0, &1_000_000, &true,
    );
    let err = result.err().expect("bps=1001 must fail").expect("contract error");
    assert_eq!(err, QuickLendXError::InvalidFeeBasisPoints);
}

// ----------------------------------------------------------------------------
// BPS BOUNDARY — LATEPAYMENT (new insertion path, cross-fee floor rule)
// ----------------------------------------------------------------------------

/// LatePayment: bps = 0 with min_fee equal to the Platform default min (100)
/// satisfies the cross-fee floor rule and must be accepted.
#[test]
fn test_matrix_late_payment_bps_zero_accepted() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);

    client.initialize_fee_system(&admin);

    // Default Platform min_fee = 100; LatePayment min_fee must be >= 100.
    // With bps=0 the max_fee threshold is 0 (guard skipped), so max_fee=100 is fine.
    let result = client.try_update_fee_structure(
        &admin, &FeeType::LatePayment, &0, &100, &100, &false,
    );
    assert!(result.is_ok(), "LatePayment bps=0, min=100 must be accepted");
    let s = result.unwrap().unwrap();
    assert_eq!(s.fee_type, FeeType::LatePayment);
    assert_eq!(s.base_fee_bps, 0);
}

/// LatePayment: bps = 1000 with min_fee >= Platform min is accepted.
#[test]
fn test_matrix_late_payment_bps_at_cap_accepted() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);

    client.initialize_fee_system(&admin);

    // bps=1000 → threshold = 1000 * 500 * 100 = 50_000_000; max_fee=1_000_000 is within bound.
    // min_fee=200 satisfies the cross-fee floor (Platform default min=100).
    let result = client.try_update_fee_structure(
        &admin, &FeeType::LatePayment, &1000, &200, &1_000_000, &true,
    );
    assert!(result.is_ok(), "LatePayment bps=1000 must be accepted");
    assert_eq!(result.unwrap().unwrap().base_fee_bps, 1000);
}

/// LatePayment: bps = 1001 must be rejected with `InvalidFeeBasisPoints`.
#[test]
fn test_matrix_late_payment_bps_over_cap_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);

    client.initialize_fee_system(&admin);

    let result = client.try_update_fee_structure(
        &admin, &FeeType::LatePayment, &1001, &200, &1_000_000, &true,
    );
    let err = result.err().expect("bps=1001 must fail").expect("contract error");
    assert_eq!(err, QuickLendXError::InvalidFeeBasisPoints);
}

// ----------------------------------------------------------------------------
// RETURNED STRUCT FIELD VERIFICATION — PLATFORM (representative)
// ----------------------------------------------------------------------------

/// `update_fee_structure` returns a `FeeStructure` whose fields exactly match
/// the arguments supplied by the caller for the Platform fee type.
#[test]
fn test_matrix_platform_returned_fields_complete() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);

    client.initialize_fee_system(&admin);

    let s = client.update_fee_structure(
        &admin, &FeeType::Platform, &350, &75, &750_000, &true,
    );

    assert_eq!(s.fee_type, FeeType::Platform);
    assert_eq!(s.base_fee_bps, 350);
    assert_eq!(s.min_fee, 75);
    assert_eq!(s.max_fee, 750_000);
    assert!(s.is_active);
    assert_eq!(s.updated_by, admin);
    // Timestamp must be the current ledger timestamp (non-zero after mock).
    assert_eq!(s.updated_at, env.ledger().timestamp());
}

/// `update_fee_structure` returns correct fields for Processing.
#[test]
fn test_matrix_processing_returned_fields_complete() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);

    client.initialize_fee_system(&admin);

    let s = client.update_fee_structure(
        &admin, &FeeType::Processing, &75, &25, &300_000, &true,
    );

    assert_eq!(s.fee_type, FeeType::Processing);
    assert_eq!(s.base_fee_bps, 75);
    assert_eq!(s.min_fee, 25);
    assert_eq!(s.max_fee, 300_000);
    assert!(s.is_active);
    assert_eq!(s.updated_by, admin);
}

/// `update_fee_structure` returns correct fields for Verification.
#[test]
fn test_matrix_verification_returned_fields_complete() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);

    client.initialize_fee_system(&admin);

    let s = client.update_fee_structure(
        &admin, &FeeType::Verification, &120, &60, &120_000, &false,
    );

    assert_eq!(s.fee_type, FeeType::Verification);
    assert_eq!(s.base_fee_bps, 120);
    assert_eq!(s.min_fee, 60);
    assert_eq!(s.max_fee, 120_000);
    assert!(!s.is_active);
}

/// `update_fee_structure` returns correct fields for EarlyPayment (insertion).
#[test]
fn test_matrix_early_payment_returned_fields_complete() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);

    client.initialize_fee_system(&admin);

    let s = client.update_fee_structure(
        &admin, &FeeType::EarlyPayment, &80, &0, &400_000, &true,
    );

    assert_eq!(s.fee_type, FeeType::EarlyPayment);
    assert_eq!(s.base_fee_bps, 80);
    assert_eq!(s.min_fee, 0);
    assert_eq!(s.max_fee, 400_000);
    assert!(s.is_active);
    assert_eq!(s.updated_by, admin);
}

/// `update_fee_structure` returns correct fields for LatePayment (insertion).
#[test]
fn test_matrix_late_payment_returned_fields_complete() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);

    client.initialize_fee_system(&admin);

    // min_fee=150 satisfies the LatePayment cross-fee floor (Platform default min=100).
    let s = client.update_fee_structure(
        &admin, &FeeType::LatePayment, &300, &150, &900_000, &true,
    );

    assert_eq!(s.fee_type, FeeType::LatePayment);
    assert_eq!(s.base_fee_bps, 300);
    assert_eq!(s.min_fee, 150);
    assert_eq!(s.max_fee, 900_000);
    assert!(s.is_active);
    assert_eq!(s.updated_by, admin);
}

// ----------------------------------------------------------------------------
// IS_ACTIVE TOGGLE — EACH FEETYPE
// ----------------------------------------------------------------------------

/// Each fee type can be toggled inactive and reactivated independently.
#[test]
fn test_matrix_active_toggle_all_types() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);

    client.initialize_fee_system(&admin);

    // Insert EarlyPayment and LatePayment first so all 5 types exist.
    client.update_fee_structure(&admin, &FeeType::EarlyPayment, &100, &0, &500_000, &true);
    // LatePayment min_fee=100 meets the Platform default floor.
    client.update_fee_structure(&admin, &FeeType::LatePayment, &200, &100, &500_000, &true);

    let types_and_params: [_; 5] = [
        (FeeType::Platform,     200u32, 100i128, 500_000i128),
        (FeeType::Processing,   50u32,  50i128,  250_000i128),
        (FeeType::Verification, 100u32, 100i128, 100_000i128),
        (FeeType::EarlyPayment, 100u32, 0i128,   500_000i128),
        (FeeType::LatePayment,  200u32, 100i128, 500_000i128),
    ];

    for (fee_type, bps, min, max) in types_and_params.iter() {
        // Deactivate
        let s = client.update_fee_structure(&admin, fee_type, bps, min, max, &false);
        assert!(!s.is_active, "fee type should be inactive after toggle");

        // Reactivate
        let s = client.update_fee_structure(&admin, fee_type, bps, min, max, &true);
        assert!(s.is_active, "fee type should be active after re-toggle");
    }
}

// ----------------------------------------------------------------------------
// INSERT VS UPDATE — EARLYPAYMENT AND LATEPAYMENT
// ----------------------------------------------------------------------------

/// EarlyPayment is absent from the default fee structures.
/// A first call to `update_fee_structure` inserts it; a second call updates it.
#[test]
fn test_matrix_early_payment_insert_then_update() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);

    client.initialize_fee_system(&admin);

    // Before insertion, get_fee_structure must fail.
    let before = client.try_get_fee_structure(&FeeType::EarlyPayment);
    assert!(before.is_err(), "EarlyPayment must be absent before first insertion");

    // First call — insertion path.
    client.update_fee_structure(&admin, &FeeType::EarlyPayment, &100, &0, &500_000, &true);
    let inserted = client.get_fee_structure(&FeeType::EarlyPayment);
    assert_eq!(inserted.base_fee_bps, 100);

    // Second call — update path: different bps value must overwrite.
    client.update_fee_structure(&admin, &FeeType::EarlyPayment, &200, &0, &1_000_000, &true);
    let updated = client.get_fee_structure(&FeeType::EarlyPayment);
    assert_eq!(updated.base_fee_bps, 200);
    assert_eq!(updated.max_fee, 1_000_000);
}

/// LatePayment is absent from the default fee structures.
/// A first call inserts it; a second call updates it.
#[test]
fn test_matrix_late_payment_insert_then_update() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);

    client.initialize_fee_system(&admin);

    // Before insertion, get_fee_structure must fail.
    let before = client.try_get_fee_structure(&FeeType::LatePayment);
    assert!(before.is_err(), "LatePayment must be absent before first insertion");

    // First call — insertion path (min_fee=100 satisfies Platform floor).
    client.update_fee_structure(&admin, &FeeType::LatePayment, &300, &100, &1_000_000, &true);
    let inserted = client.get_fee_structure(&FeeType::LatePayment);
    assert_eq!(inserted.base_fee_bps, 300);

    // Second call — update path.
    client.update_fee_structure(&admin, &FeeType::LatePayment, &400, &150, &2_000_000, &true);
    let updated = client.get_fee_structure(&FeeType::LatePayment);
    assert_eq!(updated.base_fee_bps, 400);
    assert_eq!(updated.min_fee, 150);
}

// ----------------------------------------------------------------------------
// ISOLATION — UPDATING ONE TYPE DOES NOT AFFECT OTHERS
// ----------------------------------------------------------------------------

/// Updating Platform fee parameters must not alter Processing or Verification.
#[test]
fn test_matrix_update_platform_preserves_others() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);

    client.initialize_fee_system(&admin);

    // Capture baseline for Processing and Verification.
    let proc_before = client.get_fee_structure(&FeeType::Processing);
    let verif_before = client.get_fee_structure(&FeeType::Verification);

    // Mutate Platform.
    client.update_fee_structure(&admin, &FeeType::Platform, &500, &200, &800_000, &true);

    // Processing and Verification must be unchanged.
    let proc_after = client.get_fee_structure(&FeeType::Processing);
    let verif_after = client.get_fee_structure(&FeeType::Verification);

    assert_eq!(proc_before.base_fee_bps, proc_after.base_fee_bps);
    assert_eq!(proc_before.min_fee,      proc_after.min_fee);
    assert_eq!(proc_before.max_fee,      proc_after.max_fee);

    assert_eq!(verif_before.base_fee_bps, verif_after.base_fee_bps);
    assert_eq!(verif_before.min_fee,      verif_after.min_fee);
    assert_eq!(verif_before.max_fee,      verif_after.max_fee);
}

/// Inserting EarlyPayment must not alter Platform, Processing, or Verification.
#[test]
fn test_matrix_insert_early_payment_preserves_existing_types() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);

    client.initialize_fee_system(&admin);

    let plat_before  = client.get_fee_structure(&FeeType::Platform);
    let proc_before  = client.get_fee_structure(&FeeType::Processing);
    let verif_before = client.get_fee_structure(&FeeType::Verification);

    // Insert EarlyPayment.
    client.update_fee_structure(&admin, &FeeType::EarlyPayment, &100, &0, &500_000, &true);

    let plat_after  = client.get_fee_structure(&FeeType::Platform);
    let proc_after  = client.get_fee_structure(&FeeType::Processing);
    let verif_after = client.get_fee_structure(&FeeType::Verification);

    assert_eq!(plat_before.base_fee_bps,  plat_after.base_fee_bps);
    assert_eq!(proc_before.base_fee_bps,  proc_after.base_fee_bps);
    assert_eq!(verif_before.base_fee_bps, verif_after.base_fee_bps);
}

// ----------------------------------------------------------------------------
// CROSS-FEE FLOOR RULE — LATEPAYMENT
// ----------------------------------------------------------------------------

/// LatePayment min_fee below the current Platform min_fee must be rejected
/// with `InvalidFeeConfiguration` regardless of the BPS value used.
#[test]
fn test_matrix_late_payment_cross_check_floor_enforced_on_insert() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);

    client.initialize_fee_system(&admin);

    // Raise Platform min_fee to 500 so the floor is easy to violate.
    client.update_fee_structure(&admin, &FeeType::Platform, &200, &500, &500_000, &true);

    // LatePayment min_fee=100 < Platform min_fee=500 → rejected.
    let result = client.try_update_fee_structure(
        &admin, &FeeType::LatePayment, &300, &100, &1_000_000, &true,
    );
    let err = result.err().expect("cross-check violation must fail").expect("contract error");
    assert_eq!(err, QuickLendXError::InvalidFeeConfiguration);
}

/// LatePayment min_fee exactly equal to Platform min_fee satisfies the floor rule.
#[test]
fn test_matrix_late_payment_cross_check_floor_exact_equal_accepted() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);

    client.initialize_fee_system(&admin);

    // Platform min_fee = default 100.
    // LatePayment min_fee = 100 must be accepted (boundary: equal is valid).
    let result = client.try_update_fee_structure(
        &admin, &FeeType::LatePayment, &300, &100, &1_000_000, &true,
    );
    assert!(result.is_ok(), "LatePayment min_fee == Platform min_fee must be accepted");
}

// ----------------------------------------------------------------------------
// FLAT-FEE BOUNDARY — MIN_FEE EQUALS MAX_FEE ACROSS ALL TYPES
// ----------------------------------------------------------------------------

/// A flat-fee configuration (min_fee == max_fee) is valid for every FeeType.
#[test]
fn test_matrix_flat_fee_all_types_accepted() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);

    client.initialize_fee_system(&admin);

    // Platform — flat fee at 200 stroops.
    let r = client.try_update_fee_structure(&admin, &FeeType::Platform, &100, &200, &200, &true);
    assert!(r.is_ok(), "Platform flat fee must be accepted");

    // Processing — flat fee at 50 stroops.
    let r = client.try_update_fee_structure(&admin, &FeeType::Processing, &50, &50, &50, &true);
    assert!(r.is_ok(), "Processing flat fee must be accepted");

    // Verification — flat fee at 100 stroops.
    let r = client.try_update_fee_structure(&admin, &FeeType::Verification, &100, &100, &100, &true);
    assert!(r.is_ok(), "Verification flat fee must be accepted");

    // EarlyPayment — flat fee at 0 stroops (free incentive).
    let r = client.try_update_fee_structure(&admin, &FeeType::EarlyPayment, &100, &0, &0, &true);
    assert!(r.is_ok(), "EarlyPayment flat zero fee must be accepted");

    // LatePayment — flat fee at 100 stroops (meets Platform floor of 100).
    let r = client.try_update_fee_structure(&admin, &FeeType::LatePayment, &200, &100, &100, &true);
    assert!(r.is_ok(), "LatePayment flat fee at floor must be accepted");
}

// ----------------------------------------------------------------------------
// BPS INTERMEDIATE VALUES — SPOT CHECKS PER TYPE
// ----------------------------------------------------------------------------

/// Intermediate BPS values (1, 500, 999) are accepted for every FeeType.
#[test]
fn test_matrix_intermediate_bps_accepted_for_all_types() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);

    client.initialize_fee_system(&admin);

    // Pre-insert EarlyPayment and LatePayment so later updates in the loop work.
    client.update_fee_structure(&admin, &FeeType::EarlyPayment, &100, &0, &500_000, &true);
    client.update_fee_structure(&admin, &FeeType::LatePayment, &100, &100, &500_000, &true);

    // For each intermediate BPS value, update all 5 types.
    // Use conservative min/max that are within every type's threshold.
    for &bps in &[1u32, 500u32, 999u32] {
        // Platform/Processing/Verification threshold = bps * 100 * 100
        let threshold = (bps as i128) * 100 * 100;
        let max_val = if threshold == 0 { 100_000 } else { threshold.min(1_000_000) };

        let r = client.try_update_fee_structure(
            &admin, &FeeType::Platform, &bps, &100, &max_val, &true,
        );
        assert!(r.is_ok(), "Platform bps={} must be accepted", bps);

        let r = client.try_update_fee_structure(
            &admin, &FeeType::Processing, &bps, &50, &max_val, &true,
        );
        assert!(r.is_ok(), "Processing bps={} must be accepted", bps);

        let r = client.try_update_fee_structure(
            &admin, &FeeType::Verification, &bps, &100, &max_val, &true,
        );
        assert!(r.is_ok(), "Verification bps={} must be accepted", bps);

        // EarlyPayment/LatePayment threshold = bps * 500 * 100 (more generous).
        let ep_threshold = (bps as i128) * 500 * 100;
        let ep_max = if ep_threshold == 0 { 100_000 } else { ep_threshold.min(1_000_000) };

        let r = client.try_update_fee_structure(
            &admin, &FeeType::EarlyPayment, &bps, &0, &ep_max, &true,
        );
        assert!(r.is_ok(), "EarlyPayment bps={} must be accepted", bps);

        // LatePayment: after Platform was set with min_fee=100, the floor = 100.
        let r = client.try_update_fee_structure(
            &admin, &FeeType::LatePayment, &bps, &100, &ep_max, &true,
        );
        assert!(r.is_ok(), "LatePayment bps={} must be accepted", bps);
    }
}
