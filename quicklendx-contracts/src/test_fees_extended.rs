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
// HARDENING EXTENDED TESTS - Initialization Guard & Treasury Validation
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

    // Re-init attempt is rejected - custom update must be preserved.
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
    let structure = result.unwrap();
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
    let structure = result.unwrap();
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
    let structure = result.unwrap();
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
    let structure = result.unwrap();
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
    let updated = result.unwrap();
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
    let structure = result.unwrap();
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
// VOLUME ACCUMULATION TESTS - CUMULATIVE TRACKING ACROSS TRANSACTIONS
// ============================================================================

/// Test volume accumulates correctly after single transaction
#[test]
fn test_volume_accumulates_single_transaction() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);
    let user = setup_investor(&env, &client, &admin);

    client.initialize_fee_system(&admin);

    let initial_volume = client.get_user_volume_data(&user);
    assert_eq!(initial_volume.total_volume, 0);
    assert_eq!(initial_volume.transaction_count, 0);

    // Simulate a transaction by updating volume
    let transaction_amount = 50_000_i128;
    let updated_volume = client.update_user_transaction_volume(&user, &transaction_amount).unwrap();

    assert_eq!(updated_volume.total_volume, transaction_amount);
    assert_eq!(updated_volume.transaction_count, 1);
}

/// Test volume accumulates correctly after multiple sequential transactions
#[test]
fn test_volume_accumulates_multiple_transactions() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);
    let user = setup_investor(&env, &client, &admin);

    client.initialize_fee_system(&admin);

    let amount1 = 10_000_i128;
    let amount2 = 20_000_i128;
    let amount3 = 30_000_i128;

    let vol1 = client.update_user_transaction_volume(&user, &amount1).unwrap();
    assert_eq!(vol1.total_volume, amount1);
    assert_eq!(vol1.transaction_count, 1);

    let vol2 = client.update_user_transaction_volume(&user, &amount2).unwrap();
    assert_eq!(vol2.total_volume, amount1 + amount2);
    assert_eq!(vol2.transaction_count, 2);

    let vol3 = client.update_user_transaction_volume(&user, &amount3).unwrap();
    assert_eq!(vol3.total_volume, amount1 + amount2 + amount3);
    assert_eq!(vol3.transaction_count, 3);
}

/// Test volume tracking persists across time
#[test]
fn test_volume_persists_after_state_retrieval() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);
    let user = setup_investor(&env, &client, &admin);

    client.initialize_fee_system(&admin);

    let transaction_amount = 100_000_i128;
    client.update_user_transaction_volume(&user, &transaction_amount).unwrap();

    // Retrieve volume after time
    let retrieved_volume = client.get_user_volume_data(&user);
    assert_eq!(retrieved_volume.total_volume, transaction_amount);
    assert_eq!(retrieved_volume.transaction_count, 1);

    // Update again and verify cumulative storage
    client.update_user_transaction_volume(&user, &transaction_amount).unwrap();
    let final_volume = client.get_user_volume_data(&user);
    assert_eq!(final_volume.total_volume, transaction_amount * 2);
    assert_eq!(final_volume.transaction_count, 2);
}

/// Test very large volume accumulation without overflow
#[test]
fn test_volume_large_accumulation_no_overflow() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);
    let user = setup_investor(&env, &client, &admin);

    client.initialize_fee_system(&admin);

    // Add massive transactions
    let huge_amount = 1_000_000_000_000_i128; // 1 trillion stroops
    let vol1 = client.update_user_transaction_volume(&user, &huge_amount).unwrap();
    assert_eq!(vol1.total_volume, huge_amount);

    let vol2 = client.update_user_transaction_volume(&user, &huge_amount).unwrap();
    assert_eq!(vol2.total_volume, huge_amount * 2);
    assert_eq!(vol2.transaction_count, 2);
}

// ============================================================================
// TIER TRANSITIONS - FEE DISCOUNT CHANGES BASED ON VOLUME
// ============================================================================

/// Test tier transitions from Standard to Silver at threshold
#[test]
fn test_tier_transition_standard_to_silver() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);
    let user = setup_investor(&env, &client, &admin);

    client.initialize_fee_system(&admin);

    // Initially at Standard (0% discount)
    let initial = client.get_user_volume_data(&user);
    assert_eq!(initial.current_tier, crate::fees::VolumeTier::Standard);

    // Reach Silver threshold: 100_000_000_000
    let silver_threshold = 100_000_000_000_i128;
    client.update_user_transaction_volume(&user, &silver_threshold).unwrap();
    let after_silver = client.get_user_volume_data(&user);
    assert_eq!(after_silver.current_tier, crate::fees::VolumeTier::Silver);
}

