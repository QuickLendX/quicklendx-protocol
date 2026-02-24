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
use crate::profits::{calculate_treasury_split, validate_calculation_inputs};
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
    let admin = setup_admin(&env, &client);

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
    let admin = setup_admin(&env, &client);

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
    let admin = setup_admin(&env, &client);

    // Max allowed: 10% (1000 bps)
    client.set_platform_fee(&1000);
    let fee_config = client.get_platform_fee();
    assert_eq!(fee_config.fee_bps, 1000);
}

#[test]
fn test_fee_config_exceeds_max() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);

    // Attempt to set > 10% should fail
    let result = client.try_set_platform_fee(&1200);
    assert!(result.is_err());
}

// ============================================================================
// Integration Tests
// ============================================================================

#[test]
fn test_profit_calculation_integration_with_fee_manager() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);

    // Initialize fee system
    client.initialize_fee_system(&admin);

    // Both calculate_profit and FeeManager should give same results
    let investment = 10000;
    let payment = 11000;

    let (investor_return, platform_fee) = client.calculate_profit(&investment, &payment);

    // Verify consistency
    assert_eq!(platform_fee, 20); // 2% of 1000 profit
    assert_eq!(investor_return, 10980);
    assert_eq!(investor_return + platform_fee, payment);
}

// ============================================================================
// Stress Tests
// ============================================================================

#[test]
fn test_many_calculations_no_dust() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    // Run 100 random-ish calculations and verify no dust
    for i in 1..=100 {
        let investment = i * 1000;
        let payment = investment + (i * 10); // Small profit

        let (investor_return, platform_fee) = client.calculate_profit(&investment, &payment);

        assert_eq!(
            investor_return + platform_fee,
            payment,
            "Dust at iteration {}: inv={}, pay={}, return={}, fee={}",
            i,
            investment,
            payment,
            investor_return,
            platform_fee
        );
    }
}

// ============================================================================
// Specific Scenario Tests
// ============================================================================

#[test]
fn test_realistic_invoice_scenario() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    // Realistic scenario: $10,000 invoice, investor funds at 8% discount
    // Investment: 9,200 (92% of invoice)
    // Expected payment at maturity: 10,000 (108.7% return)
    let investment_amount = 9_200_000_000; // 9,200 in stroops (7 decimals)
    let payment_amount = 10_000_000_000; // 10,000 in stroops

    let (investor_return, platform_fee) =
        client.calculate_profit(&investment_amount, &payment_amount);

    // Profit: 800,000,000 stroops
    // Platform fee: 2% of 800M = 16,000,000 stroops
    assert_eq!(platform_fee, 16_000_000);
    assert_eq!(investor_return, 9_984_000_000);
    assert_eq!(investor_return + platform_fee, payment_amount);
}

#[test]
fn test_minimal_profit_scenario() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    // Minimal profit scenario where rounding matters most
    // Investment: 9,999,999
    // Payment: 10,000,000 (profit of 1)
    let investment_amount = 9_999_999;
    let payment_amount = 10_000_000;

    let (investor_return, platform_fee) =
        client.calculate_profit(&investment_amount, &payment_amount);

    // Profit: 1, Fee: 2% of 1 = 0.02 -> 0 (rounds down)
    assert_eq!(platform_fee, 0);
    assert_eq!(investor_return, 10_000_000);
    assert_eq!(investor_return + platform_fee, payment_amount);
}

#[test]
fn test_default_scenario_no_profit() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    // Default scenario: business can only pay 80% of invoice
    let investment_amount = 9_200_000_000;
    let payment_amount = 8_000_000_000; // Only 80% recovered

    let (investor_return, platform_fee) =
        client.calculate_profit(&investment_amount, &payment_amount);

    // No profit (actually a loss), so no fee
    assert_eq!(platform_fee, 0);
    assert_eq!(investor_return, 8_000_000_000);
}

// ============================================================================
// Additional Edge Case Tests
// ============================================================================

