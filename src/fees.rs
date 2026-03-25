/// # Fees Module
///
/// Computes all protocol fees in the QuickLendX invoice-financing platform:
/// origination, servicing, default, and early-repayment fees.
///
/// ## Design Principles
///
/// 1. **Checked arithmetic only** — every multiply/divide uses the `checked_*`
///    family; overflow returns `None` rather than wrapping or panicking.
/// 2. **Unsigned integers** — `u128` eliminates signed-integer edge cases
///    (e.g., `i128::MIN.abs()` overflow).
/// 3. **Basis-point precision** — rates are expressed in bps (1/100 of a
///    percent, denominator 10_000) to avoid floating-point imprecision.
/// 4. **Division last** — multiplications are completed before dividing to
///    maximise precision and minimise intermediate rounding.
///
/// ## Fee Taxonomy
///
/// | Fee              | Applied to    | Max rate |
/// |------------------|---------------|----------|
/// | Origination      | face_value    | 500 bps  |
/// | Servicing        | face_value    | 300 bps  |
/// | Default penalty  | outstanding   | 2 000 bps|
/// | Early repayment  | outstanding   | 500 bps  |

/// Basis-point denominator.
pub const BPS_DENOMINATOR: u128 = 10_000;

/// Cap on the origination fee rate (5%).
pub const MAX_ORIGINATION_BPS: u128 = 500;

/// Cap on the servicing fee rate (3%).
pub const MAX_SERVICING_BPS: u128 = 300;

/// Cap on the default penalty rate (20%).
pub const MAX_DEFAULT_PENALTY_BPS: u128 = 2_000;

/// Cap on the early-repayment fee rate (5%).
pub const MAX_EARLY_REPAYMENT_BPS: u128 = 500;

/// Maximum amount accepted by any fee computation.
/// Keeps `amount * bps` within u128 (bps ≤ 10_000, so headroom = u128::MAX / 10_000).
pub const MAX_AMOUNT: u128 = u128::MAX / 10_001;

// ─────────────────────────────────────────────────────────────────────────────
// Internal helper
// ─────────────────────────────────────────────────────────────────────────────

/// Core fee formula: `amount * rate_bps / BPS_DENOMINATOR`.
///
/// Returns `None` on overflow or if `rate_bps > BPS_DENOMINATOR`.
#[inline]
fn bps_fee(amount: u128, rate_bps: u128) -> Option<u128> {
    if rate_bps > BPS_DENOMINATOR {
        return None;
    }
    amount
        .checked_mul(rate_bps)?
        .checked_div(BPS_DENOMINATOR)
}

// ─────────────────────────────────────────────────────────────────────────────
// Public API
// ─────────────────────────────────────────────────────────────────────────────

/// Calculates the one-time origination fee charged when an invoice is funded.
///
/// # Parameters
/// - `face_value`         — Invoice face value in smallest currency unit.
/// - `origination_fee_bps`— Origination rate in bps (0 – `MAX_ORIGINATION_BPS`).
///
/// # Returns
/// `Some(fee)` or `None` on invalid input / overflow.
pub fn origination_fee(face_value: u128, origination_fee_bps: u128) -> Option<u128> {
    if face_value == 0 || face_value > MAX_AMOUNT {
        return None;
    }
    if origination_fee_bps > MAX_ORIGINATION_BPS {
        return None;
    }
    bps_fee(face_value, origination_fee_bps)
}

/// Calculates the ongoing servicing fee for a single period.
///
/// # Parameters
/// - `face_value`        — Invoice face value.
/// - `servicing_fee_bps` — Servicing rate in bps (0 – `MAX_SERVICING_BPS`).
///
/// # Returns
/// `Some(fee)` or `None` on invalid input / overflow.
pub fn servicing_fee(face_value: u128, servicing_fee_bps: u128) -> Option<u128> {
    if face_value == 0 || face_value > MAX_AMOUNT {
        return None;
    }
    if servicing_fee_bps > MAX_SERVICING_BPS {
        return None;
    }
    bps_fee(face_value, servicing_fee_bps)
}

