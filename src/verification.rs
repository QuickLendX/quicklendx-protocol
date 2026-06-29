#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvestorTier {
    Basic,
    Silver,
    Gold,
    Platinum,
    Vip,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerificationStatus {
    Pending,
    Verified,
    Rejected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RiskLevel {
    Low,
    Medium,
    High,
    VeryHigh,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuardError {
    NotSubmitted,
    NotVerified,
    KycExpired,
}

pub fn compute_investor_tier(
    total_invested: i128,
    successful_investments: u32,
    defaulted_investments: u32,
    _risk_score: u32,
) -> InvestorTier {
    if total_invested > 5_000_000 && successful_investments > 50 && defaulted_investments < 5 {
        InvestorTier::Vip
    } else if total_invested > 1_000_000 && successful_investments > 20 {
        InvestorTier::Platinum
    } else if total_invested > 100_000 && successful_investments > 10 {
        InvestorTier::Gold
    } else if total_invested > 10_000 && successful_investments > 3 {
        InvestorTier::Silver
    } else {
        InvestorTier::Basic
    }
}

pub fn tier_multiplier(tier: InvestorTier) -> u128 {
    match tier {
        InvestorTier::Basic => 1,
        InvestorTier::Silver => 2,
        InvestorTier::Gold => 3,
        InvestorTier::Platinum => 5,
        InvestorTier::Vip => 10,
    }
}

pub fn verify_business_kyc(
    status: Option<VerificationStatus>,
    expires_at: u64,
    current_time: u64,
) -> Result<(), GuardError> {
    match status {
        None => Err(GuardError::NotSubmitted),
        Some(VerificationStatus::Verified) if current_time < expires_at => Ok(()),
        Some(VerificationStatus::Verified) => Err(GuardError::KycExpired),
        Some(_) => Err(GuardError::NotVerified),
    }
}

pub fn validate_transition(
    _from: VerificationStatus,
    _to: VerificationStatus,
) -> Result<(), &'static str> {
    Ok(())
}

pub fn guard_investment_action(
    _status: Option<VerificationStatus>,
    _amount: u128,
    _base_limit: u128,
    _tier: InvestorTier,
    _risk: RiskLevel,
) -> Result<(), &'static str> {
    Ok(())
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
}
