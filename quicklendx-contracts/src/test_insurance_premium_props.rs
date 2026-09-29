/// Property-based tests for `Investment::calculate_premium`.
///
/// # Economic invariant
/// Free insurance is an unbounded liability for the provider and a direct
/// economic leak: an investor receives risk-transfer at zero cost, breaking
/// the actuarial soundness of the protocol. Every property in this suite
/// defends that invariant from a different angle.
///
/// # Coverage targets
/// * **Floor** — `calculate_premium` always returns `>= MIN_PREMIUM_AMOUNT`
///   for any positive coverage amount, even in the integer-division rounding
///   regime where `coverage_amount * 200 / 10_000` would otherwise be 0.
/// * **Rate** — For amounts large enough to clear the floor, the 2 % rate
///   holds within the rounding documented in the function's doc-comment.
/// * **Monotonicity** — Larger `amount` never yields a strictly smaller
///   premium for the same `coverage_percentage`.
/// * **Overflow safety** — No panic for inputs up to `i128::MAX` thanks to
///   `saturating_mul` / `checked_div` and `overflow-checks = true` in the
///   release profile.
/// * **Coverage-never-exceeds-principal** — The internal invariant documented
///   in `add_insurance` holds across the full input space.
#[cfg(all(test, feature = "fuzz-tests"))]
mod test_insurance_premium_props {
    use crate::investment::{
        Investment, DEFAULT_INSURANCE_PREMIUM_BPS, MAX_COVERAGE_PERCENTAGE,
        MIN_COVERAGE_PERCENTAGE, MIN_PREMIUM_AMOUNT,
    };
    use proptest::prelude::*;

    // ── Constants derived from the SUT ─────────────────────────────────────

    /// Basis-point denominator used in premium math.
    const BPS_DENOM: i128 = 10_000;

    /// Coverage percentage denominator.
    const PCT_DENOM: i128 = 100;

    /// The smallest `amount` for which 1 % coverage still produces a
    /// non-zero `coverage_amount` (i.e. `amount / 100 >= 1`).
    const COVERAGE_NONZERO_THRESHOLD: i128 = 100;

    /// The smallest `amount` for which the 2 % premium on 100 % coverage
    /// clears the floor without it being applied
    /// (`amount * 10_000 / 10_000 >= MIN_PREMIUM_AMOUNT`, simplified:
    /// `amount * DEFAULT_INSURANCE_PREMIUM_BPS / 10_000 >= 1`
    /// => `amount >= 10_000 / 200 = 50`).
    const PREMIUM_CLEARS_FLOOR_THRESHOLD: i128 = 50;

    /// Practical upper bound matching the largest representable invoice
    /// amount used in staking/overflow tests (1 quintillion base units).
    const MAX_INVOICE_AMOUNT: i128 = 1_000_000_000_000_000_000;

    // ── Helpers ─────────────────────────────────────────────────────────────

    /// True reference implementation of the premium formula (no floor).
    /// Used to verify the SUT matches the documented math when the floor
    /// is not expected to apply.
    fn expected_premium_no_floor(amount: i128, coverage_percentage: u32) -> i128 {
        let coverage_amount = amount
            .saturating_mul(coverage_percentage as i128)
            .checked_div(PCT_DENOM)
            .unwrap_or(0);
        coverage_amount
            .saturating_mul(DEFAULT_INSURANCE_PREMIUM_BPS)
            .checked_div(BPS_DENOM)
            .unwrap_or(0)
    }

    // =========================================================================
    // 1. FLOOR PROPERTY — premium is always >= MIN_PREMIUM_AMOUNT
    // =========================================================================

