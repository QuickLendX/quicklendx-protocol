use crate::admin::AdminStorage;
use crate::errors::QuickLendXError;
use crate::storage::InvoiceStorage;
use crate::types::{Dispute, DisputeStatus, InvoiceStatus};
use crate::protocol_limits::*;
use soroban_sdk::{symbol_short, Address, BytesN, Env, String, Vec};

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

/// @notice Track an invoice ID in the dispute index.
/// @dev Idempotent helper used by contract entry points to keep query indexes consistent.
/// @param env The contract environment.
/// @param invoice_id The invoice to index as dispute-bearing.
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
/// @param reason The dispute reason (1-1000 chars).
/// @param evidence Supporting evidence (1-2000 chars).
/// @return Ok(()) on success, Err with typed error on failure.
#[allow(dead_code)]
pub fn create_dispute(
    env: &Env,
    invoice_id: &BytesN<32>,
    creator: &Address,
    reason: &String,
    evidence: &String,
) -> Result<(), QuickLendXError> {
    creator.require_auth();

    let mut invoice = InvoiceStorage::get_invoice(env, invoice_id)
        .ok_or(QuickLendXError::InvoiceNotFound)?;

    validate_dispute_reason(reason)?;
    validate_dispute_evidence(evidence)?;
    validate_dispute_eligibility(&invoice, creator)?;

    // Set dispute fields
    invoice.dispute_status = DisputeStatus::Disputed;
    invoice.dispute = Dispute {
        created_by: creator.clone(),
        created_at: env.ledger().timestamp(),
        reason: reason.clone(),
        evidence: evidence.clone(),
        resolution: String::from_str(env, ""),
        resolved_by: creator.clone(), // Placeholder
        resolved_at: 0,
    };

    InvoiceStorage::update_invoice(env, &invoice);
    InvoiceStorage::add_to_dispute_index(env, invoice_id);

    Ok(())
}

pub fn put_dispute_under_review(
    env: &Env,
    admin: &Address,
    invoice_id: &BytesN<32>,
) -> Result<(), QuickLendXError> {
    AdminStorage::require_admin(env, admin)?;
    let mut invoice = InvoiceStorage::get_invoice(env, invoice_id)
        .ok_or(QuickLendXError::InvoiceNotFound)?;

    if invoice.dispute_status != DisputeStatus::Disputed {
        return Err(QuickLendXError::DisputeNotFound);
    }

    invoice.dispute_status = DisputeStatus::UnderReview;
    InvoiceStorage::update_invoice(env, &invoice);
    Ok(())
}

pub fn resolve_dispute(
    env: &Env,
    admin: &Address,
    invoice_id: &BytesN<32>,
    resolution: &String,
) -> Result<(), QuickLendXError> {
    AdminStorage::require_admin(env, admin)?;

    validate_dispute_resolution(resolution)?;

    let mut invoice = InvoiceStorage::get_invoice(env, invoice_id)
        .ok_or(QuickLendXError::InvoiceNotFound)?;

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
    InvoiceStorage::get_dispute_index(env)
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
    for invoice_id in InvoiceStorage::get_dispute_index(env).iter() {
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
