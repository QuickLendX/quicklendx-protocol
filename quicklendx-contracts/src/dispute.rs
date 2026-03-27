//! # Dispute Resolution Module
//!
//! Implements a three-stage dispute state machine for invoice financing disputes.
//! Only invoice stakeholders (business owner or investor) may open a dispute, and
//! only the platform admin may advance or resolve it — preventing any unauthorized
//! write to the `Resolved` state.
//!
//! ## State Machine
//! ```
//! DisputeStatus::None (invoice default)
//!       │
//!       ▼  create_dispute  (business | investor)
//! DisputeStatus::Disputed
//!       │
//!       ▼  put_dispute_under_review  (admin only)
//! DisputeStatus::UnderReview
//!       │
//!       ▼  resolve_dispute  (admin only)
//! DisputeStatus::Resolved  ◄── terminal: no further transitions allowed
//! ```
//!
//! ## Role Constraints
//! | Operation               | Authorized callers          |
//! |-------------------------|-----------------------------|
//! | `create_dispute`        | Invoice business or investor |
//! | `put_dispute_under_review` | Platform admin only      |
//! | `resolve_dispute`       | Platform admin only         |
//!
//! ## Security Invariants
//! 1. Exactly one dispute per invoice (`DisputeAlreadyExists` guard).
//! 2. Forward-only transitions — skipping or reversing states is rejected.
//! 3. `resolution` field is write-once; a second call returns
//!    `DisputeNotUnderReview` (because status is `Resolved`, not `UnderReview`).
//! 4. Admin identity is read from instance storage — the caller-supplied `admin`
//!    address is cross-checked against the stored value *after* `require_auth`,
//!    so a valid signature for the wrong address still fails.

use crate::errors::QuickLendXError;
use crate::invoice::{Dispute, DisputeStatus, Invoice, InvoiceStatus, InvoiceStorage};
use crate::protocol_limits::{
    MAX_DISPUTE_EVIDENCE_LENGTH, MAX_DISPUTE_REASON_LENGTH, MAX_DISPUTE_RESOLUTION_LENGTH,
};
use soroban_sdk::{symbol_short, Address, BytesN, Env, String, Vec};

// ---------------------------------------------------------------------------
// Storage helpers
// ---------------------------------------------------------------------------

/// @notice Storage key for the list of all invoice IDs that have an active
///         or historical dispute.  Used by `get_invoices_with_disputes`.
const DISPUTE_INDEX_KEY: soroban_sdk::Symbol = symbol_short!("dsp_idx");

/// @notice Returns the dispute index (list of invoice IDs that have disputes).
/// @dev Stored in instance storage under `DISPUTE_INDEX_KEY`.
fn get_dispute_index(env: &Env) -> Vec<BytesN<32>> {
    env.storage()
        .instance()
        .get(&DISPUTE_INDEX_KEY)
        .unwrap_or_else(|| Vec::new(env))
}

/// @notice Appends `invoice_id` to the dispute index if not already present.
/// @dev Deduplication prevents the index from growing unboundedly on re-open
///      attempts (which are rejected before reaching this function, but
///      the guard is cheap insurance).
fn add_to_dispute_index(env: &Env, invoice_id: &BytesN<32>) {
    let mut index = get_dispute_index(env);
    for existing in index.iter() {
        if existing == *invoice_id {
            return;
        }
    }
    index.push_back(invoice_id.clone());
    env.storage().instance().set(&DISPUTE_INDEX_KEY, &index);
}

// ---------------------------------------------------------------------------
// Admin verification helper
// ---------------------------------------------------------------------------

