#[cfg(all(test, feature = "fuzz-tests"))]
mod test_rounding_direction_pinned {
    use crate::profits::PlatformFee;
    use proptest::prelude::*;

    proptest! {
        // Same inputs always round the same way; deterministic policy.
        #[test]
        fn rounding_direction_pinned(
            investment in 0i128..1_000_000_000_000_000i128,
            payment in 0i128..2_000_000_000_000_000i128,
            fee_bps in 0i128..10_000i128,
        ) {
            // First call
            let (investor_return_1, platform_fee_1) =
                PlatformFee::calculate_with_fee_bps(investment, payment, fee_bps);

            // Second call
            let (investor_return_2, platform_fee_2) =
                PlatformFee::calculate_with_fee_bps(investment, payment, fee_bps);

            // Must be deterministic
            prop_assert_eq!(platform_fee_1, platform_fee_2, "Fee calculation must be deterministic");
            prop_assert_eq!(investor_return_1, investor_return_2, "Return calculation must be deterministic");

            if payment > investment {
                let profit = payment - investment;
                let expected_fee = profit.saturating_mul(fee_bps) / 10_000;
                prop_assert_eq!(platform_fee_1, expected_fee, "Fee must round down using integer division");
            } else {
                prop_assert_eq!(platform_fee_1, 0, "Fee must be zero for no profit");
            }

            // No dust guarantee
            prop_assert_eq!(investor_return_1.saturating_add(platform_fee_1), payment.max(0), "No dust invariant violated");
        }
    }
}
