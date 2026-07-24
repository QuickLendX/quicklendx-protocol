#[cfg(test)]
mod test_tier_boundary {
    use crate::errors::QuickLendXError;
    use crate::verification::{compute_investor_tier_from_stats, InvestorTier};

    // ── VIP tier boundaries ─────────────────────────────────────────────

    #[test]
    fn vip_risk_score_at_boundary_returns_vip() {
        // risk_score == 10 is the maximum allowed for VIP
        let result = compute_investor_tier_from_stats(5_000_000, 50, 0, 10);
        assert_eq!(result, Ok(InvestorTier::VIP));
    }

    #[test]
    fn vip_risk_score_above_boundary_returns_platinum() {
        // risk_score == 11 is one above the VIP max → Platinum
        let result = compute_investor_tier_from_stats(5_000_000, 50, 0, 11);
        assert_eq!(result, Ok(InvestorTier::Platinum));
    }

    #[test]
    fn vip_total_invested_at_boundary_returns_vip() {
        let result = compute_investor_tier_from_stats(5_000_000, 50, 0, 10);
        assert_eq!(result, Ok(InvestorTier::VIP));
    }

    #[test]
    fn vip_total_invested_below_boundary_returns_platinum() {
        let result = compute_investor_tier_from_stats(4_999_999, 50, 0, 10);
        assert_eq!(result, Ok(InvestorTier::Platinum));
    }

    #[test]
    fn vip_successful_investments_at_boundary_returns_vip() {
        let result = compute_investor_tier_from_stats(5_000_000, 50, 0, 10);
        assert_eq!(result, Ok(InvestorTier::VIP));
    }

    #[test]
    fn vip_successful_investments_below_boundary_returns_platinum() {
        let result = compute_investor_tier_from_stats(5_000_000, 49, 0, 10);
        assert_eq!(result, Ok(InvestorTier::Platinum));
    }

    #[test]
    fn vip_default_rate_at_boundary_returns_vip() {
        // 1 default out of 20 total = 5% which is at the VIP max
        let result = compute_investor_tier_from_stats(5_000_000, 19, 1, 10);
        assert_eq!(result, Ok(InvestorTier::VIP));
    }

    #[test]
    fn vip_default_rate_above_boundary_returns_platinum() {
        // 2 defaults out of 22 total = 9% → above VIP max (5%), within Platinum max (10%)
        let result = compute_investor_tier_from_stats(5_000_000, 20, 2, 10);
        assert_eq!(result, Ok(InvestorTier::Platinum));
    }

    // ── Platinum tier boundaries ─────────────────────────────────────────

    #[test]
    fn platinum_risk_score_at_boundary_returns_platinum() {
        let result = compute_investor_tier_from_stats(1_000_000, 20, 0, 20);
        assert_eq!(result, Ok(InvestorTier::Platinum));
    }

    #[test]
    fn platinum_risk_score_above_boundary_returns_gold() {
        let result = compute_investor_tier_from_stats(1_000_000, 20, 0, 21);
        assert_eq!(result, Ok(InvestorTier::Gold));
    }

    #[test]
    fn platinum_total_invested_below_boundary_returns_gold() {
        let result = compute_investor_tier_from_stats(999_999, 20, 0, 20);
        assert_eq!(result, Ok(InvestorTier::Gold));
    }

    #[test]
    fn platinum_successful_investments_below_boundary_returns_gold() {
        let result = compute_investor_tier_from_stats(1_000_000, 19, 0, 20);
        assert_eq!(result, Ok(InvestorTier::Gold));
    }

    #[test]
    fn platinum_default_rate_above_boundary_returns_gold() {
        // 3 defaults out of 27 total = 11% which is above 10%
        let result = compute_investor_tier_from_stats(1_000_000, 24, 3, 20);
        assert_eq!(result, Ok(InvestorTier::Gold));
    }

    // ── Gold tier boundaries ─────────────────────────────────────────────

