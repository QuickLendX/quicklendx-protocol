//! # Settlement Module
//!
//! Handles the core arithmetic for invoice settlement in the QuickLendX protocol.
//!
//! ## Security Model
//!
//! All arithmetic operations use checked math (`checked_add`, `checked_sub`,
//! `checked_mul`, `checked_div`) to prevent silent overflow/underflow. Any
//! operation that would overflow returns `None`, which callers must handle as
//! an error. Amounts are represented as `u128` (unsigned 128-bit integers) to
//! support large invoice values while eliminating sign-related edge cases.
//!
//! ## Precision
//!
//! Internal computations use basis-point (bps) scaling (1 bps = 0.01%).
//! All division is performed last to minimize rounding error.
//!
//! ## Invariants
//!
//! - `face_value` ≥ `funded_amount` (discount never exceeds face value)
//! - `funded_amount` > 0 for any active invoice
//! - Fee percentages are expressed in basis points: 0–10_000 (0%–100%)

/// Basis-point denominator (10_000 = 100%).
pub const BPS_DENOMINATOR: u128 = 10_000;

/// Maximum face value accepted by the protocol (prevents u128 overflow during
/// intermediate multiplication with fee percentages).
/// 10^30 leaves headroom for `face_value * bps (≤10_000)` within u128::MAX.
pub const MAX_FACE_VALUE: u128 = 1_000_000_000_000_000_000_000_000_000_000; // 10^30

/// Maximum allowed late-penalty rate in basis points (50% = 5_000 bps).
pub const MAX_PENALTY_BPS: u128 = 5_000;

/// Represents a settled invoice payout broken down by recipient.
#[derive(Debug, PartialEq)]
pub struct SettlementResult {
    /// Net amount remitted to the investor after fees.
    pub investor_payout: u128,
    /// Protocol fee collected on this settlement.
    pub protocol_fee: u128,
    /// Late-payment penalty charged to the business (0 if on time).
    pub late_penalty: u128,
    /// Total amount collected from the debtor (face_value + late_penalty).
    pub total_collected: u128,
}

/// Computes the settlement amounts for a fully paid invoice.
///
/// # Parameters
///   - `face_value`      — Original invoice amount in the smallest currency unit.
///   - `funded_amount`   — Amount the investor disbursed (≤ face_value).
///   - `protocol_fee_bps`— Protocol fee in basis points (0–10_000).
///   - `late_penalty_bps`— Late-payment penalty in basis points (0–5_000); pass
///     `0` for on-time payments.
///
/// # Returns
/// `Some(SettlementResult)` on success, `None` on arithmetic overflow/underflow
/// or invalid inputs.
///
/// # Errors (returns `None`)
/// - `face_value` is 0 or exceeds `MAX_FACE_VALUE`
/// - `funded_amount` is 0 or exceeds `face_value`
/// - `protocol_fee_bps` > `BPS_DENOMINATOR`
/// - `late_penalty_bps` > `MAX_PENALTY_BPS`
/// - Any intermediate multiplication overflows u128
pub fn compute_settlement(
    face_value: u128,
    funded_amount: u128,
    protocol_fee_bps: u128,
    late_penalty_bps: u128,
) -> Option<SettlementResult> {
    // --- Input validation ---
    if face_value == 0 || face_value > MAX_FACE_VALUE {
        return None;
    }
    if funded_amount == 0 || funded_amount > face_value {
        return None;
    }
    if protocol_fee_bps > BPS_DENOMINATOR {
        return None;
    }
    if late_penalty_bps > MAX_PENALTY_BPS {
        return None;
    }

    // --- Late penalty (applied to face_value) ---
    // penalty = face_value * late_penalty_bps / BPS_DENOMINATOR
    let late_penalty = face_value
        .checked_mul(late_penalty_bps)?
        .checked_div(BPS_DENOMINATOR)?;

    // --- Total collected from debtor ---
    let total_collected = face_value.checked_add(late_penalty)?;

    // --- Protocol fee (applied to face_value only, not penalty) ---
    // protocol_fee = face_value * protocol_fee_bps / BPS_DENOMINATOR
    let protocol_fee = face_value
        .checked_mul(protocol_fee_bps)?
        .checked_div(BPS_DENOMINATOR)?;

    // --- Investor payout = total_collected - protocol_fee ---
    // Must not go below funded_amount; if it does, input parameters are
    // economically invalid (fee would consume more than the spread).
    let investor_payout = total_collected.checked_sub(protocol_fee)?;

    // Investor must at minimum recover the funded principal.
    if investor_payout < funded_amount {
        return None;
    }

    Some(SettlementResult {
        investor_payout,
        protocol_fee,
        late_penalty,
        total_collected,
    })
}

