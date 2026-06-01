//! # Nonce Replay-Window Property Tests
//!
//! Verifies that `invalidate_nonce_range(new_nonce)` permanently renders
//! every nonce in `[0, new_nonce)` unspendable — regardless of the initial
//! counter value, the invalidation span, or which specific nonce the attacker
//! attempts to replay.
//!
//! ## Security claim under test
//!
//! For any delegator whose `NonceStore` has been advanced to `N` (by any
//! combination of `consume` and `invalidate_nonce_range` calls), an attacker
//! holding a pre-signed payload with `nonce < N` cannot execute that payload:
//! `consume(nonce)` will always return `Err(NonceError::InvalidNonce)`.
//!
//! ## Running
//!
//! ```bash
//! cargo test -p credence_delegation nonce_replay
//! ```
//!
//! Property tests run 10 000 randomly generated cases each.
//! Deterministic edge-case tests always run unconditionally.

use credence_delegation::nonce::{NonceError, NonceStore, MAX_NONCE_INVALIDATION_SPAN};
use proptest::prelude::*;

// ─────────────────────────────────────────────────────────────────────────────
// Property tests — 10 000 cases each
// ─────────────────────────────────────────────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(10_000))]

    /// **Core replay invariant**: after `invalidate_nonce_range(new_nonce)`,
    /// every nonce `n` in `[0, new_nonce)` is rejected by `consume`.
    ///
    /// Generates:
    /// - `start`  — initial counter value (avoids overflow in `start + span`)
    /// - `span`   — invalidation size in `[1, MAX_NONCE_INVALIDATION_SPAN]`
    /// - `replay` — attacker-chosen nonce, pinned to `[0, new_nonce)` via modulo
    #[test]
    fn prop_invalidated_nonces_are_unspendable(
        start  in 0u64..u64::MAX - MAX_NONCE_INVALIDATION_SPAN,
        span   in 1u64..=MAX_NONCE_INVALIDATION_SPAN,
        replay in any::<u64>(),
    ) {
        let new_nonce = start + span;
        // Pin replay to [0, new_nonce) — the full invalidated prefix.
        let replay = replay % new_nonce; // new_nonce >= 1, so no division by zero

        let mut store = NonceStore::new(start);
        store.invalidate_nonce_range(new_nonce).unwrap();

        prop_assert_eq!(
            store.consume(replay),
            Err(NonceError::InvalidNonce),
            "Replay of nonce {replay} must be rejected after invalidation \
             to {new_nonce} (start={start} span={span})"
        );
    }

    /// **Minimal-span invariant**: span = 1 invalidates exactly one nonce.
    ///
    /// After invalidation by 1, nonce `start` is rejected and `start + 1`
    /// (the new `current`) is accepted.
    #[test]
    fn prop_minimal_span_invalidates_exactly_one_nonce(
        start in 0u64..u64::MAX - 1,
    ) {
        let mut store = NonceStore::new(start);
        store.invalidate_nonce_range(start + 1).unwrap();

        // The invalidated nonce must be rejected.
        prop_assert_eq!(
            store.consume(start),
            Err(NonceError::InvalidNonce),
            "Nonce {start} must be invalid after minimal invalidation"
        );
        // The next nonce (new current) must be accepted.
        prop_assert!(
            store.consume(start + 1).is_ok(),
            "Nonce {} must be valid after minimal invalidation", start + 1
        );
    }

    /// **Max-span boundary**: span = MAX succeeds; the boundary nonce
    /// `new_nonce - 1` (last of the invalidated range) is rejected, while
    /// `new_nonce` itself is accepted.
    #[test]
    fn prop_max_span_boundary_nonce_rejected(
        start in 0u64..u64::MAX - MAX_NONCE_INVALIDATION_SPAN,
    ) {
        let new_nonce = start + MAX_NONCE_INVALIDATION_SPAN;
        let mut store = NonceStore::new(start);

        prop_assert!(
            store.invalidate_nonce_range(new_nonce).is_ok(),
            "Max-span invalidation must succeed (start={start})"
        );

        // Boundary: new_nonce - 1 is inside [start, new_nonce) — must fail.
        prop_assert_eq!(
            store.consume(new_nonce - 1),
            Err(NonceError::InvalidNonce),
            "Boundary nonce {} must be rejected", new_nonce - 1
        );

        // new_nonce is now the live counter — must succeed.
        prop_assert!(
            store.consume(new_nonce).is_ok(),
            "new_nonce {new_nonce} must be accepted"
        );
    }

    /// **Over-limit rejection**: any span > MAX is always rejected.
    #[test]
    fn prop_over_max_span_always_rejected(
        start     in 0u64..u64::MAX - MAX_NONCE_INVALIDATION_SPAN - 1,
        extra_span in 1u64..1_000u64,
    ) {
        let over_span = MAX_NONCE_INVALIDATION_SPAN + extra_span;
        let mut store = NonceStore::new(start);
        let result = store.invalidate_nonce_range(start + over_span);

        prop_assert!(
            matches!(result, Err(NonceError::SpanTooLarge { .. })),
            "Span {over_span} > MAX must be rejected, got {result:?}"
        );
        // Store must be unchanged on error.
        prop_assert_eq!(store.current(), start);
    }

    /// **First valid nonce accepted**: `new_nonce` (the value assigned to
    /// `current` after invalidation) is always accepted immediately.
    #[test]
    fn prop_first_valid_nonce_accepted(
        start in 0u64..u64::MAX - MAX_NONCE_INVALIDATION_SPAN,
        span  in 1u64..=MAX_NONCE_INVALIDATION_SPAN,
    ) {
        let new_nonce = start + span;
        let mut store = NonceStore::new(start);
        store.invalidate_nonce_range(new_nonce).unwrap();

        prop_assert!(
            store.consume(new_nonce).is_ok(),
            "new_nonce {new_nonce} must be the first accepted nonce"
        );
    }

    /// **Future nonces rejected**: nonces strictly greater than `new_nonce`
    /// are also invalid — they do not match `current` and cannot be used
    /// out of order.
    #[test]
    fn prop_future_nonces_rejected(
        start  in 0u64..u64::MAX - MAX_NONCE_INVALIDATION_SPAN - 1_000,
        span   in 1u64..=MAX_NONCE_INVALIDATION_SPAN,
        offset in 1u64..1_000u64,
    ) {
        let new_nonce = start + span;
        let mut store = NonceStore::new(start);
        store.invalidate_nonce_range(new_nonce).unwrap();

        let future = new_nonce + offset;
        prop_assert_eq!(
            store.consume(future),
            Err(NonceError::InvalidNonce),
            "Future nonce {future} must be rejected (current is {new_nonce})"
        );
    }

    /// **Monotonicity**: `current` never decreases after any sequence of
    /// valid `invalidate_nonce_range` calls.
    #[test]
    fn prop_current_is_non_decreasing(
        start in 0u64..u64::MAX - 2 * MAX_NONCE_INVALIDATION_SPAN,
        s1    in 1u64..=MAX_NONCE_INVALIDATION_SPAN,
        s2    in 1u64..=MAX_NONCE_INVALIDATION_SPAN,
    ) {
        let mut store = NonceStore::new(start);
        let after_first = start + s1;
        let after_second = after_first + s2;

        store.invalidate_nonce_range(after_first).unwrap();
        prop_assert_eq!(store.current(), after_first);

        store.invalidate_nonce_range(after_second).unwrap();
        prop_assert_eq!(store.current(), after_second);

        // Attempting to go back must always fail.
        prop_assert_eq!(
            store.invalidate_nonce_range(after_first),
            Err(NonceError::NonceMustIncrease)
        );
        // Current is unchanged after the failed attempt.
        prop_assert_eq!(store.current(), after_second);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Deterministic edge-case tests (always run, no feature flag required)
// ─────────────────────────────────────────────────────────────────────────────

/// Invalidation by exactly 1: the single skipped nonce is immediately invalid.
#[test]
fn nonce_replay_invalidation_by_one() {
    let mut store = NonceStore::new(0);
    assert!(store.invalidate_nonce_range(1).is_ok());
    assert_eq!(store.consume(0), Err(NonceError::InvalidNonce));
    assert!(store.consume(1).is_ok());
}

/// Invalidation by MAX: all 10 000 nonces [0, MAX) are invalid.
#[test]
fn nonce_replay_invalidation_by_max() {
    let mut store = NonceStore::new(0);
    assert!(store.invalidate_nonce_range(MAX_NONCE_INVALIDATION_SPAN).is_ok());

    for n in 0..MAX_NONCE_INVALIDATION_SPAN {
        assert_eq!(
            store.consume(n),
            Err(NonceError::InvalidNonce),
            "nonce {n} should be invalid after max-span invalidation"
        );
    }
    // Only MAX itself is the live nonce.
    assert!(store.consume(MAX_NONCE_INVALIDATION_SPAN).is_ok());
}

/// Invalidation by MAX + 1 must be rejected; store must not change.
#[test]
fn nonce_replay_invalidation_by_max_plus_one_rejected() {
    let mut store = NonceStore::new(0);
    let result = store.invalidate_nonce_range(MAX_NONCE_INVALIDATION_SPAN + 1);
    assert!(
        matches!(result, Err(NonceError::SpanTooLarge { span: 10_001 })),
        "Expected SpanTooLarge(10_001), got {result:?}"
    );
    assert_eq!(store.current(), 0, "Store must be unchanged on over-span error");
}

/// Boundary case: `new_nonce - 1` (last invalidated) fails; `new_nonce` succeeds.
///
/// This test validates the half-open range semantics at a non-zero start,
/// confirming the boundary nonce cannot be replayed even after bumping by
/// `MAX - 1` positions.
#[test]
fn nonce_replay_boundary_last_invalidated() {
    const START: u64 = 500;
    // Bump by MAX - 1 = 9_999 (within the allowed span).
    const SPAN: u64 = MAX_NONCE_INVALIDATION_SPAN - 1;
    const NEW: u64 = START + SPAN;

    let mut store = NonceStore::new(START);
    store.invalidate_nonce_range(NEW).unwrap();

    // NEW - 1 is the last nonce in [START, NEW) — must be rejected.
    assert_eq!(
        store.consume(NEW - 1),
        Err(NonceError::InvalidNonce),
        "Boundary nonce {} must be invalid", NEW - 1
    );
    // NEW is the new current — must succeed.
    assert!(store.consume(NEW).is_ok());
}

/// Once invalidated, a nonce remains invalid through subsequent invalidations.
#[test]
fn nonce_replay_persists_across_further_invalidations() {
    let mut store = NonceStore::new(0);
    store.invalidate_nonce_range(5).unwrap();
    store.invalidate_nonce_range(10).unwrap();

    // Nonces 0–9 were invalidated in two separate calls — all must fail.
    for n in 0..10 {
        assert_eq!(
            store.consume(n),
            Err(NonceError::InvalidNonce),
            "nonce {n} should remain invalid after chained invalidations"
        );
    }
    assert!(store.consume(10).is_ok());
}

/// `invalidate_nonce_range` must not accept a non-increasing value.
#[test]
fn nonce_replay_no_decrease() {
    let mut store = NonceStore::new(100);
    assert!(store.invalidate_nonce_range(101).is_ok());
    assert_eq!(store.invalidate_nonce_range(100), Err(NonceError::NonceMustIncrease));
    assert_eq!(store.invalidate_nonce_range(101), Err(NonceError::NonceMustIncrease));
    assert_eq!(store.current(), 101);
}