    #[test]
    fn gold_risk_score_at_boundary_returns_gold() {
        let result = compute_investor_tier_from_stats(100_000, 10, 0, 40);
        assert_eq!(result, Ok(InvestorTier::Gold));
    }

    #[test]
    fn gold_risk_score_above_boundary_returns_silver() {
        let result = compute_investor_tier_from_stats(100_000, 10, 0, 41);
        assert_eq!(result, Ok(InvestorTier::Silver));
    }

    #[test]
    fn gold_total_invested_below_boundary_returns_silver() {
        let result = compute_investor_tier_from_stats(99_999, 10, 0, 40);
        assert_eq!(result, Ok(InvestorTier::Silver));
    }

    #[test]
    fn gold_successful_investments_below_boundary_returns_silver() {
        let result = compute_investor_tier_from_stats(100_000, 9, 0, 40);
        assert_eq!(result, Ok(InvestorTier::Silver));
    }

    #[test]
    fn gold_default_rate_above_boundary_returns_silver() {
        // 4 defaults out of 24 total = 16% which is above 15%
        let result = compute_investor_tier_from_stats(100_000, 20, 4, 40);
        assert_eq!(result, Ok(InvestorTier::Silver));
    }

    // ── Silver tier boundaries ───────────────────────────────────────────

    #[test]
    fn silver_risk_score_at_boundary_returns_silver() {
        let result = compute_investor_tier_from_stats(10_000, 3, 0, 60);
        assert_eq!(result, Ok(InvestorTier::Silver));
    }

    #[test]
    fn silver_risk_score_above_boundary_returns_basic() {
        let result = compute_investor_tier_from_stats(10_000, 3, 0, 61);
        assert_eq!(result, Ok(InvestorTier::Basic));
    }

    #[test]
    fn silver_total_invested_below_boundary_returns_basic() {
        let result = compute_investor_tier_from_stats(9_999, 3, 0, 60);
        assert_eq!(result, Ok(InvestorTier::Basic));
    }

    #[test]
    fn silver_successful_investments_below_boundary_returns_basic() {
        let result = compute_investor_tier_from_stats(10_000, 2, 0, 60);
        assert_eq!(result, Ok(InvestorTier::Basic));
    }

    #[test]
    fn silver_default_rate_above_boundary_returns_basic() {
        // 5 defaults out of 18 total = 27% which is above 25%
        let result = compute_investor_tier_from_stats(10_000, 13, 5, 60);
        assert_eq!(result, Ok(InvestorTier::Basic));
    }

    // ── Invalid risk score ───────────────────────────────────────────────

    #[test]
    fn risk_score_above_100_returns_error() {
        let result = compute_investor_tier_from_stats(0, 0, 0, 101);
        assert_eq!(result, Err(QuickLendXError::InvalidAmount));
    }

    // ── Edge: zero stats returns Basic ────────────────────────────────────

    #[test]
    fn zero_stats_returns_basic() {
        let result = compute_investor_tier_from_stats(0, 0, 0, 0);
        assert_eq!(result, Ok(InvestorTier::Basic));
    }

    // ── Edge: meets all lower-tier criteria but not the tier above ────────

    #[test]
    fn meets_silver_but_not_gold_returns_silver() {
        let result = compute_investor_tier_from_stats(50_000, 5, 0, 50);
        assert_eq!(result, Ok(InvestorTier::Silver));
    }

    #[test]
    fn meets_gold_but_not_platinum_returns_gold() {
        let result = compute_investor_tier_from_stats(500_000, 15, 1, 30);
        assert_eq!(result, Ok(InvestorTier::Gold));
    }

    #[test]
    fn meets_platinum_but_not_vip_returns_platinum() {
        let result = compute_investor_tier_from_stats(3_000_000, 30, 2, 15);
        assert_eq!(result, Ok(InvestorTier::Platinum));
    }
}