/// @notice Verifies that `caller` is the stored platform admin.
/// @dev Reads the admin address from instance storage under key `ADMIN_KEY` (`"admin"`).
///      Returns `NotAdmin` if the key is absent and `Unauthorized` if the
///      stored address does not match `caller`.
/// @security Called *after* `caller.require_auth()` so both the cryptographic
///           signature and the role binding must be satisfied.
fn assert_is_admin(env: &Env, caller: &Address) -> Result<(), QuickLendXError> {
    use crate::admin::ADMIN_KEY;
    
    let stored_admin: Address = env
        .storage()
        .instance()
        .get(&ADMIN_KEY)
        .ok_or(QuickLendXError::NotAdmin)?;

    if *caller != stored_admin {
        return Err(QuickLendXError::Unauthorized);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Public entry points
// ---------------------------------------------------------------------------

/// @notice Opens a new dispute on an invoice.
///
/// @dev Only the business that owns the invoice, or the investor who funded it,
///      may call this function.  Exactly one dispute is allowed per invoice;
///      subsequent attempts return `DisputeAlreadyExists`.
///
/// @param env        The Soroban contract environment.
/// @param invoice_id The 32-byte invoice identifier.
/// @param creator    The address of the party raising the dispute.
/// @param reason     A human-readable reason string (1 – MAX_DISPUTE_REASON_LENGTH chars).
/// @param evidence   Supporting evidence string (1 – MAX_DISPUTE_EVIDENCE_LENGTH chars).
///
/// @return `Ok(())` on success.
///
/// @error `InvoiceNotFound`             Invoice does not exist in storage.
/// @error `DisputeAlreadyExists`        A dispute already exists for this invoice.
/// @error `DisputeNotAuthorized`        Creator is neither the business nor the investor.
/// @error `InvoiceNotAvailableForFunding` Invoice is not in a state that allows disputes.
/// @error `InvalidDisputeReason`        `reason` is empty or exceeds the length limit.
/// @error `InvalidDisputeEvidence`      `evidence` is empty or exceeds the length limit.
#[allow(dead_code)]
pub fn create_dispute(
    env: &Env,
    invoice_id: &BytesN<32>,
    creator: &Address,
    reason: &String,
    evidence: &String,
) -> Result<(), QuickLendXError> {
    // --- 1. Authentication: creator must sign the transaction ---
    creator.require_auth();

    // --- 2. Load the invoice ---
    let mut invoice: Invoice = InvoiceStorage::get_invoice(env, invoice_id)
        .ok_or(QuickLendXError::InvoiceNotFound)?;

    // --- 3. Guard: exactly one dispute per invoice ---
    if invoice.dispute_status != DisputeStatus::None {
        return Err(QuickLendXError::DisputeAlreadyExists);
    }

    // --- 4. Invoice must be in a state where disputes are meaningful ---
    //    Disputes are relevant once the invoice has moved past initial upload:
    //    Pending, Verified, Funded, or Paid all qualify.  Cancelled, Defaulted,
    //    and Refunded are terminal failure/resolution states where raising a new
    //    dispute adds no value.
    match invoice.status {
        InvoiceStatus::Pending
        | InvoiceStatus::Verified
        | InvoiceStatus::Funded
        | InvoiceStatus::Paid => {}
        _ => return Err(QuickLendXError::InvoiceNotAvailableForFunding),
    }

    // --- 5. Role check: only the business owner or the investor may dispute ---
    let is_business = *creator == invoice.business;
    let is_investor = invoice
        .investor
        .as_ref()
        .map_or(false, |inv| creator == inv);

    if !is_business && !is_investor {
        return Err(QuickLendXError::DisputeNotAuthorized);
    }

    // --- 6. Input validation ---
    if reason.len() == 0 || reason.len() > MAX_DISPUTE_REASON_LENGTH {
        return Err(QuickLendXError::InvalidDisputeReason);
    }
    if evidence.len() == 0 || evidence.len() > MAX_DISPUTE_EVIDENCE_LENGTH {
        return Err(QuickLendXError::InvalidDisputeEvidence);
    }

    // --- 7. Record the dispute on the invoice ---
    let now = env.ledger().timestamp();
    invoice.dispute_status = DisputeStatus::Disputed;
    invoice.dispute = Dispute {
        created_by: creator.clone(),
        created_at: now,
        reason: reason.clone(),
        evidence: evidence.clone(),
        resolution: String::from_str(env, ""),
        resolved_by: Address::from_str(
            env,
            "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
        ),
        resolved_at: 0,
    };

    // --- 8. Persist and index ---
    InvoiceStorage::update_invoice(env, &invoice);
    add_to_dispute_index(env, invoice_id);

    Ok(())
}

/// @notice Advances a dispute from `Disputed` to `UnderReview`.
///
/// @dev Only the platform admin may call this function.  The dispute must be
///      in the `Disputed` state; any other state (including `UnderReview` or
///      `Resolved`) is rejected.
///
/// @param env        The Soroban contract environment.
/// @param admin      The admin address (must match the stored admin).
/// @param invoice_id The 32-byte invoice identifier.
///
/// @return `Ok(())` on success.
///
/// @error `NotAdmin`          No admin has been configured.
/// @error `Unauthorized`      Caller is not the stored admin.
/// @error `DisputeNotFound`   No dispute exists on this invoice.
/// @error `InvalidStatus`     Dispute is not in `Disputed` state.
#[allow(dead_code)]
pub fn put_dispute_under_review(
    env: &Env,
    admin: &Address,
    invoice_id: &BytesN<32>,
) -> Result<(), QuickLendXError> {
    // --- 1. Authentication and role check ---
    admin.require_auth();
    assert_is_admin(env, admin)?;

    // --- 2. Load the invoice ---
    let mut invoice: Invoice = InvoiceStorage::get_invoice(env, invoice_id)
        .ok_or(QuickLendXError::InvoiceNotFound)?;

    // --- 3. Dispute must exist ---
    if invoice.dispute_status == DisputeStatus::None {
        return Err(QuickLendXError::DisputeNotFound);
    }

    // --- 4. State machine: only Disputed → UnderReview is allowed ---
    if invoice.dispute_status != DisputeStatus::Disputed {
        return Err(QuickLendXError::InvalidStatus);
    }

    // --- 5. Transition ---
    invoice.dispute_status = DisputeStatus::UnderReview;
    InvoiceStorage::update_invoice(env, &invoice);

    Ok(())
}

/// @notice Finalizes a dispute, recording the admin's resolution text.
///
/// @dev Only the platform admin may call this function.  The dispute must be
///      in the `UnderReview` state.  The `Resolved` state is terminal — no
///      further transitions are possible, and a second call returns
///      `DisputeNotUnderReview` because the status is no longer `UnderReview`.
///
/// @param env        The Soroban contract environment.
/// @param admin      The admin address (must match the stored admin).
/// @param invoice_id The 32-byte invoice identifier.
/// @param resolution Resolution text (1 – MAX_DISPUTE_RESOLUTION_LENGTH chars).
///
/// @return `Ok(())` on success.
///
/// @error `NotAdmin`              No admin has been configured.
/// @error `Unauthorized`          Caller is not the stored admin.
/// @error `DisputeNotFound`       No dispute exists on this invoice.
/// @error `DisputeNotUnderReview` Dispute is not in `UnderReview` state.
/// @error `InvalidDisputeReason`  `resolution` is empty or exceeds the length limit.
#[allow(dead_code)]
pub fn resolve_dispute(
    env: &Env,
    admin: &Address,
    invoice_id: &BytesN<32>,
    resolution: &String,
) -> Result<(), QuickLendXError> {
    // --- 1. Authentication and role check ---
    admin.require_auth();
    assert_is_admin(env, admin)?;

    // --- 2. Load the invoice ---
    let mut invoice: Invoice = InvoiceStorage::get_invoice(env, invoice_id)
        .ok_or(QuickLendXError::InvoiceNotFound)?;

    // --- 3. Dispute must exist ---
    if invoice.dispute_status == DisputeStatus::None {
        return Err(QuickLendXError::DisputeNotFound);
    }

    // --- 4. State machine: only UnderReview → Resolved is allowed.
    //    This also prevents re-resolution (Resolved → Resolved) because
    //    the status is no longer UnderReview. ---
    if invoice.dispute_status != DisputeStatus::UnderReview {
        return Err(QuickLendXError::DisputeNotUnderReview);
    }

    // --- 5. Validate resolution text ---
    if resolution.len() == 0 || resolution.len() > MAX_DISPUTE_RESOLUTION_LENGTH {
        return Err(QuickLendXError::InvalidDisputeReason);
    }

    // --- 6. Record resolution (write-once) ---
    let now = env.ledger().timestamp();
    invoice.dispute_status = DisputeStatus::Resolved;
    invoice.dispute.resolution = resolution.clone();
    invoice.dispute.resolved_by = admin.clone();
    invoice.dispute.resolved_at = now;

    InvoiceStorage::update_invoice(env, &invoice);

    Ok(())
}

// ---------------------------------------------------------------------------
// Query entry points
// ---------------------------------------------------------------------------

/// @notice Returns the dispute embedded in the invoice, if one exists.
///
/// @param env        The Soroban contract environment.
/// @param invoice_id The 32-byte invoice identifier.
///
/// @return `Some(Dispute)` when a dispute exists, `None` otherwise.
///
/// @dev Returns `None` (not an error) when `dispute_status == DisputeStatus::None`
///      so callers can distinguish "no dispute" from "invoice not found".
#[allow(dead_code)]
pub fn get_dispute_details(env: &Env, invoice_id: &BytesN<32>) -> Option<Dispute> {
    let invoice = InvoiceStorage::get_invoice(env, invoice_id)?;
    if invoice.dispute_status == DisputeStatus::None {
        return None;
    }
    Some(invoice.dispute)
}

/// @notice Returns all invoice IDs that have an active or historical dispute.
///
/// @dev Iterates the persisted dispute index; the list grows as disputes are
///      created and is never pruned (historical disputes remain visible).
///
/// @param env The Soroban contract environment.
/// @return A `Vec<BytesN<32>>` of invoice IDs.
#[allow(dead_code)]
pub fn get_invoices_with_disputes(env: &Env) -> Vec<BytesN<32>> {
    get_dispute_index(env)
}

/// @notice Returns all invoice IDs whose dispute status matches `status`.
///
/// @dev Iterates every invoice in the dispute index and filters by status.
///      The caller supplies the desired `DisputeStatus` variant.  Passing
///      `DisputeStatus::None` always returns an empty list because invoices
///      are only added to the index when a dispute is created.
///
/// @param env    The Soroban contract environment.
/// @param status The dispute status to filter by.
/// @return A `Vec<BytesN<32>>` of matching invoice IDs.
#[allow(dead_code)]
pub fn get_invoices_by_dispute_status(env: &Env, status: &DisputeStatus) -> Vec<BytesN<32>> {
    let index = get_dispute_index(env);
    let mut result = Vec::new(env);
    for invoice_id in index.iter() {
        if let Some(invoice) = InvoiceStorage::get_invoice(env, &invoice_id) {
            if invoice.dispute_status == *status {
                result.push_back(invoice_id);
            }
        }
    }
    result
}
