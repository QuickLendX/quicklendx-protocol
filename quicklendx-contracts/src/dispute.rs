/// @title Dispute Module (Standalone Storage)
/// @notice Provides dispute lifecycle management using separate persistent storage.
/// @dev This module stores disputes independently from invoices in persistent storage
///      keyed by ("dispute", invoice_id). The primary contract interface uses the
///      invoice-embedded dispute model (see lib.rs). This module is retained for
///      reference and potential future migration to standalone dispute storage.
///
/// ## Security: Input Validation for Storage Growth Prevention
///
/// All string fields (reason, evidence, resolution) are bounded by protocol limits
/// defined in `protocol_limits.rs`:
///   - `MAX_DISPUTE_REASON_LENGTH`     = 1000 chars
///   - `MAX_DISPUTE_EVIDENCE_LENGTH`   = 2000 chars
///   - `MAX_DISPUTE_RESOLUTION_LENGTH` = 2000 chars
///
/// These limits prevent adversarial callers from inflating on-chain storage costs
/// by submitting oversized payloads. Empty reason/resolution strings are also
/// rejected to ensure disputes carry meaningful content.
use crate::admin::AdminStorage;
use crate::invoice::{Dispute, DisputeStatus, Invoice, InvoiceStatus, InvoiceStorage};
use crate::protocol_limits::{
    MAX_DISPUTE_EVIDENCE_LENGTH, MAX_DISPUTE_REASON_LENGTH, MAX_DISPUTE_RESOLUTION_LENGTH,
};
use crate::QuickLendXError;
use soroban_sdk::{symbol_short, Address, BytesN, Env, String, Vec};

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

fn assert_is_admin(env: &Env, address: &Address) -> Result<(), QuickLendXError> {
    AdminStorage::require_admin(env, address)
}

fn add_to_dispute_index(env: &Env, invoice_id: &BytesN<32>) {
    let key = symbol_short!("disp_idx");
    let mut index: Vec<BytesN<32>> = env
        .storage()
        .persistent()
        .get(&key)
        .unwrap_or_else(|| Vec::new(env));
    for existing in index.iter() {
        if existing == *invoice_id {
            return;
        }
    }
    index.push_back(invoice_id.clone());
    env.storage().persistent().set(&key, &index);
}

fn get_dispute_index(env: &Env) -> Vec<BytesN<32>> {
    let key = symbol_short!("disp_idx");
    env.storage()
        .persistent()
        .get(&key)
        .unwrap_or_else(|| Vec::new(env))
}

// ---------------------------------------------------------------------------
// Public entry points
// ---------------------------------------------------------------------------

/// @notice Create a dispute on an invoice (standalone storage variant).
/// @dev Validates:
///   - No duplicate dispute for the same invoice
///   - Invoice exists and is in a disputable status (Pending/Verified/Funded/Paid)
///   - Creator is the business owner or investor on the invoice
///   - Reason is non-empty and <= MAX_DISPUTE_REASON_LENGTH (1000 chars)
///   - Evidence is non-empty and <= MAX_DISPUTE_EVIDENCE_LENGTH (2000 chars)
/// @param env The contract environment.
/// @param invoice_id The invoice to dispute.
/// @param creator The address creating the dispute (must be authorized).
/// @param reason The dispute reason (1–1000 chars).
/// @param evidence Supporting evidence (1–2000 chars).
/// @return Ok(()) on success, Err with typed error on failure.
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

    if env
        .storage()
        .persistent()
        .has(&("dispute", invoice_id.clone()))
    {
        return Err(QuickLendXError::DisputeAlreadyExists);
    }

    // --- 3. Load the invoice ---
    let mut invoice: Invoice = InvoiceStorage::get_invoice(env, invoice_id)
        .ok_or(QuickLendXError::InvoiceNotFound)?;

    // --- 4. Invoice must be in a state where disputes are meaningful ---
    match invoice.status {
        InvoiceStatus::Pending
        | InvoiceStatus::Verified
        | InvoiceStatus::Funded
        | InvoiceStatus::Paid => {}
        _ => return Err(QuickLendXError::InvoiceNotAvailableForFunding),
    }

    let is_authorized = creator == &invoice.business
        || invoice
            .investor
            .as_ref()
            .map_or(false, |inv| creator == inv);

    if !is_authorized {
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
        resolution: soroban_sdk::String::from_str(env, ""),
        resolved_by: env.current_contract_address(),
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
