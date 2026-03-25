use crate::QuickLendXError;
use crate::invoice::{Invoice, InvoiceStatus};
use crate::protocol_limits::{
    MAX_DISPUTE_EVIDENCE_LENGTH, MAX_DISPUTE_REASON_LENGTH, MAX_DISPUTE_RESOLUTION_LENGTH,
};
use soroban_sdk::{contracttype, Address, BytesN, Env, String, Vec};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DisputeStatus {
    Open,
    UnderReview,
    Resolved,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Dispute {
    pub invoice_id: BytesN<32>,
    pub creator: Address,
    pub reason: String,
    pub evidence: String,
    pub status: DisputeStatus,
    pub resolution: Option<String>,
    pub created_at: u64,
    pub resolved_at: Option<u64>,
}





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

    if reason.len() == 0 || reason.len() > MAX_DISPUTE_REASON_LENGTH {
        return Err(QuickLendXError::InvalidDisputeReason);
    }

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

#[allow(dead_code)]
pub fn get_dispute_details(env: &Env, invoice_id: BytesN<32>) -> Result<Dispute, QuickLendXError> {
    env.storage()
        .persistent()
        .get(&("dispute", invoice_id))
        .ok_or(QuickLendXError::DisputeNotFound)
}

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
