use crate::admin::AdminStorage;
use crate::protocol_limits::{
    MAX_DISPUTE_EVIDENCE_LENGTH, MAX_DISPUTE_REASON_LENGTH, MAX_DISPUTE_RESOLUTION_LENGTH,
};
use crate::storage::{DisputeStorage, InvoiceStorage};
use crate::types::{Dispute, DisputeStatus, Invoice, InvoiceStatus};
use crate::QuickLendXError;
use soroban_sdk::{Address, BytesN, Env, String, Vec};

// ---------------------------------------------------------------------------
// Storage helpers
// ---------------------------------------------------------------------------

pub struct DisputeResolution;

impl DisputeResolution {
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

        // Check if a dispute already exists for this invoice (using the invoice status)
        let mut invoice = InvoiceStorage::get_invoice(env, invoice_id)
            .ok_or(QuickLendXError::InvoiceNotFound)?;

        if invoice.dispute_status != DisputeStatus::None {
            return Err(QuickLendXError::DisputeAlreadyExists);
        }

        // --- 3. Invoice must be in a state where disputes are meaningful ---
        match invoice.status {
            InvoiceStatus::Pending
            | InvoiceStatus::Verified
            | InvoiceStatus::Funded
            | InvoiceStatus::Paid => {}
            _ => return Err(QuickLendXError::InvoiceNotAvailableForFunding),
        }

        let is_business = creator == &invoice.business;
        let is_investor = invoice
            .investor
            .as_ref()
            .map_or(false, |inv| creator == inv);

        if !is_business && !is_investor {
            return Err(QuickLendXError::DisputeNotAuthorized);
        }

        // --- 4. Input validation ---
        if reason.len() == 0 || reason.len() > MAX_DISPUTE_REASON_LENGTH {
            return Err(QuickLendXError::InvalidDisputeReason);
        }
        if evidence.len() == 0 || evidence.len() > MAX_DISPUTE_EVIDENCE_LENGTH {
            return Err(QuickLendXError::InvalidDisputeEvidence);
        }

        // --- 5. Record the dispute on the invoice ---
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

        // --- 6. Persist and index ---
        InvoiceStorage::update_invoice(env, &invoice);
        DisputeStorage::add_to_dispute_index(env, invoice_id);

        Ok(())
    }

    #[allow(dead_code)]
    pub fn put_dispute_under_review(
        env: &Env,
        admin: &Address,
        invoice_id: &BytesN<32>,
    ) -> Result<(), QuickLendXError> {
        // --- 1. Authentication and role check ---
        AdminStorage::require_admin(env, admin)?;

        // --- 2. Load the invoice ---
        let mut invoice = InvoiceStorage::get_invoice(env, invoice_id)
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

    #[allow(dead_code)]
    pub fn resolve_dispute(
        env: &Env,
        admin: &Address,
        invoice_id: &BytesN<32>,
        resolution: &String,
    ) -> Result<(), QuickLendXError> {
        // --- 1. Authentication and role check ---
        AdminStorage::require_admin(env, admin)?;

        // --- 2. Load the invoice ---
        let mut invoice = InvoiceStorage::get_invoice(env, invoice_id)
            .ok_or(QuickLendXError::InvoiceNotFound)?;

        // --- 3. Dispute must exist ---
        if invoice.dispute_status == DisputeStatus::None {
            return Err(QuickLendXError::DisputeNotFound);
        }

        // --- 4. State machine: only UnderReview → Resolved is allowed ---
        if invoice.dispute_status != DisputeStatus::UnderReview {
            return Err(QuickLendXError::DisputeNotUnderReview);
        }

        // --- 5. Validate resolution text ---
        if resolution.len() == 0 || resolution.len() > MAX_DISPUTE_RESOLUTION_LENGTH {
            return Err(QuickLendXError::InvalidDisputeReason);
        }

        // --- 6. Record resolution ---
        let now = env.ledger().timestamp();
        invoice.dispute_status = DisputeStatus::Resolved;
        invoice.dispute.resolution = resolution.clone();
        invoice.dispute.resolved_by = admin.clone();
        invoice.dispute.resolved_at = now;

        InvoiceStorage::update_invoice(env, &invoice);

        Ok(())
    }

    #[allow(dead_code)]
    pub fn get_dispute_details(env: &Env, invoice_id: &BytesN<32>) -> Option<Dispute> {
        let invoice = InvoiceStorage::get_invoice(env, invoice_id)?;
        if invoice.dispute_status == DisputeStatus::None {
            return None;
        }
        Some(invoice.dispute)
    }

    #[allow(dead_code)]
    pub fn get_invoices_with_disputes(env: &Env) -> Vec<BytesN<32>> {
        DisputeStorage::get_dispute_index(env)
    }

    #[allow(dead_code)]
    pub fn get_invoices_by_dispute_status(env: &Env, status: &DisputeStatus) -> Vec<BytesN<32>> {
        let index = DisputeStorage::get_dispute_index(env);
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
}