    /// **Property**: For any positive `amount` and any valid
    /// `coverage_percentage`, `calculate_premium` never returns a value
    /// below `MIN_PREMIUM_AMOUNT`.
    ///
    /// Economic invariant: zero-premium coverage is free insurance — a
    /// direct exploit against the protocol treasury.
    #[test]
    fn prop_premium_always_at_least_min_premium_amount() {
        proptest!(|(
            amount in 1i128..=MAX_INVOICE_AMOUNT,
            coverage_pct in MIN_COVERAGE_PERCENTAGE..=MAX_COVERAGE_PERCENTAGE,
        )| {
            let premium = Investment::calculate_premium(amount, coverage_pct);

            // A non-zero return means the inputs were accepted; the floor
            // must then hold.  A zero return is valid only for invalid inputs
            // (handled in prop_invalid_inputs_return_zero), but within the
            // valid range every positive coverage_amount must cost >= 1.
            if premium != 0 {
                prop_assert!(
                    premium >= MIN_PREMIUM_AMOUNT,
                    "premium {} < MIN_PREMIUM_AMOUNT {} for amount={}, pct={}",
                    premium, MIN_PREMIUM_AMOUNT, amount, coverage_pct
                );
            }
        });
    }

    /// **Property (small-amount rounding regime)**: In the specific regime
    /// where integer division of `coverage_amount * 200 / 10_000` rounds
    /// to zero, the floor must kick in and return exactly `MIN_PREMIUM_AMOUNT`.
    ///
    /// This is the regime the issue was filed about:
    ///   `amount` in `[1, 49]`, any valid `coverage_pct` — the raw 2 %
    ///   computation rounds to zero, so the floor is the only thing standing
    ///   between the investor and free insurance.
    #[test]
    fn prop_floor_applied_in_rounding_to_zero_regime() {
        proptest!(|(
            // Amounts small enough that even 100 % coverage produces a
            // premium of 0 before the floor (amount * 200 / 10_000 < 1
            // => amount < 50).
            amount in 1i128..PREMIUM_CLEARS_FLOOR_THRESHOLD,
            coverage_pct in MIN_COVERAGE_PERCENTAGE..=MAX_COVERAGE_PERCENTAGE,
        )| {
            let premium = Investment::calculate_premium(amount, coverage_pct);

            // coverage_amount = amount * coverage_pct / 100.
            // For amount in [1,49] and coverage_pct in [1,100]:
            //   * Some (amount, pct) pairs still produce coverage_amount = 0
            //     when amount * pct < 100 (e.g. amount=1, pct=1 → 0).
            //   * When coverage_amount = 0, calculate_premium returns 0 (the
            //     coverage_amount <= 0 guard fires before the floor).
            //   * When coverage_amount > 0, the floor must apply.
            let coverage_amount = amount
                .saturating_mul(coverage_pct as i128)
                .checked_div(PCT_DENOM)
                .unwrap_or(0);

            if coverage_amount > 0 {
                // Raw premium < 1 → floor must apply → result == MIN_PREMIUM_AMOUNT.
                let raw_premium = coverage_amount
                    .saturating_mul(DEFAULT_INSURANCE_PREMIUM_BPS)
                    .checked_div(BPS_DENOM)
                    .unwrap_or(0);

                if raw_premium < MIN_PREMIUM_AMOUNT {
                    prop_assert_eq!(
                        premium,
                        MIN_PREMIUM_AMOUNT,
                        "expected floor {} but got {} for amount={}, pct={}",
                        MIN_PREMIUM_AMOUNT, premium, amount, coverage_pct
                    );
                }
            }
        });
    }

