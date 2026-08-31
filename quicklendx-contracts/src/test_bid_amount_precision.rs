//! Tests for `bid_amount` — exact integer rules, sign, overflow, and
//! auction-selection ranking-key precision for bid amounts (Issue QE-2026-08).
//!
//! These tests lock in the acceptance-criteria boundaries:
//!
//! | Bucket | Values exercised | Expectation |
//! |---|---|---|
//! | Success | `1`, `7` (dust), `1_000_000`, `MAX_BID_AMOUNT` | `Ok` |
//! | Zero / sign | `0`, `-1`, `i128::MIN` | `InvalidAmount` |
//! | Ceiling | `MAX_BID_AMOUNT + 1`, `i128::MAX` | `InvalidAmount` |
//! | Invoice ceiling | `bid == invoice` / `bid == invoice + 1` | `Ok` / `InvalidAmount` |
//! | Return floor | `ret == amount - 1` / `== amount` / `== amount + 1` | `Err` / `Ok` / `Ok` |
//! | Profit key | `checked_bid_profit` vs independent `i128` oracle | exact match, no clamp |
//! | Exposure sum | near-`i128::MAX` running totals | `Ok` / `ArithmeticOverflow` |
//! | Near-overflow | `MAX * 10_000 / 10_000`, `(MAX+1) * 10_000` | `Ok(MAX)` / `ArithmeticOverflow` |
//! | Fractional (floor) | `(100, 333)`, `(1, 5_000)`, `(1_234_567, 1)` | `floor(amount * bps / 10_000)` |
//! | Conversion boundary | `MAX_BID_AMOUNT` vs `i128::MAX` in bps + profit math | exact boundary proven |
//!
//! The boundary sweep tests compare the helpers against an **independent
//! oracle** written directly from the specification (plain integer comparison
//! and `i128` / `u128` arithmetic), not by reusing the code under test.
//!
//! All helpers under test are pure and side-effect free; the repeated-invocation
//! tests below additionally pin that rejected operations are deterministic and
//! leave no state behind.

#![cfg(test)]

use crate::bid_amount::{
    checked_bid_amount_sum, checked_bid_fee_amount, checked_bid_profit, validate_bid,
    validate_bid_amount_ceiling, validate_expected_return, BPS_DENOMINATOR, MAX_BID_AMOUNT,
};
use crate::errors::QuickLendXError;
use crate::invoice_amount::MAX_INVOICE_AMOUNT;

// ============================================================================
// Independent oracles (written from the spec, not from the code under test)
// ============================================================================

/// Reference rule for the sign/ceiling predicate.
///
/// `amount` is valid iff `amount > 0` and `amount <= i128::MAX / 10_000`.
fn oracle_amount_ok(amount: i128) -> bool {
    amount > 0 && amount <= i128::MAX / 10_000
}

/// Reference rule for the full bid-submission predicate.
fn oracle_bid_ok(bid_amount: i128, expected_return: i128, invoice_amount: i128) -> bool {
    if !oracle_amount_ok(bid_amount) {
        return false;
    }
    if invoice_amount > 0 && bid_amount > invoice_amount {
        return false;
    }
    if !oracle_amount_ok(expected_return) {
        return false;
    }
    expected_return >= bid_amount
}

/// Reference implementation of `expected_return - bid_amount` using `i128`
/// checked arithmetic — the auction-selection ranking key.
fn oracle_profit(expected_return: i128, bid_amount: i128) -> Option<i128> {
    expected_return.checked_sub(bid_amount)
}

/// Reference implementation of `floor(amount * bps / 10_000)` using `u128`
/// arithmetic so the oracle itself can never overflow for the fed inputs.
fn oracle_fee(amount: i128, bps: u32) -> Option<i128> {
    if amount <= 0 || bps > BPS_DENOMINATOR as u32 {
        return None;
    }
    let product = (amount as u128).checked_mul(bps as u128)?;
    Some((product / BPS_DENOMINATOR as u128) as i128)
}

// ============================================================================
// Success path
// ============================================================================