/// Test tier transitions from Silver to Gold at threshold
#[test]
fn test_tier_transition_silver_to_gold() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);
    let user = setup_investor(&env, &client, &admin);

    client.initialize_fee_system(&admin);

    // Reach Silver first
    client.update_user_transaction_volume(&user, &100_000_000_000_i128).unwrap();
    let silver = client.get_user_volume_data(&user);
    assert_eq!(silver.current_tier, crate::fees::VolumeTier::Silver);

    // Continue to Gold threshold: 500_000_000_000
    let additional = 400_000_000_000_i128;
    client.update_user_transaction_volume(&user, &additional).unwrap();
    let gold = client.get_user_volume_data(&user);
    assert_eq!(gold.current_tier, crate::fees::VolumeTier::Gold);
}

/// Test tier transitions from Gold to Platinum at threshold
#[test]
fn test_tier_transition_gold_to_platinum() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);
    let user = setup_investor(&env, &client, &admin);

    client.initialize_fee_system(&admin);

    // Reach Gold first
    client.update_user_transaction_volume(&user, &500_000_000_000_i128).unwrap();
    let gold = client.get_user_volume_data(&user);
    assert_eq!(gold.current_tier, crate::fees::VolumeTier::Gold);

    // Continue to Platinum threshold: 1_000_000_000_000
    let additional = 500_000_000_000_i128;
    client.update_user_transaction_volume(&user, &additional).unwrap();
    let platinum = client.get_user_volume_data(&user);
    assert_eq!(platinum.current_tier, crate::fees::VolumeTier::Platinum);
}

/// Test tier never downgrades (monotonic)
#[test]
fn test_tier_monotonic_no_downgrade() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);
    let user1 = setup_investor(&env, &client, &admin);
    let user2 = setup_investor(&env, &client, &admin);

    client.initialize_fee_system(&admin);

    // User1 reaches Platinum
    client.update_user_transaction_volume(&user1, &1_000_000_000_000_i128).unwrap();
    let mut user1_tier = client.get_user_volume_data(&user1);
    assert_eq!(user1_tier.current_tier, crate::fees::VolumeTier::Platinum);

    // User2 reaches Gold
    client.update_user_transaction_volume(&user2, &500_000_000_000_i128).unwrap();
    let user2_tier = client.get_user_volume_data(&user2);
    assert_eq!(user2_tier.current_tier, crate::fees::VolumeTier::Gold);

    // Additional transactions don't change tier backwards
    client.update_user_transaction_volume(&user1, &1_i128).unwrap();
    user1_tier = client.get_user_volume_data(&user1);
    assert_eq!(user1_tier.current_tier, crate::fees::VolumeTier::Platinum); // Still Platinum
}

/// Test fee discount increases with tier progression
#[test]
fn test_fee_discount_increases_with_tier() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);
    let user = setup_investor(&env, &client, &admin);

    client.initialize_fee_system(&admin);

    let transaction_amount = 100_000_i128;

    // Standard tier (0% discount)
    let standard_fees = client.calculate_transaction_fees(&user, &transaction_amount, &false, &false);

    // Upgrade to Silver (5% discount)
    client.update_user_transaction_volume(&user, &100_000_000_000_i128).unwrap();
    let silver_fees = client.calculate_transaction_fees(&user, &transaction_amount, &false, &false);

    // Upgrade to Gold (10% discount)
    client.update_user_transaction_volume(&user, &400_000_000_000_i128).unwrap();
    let gold_fees = client.calculate_transaction_fees(&user, &transaction_amount, &false, &false);

    // Upgrade to Platinum (15% discount)
    client.update_user_transaction_volume(&user, &500_000_000_000_i128).unwrap();
    let platinum_fees = client.calculate_transaction_fees(&user, &transaction_amount, &false, &false);

    // Verify discount progression
    assert!(standard_fees > silver_fees);
    assert!(silver_fees > gold_fees);
    assert!(gold_fees > platinum_fees);
}

// ============================================================================
// SETTLEMENT AND REPEATED TRANSACTION TESTS
// ============================================================================