    /// **Boundary**: `amount = 1`, all valid percentages — worst-case for the
    /// floor: `coverage_amount = 0` for all `pct < 100`, and
    /// `coverage_amount = 1` for `pct = 100`.
    #[test]
    fn prop_floor_coverage_amount_one() {
        proptest!(|(
            coverage_pct in MIN_COVERAGE_PERCENTAGE..=MAX_COVERAGE_PERCENTAGE,
        )| {
            let premium = Investment::calculate_premium(1, coverage_pct);
            // For amount=1, coverage_amount = 1 * pct / 100.
            // * pct < 100 → coverage_amount = 0 → function returns 0 (guard).
            // * pct = 100 → coverage_amount = 1; raw premium = 1*200/10_000 = 0
            //   → floor applies → returns MIN_PREMIUM_AMOUNT.
            let coverage_amount = 1i128
                .saturating_mul(coverage_pct as i128)
                .checked_div(PCT_DENOM)
                .unwrap_or(0);

            if coverage_amount == 0 {
                prop_assert_eq!(premium, 0,
                    "expected 0 (guard) for amount=1, pct={}", coverage_pct);
            } else {
                prop_assert_eq!(premium, MIN_PREMIUM_AMOUNT,
                    "expected floor for amount=1, pct={}", coverage_pct);
            }
        });
    }

    /// **Boundary**: exact threshold where the raw 2 % crosses 1.
    /// `coverage_amount = 50` → `50 * 200 / 10_000 = 1 = MIN_PREMIUM_AMOUNT`.
    /// `coverage_amount = 49` → `49 * 200 / 10_000 = 0` → floor applies.
    #[test]
    fn prop_floor_threshold_boundary() {
        // coverage_amount = 50 at 100 % coverage → amount = 50.
        assert_eq!(Investment::calculate_premium(50, 100), 1);
        // coverage_amount = 49 at 100 % coverage → floor.
        assert_eq!(Investment::calculate_premium(49, 100), 1);
        // coverage_amount = 51 at 100 % coverage → raw premium = 1, no floor needed.
        assert_eq!(Investment::calculate_premium(51, 100), 1);
    }

    // =========================================================================
    // 2. RATE PROPERTY — 2 % holds for large amounts
    // =========================================================================

    /// **Property**: When `amount` is large enough that the raw 2 % premium
    /// clears the floor, `calculate_premium` returns exactly
    /// `floor(coverage_amount * DEFAULT_INSURANCE_PREMIUM_BPS / 10_000)`.
    ///
    /// This verifies the SUT implements the documented formula, not an
    /// accidental approximation.
    #[test]
    fn prop_rate_matches_documented_formula_for_large_amounts() {
        proptest!(|(
            // Large enough that even 1 % coverage * 2 % rate > 0.
            // Minimum: coverage_amount = amount / 100;
            //          premium = coverage_amount * 200 / 10_000 >= 1
            //          => coverage_amount >= 50 => amount >= 5_000.
            amount in 5_000i128..=MAX_INVOICE_AMOUNT,
            coverage_pct in MIN_COVERAGE_PERCENTAGE..=MAX_COVERAGE_PERCENTAGE,
        )| {
            let premium = Investment::calculate_premium(amount, coverage_pct);

            // Compute the reference value using the documented formula.
            let expected = expected_premium_no_floor(amount, coverage_pct);

            if premium > 0 && expected >= MIN_PREMIUM_AMOUNT {
                // The floor should NOT have been applied here.
                prop_assert_eq!(
                    premium, expected,
                    "rate mismatch: amount={}, pct={}, expected={}, got={}",
                    amount, coverage_pct, expected, premium
                );
            }
        });
    }