#[test]
fn test_accepts_valid_bid_amounts_across_scale() {
    for amount in [
        1i128,
        7,
        10,
        1_000,
        1_000_000,
        1_500_000,
        123_456_789,
        MAX_BID_AMOUNT,
    ] {
        assert_eq!(
            validate_bid_amount_ceiling(amount),
            Ok(()),
            "bid amount {amount} must be accepted"
        );
    }
}

/// A representative full submission: principal below the invoice face value,
/// expected return above principal, both within the ceiling.
#[test]
fn test_accepts_representative_full_submission() {
    assert_eq!(validate_bid(900_000, 1_000_000, 1_000_000), Ok(()));
    // Zero-profit bid (return floor is inclusive).
    assert_eq!(validate_bid(1_000_000, 1_000_000, 1_000_000), Ok(()));
    // Bid exactly equal to the invoice amount (invoice ceiling is inclusive).
    assert_eq!(validate_bid(1_000_000, 1_200_000, 1_000_000), Ok(()));
}

/// The bid ceiling is defined to equal the invoice ceiling so a bid that fits
/// under an accepted invoice amount automatically fits under the bid ceiling.
#[test]
fn test_max_bid_amount_matches_invoice_ceiling() {
    assert_eq!(MAX_BID_AMOUNT, MAX_INVOICE_AMOUNT);
    assert_eq!(MAX_BID_AMOUNT, i128::MAX / 10_000);
    assert!(
        MAX_BID_AMOUNT.checked_mul(BPS_DENOMINATOR).is_some(),
        "MAX_BID_AMOUNT * 10_000 must fit in i128"
    );
    assert!(
        (MAX_BID_AMOUNT + 1).checked_mul(BPS_DENOMINATOR).is_none(),
        "(MAX_BID_AMOUNT + 1) * 10_000 must overflow i128 — this is why the ceiling exists"
    );
}

// ============================================================================
// Sign and zero rejection
// ============================================================================

#[test]
fn test_rejects_zero_bid_amount() {
    assert_eq!(
        validate_bid_amount_ceiling(0),
        Err(QuickLendXError::InvalidAmount)
    );
    assert_eq!(
        validate_bid(0, 1_000, 1_000),
        Err(QuickLendXError::InvalidAmount)
    );
}

#[test]
fn test_rejects_negative_amounts() {
    for amount in [-1i128, -10_000, i128::MIN] {
        assert_eq!(
            validate_bid_amount_ceiling(amount),
            Err(QuickLendXError::InvalidAmount),
            "negative bid amount {amount} must be rejected"
        );
    }
    // Negative expected_return is rejected regardless of principal.
    assert_eq!(
        validate_expected_return(-1, 1_000),
        Err(QuickLendXError::InvalidAmount)
    );
    assert_eq!(
        validate_bid(1_000, -1, 10_000),
        Err(QuickLendXError::InvalidAmount)
    );
}

// ============================================================================
// Overflow ceiling rejection
// ============================================================================

#[test]
fn test_rejects_amount_above_ceiling() {
    for amount in [MAX_BID_AMOUNT + 1, i128::MAX - 1, i128::MAX] {
        assert_eq!(
            validate_bid_amount_ceiling(amount),
            Err(QuickLendXError::InvalidAmount),
            "bid amount {amount} above the ceiling must be rejected"
        );
    }
    // The ceiling still bites through the full-submission entrypoint, even when
    // the (nonsensical) invoice amount is also huge.
    assert_eq!(
        validate_bid(MAX_BID_AMOUNT + 1, MAX_BID_AMOUNT + 1, i128::MAX),
        Err(QuickLendXError::InvalidAmount)
    );
    assert_eq!(
        validate_bid(1_000, i128::MAX, 10_000),
        Err(QuickLendXError::InvalidAmount)
    );
}

// ============================================================================
// Invoice ceiling (inclusive)
// ============================================================================

#[test]
fn test_invoice_ceiling_is_inclusive() {
    let invoice = 1_000_000i128;
    // One below / exactly at / one above the invoice face value.
    assert_eq!(validate_bid(999_999, 1_000_000, invoice), Ok(()));
    assert_eq!(validate_bid(1_000_000, 1_000_000, invoice), Ok(()));
    assert_eq!(
        validate_bid(1_000_001, 1_100_000, invoice),
        Err(QuickLendXError::InvalidAmount)
    );
}

