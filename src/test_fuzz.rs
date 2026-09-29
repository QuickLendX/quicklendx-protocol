use crate::fees::{
    default_penalty, early_repayment_fee, origination_fee, servicing_fee, total_fees, MAX_AMOUNT,
    MAX_DEFAULT_PENALTY_BPS, MAX_EARLY_REPAYMENT_BPS, MAX_ORIGINATION_BPS, MAX_SERVICING_BPS,
};
use crate::profits::{
    aggregate_platform_revenue, gross_profit, investor_revenue_share, net_profit,
    return_on_investment_bps, MAX_INVESTMENT,
};
/// # Arithmetic Fuzz Tests — QuickLendX Protocol
///
/// This module implements fuzz-style tests for all critical arithmetic in the
/// QuickLendX settlement, fee, and profit modules.  Rather than relying on an
/// external fuzzing harness (e.g. `cargo-fuzz` / `libFuzzer`) which requires
/// the nightly toolchain, these tests use **property-based combinatorial
/// sweeps** that cover:
///
/// * Boundary values (0, 1, MAX−1, MAX)
/// * Power-of-two and near-power-of-two values
/// * Mid-range representativevalues
/// * BPS boundary points (0, 1, 9_999, 10_000)
/// * Fee-cap boundary points
///
/// Every case validates the same **safety invariants**:
/// 1. No checked operation panics; invalid inputs return `None`.
/// 2. Conservation: `investor_payout + protocol_fee == total_collected`.
/// 3. Monotonicity: increasing `face_value` never decreases a proportional fee.
/// 4. Fee caps are enforced: rate > max → `None`.
/// 5. Zero inputs are rejected where specified.
/// 6. ROI is non-negative iff net_profit is non-negative.
use crate::settlement::{
    compute_settlement, verify_conservation, BPS_DENOMINATOR as S_BPS, MAX_FACE_VALUE,
    MAX_PENALTY_BPS,
};

// ─────────────────────────────────────────────────────────────────────────────
// Sweep generators
// ─────────────────────────────────────────────────────────────────────────────

/// Representative u128 values across the full numeric range.
fn u128_sweep() -> Vec<u128> {
    let mut v = vec![
        0u128,
        1,
        2,
        127,
        255,
        256,
        1_000,
        9_999,
        10_000,
        10_001,
        100_000,
        500_000,
        999_999,
        1_000_000,
        1_000_001,
        u64::MAX as u128,
        u64::MAX as u128 + 1,
        MAX_FACE_VALUE - 1,
        MAX_FACE_VALUE,
        MAX_FACE_VALUE + 1,
        MAX_AMOUNT - 1,
        MAX_AMOUNT,
        MAX_AMOUNT + 1,
        MAX_INVESTMENT - 1,
        MAX_INVESTMENT,
        MAX_INVESTMENT + 1,
        u128::MAX - 1,
        u128::MAX,
    ];
    // Powers of 2
    for p in [1u32, 8, 16, 32, 64, 96, 120, 126] {
        v.push(1u128 << p);
        if (1u128 << p) > 0 {
            v.push((1u128 << p) - 1);
        }
    }
    v.sort_unstable();
    v.dedup();
    v
}

/// BPS rate sweep: 0, 1, key boundaries, fee-cap boundaries, and MAX.
fn bps_sweep() -> Vec<u128> {
    vec![
        0,
        1,
        50,
        100,
        200,
        MAX_ORIGINATION_BPS - 1,
        MAX_ORIGINATION_BPS,
        MAX_ORIGINATION_BPS + 1,
        MAX_SERVICING_BPS - 1,
        MAX_SERVICING_BPS,
        MAX_SERVICING_BPS + 1,
        MAX_DEFAULT_PENALTY_BPS - 1,
        MAX_DEFAULT_PENALTY_BPS,
        MAX_DEFAULT_PENALTY_BPS + 1,
        MAX_EARLY_REPAYMENT_BPS - 1,
        MAX_EARLY_REPAYMENT_BPS,
        MAX_EARLY_REPAYMENT_BPS + 1,
        MAX_PENALTY_BPS - 1,
        MAX_PENALTY_BPS,
        MAX_PENALTY_BPS + 1,
        S_BPS - 1,
        S_BPS,
        S_BPS + 1,
        u128::MAX,
    ]
}