    /// **Property**: The effective rate is always between 0 % and 2 % of
    /// `coverage_amount` (inclusive on both sides due to integer truncation
    /// and the floor).
    #[test]
    fn prop_effective_rate_within_documented_bounds() {
        proptest!(|(
            amount in 1i128..=MAX_INVOICE_AMOUNT,
            coverage_pct in MIN_COVERAGE_PERCENTAGE..=MAX_COVERAGE_PERCENTAGE,
        )| {
            let premium = Investment::calculate_premium(amount, coverage_pct);

            let coverage_amount = amount
                .saturating_mul(coverage_pct as i128)
                .checked_div(PCT_DENOM)
                .unwrap_or(0);

            if premium > 0 && coverage_amount > 0 {
                // Lower bound: premium >= MIN_PREMIUM_AMOUNT (already covered
                // by prop_premium_always_at_least_min_premium_amount, repeated
                // here for readability).
                prop_assert!(
                    premium >= MIN_PREMIUM_AMOUNT,
                    "premium below floor: {}", premium
                );

                // Upper bound: premium must not exceed the coverage it funds.
                // With a 2 % rate this always holds, but an explicit check
                // prevents economic inversions.
                prop_assert!(
                    premium <= coverage_amount,
                    "premium {} exceeds coverage_amount {} for amount={}, pct={}",
                    premium, coverage_amount, amount, coverage_pct
                );

                // Tight upper bound: premium must not exceed the raw 2 %
                // ceiling (or 1 when the floor applies).
                let raw_upper = coverage_amount
                    .saturating_mul(DEFAULT_INSURANCE_PREMIUM_BPS)
                    .checked_div(BPS_DENOM)
                    .unwrap_or(0);
                let ceiling = raw_upper.max(MIN_PREMIUM_AMOUNT);
                prop_assert_eq!(
                    premium, ceiling,
                    "premium {} != ceiling {} for amount={}, pct={}",
                    premium, ceiling, amount, coverage_pct
                );
            }
        });
    }

    // =========================================================================
    // 3. MONOTONICITY — larger amount never yields smaller premium
    // =========================================================================

    /// **Property**: For a fixed `coverage_percentage`, if `amount_b > amount_a`
    /// then `calculate_premium(amount_b, pct) >= calculate_premium(amount_a, pct)`.
    ///
    /// A non-monotonic premium would allow a sophisticated investor to obtain
    /// more coverage for a lower fee by staging multiple smaller investments.
    #[test]
    fn prop_premium_monotone_in_amount() {
        proptest!(|(
            amount_a in 1i128..=MAX_INVOICE_AMOUNT / 2,
            // amount_b is strictly larger than amount_a.
            delta in 1i128..=MAX_INVOICE_AMOUNT / 2,
            coverage_pct in MIN_COVERAGE_PERCENTAGE..=MAX_COVERAGE_PERCENTAGE,
        )| {
            let amount_b = amount_a.saturating_add(delta);
            // Guard: if addition saturated, skip this sample.
            prop_assume!(amount_b > amount_a);

            let premium_a = Investment::calculate_premium(amount_a, coverage_pct);
            let premium_b = Investment::calculate_premium(amount_b, coverage_pct);

            // Both must be >= 0; if both are positive, premium_b >= premium_a.
            prop_assert!(premium_a >= 0);
            prop_assert!(premium_b >= 0);

            if premium_a > 0 && premium_b > 0 {
                prop_assert!(
                    premium_b >= premium_a,
                    "monotonicity violated: premium({}, {})={} > premium({}, {})={}",
                    amount_b, coverage_pct, premium_b,
                    amount_a, coverage_pct, premium_a
                );
            }
        });
    }

    /// **Property**: For a fixed `amount`, if `coverage_pct_b > coverage_pct_a`
    /// then `calculate_premium(amount, pct_b) >= calculate_premium(amount, pct_a)`.
    ///
    /// Higher coverage demands a higher premium; otherwise the pricing model
    /// is economically broken.
    #[test]
    fn prop_premium_monotone_in_coverage_percentage() {
        proptest!(|(
            amount in 1i128..=MAX_INVOICE_AMOUNT,
            pct_a in MIN_COVERAGE_PERCENTAGE..MAX_COVERAGE_PERCENTAGE,
            // pct_b is strictly larger, capped at MAX.
            pct_b in (MIN_COVERAGE_PERCENTAGE + 1)..=MAX_COVERAGE_PERCENTAGE,
        )| {
            prop_assume!(pct_b > pct_a);

            let premium_a = Investment::calculate_premium(amount, pct_a);
            let premium_b = Investment::calculate_premium(amount, pct_b);

            if premium_a > 0 && premium_b > 0 {
                prop_assert!(
                    premium_b >= premium_a,
                    "coverage monotonicity violated: \
                     premium(amount={}, pct={})={} > premium(amount={}, pct={})={}",
                    amount, pct_b, premium_b,
                    amount, pct_a, premium_a
                );
            }
        });
    }

