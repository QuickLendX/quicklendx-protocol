use crate::QuickLendXError;
use soroban_sdk::{contracttype, Address, Env, String, Vec};

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
    pub invoice_id: u64,
    pub creator: Address,
    pub reason: String,
    pub evidence: String,
    pub status: DisputeStatus,
    pub resolution: Option<String>,
    pub created_at: u64,
    pub resolved_at: Option<u64>,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InvoiceStatus {
    Funded,
    Settled,
    Defaulted,
}

#[allow(dead_code)]
const MAX_REASON_LENGTH: u32 = 500;
#[allow(dead_code)]
const MAX_EVIDENCE_LENGTH: u32 = 2000;
#[allow(dead_code)]
const MAX_RESOLUTION_LENGTH: u32 = 1000;

#[allow(dead_code)]
pub fn create_dispute(
    env: Env,
    invoice_id: u64,
    creator: Address,
    reason: String,
    evidence: String,
) -> Result<(), QuickLendXError> {
    creator.require_auth();

    if env.storage().persistent().has(&("dispute", invoice_id)) {
        return Err(QuickLendXError::DisputeAlreadyExists);
    }

    let invoice_status: Option<InvoiceStatus> = env
        .storage()
        .persistent()
        .get(&("invoice_status", invoice_id));

    match invoice_status {
        Some(InvoiceStatus::Funded) | Some(InvoiceStatus::Settled) => {}
        _ => return Err(QuickLendXError::InvoiceNotAvailableForFunding),
    }

    let invoice_data: Option<(Address, Address, i128)> =
        env.storage().persistent().get(&("invoice", invoice_id));

    if let Some((business, investor, _)) = invoice_data {
        if creator != business && creator != investor {
            return Err(QuickLendXError::DisputeNotAuthorized);
        }
    } else {
        return Err(QuickLendXError::InvoiceNotFound);
    }

    if reason.len() == 0 || reason.len() > MAX_REASON_LENGTH {
        return Err(QuickLendXError::InvalidDisputeReason);
    }

    if evidence.len() > MAX_EVIDENCE_LENGTH {
        return Err(QuickLendXError::InvalidDisputeEvidence);
    }

    let dispute = Dispute {
        invoice_id,
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
    invoice_id: u64,
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
        .get(&("dispute", invoice_id))
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
    invoice_id: u64,
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
        .get(&("dispute", invoice_id))
        .ok_or(QuickLendXError::DisputeNotFound)?;

    if dispute.status != DisputeStatus::UnderReview {
        return Err(QuickLendXError::DisputeNotUnderReview);
    }

    if dispute.status == DisputeStatus::Resolved {
        return Err(QuickLendXError::DisputeAlreadyResolved);
    }

    if resolution.len() == 0 || resolution.len() > MAX_RESOLUTION_LENGTH {
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
pub fn get_dispute_details(env: &Env, invoice_id: u64) -> Result<Dispute, QuickLendXError> {
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
