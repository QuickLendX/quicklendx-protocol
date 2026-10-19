#![cfg(feature = "fuzz-tests")]

use proptest::prelude::*;
use crate::verification::{compute_investor_tier_from_stats, InvestorTier};

proptest! {
    #![proptest_config(ProptestConfig::with_cases(5000))]

    #[test]
    fn test_compute_investor_tier_properties(
        total_invested in 0i128..10_000_000i128,
        successful_investments in 0u32..200u32,
        defaulted_investments in 0u32..50u32,
        risk_score in 0u32..=100u32,
    ) {
        let result = compute_investor_tier_from_stats(
            total_invested,
            successful_investments,
            defaulted_investments,
            risk_score,
        ).expect("compute_investor_tier_from_stats should not fail for risk_score <= 100");

        let total_active_or_completed = successful_investments.saturating_add(defaulted_investments);
        let default_rate_pct = if total_active_or_completed > 0 {
            (defaulted_investments as u64)
                .saturating_mul(100)
                .checked_div(total_active_or_completed as u64)
                .unwrap_or(0) as u32
        } else {
            0
        };

        // Assert strictly the same tier promotion thresholds defined in verification.rs
        let expected = if risk_score <= 10
            && total_invested >= 5_000_000
            && successful_investments >= 50
            && default_rate_pct <= 5
        {
            InvestorTier::VIP
        } else if risk_score <= 20
            && total_invested >= 1_000_000
            && successful_investments >= 20
            && default_rate_pct <= 10
        {
            InvestorTier::Platinum
        } else if risk_score <= 40
            && total_invested >= 100_000
            && successful_investments >= 10
            && default_rate_pct <= 15
        {
            InvestorTier::Gold
        } else if risk_score <= 60
            && total_invested >= 10_000
            && successful_investments >= 3
            && default_rate_pct <= 25
        {
            InvestorTier::Silver
        } else {
            InvestorTier::Basic
        };

        prop_assert_eq!(
            result,
            expected,
            "Investor tier calculation mismatch: total_invested={}, successful={}, defaulted={}, default_rate_pct={}, risk_score={}",
            total_invested, successful_investments, defaulted_investments, default_rate_pct, risk_score
        );
    }
}
