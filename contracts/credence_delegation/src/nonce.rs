/// Maximum number of nonces that a single `invalidate_nonce_range` call may
/// skip forward.
///
/// # Bound and rationale
///
/// `invalidate_nonce_range` advances `current` from its present value to
/// `new_nonce` in a single atomic step.  Without a cap an attacker who
/// controls `new_nonce` (or a buggy delegator) could push `current` to
/// `u64::MAX` in one call, permanently locking the delegation channel.
/// Capping the span to 10 000 per call:
///
/// 1. **Prevents accidental lockout** — a typo in `new_nonce` can discard at
///    most 10 000 pre-signed payloads, not all of them.
/// 2. **Bounds gas / iteration cost** — on-chain implementations that must
///    iterate over the range have a predictable worst-case of 10 000 steps.
/// 3. **Preserves forward progress** — large invalidations can always be
///    achieved by chaining multiple calls, each within the limit.
///
/// The value 10 000 was chosen to match the protocol's BPS denominator
/// (basis-point scale), making it easy to reason about in the context of
/// percentage-based limits elsewhere in QuickLendX.
pub const MAX_NONCE_INVALIDATION_SPAN: u64 = 10_000;

// ─────────────────────────────────────────────────────────────────────────────
// Error types
// ─────────────────────────────────────────────────────────────────────────────

/// Errors returned by nonce operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NonceError {
    /// The payload nonce does not equal the current stored nonce.
    ///
    /// This covers three cases uniformly:
    /// - Replay: `payload.nonce < current` — nonce has already been consumed
    ///   or invalidated.
    /// - Skipped: `payload.nonce > current` — out-of-order execution is not
    ///   permitted; nonces must be used strictly in sequence.
    InvalidNonce,

    /// The requested invalidation span exceeds [`MAX_NONCE_INVALIDATION_SPAN`].
    ///
    /// `span = new_nonce - current` was larger than the allowed maximum.
    SpanTooLarge {
        /// The span that was requested.
        span: u64,
    },

    /// `new_nonce` must be strictly greater than the current nonce.
    NonceMustIncrease,
}

// ─────────────────────────────────────────────────────────────────────────────
// NonceStore
// ─────────────────────────────────────────────────────────────────────────────

/// Per-delegator monotonic nonce counter.
///
/// # Monotonicity invariant
///
/// `current` only ever increases.  Execution of a delegated payload requires
/// `payload.nonce == current`; on success `current` becomes `current + 1`.
/// Because `current` never decreases, every nonce that has been passed or
/// skipped is **permanently unspendable**.
///
/// Formally: after any sequence of `consume` and `invalidate_nonce_range`
/// calls that leave `current == N`, no payload with `nonce < N` can ever
/// satisfy the equality check again.
///
/// # Replay-window coverage
///
/// `invalidate_nonce_range(new_nonce)` covers the half-open range
/// `[old_current, new_nonce)`.  Every nonce `k` in that range satisfies
/// `k < new_nonce == current`, so `consume(k)` will return
/// [`NonceError::InvalidNonce`] for all time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NonceStore {
    current: u64,
}

impl NonceStore {
    /// Creates a `NonceStore` whose first valid nonce is `initial`.
    pub fn new(initial: u64) -> Self {
        Self { current: initial }
    }

    /// Returns the current (next-expected) nonce value.
    pub fn current(&self) -> u64 {
        self.current
    }

