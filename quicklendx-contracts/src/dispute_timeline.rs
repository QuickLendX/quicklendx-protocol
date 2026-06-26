//! Dispute timeline endpoint - normalizes dispute lifecycle events into a
//! chronologically ordered, redacted sequence suitable for UI consumption.
//!
//! # Design
//!
//! On-chain dispute state is stored as a flat [`Dispute`] struct embedded in
//! each [`Invoice`].  This module reconstructs the implicit event sequence
//! (Opened -> UnderReview -> Resolved) from that struct, redacts fields that
//! must not leak to unprivileged callers (evidence, resolution text), and
//! returns a paginated [`DisputeTimeline`] value.
//!
//! # Invariants
//!
//! The timeline is intentionally stricter than the dispute storage shape:
//! - `Opened` always comes first.
//! - `UnderReview` may appear at most once and only after `Opened`.
//! - `Resolved` may appear at most once and only after `UnderReview`.
//! - `update_dispute_evidence` never appends a visible timeline entry.
//! - `Resolved` is terminal; later actions must be rejected by the state machine.
//!
//! The executable version of this ordering lives in
//! `docs/dispute-timeline-invariants.md`, and the property tests lock that
//! document to the code path so drift becomes a test failure.
//!
//! # Security
//!
//! - Evidence is **always** redacted from timeline entries; it is only
//!   accessible via `get_dispute_details` to authorized parties.
//! - Resolution text is redacted until the dispute reaches `Resolved` status.
//! - No PII from invoice metadata is included.
//! - Pagination bounds use saturating arithmetic to prevent overflow.
//! - The dispute timeline is a user-facing summary, not a replacement for the
//!   append-only invoice audit trail.

use crate::errors::QuickLendXError;
use crate::storage::InvoiceStorage;
use crate::types::{Dispute, DisputeResolution, DisputeStatus, OptionalDisputeResolution};
use soroban_sdk::{contracttype, symbol_short, Address, BytesN, Env, String, Symbol, Vec};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Maximum entries returned in a single timeline page.
pub const TIMELINE_MAX_PAGE_SIZE: u32 = 50;

/// Sentinel address used when a field is redacted (all-zero Stellar address).
const REDACTED_ADDRESS: &str = "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF";

/// Persistent dispute-review timestamp namespace.
const DISPUTE_REVIEW_AT_KEY: Symbol = symbol_short!("dsp_rvat");

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// A single event in the dispute lifecycle, safe for UI consumption.
///
/// Fields that could expose sensitive information are replaced with
/// redacted placeholders rather than omitted, so callers always receive
/// a consistent shape regardless of dispute state.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DisputeTimelineEntry {
    /// Monotonically increasing position within the timeline (0-based).
    pub sequence: u32,
    /// Human-readable event label: "Opened", "UnderReview", or "Resolved".
    pub event: String,
    /// Ledger timestamp when this event occurred.
    pub timestamp: u64,
    /// Address of the actor who triggered this event.
    /// Redacted (zero address) for the `UnderReview` step to avoid leaking
    /// admin identity to non-admin callers.
    pub actor: Address,
    /// Short summary visible to all callers.
    /// For "Opened": the dispute reason (not evidence).
    /// For "UnderReview": empty string.
    /// For "Resolved": resolution text (only when status == Resolved,
    ///   otherwise redacted as empty string).
    pub summary: String,
    /// Structured resolution outcome (only present for "Resolved" events
    /// that were resolved using resolve_dispute_structured).
    pub resolution_outcome: DisputeResolution,
}

/// Paginated dispute timeline response.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DisputeTimeline {
    /// Ordered slice of timeline entries for this page.
    pub entries: Vec<DisputeTimelineEntry>,
    /// Total number of events in the full (unpaginated) timeline.
    pub total: u32,
    /// Whether additional pages exist after this one.
    pub has_more: bool,
    /// Current dispute status, reflecting on-chain truth.
    pub current_status: DisputeStatus,
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn redacted_address(env: &Env) -> Address {
    Address::from_str(env, REDACTED_ADDRESS)
}

fn dispute_review_at_key(invoice_id: &BytesN<32>) -> (Symbol, BytesN<32>) {
    (DISPUTE_REVIEW_AT_KEY, invoice_id.clone())
}

/// Persist the exact ledger timestamp when a dispute entered `UnderReview`.
pub(crate) fn set_under_review_timestamp(env: &Env, invoice_id: &BytesN<32>, timestamp: u64) {
    env.storage()
        .persistent()
        .set(&dispute_review_at_key(invoice_id), &timestamp);
}

/// Read the persisted `UnderReview` timestamp, if the dispute reached review.
pub(crate) fn get_under_review_timestamp(env: &Env, invoice_id: &BytesN<32>) -> Option<u64> {
    env.storage()
        .persistent()
        .get(&dispute_review_at_key(invoice_id))
}

/// Remove any stale persisted `UnderReview` timestamp for a dispute.
pub(crate) fn clear_under_review_timestamp(env: &Env, invoice_id: &BytesN<32>) {
    env.storage()
        .persistent()
        .remove(&dispute_review_at_key(invoice_id));
}

