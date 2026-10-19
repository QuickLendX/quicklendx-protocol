//! Tests for `invoice_amount` — exact integer rules, scale, sign, and
//! overflow rejection for invoice amounts (Issue #2432).
//!
//! These tests lock in the acceptance-criteria boundaries:
//!
//! | Bucket | Values exercised | Expectation |
//! |---|---|---|
//! | Success | `1`, `7` (dust), `1_000_000` (1 token @ 6dp), `MAX_INVOICE_AMOUNT` | `Ok` |
//! | Zero / sign | `0`, `-1`, `i128::MIN` | `InvalidAmount` |
//! | Ceiling | `MAX_INVOICE_AMOUNT + 1`, `i128::MAX` | `InvalidAmount` |
//! | Minimum | `min-1` / `min` / `min+1` | `Err` / `Ok` / `Ok` (inclusive) |
//! | Scale | `0`, `18` vs `19`, `u32::MAX` | `Ok` vs `InvalidCurrency` |
//! | Near-overflow | `MAX * 10_000 / 10_000`, `(MAX+1) * 10_000` | `Ok(MAX)` vs `ArithmeticOverflow` |
//! | Fractional (floor) | `(100, 333)`, `(1, 5_000)`, `(1_234_567, 1)` | `floor(amount * bps / 10_000)` |
//! | Conversion boundary | `MAX_INVOICE_AMOUNT` vs `i128::MAX` in bps math | exact boundary proven |
//!
//! The boundary sweep tests compare the helpers against an **independent
//! oracle** written directly from the specification (plain integer comparison
//! and `u128` arithmetic), not by reusing the code under test.
//!
//! All helpers under test are pure and side-effect free; the repeated-invocation
//! tests below additionally pin that rejected operations are deterministic and
//! leave no state behind.

#![cfg(test)]

use crate::errors::QuickLendXError;
use crate::invoice_amount::{
    check_currency_scale, checked_fee_amount, validate_invoice_amount,
    validate_invoice_amount_ceiling, BPS_DENOMINATOR, MAX_CURRENCY_DECIMALS,
    MAX_INVOICE_AMOUNT,
};

// ============================================================================
// Independent oracles (written from the spec, not from the code under test)
// ============================================================================

/// Reference rule for the sign/ceiling predicate.
///
/// `amount` is valid iff `amount > 0` and `amount <= i128::MAX / 10_000`.
fn oracle_accepts(amount: i128) -> bool {
    amount > 0 && amount <= i128::MAX / 10_000
}

/// Reference implementation of `floor(amount * bps / 10_000)` using `u128`
/// arithmetic so the oracle itself can never overflow for the fed inputs.
///
/// Returns `None` for inputs the validation rules reject (non-positive amount,
/// bps above the denominator), mirroring the error contract.
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

/// Any positive integer in the smallest currency unit is accepted, from a
/// single unit (dust) through the documented ceiling. "Fractional" values are
/// represented exactly as integers (e.g. `1.5` tokens at 6 decimals is
/// `1_500_000`) and must be accepted.
#[test]
fn test_accepts_valid_amounts_across_scale() {
    for amount in [1i128, 7, 10, 1_000, 1_000_000, 1_500_000, 123_456_789, MAX_INVOICE_AMOUNT] {
        assert_eq!(
            validate_invoice_amount_ceiling(amount),
            Ok(()),
            "amount {amount} must be accepted"
        );
    }
}

/// `MAX_INVOICE_AMOUNT` is defined as `i128::MAX / 10_000` — the largest value
/// for which every downstream bps computation stays overflow-free. Lock the
/// constant to that documented formula so a silent re-derivation is caught.
#[test]
fn test_max_invoice_amount_matches_documented_formula() {
    assert_eq!(MAX_INVOICE_AMOUNT, i128::MAX / 10_000);
    assert!(
        MAX_INVOICE_AMOUNT
            .checked_mul(BPS_DENOMINATOR)
            .is_some(),
        "MAX_INVOICE_AMOUNT * 10_000 must fit in i128"
    );
    assert!(
        (MAX_INVOICE_AMOUNT + 1)
            .checked_mul(BPS_DENOMINATOR)
            .is_none(),
        "(MAX_INVOICE_AMOUNT + 1) * 10_000 must overflow i128 — this is why the ceiling exists"
    );
}

// ============================================================================
// Sign and zero rejection
// ============================================================================

#[test]
fn test_rejects_zero() {
    assert_eq!(
        validate_invoice_amount_ceiling(0),
        Err(QuickLendXError::InvalidAmount)
    );
}

#[test]
fn test_rejects_negative_amounts() {
    for amount in [-1i128, -10_000, i128::MIN] {
        assert_eq!(
            validate_invoice_amount_ceiling(amount),
            Err(QuickLendXError::InvalidAmount),
            "negative amount {amount} must be rejected"
        );
    }
}

