//! Dispute timeline endpoint — normalizes dispute lifecycle events into a
//! chronologically ordered, redacted sequence suitable for UI consumption.
//!
//! # Design
//!
//! On-chain dispute state is stored as a flat [`Dispute`] struct embedded in
//! each [`Invoice`].  This module reconstructs the implicit event sequence
//! (Opened → UnderReview → Resolved) from that struct, redacts fields that
//! must not leak to unprivileged callers (evidence, resolution text), and
//! returns a paginated [`DisputeTimeline`] value.
//!
//! # Security
//!
//! - Evidence is **always** redacted from timeline entries; it is only
//!   accessible via `get_dispute_details` to authorized parties.
//! - Resolution text is redacted until the dispute reaches `Resolved` status.
//! - No PII from invoice metadata is included.
//! - Pagination bounds use saturating arithmetic to prevent overflow.

use crate::errors::QuickLendXError;
use crate::invoice::{Dispute, DisputeStatus, InvoiceStorage};
use soroban_sdk::{contracttype, Address, BytesN, Env, String, Vec};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Maximum entries returned in a single timeline page.
pub const TIMELINE_MAX_PAGE_SIZE: u32 = 50;

/// Sentinel address used when a field is redacted (all-zero Stellar address).
const REDACTED_ADDRESS: &str = "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF";

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

/// Builds the full ordered event list from a [`Dispute`] and its current
/// [`DisputeStatus`].  Returns at most 3 entries (one per lifecycle stage).
fn build_all_entries(
    env: &Env,
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
    });

    // --- Event 1: UnderReview ----------------------------------------------
    // Present when status is UnderReview or Resolved.
    let include_review = matches!(status, DisputeStatus::UnderReview | DisputeStatus::Resolved);
    if include_review {
        entries.push_back(DisputeTimelineEntry {
            sequence: 1,
            event: String::from_str(env, "UnderReview"),
            // The review timestamp is not stored separately; we use the
            // resolution timestamp as a lower bound when resolved, otherwise
            // we use created_at as a conservative placeholder.  This reflects
            // on-chain truth: the exact review time is not persisted.
            timestamp: if dispute.resolved_at > 0 {
                dispute.resolved_at
            } else {
                dispute.created_at
            },
            // Admin identity is redacted to avoid leaking privileged info.
            actor: redacted_address(env),
            summary: String::from_str(env, ""),
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
    let total = all.len() as u32;
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
/// * `env`        — Soroban environment.
/// * `invoice_id` — The invoice whose dispute timeline is requested.
/// * `offset`     — Zero-based starting position (0 = first event).
/// * `limit`      — Maximum entries to return; capped at
///                  [`TIMELINE_MAX_PAGE_SIZE`] (50).
///
/// # Returns
/// * `Ok(DisputeTimeline)` — Paginated timeline reflecting on-chain state.
/// * `Err(DisputeNotFound)` — Invoice exists but has no active dispute.
/// * `Err(InvoiceNotFound)` — Invoice does not exist.
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
    let invoice =
        InvoiceStorage::get(env, invoice_id).ok_or(QuickLendXError::InvoiceNotFound)?;

    if invoice.dispute_status == DisputeStatus::None {
        return Err(QuickLendXError::DisputeNotFound);
    }

    let all_entries = build_all_entries(env, &invoice.dispute, &invoice.dispute_status);
    let total = all_entries.len() as u32;
    let (entries, has_more) = paginate(env, &all_entries, offset, limit);

    Ok(DisputeTimeline {
        entries,
        total,
        has_more,
        current_status: invoice.dispute_status,
    })
}