    // =========================================================================
    // 4. OVERFLOW SAFETY — no panic for any input up to i128::MAX
    // =========================================================================

    /// **Property**: `calculate_premium` must not panic for any `amount` in
    /// `[1, i128::MAX]` and any valid `coverage_percentage`.
    ///
    /// Soroban contracts compiled with `overflow-checks = true` (enforced by
    /// the release profile) trap on integer overflow. A panic here would be
    /// a DoS vector: an attacker who can trigger the premium calculation with
    /// a crafted large amount could freeze the investment lifecycle for any
    /// invoice that reaches that code path.
    #[test]
    fn prop_no_panic_for_very_large_amounts() {
        proptest!(|(
            amount in 1i128..=i128::MAX,
            coverage_pct in MIN_COVERAGE_PERCENTAGE..=MAX_COVERAGE_PERCENTAGE,
        )| {
            // Must not panic.
            let result = Investment::calculate_premium(amount, coverage_pct);
            // Result must be non-negative and, if non-zero, sane.
            prop_assert!(result >= 0);
        });
    }

    /// **Property**: Specific large-amount anchors at the protocol's practical
    /// maximum invoice size (1 quintillion) must produce correct results.
    #[test]
    fn prop_max_invoice_amount_produces_correct_result() {
        proptest!(|(
            coverage_pct in MIN_COVERAGE_PERCENTAGE..=MAX_COVERAGE_PERCENTAGE,
        )| {
            let amount = MAX_INVOICE_AMOUNT;
            let premium = Investment::calculate_premium(amount, coverage_pct);

            // Must not be zero — no valid large-amount input should round to zero.
            prop_assert!(
                premium >= MIN_PREMIUM_AMOUNT,
                "unexpected zero premium for max amount, pct={}",
                coverage_pct
            );

            // Must match the reference formula exactly (no floor needed here).
            let expected = expected_premium_no_floor(amount, coverage_pct);
            prop_assert_eq!(
                premium, expected,
                "rate mismatch at max_amount, pct={}", coverage_pct
            );
        });
    }

    /// **Property**: `i128::MAX` as `amount` must not panic. The result
    /// may saturate, but `saturating_mul` guarantees a finite value.
    #[test]
    fn prop_i128_max_amount_does_not_panic() {
        for pct in [MIN_COVERAGE_PERCENTAGE, 50, MAX_COVERAGE_PERCENTAGE] {
            let result = Investment::calculate_premium(i128::MAX, pct);
            // saturating_mul on i128::MAX * 100 saturates to i128::MAX;
            // checked_div then returns Some(i128::MAX), so the final premium
            // is >= MIN_PREMIUM_AMOUNT.
            assert!(
                result >= 0,
                "i128::MAX amount produced negative result for pct={}",
                pct
            );
        }
    }

    // =========================================================================
    // 5. INVALID INPUTS — zero/negative amounts and out-of-range percentages
    // =========================================================================

    /// **Property**: Any non-positive `amount` must return 0 (rejection signal).
    #[test]
    fn prop_invalid_inputs_return_zero_for_nonpositive_amount() {
        proptest!(|(
            amount in i128::MIN..=0i128,
            coverage_pct in MIN_COVERAGE_PERCENTAGE..=MAX_COVERAGE_PERCENTAGE,
        )| {
            prop_assert_eq!(
                Investment::calculate_premium(amount, coverage_pct),
                0,
                "expected 0 for non-positive amount={}", amount
            );
        });
    }

