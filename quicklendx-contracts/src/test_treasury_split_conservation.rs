#![cfg(all(test, feature = "fuzz-tests"))]

use crate::errors::QuickLendXError;
use crate::profits::{calculate_treasury_split_checked, verify_no_dust};
use proptest::prelude::*;

const MAX_NON_OVERFLOW_PLATFORM_FEE: i128 = i128::MAX / 10_000;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1000))]

    #[test]
    fn test_treasury_split_conservation_for_valid_inputs(
        platform_fee in 0i128..=MAX_NON_OVERFLOW_PLATFORM_FEE,
        treasury_share_bps in 0i128..=10_000,
    ) {
        let result = calculate_treasury_split_checked(platform_fee, treasury_share_bps);
        prop_assert!(result.is_ok(), "valid bounded inputs must not overflow: {:?}", result);
        let (treasury_part, remainder) = result.unwrap();

        prop_assert_eq!(
            treasury_part + remainder,
            platform_fee,
            "treasury split must conserve the full platform fee"
        );
        prop_assert!(treasury_part >= 0, "treasury part must not be negative");
        prop_assert!(remainder >= 0, "remainder must not be negative");
        prop_assert!(treasury_part <= platform_fee, "treasury part must not exceed platform fee");
        prop_assert!(remainder <= platform_fee, "remainder must not exceed platform fee");
        prop_assert!(
            verify_no_dust(remainder, treasury_part, platform_fee),
            "treasury split must remain dust-free"
        );
    }

    #[test]
    fn test_treasury_split_checked_overflow_returns_error(
        treasury_share_bps in 2i128..10_000,
        overflow_offset in 1i128..=1_000,
    ) {
        let platform_fee = (i128::MAX / treasury_share_bps)
            .saturating_add(overflow_offset)
            .min(i128::MAX);
        let result = calculate_treasury_split_checked(platform_fee, treasury_share_bps);

        prop_assert_eq!(result, Err(QuickLendXError::ArithmeticOverflow));
        prop_assert!(platform_fee.checked_mul(treasury_share_bps).is_none());
    }
}

#[test]
fn test_treasury_split_conservation_edge_cases() {
    for (platform_fee, treasury_share_bps, expected) in [
        (0, 0, (0, 0)),
        (0, 5_000, (0, 0)),
        (1, 0, (0, 1)),
        (1, 10_000, (1, 0)),
        (101, 5_000, (50, 51)),
        (100, 3_333, (33, 67)),
        (
            MAX_NON_OVERFLOW_PLATFORM_FEE,
            9_999,
            (
                MAX_NON_OVERFLOW_PLATFORM_FEE * 9_999 / 10_000,
                MAX_NON_OVERFLOW_PLATFORM_FEE - (MAX_NON_OVERFLOW_PLATFORM_FEE * 9_999 / 10_000),
            ),
        ),
    ] {
        let (treasury_part, remainder) =
            calculate_treasury_split_checked(platform_fee, treasury_share_bps)
                .expect("edge case should be within checked arithmetic bounds");

        assert_eq!((treasury_part, remainder), expected);
        assert_eq!(treasury_part + remainder, platform_fee);
        assert!(treasury_part >= 0);
        assert!(remainder >= 0);
        assert!(treasury_part <= platform_fee);
        assert!(remainder <= platform_fee);
        assert!(verify_no_dust(remainder, treasury_part, platform_fee));
    }
}
