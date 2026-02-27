//! Comprehensive tests for the Profit and Fee Calculation Formula
//!
//! This test module covers:
//! - Basic profit/fee calculations
//! - Edge cases (exact payment, overpayment, underpayment)
//! - Rounding behavior (no dust guarantee)
//! - Overflow safety with large amounts
//! - Treasury split calculations
//! - Integration with settlement flow
//!
//! Test coverage target: >= 95%

extern crate std;

use super::*;
use crate::profits::{calculate_treasury_split, validate_calculation_inputs, PlatformFee};
use soroban_sdk::{testutils::Address as _, Address, Env, String};
use std::vec;

// ============================================================================
// Test Helpers
// ============================================================================

/// Helper function to set up admin for testing
fn setup_admin(env: &Env, client: &QuickLendXContractClient) -> Address {
    let admin = Address::generate(&env);
    client.set_admin(&admin);
    admin
}

/// Helper function to create and verify a business
fn setup_business(env: &Env, client: &QuickLendXContractClient, admin: &Address) -> Address {
    let business = Address::generate(&env);
    client.submit_kyc_application(&business, &String::from_str(env, "Business KYC"));
    client.verify_business(admin, &business);
    business
}

/// Helper function to create and verify an investor
fn setup_investor(env: &Env, client: &QuickLendXContractClient, admin: &Address) -> Address {
    let investor = Address::generate(&env);
    client.submit_investor_kyc(&investor, &String::from_str(env, "Investor KYC"));
    client.verify_investor(&investor, &1_000_000);
    investor
}

// ============================================================================
// Basic Calculation Tests
// ============================================================================

#[test]
fn test_profit_fee_basic_calculation() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    // Test default 2% fee calculation
    // Investment: 1000, Payment: 1100 (10% return)
    // Profit: 100, Fee: 2% of 100 = 2
    let investment_amount = 1000;
    let payment_amount = 1100;

    let (investor_return, platform_fee) =
        client.calculate_profit(&investment_amount, &payment_amount);

    assert_eq!(platform_fee, 2);
    assert_eq!(investor_return, 1098);

    // Verify no dust: investor_return + platform_fee == payment_amount
    assert_eq!(investor_return + platform_fee, payment_amount);
}

#[test]
fn test_profit_fee_with_custom_rate() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);

    // Set custom fee to 5%
    client.set_platform_fee(&500);

    let investment_amount = 1000;
    let payment_amount = 1100;

    let (investor_return, platform_fee) =
        client.calculate_profit(&investment_amount, &payment_amount);

    // 5% of 100 profit = 5
    assert_eq!(platform_fee, 5);
    assert_eq!(investor_return, 1095);
    assert_eq!(investor_return + platform_fee, payment_amount);
}

#[test]
fn test_profit_fee_max_rate() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);

    // Set maximum fee of 10%
    client.set_platform_fee(&1000);

    let investment_amount = 1000;
    let payment_amount = 1100;

    let (investor_return, platform_fee) =
        client.calculate_profit(&investment_amount, &payment_amount);

    // 10% of 100 profit = 10
    assert_eq!(platform_fee, 10);
    assert_eq!(investor_return, 1090);
    assert_eq!(investor_return + platform_fee, payment_amount);
}

#[test]
fn test_profit_fee_zero_rate() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);

    // Set zero fee
    client.set_platform_fee(&0);

    let investment_amount = 1000;
    let payment_amount = 1100;

    let (investor_return, platform_fee) =
        client.calculate_profit(&investment_amount, &payment_amount);

    // No fee
    assert_eq!(platform_fee, 0);
    assert_eq!(investor_return, 1100);
}

// ============================================================================
// Edge Case Tests: Exact Payment
// ============================================================================

#[test]
fn test_exact_payment_no_profit() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    // Payment exactly equals investment - no profit
    let investment_amount = 1000;
    let payment_amount = 1000;

    let (investor_return, platform_fee) =
        client.calculate_profit(&investment_amount, &payment_amount);

    // No profit means no fee
    assert_eq!(platform_fee, 0);
    assert_eq!(investor_return, 1000);
    assert_eq!(investor_return + platform_fee, payment_amount);
}