/// Calculates the investor's gross return (profit) on a settlement.
///
/// # Parameters
/// - `investor_payout` — Net payout received by investor.
/// - `funded_amount`   — Original amount investor disbursed.
///
/// # Returns
/// `Some(profit)` where profit = payout − funded_amount, or `None` if
/// `funded_amount` > `investor_payout` (loss scenario, handled separately).
pub fn investor_profit(investor_payout: u128, funded_amount: u128) -> Option<u128> {
    investor_payout.checked_sub(funded_amount)
}

/// Checks the conservation invariant: all output amounts must sum to
/// `total_collected`.
///
/// Used internally and in tests to assert no value is created or destroyed.
pub fn verify_conservation(result: &SettlementResult) -> bool {
    match result.investor_payout.checked_add(result.protocol_fee) {
        Some(sum) => sum == result.total_collected,
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Helpers ──────────────────────────────────────────────────────────────

    fn settlement_ok(
        face: u128,
        funded: u128,
        fee_bps: u128,
        penalty_bps: u128,
    ) -> SettlementResult {
        compute_settlement(face, funded, fee_bps, penalty_bps).expect("expected valid settlement")
    }

    // ── Basic happy-path tests ────────────────────────────────────────────────

    #[test]
    fn test_zero_fees_no_penalty() {
        let r = settlement_ok(1_000_000, 900_000, 0, 0);
        assert_eq!(r.late_penalty, 0);
        assert_eq!(r.protocol_fee, 0);
        assert_eq!(r.total_collected, 1_000_000);
        assert_eq!(r.investor_payout, 1_000_000);
        assert!(verify_conservation(&r));
    }

    #[test]
    fn test_fee_200_bps() {
        // 2% fee on face_value = 1_000_000 → fee = 20_000, payout = 980_000
        let r = settlement_ok(1_000_000, 900_000, 200, 0);
        assert_eq!(r.protocol_fee, 20_000);
        assert_eq!(r.investor_payout, 980_000);
        assert!(verify_conservation(&r));
    }

    #[test]
    fn test_penalty_500_bps() {
        // 5% late penalty on 1_000_000 → penalty = 50_000
        let r = settlement_ok(1_000_000, 800_000, 0, 500);
        assert_eq!(r.late_penalty, 50_000);
        assert_eq!(r.total_collected, 1_050_000);
        assert_eq!(r.investor_payout, 1_050_000);
        assert!(verify_conservation(&r));
    }

    #[test]
    fn test_fee_and_penalty_combined() {
        // face=1_000_000, funded=850_000, fee=300bps=30_000, penalty=200bps=20_000
        let r = settlement_ok(1_000_000, 850_000, 300, 200);
        assert_eq!(r.late_penalty, 20_000);
        assert_eq!(r.total_collected, 1_020_000);
        assert_eq!(r.protocol_fee, 30_000);
        assert_eq!(r.investor_payout, 990_000);
        assert!(verify_conservation(&r));
    }

    #[test]
    fn test_funded_equals_face_value_with_fee_rejected() {
        // funded == face_value with fee=100bps: payout (495_000) < funded (500_000).
        // The protocol rejects this to protect the investor from a guaranteed loss.
        assert!(compute_settlement(500_000, 500_000, 100, 0).is_none());
    }

    #[test]
    fn test_funded_equals_face_value_zero_fee() {
        // funded == face_value is valid only with zero fee (investor breaks even)
        let r = settlement_ok(500_000, 500_000, 0, 0);
        assert_eq!(r.protocol_fee, 0);
        assert_eq!(r.investor_payout, 500_000);
        assert!(verify_conservation(&r));
    }

    // ── Edge-case: boundary values ────────────────────────────────────────────

    #[test]
    fn test_minimum_face_value() {
        let r = settlement_ok(1, 1, 0, 0);
        assert_eq!(r.investor_payout, 1);
        assert!(verify_conservation(&r));
    }

    #[test]
    fn test_maximum_face_value() {
        let r = settlement_ok(MAX_FACE_VALUE, 1, 0, 0);
        assert_eq!(r.investor_payout, MAX_FACE_VALUE);
        assert!(verify_conservation(&r));
    }

    #[test]
    fn test_maximum_protocol_fee_bps() {
        // 100% fee: all value goes to protocol
        let result = compute_settlement(1_000_000, 500_000, BPS_DENOMINATOR, 0);
        // investor_payout = 1_000_000 - 1_000_000 = 0 < funded_amount → None
        assert!(result.is_none());
    }

    #[test]
    fn test_maximum_penalty_bps() {
        let r = settlement_ok(1_000_000, 400_000, 0, MAX_PENALTY_BPS);
        // 50% penalty: total_collected = 1_500_000
        assert_eq!(r.late_penalty, 500_000);
        assert_eq!(r.total_collected, 1_500_000);
        assert!(verify_conservation(&r));
    }

    // ── Overflow / underflow guards ───────────────────────────────────────────

    #[test]
    fn test_face_value_zero_rejected() {
        assert!(compute_settlement(0, 0, 0, 0).is_none());
    }

    #[test]
    fn test_face_value_exceeds_max_rejected() {
        assert!(compute_settlement(MAX_FACE_VALUE + 1, 1, 0, 0).is_none());
    }

    #[test]
    fn test_funded_amount_zero_rejected() {
        assert!(compute_settlement(1_000_000, 0, 0, 0).is_none());
    }

    #[test]
    fn test_funded_exceeds_face_rejected() {
        assert!(compute_settlement(1_000, 1_001, 0, 0).is_none());
    }

    #[test]
    fn test_fee_bps_exceeds_denominator_rejected() {
        assert!(compute_settlement(1_000_000, 500_000, BPS_DENOMINATOR + 1, 0).is_none());
    }

    #[test]
    fn test_penalty_bps_exceeds_max_rejected() {
        assert!(compute_settlement(1_000_000, 500_000, 0, MAX_PENALTY_BPS + 1).is_none());
    }

    // ── Conservation invariant ────────────────────────────────────────────────

    #[test]
    fn test_conservation_all_scenarios() {
        let cases = [
            (1_000_000u128, 900_000u128, 200u128, 100u128),
            (999_999, 1, 9_999, 4_999),
            (1, 1, 0, 0),
            (MAX_FACE_VALUE, MAX_FACE_VALUE / 2, 500, 1_000),
        ];
        for (face, funded, fee, penalty) in cases {
            if let Some(r) = compute_settlement(face, funded, fee, penalty) {
                assert!(
                    verify_conservation(&r),
                    "Conservation failed for face={face} funded={funded} fee={fee} penalty={penalty}"
                );
            }
        }
    }

    // ── investor_profit helper ────────────────────────────────────────────────

    #[test]
    fn test_investor_profit_positive() {
        assert_eq!(investor_profit(1_000_000, 900_000), Some(100_000));
    }

    #[test]
    fn test_investor_profit_break_even() {
        assert_eq!(investor_profit(900_000, 900_000), Some(0));
    }

    #[test]
    fn test_investor_profit_loss_returns_none() {
        // payout < funded_amount → underflow → None
        assert_eq!(investor_profit(800_000, 900_000), None);
    }
}
