//! Dispute resolution module for the QuickLendX protocol.
//!
//! # Overview
//!
//! This module defines the dispute lifecycle types and the locking semantics
//! that prevent resolved disputes from being overwritten.  The authoritative
//! contract entry-points live in `lib.rs`; this module provides the shared
//! types, constants, and helper logic consumed by those entry-points.
//!
//! # State Machine
//!
//! ```text
//! (none) ──create──▶ Disputed ──review──▶ UnderReview ──resolve──▶ Resolved
//!                                                                      │
//!                                                              TERMINAL / LOCKED
//! ```
//!
//! - `Disputed`    – dispute opened by business or investor.
//! - `UnderReview` – admin has acknowledged and is investigating.
//! - `Resolved`    – admin has written a final resolution.  **This state is
//!                   terminal and immutable.**  No further transitions are
//!                   possible without an explicit policy-override path.
//!
//! # Security Model
//!
//! 1. **Locking**: `resolve_dispute` enforces `UnderReview → Resolved` only.
//!    A second call on an already-resolved dispute returns
//!    `DisputeNotUnderReview`, preventing silent overwrites.
//! 2. **Role separation**: only the invoice's business owner or its investor
//!    may open a dispute; only the platform admin may advance or resolve it.
//! 3. **Input validation**: all string fields are length-checked against
//!    `protocol_limits` constants before any state is written.
//! 4. **One-dispute-per-invoice**: duplicate creation is rejected with
//!    `DisputeAlreadyExists`.
//! 5. **Replay prevention**: `creator.require_auth()` and
//!    `admin.require_auth()` ensure every state change is cryptographically
//!    signed by the authorised party.

use crate::invoice::DisputeStatus;
use crate::protocol_limits::{
    MAX_DISPUTE_EVIDENCE_LENGTH, MAX_DISPUTE_REASON_LENGTH, MAX_DISPUTE_RESOLUTION_LENGTH,
};
use crate::QuickLendXError;

// ---------------------------------------------------------------------------
// Re-export constants so callers can reference them from this module.
// ---------------------------------------------------------------------------

/// Maximum length (in characters) of a dispute reason string.
pub const REASON_MAX: u32 = MAX_DISPUTE_REASON_LENGTH;

/// Maximum length (in characters) of a dispute evidence string.
pub const EVIDENCE_MAX: u32 = MAX_DISPUTE_EVIDENCE_LENGTH;

/// Maximum length (in characters) of a dispute resolution string.
pub const RESOLUTION_MAX: u32 = MAX_DISPUTE_RESOLUTION_LENGTH;

// ---------------------------------------------------------------------------
// Validation helpers
// ---------------------------------------------------------------------------

/// @notice Validate a dispute reason string.
///
/// @dev Rejects empty strings and strings exceeding `REASON_MAX` characters.
///
/// @param len  The byte-length of the reason string.
/// @return `Ok(())` if valid, `Err(InvalidDisputeReason)` otherwise.
pub fn validate_reason_len(len: u32) -> Result<(), QuickLendXError> {
    if len == 0 || len > REASON_MAX {
        return Err(QuickLendXError::InvalidDisputeReason);
    }
    Ok(())
}

/// @notice Validate a dispute evidence string.
///
/// @dev Rejects empty strings and strings exceeding `EVIDENCE_MAX` characters.
///
/// @param len  The byte-length of the evidence string.
/// @return `Ok(())` if valid, `Err(InvalidDisputeEvidence)` otherwise.
pub fn validate_evidence_len(len: u32) -> Result<(), QuickLendXError> {
    if len == 0 || len > EVIDENCE_MAX {
        return Err(QuickLendXError::InvalidDisputeEvidence);
    }
    Ok(())
}

/// @notice Validate a dispute resolution string.
///
/// @dev Rejects empty strings and strings exceeding `RESOLUTION_MAX` characters.
///      Uses `InvalidDisputeReason` for consistency with the existing error set.
///
/// @param len  The byte-length of the resolution string.
/// @return `Ok(())` if valid, `Err(InvalidDisputeReason)` otherwise.
pub fn validate_resolution_len(len: u32) -> Result<(), QuickLendXError> {
    if len == 0 || len > RESOLUTION_MAX {
        return Err(QuickLendXError::InvalidDisputeReason);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// State-machine helpers
// ---------------------------------------------------------------------------

/// @notice Assert that a dispute is in the `Disputed` state.
///
/// @dev Used by `put_dispute_under_review` to enforce the forward-only
///      state machine.  Returns `DisputeNotFound` when no dispute exists
///      and `InvalidStatus` when the dispute is in any other state.
///
/// @param status  The current `DisputeStatus` of the invoice.
/// @return `Ok(())` if the status is `Disputed`.
pub fn require_disputed(status: &DisputeStatus) -> Result<(), QuickLendXError> {
    match status {
        DisputeStatus::None => Err(QuickLendXError::DisputeNotFound),
        DisputeStatus::Disputed => Ok(()),
        _ => Err(QuickLendXError::InvalidStatus),
    }
}

/// @notice Assert that a dispute is in the `UnderReview` state.
///
/// @dev Used by `resolve_dispute` to enforce the locking invariant.
///      Returns `DisputeNotFound` when no dispute exists and
///      `DisputeNotUnderReview` for any other state — including `Resolved`,
///      which prevents silent overwrites of the terminal state.
///
/// @param status  The current `DisputeStatus` of the invoice.
/// @return `Ok(())` if the status is `UnderReview`.
pub fn require_under_review(status: &DisputeStatus) -> Result<(), QuickLendXError> {
    match status {
        DisputeStatus::None => Err(QuickLendXError::DisputeNotFound),
        DisputeStatus::UnderReview => Ok(()),
        _ => Err(QuickLendXError::DisputeNotUnderReview),
    }
}

/// @notice Return `true` when the dispute is in a terminal (locked) state.
///
/// @dev A `Resolved` dispute cannot be modified without an explicit
///      policy-override path.  Callers can use this predicate to gate
///      any future override logic.
///
/// @param status  The current `DisputeStatus` of the invoice.
/// @return `true` if `status == Resolved`.
pub fn is_locked(status: &DisputeStatus) -> bool {
    matches!(status, DisputeStatus::Resolved)
}
