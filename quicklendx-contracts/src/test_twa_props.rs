//! Property-based invariant tests for `profits::compute_twa`.
//!
//! # Invariant under test
//!
//! Roll forward a sequence of random ledger deltas (balance, duration) pairs.
//! The time-weighted average computed by `compute_twa` must match the
//! `compute_twa_reference` oracle for all random inputs:
//!
//! ```text
//! compute_twa(deltas) == compute_twa_reference(deltas)
//! ```
//!
//! Additional boundary invariants verified:
//!
//! * **Empty input returns zero** — `compute_twa(&[])` == 0
//! * **Single delta returns the balance** — TWA of one entry equals its balance
//! * **All-zero durations return zero** — no division by zero
//! * **Constant balance is preserved** — TWA of identical balances == that balance
//!
//! # Run command
//! ```bash
//! PROPTEST_CASES=1000 cargo test --features fuzz-tests twa_matches_reference_impl
//! ```

#[cfg(all(test, feature = "fuzz-tests"))]
mod test_twa_props {
    extern crate alloc;

    use crate::profits::{compute_twa, compute_twa_reference, LedgerDelta};
    use proptest::prelude::*;

    // -------------------------------------------------------------------------
    // Strategy helpers
    // -------------------------------------------------------------------------

    /// Generate a single `LedgerDelta` with realistic protocol ranges:
    ///   * balance: 0 .. 10^15 stroops (roughly 10 billion XLM-equivalent)
    ///   * duration: 0 .. 17_280 ledgers (≈ 1 day at 5-second ledger pace)
    fn arb_delta() -> impl Strategy<Value = LedgerDelta> {
        (0i128..1_000_000_000_000_000_i128, 0u32..17_280u32).prop_map(
            |(balance, duration_ledgers)| LedgerDelta { balance, duration_ledgers },
        )
    }

    /// Generate a non-empty vec of up to 32 ledger deltas.
    fn arb_deltas() -> impl Strategy<Value = alloc::vec::Vec<LedgerDelta>> {
        prop::collection::vec(arb_delta(), 1..=32)
    }

    // -------------------------------------------------------------------------
    // Property tests
    // -------------------------------------------------------------------------

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(500))]

        /// **Core invariant**: roll-forward TWA matches the reference implementation
        /// for all random ledger-delta sequences.
        #[test]
        fn twa_matches_reference_impl(deltas in arb_deltas()) {
            let got = compute_twa(&deltas);
            let expected = compute_twa_reference(&deltas);
            prop_assert_eq!(
                got, expected,
                "compute_twa({:?}) = {} but reference returned {}",
                deltas, got, expected
            );
        }

        /// **Non-negativity**: TWA is always >= 0 when all balances are >= 0.
        #[test]
        fn twa_is_non_negative(deltas in arb_deltas()) {
            let twa = compute_twa(&deltas);
            prop_assert!(
                twa >= 0,
                "TWA must be non-negative, got {} for {:?}",
                twa, deltas
            );
        }

        /// **Upper bound**: TWA never exceeds the maximum balance in the sequence.
        #[test]
        fn twa_does_not_exceed_max_balance(deltas in arb_deltas()) {
            let max_bal = deltas.iter().map(|d| d.balance).max().unwrap_or(0);
            let twa = compute_twa(&deltas);
            prop_assert!(
                twa <= max_bal,
                "TWA {} must not exceed max balance {} in {:?}",
                twa, max_bal, deltas
            );
        }

        /// **Constant-balance preservation**: when every delta has the same balance,
        /// the TWA equals that balance (regardless of durations, as long as total > 0).
        #[test]
        fn constant_balance_is_preserved(
            balance in 1i128..1_000_000_000_000_000_i128,
            n in 1usize..=16usize,
            dur in 1u32..1_000u32,
        ) {
            let deltas: alloc::vec::Vec<LedgerDelta> = (0..n)
                .map(|_| LedgerDelta { balance, duration_ledgers: dur })
                .collect();
            let twa = compute_twa(&deltas);
            prop_assert_eq!(
                twa, balance,
                "constant balance {} must be preserved by TWA, got {}",
                balance, twa
            );
        }
    }

    // -------------------------------------------------------------------------
    // Explicit sad-path and boundary tests
    // -------------------------------------------------------------------------

    /// Empty delta slice must return zero without panicking.
    #[test]
    fn returns_zero_when_deltas_empty() {
        assert_eq!(compute_twa(&[]), 0);
    }

    /// Single delta: TWA must equal the balance (duration > 0).
    #[test]
    fn single_delta_returns_balance() {
        let delta = LedgerDelta { balance: 500_000, duration_ledgers: 100 };
        assert_eq!(compute_twa(&[delta]), 500_000);
    }

    /// All-zero durations must not panic and must return zero.
    #[test]
    fn returns_zero_when_all_durations_are_zero() {
        let deltas = alloc::vec![
            LedgerDelta { balance: 1_000, duration_ledgers: 0 },
            LedgerDelta { balance: 2_000, duration_ledgers: 0 },
        ];
        assert_eq!(compute_twa(&deltas), 0);
    }

    /// Zero-balance deltas yield zero TWA regardless of duration.
    #[test]
    fn returns_zero_when_balance_is_zero() {
        let deltas = alloc::vec![
            LedgerDelta { balance: 0, duration_ledgers: 100 },
            LedgerDelta { balance: 0, duration_ledgers: 200 },
        ];
        assert_eq!(compute_twa(&deltas), 0);
    }
}