#[test]
fn test_non_positive_invoice_amount_disables_invoice_ceiling() {
    // With no invoice reference the bid ceiling still applies, but the
    // invoice-relative check is skipped.
    assert_eq!(validate_bid(5_000, 5_000, 0), Ok(()));
    assert_eq!(validate_bid(5_000, 5_000, -1), Ok(()));
}

// ============================================================================
// Return floor (inclusive)
// ============================================================================

#[test]
fn test_return_floor_is_inclusive() {
    let amount = 1_000i128;
    assert_eq!(
        validate_expected_return(amount - 1, amount),
        Err(QuickLendXError::InvalidAmount)
    );
    assert_eq!(validate_expected_return(amount, amount), Ok(()));
    assert_eq!(validate_expected_return(amount + 1, amount), Ok(()));
    // Same three cases through the full entrypoint.
    assert_eq!(
        validate_bid(amount, amount - 1, 10_000),
        Err(QuickLendXError::InvalidAmount)
    );
    assert_eq!(validate_bid(amount, amount, 10_000), Ok(()));
    assert_eq!(validate_bid(amount, amount + 1, 10_000), Ok(()));
}

// ============================================================================
// Auction-selection profit key
// ============================================================================

/// For every pair accepted by `validate_bid`, the checked profit key is
/// non-negative, within `[0, MAX_BID_AMOUNT)`, and identical to the legacy
/// `saturating_sub` result — so the auction ranking is deterministic.
#[test]
fn test_profit_key_matches_saturating_for_valid_pairs() {
    let pairs = [
        (1i128, 1i128),
        (1, 2),
        (1_000, 1_000),
        (1_000, 5_000),
        (1, MAX_BID_AMOUNT),
        (MAX_BID_AMOUNT, MAX_BID_AMOUNT),
        (MAX_BID_AMOUNT - 1, MAX_BID_AMOUNT),
    ];
    for (bid_amount, expected_return) in pairs {
        assert_eq!(
            validate_bid(bid_amount, expected_return, MAX_BID_AMOUNT),
            Ok(()),
            "pair ({bid_amount}, {expected_return}) should be valid"
        );
        let checked = checked_bid_profit(expected_return, bid_amount).unwrap();
        assert_eq!(checked, expected_return - bid_amount);
        assert_eq!(checked, expected_return.saturating_sub(bid_amount));
        assert!(
            (0..MAX_BID_AMOUNT).contains(&checked),
            "profit {checked} out of [0, MAX_BID_AMOUNT) for ({bid_amount}, {expected_return})"
        );
    }
}

/// An adversarial pair outside the `validate_bid` contract that would overflow
/// `i128` under subtraction surfaces as `ArithmeticOverflow` instead of a
/// silently clamped rank.
#[test]
fn test_profit_key_reports_overflow_instead_of_clamping() {
    assert_eq!(
        checked_bid_profit(i128::MAX, -1),
        Err(QuickLendXError::ArithmeticOverflow)
    );
    assert_eq!(
        checked_bid_profit(i128::MAX, i128::MIN),
        Err(QuickLendXError::ArithmeticOverflow)
    );
    assert_eq!(
        checked_bid_profit(i128::MIN, 1),
        Err(QuickLendXError::ArithmeticOverflow)
    );
    // `saturating_sub` would have clamped all three to `i128::MAX` / `i128::MIN`
    // and let an attacker pin a deterministic rank.
    assert_eq!(i128::MAX.saturating_sub(-1), i128::MAX);
}

// ============================================================================
// Per-investor exposure sum
// ============================================================================

#[test]
fn test_exposure_sum_is_exact_and_reports_overflow() {
    assert_eq!(checked_bid_amount_sum(0, 0), Ok(0));
    assert_eq!(
        checked_bid_amount_sum(MAX_BID_AMOUNT, MAX_BID_AMOUNT),
        Ok(2 * MAX_BID_AMOUNT)
    );
    // Running total already at the very top: any positive add overflows.
    assert_eq!(
        checked_bid_amount_sum(i128::MAX, 1),
        Err(QuickLendXError::ArithmeticOverflow)
    );
    assert_eq!(
        checked_bid_amount_sum(i128::MAX - 1, 2),
        Err(QuickLendXError::ArithmeticOverflow)
    );
    // Exact boundary: total + add == i128::MAX is fine.
    assert_eq!(checked_bid_amount_sum(i128::MAX - 1, 1), Ok(i128::MAX));
}