/// Builds the full ordered event list from a [`Dispute`] and its current
/// [`DisputeStatus`].  Returns at most 3 entries (one per lifecycle stage).
fn build_all_entries(
    env: &Env,
    invoice_id: &BytesN<32>,
    dispute: &Dispute,
    status: &DisputeStatus,
) -> Vec<DisputeTimelineEntry> {
    let mut entries: Vec<DisputeTimelineEntry> = Vec::new(env);

    // --- Event 0: Opened ---------------------------------------------------
    // Always present when a dispute exists.
    entries.push_back(DisputeTimelineEntry {
        sequence: 0,
        event: String::from_str(env, "Opened"),
        timestamp: dispute.created_at,
        actor: dispute.created_by.clone(),
        // Reason is safe to surface; evidence is not included here.
        summary: dispute.reason.clone(),
        resolution_outcome: DisputeResolution::None,
    });

    // --- Event 1: UnderReview ----------------------------------------------
    // Present when status is UnderReview or Resolved.
    let include_review = matches!(status, DisputeStatus::UnderReview | DisputeStatus::Resolved);
    if include_review {
        let review_timestamp = get_under_review_timestamp(env, invoice_id).unwrap_or_else(|| {
            // Older records may predate the dedicated review timestamp key.
            // Fall back to the best available lower bound without mutating state.
            if dispute.resolved_at > dispute.created_at {
                dispute.resolved_at.saturating_sub(1)
            } else {
                dispute.created_at
            }
        });

        entries.push_back(DisputeTimelineEntry {
            sequence: 1,
            event: String::from_str(env, "UnderReview"),
            timestamp: review_timestamp,
            // Admin identity is redacted to avoid leaking privileged info.
            actor: redacted_address(env),
            summary: String::from_str(env, ""),
            resolution_outcome: DisputeResolution::None,
        });
    }

    // --- Event 2: Resolved -------------------------------------------------
    // Present only when status is Resolved.
    if matches!(status, DisputeStatus::Resolved) {
        entries.push_back(DisputeTimelineEntry {
            sequence: 2,
            event: String::from_str(env, "Resolved"),
            timestamp: dispute.resolved_at,
            // resolved_by is the admin; surface it only in the Resolved entry
            // so callers can verify finality without exposing review identity.
            actor: dispute.resolved_by.clone(),
            summary: dispute.resolution.clone(),
            resolution_outcome: dispute.resolution_outcome.clone(),
        });
    }

    entries
}

/// Applies offset/limit pagination to a pre-built entry list.
fn paginate(
    env: &Env,
    all: &Vec<DisputeTimelineEntry>,
    offset: u32,
    limit: u32,
) -> (Vec<DisputeTimelineEntry>, bool) {
    let total = all.len();
    let capped_limit = limit.min(TIMELINE_MAX_PAGE_SIZE);
    let start = offset.min(total);
    let end = start.saturating_add(capped_limit).min(total);
    let has_more = end < total;

    let mut page: Vec<DisputeTimelineEntry> = Vec::new(env);
    let mut i = start;
    while i < end {
        if let Some(entry) = all.get(i) {
            page.push_back(entry);
        }
        i = i.saturating_add(1);
    }

    (page, has_more)
}

// ---------------------------------------------------------------------------
// Public endpoint
// ---------------------------------------------------------------------------

/// Returns a paginated, redacted dispute timeline for the given invoice.
///
/// # Arguments
/// * `env`        - Soroban environment.
/// * `invoice_id` - The invoice whose dispute timeline is requested.
/// * `offset`     - Zero-based starting position (0 = first event).
/// * `limit`      - Maximum entries to return; capped at
///                  [`TIMELINE_MAX_PAGE_SIZE`] (50).
///
/// # Returns
/// * `Ok(DisputeTimeline)` - Paginated timeline reflecting on-chain state.
/// * `Err(DisputeNotFound)` - Invoice exists but has no active dispute.
/// * `Err(InvoiceNotFound)` - Invoice does not exist.
///
/// # Redaction rules
/// * Evidence is **never** included.
/// * Resolution text is included only in the `Resolved` entry and only
///   when the dispute has actually been resolved.
/// * The admin actor for the `UnderReview` step is replaced with the
///   zero address.
///
/// # Pagination
/// * `offset` beyond the total event count returns an empty page with
///   `has_more = false`.
/// * `limit = 0` returns an empty page.
#[allow(dead_code)]
pub fn get_dispute_timeline(
    env: &Env,
    invoice_id: &BytesN<32>,
    offset: u32,
    limit: u32,
) -> Result<DisputeTimeline, QuickLendXError> {
    let invoice = InvoiceStorage::get(env, invoice_id).ok_or(QuickLendXError::InvoiceNotFound)?;

    if invoice.dispute_status == DisputeStatus::None {
        return Err(QuickLendXError::DisputeNotFound);
    }

    let all_entries = build_all_entries(env, invoice_id, &invoice.dispute, &invoice.dispute_status);
    let total = all_entries.len();
    let (entries, has_more) = paginate(env, &all_entries, offset, limit);

    Ok(DisputeTimeline {
        entries,
        total,
        has_more,
        current_status: invoice.dispute_status,
    })
}
