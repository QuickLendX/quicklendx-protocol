use crate::admin::AdminStorage;
use crate::dispute_timeline::{
    clear_under_review_timestamp, set_under_review_timestamp,
};
use crate::errors::QuickLendXError;
use crate::storage::InvoiceStorage;
use crate::types::{Dispute, DisputeStatus};
use crate::verification::{
    validate_dispute_eligibility, validate_dispute_evidence, validate_dispute_reason,
    validate_dispute_resolution,
};
use soroban_sdk::{symbol_short, Address, BytesN, Env, String, Vec};

/// # Settlement-Dispute Interaction Safety
///
/// ## Invariant: Settlement Mutual Exclusion
/// **Settlement finalization MUST be blocked while `dispute_status != DisputeStatus::None`.**
///
/// ### Implementation Strategy
/// The dispute module manages `dispute_status` transitions:
/// - `None → Disputed` (via `create_dispute`)
/// - `Disputed → UnderReview` (via `put_dispute_under_review`)
/// - `UnderReview → Resolved` (via `resolve_dispute`)
///
/// The settlement module (`settlement.rs`) enforces blocking through invoice status checks.
/// When a dispute is active, the invoice:
/// 1. **Option A**: Remains `Funded` but has `dispute_status != None`
///    - Requires explicit check: `if dispute_status != None { reject settlement }`
/// 2. **Option B**: Transitions to dispute-specific status (e.g., `Disputed` enum variant)
///    - Automatically blocks settlement via `ensure_payable_status()`
///
/// **Current implementation uses Option A**: Invoice stays `Funded`, so settlement logic
/// must explicitly check `dispute_status` before finalizing.
///
/// ### Resolution Outcomes & Settlement
///
/// #### Resolution in Favor of Investor
/// - Admin should transition invoice to `Cancelled` or `Refunded` status
/// - This unlocks `refund_escrow()` and permanently blocks settlement
/// - **Guarantee**: Investor can recover funds; business cannot trigger settlement
///
/// #### Resolution in Favor of Business
/// - Invoice returns to `Funded` status (or stays `Funded` with `dispute_status = Resolved`)
/// - Business completes remaining payments
/// - Settlement logic checks `dispute_status == Resolved` → allows finalization
/// - **Guarantee**: Normal settlement flow resumes after resolution
///
/// #### Neutral Resolution
/// - Platform policy determines outcome (settlement, partial refund, mediation)
/// - **Guarantee**: No permanent fund freeze; deterministic path provided
///
/// ### Escrow Interaction
/// Disputes do NOT directly modify escrow state. Instead:
/// - Disputes influence invoice status transitions
/// - Invoice status gates escrow operations:
///   - `release_escrow`: Requires `invoice.status == Paid`
///   - `refund_escrow`: Requires `invoice.status == Cancelled/Refunded`
/// - Dispute resolution determines which status the invoice transitions to,
///   thereby enabling the appropriate escrow operation
///
/// ### Testing
/// See `src/test_settlement_dispute_interaction.rs` for comprehensive integration
/// tests covering all dispute resolution scenarios and settlement blocking behavior.
///
/// ### Documentation
/// See `docs/settlement-dispute-interaction.md` for complete state machine diagrams
/// and resolution outcome specifications.
fn dispute_index_key() -> soroban_sdk::Symbol {
    symbol_short!("dispute")
}

fn get_dispute_index(env: &Env) -> Vec<BytesN<32>> {
    env.storage()
        .instance()
        .get(&dispute_index_key())
        .unwrap_or_else(|| Vec::new(env))
}

fn add_to_dispute_index(env: &Env, invoice_id: &BytesN<32>) {
    let mut ids = get_dispute_index(env);
    if !ids.iter().any(|id| id == *invoice_id) {
        ids.push_back(invoice_id.clone());
        env.storage().instance().set(&dispute_index_key(), &ids);
    }
}

/// Track an invoice ID in the dispute index.
///
/// Idempotent helper used by contract entry points to keep query indexes
/// consistent.  Safe to call multiple times for the same invoice — duplicate
/// entries are suppressed.
///
/// # Parameters
/// - `env`        — The contract environment.
/// - `invoice_id` — The invoice to index as dispute-bearing.
pub(crate) fn track_dispute_invoice(env: &Env, invoice_id: &BytesN<32>) {
    add_to_dispute_index(env, invoice_id);
}

fn zero_address(env: &Env) -> Address {
    Address::from_str(
        env,
        "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
    )
}
fn assert_is_admin(_env: &Env, _admin: &Address) -> Result<(), QuickLendXError> {
    Ok(())
}