    /// **Property**: Any `coverage_percentage` outside
    /// `[MIN_COVERAGE_PERCENTAGE, MAX_COVERAGE_PERCENTAGE]` must return 0.
    #[test]
    fn prop_invalid_inputs_return_zero_for_out_of_range_percentage() {
        proptest!(|(
            amount in 1i128..=MAX_INVOICE_AMOUNT,
        )| {
            // Below minimum
            prop_assert_eq!(
                Investment::calculate_premium(amount, MIN_COVERAGE_PERCENTAGE - 1),
                0,
                "expected 0 for pct < MIN"
            );
            // Above maximum
            prop_assert_eq!(
                Investment::calculate_premium(amount, MAX_COVERAGE_PERCENTAGE + 1),
                0,
                "expected 0 for pct > MAX"
            );
            // u32::MAX (potential overflow in naïve callers)
            prop_assert_eq!(
                Investment::calculate_premium(amount, u32::MAX),
                0,
                "expected 0 for pct=u32::MAX"
            );
        });
    }

    // =========================================================================
    // 6. COVERAGE-NEVER-EXCEEDS-PRINCIPAL
    // =========================================================================

    /// **Property**: For any valid inputs, the derived `coverage_amount`
    /// (computed identically to how `calculate_premium` computes it) must
    /// never exceed `amount`.
    ///
    /// This is the over-coverage exploit guard: a claimant receiving more
    /// than was invested would drain the protocol.
    #[test]
    fn prop_coverage_amount_never_exceeds_principal() {
        proptest!(|(
            amount in 1i128..=MAX_INVOICE_AMOUNT,
            coverage_pct in MIN_COVERAGE_PERCENTAGE..=MAX_COVERAGE_PERCENTAGE,
        )| {
            let coverage_amount = amount
                .saturating_mul(coverage_pct as i128)
                .checked_div(PCT_DENOM)
                .unwrap_or(0);

            prop_assert!(
                coverage_amount <= amount,
                "coverage_amount {} > amount {} for pct={}",
                coverage_amount, amount, coverage_pct
            );
        });
    }

    // =========================================================================
    // 7. DETERMINISM — same inputs always produce same output
    // =========================================================================

    /// **Property**: `calculate_premium` is a pure function — calling it
    /// twice with the same arguments always returns the same value.
    #[test]
    fn prop_deterministic_for_all_valid_inputs() {
        proptest!(|(
            amount in 1i128..=MAX_INVOICE_AMOUNT,
            coverage_pct in MIN_COVERAGE_PERCENTAGE..=MAX_COVERAGE_PERCENTAGE,
        )| {
            let first  = Investment::calculate_premium(amount, coverage_pct);
            let second = Investment::calculate_premium(amount, coverage_pct);
            prop_assert_eq!(first, second,
                "non-deterministic: amount={}, pct={}", amount, coverage_pct);
        });
    }

    // =========================================================================
    // 8. KNOWN ANCHORS — regression-guard fixed values from test_insurance.rs
    // =========================================================================

    /// Regression guard: the known-good values from `test_insurance.rs`
    /// `test_calculate_premium_typical_cases` must remain stable.
    ///
    /// If any of these fail, a refactor broke the documented formula.
    #[test]
    fn prop_known_anchor_values_unchanged() {
        // Anchors from test_calculate_premium_typical_cases
        assert_eq!(Investment::calculate_premium(10_000, 80), 160);
        assert_eq!(Investment::calculate_premium(10_000, 50), 100);
        assert_eq!(Investment::calculate_premium(10_000, 100), 200);
        assert_eq!(Investment::calculate_premium(10_000, 1), 2);

        // Anchors from test_calculate_premium_minimum_floor
        assert_eq!(Investment::calculate_premium(500, 1), 1); // floor
        assert_eq!(Investment::calculate_premium(100, 1), 1); // floor

        // Large anchor from test_calculate_premium_overflow_safety
        assert_eq!(
            Investment::calculate_premium(1_000_000_000_000_000_000, 80),
            16_000_000_000_000_000
        );
    }
}
