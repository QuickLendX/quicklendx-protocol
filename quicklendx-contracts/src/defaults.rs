use crate::errors::QuickLendXError;
use crate::events::{emit_invoice_defaulted, emit_dispute_created, emit_dispute_under_review, emit_dispute_resolved};
use crate::investment::{Investment, InvestmentStatus, InvestmentStorage};
use crate::invoice::{Invoice, InvoiceStatus, InvoiceStorage, DisputeStatus, Dispute};
use soroban_sdk::{Address, BytesN, Env, String, Vec};

pub fn handle_default(env: &Env, invoice_id: &BytesN<32>) -> Result<(), QuickLendXError> {
    let mut invoice =
        InvoiceStorage::get_invoice(env, invoice_id).ok_or(QuickLendXError::InvoiceNotFound)?;
    if invoice.status != InvoiceStatus::Funded {
        return Err(QuickLendXError::InvalidStatus);
    }
    invoice.mark_as_defaulted();
    InvoiceStorage::update_invoice(env, &invoice);
    let mut investment = InvestmentStorage::get_investment(env, invoice_id)
        .ok_or(QuickLendXError::StorageKeyNotFound)?;
    investment.status = InvestmentStatus::Withdrawn;
    InvestmentStorage::update_investment(env, &investment);
    emit_invoice_defaulted(env, &invoice);
    Ok(())
}

/// Create a dispute for an invoice
pub fn create_dispute(
    env: &Env,
    invoice_id: &BytesN<32>,
    creator: &Address,
    reason: String,
    evidence: String,
) -> Result<(), QuickLendXError> {
    creator.require_auth();
    
    let mut invoice = InvoiceStorage::get_invoice(env, invoice_id)
        .ok_or(QuickLendXError::InvoiceNotFound)?;
    
    // Check if dispute already exists
    if invoice.dispute_status != DisputeStatus::None {
        return Err(QuickLendXError::DisputeAlreadyExists);
    }
    
    // Validate creator has stake in invoice (business or investor)
    if creator != &invoice.business {
        if let Some(investor) = &invoice.investor {
            if creator != investor {
                return Err(QuickLendXError::DisputeNotAuthorized);
            }
        } else {
            return Err(QuickLendXError::DisputeNotAuthorized);
        }
    }
    
    // Validate reason and evidence
    if reason.len() == 0 || reason.len() > 500 {
        return Err(QuickLendXError::InvalidDisputeReason);
    }
    
    if evidence.len() == 0 || evidence.len() > 1000 {
        return Err(QuickLendXError::InvalidDisputeEvidence);
    }
    
    // Create dispute
    let dispute = Dispute {
        created_by: creator.clone(),
        created_at: env.ledger().timestamp(),
        reason: reason.clone(),
        evidence,
        resolution: None,
        resolved_by: None,
        resolved_at: None,
    };
    
    // Update invoice with dispute
    invoice.dispute_status = DisputeStatus::Disputed;
    invoice.dispute = Some(dispute);
    
    // Update invoice in storage
    InvoiceStorage::update_invoice(env, &invoice);
    
    // Emit dispute created event
    emit_dispute_created(env, invoice_id, creator, &reason);
    
    Ok(())
}

/// Put a dispute under review (admin function)
pub fn put_dispute_under_review(
    env: &Env,
    invoice_id: &BytesN<32>,
    reviewer: &Address,
) -> Result<(), QuickLendXError> {
    reviewer.require_auth();
    
    let mut invoice = InvoiceStorage::get_invoice(env, invoice_id)
        .ok_or(QuickLendXError::InvoiceNotFound)?;
    
    // Check if dispute exists and is in disputed status
    if invoice.dispute_status != DisputeStatus::Disputed {
        return Err(QuickLendXError::DisputeNotFound);
    }
    
    // Update dispute status
    invoice.dispute_status = DisputeStatus::UnderReview;
    
    // Update invoice in storage
    InvoiceStorage::update_invoice(env, &invoice);
    
    // Emit dispute under review event
    emit_dispute_under_review(env, invoice_id, reviewer);
    
    Ok(())
}

/// Resolve a dispute (admin function)
pub fn resolve_dispute(
    env: &Env,
    invoice_id: &BytesN<32>,
    resolver: &Address,
    resolution: String,
) -> Result<(), QuickLendXError> {
    resolver.require_auth();
    
    let mut invoice = InvoiceStorage::get_invoice(env, invoice_id)
        .ok_or(QuickLendXError::InvoiceNotFound)?;
    
    // Check if dispute exists and is under review
    if invoice.dispute_status != DisputeStatus::UnderReview {
        return Err(QuickLendXError::DisputeNotUnderReview);
    }
    
    // Validate resolution
    if resolution.len() == 0 || resolution.len() > 500 {
        return Err(QuickLendXError::InvalidDisputeReason);
    }
    
    // Update dispute with resolution
    if let Some(mut dispute) = invoice.dispute.clone() {
        dispute.resolution = Some(resolution.clone());
        dispute.resolved_by = Some(resolver.clone());
        dispute.resolved_at = Some(env.ledger().timestamp());
        invoice.dispute = Some(dispute);
    }
    
    // Update dispute status
    invoice.dispute_status = DisputeStatus::Resolved;
    
    // Update invoice in storage
    InvoiceStorage::update_invoice(env, &invoice);
    
    // Emit dispute resolved event
    emit_dispute_resolved(env, invoice_id, resolver, &resolution);
    
    Ok(())
}

/// Get dispute details for an invoice
pub fn get_dispute_details(
    env: &Env,
    invoice_id: &BytesN<32>,
) -> Result<Option<Dispute>, QuickLendXError> {
    let invoice = InvoiceStorage::get_invoice(env, invoice_id)
        .ok_or(QuickLendXError::InvoiceNotFound)?;
    
    Ok(invoice.dispute)
}

/// Get all invoices with disputes
pub fn get_invoices_with_disputes(env: &Env) -> Vec<BytesN<32>> {
    let mut disputed_invoices = Vec::new(env);
    
    // Check all invoice statuses for disputes
    let all_statuses = [
        InvoiceStatus::Pending,
        InvoiceStatus::Verified,
        InvoiceStatus::Funded,
        InvoiceStatus::Paid,
        InvoiceStatus::Defaulted,
    ];
    
    for status in all_statuses.iter() {
        let invoices = InvoiceStorage::get_invoices_by_status(env, status);
        for invoice_id in invoices.iter() {
            if let Some(invoice) = InvoiceStorage::get_invoice(env, &invoice_id) {
                if invoice.dispute_status != DisputeStatus::None {
                    disputed_invoices.push_back(invoice_id);
                }
            }
        }
    }
    
    disputed_invoices
}

/// Get invoices by dispute status
pub fn get_invoices_by_dispute_status(
    env: &Env,
    dispute_status: DisputeStatus,
) -> Vec<BytesN<32>> {
    let mut filtered_invoices = Vec::new(env);
    
    // Check all invoice statuses for specific dispute status
    let all_statuses = [
        InvoiceStatus::Pending,
        InvoiceStatus::Verified,
        InvoiceStatus::Funded,
        InvoiceStatus::Paid,
        InvoiceStatus::Defaulted,
    ];
    
    for status in all_statuses.iter() {
        let invoices = InvoiceStorage::get_invoices_by_status(env, status);
        for invoice_id in invoices.iter() {
            if let Some(invoice) = InvoiceStorage::get_invoice(env, &invoice_id) {
                if invoice.dispute_status == dispute_status {
                    filtered_invoices.push_back(invoice_id);
                }
            }
        }
    }
    
    filtered_invoices
}
