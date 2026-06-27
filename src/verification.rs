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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GuardError {
    NotSubmitted,
    VerificationPending,
    VerificationRejected,
    KycExpired,
    ZeroAmount,
    ArithmeticOverflow,
    InvestmentLimitExceeded {
        requested: u128,
        effective_limit: u128,
    },
    PerInvestmentCapExceeded {
        requested: u128,
        cap: u128,
    },
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
    now: u64,
) -> Result<(), GuardError> {
    match status {
        None => Err(GuardError::NotSubmitted),
        Some(VerificationStatus::Pending) => Err(GuardError::VerificationPending),
        Some(VerificationStatus::Rejected) => Err(GuardError::VerificationRejected),
        Some(VerificationStatus::Verified) if now >= expires_at => Err(GuardError::KycExpired),
        Some(VerificationStatus::Verified) => Ok(()),
    }
}

pub fn validate_transition(_from: VerificationStatus, _to: VerificationStatus) -> Result<(), &'static str> {
    Ok(())
}

pub fn guard_investment_action(
    status: Option<VerificationStatus>,
    amount: u128,
    base_limit: u128,
    tier: InvestorTier,
    risk: RiskLevel,
) -> Result<(), GuardError> {
    match status {
        None => return Err(GuardError::NotSubmitted),
        Some(VerificationStatus::Pending) => return Err(GuardError::VerificationPending),
        Some(VerificationStatus::Rejected) => return Err(GuardError::VerificationRejected),
        Some(VerificationStatus::Verified) => {}
    }

    if amount == 0 {
        return Err(GuardError::ZeroAmount);
    }

    let tier_limit = base_limit
        .checked_mul(tier_multiplier(tier))
        .ok_or(GuardError::ArithmeticOverflow)?;
    let risk_bps = match risk {
        RiskLevel::Low => 10_000,
        RiskLevel::Medium => 7_500,
        RiskLevel::High => 5_000,
        RiskLevel::VeryHigh => 2_500,
    };
    let effective_limit = tier_limit
        .checked_mul(risk_bps)
        .and_then(|value| value.checked_div(10_000))
        .ok_or(GuardError::ArithmeticOverflow)?;

    if amount > effective_limit {
        return Err(GuardError::InvestmentLimitExceeded {
            requested: amount,
            effective_limit,
        });
    }

    let per_investment_cap = match risk {
        RiskLevel::High => effective_limit / 2,
        RiskLevel::VeryHigh => effective_limit / 4,
        RiskLevel::Low | RiskLevel::Medium => effective_limit,
    };
    if amount > per_investment_cap {
        return Err(GuardError::PerInvestmentCapExceeded {
            requested: amount,
            cap: per_investment_cap,
        });
    }

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