#[test]
fn test_dust_prevention_various_amounts() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    // Test various amounts to ensure no dust is created
    let test_cases = vec![
        (100, 101),
        (100, 150),
        (100, 200),
        (1000, 1001),
        (1000, 1100),
        (10000, 10001),
        (10000, 11000),
        (999999, 1000000),
        (1000000, 1000001),
    ];

    for (investment, payment) in test_cases {
        let (investor_return, platform_fee) = client.calculate_profit(&investment, &payment);
        assert_eq!(
            investor_return + platform_fee,
            payment,
            "Dust detected for investment={}, payment={}",
            investment,
            payment
        );
    }
}

#[test]
fn test_payment_equals_investment_boundary() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    // Exact payment = investment (no profit, no loss)
    let investment_amount = 5_000_000;
    let payment_amount = 5_000_000;

    let (investor_return, platform_fee) =
        client.calculate_profit(&investment_amount, &payment_amount);

    assert_eq!(platform_fee, 0, "No fee when no profit");
    assert_eq!(
        investor_return, payment_amount,
        "Investor gets full payment"
    );
    assert_eq!(investor_return + platform_fee, payment_amount);
}

#[test]
fn test_one_stroop_profit() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    // Smallest possible profit: 1 stroop
    let investment_amount = 1_000_000;
    let payment_amount = 1_000_001;

    let (investor_return, platform_fee) =
        client.calculate_profit(&investment_amount, &payment_amount);

    // 2% of 1 = 0.02 -> rounds down to 0
    assert_eq!(platform_fee, 0);
    assert_eq!(investor_return, 1_000_001);
    assert_eq!(investor_return + platform_fee, payment_amount);
}

#[test]
fn test_minimum_fee_threshold() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    // Find minimum profit that generates 1 stroop fee at 2%
    // fee = profit * 200 / 10000 >= 1
    // profit >= 10000 / 200 = 50
    let investment_amount = 1_000_000;
    let payment_amount = 1_000_050; // Profit of 50

    let (investor_return, platform_fee) =
        client.calculate_profit(&investment_amount, &payment_amount);

    assert_eq!(platform_fee, 1, "Minimum profit for 1 stroop fee");
    assert_eq!(investor_return, 1_000_049);
    assert_eq!(investor_return + platform_fee, payment_amount);
}

#[test]
fn test_fee_just_below_threshold() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    // Profit of 49 should round down to 0 fee
    let investment_amount = 1_000_000;
    let payment_amount = 1_000_049;

    let (investor_return, platform_fee) =
        client.calculate_profit(&investment_amount, &payment_amount);

    assert_eq!(platform_fee, 0, "Just below threshold rounds to 0");
    assert_eq!(investor_return, 1_000_049);
    assert_eq!(investor_return + platform_fee, payment_amount);
}

#[test]
fn test_maximum_safe_i128_values() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    // Test with very large but safe values (well below i128::MAX)
    let investment_amount = 1_000_000_000_000_000i128; // 1 quadrillion
    let payment_amount = 1_100_000_000_000_000i128; // 10% profit

    let (investor_return, platform_fee) =
        client.calculate_profit(&investment_amount, &payment_amount);

    // Profit: 100 trillion, Fee: 2% = 2 trillion
    assert_eq!(platform_fee, 2_000_000_000_000);
    assert_eq!(investor_return, 1_098_000_000_000_000);
    assert_eq!(investor_return + platform_fee, payment_amount);
}