/// Open a new dispute on an invoice.
///
/// # Preconditions
/// - `creator.require_auth()` must pass (on-chain authorization).
/// - The invoice identified by `invoice_id` must exist.
/// - The invoice must be in one of the disputable statuses:
///   `Pending`, `Verified`, `Funded`, or `Paid`.
/// - `creator` must be either the business owner **or** the investor recorded
///   on the invoice.  Any other caller is rejected with
///   [`QuickLendXError::DisputeNotAuthorized`].
/// - No active dispute may already exist for this invoice
///   (`dispute_status == DisputeStatus::None`).  A second attempt returns
///   [`QuickLendXError::DisputeAlreadyExists`].
/// - `reason`   must be 1–`MAX_DISPUTE_REASON_LENGTH` (1 000) characters.
/// - `evidence` must be 1–`MAX_DISPUTE_EVIDENCE_LENGTH` (2 000) characters.
///
/// # Postconditions
/// - `invoice.dispute_status` is set to [`DisputeStatus::Disputed`].
/// - The `Dispute` struct fields `created_by`, `created_at`, `reason`, and
///   `evidence` are populated; `resolution`, `resolved_by`, and `resolved_at`
///   are zero-valued placeholders.
/// - The invoice ID is appended to the global dispute index exactly once.
///
/// # Authorization
/// Caller: business owner **or** investor on the invoice.
///
/// # Errors
/// | Error | Condition |
/// |---|---|
/// | [`QuickLendXError::InvoiceNotFound`] | `invoice_id` does not exist |
/// | [`QuickLendXError::InvoiceNotAvailableForFunding`] | Invoice in a non-disputable status |
/// | [`QuickLendXError::DisputeNotAuthorized`] | Caller is not business or investor |
/// | [`QuickLendXError::DisputeAlreadyExists`] | Dispute already open on this invoice |
/// | [`QuickLendXError::InvalidDisputeReason`] | `reason` empty or > 1 000 chars |
/// | [`QuickLendXError::InvalidDisputeEvidence`] | `evidence` empty or > 2 000 chars |
#[allow(dead_code)]
pub fn create_dispute(
    env: &Env,
    invoice_id: &BytesN<32>,
    creator: &Address,
    reason: &String,
    evidence: &String,
) -> Result<(), QuickLendXError> {
    creator.require_auth();

    let mut invoice =
        InvoiceStorage::get_invoice(env, invoice_id).ok_or(QuickLendXError::InvoiceNotFound)?;

    validate_dispute_reason(reason)?;
    validate_dispute_evidence(evidence)?;
    validate_dispute_eligibility(&invoice, creator)?;
    clear_under_review_timestamp(env, invoice_id);

    // Set dispute fields
    invoice.dispute_status = DisputeStatus::Disputed;
    invoice.dispute = Dispute {
        created_by: creator.clone(),
        created_at: env.ledger().timestamp(),
        reason: reason.clone(),
        evidence: evidence.clone(),
        resolution: String::from_str(env, ""),
        resolved_by: creator.clone(), // Placeholder — overwritten on resolution
        resolved_at: 0,
    };

    InvoiceStorage::update_invoice(env, &invoice);
    add_to_dispute_index(env, invoice_id);

    Ok(())
}

/// Advance a dispute from `Disputed` to `UnderReview`.
///
/// Signals that a platform administrator has acknowledged the dispute and is
/// actively investigating it.  This is the mandatory second step in the
/// dispute lifecycle; resolution is only permitted after this transition.
///
/// # Preconditions
/// - `admin` must be the registered platform admin
///   ([`AdminStorage::require_admin`] passes).
/// - The invoice identified by `invoice_id` must exist.
/// - `invoice.dispute_status` must be exactly [`DisputeStatus::Disputed`].
///   Any other status (including `UnderReview` or `Resolved`) is rejected to
///   enforce a strictly forward-only, acyclic state machine.
///
/// # Postconditions
/// - `invoice.dispute_status` is set to [`DisputeStatus::UnderReview`].
/// - The invoice record is persisted in storage.
///
/// # Authorization
/// Caller: platform admin only.
///
/// # Errors
/// | Error | Condition |
/// |---|---|
/// | [`QuickLendXError::Unauthorized`] / [`QuickLendXError::NotAdmin`] | Caller is not the admin |
/// | [`QuickLendXError::InvoiceNotFound`] | `invoice_id` does not exist |
/// | [`QuickLendXError::DisputeNotFound`] | Invoice has no active dispute (`dispute_status != Disputed`) |
/// | [`QuickLendXError::InvalidStatus`] | Dispute is already `UnderReview` or `Resolved` |
pub fn put_dispute_under_review(
    env: &Env,
    admin: &Address,
    invoice_id: &BytesN<32>,
) -> Result<(), QuickLendXError> {
    AdminStorage::require_admin(env, admin)?;
    let mut invoice =
        InvoiceStorage::get_invoice(env, invoice_id).ok_or(QuickLendXError::InvoiceNotFound)?;

    match invoice.dispute_status {
        DisputeStatus::None => return Err(QuickLendXError::DisputeNotFound),
        DisputeStatus::Disputed => {}
        DisputeStatus::UnderReview | DisputeStatus::Resolved => {
            return Err(QuickLendXError::InvalidStatus);
        }
    }

    invoice.dispute_status = DisputeStatus::UnderReview;
    InvoiceStorage::update_invoice(env, &invoice);
    set_under_review_timestamp(env, invoice_id, env.ledger().timestamp());
    Ok(())
}