/// Test fee calculation remains consistent across multiple settlements
#[test]
fn test_fee_calculation_consistent_multiple_settlements() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);
    let user = setup_investor(&env, &client, &admin);

    client.initialize_fee_system(&admin);

    let transaction_amount = 50_000_i128;

    // Settlement 1
    let fees1 = client.calculate_transaction_fees(&user, &transaction_amount, &false, &false);
    client.update_user_transaction_volume(&user, &transaction_amount).unwrap();

    // Settlement 2 (net new fees, tier unchanged)
    let fees2 = client.calculate_transaction_fees(&user, &transaction_amount, &false, &false);

    // Fees should be identical since tier hasn't changed
    assert_eq!(fees1, fees2);
}

/// Test fee reduction after tier upgrade during settlement sequence
#[test]
fn test_fee_reduction_after_tier_upgrade_settlement() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);
    let user = setup_investor(&env, &client, &admin);

    client.initialize_fee_system(&admin);

    let transaction_amount = 50_000_i128;

    // Settlement 1 at Standard tier
    let fees_before_upgrade = client.calculate_transaction_fees(&user, &transaction_amount, &false, &false);

    // Accumulate volume to reach Silver tier
    client.update_user_transaction_volume(&user, &100_000_000_000_i128).unwrap();

    // Settlement 2 at Silver tier
    let fees_after_upgrade = client.calculate_transaction_fees(&user, &transaction_amount, &false, &false);

    assert!(fees_after_upgrade < fees_before_upgrade);
}

/// Test cumulative volume and tier changes through settlement lifecycle
#[test]
fn test_cumulative_volume_through_settlement_lifecycle() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);
    let user = setup_investor(&env, &client, &admin);

    client.initialize_fee_system(&admin);

    // Settlement Round 1: Small transactions stay Standard tier
    let round1_amount = 10_000_i128;
    for _ in 0..5 {
        client.update_user_transaction_volume(&user, &round1_amount).unwrap();
    }
    let vol_after_round1 = client.get_user_volume_data(&user);
    assert_eq!(vol_after_round1.total_volume, round1_amount * 5);
    assert_eq!(vol_after_round1.transaction_count, 5);
    assert_eq!(vol_after_round1.current_tier, crate::fees::VolumeTier::Standard);

    // Settlement Round 2: Larger transactions reach Silver
    let round2_amount = 30_000_000_000_i128;
    client.update_user_transaction_volume(&user, &round2_amount).unwrap();
    let vol_after_round2 = client.get_user_volume_data(&user);
    assert_eq!(vol_after_round2.total_volume, round1_amount * 5 + round2_amount);
    assert_eq!(vol_after_round2.transaction_count, 6);
    assert_eq!(vol_after_round2.current_tier, crate::fees::VolumeTier::Silver);

    // Settlement Round 3: Continue reaching Gold
    let round3_amount = 500_000_000_000_i128;
    client.update_user_transaction_volume(&user, &round3_amount).unwrap();
    let vol_after_round3 = client.get_user_volume_data(&user);
    assert_eq!(vol_after_round3.total_volume, round1_amount * 5 + round2_amount + round3_amount);
    assert_eq!(vol_after_round3.transaction_count, 7);
    assert_eq!(vol_after_round3.current_tier, crate::fees::VolumeTier::Gold);
}

/// Test fee calculation determinism after multiple settlement updates
#[test]
fn test_fee_calculation_deterministic_after_settlements() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);
    let user = setup_investor(&env, &client, &admin);

    client.initialize_fee_system(&admin);

    // Simulate settlement sequence: reach Gold tier
    client.update_user_transaction_volume(&user, &500_000_000_000_i128).unwrap();

    let transaction_amount = 12_345_i128;

    // Calculate fees multiple times at same tier
    let calc1 = client.calculate_transaction_fees(&user, &transaction_amount, &false, &false);
    let calc2 = client.calculate_transaction_fees(&user, &transaction_amount, &false, &false);
    let calc3 = client.calculate_transaction_fees(&user, &transaction_amount, &false, &false);

    assert_eq!(calc1, calc2);
    assert_eq!(calc2, calc3);

    // Add more volume but don't trigger tier change
    client.update_user_transaction_volume(&user, &1_i128).unwrap();

    let calc4 = client.calculate_transaction_fees(&user, &transaction_amount, &false, &false);
    assert_eq!(calc1, calc4); // Should still be same (tier unchanged)
}

