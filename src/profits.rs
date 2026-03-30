//! # Profits Module
//!
//! Computes investor return metrics and platform revenue in the QuickLendX
//! protocol.
//!
//! ## Return Metrics
//!
//! | Metric              | Formula                                                   |
//! |---------------------|-----------------------------------------------------------|
//! | Gross Profit        | `payout − funded_amount`                                  |
//! | Net Profit          | `gross_profit − investor_fees`                            |
//! | Return on Investment| `net_profit * BPS_DENOMINATOR / funded_amount` (in bps)  |
//! | Platform Revenue    | `sum(protocol_fees)`                                      |
//!
//! ## Safety
//!
//! All arithmetic is checked. Division is guarded against zero divisors.
//! ROI is expressed in basis points to avoid floating-point; callers can
//! convert: `roi_bps / 100` gives percent with two-decimal precision.

/// Basis-point denominator (10_000 = 100%).
pub const BPS_DENOMINATOR: u128 = 10_000;

/// Maximum supported investment amount (same ceiling as settlement / fees).
pub const MAX_INVESTMENT: u128 = 1_000_000_000_000_000_000_000_000_000_000; // 10^30

/// Aggregated platform revenue over a reporting window.
#[derive(Debug, PartialEq)]
pub struct PlatformRevenue {
    /// Sum of protocol fees collected.
    pub total_fees: u128,
    /// Sum of late-penalty amounts collected.
    pub total_penalties: u128,
    /// Combined revenue.
    pub total_revenue: u128,
}

// ─────────────────────────────────────────────────────────────────────────────
// Investor profit helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Calculates the gross profit for an investor.
///
/// `gross_profit = payout - funded_amount`
///
/// # Returns
/// `Some(profit)` or `None` if `payout < funded_amount` (net-loss scenario,
/// which should trigger a separate loss-recovery path) or on overflow.
pub fn gross_profit(investor_payout: u128, funded_amount: u128) -> Option<u128> {
    if funded_amount == 0 || funded_amount > MAX_INVESTMENT {
        return None;
    }
    investor_payout.checked_sub(funded_amount)
}

/// Calculates the net profit after deducting investor-side fees.
///
/// `net_profit = gross_profit - investor_fees`
///
/// # Returns
/// `Some(net)` where net may be 0 (break-even), or `None` on underflow/invalid
/// input.
pub fn net_profit(investor_payout: u128, funded_amount: u128, investor_fees: u128) -> Option<u128> {
    let gp = gross_profit(investor_payout, funded_amount)?;
    gp.checked_sub(investor_fees)
}

/// Calculates ROI in basis points.
///
/// `roi_bps = net_profit * BPS_DENOMINATOR / funded_amount`
///
/// A result of 200 means 2.00%.
///
/// # Returns
/// `Some(roi_bps)` or `None` on overflow or zero `funded_amount`.
pub fn return_on_investment_bps(
    investor_payout: u128,
    funded_amount: u128,
    investor_fees: u128,
) -> Option<u128> {
    if funded_amount == 0 {
        return None;
    }
    let np = net_profit(investor_payout, funded_amount, investor_fees)?;
    np.checked_mul(BPS_DENOMINATOR)?.checked_div(funded_amount)
}

// ─────────────────────────────────────────────────────────────────────────────
// Platform revenue aggregation
// ─────────────────────────────────────────────────────────────────────────────

/// Aggregates platform revenue from a slice of (protocol_fee, late_penalty)
/// tuples representing individual settlement events.
///
/// All additions are checked; if any intermediate sum would overflow u128 the
/// function returns `None`.
///
/// # Parameters
/// - `events` — slice of `(protocol_fee, late_penalty)` pairs.
///
/// # Returns
/// `Some(PlatformRevenue)` or `None` on overflow.
pub fn aggregate_platform_revenue(events: &[(u128, u128)]) -> Option<PlatformRevenue> {
    let mut total_fees: u128 = 0;
    let mut total_penalties: u128 = 0;

    for &(fee, penalty) in events {
        total_fees = total_fees.checked_add(fee)?;
        total_penalties = total_penalties.checked_add(penalty)?;
    }

    let total_revenue = total_fees.checked_add(total_penalties)?;

    Some(PlatformRevenue {
        total_fees,
        total_penalties,
        total_revenue,
    })
}

