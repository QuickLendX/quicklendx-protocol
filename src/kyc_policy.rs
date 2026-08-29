use crate::verification::{VerificationStatus, InvestorTier, RiskLevel, tier_multiplier};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuardError {
    NotSubmitted,
    KycExpired,
    NotVerified,
    InvalidTransition,
    LimitExceeded,
    InvalidTenant,
}

pub fn verify_business_kyc(
    status: Option<VerificationStatus>,
    expiration: u64,
    current: u64,
) -> Result<(), GuardError> {
    match status {
        None => Err(GuardError::NotSubmitted),
        Some(VerificationStatus::Pending) | Some(VerificationStatus::Rejected) => Err(GuardError::NotVerified),
        Some(VerificationStatus::Verified) => {
            if current >= expiration {
                Err(GuardError::KycExpired)
            } else {
                Ok(())
            }
        }
    }
}

pub fn validate_transition(from: VerificationStatus, to: VerificationStatus) -> Result<(), GuardError> {
    if from == to {
        return Err(GuardError::InvalidTransition);
    }
    match (from, to) {
        (VerificationStatus::Pending, VerificationStatus::Verified) => Ok(()),
        (VerificationStatus::Pending, VerificationStatus::Rejected) => Ok(()),
        (VerificationStatus::Rejected, VerificationStatus::Pending) => Ok(()),
        (VerificationStatus::Verified, VerificationStatus::Rejected) => Ok(()),
        _ => Err(GuardError::InvalidTransition),
    }
}

pub fn guard_investment_action(
    status: Option<VerificationStatus>,
    amount: u128,
    base_limit: u128,
    tier: InvestorTier,
    _risk: RiskLevel,
    tenant_id: u64,
    expected_tenant: u64,
) -> Result<(), GuardError> {
    if tenant_id != expected_tenant {
        return Err(GuardError::InvalidTenant);
    }
    
    match status {
        Some(VerificationStatus::Verified) => {
            let max_limit = base_limit.saturating_mul(tier_multiplier(tier));
            if amount > max_limit {
                return Err(GuardError::LimitExceeded);
            }
            Ok(())
        },
        Some(VerificationStatus::Pending) | Some(VerificationStatus::Rejected) => Err(GuardError::NotVerified),
        None => Err(GuardError::NotSubmitted),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returns_error_when_kyc_is_missing() {
        let result = verify_business_kyc(None, 2000, 1000);
        assert_eq!(result, Err(GuardError::NotSubmitted));
    }

    #[test]
    fn returns_error_when_kyc_is_expired() {
        let exact_boundary = verify_business_kyc(Some(VerificationStatus::Verified), 2000, 2000);
        assert_eq!(exact_boundary, Err(GuardError::KycExpired));

        let past_boundary = verify_business_kyc(Some(VerificationStatus::Verified), 2000, 2001);
        assert_eq!(past_boundary, Err(GuardError::KycExpired));
    }

    #[test]
    fn succeeds_when_kyc_is_current() {
        let result = verify_business_kyc(Some(VerificationStatus::Verified), 2000, 1999);
        assert_eq!(result, Ok(()));
    }
    
    #[test]
    fn test_validate_transition() {
        assert_eq!(validate_transition(VerificationStatus::Pending, VerificationStatus::Verified), Ok(()));
        assert_eq!(validate_transition(VerificationStatus::Verified, VerificationStatus::Verified), Err(GuardError::InvalidTransition));
        assert_eq!(validate_transition(VerificationStatus::Verified, VerificationStatus::Pending), Err(GuardError::InvalidTransition));
    }
    
    #[test]
    fn test_guard_investment_action() {
        assert_eq!(guard_investment_action(Some(VerificationStatus::Verified), 1000, 1000, InvestorTier::Basic, RiskLevel::Low, 1, 1), Ok(()));
        assert_eq!(guard_investment_action(Some(VerificationStatus::Verified), 1001, 1000, InvestorTier::Basic, RiskLevel::Low, 1, 1), Err(GuardError::LimitExceeded));
        assert_eq!(guard_investment_action(Some(VerificationStatus::Verified), 1000, 1000, InvestorTier::Basic, RiskLevel::Low, 1, 2), Err(GuardError::InvalidTenant));
        assert_eq!(guard_investment_action(None, 1000, 1000, InvestorTier::Basic, RiskLevel::Low, 1, 1), Err(GuardError::NotSubmitted));
    }
}