// ============================================================================
// Overflow ceiling rejection
// ============================================================================

#[test]
fn test_rejects_amount_above_ceiling() {
    for amount in [MAX_INVOICE_AMOUNT + 1, i128::MAX - 1, i128::MAX] {
        assert_eq!(
            validate_invoice_amount_ceiling(amount),
            Err(QuickLendXError::InvalidAmount),
            "amount {amount} above the ceiling must be rejected"
        );
    }
}

// ============================================================================
// Minimum boundary (inclusive)
// ============================================================================

#[test]
fn test_minimum_boundary_is_inclusive() {
    let min = 1_000i128;
    // One below the floor.
    assert_eq!(
        validate_invoice_amount(999, min),
        Err(QuickLendXError::InvalidAmount)
    );
    // Exactly at the floor (inclusive).
    assert_eq!(validate_invoice_amount(1_000, min), Ok(()));
    // One above the floor.
    assert_eq!(validate_invoice_amount(1_001, min), Ok(()));
    // A non-positive floor disables the minimum rule (configuration rejects
    // such floors at set_protocol_limits time).
    assert_eq!(validate_invoice_amount(5, 0), Ok(()));
    assert_eq!(validate_invoice_amount(5, -1), Ok(()));
}

/// The ceiling rule still applies when only the minimum is checked — a value
/// that passes the floor but exceeds the ceiling is still rejected.
#[test]
fn test_minimum_does_not_bypass_ceiling() {
    assert_eq!(
        validate_invoice_amount(i128::MAX, 1),
        Err(QuickLendXError::InvalidAmount)
    );
    assert_eq!(
        validate_invoice_amount(MAX_INVOICE_AMOUNT + 1, 1),
        Err(QuickLendXError::InvalidAmount)
    );
}

// ============================================================================
// Currency scale (decimals) boundaries
// ============================================================================

#[test]
fn test_scale_accepts_boundary_decimals() {
    // 0 = atomic units; 18 = the documented maximum — both inclusive.
    assert_eq!(check_currency_scale(0), Ok(()));
    assert_eq!(check_currency_scale(MAX_CURRENCY_DECIMALS), Ok(()));
    assert_eq!(check_currency_scale(7), Ok(())); // Stellar SAC default
}

#[test]
fn test_scale_rejects_overprecision() {
    for decimals in [MAX_CURRENCY_DECIMALS + 1, 19, 20, u32::MAX] {
        assert_eq!(
            check_currency_scale(decimals),
            Err(QuickLendXError::InvalidCurrency),
            "decimals={decimals} must be rejected as over-precision"
        );
    }
}

// ============================================================================
// Near-overflow / bps math
// ============================================================================

/// At the ceiling with a 100 % bps rate the fee equals the amount exactly:
/// `MAX * 10_000 / 10_000 == MAX`.
#[test]
fn test_fee_math_at_max_amount_max_bps_is_exact() {
    assert_eq!(
        checked_fee_amount(MAX_INVOICE_AMOUNT, BPS_DENOMINATOR as u32),
        Ok(MAX_INVOICE_AMOUNT)
    );
}

/// One unit above the ceiling makes the very same computation overflow `i128`.
/// Together with the previous test this proves the ceiling is exactly the
/// conversion boundary: entrypoints reject `MAX + 1` with `InvalidAmount`
/// *before* any fee math runs, so overflow is unreachable in the lifecycle.
#[test]
fn test_fee_math_overflow_boundary_is_exactly_ceiling_plus_one() {
    assert_eq!(
        checked_fee_amount(MAX_INVOICE_AMOUNT + 1, BPS_DENOMINATOR as u32),
        Err(QuickLendXError::ArithmeticOverflow)
    );
    assert_eq!(
        checked_fee_amount(i128::MAX, BPS_DENOMINATOR as u32),
        Err(QuickLendXError::ArithmeticOverflow)
    );
    // And the validation layer rejects these inputs before they can reach math.
    assert_eq!(
        validate_invoice_amount_ceiling(MAX_INVOICE_AMOUNT + 1),
        Err(QuickLendXError::InvalidAmount)
    );
    assert_eq!(
        validate_invoice_amount_ceiling(i128::MAX),
        Err(QuickLendXError::InvalidAmount)
    );
}