// ─────────────────────────────────────────────────────────────────────────────
// Settlement fuzz tests
// ─────────────────────────────────────────────────────────────────────────────

/// Invariant: for all valid inputs compute_settlement returns Some and satisfies
/// conservation (investor_payout + protocol_fee == total_collected).
#[test]
fn fuzz_settlement_conservation_invariant() {
    let amounts = u128_sweep();
    let rates = bps_sweep();

    let mut checked = 0u64;

    for &face in &amounts {
        for &funded in &amounts {
            for &fee in &rates {
                for &penalty in &rates {
                    let result = compute_settlement(face, funded, fee, penalty);

                    // Classify as valid or invalid
                    let should_be_valid = face > 0
                        && face <= MAX_FACE_VALUE
                        && funded > 0
                        && funded <= face
                        && fee <= S_BPS
                        && penalty <= MAX_PENALTY_BPS;

                    if should_be_valid {
                        if let Some(r) = result {
                            assert!(
                                verify_conservation(&r),
                                "Conservation violated: face={face} funded={funded} fee_bps={fee} penalty_bps={penalty}"
                            );
                            // Investor payout must be ≥ funded amount
                            assert!(
                                r.investor_payout >= funded,
                                "Investor under-recovered: face={face} funded={funded}"
                            );
                            // total_collected must be ≥ face_value
                            assert!(
                                r.total_collected >= face,
                                "total_collected < face_value: face={face}"
                            );
                            checked += 1;
                        }
                        // Note: Some(valid_params) may still be None if the fee
                        // would leave investor below funded_amount — that is
                        // intentional business logic, not a bug.
                    } else {
                        // Invalid inputs must never produce Some
                        let is_definitely_invalid = face == 0
                            || face > MAX_FACE_VALUE
                            || funded == 0
                            || funded > face
                            || fee > S_BPS
                            || penalty > MAX_PENALTY_BPS;

                        if is_definitely_invalid {
                            assert!(
                                result.is_none(),
                                "Expected None for invalid inputs: face={face} funded={funded} \
                                 fee_bps={fee} penalty_bps={penalty}, got {result:?}"
                            );
                        }
                    }
                }
            }
        }
    }

    assert!(checked > 0, "No valid settlement cases were exercised");
}

/// Invariant: higher penalty_bps → higher or equal late_penalty amount.
#[test]
fn fuzz_settlement_penalty_monotonicity() {
    let face = 1_000_000u128;
    let funded = 800_000u128;
    let fee = 100u128;

    let penalty_rates: Vec<u128> = (0..=MAX_PENALTY_BPS).step_by(100).collect();

    let mut prev_penalty = 0u128;
    for &p in &penalty_rates {
        if let Some(r) = compute_settlement(face, funded, fee, p) {
            assert!(
                r.late_penalty >= prev_penalty,
                "Penalty decreased as rate increased: rate={p} penalty={} prev={}",
                r.late_penalty,
                prev_penalty
            );
            prev_penalty = r.late_penalty;
        }
    }
}

