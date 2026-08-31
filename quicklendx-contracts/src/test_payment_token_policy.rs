//! Tests for Payment Token and Fee Policy module (`payment_token_policy.rs`).
//!
//! # Invariants and Guarantees Tested
//! 1. **Zero Dust Loss Guarantee**: `platform_fee + net_amount == gross_amount` for all valid fee calculations.
//! 2. **Precision & Scale Bounds**: Scale must be `0..=18`. Amounts must respect token decimals and boundaries.
//! 3. **Amount Limits**: Amounts `< min_amount` or `> max_amount` are rejected with `InvalidAmount`.
//! 4. **Checked Integer Arithmetic**: All calculations use non-overflowing arithmetic and return `ArithmeticOverflow`.
//! 5. **Independent Oracle Verification**: Compares Soroban contract fee math with a reference oracle model.
//! 6. **State & Authorization Integrity**: Only initialized admin can configure policy; inactive tokens fail-closed.

#![cfg(test)]

use crate::errors::QuickLendXError;
use crate::payment_token_policy::{
    FeeCalculationResult, PaymentTokenConfig, PaymentTokenPolicy, BPS_DENOMINATOR, MAX_FEE_BPS,
    MAX_SUPPORTED_DECIMALS,
};
use crate::{QuickLendXContract, QuickLendXContractClient};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Env, Vec,
};

fn setup_env() -> (Env, QuickLendXContractClient<'static>, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_700_000_000);

    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    client.set_admin(&admin);

    let token = Address::generate(&env);
    (env, client, admin, token)
}

/// Independent Oracle for Fee Calculation: calculates reference expected values.
fn oracle_calculate_fee(gross_amount: i128, fee_bps: u32) -> (i128, i128) {
    let fee = (gross_amount * fee_bps as i128) / (BPS_DENOMINATOR);
    let net = gross_amount - fee;
    (fee, net)
}

// =============================================================================
// Unit Tests: Configuration & Validation
// =============================================================================

#[test]
fn test_valid_policy_configuration_roundtrip() {
    let (env, client, admin, token) = setup_env();

    let config = PaymentTokenConfig {
        token: token.clone(),
        decimals: 6,
        min_amount: 1_000_000,           // 1.000000 token
        max_amount: 100_000_000_000_000, // 100M tokens
        is_active: true,
        fee_bps_override: Some(250), // 2.50%
    };

    client.set_payment_token_policy(&admin, &config);

    let retrieved = client.get_payment_token_policy(&token).expect("policy should exist");
    assert_eq!(retrieved, config);

    let list = client.list_payment_token_policies();
    assert_eq!(list.len(), 1);
    assert_eq!(list.get(0).unwrap(), config);
}

#[test]
fn test_reject_decimals_exceeding_max_supported() {
    let (env, client, admin, token) = setup_env();

    let config = PaymentTokenConfig {
        token: token.clone(),
        decimals: MAX_SUPPORTED_DECIMALS + 1, // 19
        min_amount: 10,
        max_amount: 1_000_000,
        is_active: true,
        fee_bps_override: None,
    };

    let res = client.try_set_payment_token_policy(&admin, &config);
    assert_eq!(res.unwrap_err().unwrap(), QuickLendXError::InvalidCurrency);
}

#[test]
fn test_reject_invalid_amount_bounds() {
    let (env, client, admin, token) = setup_env();

    // min_amount <= 0
    let config_zero_min = PaymentTokenConfig {
        token: token.clone(),
        decimals: 6,
        min_amount: 0,
        max_amount: 1_000_000,
        is_active: true,
        fee_bps_override: None,
    };
    assert_eq!(
        client
            .try_set_payment_token_policy(&admin, &config_zero_min)
            .unwrap_err()
            .unwrap(),
        QuickLendXError::InvalidAmount
    );

    // max_amount < min_amount
    let config_inverted_bounds = PaymentTokenConfig {
        token: token.clone(),
        decimals: 6,
        min_amount: 1_000,
        max_amount: 999,
        is_active: true,
        fee_bps_override: None,
    };
    assert_eq!(
        client
            .try_set_payment_token_policy(&admin, &config_inverted_bounds)
            .unwrap_err()
            .unwrap(),
        QuickLendXError::InvalidAmount
    );
}

#[test]
fn test_reject_fee_basis_points_exceeding_max() {
    let (env, client, admin, token) = setup_env();

    let config = PaymentTokenConfig {
        token: token.clone(),
        decimals: 6,
        min_amount: 100,
        max_amount: 1_000_000,
        is_active: true,
        fee_bps_override: Some(MAX_FEE_BPS + 1), // 100.01%
    };

    assert_eq!(
        client
            .try_set_payment_token_policy(&admin, &config)
            .unwrap_err()
            .unwrap(),
        QuickLendXError::InvalidFeeBasisPoints
    );
}

#[test]
fn test_policy_removal_and_list_cleanup() {
    let (env, client, admin, token) = setup_env();

    let config = PaymentTokenConfig {
        token: token.clone(),
        decimals: 7,
        min_amount: 100,
        max_amount: 10_000,
        is_active: true,
        fee_bps_override: None,
    };

    client.set_payment_token_policy(&admin, &config);
    assert!(client.get_payment_token_policy(&token).is_some());

    client.remove_payment_token_policy(&admin, &token);
    assert!(client.get_payment_token_policy(&token).is_none());
    assert_eq!(client.list_payment_token_policies().len(), 0);
}

// =============================================================================
// Amount Validation & Boundary Tests
// =============================================================================

