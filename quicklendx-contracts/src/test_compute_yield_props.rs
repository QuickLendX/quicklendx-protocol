//! Property-based tests for `profits::compute_yield`.
//!
//! # Invariant under test
//! `compute_yield(amount, rate_bps, duration_days)` is **monotone non-decreasing**
//! in each of its three inputs when the other two are held constant:
//!
//! * `amount₁ ≤ amount₂  →  compute_yield(amount₁, r, d) ≤ compute_yield(amount₂, r, d)`
//! * `r₁ ≤ r₂            →  compute_yield(a, r₁, d) ≤ compute_yield(a, r₂, d)`
//! * `d₁ ≤ d₂            →  compute_yield(a, r, d₁) ≤ compute_yield(a, r, d₂)`
//!
//! A sad-path property is also included: any zero input must return 0.

#[cfg(all(test, feature = "fuzz-tests"))]
mod test_compute_yield_props {
    use crate::profits::{compute_expected_return, compute_yield};
    use proptest::prelude::*;

    // Ranges chosen to stay well inside i128 bounds while exercising realistic
    // protocol values (invoice amounts up to 1 quintillion base units, annual
    // rates up to 50 %, durations up to 10 years).
    const MAX_AMOUNT: i128 = 1_000_000_000_000_000_000; // 1 quintillion
    const MAX_RATE_BPS: u32 = 5_000; // 50 %
    const MAX_DURATION_DAYS: u32 = 3_650; // ~10 years

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(500))]

        // ── Monotonicity in amount ─────────────────────────────────────────

        #[test]
        fn yield_is_monotone_in_amount(
            lo in 0i128..MAX_AMOUNT,
            hi in 0i128..MAX_AMOUNT,
            rate_bps in 0u32..=MAX_RATE_BPS,
            duration_days in 0u32..=MAX_DURATION_DAYS,
        ) {
            let (a, b) = if lo <= hi { (lo, hi) } else { (hi, lo) };
            let y_a = compute_yield(a, rate_bps, duration_days);
            let y_b = compute_yield(b, rate_bps, duration_days);
            prop_assert!(
                y_a <= y_b,
                "yield must be non-decreasing in amount: \
                 compute_yield({a}, {rate_bps}, {duration_days}) = {y_a} > \
                 compute_yield({b}, {rate_bps}, {duration_days}) = {y_b}"
            );
        }

        // ── Monotonicity in rate_bps ───────────────────────────────────────

        #[test]
        fn yield_is_monotone_in_rate_bps(
            amount in 0i128..MAX_AMOUNT,
            lo_rate in 0u32..=MAX_RATE_BPS,
            hi_rate in 0u32..=MAX_RATE_BPS,
            duration_days in 0u32..=MAX_DURATION_DAYS,
        ) {
            let (r_lo, r_hi) = if lo_rate <= hi_rate { (lo_rate, hi_rate) } else { (hi_rate, lo_rate) };
            let y_lo = compute_yield(amount, r_lo, duration_days);
            let y_hi = compute_yield(amount, r_hi, duration_days);
            prop_assert!(
                y_lo <= y_hi,
                "yield must be non-decreasing in rate_bps: \
                 compute_yield({amount}, {r_lo}, {duration_days}) = {y_lo} > \
                 compute_yield({amount}, {r_hi}, {duration_days}) = {y_hi}"
            );
        }

        // ── Monotonicity in duration_days ──────────────────────────────────

        #[test]
        fn yield_is_monotone_in_duration_days(
            amount in 0i128..MAX_AMOUNT,
            rate_bps in 0u32..=MAX_RATE_BPS,
            lo_dur in 0u32..=MAX_DURATION_DAYS,
            hi_dur in 0u32..=MAX_DURATION_DAYS,
        ) {
            let (d_lo, d_hi) = if lo_dur <= hi_dur { (lo_dur, hi_dur) } else { (hi_dur, lo_dur) };
            let y_lo = compute_yield(amount, rate_bps, d_lo);
            let y_hi = compute_yield(amount, rate_bps, d_hi);
            prop_assert!(
                y_lo <= y_hi,
                "yield must be non-decreasing in duration_days: \
                 compute_yield({amount}, {rate_bps}, {d_lo}) = {y_lo} > \
                 compute_yield({amount}, {rate_bps}, {d_hi}) = {y_hi}"
            );
        }

        // ── Sad path: any zero input returns zero ──────────────────────────

        #[test]
        fn returns_zero_when_any_input_is_zero(
            amount in 0i128..MAX_AMOUNT,
            rate_bps in 0u32..=MAX_RATE_BPS,
            duration_days in 0u32..=MAX_DURATION_DAYS,
        ) {
            prop_assert_eq!(compute_yield(0, rate_bps, duration_days), 0,
                "zero amount must yield 0");
            prop_assert_eq!(compute_yield(amount, 0, duration_days), 0,
                "zero rate must yield 0");
            prop_assert_eq!(compute_yield(amount, rate_bps, 0), 0,
                "zero duration must yield 0");
        }
        // ── Expected Return ────────────────────────────────────────────────

        #[test]
        fn expected_return_is_non_negative_and_bounded(
            amount in 0i128..MAX_AMOUNT,
            rate_bps in 0u32..=MAX_RATE_BPS,
            duration_days in 0u32..=MAX_DURATION_DAYS,
        ) {
            let er = compute_expected_return(amount, rate_bps, duration_days);
            prop_assert!(er >= 0, "expected return must be non-negative");
            
            // Expected return must be bounded by the return at MAX_RATE_BPS
            let max_er = compute_expected_return(amount, MAX_RATE_BPS, duration_days);
            prop_assert!(er <= max_er, "expected return must be bounded by MAX_RATE");
            prop_assert!(er >= amount, "expected return must be >= amount");
        }
    }
}