/// Calculates the penalty charged on a defaulted (unpaid) invoice.
///
/// # Parameters
/// - `outstanding_amount`   — Remaining unpaid balance.
/// - `default_penalty_bps`  — Penalty rate in bps (0 – `MAX_DEFAULT_PENALTY_BPS`).
///
/// # Returns
/// `Some(penalty)` or `None` on invalid input / overflow.
pub fn default_penalty(outstanding_amount: u128, default_penalty_bps: u128) -> Option<u128> {
    if outstanding_amount == 0 || outstanding_amount > MAX_AMOUNT {
        return None;
    }
    if default_penalty_bps > MAX_DEFAULT_PENALTY_BPS {
        return None;
    }
    bps_fee(outstanding_amount, default_penalty_bps)
}

/// Calculates the fee for early repayment of an invoice.
///
/// # Parameters
/// - `outstanding_amount`     — Amount being repaid early.
/// - `early_repayment_fee_bps`— Early repayment fee in bps (0 – `MAX_EARLY_REPAYMENT_BPS`).
///
/// # Returns
/// `Some(fee)` or `None` on invalid input / overflow.
pub fn early_repayment_fee(outstanding_amount: u128, early_repayment_fee_bps: u128) -> Option<u128> {
    if outstanding_amount == 0 || outstanding_amount > MAX_AMOUNT {
        return None;
    }
    if early_repayment_fee_bps > MAX_EARLY_REPAYMENT_BPS {
        return None;
    }
    bps_fee(outstanding_amount, early_repayment_fee_bps)
}