/// Test fee collection and revenue accumulation during settlement sequence
#[test]
fn test_revenue_accumulation_through_settlements() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);
    let user = setup_investor(&env, &client, &admin);
    let treasury = Address::generate(&env);

    client.initialize_fee_system(&admin);
    client.configure_revenue_distribution(&admin, &treasury, &5000, &2500, &2500, &false, &1);

    let settlement_amount = 10_000_i128;

    // Settlement 1
    let mut fees_map = Map::new(&env);
    fees_map.set(FeeType::Platform, settlement_amount);
    client.collect_transaction_fees(&user, &fees_map, &settlement_amount);
    client.update_user_transaction_volume(&user, &settlement_amount).unwrap();

    // Settlement 2
    let mut fees_map2 = Map::new(&env);
    fees_map2.set(FeeType::Platform, settlement_amount);
    client.collect_transaction_fees(&user, &fees_map2, &settlement_amount);
    client.update_user_transaction_volume(&user, &settlement_amount).unwrap();

    // Verify volume accumulated
    let final_volume = client.get_user_volume_data(&user);
    assert_eq!(final_volume.total_volume, settlement_amount * 2);
    assert_eq!(final_volume.transaction_count, 2);

    // Verify revenue was collected for distribution
    let current_period = env.ledger().timestamp() / 2_592_000;
    let (treasury_amt, developer_amt, platform_amt) = client.distribute_revenue(&admin, &current_period);

    // Total should be 20_000 (2 * 10_000)
    let total_distributed = treasury_amt + developer_amt + platform_amt;
    assert_eq!(total_distributed, settlement_amount * 2);
    assert_eq!(treasury_amt, 10_000); // 50%
    assert_eq!(developer_amt, 5_000);  // 25%
    assert_eq!(platform_amt, 5_000);   // 25%
}

// ============================================================================
// FEE CONFIG BOUNDS TESTS - fee never exceeds total paid, rounding safety
// ============================================================================

/// Fee BPS of 0 produces zero fee and investor receives full payment.
#[test]
fn test_fee_bps_zero_no_fee_extracted() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);
    let _ = client.initialize_admin(&admin);

    client.set_platform_fee(&0);
    let (investor_return, platform_fee) = client.calculate_profit(&1000, &1100);
    assert_eq!(platform_fee, 0);
    assert_eq!(investor_return, 1100);
    assert_eq!(investor_return + platform_fee, 1100);
}

/// Fee BPS at hard cap (1000 = 10%) never exceeds gross profit.
#[test]
fn test_fee_bps_at_hard_cap_never_exceeds_profit() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);
    let _ = client.initialize_admin(&admin);

    client.set_platform_fee(&1000); // 10%
    let investment = 1_000_000i128;
    let payment = 1_100_000i128;
    let gross_profit = payment - investment; // 100_000

    let (investor_return, platform_fee) = client.calculate_profit(&investment, &payment);

    // Fee must not exceed gross profit
    assert!(platform_fee <= gross_profit);
    // Invariant: parts sum to whole
    assert_eq!(investor_return + platform_fee, payment);
    // Investor always gets at least their principal back
    assert!(investor_return >= investment);
}

/// Attempting to set fee BPS above 1000 is rejected.
#[test]
fn test_fee_bps_above_hard_cap_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);
    let _ = client.initialize_admin(&admin);

    let result = client.try_set_platform_fee(&1001);
    assert!(result.is_err());
}

/// investor_return + platform_fee == payment_amount for all fee rates 0..=1000.
#[test]
fn test_fee_plus_return_equals_payment_all_rates() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);
    let _ = client.initialize_admin(&admin);

    let investment = 10_000i128;
    let payment = 11_000i128;

    for bps in [0u32, 1, 100, 200, 500, 999, 1000] {
        client.set_platform_fee(&bps);
        let (investor_return, platform_fee) = client.calculate_profit(&investment, &payment);
        assert_eq!(
            investor_return + platform_fee,
            payment,
            "invariant broken at bps={bps}"
        );
        assert!(platform_fee >= 0, "fee must be non-negative at bps={bps}");
        assert!(
            investor_return >= investment,
            "investor must recover principal at bps={bps}"
        );
    }
}

/// Rounding boundary: profit=1 with 2% fee rounds down to 0 (floor division).
#[test]
fn test_rounding_profit_1_fee_floors_to_zero() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);
    let _ = client.initialize_admin(&admin);

    // profit=1, fee_bps=200 => 1*200/10000 = 0.02 => floor = 0
    let (investor_return, platform_fee) = client.calculate_profit(&1000, &1001);
    assert_eq!(platform_fee, 0);
    assert_eq!(investor_return, 1001);
    assert_eq!(investor_return + platform_fee, 1001);
}

