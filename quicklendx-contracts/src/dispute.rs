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
use crate::QuickLendXError;
use crate::invoice::{Invoice, InvoiceStatus};
use crate::protocol_limits::{
    MAX_DISPUTE_EVIDENCE_LENGTH, MAX_DISPUTE_REASON_LENGTH, MAX_DISPUTE_RESOLUTION_LENGTH,
};
use soroban_sdk::{contracttype, Address, BytesN, Env, String, Vec};

/// @notice Dispute status for standalone dispute storage.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DisputeStatus {
    Open,
    UnderReview,
    Resolved,
}

/// @notice Dispute record stored in persistent storage.
/// @dev Keyed by ("dispute", invoice_id). One dispute per invoice.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Dispute {
    pub invoice_id: BytesN<32>,
    pub creator: Address,
    /// @notice Dispute reason. Must be 1–1000 chars (non-empty, bounded).
    pub reason: String,
    /// @notice Supporting evidence. Must be 0–2000 chars (bounded).
    pub evidence: String,
    pub status: DisputeStatus,
    /// @notice Admin-provided resolution text. Set when dispute is resolved.
    pub resolution: Option<String>,
    pub created_at: u64,
    pub resolved_at: Option<u64>,
}

/// @notice Create a dispute on an invoice (standalone storage variant).
/// @dev Validates:
///   - No duplicate dispute for the same invoice
///   - Invoice exists and is in a disputable status (Pending/Verified/Funded/Paid)
///   - Creator is the business owner or investor on the invoice
///   - Reason is non-empty and <= MAX_DISPUTE_REASON_LENGTH (1000 chars)
///   - Evidence is <= MAX_DISPUTE_EVIDENCE_LENGTH (2000 chars)
/// @param env The contract environment.
/// @param invoice_id The invoice to dispute.
/// @param creator The address creating the dispute (must be authorized).
/// @param reason The dispute reason (1–1000 chars).
/// @param evidence Supporting evidence (0–2000 chars).
/// @return Ok(()) on success, Err with typed error on failure.
#[allow(dead_code)]
pub fn create_dispute(
    env: Env,
    invoice_id: BytesN<32>,
    creator: Address,
    reason: String,
    evidence: String,
) -> Result<(), QuickLendXError> {
    creator.require_auth();

    if env.storage().persistent().has(&("dispute", invoice_id.clone())) {
        return Err(QuickLendXError::DisputeAlreadyExists);
    }

    let invoice: Invoice = env
        .storage()
        .instance()
        .get(&invoice_id)
        .ok_or(QuickLendXError::InvoiceNotFound)?;

    match invoice.status {
        InvoiceStatus::Pending | InvoiceStatus::Verified | InvoiceStatus::Funded | InvoiceStatus::Paid => {}
        _ => return Err(QuickLendXError::InvoiceNotAvailableForFunding),
    }

    let is_authorized = creator == invoice.business ||
        invoice.investor.as_ref().map_or(false, |inv| creator == *inv);

    if !is_authorized {
        return Err(QuickLendXError::DisputeNotAuthorized);
    }

    // Validate reason: must be non-empty and within the protocol limit
    if reason.len() == 0 || reason.len() > MAX_DISPUTE_REASON_LENGTH {
        return Err(QuickLendXError::InvalidDisputeReason);
    }

    // Validate evidence: bounded to prevent storage abuse
    if evidence.len() > MAX_DISPUTE_EVIDENCE_LENGTH {
        return Err(QuickLendXError::InvalidDisputeEvidence);
    }

    let dispute = Dispute {
        invoice_id: invoice_id.clone(),
        creator: creator.clone(),
        reason,
        evidence,
        status: DisputeStatus::Open,
        resolution: None,
        created_at: env.ledger().timestamp(),
        resolved_at: None,
    };

    env.storage()
        .persistent()
        .set(&("dispute", invoice_id), &dispute);

    Ok(())
}