// ============================================================================
// Near-overflow / bps math
// ============================================================================

#[test]
fn test_fee_math_at_max_amount_max_bps_is_exact() {
    assert_eq!(
        checked_bid_fee_amount(MAX_BID_AMOUNT, BPS_DENOMINATOR as u32),
        Ok(MAX_BID_AMOUNT)
    );
}

#[test]
fn test_fee_math_overflow_boundary_is_exactly_ceiling_plus_one() {
    assert_eq!(
        checked_bid_fee_amount(MAX_BID_AMOUNT + 1, BPS_DENOMINATOR as u32),
        Err(QuickLendXError::ArithmeticOverflow)
    );
    assert_eq!(
        checked_bid_fee_amount(i128::MAX, BPS_DENOMINATOR as u32),
        Err(QuickLendXError::ArithmeticOverflow)
    );
    // The validation layer rejects these inputs before they can reach math.
    assert_eq!(
        validate_bid_amount_ceiling(MAX_BID_AMOUNT + 1),
        Err(QuickLendXError::InvalidAmount)
    );
    assert_eq!(
        validate_bid_amount_ceiling(i128::MAX),
        Err(QuickLendXError::InvalidAmount)
    );
}

#[test]
fn test_fee_math_truncates_like_existing_formula() {
    assert_eq!(checked_bid_fee_amount(100, 333), Ok(3)); // 3.33 → 3
    assert_eq!(checked_bid_fee_amount(1, 5_000), Ok(0)); // 0.5 → 0
    assert_eq!(checked_bid_fee_amount(7, 10_000), Ok(7)); // 7.0 → 7
    assert_eq!(checked_bid_fee_amount(10_000, 2_500), Ok(2_500)); // exact quarter
    assert_eq!(checked_bid_fee_amount(1_234_567, 1), Ok(123)); // 123.4567 → 123
    assert_eq!(checked_bid_fee_amount(1_000_000, 0), Ok(0)); // 0 bps → 0 fee
}

#[test]
fn test_fee_math_rejects_invalid_inputs() {
    assert_eq!(
        checked_bid_fee_amount(0, 1_000),
        Err(QuickLendXError::InvalidAmount)
    );
    assert_eq!(
        checked_bid_fee_amount(-1, 1_000),
        Err(QuickLendXError::InvalidAmount)
    );
    assert_eq!(
        checked_bid_fee_amount(1_000, BPS_DENOMINATOR as u32 + 1),
        Err(QuickLendXError::InvalidFeeBasisPoints)
    );
    assert_eq!(
        checked_bid_fee_amount(1_000, u32::MAX),
        Err(QuickLendXError::InvalidFeeBasisPoints)
    );
}

// ============================================================================
// Independent-oracle boundary sweeps
// ============================================================================

#[test]
fn test_ceiling_boundary_sweep_against_oracle() {
    let interesting = [
        i128::MIN,
        i128::MIN + 1,
        -10_000,
        -1,
        0,
        1,
        10,
        1_000,
        10_000,
        1_000_000,
        MAX_BID_AMOUNT - 1,
        MAX_BID_AMOUNT,
        MAX_BID_AMOUNT + 1,
        i128::MAX - 1,
        i128::MAX,
    ];
    for amount in interesting {
        let expected = if oracle_amount_ok(amount) {
            Ok(())
        } else {
            Err(QuickLendXError::InvalidAmount)
        };
        assert_eq!(
            validate_bid_amount_ceiling(amount),
            expected,
            "oracle mismatch for bid amount {amount}"
        );
    }
}

