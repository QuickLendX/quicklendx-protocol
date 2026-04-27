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

    let mut invoice =
        InvoiceStorage::get_invoice(env, invoice_id).ok_or(QuickLendXError::InvoiceNotFound)?;

    if invoice.dispute_status != DisputeStatus::None {
        return Err(QuickLendXError::DisputeAlreadyExists);
    }

    match invoice.status {
        InvoiceStatus::Pending
        | InvoiceStatus::Verified
        | InvoiceStatus::Funded
        | InvoiceStatus::Paid => {}
        _ => return Err(QuickLendXError::InvalidStatus),
    }

    let is_business = *creator == invoice.business;
    let is_investor = invoice
        .investor
        .as_ref()
        .map_or(false, |investor| *creator == *investor);
    if !is_business && !is_investor {
        return Err(QuickLendXError::DisputeNotAuthorized);
    }

    if reason.len() == 0 || reason.len() > MAX_DISPUTE_REASON_LENGTH {
        return Err(QuickLendXError::InvalidDisputeReason);
    }
    if evidence.len() == 0 || evidence.len() > MAX_DISPUTE_EVIDENCE_LENGTH {
        return Err(QuickLendXError::InvalidDisputeEvidence);
    }

    invoice.dispute_status = DisputeStatus::Disputed;
    invoice.dispute = Dispute {
        created_by: creator.clone(),
        created_at: env.ledger().timestamp(),
        reason: reason.clone(),
        evidence: evidence.clone(),
        resolution: String::from_str(env, ""),
        resolved_by: zero_address(env),
        resolved_at: 0,
    };

    InvoiceStorage::update_invoice(env, &invoice);
    add_to_dispute_index(env, invoice_id);
    Ok(())
}

#[allow(dead_code)]
pub fn put_dispute_under_review(
    env: &Env,
    admin: &Address,
    invoice_id: &BytesN<32>,
) -> Result<(), QuickLendXError> {
    AdminStorage::require_admin(env, admin)?;
    let mut invoice =
        InvoiceStorage::get_invoice(env, invoice_id).ok_or(QuickLendXError::InvoiceNotFound)?;

    if invoice.dispute_status == DisputeStatus::None {
        return Err(QuickLendXError::DisputeNotFound);
    }
    if invoice.dispute_status != DisputeStatus::Disputed {
        return Err(QuickLendXError::InvalidStatus);
    }

    invoice.dispute_status = DisputeStatus::UnderReview;
    InvoiceStorage::update_invoice(env, &invoice);
    Ok(())
}

#[allow(dead_code)]
pub fn resolve_dispute(
    env: &Env,
    admin: &Address,
    invoice_id: &BytesN<32>,
    resolution: &String,
) -> Result<(), QuickLendXError> {
    AdminStorage::require_admin(env, admin)?;
    let mut invoice =
        InvoiceStorage::get_invoice(env, invoice_id).ok_or(QuickLendXError::InvoiceNotFound)?;

    if invoice.dispute_status == DisputeStatus::None {
        return Err(QuickLendXError::DisputeNotFound);
    }
    if invoice.dispute_status != DisputeStatus::UnderReview {
        return Err(QuickLendXError::DisputeNotUnderReview);
    }
    if resolution.len() == 0 || resolution.len() > MAX_DISPUTE_RESOLUTION_LENGTH {
        return Err(QuickLendXError::InvalidDisputeReason);
    }

    invoice.dispute_status = DisputeStatus::Resolved;
    invoice.dispute.resolution = resolution.clone();
    invoice.dispute.resolved_by = admin.clone();
    invoice.dispute.resolved_at = env.ledger().timestamp();
    InvoiceStorage::update_invoice(env, &invoice);
    Ok(())
}

#[allow(dead_code)]
pub fn get_dispute_details(env: &Env, invoice_id: &BytesN<32>) -> Option<Dispute> {
    let invoice = InvoiceStorage::get_invoice(env, invoice_id)?;
    if invoice.dispute_status == DisputeStatus::None {
        None
    } else {
        Some(invoice.dispute)
    }
}

#[allow(dead_code)]
pub fn get_invoices_with_disputes(env: &Env) -> Vec<BytesN<32>> {
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
// Invoice disputes are represented on [`crate::invoice::Invoice`] and handled by contract
// entry points in `lib.rs`. This module is reserved for future dispute-specific helpers.