    /// Invalidates all pre-signed payloads whose nonce falls in the
    /// half-open range `[current, new_nonce)` by advancing `current` to
    /// `new_nonce`.
    ///
    /// # Proof of unspendability
    ///
    /// Let `N_before = current` before the call and `N_after = new_nonce`
    /// after.  For any nonce `k` where `N_before ≤ k < N_after`:
    ///
    /// ```text
    /// 1. After the call: current = N_after.
    /// 2. Execution requires payload.nonce == current  (exact equality).
    /// 3. k < N_after = current  →  k ≠ current  →  consume(k) = Err(InvalidNonce).
    /// 4. current is monotonically non-decreasing, so current ≥ N_after forever.
    /// 5. Therefore consume(k) = Err(InvalidNonce) for all future states too.
    /// ```
    ///
    /// Nonces below `N_before` were already unspendable by the same argument.
    /// The union of the two ranges gives the full set `[0, N_after)`.
    ///
    /// # Errors
    ///
    /// * [`NonceError::NonceMustIncrease`] — `new_nonce ≤ current`.
    /// * [`NonceError::SpanTooLarge`]      — `new_nonce - current > MAX_NONCE_INVALIDATION_SPAN`.
    ///
    /// On error the store is **not modified**.
    pub fn invalidate_nonce_range(&mut self, new_nonce: u64) -> Result<(), NonceError> {
        if new_nonce <= self.current {
            return Err(NonceError::NonceMustIncrease);
        }
        let span = new_nonce - self.current;
        if span > MAX_NONCE_INVALIDATION_SPAN {
            return Err(NonceError::SpanTooLarge { span });
        }
        self.current = new_nonce;
        Ok(())
    }

    /// Attempts to execute a delegated payload identified by `nonce`.
    ///
    /// Succeeds only when `nonce == current`.  On success `current` is
    /// incremented by 1, preventing re-use of the same nonce.
    ///
    /// # Errors
    ///
    /// [`NonceError::InvalidNonce`] when `nonce ≠ current` (replay or skip).
    pub fn consume(&mut self, nonce: u64) -> Result<(), NonceError> {
        if nonce != self.current {
            return Err(NonceError::InvalidNonce);
        }
        // Monotonic increment — current can never decrease after this point.
        self.current += 1;
        Ok(())
    }

    /// Verifies that `nonce` matches `current` without consuming it.
    ///
    /// Useful for pre-flight signature checks before committing side effects.
    ///
    /// # Errors
    ///
    /// [`NonceError::InvalidNonce`] when `nonce ≠ current`.
    pub fn verify(&self, nonce: u64) -> Result<(), NonceError> {
        if nonce != self.current {
            return Err(NonceError::InvalidNonce);
        }
        Ok(())
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Unit tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_store_accepts_zero() {
        let mut s = NonceStore::new(0);
        assert!(s.consume(0).is_ok());
        assert_eq!(s.current(), 1);
    }

    #[test]
    fn consume_wrong_nonce_is_invalid() {
        let mut s = NonceStore::new(5);
        assert_eq!(s.consume(4), Err(NonceError::InvalidNonce));
        assert_eq!(s.consume(6), Err(NonceError::InvalidNonce));
        assert_eq!(s.current(), 5); // store unchanged on error
    }

    #[test]
    fn invalidate_advances_current() {
        let mut s = NonceStore::new(0);
        s.invalidate_nonce_range(100).unwrap();
        assert_eq!(s.current(), 100);
    }

    #[test]
    fn invalidate_rejects_non_increase() {
        let mut s = NonceStore::new(50);
        assert_eq!(s.invalidate_nonce_range(50), Err(NonceError::NonceMustIncrease));
        assert_eq!(s.invalidate_nonce_range(10), Err(NonceError::NonceMustIncrease));
        assert_eq!(s.current(), 50);
    }

    #[test]
    fn invalidate_at_max_span_succeeds() {
        let mut s = NonceStore::new(0);
        assert!(s.invalidate_nonce_range(MAX_NONCE_INVALIDATION_SPAN).is_ok());
        assert_eq!(s.current(), MAX_NONCE_INVALIDATION_SPAN);
    }

    #[test]
    fn invalidate_over_max_span_rejected() {
        let mut s = NonceStore::new(0);
        let result = s.invalidate_nonce_range(MAX_NONCE_INVALIDATION_SPAN + 1);
        assert!(matches!(result, Err(NonceError::SpanTooLarge { span: 10_001 })));
        assert_eq!(s.current(), 0); // store unchanged
    }

    #[test]
    fn verify_does_not_advance_current() {
        let s = NonceStore::new(7);
        assert!(s.verify(7).is_ok());
        assert_eq!(s.current(), 7);
        assert_eq!(s.verify(6), Err(NonceError::InvalidNonce));
        assert_eq!(s.verify(8), Err(NonceError::InvalidNonce));
    }
}