/// Deterministic pseudo-random sweep (simple LCG) over the full
/// `(bid_amount, expected_return, invoice_amount)` space, compared against the
/// independent oracle for both the accept/reject decision and — on accepted
/// pairs — the profit key.
#[test]
fn test_random_sweep_against_oracle() {
    let mut state: u64 = 0x0806_2026_0808;
    let mut next = || {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        state
    };
    for _ in 0..5_000 {
        let bid_amount = (next() as i128) ^ ((next() >> 33) as i128).wrapping_shl(64);
        let expected_return = (next() as i128) ^ ((next() >> 33) as i128).wrapping_shl(64);
        let invoice_amount = (next() as i128) ^ ((next() >> 33) as i128).wrapping_shl(64);

        let got = validate_bid(bid_amount, expected_return, invoice_amount);
        let want = if oracle_bid_ok(bid_amount, expected_return, invoice_amount) {
            Ok(())
        } else {
            Err(QuickLendXError::InvalidAmount)
        };
        assert_eq!(
            got, want,
            "oracle mismatch for ({bid_amount}, {expected_return}, {invoice_amount})"
        );

        if got.is_ok() {
            // Accepted pair: the profit key must be exact and never clamp.
            let checked = checked_bid_profit(expected_return, bid_amount);
            assert_eq!(
                checked,
                Ok(oracle_profit(expected_return, bid_amount).unwrap())
            );
            let p = checked.unwrap();
            assert!(
                (0..MAX_BID_AMOUNT).contains(&p),
                "profit {p} out of range for accepted ({bid_amount}, {expected_return})"
            );
        }
    }
}

#[test]
fn test_fee_math_sweep_against_oracle() {
    let interesting = [
        -1i128,
        0,
        1,
        7,
        100,
        1_000,
        1_000_000,
        MAX_BID_AMOUNT - 1,
        MAX_BID_AMOUNT,
        MAX_BID_AMOUNT + 1,
        i128::MAX,
    ];
    for amount in interesting {
        for bps in [0u32, 1, 333, 2_500, 5_000, 10_000] {
            match oracle_fee(amount, bps) {
                Some(expected) if oracle_amount_ok(amount) => {
                    assert_eq!(
                        checked_bid_fee_amount(amount, bps),
                        Ok(expected),
                        "fee oracle mismatch for amount {amount}, bps {bps}"
                    );
                }
                Some(_) => {
                    if let Ok(fee) = checked_bid_fee_amount(amount, bps) {
                        assert_eq!(fee, oracle_fee(amount, bps).unwrap());
                    }
                }
                None => {
                    assert!(
                        checked_bid_fee_amount(amount, bps).is_err(),
                        "fee math must reject invalid amount {amount} / bps {bps}"
                    );
                }
            }
        }
    }
}

// ============================================================================
// Determinism / no partial state
// ============================================================================

/// Validation is pure and deterministic: the same rejected input yields the
/// same error on every invocation, with no hidden counters or storage writes.
/// In the entrypoints these checks run *before* any storage mutation, so
/// rejected, stale, and repeated operations cannot leave a partial bid record.
#[test]
fn test_validation_is_pure_and_repeatable() {
    let invalid = [
        (0i128, 0i128, 0i128),
        (-1, 1_000, 10_000),
        (MAX_BID_AMOUNT + 1, MAX_BID_AMOUNT + 1, i128::MAX),
        (2_000, 1_000, 10_000),   // return below principal
        (20_000, 20_000, 10_000), // bid above invoice
    ];
    for (bid_amount, expected_return, invoice_amount) in invalid {
        let first = validate_bid(bid_amount, expected_return, invoice_amount);
        assert!(
            first.is_err(),
            "({bid_amount}, {expected_return}, {invoice_amount}) must be rejected"
        );
        for _ in 0..5 {
            assert_eq!(
                validate_bid(bid_amount, expected_return, invoice_amount),
                first,
                "rejected input must fail identically on every call"
            );
        }
    }
    for (bid_amount, expected_return, invoice_amount) in [
        (1i128, 1i128, 1i128),
        (900_000, 1_000_000, 1_000_000),
        (MAX_BID_AMOUNT, MAX_BID_AMOUNT, MAX_BID_AMOUNT),
    ] {
        for _ in 0..5 {
            assert_eq!(
                validate_bid(bid_amount, expected_return, invoice_amount),
                Ok(())
            );
        }
    }
}
