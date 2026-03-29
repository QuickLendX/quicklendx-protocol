use crate::admin::AdminStorage;
use crate::protocol_limits::{
    MAX_DISPUTE_EVIDENCE_LENGTH, MAX_DISPUTE_REASON_LENGTH, MAX_DISPUTE_RESOLUTION_LENGTH,
};
use crate::storage::{DisputeStorage, InvoiceStorage};
use crate::types::{Dispute, DisputeStatus, InvoiceStatus};
use crate::QuickLendXError;
use soroban_sdk::{Address, BytesN, Env, String, Vec};

pub struct DisputeResolution;

impl DisputeResolution {
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

        if invoice.dispute_status != DisputeStatus::None {
            return Err(QuickLendXError::DisputeAlreadyExists);
        }

        match invoice.status {
            InvoiceStatus::Pending
            | InvoiceStatus::Verified
            | InvoiceStatus::Funded
            | InvoiceStatus::Paid => {}
            _ => return Err(QuickLendXError::InvoiceNotAvailableForFunding),
        }

        let is_investor = invoice.investor.as_ref().map_or(false, |inv| creator == inv);
        if creator != &invoice.business && !is_investor {
            return Err(QuickLendXError::DisputeNotAuthorized);
        }

        if reason.len() == 0 || reason.len() > MAX_DISPUTE_REASON_LENGTH {
            return Err(QuickLendXError::InvalidDisputeReason);
        }
        if evidence.len() == 0 || evidence.len() > MAX_DISPUTE_EVIDENCE_LENGTH {
            return Err(QuickLendXError::InvalidDisputeEvidence);
        }

        let now = env.ledger().timestamp();
        invoice.dispute_status = DisputeStatus::Disputed;
        invoice.dispute = Dispute {
            created_by: creator.clone(),
            created_at: now,
            reason: reason.clone(),
            evidence: evidence.clone(),
            resolution: String::from_str(env, ""),
            // Fixed address parsing: use Address::from_string directly if it's already a String or String::from_str then Address::from_string
            resolved_by: Address::from_string(&String::from_str(env, "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF")),
            resolved_at: 0,
        };

        InvoiceStorage::update_invoice(env, &invoice);
        DisputeStorage::add_to_dispute_index(env, invoice_id);

        Ok(())
    }

    pub fn resolve_dispute(
        env: &Env,
        invoice_id: &BytesN<32>,
        admin: &Address,
        resolution: String,
    ) -> Result<(), QuickLendXError> {
        AdminStorage::require_admin(env, admin)?;

        let mut invoice = InvoiceStorage::get_invoice(env, invoice_id)
            .ok_or(QuickLendXError::InvoiceNotFound)?;

        if invoice.dispute_status != DisputeStatus::UnderReview {
            return Err(QuickLendXError::DisputeNotUnderReview);
        }

        if resolution.len() == 0 || resolution.len() > MAX_DISPUTE_RESOLUTION_LENGTH {
            return Err(QuickLendXError::InvalidDisputeReason);
        }

        let now = env.ledger().timestamp();
        invoice.dispute_status = DisputeStatus::Resolved;
        invoice.dispute.resolution = resolution.clone();
        invoice.dispute.resolved_by = admin.clone();
        invoice.dispute.resolved_at = now;

        InvoiceStorage::update_invoice(env, &invoice);
        Ok(())
    }
}