#[test]
fn test_treasury_split_all_edge_cases() {
    // Test 0% treasury share
    let (treasury, remaining) = calculate_treasury_split(1000, 0);
    assert_eq!(treasury, 0);
    assert_eq!(remaining, 1000);
    assert_eq!(treasury + remaining, 1000);

    // Test 100% treasury share
    let (treasury, remaining) = calculate_treasury_split(1000, 10000);
    assert_eq!(treasury, 1000);
    assert_eq!(remaining, 0);
    assert_eq!(treasury + remaining, 1000);

    // Test 50% treasury share
    let (treasury, remaining) = calculate_treasury_split(1000, 5000);
    assert_eq!(treasury, 500);
    assert_eq!(remaining, 500);
    assert_eq!(treasury + remaining, 1000);

    // Test odd amount with 50% share (rounding)
    let (treasury, remaining) = calculate_treasury_split(1001, 5000);
    assert_eq!(treasury, 500); // floor(1001 * 5000 / 10000) = 500
    assert_eq!(remaining, 501); // 1001 - 500 = 501
    assert_eq!(treasury + remaining, 1001);

    // Test 1% treasury share
    let (treasury, remaining) = calculate_treasury_split(10000, 100);
    assert_eq!(treasury, 100); // 1% of 10000
    assert_eq!(remaining, 9900);
    assert_eq!(treasury + remaining, 10000);

    // Test 99% treasury share
    let (treasury, remaining) = calculate_treasury_split(10000, 9900);
    assert_eq!(treasury, 9900); // 99% of 10000
    assert_eq!(remaining, 100);
    assert_eq!(treasury + remaining, 10000);
}

#[test]
fn test_treasury_split_with_small_fees() {
    // Test treasury split with very small fees
    let (treasury, remaining) = calculate_treasury_split(1, 5000);
    assert_eq!(treasury, 0); // floor(1 * 5000 / 10000) = 0
    assert_eq!(remaining, 1);
    assert_eq!(treasury + remaining, 1);

    let (treasury, remaining) = calculate_treasury_split(2, 5000);
    assert_eq!(treasury, 1); // floor(2 * 5000 / 10000) = 1
    assert_eq!(remaining, 1);
    assert_eq!(treasury + remaining, 2);

    let (treasury, remaining) = calculate_treasury_split(3, 5000);
    assert_eq!(treasury, 1); // floor(3 * 5000 / 10000) = 1
    assert_eq!(remaining, 2);
    assert_eq!(treasury + remaining, 3);
}

#[test]
fn test_treasury_split_negative_fee() {
    // Negative fee should return (0, 0)
    let (treasury, remaining) = calculate_treasury_split(-100, 5000);
    assert_eq!(treasury, 0);
    assert_eq!(remaining, 0);
}

#[test]
fn test_treasury_split_over_max_share() {
    // Share > 10000 (100%) should give all to treasury
    let (treasury, remaining) = calculate_treasury_split(1000, 15000);
    assert_eq!(treasury, 1000);
    assert_eq!(remaining, 0);
}

#[test]
fn test_validate_inputs_edge_cases() {
    // Valid: zero investment, zero payment
    assert!(validate_calculation_inputs(0, 0).is_ok());

    // Valid: zero investment, positive payment
    assert!(validate_calculation_inputs(0, 100).is_ok());

    // Valid: positive investment, zero payment
    assert!(validate_calculation_inputs(100, 0).is_ok());

    // Invalid: negative investment
    assert!(validate_calculation_inputs(-1, 100).is_err());

    // Invalid: negative payment
    assert!(validate_calculation_inputs(100, -1).is_err());

    // Invalid: both negative
    assert!(validate_calculation_inputs(-100, -100).is_err());
}

#[test]
fn test_profit_with_various_fee_rates() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);

    let investment_amount = 10_000;
    let payment_amount = 11_000; // 1000 profit

    // Test various fee rates (max allowed is 1000 bps = 10%)
    let fee_rates = vec![
        (0, 0),      // 0% fee
        (50, 5),     // 0.5% fee
        (100, 10),   // 1% fee
        (200, 20),   // 2% fee (default)
        (500, 50),   // 5% fee
        (750, 75),   // 7.5% fee
        (1000, 100), // 10% fee (max)
    ];

    for (fee_bps, expected_fee) in fee_rates {
        client.set_platform_fee(&fee_bps);
        let (investor_return, platform_fee) =
            client.calculate_profit(&investment_amount, &payment_amount);

        assert_eq!(
            platform_fee, expected_fee,
            "Fee mismatch for rate {}bps",
            fee_bps
        );
        assert_eq!(
            investor_return + platform_fee,
            payment_amount,
            "Dust detected for rate {}bps",
            fee_bps
        );
    }
}