/// Computes an investor's share of revenue in a pool.
///
/// `share = (investor_contribution * total_pool_revenue) / total_pool_size`
///
/// Uses u128 with checked arithmetic; division is last.
///
/// # Returns
/// `Some(share)` or `None` on overflow / zero `total_pool_size`.
pub fn investor_revenue_share(
    investor_contribution: u128,
    total_pool_size: u128,
    total_pool_revenue: u128,
) -> Option<u128> {
    if total_pool_size == 0 {
        return None;
    }
    investor_contribution
        .checked_mul(total_pool_revenue)?
        .checked_div(total_pool_size)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── gross_profit ──────────────────────────────────────────────────────────

    #[test]
    fn test_gross_profit_positive() {
        assert_eq!(gross_profit(1_100_000, 1_000_000), Some(100_000));
    }

    #[test]
    fn test_gross_profit_break_even() {
        assert_eq!(gross_profit(1_000_000, 1_000_000), Some(0));
    }

    #[test]
    fn test_gross_profit_loss_returns_none() {
        // payout < funded_amount → underflow → None
        assert_eq!(gross_profit(900_000, 1_000_000), None);
    }

    #[test]
    fn test_gross_profit_zero_funded_rejected() {
        assert!(gross_profit(100_000, 0).is_none());
    }

    #[test]
    fn test_gross_profit_funded_exceeds_max_rejected() {
        assert!(gross_profit(u128::MAX, MAX_INVESTMENT + 1).is_none());
    }

    #[test]
    fn test_gross_profit_minimum_amounts() {
        assert_eq!(gross_profit(2, 1), Some(1));
        assert_eq!(gross_profit(1, 1), Some(0));
    }

    // ── net_profit ────────────────────────────────────────────────────────────

    #[test]
    fn test_net_profit_basic() {
        // gross=100_000, fees=20_000 → net=80_000
        assert_eq!(net_profit(1_100_000, 1_000_000, 20_000), Some(80_000));
    }

    #[test]
    fn test_net_profit_fees_equal_gross_break_even() {
        assert_eq!(net_profit(1_100_000, 1_000_000, 100_000), Some(0));
    }

    #[test]
    fn test_net_profit_fees_exceed_gross_returns_none() {
        // fees > gross profit → underflow → None
        assert!(net_profit(1_100_000, 1_000_000, 100_001).is_none());
    }

    #[test]
    fn test_net_profit_zero_fees() {
        assert_eq!(net_profit(1_050_000, 1_000_000, 0), Some(50_000));
    }

    // ── return_on_investment_bps ──────────────────────────────────────────────

    #[test]
    fn test_roi_10_percent() {
        // net=100_000, funded=1_000_000 → ROI = 100_000 * 10_000 / 1_000_000 = 1_000 bps = 10%
        let roi = return_on_investment_bps(1_100_000, 1_000_000, 0).unwrap();
        assert_eq!(roi, 1_000);
    }

    #[test]
    fn test_roi_zero_profit() {
        assert_eq!(return_on_investment_bps(1_000_000, 1_000_000, 0), Some(0));
    }

    #[test]
    fn test_roi_zero_funded_rejected() {
        assert!(return_on_investment_bps(1_000_000, 0, 0).is_none());
    }

    #[test]
    fn test_roi_loss_returns_none() {
        // net_profit underflows → None propagates
        assert!(return_on_investment_bps(900_000, 1_000_000, 0).is_none());
    }

    #[test]
    fn test_roi_with_fees_reduces_roi() {
        // gross=100_000 fees=50_000 net=50_000, funded=1_000_000 → 500 bps = 5%
        let roi = return_on_investment_bps(1_100_000, 1_000_000, 50_000).unwrap();
        assert_eq!(roi, 500);
    }

    #[test]
    fn test_roi_small_funded_large_profit() {
        // funded=1, payout=10_001, fees=0 → net=10_000, ROI = 10_000 * 10_000 / 1
        let roi = return_on_investment_bps(10_001, 1, 0).unwrap();
        assert_eq!(roi, 100_000_000);
    }

    // ── aggregate_platform_revenue ────────────────────────────────────────────

    #[test]
    fn test_aggregate_empty_events() {
        let rev = aggregate_platform_revenue(&[]).unwrap();
        assert_eq!(rev.total_fees, 0);
        assert_eq!(rev.total_penalties, 0);
        assert_eq!(rev.total_revenue, 0);
    }

    #[test]
    fn test_aggregate_single_event() {
        let rev = aggregate_platform_revenue(&[(10_000, 5_000)]).unwrap();
        assert_eq!(rev.total_fees, 10_000);
        assert_eq!(rev.total_penalties, 5_000);
        assert_eq!(rev.total_revenue, 15_000);
    }

    #[test]
    fn test_aggregate_multiple_events() {
        let events = [(10_000, 5_000), (20_000, 0), (0, 3_000)];
        let rev = aggregate_platform_revenue(&events).unwrap();
        assert_eq!(rev.total_fees, 30_000);
        assert_eq!(rev.total_penalties, 8_000);
        assert_eq!(rev.total_revenue, 38_000);
    }

    #[test]
    fn test_aggregate_overflow_fees_returns_none() {
        // Two u128::MAX fees → overflow on second add
        let events = [(u128::MAX, 0), (1, 0)];
        assert!(aggregate_platform_revenue(&events).is_none());
    }

    #[test]
    fn test_aggregate_overflow_revenue_sum_returns_none() {
        // MAX fees + MAX penalties → total_revenue overflows
        let events = [(u128::MAX / 2 + 1, u128::MAX / 2 + 1)];
        assert!(aggregate_platform_revenue(&events).is_none());
    }

    // ── investor_revenue_share ────────────────────────────────────────────────

    #[test]
    fn test_revenue_share_equal_contribution() {
        // 50% of pool = 50% of revenue
        let share = investor_revenue_share(500_000, 1_000_000, 100_000).unwrap();
        assert_eq!(share, 50_000);
    }

    #[test]
    fn test_revenue_share_full_pool() {
        // 100% of pool → full revenue
        let share = investor_revenue_share(1_000_000, 1_000_000, 100_000).unwrap();
        assert_eq!(share, 100_000);
    }

    #[test]
    fn test_revenue_share_zero_pool_size_rejected() {
        assert!(investor_revenue_share(500_000, 0, 100_000).is_none());
    }

    #[test]
    fn test_revenue_share_zero_contribution() {
        // 0 contribution → 0 share
        assert_eq!(investor_revenue_share(0, 1_000_000, 100_000), Some(0));
    }

    #[test]
    fn test_revenue_share_zero_revenue() {
        assert_eq!(investor_revenue_share(500_000, 1_000_000, 0), Some(0));
    }

    #[test]
    fn test_revenue_share_overflow_intermediate_returns_none() {
        // contribution * revenue overflows before division
        assert!(investor_revenue_share(u128::MAX, 1, u128::MAX).is_none());
    }
}
