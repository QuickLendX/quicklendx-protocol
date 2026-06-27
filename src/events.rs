/// # Events Module
///
/// Defines the event types and emission interface for the QuickLendX protocol.
///
/// ## Design
///
/// Event emission is modelled through the [`EventEmitter`] trait.  Production
/// call sites pass a [`NoopEmitter`] — a zero-sized, `#[inline(always)]` stub
/// that the compiler eliminates entirely — so there is no overhead on the hot
/// (unblocked) path.  Test call sites pass a [`VecEmitter`] that captures
/// every emission for assertion.
///
/// ## Indexer compatibility
///
/// All topic strings (e.g. [`TOPIC_PAUSE_BLOCKED`]) are `'static` string
/// constants.  They must not be renamed or reordered without a coordinated
/// indexer migration, because external consumers key on the exact byte
/// sequence of each topic.  See [`crate::pause`] for the entrypoint-symbol
/// constants that appear in the `entrypoint` field.

// ─────────────────────────────────────────────────────────────────────────────
// Topic constants
// ─────────────────────────────────────────────────────────────────────────────

/// Stable event topic for pause-blocked rejections.
///
/// Indexers must subscribe to this exact string.  Do not rename without
/// a versioned migration.
#[cfg(any(test, feature = "test-support"))]
extern crate alloc;

#[cfg(any(test, feature = "test-support"))]
use alloc::vec::Vec;

pub const TOPIC_PAUSE_BLOCKED: &str = "PauseBlocked";

// ─────────────────────────────────────────────────────────────────────────────
// Event payload
// ─────────────────────────────────────────────────────────────────────────────

/// Emitted on every call that `require_unpaused` rejects.
///
/// Carries enough context for off-chain monitors to attribute rejected
/// traffic to a specific entrypoint, caller, and ledger instant.
///
/// # Field stability
///
/// Field names and ordering are part of the public event schema.  Adding
/// fields is backwards-compatible; removing or reordering fields is a
/// breaking change that requires an indexer migration notice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PauseBlockedEvent {
    /// Stable symbol identifying the entrypoint that was blocked.
    ///
    /// One of the `EP_*` constants from [`crate::pause`].  Indexers use
    /// this field to bucket rejected-traffic metrics per entrypoint.
    pub entrypoint: &'static str,

    /// Numeric identifier of the caller whose invocation was rejected.
    pub caller: u64,

    /// Ledger timestamp at which the rejection occurred.
    pub ledger_ts: u64,
}

// ─────────────────────────────────────────────────────────────────────────────
// Emitter trait
// ─────────────────────────────────────────────────────────────────────────────

/// Sink for protocol events.
///
/// Production code passes [`NoopEmitter`]; tests pass [`VecEmitter`] to
/// capture emissions for assertion.
pub trait EventEmitter {
    /// Called by `require_unpaused` on every blocked entrypoint invocation.
    fn emit_pause_blocked(&mut self, event: PauseBlockedEvent);
}

// ─────────────────────────────────────────────────────────────────────────────
// NoopEmitter — zero overhead on the unblocked hot path
// ─────────────────────────────────────────────────────────────────────────────

/// A zero-sized [`EventEmitter`] that discards every emission.
///
/// Pass this to `require_unpaused` in production code.  The `#[inline(always)]`
/// body is optimised away entirely by the compiler, so there is no runtime
/// cost on the unpaused hot path.
pub struct NoopEmitter;

impl EventEmitter for NoopEmitter {
    #[inline(always)]
    fn emit_pause_blocked(&mut self, _event: PauseBlockedEvent) {}
}

// ─────────────────────────────────────────────────────────────────────────────
// VecEmitter — test helper
// ─────────────────────────────────────────────────────────────────────────────

/// An [`EventEmitter`] that accumulates every emission into a `Vec`.
///
/// Available in test builds only.  Typical usage:
///
/// ```rust
/// # use quicklendx_contracts::events::VecEmitter;
/// # use quicklendx_contracts::pause::{PauseState, EP_INVOICE_UPLOAD};
/// let mut sink = VecEmitter::default();
/// let state = PauseState::active();
/// let _ = state.require_unpaused(EP_INVOICE_UPLOAD, 42, 1_000, &mut sink);
/// assert_eq!(sink.events().len(), 1);
/// ```
#[cfg(any(test, feature = "test-support"))]
#[derive(Debug, Default)]
pub struct VecEmitter {
    collected: Vec<PauseBlockedEvent>,
}

#[cfg(any(test, feature = "test-support"))]
impl VecEmitter {
    /// Returns a slice of all collected events in emission order.
    pub fn events(&self) -> &[PauseBlockedEvent] {
        &self.collected
    }

    /// Clears all collected events.
    pub fn clear(&mut self) {
        self.collected.clear();
    }
}

#[cfg(any(test, feature = "test-support"))]
impl EventEmitter for VecEmitter {
    fn emit_pause_blocked(&mut self, event: PauseBlockedEvent) {
        self.collected.push(event);
    }
}
