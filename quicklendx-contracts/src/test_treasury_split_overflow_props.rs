#![cfg(all(test, feature = "fuzz-tests"))]

//! Property tests for the FeeManager treasury split and revenue distribution math.
//!
//! Specifically tests:
//! - `profits::calculate_treasury_split_checked`
//! - `fees::FeeManager::validate_revenue_shares`
//! - Overflow bounds, dust-free invariants, and correct error returns.

use crate::errors::QuickLendXError;
use crate::fees::FeeManager;
use crate::profits::{calculate_treasury_split_checked, verify_no_dust};
use proptest::prelude::*;

// ---------------------------------------------------------------------------
// Constants & Generators
// ---------------------------------------------------------------------------

const MAX_AMOUNT: i128 = i128::MAX;

/// Fee amount including small, medium, and extreme boundary values.
fn fee_amount_extended() -> impl Strategy<Value = i128> {
    prop_oneof![
        Just(0i128),
        Just(1i128),
        Just(2i128),
        Just(MAX_AMOUNT - 1),
        Just(MAX_AMOUNT),
        0..=1_000_000_000_000_i128, // Common values
        // Very large values
        (i128::MAX - 1_000_000_000)..=i128::MAX,
    ]
}

/// A valid tuple of (treasury_bps, developer_bps, platform_bps) that sum to 10,000.
fn valid_bps_split() -> impl Strategy<Value = (u32, u32, u32)> {
    (0u32..=10_000).prop_flat_map(|t| {
        let remaining = 10_000 - t;
        (0u32..=remaining).prop_map(move |d| {
            let p = 10_000 - t - d;
            (t, d, p)
        })
    })
}

// ---------------------------------------------------------------------------
// Property Tests
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1000))]

    /// Assert `validate_revenue_shares` rejects any split whose bps do not sum to the denominator.
    #[test]
    fn test_validate_revenue_shares_invalid_sums(
        t in 0u32..=20_000,
        d in 0u32..=20_000,
        p in 0u32..=20_000,
    ) {
        let sum = (t as u64) + (d as u64) + (p as u64);
        let result = FeeManager::validate_revenue_shares(t, d, p);

        if t > 10_000 || d > 10_000 || p > 10_000 {
            prop_assert_eq!(result, Err(QuickLendXError::InvalidFeeConfiguration));
        } else if sum != 10_000 {
            prop_assert_eq!(result, Err(QuickLendXError::InvalidAmount));
        } else {
            prop_assert_eq!(result, Ok(()));
        }
    }

    /// Assert `validate_revenue_shares` accepts valid sums.
    #[test]
    fn test_validate_revenue_shares_valid_sums(
        (t, d, p) in valid_bps_split()
    ) {
        prop_assert_eq!(FeeManager::validate_revenue_shares(t, d, p), Ok(()));
    }

    /// Assert the share-sum identity and no panics.
    /// Also asserts `verify_no_dust` holds across the generated range.
    #[test]
    fn test_calculate_treasury_split_properties(
        platform_fee in fee_amount_extended(),
        treasury_share_bps in 0i128..=10_000,
    ) {
        let result = calculate_treasury_split_checked(platform_fee, treasury_share_bps);

        match result {
            Ok((treasury_amount, remaining)) => {
                // Invariant: treasury_share + remainder == platform_fee
                prop_assert_eq!(
                    treasury_amount + remaining, 
                    platform_fee,
                    "Sum invariant violated: {} + {} != {}", 
                    treasury_amount, remaining, platform_fee
                );

                // No negative distributions
                prop_assert!(treasury_amount >= 0);
                prop_assert!(remaining >= 0);

                // verify_no_dust checks: investor_return + platform_fee == payment_amount
                // We map this to: remaining + treasury_amount == platform_fee
                prop_assert!(verify_no_dust(remaining, treasury_amount, platform_fee));
            }
            Err(QuickLendXError::ArithmeticOverflow) => {
                // Verify that the overflow was indeed mathematically expected.
                prop_assert!(platform_fee.checked_mul(treasury_share_bps).is_none());
            }
            Err(e) => {
                prop_assert!(false, "Unexpected error: {:?}", e);
            }
        }
    }

    /// Assert calculate_treasury_split_checked returns Err(ArithmeticOverflow) for extreme inputs.
    #[test]
    fn test_calculate_treasury_split_overflows_never_panics(
        high_bits in 0i128..=1_000,
        treasury_share_bps in 2i128..=9_999, // Needs to be > 1 to multiply and overflow
    ) {
        let platform_fee = i128::MAX - high_bits;
        
        let result = calculate_treasury_split_checked(platform_fee, treasury_share_bps);
        
        // As long as treasury_share_bps > 1, the multiplication `(i128::MAX - high_bits) * bps`
        // will overflow an i128.
        prop_assert_eq!(result, Err(QuickLendXError::ArithmeticOverflow));
        
        // Double check using checked_mul
        prop_assert!(platform_fee.checked_mul(treasury_share_bps).is_none());
    }
}
