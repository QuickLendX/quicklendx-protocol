/// # Pause Module
///
/// Provides a protocol-wide pause guard that short-circuits every protected
/// entrypoint and emits a structured [`PauseBlockedEvent`] so off-chain
/// monitors can quantify rejected traffic during incidents.
///
/// ## Hot-path guarantee
///
/// When `paused == false`, `require_unpaused` returns `Ok(())` at the first
/// branch without touching the emitter.  The [`NoopEmitter`] used in
/// production is a zero-sized type whose single method is `#[inline(always)]`
/// and produces no instructions, so the event system has **zero cost on the
/// live path**.
///
/// ## Entrypoint symbols
///
/// Each guarded entrypoint is identified by a `&'static str` constant
/// (`EP_*`).  These strings are part of the public event schema and must not
/// be renamed without a coordinated indexer migration.
///
/// [`PauseBlockedEvent`]: crate::events::PauseBlockedEvent
/// [`NoopEmitter`]: crate::events::NoopEmitter
use crate::events::{EventEmitter, PauseBlockedEvent};

// ─────────────────────────────────────────────────────────────────────────────
// Entrypoint symbols — stable, indexer-visible
// ─────────────────────────────────────────────────────────────────────────────

/// Entrypoint symbol for the invoice-upload action.
///
/// Stable: renaming this constant is a breaking change for indexers.
pub const EP_INVOICE_UPLOAD: &str = "invoice_upload";

/// Entrypoint symbol for the bid-placement action.
///
/// Stable: renaming this constant is a breaking change for indexers.
pub const EP_BID_PLACEMENT: &str = "bid_placement";

/// Entrypoint symbol for the settlement-initiation action.
///
/// Stable: renaming this constant is a breaking change for indexers.
pub const EP_SETTLEMENT_INITIATION: &str = "settlement_initiation";

/// Entrypoint symbol for the escrow-release action.
///
/// Stable: renaming this constant is a breaking change for indexers.
pub const EP_ESCROW_RELEASE: &str = "escrow_release";

/// Entrypoint symbol for the investment action.
///
/// Stable: renaming this constant is a breaking change for indexers.
pub const EP_INVESTMENT_ACTION: &str = "investment_action";

/// Exhaustive slice of every guarded entrypoint symbol.
///
/// Used by tests to assert complete coverage and by monitoring tools to
/// validate that every expected entrypoint produces a [`PauseBlockedEvent`]
/// when the protocol is paused.
///
/// [`PauseBlockedEvent`]: crate::events::PauseBlockedEvent
pub const ALL_ENTRYPOINTS: &[&str] = &[
    EP_INVOICE_UPLOAD,
    EP_BID_PLACEMENT,
    EP_SETTLEMENT_INITIATION,
    EP_ESCROW_RELEASE,
    EP_INVESTMENT_ACTION,
];

// ─────────────────────────────────────────────────────────────────────────────
// Error type
// ─────────────────────────────────────────────────────────────────────────────

/// Error returned by [`PauseState::require_unpaused`] on the blocked path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PauseError {
    /// The protocol is currently paused; the caller's action was rejected.
    ContractPaused,
}

// ─────────────────────────────────────────────────────────────────────────────
// PauseState
// ─────────────────────────────────────────────────────────────────────────────

/// Carries the protocol pause flag.
///
/// In a live on-chain deployment this value is read from persistent storage
/// before each guarded entrypoint.  In unit tests it is constructed directly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PauseState {
    /// `true` when the protocol is paused and all protected entrypoints must
    /// be blocked; `false` during normal operation.
    pub paused: bool,
}

impl PauseState {
    /// Constructs a `PauseState` from a raw flag.
    pub fn new(paused: bool) -> Self {
        Self { paused }
    }

    /// Returns a `PauseState` that blocks all entrypoints.
    pub fn active() -> Self {
        Self { paused: true }
    }

    /// Returns a `PauseState` that allows all entrypoints.
    pub fn inactive() -> Self {
        Self { paused: false }
    }

    /// Guards an entrypoint against the pause flag.
    ///
    /// On the **unpaused path** (`paused == false`) this function returns
    /// `Ok(())` immediately — the emitter is never called and the event
    /// system costs nothing.
    ///
    /// On the **blocked path** (`paused == true`) the function:
    /// 1. Constructs a [`PauseBlockedEvent`] from the supplied context.
    /// 2. Passes it to `emitter.emit_pause_blocked(…)`.
    /// 3. Returns `Err(PauseError::ContractPaused)`.
    ///
    /// # Parameters
    /// - `entrypoint` — Stable symbol for the blocked entrypoint.  Must be
    ///   one of the `EP_*` constants defined in this module.
    /// - `caller`     — Numeric ID of the calling account.
    /// - `ledger_ts`  — Current ledger timestamp in seconds since the epoch.
    /// - `emitter`    — Receives the event; pass [`NoopEmitter`] in production.
    ///
    /// # Hot-path cost
    ///
    /// When `paused == false` the branch resolves to a single comparison and a
    /// return; the emitter generic parameter is monomorphised away.  The
    /// [`NoopEmitter`] variant produces **zero additional instructions**.
    ///
    /// [`PauseBlockedEvent`]: crate::events::PauseBlockedEvent
    /// [`NoopEmitter`]: crate::events::NoopEmitter
    pub fn require_unpaused<E: EventEmitter>(
        &self,
        entrypoint: &'static str,
        caller: u64,
        ledger_ts: u64,
        emitter: &mut E,
    ) -> Result<(), PauseError> {
        if !self.paused {
            return Ok(());
        }
        emitter.emit_pause_blocked(PauseBlockedEvent {
            entrypoint,
            caller,
            ledger_ts,
        });
        Err(PauseError::ContractPaused)
    }
}