#[test]
fn test_exact_payment_one_unit_profit() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    // Payment is investment + 1 (minimal profit)
    let investment_amount = 1000;
    let payment_amount = 1001;

    let (investor_return, platform_fee) =
        client.calculate_profit(&investment_amount, &payment_amount);

    // Profit: 1, Fee: 2% of 1 = 0.02 -> rounds to 0
    assert_eq!(platform_fee, 0);
    assert_eq!(investor_return, 1001);
    assert_eq!(investor_return + platform_fee, payment_amount);
}

// ============================================================================
// Edge Case Tests: Overpayment
// ============================================================================

#[test]
fn test_overpayment_large_profit() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    // Payment is 2x investment (100% profit)
    let investment_amount = 1000;
    let payment_amount = 2000;

    let (investor_return, platform_fee) =
        client.calculate_profit(&investment_amount, &payment_amount);

    // Profit: 1000, Fee: 2% of 1000 = 20
    assert_eq!(platform_fee, 20);
    assert_eq!(investor_return, 1980);
    assert_eq!(investor_return + platform_fee, payment_amount);
}

#[test]
fn test_overpayment_extreme_profit() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    // Payment is 10x investment (900% profit)
    let investment_amount = 1000;
    let payment_amount = 10000;

    let (investor_return, platform_fee) =
        client.calculate_profit(&investment_amount, &payment_amount);

    // Profit: 9000, Fee: 2% of 9000 = 180
    assert_eq!(platform_fee, 180);
    assert_eq!(investor_return, 9820);
    assert_eq!(investor_return + platform_fee, payment_amount);
}

// ============================================================================
// Edge Case Tests: Underpayment
// ============================================================================

#[test]
fn test_underpayment_partial_loss() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    // Payment is less than investment (10% loss)
    let investment_amount = 1000;
    let payment_amount = 900;

    let (investor_return, platform_fee) =
        client.calculate_profit(&investment_amount, &payment_amount);

    // No profit means no fee, investor gets whatever was paid
    assert_eq!(platform_fee, 0);
    assert_eq!(investor_return, 900);
    assert_eq!(investor_return + platform_fee, payment_amount);
}

#[test]
fn test_underpayment_severe_loss() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    // Payment is much less than investment (90% loss)
    let investment_amount = 1000;
    let payment_amount = 100;

    let (investor_return, platform_fee) =
        client.calculate_profit(&investment_amount, &payment_amount);

    // No profit means no fee
    assert_eq!(platform_fee, 0);
    assert_eq!(investor_return, 100);
}

#[test]
fn test_underpayment_zero_payment() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    // Zero payment (total default)
    let investment_amount = 1000;
    let payment_amount = 0;

    let (investor_return, platform_fee) =
        client.calculate_profit(&investment_amount, &payment_amount);

    assert_eq!(platform_fee, 0);
    assert_eq!(investor_return, 0);
}

// ============================================================================
// Rounding Tests (No Dust Guarantee)
// ============================================================================

#[test]
fn test_rounding_small_profit_various_fees() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);

    // Test rounding with various fee rates and small profits
    let test_cases = vec![
        // (investment, payment, fee_bps, expected_fee)
        (1000, 1001, 200, 0), // 1 profit, 2% = 0.02 -> 0
        (1000, 1010, 200, 0), // 10 profit, 2% = 0.2 -> 0
        (1000, 1049, 200, 0), // 49 profit, 2% = 0.98 -> 0
        (1000, 1050, 200, 1), // 50 profit, 2% = 1.0 -> 1
        (1000, 1051, 200, 1), // 51 profit, 2% = 1.02 -> 1
        (1000, 1099, 200, 1), // 99 profit, 2% = 1.98 -> 1
        (1000, 1100, 200, 2), // 100 profit, 2% = 2.0 -> 2
    ];

    for (investment, payment, fee_bps, expected_fee) in test_cases {
        client.set_platform_fee(&fee_bps);
        let (investor_return, platform_fee) = client.calculate_profit(&investment, &payment);

        assert_eq!(
            platform_fee, expected_fee,
            "Failed for inv={}, pay={}, fee_bps={}",
            investment, payment, fee_bps
        );

        // Always verify no dust
        assert_eq!(
            investor_return + platform_fee,
            payment,
            "Dust found for inv={}, pay={}, fee_bps={}",
            investment,
            payment,
            fee_bps
        );
    }
}

