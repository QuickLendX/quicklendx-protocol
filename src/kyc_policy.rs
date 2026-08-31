//! One deterministic KYC eligibility predicate for every KYC-dependent action.
use crate::verification::VerificationStatus;
use soroban_sdk::contracttype;

#[contracttype]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KycRecord {
    V1(KycRecordV1),
}

#[contracttype]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KycRecordV1 {
    pub status: VerificationStatus,
    pub expires_at: u64,
    pub version: u32,
}

#[contracttype]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KycEligibilityError {
    Missing,
    Pending,
    Revoked,
    Expired,
    InvalidExpiry,
    ReplayedNonce,
}

#[contracttype]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KycActor {
    Business,
    Investor,
}

#[contracttype]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KycDependentAction {
    CreateInvoice,
    SubmitBid,
    FundInvoice,
    SettleInvoice,
}

/// The shared predicate. Expiry is exclusive: 
ow == expires_at is expired.
pub fn require_eligible(
    record: Option<KycRecord>,
    now: u64,
    nonce: u64,
) -> Result<KycRecord, KycEligibilityError> {
    let record = record.ok_or(KycEligibilityError::Missing)?;
    let KycRecord::V1(inner) = &record;
    if inner.expires_at == 0 {
        return Err(KycEligibilityError::InvalidExpiry);
    }
    let status_res = match inner.status {
        VerificationStatus::Verified if now < inner.expires_at => Ok(record.clone()),
        VerificationStatus::Verified => Err(KycEligibilityError::Expired),
        VerificationStatus::Pending => Err(KycEligibilityError::Pending),
        VerificationStatus::Rejected => Err(KycEligibilityError::Revoked),
    };
    if status_res.is_err() {
        return status_res;
    }
    if nonce > 0 {
        match crate::kyc_nonces::check_and_record_nonce(inner.version, nonce, None, None) {
            crate::kyc_nonces::NonceCheckResult::New
            | crate::kyc_nonces::NonceCheckResult::SafeRetry => {}
            crate::kyc_nonces::NonceCheckResult::Conflict => {
                return Err(KycEligibilityError::ReplayedNonce)
            }
        }
    }
    status_res
}

/// Apply the same predicate to business and investor entrypoints.
pub fn authorize_action(
    record: Option<KycRecord>,
    actor: KycActor,
    action: KycDependentAction,
    now: u64,
    nonce: u64,
) -> Result<KycRecord, KycEligibilityError> {
    let record = record.ok_or(KycEligibilityError::Missing)?;
    let KycRecord::V1(inner) = &record;
    if inner.expires_at == 0 {
        return Err(KycEligibilityError::InvalidExpiry);
    }
    let actor_allowed = match action {
        KycDependentAction::CreateInvoice => actor == KycActor::Business,
        KycDependentAction::SubmitBid | KycDependentAction::FundInvoice => {
            actor == KycActor::Investor
        }
        KycDependentAction::SettleInvoice => true,
    };
    let status_res = match inner.status {
        VerificationStatus::Verified if now < inner.expires_at => {
            if actor_allowed {
                Ok(record.clone())
            } else {
                Err(KycEligibilityError::Revoked)
            }
        }
        VerificationStatus::Verified => Err(KycEligibilityError::Expired),
        VerificationStatus::Pending => Err(KycEligibilityError::Pending),
        VerificationStatus::Rejected => Err(KycEligibilityError::Revoked),
    };
    if status_res.is_err() {
        return status_res;
    }
    if nonce > 0 {
        match crate::kyc_nonces::check_and_record_nonce(
            inner.version,
            nonce,
            Some(actor),
            Some(action),
        ) {
            crate::kyc_nonces::NonceCheckResult::New
            | crate::kyc_nonces::NonceCheckResult::SafeRetry => {}
            crate::kyc_nonces::NonceCheckResult::Conflict => {
                return Err(KycEligibilityError::ReplayedNonce)
            }
        }
    }
    status_res
}

/// Check eligibility before a financial side effect or public bid mutation.
pub fn authorize_before_side_effect(
    record: Option<KycRecord>,
    actor: KycActor,
    action: KycDependentAction,
    now: u64,
    terminal: bool,
    nonce: u64,
) -> Result<KycRecord, KycEligibilityError> {
    if terminal {
        return Ok(record.unwrap_or(KycRecord::V1(KycRecordV1 {
            status: VerificationStatus::Verified,
            expires_at: u64::MAX,
            version: 0,
        })));
    }
    let record = authorize_action(record, actor, action, now, nonce)?;
    Ok(record)
}

/// Verification updates may be stored after a terminal financial record, but
/// they cannot change the terminal record's outcome.
pub fn can_update_verification(
    record_is_terminal: bool,
    current_version: u32,
    next_version: u32,
) -> bool {
    !record_is_terminal && next_version > current_version
}

pub fn is_terminal_action(action: KycDependentAction) -> bool {
    action == KycDependentAction::SettleInvoice
}

#[cfg(test)]
mod tests {
    use super::*;

    fn verified(expires_at: u64) -> Option<KycRecord> {
        Some(KycRecord::V1(KycRecordV1 {
            status: VerificationStatus::Verified,
            expires_at,
            version: 1,
        }))
    }

    #[test]
    fn verified_before_expiry_is_accepted() {
        assert!(require_eligible(verified(100), 99, 1).is_ok());
    }
    #[test]
    fn exact_expiry_is_rejected() {
        assert_eq!(
            require_eligible(verified(100), 100, 1),
            Err(KycEligibilityError::Expired)
        );
    }
    #[test]
    fn versions_must_increase() {
        assert!(can_update_verification(false, 1, 2));
        assert!(!can_update_verification(false, 2, 2));
    }
    #[test]
    fn terminal_record_cannot_be_retroactively_updated() {
        assert!(!can_update_verification(true, 1, 2));
    }
    #[test]
    fn action_classification_is_stable() {
        assert!(is_terminal_action(KycDependentAction::SettleInvoice));
        assert!(!is_terminal_action(KycDependentAction::FundInvoice));
    }
    #[test]
    fn safe_retry_same_nonce_succeeds() {
        crate::kyc_nonces::reset_nonces();
        let r = verified(100);
        assert!(authorize_action(
            r.clone(),
            KycActor::Business,
            KycDependentAction::CreateInvoice,
            50,
            101
        )
        .is_ok());
        // Safe retry (same operation and nonce)
        assert!(authorize_action(
            r,
            KycActor::Business,
            KycDependentAction::CreateInvoice,
            50,
            101
        )
        .is_ok());
    }
    #[test]
    fn conflicting_reuse_same_nonce_fails() {
        crate::kyc_nonces::reset_nonces();
        let r = verified(100);
        assert!(authorize_action(
            r.clone(),
            KycActor::Business,
            KycDependentAction::CreateInvoice,
            50,
            102
        )
        .is_ok());
        // Conflicting reuse (different actor or action with same nonce)
        assert_eq!(
            authorize_action(
                r,
                KycActor::Investor,
                KycDependentAction::SubmitBid,
                50,
                102
            ),
            Err(KycEligibilityError::ReplayedNonce)
        );
    }
}