/// Aggregates all applicable fees for a single invoice event into a total.
///
/// All individual fee computations must succeed; if any returns `None`, the
/// whole aggregation returns `None` to prevent partial-fee states.
///
/// # Parameters
/// - `face_value`          — Invoice face value.
/// - `outstanding_amount`  — Current unpaid balance (may differ from face_value).
/// - `origination_bps`     — Origination fee rate.
/// - `servicing_bps`       — Servicing fee rate.
/// - `default_penalty_bps` — Default penalty rate (0 if not in default).
/// - `early_repayment_bps` — Early repayment fee rate (0 if not early).
///
/// # Returns
/// `Some(total_fees)` or `None`.
pub fn total_fees(
    face_value: u128,
    outstanding_amount: u128,
    origination_bps: u128,
    servicing_bps: u128,
    default_penalty_bps: u128,
    early_repayment_bps: u128,
) -> Option<u128> {
    let orig = origination_fee(face_value, origination_bps)?;
    let serv = servicing_fee(face_value, servicing_bps)?;
    let def_pen = default_penalty(outstanding_amount, default_penalty_bps)?;
    let early = early_repayment_fee(outstanding_amount, early_repayment_bps)?;

    orig.checked_add(serv)?
        .checked_add(def_pen)?
        .checked_add(early)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── origination_fee ───────────────────────────────────────────────────────

    #[test]
    fn test_origination_zero_rate() {
        assert_eq!(origination_fee(1_000_000, 0), Some(0));
    }

    #[test]
    fn test_origination_100_bps() {
        // 1% of 1_000_000 = 10_000
        assert_eq!(origination_fee(1_000_000, 100), Some(10_000));
    }

    #[test]
    fn test_origination_max_rate() {
        // 5% of 2_000_000 = 100_000
        assert_eq!(origination_fee(2_000_000, MAX_ORIGINATION_BPS), Some(100_000));
    }

    #[test]
    fn test_origination_rate_exceeds_max_rejected() {
        assert!(origination_fee(1_000_000, MAX_ORIGINATION_BPS + 1).is_none());
    }

    #[test]
    fn test_origination_zero_face_value_rejected() {
        assert!(origination_fee(0, 100).is_none());
    }

    #[test]
    fn test_origination_amount_exceeds_max_rejected() {
        assert!(origination_fee(MAX_AMOUNT + 1, 100).is_none());
    }

    // ── servicing_fee ─────────────────────────────────────────────────────────

    #[test]
    fn test_servicing_zero_rate() {
        assert_eq!(servicing_fee(500_000, 0), Some(0));
    }

    #[test]
    fn test_servicing_50_bps() {
        // 0.5% of 500_000 = 2_500
        assert_eq!(servicing_fee(500_000, 50), Some(2_500));
    }

    #[test]
    fn test_servicing_max_rate() {
        // 3% of 1_000_000 = 30_000
        assert_eq!(servicing_fee(1_000_000, MAX_SERVICING_BPS), Some(30_000));
    }

    #[test]
    fn test_servicing_rate_exceeds_max_rejected() {
        assert!(servicing_fee(1_000_000, MAX_SERVICING_BPS + 1).is_none());
    }

    #[test]
    fn test_servicing_zero_face_rejected() {
        assert!(servicing_fee(0, 100).is_none());
    }

    // ── default_penalty ───────────────────────────────────────────────────────

    #[test]
    fn test_default_penalty_zero_rate() {
        assert_eq!(default_penalty(1_000_000, 0), Some(0));
    }

    #[test]
    fn test_default_penalty_500_bps() {
        // 5% of 800_000 = 40_000
        assert_eq!(default_penalty(800_000, 500), Some(40_000));
    }

    #[test]
    fn test_default_penalty_max_rate() {
        // 20% of 1_000_000 = 200_000
        assert_eq!(default_penalty(1_000_000, MAX_DEFAULT_PENALTY_BPS), Some(200_000));
    }

    #[test]
    fn test_default_penalty_rate_exceeds_max_rejected() {
        assert!(default_penalty(1_000_000, MAX_DEFAULT_PENALTY_BPS + 1).is_none());
    }

    #[test]
    fn test_default_penalty_zero_outstanding_rejected() {
        assert!(default_penalty(0, 500).is_none());
    }

    // ── early_repayment_fee ───────────────────────────────────────────────────

    #[test]
    fn test_early_repayment_zero_rate() {
        assert_eq!(early_repayment_fee(1_000_000, 0), Some(0));
    }

    #[test]
    fn test_early_repayment_200_bps() {
        // 2% of 1_000_000 = 20_000
        assert_eq!(early_repayment_fee(1_000_000, 200), Some(20_000));
    }

    #[test]
    fn test_early_repayment_max_rate() {
        assert_eq!(
            early_repayment_fee(1_000_000, MAX_EARLY_REPAYMENT_BPS),
            Some(50_000)
        );
    }

    #[test]
    fn test_early_repayment_rate_exceeds_max_rejected() {
        assert!(early_repayment_fee(1_000_000, MAX_EARLY_REPAYMENT_BPS + 1).is_none());
    }

    // ── total_fees ────────────────────────────────────────────────────────────

    #[test]
    fn test_total_fees_all_zero_rates() {
        assert_eq!(total_fees(1_000_000, 1_000_000, 0, 0, 0, 0), Some(0));
    }

    #[test]
    fn test_total_fees_combined() {
        // orig=100bps=10_000, serv=50bps=5_000, def=0, early=0 → 15_000
        assert_eq!(
            total_fees(1_000_000, 1_000_000, 100, 50, 0, 0),
            Some(15_000)
        );
    }

    #[test]
    fn test_total_fees_returns_none_on_invalid_rate() {
        assert!(total_fees(1_000_000, 1_000_000, MAX_ORIGINATION_BPS + 1, 0, 0, 0).is_none());
    }

    // ── Boundary & overflow guards ────────────────────────────────────────────

    #[test]
    fn test_bps_fee_zero_amount() {
        // 0 * 100 / 10_000 = 0, but origination rejects amount==0
        assert!(origination_fee(0, 100).is_none());
    }

    #[test]
    fn test_amount_boundary_max_amount() {
        // MAX_AMOUNT * MAX_ORIGINATION_BPS should not overflow u128
        let result = origination_fee(MAX_AMOUNT, MAX_ORIGINATION_BPS);
        assert!(result.is_some());
    }

    #[test]
    fn test_amount_just_over_max_rejected() {
        assert!(origination_fee(MAX_AMOUNT + 1, MAX_ORIGINATION_BPS).is_none());
    }

    #[test]
    fn test_rate_exactly_bps_denominator_rejected_by_origination() {
        // BPS_DENOMINATOR = 10_000 > MAX_ORIGINATION_BPS = 500
        assert!(origination_fee(1_000_000, BPS_DENOMINATOR).is_none());
    }
}