#[test]
fn test_rounding_boundary_cases() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let _admin = setup_admin(&env, &client);

    // Test exact boundaries where fee should just cross integer threshold
    // At 2% (200 bps), fee = profit * 200 / 10000 = profit / 50
    // So for fee = 1, we need profit >= 50

    client.set_platform_fee(&200);

    // profit = 49 -> fee = 49/50 = 0.98 -> 0
    let (_, fee) = client.calculate_profit(&1000, &1049);
    assert_eq!(fee, 0);

    // profit = 50 -> fee = 50/50 = 1.0 -> 1
    let (_, fee) = client.calculate_profit(&1000, &1050);
    assert_eq!(fee, 1);

    // profit = 99 -> fee = 99/50 = 1.98 -> 1
    let (_, fee) = client.calculate_profit(&1000, &1099);
    assert_eq!(fee, 1);

    // profit = 100 -> fee = 100/50 = 2.0 -> 2
    let (_, fee) = client.calculate_profit(&1000, &1100);
    assert_eq!(fee, 2);
}

#[test]
fn test_no_dust_comprehensive() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let _admin = setup_admin(&env, &client);

    // Test many combinations to ensure no dust ever
    let investments = vec![100, 1000, 10000, 123456, 999999];
    let fee_rates = vec![0, 50, 100, 200, 333, 500, 750, 1000];

    for investment in &investments {
        for fee_bps in &fee_rates {
            client.set_platform_fee(fee_bps);

            // Test various payment amounts
            for multiplier in [0.5, 0.9, 1.0, 1.01, 1.1, 1.5, 2.0, 5.0] {
                let payment = (*investment as f64 * multiplier) as i128;
                let (investor_return, platform_fee) = client.calculate_profit(investment, &payment);

                // THE KEY INVARIANT: no dust
                assert_eq!(
                    investor_return + platform_fee,
                    payment,
                    "Dust found: inv={}, pay={}, fee_bps={}, return={}, fee={}",
                    investment,
                    payment,
                    fee_bps,
                    investor_return,
                    platform_fee
                );
            }
        }
    }
}

// ============================================================================
// Large Amount Tests (Overflow Safety)
// ============================================================================

#[test]
fn test_large_amounts_no_overflow() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    // Test with very large amounts (trillions)
    let investment_amount: i128 = 1_000_000_000_000; // 1 trillion
    let payment_amount: i128 = 1_100_000_000_000; // 1.1 trillion

    let (investor_return, platform_fee) =
        client.calculate_profit(&investment_amount, &payment_amount);

    // Profit: 100 billion, Fee: 2% = 2 billion
    assert_eq!(platform_fee, 2_000_000_000);
    assert_eq!(investor_return, 1_098_000_000_000);
    assert_eq!(investor_return + platform_fee, payment_amount);
}

#[test]
fn test_extreme_large_amounts() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    // Test with amounts near i128 limits (but safe for multiplication)
    let investment_amount: i128 = 1_000_000_000_000_000_000; // 1 quintillion
    let payment_amount: i128 = 1_100_000_000_000_000_000; // 1.1 quintillion

    let (investor_return, platform_fee) =
        client.calculate_profit(&investment_amount, &payment_amount);

    // Profit: 100 quadrillion, Fee: 2% = 2 quadrillion
    assert_eq!(platform_fee, 2_000_000_000_000_000);
    assert_eq!(investor_return, 1_098_000_000_000_000_000);
    assert_eq!(investor_return + platform_fee, payment_amount);
}

