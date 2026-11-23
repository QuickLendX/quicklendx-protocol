use crate::admin::AdminStorage;
use crate::arbiter::ArbiterStorage;
use crate::dispute_timeline::{clear_under_review_timestamp, set_under_review_timestamp};
use crate::errors::QuickLendXError;
use crate::investment::InvestmentStorage;
use crate::resolution_policy::{EscrowEffect, InvoiceEffect, InvestmentEffect, ResolutionPolicy};
use crate::storage::{DataKey, InvoiceStorage};
use crate::types::{Dispute, DisputeResolution, DisputeStatus, InvoiceStatus, InvestmentStatus};
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

pub fn get_dispute_index(env: &Env) -> Vec<BytesN<32>> {
    let key = dispute_index_key();
    let res = env.storage().persistent().get(&key);
    if res.is_some() {
        crate::storage::extend_persistent_ttl(env, &key);
    }
    res.unwrap_or_else(|| Vec::new(env))
}

pub fn add_to_dispute_index(env: &Env, invoice_id: &BytesN<32>) {
    let mut ids = get_dispute_index(env);
    if !ids.iter().any(|id| id == *invoice_id) {
        ids.push_back(invoice_id.clone());
        let key = dispute_index_key();
        env.storage().persistent().set(&key, &ids);
        crate::storage::extend_persistent_ttl(env, &key);
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

#[cfg(all(test, feature = "legacy-tests"))]
mod evidence_identity_tests {
    use super::*;
    use soroban_sdk::testutils::Address as _;

    #[test]
    fn evidence_payload_is_reserved_once() {
        let env = Env::default();
        let invoice = BytesN::from_array(&env, &[1u8; 32]);
        let creator = Address::generate(&env);
        let evidence = String::from_str(&env, "provider-reference-1");
        let digest = reserve_evidence(&env, &invoice, &creator, &evidence).unwrap();
        let expected: BytesN<32> = env.crypto().sha256(&evidence.to_bytes()).into();
        assert_eq!(digest, expected);
        assert_eq!(
            reserve_evidence(&env, &invoice, &creator, &evidence),
            Err(QuickLendXError::InvalidDisputeEvidence)
        );
    }

    #[test]
    fn the_same_payload_cannot_cross_invoice_boundaries() {
        let env = Env::default();
        let first_invoice = BytesN::from_array(&env, &[2u8; 32]);
        let second_invoice = BytesN::from_array(&env, &[3u8; 32]);
        let creator = Address::generate(&env);
        let evidence = String::from_str(&env, "shared-attachment");
        reserve_evidence(&env, &first_invoice, &creator, &evidence).unwrap();
        let result = reserve_evidence(&env, &second_invoice, &creator, &evidence);
        assert_eq!(result, Err(QuickLendXError::InvalidDisputeEvidence));
    }

    #[test]
    fn different_payloads_have_independent_content_identities() {
        let env = Env::default();
        let invoice = BytesN::from_array(&env, &[4u8; 32]);
        let creator = Address::generate(&env);
        let first = String::from_str(&env, "attachment-a");
        let second = String::from_str(&env, "attachment-b");
        let first_digest = reserve_evidence(&env, &invoice, &creator, &first).unwrap();
        let second_digest = reserve_evidence(&env, &invoice, &creator, &second).unwrap();
        assert_ne!(first_digest, second_digest);
    }
}

fn zero_address(env: &Env) -> Address {
    Address::from_str(
        env,
        "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
    )
}

/// Reserve content-addressed evidence for one invoice.
///
/// The digest is the evidence identity: a retry with the same payload is
/// rejected, and the same payload cannot be attached to another invoice.
/// Reservation happens only after authorization, lifecycle, and size checks so
/// failed requests cannot consume an identifier.
pub(crate) fn reserve_evidence(
    env: &Env,
    invoice_id: &BytesN<32>,
    creator: &Address,
    evidence: &String,
) -> Result<BytesN<32>, QuickLendXError> {
    let digest: BytesN<32> = env.crypto().sha256(&evidence.to_bytes()).into();
    let key = DataKey::DisputeEvidence(digest.clone());
    if env.storage().persistent().has(&key) {
        return Err(QuickLendXError::InvalidDisputeEvidence);
    }
    env.storage().persistent().set(&key, invoice_id);
    crate::storage::extend_persistent_ttl(env, &key);
    env.events().publish(
        (symbol_short!("evidence"),),
        (invoice_id.clone(), creator.clone(), digest.clone()),
    );
    Ok(digest)
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
    reserve_evidence(env, invoice_id, creator, evidence)?;
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
        resolution_outcome: DisputeResolution::None,
    };

    InvoiceStorage::update_invoice(env, &invoice);
    add_to_dispute_index(env, invoice_id);

    // Lifecycle trigger: emits dispute-opened notifications to business and investor.
    let _ = crate::notifications::NotificationSystem::notify_dispute_opened(env, &invoice);

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
    // Per Issue #1840, the `require_dispute_arbiter` guard applies to
    // *resolve*, not the review transition. Any authenticated admin may move
    // a dispute into `UnderReview`; only registered arbiters may finalize it.
    // Keeping review on admin authority matches the intent expressed in the
    // issue title and avoids breaking every existing test that legitimately
    // drives a dispute through the review step before resolution.
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
    // Arbiter gate: even an admin cannot resolve a dispute unless they have
    // been explicitly registered as an arbiter. Splits dispute-adjudication
    // authority from protocol-configuration authority so that a single
    // compromised admin key cannot silently drain disputed escrow.
    ArbiterStorage::require_dispute_arbiter(env, admin)?;

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
    invoice.dispute.resolution_outcome = DisputeResolution::Split;
    InvoiceStorage::update_invoice(env, &invoice);

    // Lifecycle trigger: emits dispute-resolved notifications to business and investor.
    let _ = crate::notifications::NotificationSystem::notify_dispute_resolved(env, &invoice);

    // Enact the outcome. A free-text ruling is deliberately mapped to
    // `Split` (funds held, settlement/default blocked pending admin
    // follow-up) instead of a release: the platform must never infer
    // "release to business" from unstructured text — an arbiter who wants
    // a release must call `resolve_dispute_structured` with an explicit
    // outcome (issue #2963).
    apply_resolution_policy(env, invoice_id, DisputeResolution::Split)?;

    Ok(())
}

/// Finalize a dispute with a structured resolution outcome.
///
/// This is the preferred terminal step of the dispute lifecycle, providing
/// programmatic distinguishability between outcomes.
///
/// # Preconditions
/// - `admin` must be the registered platform admin.
/// - The invoice identified by `invoice_id` must exist.
/// - `invoice.dispute_status` must be exactly [`DisputeStatus::UnderReview`].
/// - `note` must be 1–`MAX_DISPUTE_RESOLUTION_LENGTH` (2 000) chars.
///
/// # Postconditions
/// - `invoice.dispute_status` is set to [`DisputeStatus::Resolved`].
/// - `invoice.dispute.resolution` stores `note`.
/// - `invoice.dispute.resolution_outcome` stores the structured outcome.
/// - `invoice.dispute.resolved_by` stores `admin`.
/// - `invoice.dispute.resolved_at` stores the current ledger timestamp.
///
/// # Authorization
/// Caller: platform admin only.
///
/// # Errors
/// | Error | Condition |
/// |---|---|
/// | [`QuickLendXError::Unauthorized`] / [`QuickLendXError::NotAdmin`] | Caller is not the admin |
/// | [`QuickLendXError::InvoiceNotFound`] | `invoice_id` does not exist |
/// | [`QuickLendXError::DisputeNotFound`] | No dispute exists |
/// | [`QuickLendXError::DisputeNotUnderReview`] | Status is not UnderReview |
/// | [`QuickLendXError::InvalidDisputeReason`] | `note` empty or > 2 000 chars |
pub fn resolve_dispute_structured(
    env: &Env,
    admin: &Address,
    invoice_id: &BytesN<32>,
    outcome: DisputeResolution,
    note: &String,
) -> Result<(), QuickLendXError> {
    AdminStorage::require_admin(env, admin)?;
    // See `resolve_dispute` — the structured variant shares the same arbiter
    // gate so both resolution paths are defended equally.
    ArbiterStorage::require_dispute_arbiter(env, admin)?;

    validate_dispute_resolution(note)?;

    let mut invoice =
        InvoiceStorage::get_invoice(env, invoice_id).ok_or(QuickLendXError::InvoiceNotFound)?;

    // Guard: only UnderReview disputes may be resolved.
    if invoice.dispute_status != DisputeStatus::UnderReview {
        return Err(QuickLendXError::DisputeNotUnderReview);
    }

    invoice.dispute_status = DisputeStatus::Resolved;
    invoice.dispute.resolution = note.clone();
    invoice.dispute.resolution_outcome = outcome;
    invoice.dispute.resolved_by = admin.clone();
    invoice.dispute.resolved_at = env.ledger().timestamp();
    InvoiceStorage::update_invoice(env, &invoice);

    // Lifecycle trigger: emits dispute-resolved notifications to business and investor.
    let _ = crate::notifications::NotificationSystem::notify_dispute_resolved(env, &invoice);

    // Enact the recorded outcome atomically: escrow, invoice and
    // investment transitions follow `ResolutionPolicy::for_outcome`
    // (issue #2963). Any error below aborts the whole invocation, so the
    // outcome can never be stored without its consequences applied.
    apply_resolution_policy(env, invoice_id, outcome)?;

    Ok(())
}

/// Enact [`ResolutionPolicy::for_outcome(outcome)`] on escrow, invoice,
/// and investment state.
///
/// Called by [`resolve_dispute_structured`] (and the unstructured
/// [`resolve_dispute`], which maps to `Split`) once the resolution fields
/// have been written. The policy table in [`crate::resolution_policy`]
/// drives every transition below, so the spec and the enforcement cannot
/// drift apart.
///
/// # Enforced effects (issue #2963)
///
/// | Outcome | Invoice | Escrow | Settlement | Investment |
/// |---|---|---|---|---|
/// | `FavorBusiness`, `Dismissed` | stays `Funded` | stays `Held` (released at settlement) | unblocked | stays `Active` |
/// | `FavorInvestor` | → `Refunded` | refunded to investor | permanently blocked | → `Refunded` |
/// | `Split`, `None` | stays `Funded` | stays `Held` | blocked until admin follow-up | stays `Active` |
///
/// Settlement/default blocking is enforced lazily by
/// [`crate::settlement::require_no_blocking_resolution`], which the
/// settlement and default paths consult; `Split`/`None` keep the escrow
/// `Held` but forbid any later fund movement until a follow-up admin
/// action.
///
/// # Authorization
/// Internal — no `require_auth` here. The caller (resolution path) has
/// already passed the admin **and** arbiter gates; the escrow refund uses
/// [`crate::payments::refund_escrow_internal`] for the same reason.
///
/// # Errors
/// | Error | Condition |
/// |---|---|
/// | [`QuickLendXError::InvoiceNotFound`] | `invoice_id` does not exist |
/// | [`QuickLendXError::StorageKeyNotFound`] | `FavorInvestor` but no escrow record exists |
/// | [`QuickLendXError::InvalidStatus`] | `FavorInvestor` but the escrow/investment is already terminal |
pub fn apply_resolution_policy(
    env: &Env,
    invoice_id: &BytesN<32>,
    outcome: DisputeResolution,
) -> Result<(), QuickLendXError> {
    let policy = ResolutionPolicy::for_outcome(outcome);

    // 1. Escrow disposition.
    match policy.escrow_effect {
        EscrowEffect::RefundToInvestor => {
            // Refund only when funds are actually held; a missing record
            // simply leaves the invoice/investment transitions below to
            // run, while an escrow in any other status is left untouched.
            if let Some(escrow) =
                crate::payments::EscrowStorage::get_escrow_by_invoice(env, invoice_id)
            {
                if escrow.status == crate::payments::EscrowStatus::Held {
                    crate::payments::refund_escrow_internal(env, invoice_id)?;
                }
            }
        }
        // `ReleaseToBusiness` (released later at settlement) and
        // `HeldPendingAdmin` require no immediate escrow mutation.
        EscrowEffect::ReleaseToBusiness | EscrowEffect::HeldPendingAdmin => {}
    }

    // 2. Invoice status transition.
    match policy.invoice_effect {
        InvoiceEffect::TransitionToRefunded => {
            let mut invoice = InvoiceStorage::get_invoice(env, invoice_id)
                .ok_or(QuickLendXError::InvoiceNotFound)?;
            if invoice.status != InvoiceStatus::Refunded {
                invoice.status = InvoiceStatus::Refunded;
                InvoiceStorage::update_invoice(env, &invoice);
            }
        }
        // `StaysFunded` and `PendingAdminAction` keep the current status.
        InvoiceEffect::StaysFunded
        | InvoiceEffect::PendingAdminAction
        | InvoiceEffect::RevertToPreDisputeStatus => {}
    }

    // 3. Investment status transition.
    match policy.investment_effect {
        InvestmentEffect::TransitionToRefunded => {
            if let Some(mut investment) =
                InvestmentStorage::get_investment_by_invoice(env, invoice_id)
            {
                if investment.status == InvestmentStatus::Active {
                    investment.status = InvestmentStatus::Refunded;
                    InvestmentStorage::update_investment(env, &investment);
                }
            }
        }
        // `StaysActive` and `PendingAdminAction` keep the investment as-is.
        InvestmentEffect::StaysActive | InvestmentEffect::PendingAdminAction => {}
    }

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

/// Return a paginated slice of dispute-indexed invoice IDs.
pub fn get_invoices_with_disputes_paged(
    env: &Env,
    offset: u32,
    limit: u32,
) -> Vec<BytesN<32>> {
    let list = get_dispute_index(env);
    let total_count = list.len();
    let (start, end) = crate::pagination::calculate_safe_bounds(offset, limit, total_count);
    let mut page = Vec::new(env);
    let mut idx = start;
    while idx < end {
        if let Some(id) = list.get(idx) {
            page.push_back(id);
        }
        idx = idx.saturating_add(1);
    }
    page
}

/// Return a paginated response with metadata for dispute-indexed invoices.
pub fn get_invoices_with_disputes_paginated(
    env: &Env,
    offset: u32,
    limit: u32,
) -> crate::types::PaginatedBytes32Vec {
    let items = get_invoices_with_disputes_paged(env, offset, limit);
    let total_count = get_dispute_index(env).len();
    let (_, has_more) = crate::pagination::pagination_metadata(offset, limit, total_count);
    crate::types::PaginatedBytes32Vec {
        items,
        total_count,
        has_more,
    }
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

/// Return a paginated slice of invoices matching a specific dispute status.
pub fn get_invoices_by_dispute_status_paged(
    env: &Env,
    status: &DisputeStatus,
    offset: u32,
    limit: u32,
) -> Vec<BytesN<32>> {
    let list = get_invoices_by_dispute_status(env, status);
    let total_count = list.len();
    let (start, end) = crate::pagination::calculate_safe_bounds(offset, limit, total_count);
    let mut page = Vec::new(env);
    let mut idx = start;
    while idx < end {
        if let Some(id) = list.get(idx) {
            page.push_back(id);
        }
        idx = idx.saturating_add(1);
    }
    page
}

/// @notice Filter dispute-indexed invoices by dispute status.
/// @param env The contract environment.
/// @param status Desired dispute status filter.
/// @return Invoice IDs whose current dispute status matches `status`.
pub(crate) fn indexed_invoices_by_status(env: &Env, status: &DisputeStatus) -> Vec<BytesN<32>> {
    get_invoices_by_dispute_status(env, status)
}

/// Guard: reject report/analytics-snapshot generation while any invoice has
/// an unresolved dispute.
///
/// # Threat model
/// `export_analytics_snapshot` (and the business/investor report generators)
/// feed off-chain indexers, dashboards, and downstream automated decisions
/// (pricing, risk scoring) that treat the returned numbers as settled fact.
/// A disputed invoice keeps its pre-dispute `InvoiceStatus`
/// (`Funded`/`Paid`) until the dispute resolves — `dispute_status` is a
/// side channel the report calculators never look at. Without this guard, a
/// snapshot taken while a dispute is `Disputed` or `UnderReview` silently
/// folds a contested invoice into `success_rate`, `default_rate`, and volume
/// totals as if it were final. If the dispute later resolves against the
/// business (refund to the investor), every indexer that already ingested
/// the earlier snapshot has a materially wrong number with no signal that it
/// was provisional — and a party who wants a favorable report published has
/// no way to time snapshot export around an open dispute once this check is
/// in place. Blocking generation while any dispute is active removes that
/// window instead of relying on downstream consumers to reconcile later.
///
/// # Cost
/// Bounded by the dispute index (`get_dispute_index`), which only ever
/// contains invoices that have entered the dispute lifecycle — the same
/// bound already relied on by `get_invoices_by_dispute_status`.
pub fn require_no_active_dispute_snapshot(env: &Env) -> Result<(), QuickLendXError> {
    for invoice_id in get_dispute_index(env).iter() {
        if let Some(invoice) = InvoiceStorage::get_invoice(env, &invoice_id) {
            if matches!(
                invoice.dispute_status,
                DisputeStatus::Disputed | DisputeStatus::UnderReview
            ) {
                return Err(QuickLendXError::ActiveDisputeExists);
            }
        }
    }
    Ok(())
}
// Invoice disputes are represented on [`crate::invoice::Invoice`] and handled by contract
// entry points in `lib.rs`. This module is reserved for future dispute-specific helpers.