#[test]
fn test_sequential_calculations_consistency() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let investment_amount = 5_000;
    let payment_amount = 5_500;

    // Calculate multiple times - should be consistent
    let result1 = client.calculate_profit(&investment_amount, &payment_amount);
    let result2 = client.calculate_profit(&investment_amount, &payment_amount);
    let result3 = client.calculate_profit(&investment_amount, &payment_amount);

    assert_eq!(result1, result2, "Results should be consistent");
    assert_eq!(result2, result3, "Results should be consistent");
}

#[test]
fn test_profit_calculation_symmetry() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    // Test that profit calculation is symmetric for same profit amount
    let test_cases = vec![
        (1000, 1100), // 100 profit
        (2000, 2100), // 100 profit
        (5000, 5100), // 100 profit
    ];

    let mut fees = vec![];
    for (investment, payment) in test_cases {
        let (_, platform_fee) = client.calculate_profit(&investment, &payment);
        fees.push(platform_fee);
    }

    // All should have same fee since profit is same
    assert_eq!(fees[0], fees[1], "Same profit should yield same fee");
    assert_eq!(fees[1], fees[2], "Same profit should yield same fee");
}

#[test]
fn test_zero_investment_edge_case() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    // Zero investment, positive payment (all profit)
    let investment_amount = 0;
    let payment_amount = 1000;

    let (investor_return, platform_fee) =
        client.calculate_profit(&investment_amount, &payment_amount);

    // All payment is profit, so fee = 2% of 1000 = 20
    assert_eq!(platform_fee, 20);
    assert_eq!(investor_return, 980);
    assert_eq!(investor_return + platform_fee, payment_amount);
}

#[test]
fn test_payment_one_less_than_investment() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    // Payment is 1 less than investment (minimal loss)
    let investment_amount = 10_000;
    let payment_amount = 9_999;

    let (investor_return, platform_fee) =
        client.calculate_profit(&investment_amount, &payment_amount);

    assert_eq!(platform_fee, 0, "No fee on loss");
    assert_eq!(investor_return, 9_999, "Investor gets payment amount");
}

#[test]
fn test_payment_one_more_than_investment() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    // Payment is 1 more than investment (minimal profit)
    let investment_amount = 10_000;
    let payment_amount = 10_001;

    let (investor_return, platform_fee) =
        client.calculate_profit(&investment_amount, &payment_amount);

    // 2% of 1 = 0.02 -> rounds down to 0
    assert_eq!(platform_fee, 0);
    assert_eq!(investor_return, 10_001);
    assert_eq!(investor_return + platform_fee, payment_amount);
}

#[test]
fn test_rounding_at_various_profit_levels() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    // Test rounding behavior at different profit levels
    let base_investment = 100_000;

    // Profit levels that test rounding boundaries
    let profit_levels = vec![
        49,  // Just below minimum fee threshold
        50,  // Minimum for 1 stroop fee
        51,  // Just above minimum
        99,  // Just below 2 stroop fee
        100, // Exactly 2 stroop fee
        101, // Just above 2 stroop fee
        149, // Just below 3 stroop fee
        150, // Exactly 3 stroop fee
    ];

    for profit in profit_levels {
        let payment = base_investment + profit;
        let (investor_return, platform_fee) = client.calculate_profit(&base_investment, &payment);

        // Verify no dust
        assert_eq!(
            investor_return + platform_fee,
            payment,
            "Dust detected at profit level {}",
            profit
        );

        // Verify fee is correct (2% of profit, rounded down)
        let expected_fee = (profit * 200) / 10000;
        assert_eq!(
            platform_fee, expected_fee,
            "Fee mismatch at profit level {}",
            profit
        );
    }
}