/// The bps formula floors toward zero, matching the existing fee pipeline
/// (`floor(amount * bps / 10_000)`). Fractional remainders never round up and
/// never produce dust above the amount.
#[test]
fn test_fee_math_truncates_like_existing_formula() {
    assert_eq!(checked_fee_amount(100, 333), Ok(3)); // 100*333/10000 = 3.33 → 3
    assert_eq!(checked_fee_amount(1, 5_000), Ok(0)); // 0.5 → 0 (floor)
    assert_eq!(checked_fee_amount(7, 10_000), Ok(7)); // 7.0 → 7
    assert_eq!(checked_fee_amount(10_000, 2_500), Ok(2_500)); // exact quarter
    assert_eq!(checked_fee_amount(1_234_567, 1), Ok(123)); // 123.4567 → 123
    assert_eq!(checked_fee_amount(1_000_000, 0), Ok(0)); // 0 bps → 0 fee
}

#[test]
fn test_fee_math_rejects_invalid_inputs() {
    assert_eq!(
        checked_fee_amount(0, 1_000),
        Err(QuickLendXError::InvalidAmount)
    );
    assert_eq!(
        checked_fee_amount(-1, 1_000),
        Err(QuickLendXError::InvalidAmount)
    );
    assert_eq!(
        checked_fee_amount(1_000, BPS_DENOMINATOR as u32 + 1),
        Err(QuickLendXError::InvalidFeeBasisPoints)
    );
    assert_eq!(
        checked_fee_amount(1_000, u32::MAX),
        Err(QuickLendXError::InvalidFeeBasisPoints)
    );
}

// ============================================================================
// Independent-oracle boundary sweeps
// ============================================================================

/// Sweep every interesting boundary value of `i128` through the validation
/// helper and compare against the independent oracle.
#[test]
fn test_boundary_sweep_against_oracle() {
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
        MAX_INVOICE_AMOUNT - 1,
        MAX_INVOICE_AMOUNT,
        MAX_INVOICE_AMOUNT + 1,
        i128::MAX - 1,
        i128::MAX,
    ];
    for amount in interesting {
        let expected = if oracle_accepts(amount) {
            Ok(())
        } else {
            Err(QuickLendXError::InvalidAmount)
        };
        assert_eq!(
            validate_invoice_amount_ceiling(amount),
            expected,
            "oracle mismatch for amount {amount}"
        );
    }
}

/// Deterministic pseudo-random sweep (simple LCG) so the comparison is not
/// limited to hand-picked constants.
#[test]
fn test_random_sweep_against_oracle() {
    let mut state: u64 = 0x2432_2026_0801;
    for _ in 0..2_000 {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let amount = (state as i128) ^ ((state >> 32) as i128).wrapping_shl(96);
        let expected = if oracle_accepts(amount) {
            Ok(())
        } else {
            Err(QuickLendXError::InvalidAmount)
        };
        assert_eq!(
            validate_invoice_amount_ceiling(amount),
            expected,
            "oracle mismatch for random amount {amount}"
        );
    }
}

/// For every accepted amount in the sweep, the fee math must agree exactly
/// with the independent `u128`-based oracle; rejected amounts must never
/// produce an `Ok` fee (they are caught upstream by the validation layer).
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
        MAX_INVOICE_AMOUNT - 1,
        MAX_INVOICE_AMOUNT,
        MAX_INVOICE_AMOUNT + 1,
        i128::MAX,
    ];
    for amount in interesting {
        for bps in [0u32, 1, 333, 2_500, 5_000, 10_000] {
            match oracle_fee(amount, bps) {
                Some(expected) if oracle_accepts(amount) => {
                    assert_eq!(
                        checked_fee_amount(amount, bps),
                        Ok(expected),
                        "fee oracle mismatch for amount {amount}, bps {bps}"
                    );
                }
                Some(_) => {
                    // Out-of-bounds amount (above the ceiling): the fee helper
                    // may still compute a value when the multiplication does
                    // not overflow, but it must never silently wrap. When it
                    // does return a value it must match the oracle.
                    if let Ok(fee) = checked_fee_amount(amount, bps) {
                        assert_eq!(fee, oracle_fee(amount, bps).unwrap());
                    }
                }
                None => {
                    assert!(
                        checked_fee_amount(amount, bps).is_err(),
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
/// In the lifecycle entrypoints these checks run *before* any storage
/// mutation, so rejected, stale, and repeated operations cannot leave a
/// partial invoice record.
#[test]
fn test_validation_is_pure_and_repeatable() {
    let invalid = [0i128, -1, MAX_INVOICE_AMOUNT + 1, i128::MAX];
    for amount in invalid {
        let first = validate_invoice_amount_ceiling(amount);
        for _ in 0..5 {
            assert_eq!(
                validate_invoice_amount_ceiling(amount),
                first,
                "rejected amount {amount} must fail identically on every call"
            );
        }
        assert!(first.is_err(), "amount {amount} must be rejected");
    }
    // Accepted amounts are equally stable.
    for amount in [1i128, 1_000_000, MAX_INVOICE_AMOUNT] {
        for _ in 0..5 {
            assert_eq!(validate_invoice_amount_ceiling(amount), Ok(()));
        }
    }
}