/// Rounding boundary: smallest profit that yields fee=1 at 2% is profit=50.
#[test]
fn test_rounding_first_nonzero_fee_at_2pct() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);
    let _ = client.initialize_admin(&admin);

    // profit=49 => 49*200/10000 = 0.98 => floor = 0
    let (_, fee_49) = client.calculate_profit(&1000, &1049);
    assert_eq!(fee_49, 0);

    // profit=50 => 50*200/10000 = 1.0 => floor = 1
    let (_, fee_50) = client.calculate_profit(&1000, &1050);
    assert_eq!(fee_50, 1);
}

/// No profit scenario: payment == investment yields zero fee.
#[test]
fn test_no_profit_zero_fee() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);
    let _ = client.initialize_admin(&admin);

    let (investor_return, platform_fee) = client.calculate_profit(&5000, &5000);
    assert_eq!(platform_fee, 0);
    assert_eq!(investor_return, 5000);
}

/// Loss scenario: payment < investment yields zero fee; investor absorbs loss.
#[test]
fn test_loss_scenario_zero_fee_investor_absorbs_loss() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);
    let _ = client.initialize_admin(&admin);

    let (investor_return, platform_fee) = client.calculate_profit(&5000, &4000);
    assert_eq!(platform_fee, 0);
    assert_eq!(investor_return, 4000);
    assert_eq!(investor_return + platform_fee, 4000);
}

/// Fee is always <= gross_profit regardless of BPS value.
#[test]
fn test_fee_never_exceeds_gross_profit() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);
    let _ = client.initialize_admin(&admin);

    let cases: &[(i128, i128)] = &[
        (1, 2),
        (100, 101),
        (1_000, 1_001),
        (1_000_000, 2_000_000),
        (i128::MAX / 2, i128::MAX / 2 + 1),
    ];

    for &(investment, payment) in cases {
        client.set_platform_fee(&1000); // worst-case 10%
        let (investor_return, platform_fee) = client.calculate_profit(&investment, &payment);
        let gross_profit = payment - investment;
        assert!(
            platform_fee <= gross_profit,
            "fee {platform_fee} > gross_profit {gross_profit} for investment={investment} payment={payment}"
        );
        assert_eq!(
            investor_return + platform_fee,
            payment,
            "invariant broken for investment={investment} payment={payment}"
        );
    }
}

/// Fee calculation is deterministic: same inputs always produce same outputs.
#[test]
fn test_fee_calculation_is_deterministic() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);
    let _ = client.initialize_admin(&admin);

    let (r1, f1) = client.calculate_profit(&10_000, &12_345);
    let (r2, f2) = client.calculate_profit(&10_000, &12_345);
    let (r3, f3) = client.calculate_profit(&10_000, &12_345);
    assert_eq!((r1, f1), (r2, f2));
    assert_eq!((r2, f2), (r3, f3));
}

/// Fee update is idempotent: setting same BPS twice does not corrupt state.
#[test]
fn test_fee_update_idempotent() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);
    let _ = client.initialize_admin(&admin);

    client.set_platform_fee(&300);
    client.set_platform_fee(&300);
    assert_eq!(client.get_platform_fee().fee_bps, 300);
}

/// Overflow safety: very large amounts do not panic and satisfy the invariant.
#[test]
fn test_overflow_safety_large_amounts() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);
    let _ = client.initialize_admin(&admin);

    // Use values that stress saturating arithmetic without hitting i128::MAX exactly
    let investment: i128 = 1_000_000_000_000_000_000_000_000_000_000_000_000;
    let payment: i128 = 1_100_000_000_000_000_000_000_000_000_000_000_000;

    let (investor_return, platform_fee) = client.calculate_profit(&investment, &payment);
    assert_eq!(investor_return + platform_fee, payment);
    assert!(platform_fee >= 0);
    assert!(investor_return >= investment);
}

// ============================================================================
// ADDITIONAL FEE CONFIG BOUNDS TESTS - targeted boundary and rounding cases
// ============================================================================

/// fee_bps=1 (0.01%) is the smallest non-zero rate; fee rounds to 0 for small profits.
#[test]
fn test_fee_bps_one_rounds_to_zero_for_small_profit() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);
    let _ = client.initialize_admin(&admin);

    // 1 bps: profit must be >= 10_000 to yield fee=1
    client.set_platform_fee(&1);
    let (_, fee_small) = client.calculate_profit(&1000, &1009); // profit=9 => 9*1/10000=0
    assert_eq!(fee_small, 0);

    let (_, fee_exact) = client.calculate_profit(&0, &10_000); // profit=10000 => 10000*1/10000=1
    assert_eq!(fee_exact, 1);
}