// ============================================================================
// Zero Investment Tests
// ============================================================================

#[test]
fn test_zero_investment_all_profit() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    // Zero investment means entire payment is profit
    let investment_amount = 0;
    let payment_amount = 1000;

    let (investor_return, platform_fee) =
        client.calculate_profit(&investment_amount, &payment_amount);

    // All 1000 is profit, fee = 2% = 20
    assert_eq!(platform_fee, 20);
    assert_eq!(investor_return, 980);
    assert_eq!(investor_return + platform_fee, payment_amount);
}

// ============================================================================
// Treasury Split Tests
// ============================================================================

#[test]
fn test_treasury_split_equal() {
    // 50/50 split
    let (treasury, remaining) = calculate_treasury_split(100, 5000);
    assert_eq!(treasury, 50);
    assert_eq!(remaining, 50);
    assert_eq!(treasury + remaining, 100);
}

#[test]
fn test_treasury_split_unequal() {
    // 70/30 split
    let (treasury, remaining) = calculate_treasury_split(100, 7000);
    assert_eq!(treasury, 70);
    assert_eq!(remaining, 30);
    assert_eq!(treasury + remaining, 100);
}

#[test]
fn test_treasury_split_with_rounding() {
    // 33.33% split
    let (treasury, remaining) = calculate_treasury_split(100, 3333);
    assert_eq!(treasury, 33);
    assert_eq!(remaining, 67);
    assert_eq!(treasury + remaining, 100); // No dust
}

#[test]
fn test_treasury_split_zero_fee() {
    let (treasury, remaining) = calculate_treasury_split(0, 5000);
    assert_eq!(treasury, 0);
    assert_eq!(remaining, 0);
}

#[test]
fn test_treasury_split_zero_share() {
    let (treasury, remaining) = calculate_treasury_split(100, 0);
    assert_eq!(treasury, 0);
    assert_eq!(remaining, 100);
}

#[test]
fn test_treasury_split_full_share() {
    let (treasury, remaining) = calculate_treasury_split(100, 10000);
    assert_eq!(treasury, 100);
    assert_eq!(remaining, 0);
}

#[test]
fn test_treasury_split_over_100_percent() {
    // Share > 100% should still give all to treasury
    let (treasury, remaining) = calculate_treasury_split(100, 15000);
    assert_eq!(treasury, 100);
    assert_eq!(remaining, 0);
}

// ============================================================================
// Input Validation Tests
// ============================================================================

#[test]
fn test_validate_inputs_valid() {
    assert!(validate_calculation_inputs(1000, 1100).is_ok());
    assert!(validate_calculation_inputs(0, 0).is_ok());
    assert!(validate_calculation_inputs(0, 1000).is_ok());
    assert!(validate_calculation_inputs(1000, 0).is_ok());
}

#[test]
fn test_validate_inputs_negative_investment() {
    assert!(validate_calculation_inputs(-1, 1000).is_err());
}

#[test]
fn test_validate_inputs_negative_payment() {
    assert!(validate_calculation_inputs(1000, -1).is_err());
}

// ============================================================================
// Fee Configuration Tests
// ============================================================================

#[test]
fn test_fee_config_default() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let fee_config = client.get_platform_fee();
    assert_eq!(fee_config.fee_bps, 200); // Default 2%
}

#[test]
fn test_fee_config_update() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);

    client.set_platform_fee(&500); // 5%
    let fee_config = client.get_platform_fee();
    assert_eq!(fee_config.fee_bps, 500);
    assert_eq!(fee_config.updated_by, admin);
}

#[test]
fn test_fee_config_max_boundary() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let _admin = setup_admin(&env, &client);

    // Max allowed: 10% (1000 bps)
    client.set_platform_fee(&1000);
    let fee_config = client.get_platform_fee();
    assert_eq!(fee_config.fee_bps, 1000);
}