/// @notice Transition a dispute from Open to UnderReview (admin only).
/// @dev Requires admin authorization. Dispute must exist and be in Open status.
/// @param env The contract environment.
/// @param admin The admin address (must match stored admin).
/// @param invoice_id The invoice whose dispute to review.
/// @return Ok(()) on success.
#[allow(dead_code)]
pub fn put_dispute_under_review(
    env: &Env,
    admin: Address,
    invoice_id: BytesN<32>,
) -> Result<(), QuickLendXError> {
    admin.require_auth();

    let stored_admin: Address = env
        .storage()
        .instance()
        .get(&"admin")
        .ok_or(QuickLendXError::NotAdmin)?;

    if admin != stored_admin {
        return Err(QuickLendXError::Unauthorized);
    }

    let mut dispute: Dispute = env
        .storage()
        .persistent()
        .get(&("dispute", invoice_id.clone()))
        .ok_or(QuickLendXError::DisputeNotFound)?;

    if dispute.status != DisputeStatus::Open {
        return Err(QuickLendXError::InvalidStatus);
    }

    dispute.status = DisputeStatus::UnderReview;

    env.storage()
        .persistent()
        .set(&("dispute", invoice_id), &dispute);

    Ok(())
}

/// @notice Resolve a dispute with a resolution message (admin only).
/// @dev Validates:
///   - Admin authorization
///   - Dispute exists and is in UnderReview status
///   - Resolution is non-empty and <= MAX_DISPUTE_RESOLUTION_LENGTH (2000 chars)
/// @param env The contract environment.
/// @param admin The admin address.
/// @param invoice_id The invoice whose dispute to resolve.
/// @param resolution The resolution text (1–2000 chars).
/// @return Ok(()) on success.
#[allow(dead_code)]
pub fn resolve_dispute(
    env: &Env,
    admin: Address,
    invoice_id: BytesN<32>,
    resolution: String,
) -> Result<(), QuickLendXError> {
    admin.require_auth();

    let stored_admin: Address = env
        .storage()
        .instance()
        .get(&"admin")
        .ok_or(QuickLendXError::NotAdmin)?;

    if admin != stored_admin {
        return Err(QuickLendXError::Unauthorized);
    }

    let mut dispute: Dispute = env
        .storage()
        .persistent()
        .get(&("dispute", invoice_id.clone()))
        .ok_or(QuickLendXError::DisputeNotFound)?;

    if dispute.status != DisputeStatus::UnderReview {
        return Err(QuickLendXError::DisputeNotUnderReview);
    }

    if dispute.status == DisputeStatus::Resolved {
        return Err(QuickLendXError::DisputeAlreadyResolved);
    }

    // Validate resolution: must be non-empty and within protocol limits
    if resolution.len() == 0 || resolution.len() > MAX_DISPUTE_RESOLUTION_LENGTH {
        return Err(QuickLendXError::InvalidDisputeEvidence);
    }

    dispute.status = DisputeStatus::Resolved;
    dispute.resolution = Some(resolution);
    dispute.resolved_at = Some(env.ledger().timestamp());

    env.storage()
        .persistent()
        .set(&("dispute", invoice_id), &dispute);

    Ok(())
}

/// @notice Retrieve dispute details by invoice ID.
/// @param env The contract environment.
/// @param invoice_id The invoice to query.
/// @return The dispute record, or DisputeNotFound error.
#[allow(dead_code)]
pub fn get_dispute_details(env: &Env, invoice_id: BytesN<32>) -> Result<Dispute, QuickLendXError> {
    env.storage()
        .persistent()
        .get(&("dispute", invoice_id))
        .ok_or(QuickLendXError::DisputeNotFound)
}

/// @notice Query disputes by status with pagination.
/// @dev Scans persistent storage keys sequentially. Maximum 50 results per query.
/// @param env The contract environment.
/// @param status The dispute status to filter by.
/// @param start Starting index for pagination.
/// @param limit Maximum results (capped at 50).
/// @return Vec of matching disputes.
#[allow(dead_code)]
pub fn get_disputes_by_status(
    env: &Env,
    status: DisputeStatus,
    start: u64,
    limit: u32,
) -> Vec<Dispute> {
    let mut disputes = Vec::new(env);
    let max_limit = 50u32;
    let query_limit = if limit > max_limit { max_limit } else { limit };

    let end = start.saturating_add(query_limit as u64);
    for i in start..end {
        if let Some(dispute) = env
            .storage()
            .persistent()
            .get::<_, Dispute>(&("dispute", i))
        {
            if dispute.status == status {
                disputes.push_back(dispute);
            }
        }
    }

    disputes
}