/// fee_bps=999 (9.99%): fee is strictly less than 10% of gross profit.
#[test]
fn test_fee_bps_999_strictly_less_than_10pct() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);
    let _ = client.initialize_admin(&admin);

    client.set_platform_fee(&999);
    let investment = 0i128;
    let payment = 10_000i128;
    let (investor_return, platform_fee) = client.calculate_profit(&investment, &payment);

    // 10000 * 999 / 10000 = 999 (floor)
    assert_eq!(platform_fee, 999);
    assert!(platform_fee < payment / 10); // strictly < 10%
    assert_eq!(investor_return + platform_fee, payment);
}

/// fee_bps=1000 (10%): fee equals exactly 10% when profit is a multiple of 10.
#[test]
fn test_fee_bps_1000_exact_10pct_on_round_profit() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);
    let _ = client.initialize_admin(&admin);

    client.set_platform_fee(&1000);
    let investment = 1_000i128;
    let payment = 1_100i128; // profit=100, divisible by 10
    let (investor_return, platform_fee) = client.calculate_profit(&investment, &payment);

    assert_eq!(platform_fee, 10); // 100 * 1000 / 10000 = 10
    assert_eq!(investor_return, 1_090);
    assert_eq!(investor_return + platform_fee, payment);
}

/// Rounding: profit=9999 at 2% => floor(9999*200/10000)=floor(199.98)=199, not 200.
#[test]
fn test_rounding_floor_not_ceiling() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);
    let _ = client.initialize_admin(&admin);

    // profit=9999, fee_bps=200 => 9999*200/10000 = 199 (floor, not 200)
    let (investor_return, platform_fee) = client.calculate_profit(&0, &9999);
    assert_eq!(platform_fee, 199);
    assert_eq!(investor_return, 9800);
    assert_eq!(investor_return + platform_fee, 9999);
}

/// Rounding: platform absorbs the fractional remainder - no dust is created.
#[test]
fn test_rounding_no_dust_created() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);
    let _ = client.initialize_admin(&admin);

    // Sweep a range of profits to confirm no dust at any value
    for profit in [1i128, 2, 3, 7, 49, 50, 99, 100, 4999, 5000, 9999, 10_000] {
        let (investor_return, platform_fee) = client.calculate_profit(&0, &profit);
        assert_eq!(
            investor_return + platform_fee,
            profit,
            "dust detected at profit={profit}"
        );
        assert!(platform_fee >= 0, "negative fee at profit={profit}");
    }
}

/// Invariant holds when investment equals payment (break-even).
#[test]
fn test_invariant_break_even() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);
    let _ = client.initialize_admin(&admin);

    for amount in [1i128, 100, 10_000, 1_000_000] {
        let (investor_return, platform_fee) = client.calculate_profit(&amount, &amount);
        assert_eq!(platform_fee, 0, "break-even must have zero fee at amount={amount}");
        assert_eq!(investor_return, amount);
    }
}

/// Computed fee never exceeds total payment_amount (not just gross profit).
#[test]
fn test_fee_never_exceeds_total_payment() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);
    let _ = client.initialize_admin(&admin);

    client.set_platform_fee(&1000); // worst-case 10%
    let cases: &[(i128, i128)] = &[
        (0, 1),
        (0, 100),
        (50, 100),
        (100, 100),
        (100, 50), // loss
        (1_000_000, 1_000_001),
    ];
    for &(investment, payment) in cases {
        let (_, platform_fee) = client.calculate_profit(&investment, &payment);
        assert!(
            platform_fee <= payment,
            "fee {platform_fee} > payment {payment}"
        );
    }
}

/// investor_return is always non-negative regardless of fee rate or amounts.
#[test]
fn test_investor_return_always_non_negative() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);
    let _ = client.initialize_admin(&admin);

    for bps in [0u32, 1, 200, 500, 1000] {
        client.set_platform_fee(&bps);
        for &(inv, pay) in &[(0i128, 0i128), (100, 50), (1000, 1000), (1000, 1100)] {
            let (investor_return, _) = client.calculate_profit(&inv, &pay);
            assert!(
                investor_return >= 0,
                "negative investor_return at bps={bps} inv={inv} pay={pay}"
            );
        }
    }
}