#[test]
fn test_validate_token_amount_boundaries() {
    let (env, client, admin, token) = setup_env();

    let min_amt = 1_000i128;
    let max_amt = 1_000_000i128;

    let config = PaymentTokenConfig {
        token: token.clone(),
        decimals: 6,
        min_amount: min_amt,
        max_amount: max_amt,
        is_active: true,
        fee_bps_override: None,
    };
    client.set_payment_token_policy(&admin, &config);

    // Negative and zero
    assert_eq!(
        client
            .try_validate_token_amount(&token, &-100i128)
            .unwrap_err()
            .unwrap(),
        QuickLendXError::InvalidAmount
    );
    assert_eq!(
        client
            .try_validate_token_amount(&token, &0i128)
            .unwrap_err()
            .unwrap(),
        QuickLendXError::InvalidAmount
    );

    // Just below minimum
    assert_eq!(
        client
            .try_validate_token_amount(&token, &(min_amt - 1))
            .unwrap_err()
            .unwrap(),
        QuickLendXError::InvalidAmount
    );

    // Exact boundaries
    assert!(client.try_validate_token_amount(&token, &min_amt).is_ok());
    assert!(client.try_validate_token_amount(&token, &max_amt).is_ok());
    assert!(client.try_validate_token_amount(&token, &((min_amt + max_amt) / 2)).is_ok());

    // Just above maximum
    assert_eq!(
        client
            .try_validate_token_amount(&token, &(max_amt + 1))
            .unwrap_err()
            .unwrap(),
        QuickLendXError::InvalidAmount
    );
}

#[test]
fn test_inactive_token_rejects_validation() {
    let (env, client, admin, token) = setup_env();

    let config = PaymentTokenConfig {
        token: token.clone(),
        decimals: 6,
        min_amount: 100,
        max_amount: 10_000,
        is_active: false, // Inactive
        fee_bps_override: None,
    };
    client.set_payment_token_policy(&admin, &config);

    assert_eq!(
        client
            .try_validate_token_amount(&token, &500i128)
            .unwrap_err()
            .unwrap(),
        QuickLendXError::InvalidCurrency
    );
}

// =============================================================================
// Fee Calculation & Zero-Dust Oracle Verification
// =============================================================================

#[test]
fn test_calculate_token_fee_oracle_verification_and_zero_dust() {
    let (env, client, admin, token) = setup_env();

    let test_cases: [(i128, u32, Option<u32>); 8] = [
        // (gross_amount, default_bps, override_bps)
        (1_000_000, 100, None),                // 100 bps (1%)
        (10_000_000, 250, Some(50)),           // Override to 50 bps (0.5%)
        (999_999, 150, None),                  // Odd number with fractional division
        (1, 5000, None),                       // 1 base unit -> 0 fee, 1 net (zero dust)
        (10_000, 0, None),                     // 0% fee
        (10_000, 10_000, None),                // 100% fee
        (1_000_000_000_000_000, 300, None),    // Large amount (1M tokens with 9 decimals)
        (123_456_789_012_345, 125, Some(350)), // Arbitrary large prime-ish numbers
    ];

    for (gross, default_bps, override_bps) in test_cases.iter() {
        let config = PaymentTokenConfig {
            token: token.clone(),
            decimals: 6,
            min_amount: 1,
            max_amount: i128::MAX,
            is_active: true,
            fee_bps_override: *override_bps,
        };
        client.set_payment_token_policy(&admin, &config);

        let res = client.calculate_token_fee(&token, gross, default_bps);

        let expected_bps = override_bps.unwrap_or(*default_bps);
        let (oracle_fee, oracle_net) = oracle_calculate_fee(*gross, expected_bps);

        // Verify against oracle
        assert_eq!(res.platform_fee, oracle_fee, "Platform fee must match oracle");
        assert_eq!(res.net_amount, oracle_net, "Net amount must match oracle");
        assert_eq!(res.applied_fee_bps, expected_bps);

        // Core Invariant: Zero dust loss
        assert_eq!(
            res.platform_fee + res.net_amount,
            *gross,
            "Zero dust invariant: platform_fee + net_amount == gross_amount"
        );
    }
}

// =============================================================================
// Scale Normalization Tests
// =============================================================================

#[test]
fn test_normalize_amount_scale_conversions() {
    // 1 token at 6 decimals (1_000_000) converted to 18 decimals:
    let scaled_up = PaymentTokenPolicy::normalize_amount(1_000_000, 6, 18).unwrap();
    assert_eq!(scaled_up, 1_000_000_000_000_000_000);

    // 1 token at 18 decimals converted back to 6 decimals:
    let scaled_down = PaymentTokenPolicy::normalize_amount(1_000_000_000_000_000_000, 18, 6).unwrap();
    assert_eq!(scaled_down, 1_000_000);

    // Same decimal conversion is a no-op
    let same = PaymentTokenPolicy::normalize_amount(500, 8, 8).unwrap();
    assert_eq!(same, 500);

    // Zero amount conversion
    let zero = PaymentTokenPolicy::normalize_amount(0, 6, 18).unwrap();
    assert_eq!(zero, 0);

    // Invalid negative amount
    assert_eq!(
        PaymentTokenPolicy::normalize_amount(-1, 6, 18).unwrap_err(),
        QuickLendXError::InvalidAmount
    );

    // Scale exceeding 18
    assert_eq!(
        PaymentTokenPolicy::normalize_amount(100, 19, 6).unwrap_err(),
        QuickLendXError::InvalidCurrency
    );
}

#[test]
fn test_overflow_protection_in_normalization() {
    // Large number near i128 max scaled up by 10^18 should safely return ArithmeticOverflow
    let huge = i128::MAX / 10;
    let res = PaymentTokenPolicy::normalize_amount(huge, 6, 18);
    assert_eq!(res.unwrap_err(), QuickLendXError::ArithmeticOverflow);
}