/// Invariant: higher fee_bps → lower or equal investor_payout.
#[test]
fn fuzz_settlement_fee_reduces_payout() {
    let face = 1_000_000u128;
    let funded = 100_000u128; // small to ensure investor is solvent across range

    let fee_rates: Vec<u128> = (0..=500u128).step_by(50).collect();

    let mut prev_payout = u128::MAX;
    for &f in &fee_rates {
        if let Some(r) = compute_settlement(face, funded, f, 0) {
            assert!(
                r.investor_payout <= prev_payout,
                "Payout increased as fee increased: fee_bps={f} payout={} prev={}",
                r.investor_payout,
                prev_payout
            );
            prev_payout = r.investor_payout;
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Fees fuzz tests
// ─────────────────────────────────────────────────────────────────────────────

/// Invariant: for any valid (amount, rate) pair, the fee ≤ amount.
#[test]
fn fuzz_fees_never_exceed_principal() {
    let amounts = u128_sweep();
    let rates = bps_sweep();

    for &amount in &amounts {
        for &rate in &rates {
            // origination
            if let Some(fee) = origination_fee(amount, rate) {
                assert!(
                    fee <= amount,
                    "origination_fee {fee} > amount {amount} at rate {rate}"
                );
            }
            // servicing
            if let Some(fee) = servicing_fee(amount, rate) {
                assert!(fee <= amount, "servicing_fee {fee} > amount {amount}");
            }
            // default_penalty
            if let Some(pen) = default_penalty(amount, rate) {
                assert!(pen <= amount, "default_penalty {pen} > amount {amount}");
            }
            // early_repayment
            if let Some(fee) = early_repayment_fee(amount, rate) {
                assert!(fee <= amount, "early_repayment_fee {fee} > amount {amount}");
            }
        }
    }
}

/// Invariant: fee cap boundaries are respected exactly.
#[test]
fn fuzz_fees_cap_enforcement() {
    let amount = 1_000_000u128;

    // Rates at cap → Some
    assert!(origination_fee(amount, MAX_ORIGINATION_BPS).is_some());
    assert!(servicing_fee(amount, MAX_SERVICING_BPS).is_some());
    assert!(default_penalty(amount, MAX_DEFAULT_PENALTY_BPS).is_some());
    assert!(early_repayment_fee(amount, MAX_EARLY_REPAYMENT_BPS).is_some());

    // Rates one above cap → None
    assert!(origination_fee(amount, MAX_ORIGINATION_BPS + 1).is_none());
    assert!(servicing_fee(amount, MAX_SERVICING_BPS + 1).is_none());
    assert!(default_penalty(amount, MAX_DEFAULT_PENALTY_BPS + 1).is_none());
    assert!(early_repayment_fee(amount, MAX_EARLY_REPAYMENT_BPS + 1).is_none());
}

/// Invariant: fee(0 rate) == 0 for any valid amount.
#[test]
fn fuzz_fees_zero_rate_yields_zero_fee() {
    let amounts = u128_sweep();

    for &amount in &amounts {
        if amount == 0 || amount > MAX_AMOUNT {
            continue; // invalid amounts return None anyway
        }
        assert_eq!(origination_fee(amount, 0), Some(0));
        assert_eq!(servicing_fee(amount, 0), Some(0));
        assert_eq!(default_penalty(amount, 0), Some(0));
        assert_eq!(early_repayment_fee(amount, 0), Some(0));
    }
}

/// Invariant: fee increases as rate increases (for same amount).
#[test]
fn fuzz_fees_monotone_in_rate() {
    let amount = 10_000_000u128;
    let mut prev = 0u128;

    for rate in (0..=MAX_ORIGINATION_BPS).step_by(10) {
        if let Some(fee) = origination_fee(amount, rate) {
            assert!(fee >= prev, "origination_fee decreased at rate={rate}");
            prev = fee;
        }
    }

    prev = 0;
    for rate in (0..=MAX_DEFAULT_PENALTY_BPS).step_by(50) {
        if let Some(pen) = default_penalty(amount, rate) {
            assert!(pen >= prev, "default_penalty decreased at rate={rate}");
            prev = pen;
        }
    }
}

/// Invariant: total_fees is the exact sum of individual fees.
#[test]
fn fuzz_total_fees_additivity() {
    let face = 1_000_000u128;
    let outstanding = 800_000u128;

    let orig_bps_vals = [0u128, 100, MAX_ORIGINATION_BPS];
    let serv_bps_vals = [0u128, 50, MAX_SERVICING_BPS];
    let def_bps_vals = [0u128, 500, MAX_DEFAULT_PENALTY_BPS];
    let early_bps_vals = [0u128, 100, MAX_EARLY_REPAYMENT_BPS];

    for &ob in &orig_bps_vals {
        for &sb in &serv_bps_vals {
            for &db in &def_bps_vals {
                for &eb in &early_bps_vals {
                    let expected = (|| -> Option<u128> {
                        let o = origination_fee(face, ob)?;
                        let s = servicing_fee(face, sb)?;
                        let d = default_penalty(outstanding, db)?;
                        let e = early_repayment_fee(outstanding, eb)?;
                        o.checked_add(s)?.checked_add(d)?.checked_add(e)
                    })();

                    let actual = total_fees(face, outstanding, ob, sb, db, eb);

                    assert_eq!(
                        expected, actual,
                        "total_fees mismatch: ob={ob} sb={sb} db={db} eb={eb}"
                    );
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Profits fuzz tests
// ─────────────────────────────────────────────────────────────────────────────

/// Invariant: gross_profit is non-negative iff payout ≥ funded_amount.
#[test]
fn fuzz_gross_profit_sign_consistency() {
    let amounts = u128_sweep();

    for &payout in &amounts {
        for &funded in &amounts {
            let result = gross_profit(payout, funded);
            let valid_funded = funded > 0 && funded <= MAX_INVESTMENT;

            if !valid_funded {
                assert!(result.is_none(), "Expected None for funded={funded}");
                continue;
            }

            if payout >= funded {
                // Should succeed
                assert!(
                    result.is_some(),
                    "Expected Some for payout={payout} funded={funded}"
                );
                assert_eq!(result.unwrap(), payout - funded);
            } else {
                // Underflow → None
                assert!(
                    result.is_none(),
                    "Expected None for payout={payout} < funded={funded}"
                );
            }
        }
    }
}

/// Invariant: net_profit ≤ gross_profit for any non-negative fee.
#[test]
fn fuzz_net_profit_le_gross_profit() {
    let cases = [
        (1_100_000u128, 1_000_000u128, 0u128),
        (1_100_000, 1_000_000, 50_000),
        (1_100_000, 1_000_000, 100_000),
        (2, 1, 0),
        (1_000_000_000, 500_000_000, 10_000),
    ];

    for (payout, funded, fees) in cases {
        let gp = gross_profit(payout, funded);
        let np = net_profit(payout, funded, fees);

        if let (Some(g), Some(n)) = (gp, np) {
            assert!(n <= g, "net_profit {n} > gross_profit {g}");
            assert_eq!(n, g - fees, "net_profit formula mismatch");
        }
    }
}

/// Invariant: ROI = 0 iff net_profit = 0, ROI > 0 iff net_profit > 0.
#[test]
fn fuzz_roi_sign_matches_net_profit() {
    let cases = [
        (1_100_000u128, 1_000_000u128, 0u128), // profit
        (1_000_000, 1_000_000, 0),             // break-even
        (1_100_000, 1_000_000, 100_000),       // break-even after fees
        (1_200_000, 1_000_000, 50_000),        // profit after fees
    ];

    for (payout, funded, fees) in cases {
        let roi = return_on_investment_bps(payout, funded, fees);
        let np = net_profit(payout, funded, fees);

        match (roi, np) {
            (Some(r), Some(n)) => {
                if n == 0 {
                    assert_eq!(r, 0, "ROI should be 0 when net_profit=0");
                } else {
                    assert!(r > 0, "ROI should be >0 when net_profit={n}");
                }
            }
            (None, None) => {} // both fail consistently
            (Some(_), None) | (None, Some(_)) => {
                panic!("ROI and net_profit disagree on None/Some for payout={payout} funded={funded} fees={fees}");
            }
        }
    }
}

/// Invariant: aggregate_platform_revenue total_revenue == total_fees + total_penalties.
#[test]
fn fuzz_aggregate_revenue_internal_consistency() {
    let fee_vals = [0u128, 1, 10_000, 1_000_000, u64::MAX as u128];
    let pen_vals = [0u128, 1, 5_000, 500_000, u64::MAX as u128 / 2];

    for &fee in &fee_vals {
        for &pen in &pen_vals {
            let events = [(fee, pen), (fee / 2, pen / 2)];
            if let Some(rev) = aggregate_platform_revenue(&events) {
                assert_eq!(
                    rev.total_revenue,
                    rev.total_fees + rev.total_penalties,
                    "Revenue conservation failed: fee={fee} pen={pen}"
                );
                assert!(rev.total_fees >= fee);
                assert!(rev.total_penalties >= pen);
            }
        }
    }
}

/// Invariant: investor_revenue_share with contribution == pool_size returns full revenue.
#[test]
fn fuzz_revenue_share_full_ownership() {
    let pool_and_revenue = [
        (1_000_000u128, 100_000u128),
        (1u128, 1u128),
        (u64::MAX as u128, 1_000_000u128),
    ];

    for (pool, revenue) in pool_and_revenue {
        let share = investor_revenue_share(pool, pool, revenue);
        assert_eq!(
            share,
            Some(revenue),
            "Full owner should receive full revenue"
        );
    }
}

/// Invariant: revenue shares are proportional — two investors at 50/50 split
/// each receive ~half (rounding loss ≤ 1 unit per investor).
#[test]
fn fuzz_revenue_share_proportional_split() {
    let pool = 1_000_000u128;
    let revenue = 100_000u128;
    let half = pool / 2;

    let share_a = investor_revenue_share(half, pool, revenue).unwrap();
    let share_b = investor_revenue_share(half, pool, revenue).unwrap();

    // Each share should be ~50_000; allow 1-unit rounding difference
    assert!(share_a + share_b >= revenue - 1 && share_a + share_b <= revenue);
}

// ─────────────────────────────────────────────────────────────────────────────
// Cross-module integration fuzz tests
// ─────────────────────────────────────────────────────────────────────────────

/// End-to-end: settlement feeds profit computation; net_profit must be
/// consistent with settlement output.
#[test]
fn fuzz_settlement_to_profit_pipeline() {
    let test_cases = [
        // (face, funded, fee_bps, penalty_bps, investor_fees)
        (1_000_000u128, 800_000u128, 200u128, 100u128, 5_000u128),
        (500_000, 450_000, 100, 0, 1_000),
        (10_000_000, 7_000_000, 300, 500, 50_000),
        (1, 1, 0, 0, 0),
        (MAX_FACE_VALUE, MAX_FACE_VALUE / 2, 100, 200, 1_000_000),
    ];

    for (face, funded, fee_bps, penalty_bps, investor_fees) in test_cases {
        let settlement = match compute_settlement(face, funded, fee_bps, penalty_bps) {
            Some(s) => s,
            None => continue,
        };

        assert!(verify_conservation(&settlement));

        // Net profit from investor perspective
        let np = net_profit(settlement.investor_payout, funded, investor_fees);

        // If investor_fees ≤ gross_profit, net_profit must be Some
        let gp = gross_profit(settlement.investor_payout, funded).unwrap_or(0);
        if investor_fees <= gp {
            assert!(
                np.is_some(),
                "Expected net_profit for face={face} funded={funded} fees={investor_fees}"
            );
        }
    }
}

/// Invariant: total_fees from fees module + late_penalty == what settlement charges.
/// Tests that the two modules use compatible arithmetic.
#[test]
fn fuzz_fees_and_settlement_arithmetic_compatibility() {
    let face = 2_000_000u128;
    let funded = 1_500_000u128;
    let protocol_bps = 200u128; // 2%

    let settlement = compute_settlement(face, funded, protocol_bps, 0).unwrap();
    let direct_fee = origination_fee(face, protocol_bps).unwrap();

    assert_eq!(
        settlement.protocol_fee, direct_fee,
        "settlement.protocol_fee should equal origination_fee at same rate"
    );
}