/// Finalize a dispute by recording an admin-authored resolution.
///
/// This is the terminal step of the dispute lifecycle.  Once a dispute is
/// resolved its status becomes [`DisputeStatus::Resolved`] and **all further
/// mutation is permanently blocked** — neither re-resolution nor re-review is
/// possible.
///
/// # Preconditions
/// - `admin` must be the registered platform admin.
/// - The invoice identified by `invoice_id` must exist.
/// - `invoice.dispute_status` must be exactly [`DisputeStatus::UnderReview`].
///   Attempting to resolve a `Disputed` invoice skips the mandatory review
///   step and is rejected.  Attempting to resolve a `Resolved` invoice is
///   also rejected (terminal-state guard).
/// - `resolution` must be 1–`MAX_DISPUTE_RESOLUTION_LENGTH` (2 000) chars.
///
/// # Postconditions
/// - `invoice.dispute_status` is set to [`DisputeStatus::Resolved`].
/// - `invoice.dispute.resolution` stores `resolution`.
/// - `invoice.dispute.resolved_by` stores `admin`.
/// - `invoice.dispute.resolved_at` stores the current ledger timestamp.
/// - All three fields are written atomically; none can be partially set.
///
/// # Authorization
/// Caller: platform admin only.
///
/// # Security
/// The `Resolved` status is a **write-once terminal state**.  The state-machine
/// guard at `invoice.dispute_status != DisputeStatus::UnderReview` prevents:
/// - Double-resolution (overwriting resolution text).
/// - Resolving without prior review (skipping governance step).
/// - Resolving a dispute that was never opened (`None` status).
///
/// # Errors
/// | Error | Condition |
/// |---|---|
/// | [`QuickLendXError::Unauthorized`] / [`QuickLendXError::NotAdmin`] | Caller is not the admin |
/// | [`QuickLendXError::InvoiceNotFound`] | `invoice_id` does not exist |
/// | [`QuickLendXError::DisputeNotFound`] | No dispute exists (`DisputeStatus::None`) |
/// | [`QuickLendXError::DisputeNotUnderReview`] | Status is `Disputed` or `Resolved` |
/// | [`QuickLendXError::InvalidDisputeReason`] | `resolution` empty or > 2 000 chars |
pub fn resolve_dispute(
    env: &Env,
    admin: &Address,
    invoice_id: &BytesN<32>,
    resolution: &String,
) -> Result<(), QuickLendXError> {
    AdminStorage::require_admin(env, admin)?;

    validate_dispute_resolution(resolution)?;

    let mut invoice =
        InvoiceStorage::get_invoice(env, invoice_id).ok_or(QuickLendXError::InvoiceNotFound)?;

    // Guard: only UnderReview disputes may be resolved.
    // This single check simultaneously prevents:
    //   • resolving a Disputed invoice (review step not taken)
    //   • double-resolving a Resolved invoice (terminal state guard)
    //   • resolving a None invoice (no dispute exists)
    if invoice.dispute_status != DisputeStatus::UnderReview {
        return Err(QuickLendXError::DisputeNotUnderReview);
    }

    invoice.dispute_status = DisputeStatus::Resolved;
    invoice.dispute.resolution = resolution.clone();
    invoice.dispute.resolved_by = admin.clone();
    invoice.dispute.resolved_at = env.ledger().timestamp();
    InvoiceStorage::update_invoice(env, &invoice);
    Ok(())
}

pub fn get_dispute_details(env: &Env, invoice_id: &BytesN<32>) -> Option<Dispute> {
    let invoice = InvoiceStorage::get_invoice(env, invoice_id)?;
    if invoice.dispute_status == DisputeStatus::None {
        None
    } else {
        Some(invoice.dispute)
    }
}

pub fn get_invoices_with_disputes(env: &Env) -> Vec<BytesN<32>> {
    get_dispute_index(env)
}

/// @notice Read the dispute index for query endpoints.
/// @param env The contract environment.
/// @return Invoice IDs that have entered the dispute lifecycle.
pub(crate) fn indexed_dispute_invoices(env: &Env) -> Vec<BytesN<32>> {
    get_dispute_index(env)
}

#[allow(dead_code)]
pub fn get_invoices_by_dispute_status(env: &Env, status: &DisputeStatus) -> Vec<BytesN<32>> {
    let mut result = Vec::new(env);
    for invoice_id in get_dispute_index(env).iter() {
        if let Some(invoice) = InvoiceStorage::get_invoice(env, &invoice_id) {
            if invoice.dispute_status == *status {
                result.push_back(invoice_id);
            }
        }
    }
    result
}

/// @notice Filter dispute-indexed invoices by dispute status.
/// @param env The contract environment.
/// @param status Desired dispute status filter.
/// @return Invoice IDs whose current dispute status matches `status`.
pub(crate) fn indexed_invoices_by_status(env: &Env, status: &DisputeStatus) -> Vec<BytesN<32>> {
    get_invoices_by_dispute_status(env, status)
}
// Invoice disputes are represented on [`crate::invoice::Invoice`] and handled by contract
// entry points in `lib.rs`. This module is reserved for future dispute-specific helpers.
