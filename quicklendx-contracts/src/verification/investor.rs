use super::tier::recover_base_limit_from_current_limit;
use super::{
    add_to_kyc_index, calculate_investment_limit, calculate_investor_risk_score,
    compute_investor_tier_from_stats, determine_investor_tier, determine_risk_level,
    get_kyc_index, remove_from_kyc_index, require_investor_not_frozen,
    require_tier_min_investment_amount, validate_risk_score, BusinessVerificationStatus,
    InvestorRiskLevel, InvestorTier, KycIndexKind, KycStorageKey,
};
use crate::errors::QuickLendXError;
use crate::investment::InvestmentStorage;
use crate::protocol_limits::{
    check_string_length, ProtocolLimitsContract, MAX_KYC_DATA_LENGTH, MAX_REJECTION_REASON_LENGTH,
};
use soroban_sdk::{contracttype, Address, Env, String, Vec};

#[contracttype]
pub struct InvestorVerification {
    pub investor: Address,
    pub status: BusinessVerificationStatus,
    pub verified_at: Option<u64>,
    pub verified_by: Option<Address>,
    pub kyc_data: String,
    pub investment_limit: i128,
    pub submitted_at: u64,
    pub tier: InvestorTier,
    pub risk_level: InvestorRiskLevel,
    pub risk_score: u32,
    pub total_invested: i128,
    pub total_returns: i128,
    pub successful_investments: u32,
    pub defaulted_investments: u32,
    pub last_activity: u64,
    pub rejection_reason: Option<String>,
    pub compliance_notes: Option<String>,
}

pub struct InvestorVerificationStorage;


impl InvestorVerificationStorage {
    const VERIFIED_INVESTORS_KEY: &'static str = "verified_investors";
    const PENDING_INVESTORS_KEY: &'static str = "pending_investors";
    const REJECTED_INVESTORS_KEY: &'static str = "rejected_investors";
    #[cfg(test)]
    const INVESTOR_HISTORY_KEY: &'static str = "investor_history";
    #[cfg(test)]
    const INVESTOR_ANALYTICS_KEY: &'static str = "investor_analytics";

    pub fn submit(env: &Env, investor: &Address, kyc_data: String) -> Result<(), QuickLendXError> {
        check_string_length(&kyc_data, MAX_KYC_DATA_LENGTH)?;
        let mut verification = Self::get(env, investor);
        match verification {
            Some(ref existing) => match existing.status {
                BusinessVerificationStatus::Pending => {
                    return Err(QuickLendXError::KYCAlreadyPending)
                }
                BusinessVerificationStatus::Verified => {
                    return Err(QuickLendXError::KYCAlreadyVerified)
                }
                BusinessVerificationStatus::Rejected => {
                    verification = Some(InvestorVerification {
                        investor: investor.clone(),
                        status: BusinessVerificationStatus::Pending,
                        verified_at: None,
                        verified_by: None,
                        kyc_data,
                        investment_limit: existing.investment_limit,
                        submitted_at: env.ledger().timestamp(),
                        tier: existing.tier.clone(),
                        risk_level: existing.risk_level.clone(),
                        risk_score: existing.risk_score,
                        total_invested: existing.total_invested,
                        total_returns: existing.total_returns,
                        successful_investments: existing.successful_investments,
                        defaulted_investments: existing.defaulted_investments,
                        last_activity: existing.last_activity,
                        rejection_reason: None,
                        compliance_notes: None,
                    });
                }
            },
            None => {
                verification = Some(InvestorVerification {
                    investor: investor.clone(),
                    status: BusinessVerificationStatus::Pending,
                    verified_at: None,
                    verified_by: None,
                    kyc_data,
                    investment_limit: 0,
                    submitted_at: env.ledger().timestamp(),
                    tier: InvestorTier::Basic,
                    risk_level: InvestorRiskLevel::High, // Default to high risk for new investors
                    risk_score: 100,                     // Default high risk score
                    total_invested: 0,
                    total_returns: 0,
                    successful_investments: 0,
                    defaulted_investments: 0,
                    last_activity: env.ledger().timestamp(),
                    rejection_reason: None,
                    compliance_notes: None,
                });
            }
        }

        if let Some(v) = verification {
            Self::store(env, &v);
            Self::add_to_pending_investors(env, investor);
        }
        Ok(())
    }

    pub fn store(env: &Env, verification: &InvestorVerification) {
        let key = KycStorageKey::InvestorRecord(verification.investor.clone());
        env.storage().persistent().set(&key, verification);
        crate::storage::extend_persistent_ttl(env, &key);
    }


pub fn get(env: &Env, investor: &Address) -> Option<InvestorVerification> {
    let key = KycStorageKey::InvestorRecord(investor.clone());
    let value = env.storage().persistent().get(&key);
    if value.is_some() {
        crate::storage::extend_persistent_ttl(env, &key);
    }
    value
}


    pub fn update(env: &Env, verification: &InvestorVerification) {
        let old_verification = Self::get(env, &verification.investor);

        // Remove from old status list
        if let Some(old_ver) = old_verification {
            match old_ver.status {
                BusinessVerificationStatus::Verified => {
                    Self::remove_from_verified_investors(env, &verification.investor);
                }
                BusinessVerificationStatus::Pending => {
                    Self::remove_from_pending_investors(env, &verification.investor);
                }
                BusinessVerificationStatus::Rejected => {
                    Self::remove_from_rejected_investors(env, &verification.investor);
                }
            }
        }

        // Store new verification
        Self::store(env, verification);

        // Add to new status list
        match verification.status {
            BusinessVerificationStatus::Verified => {
                Self::add_to_verified_investors(env, &verification.investor);
            }
            BusinessVerificationStatus::Pending => {
                Self::add_to_pending_investors(env, &verification.investor);
            }
            BusinessVerificationStatus::Rejected => {
                Self::add_to_rejected_investors(env, &verification.investor);
            }
        }
    }

    pub fn is_investor_verified(env: &Env, investor: &Address) -> bool {
        if let Some(verification) = Self::get(env, investor) {
            matches!(verification.status, BusinessVerificationStatus::Verified)
        } else {
            false
        }
    }

    pub fn get_verified_investors(env: &Env) -> Vec<Address> {
        get_kyc_index(env, KycIndexKind::VerifiedInvestors)
    }

    pub fn get_pending_investors(env: &Env) -> Vec<Address> {
        get_kyc_index(env, KycIndexKind::PendingInvestors)
    }

    pub fn get_rejected_investors(env: &Env) -> Vec<Address> {
        get_kyc_index(env, KycIndexKind::RejectedInvestors)
    }

    pub fn get_investors_by_tier(env: &Env, tier: InvestorTier) -> Vec<Address> {
        let verified_investors = Self::get_verified_investors(env);
        let mut tier_investors = Vec::new(env);

        for investor in verified_investors.iter() {
            if let Some(verification) = Self::get(env, &investor) {
                if verification.tier == tier {
                    tier_investors.push_back(investor);
                }
            }
        }

        tier_investors
    }

    pub fn get_investors_by_risk_level(env: &Env, risk_level: InvestorRiskLevel) -> Vec<Address> {
        let verified_investors = Self::get_verified_investors(env);
        let mut risk_investors = Vec::new(env);

        for investor in verified_investors.iter() {
            if let Some(verification) = Self::get(env, &investor) {
                if verification.risk_level == risk_level {
                    risk_investors.push_back(investor);
                }
            }
        }

        risk_investors
    }

    fn add_to_verified_investors(env: &Env, investor: &Address) {
        add_to_kyc_index(env, KycIndexKind::VerifiedInvestors, investor);
    }

    fn add_to_pending_investors(env: &Env, investor: &Address) {
        add_to_kyc_index(env, KycIndexKind::PendingInvestors, investor);
    }

    fn add_to_rejected_investors(env: &Env, investor: &Address) {
        add_to_kyc_index(env, KycIndexKind::RejectedInvestors, investor);
    }

    fn remove_from_verified_investors(env: &Env, investor: &Address) {
        remove_from_kyc_index(env, KycIndexKind::VerifiedInvestors, investor);
    }

    fn remove_from_pending_investors(env: &Env, investor: &Address) {
        remove_from_kyc_index(env, KycIndexKind::PendingInvestors, investor);
    }

    fn remove_from_rejected_investors(env: &Env, investor: &Address) {
        remove_from_kyc_index(env, KycIndexKind::RejectedInvestors, investor);
    }
}

/// Enforce that an investor is not in KYC-pending state before allowing a sensitive operation.
///
/// Pending investors have submitted KYC but have not yet been approved or rejected.
/// They must not be allowed to place bids, withdraw bids, or perform any investment
/// action until their identity has been confirmed by an admin.
///
/// # Errors
/// - `KYCAlreadyPending` if the investor has a pending KYC application
/// - `BusinessNotVerified` if the investor has no KYC record or is rejected
pub fn require_investor_not_pending(env: &Env, investor: &Address) -> Result<(), QuickLendXError> {
    match InvestorVerificationStorage::get(env, investor) {
        Some(v) => match v.status {
            BusinessVerificationStatus::Pending => Err(QuickLendXError::KYCAlreadyPending),
            BusinessVerificationStatus::Verified => Ok(()),
            BusinessVerificationStatus::Rejected => Err(QuickLendXError::BusinessNotVerified),
        },
        None => Err(QuickLendXError::BusinessNotVerified),
    }
}

pub fn submit_investor_kyc(
    env: &Env,
    investor: &Address,
    kyc_data: String,
) -> Result<(), QuickLendXError> {
    investor.require_auth();
    // Determine if this is a resubmission before the state changes
    let is_resubmit = InvestorVerificationStorage::get(env, investor)
        .map(|v| matches!(v.status, BusinessVerificationStatus::Rejected))
        .unwrap_or(false);
    InvestorVerificationStorage::submit(env, investor, kyc_data)?;
    crate::events::emit_kyc_submitted(env, investor, is_resubmit);
    crate::audit::log_kyc_operation(
        env,
        crate::audit::AuditOperation::KycSubmitted,
        investor.clone(),
        None,
        Some(String::from_str(env, "Pending")),
        Some(String::from_str(
            env,
            if is_resubmit {
                "resubmission"
            } else {
                "new_submission"
            },
        )),
    );
    Ok(())
}

pub fn verify_investor(
    env: &Env,
    admin: &Address,
    investor: &Address,
    investment_limit: i128,
) -> Result<InvestorVerification, QuickLendXError> {
    admin.require_auth();
    if !crate::admin::AdminStorage::is_admin(env, admin) {
        return Err(QuickLendXError::NotAdmin);
    }

    if investment_limit <= 0 {
        return Err(QuickLendXError::InvalidAmount);
    }

    let mut verification =
        InvestorVerificationStorage::get(env, investor).ok_or(QuickLendXError::KYCNotFound)?;

    match verification.status {
        BusinessVerificationStatus::Verified => return Err(QuickLendXError::KYCAlreadyVerified),
        BusinessVerificationStatus::Pending | BusinessVerificationStatus::Rejected => {
            // Calculate risk score and determine tier
            let risk_score = calculate_investor_risk_score(env, investor, &verification.kyc_data)?;
            validate_risk_score(risk_score)?;
            let tier = determine_investor_tier(env, investor, risk_score)?;
            let risk_level = determine_risk_level(risk_score);

            // Calculate final investment limit based on tier and risk
            let calculated_limit = calculate_investment_limit(&tier, &risk_level, investment_limit);

            verification.status = BusinessVerificationStatus::Verified;
            verification.verified_at = Some(env.ledger().timestamp());
            verification.verified_by = Some(admin.clone());
            verification.investment_limit = calculated_limit;
            verification.tier = tier;
            verification.risk_level = risk_level;
            verification.risk_score = risk_score;
            verification.compliance_notes = Some(String::from_str(env, "Verified by admin"));

            InvestorVerificationStorage::update(env, &verification);
            crate::audit::log_kyc_operation(
                env,
                crate::audit::AuditOperation::KycVerified,
                admin.clone(),
                Some(String::from_str(env, "Pending")),
                Some(String::from_str(env, "Verified")),
                Some(String::from_str(env, "investor")),
            );
            Ok(verification)
        }
    }
}

/// Reject a pending investor KYC record with an auditable reason.
///
/// # Errors
/// - `NotAdmin` if `admin` is not a contract admin
/// - `KYCNotFound` if the investor has no KYC record
/// - `InvalidKYCStatus` if the investor is not currently `Pending`
/// - `InvalidDescription` if `reason` exceeds `MAX_REJECTION_REASON_LENGTH`
pub fn reject_investor(
    env: &Env,
    admin: &Address,
    investor: &Address,
    reason: String,
) -> Result<(), QuickLendXError> {
    check_string_length(&reason, MAX_REJECTION_REASON_LENGTH)?;
    admin.require_auth();
    if !crate::admin::AdminStorage::is_admin(env, admin) {
        return Err(QuickLendXError::NotAdmin);
    }
    let mut verification =
        InvestorVerificationStorage::get(env, investor).ok_or(QuickLendXError::KYCNotFound)?;
    if !matches!(verification.status, BusinessVerificationStatus::Pending) {
        return Err(QuickLendXError::InvalidKYCStatus);
    }

    verification.status = BusinessVerificationStatus::Rejected;
    verification.verified_at = Some(env.ledger().timestamp());
    verification.verified_by = Some(admin.clone());
    verification.rejection_reason = Some(reason.clone());
    verification.compliance_notes = Some(String::from_str(env, "Rejected by admin"));

    InvestorVerificationStorage::update(env, &verification);
    crate::events::emit_kyc_rejected(env, investor, admin, &reason);
    crate::audit::log_kyc_operation(
        env,
        crate::audit::AuditOperation::KycRejected,
        admin.clone(),
        Some(String::from_str(env, "Pending")),
        Some(String::from_str(env, "Rejected")),
        Some(reason),
    );
    Ok(())
}

/// Revoke a previously-verified investor's KYC (admin only).
///
/// # Threat mitigated
/// A verified investor whose identity/compliance status is later found to be
/// invalid (sanctions hit, fraudulent KYC, compromised key) would otherwise
/// retain the ability to place and fund bids indefinitely. Without an explicit
/// revoke path, an admin can only set a new investment limit — they cannot stop
/// the investor from continuing to bid. This entrypoint moves the investor from
/// `Verified` back to `Rejected`, which causes `validate_investor_investment`
/// (and therefore `validate_bid`) to fail with `BusinessNotVerified`, blocking
/// all further bids until the investor re-submits KYC and is re-verified.
///
/// Emits a `kyc_revoke` event recording the investor, admin, timestamp, and
/// reason for the audit trail.
///
/// # Errors
/// - `NotAdmin` if `admin` is not a contract admin
/// - `KYCNotFound` if the investor has no KYC record
/// - `InvalidKYCStatus` if the investor is not currently `Verified`
/// - `InvalidDescription` if `reason` exceeds `MAX_REJECTION_REASON_LENGTH`
pub fn revoke_investor_kyc(
    env: &Env,
    admin: &Address,
    investor: &Address,
    reason: String,
) -> Result<(), QuickLendXError> {
    check_string_length(&reason, MAX_REJECTION_REASON_LENGTH)?;
    admin.require_auth();
    if !crate::admin::AdminStorage::is_admin(env, admin) {
        return Err(QuickLendXError::NotAdmin);
    }

    let mut verification =
        InvestorVerificationStorage::get(env, investor).ok_or(QuickLendXError::KYCNotFound)?;

    // Only a currently-verified investor can be revoked. Pending/rejected
    // investors are already blocked from bidding, so revoking them is a no-op
    // that we reject explicitly to keep the state machine auditable.
    if !matches!(verification.status, BusinessVerificationStatus::Verified) {
        return Err(QuickLendXError::InvalidKYCStatus);
    }

    verification.status = BusinessVerificationStatus::Rejected;
    verification.verified_at = Some(env.ledger().timestamp());
    verification.verified_by = Some(admin.clone());
    verification.rejection_reason = Some(reason.clone());
    verification.compliance_notes = Some(String::from_str(env, "KYC revoked by admin"));

    InvestorVerificationStorage::update(env, &verification);
    crate::events::emit_kyc_revoked(env, investor, admin, &reason);
    crate::audit::log_kyc_operation(
        env,
        crate::audit::AuditOperation::KycRevoked,
        admin.clone(),
        Some(String::from_str(env, "Verified")),
        Some(String::from_str(env, "Rejected")),
        Some(reason),
    );
    Ok(())
}

pub fn get_investor_verification(env: &Env, investor: &Address) -> Option<InvestorVerification> {
    InvestorVerificationStorage::get(env, investor)
}

/// Update investor analytics after an investment
pub fn update_investor_analytics(
    env: &Env,
    investor: &Address,
    investment_amount: i128,
    is_successful: bool,
) -> Result<(), QuickLendXError> {
    if investment_amount <= 0 {
        return Err(QuickLendXError::InvalidAmount);
    }

    if let Some(mut verification) = InvestorVerificationStorage::get(env, investor) {
        let prior_base_limit = recover_base_limit_from_current_limit(
            verification.investment_limit,
            &verification.tier,
            &verification.risk_level,
        );

        verification.total_invested = verification
            .total_invested
            .saturating_add(investment_amount);
        verification.last_activity = env.ledger().timestamp();

        if is_successful {
            verification.successful_investments =
                verification.successful_investments.saturating_add(1);
            // Calculate returns (simplified - would need actual return data)
            let estimated_return = investment_amount.saturating_mul(110).saturating_div(100); // 10% return
            verification.total_returns =
                verification.total_returns.saturating_add(estimated_return);
        } else {
            verification.defaulted_investments =
                verification.defaulted_investments.saturating_add(1);
        }

        // Recalculate risk score and tier
        verification.risk_score =
            calculate_investor_risk_score(env, investor, &verification.kyc_data)?;
        verification.risk_level = determine_risk_level(verification.risk_score);
        verification.tier = compute_investor_tier_from_stats(
            verification.total_invested,
            verification.successful_investments,
            verification.defaulted_investments,
            verification.risk_score,
        )?;

        // Preserve the investor's approved baseline and only re-derive the
        // dynamic limit using the updated tier/risk profile.
        let base_limit = prior_base_limit.max(1);

        verification.investment_limit =
            calculate_investment_limit(&verification.tier, &verification.risk_level, base_limit);

        InvestorVerificationStorage::update(env, &verification);
    }

    Ok(())
}

/// Get investor analytics summary
pub fn get_investor_analytics(
    env: &Env,
    investor: &Address,
) -> Result<InvestorVerification, QuickLendXError> {
    InvestorVerificationStorage::get(env, investor).ok_or(QuickLendXError::KYCNotFound)
}

/// Get total active risk exposure (active bids + active investments) for an investor.
pub fn get_investor_total_exposure(env: &Env, investor: &Address) -> i128 {
    let active_bid_exposure = BidStorage::get_active_bid_amount_sum_for_investor(env, investor);
    let active_investment_exposure =
        InvestmentStorage::get_active_investment_amount_sum_for_investor(env, investor);
    active_bid_exposure.saturating_add(active_investment_exposure)
}

/// Validate investor can make investment based on limits and risk
pub fn validate_investor_investment(
    env: &Env,
    investor: &Address,
    investment_amount: i128,
) -> Result<(), QuickLendXError> {
    require_investor_not_frozen(env, investor)?;

    let limits = ProtocolLimitsContract::get_protocol_limits(env.clone());

    if let Some(verification) = InvestorVerificationStorage::get(env, investor) {
        if verification.tier < limits.min_investor_tier {
            return Err(QuickLendXError::InsufficientKYCTier);
        }

        // 1. Verification status check
        if !matches!(verification.status, BusinessVerificationStatus::Verified) {
            return Err(QuickLendXError::BusinessNotVerified);
        }

        // 2. Aggregate Limit Check
        // Ensure that (new bid + existing active bids + active investments + total funded volume) fits within the limit
        let active_bid_exposure = BidStorage::get_active_bid_amount_sum_for_investor(env, investor);
        let active_investment_exposure =
            InvestmentStorage::get_active_investment_amount_sum_for_investor(env, investor);
        let total_risk_exposure = active_bid_exposure
            .saturating_add(active_investment_exposure)
            .saturating_add(verification.total_invested)
            .saturating_add(investment_amount);

        if total_risk_exposure > verification.investment_limit {
            return Err(QuickLendXError::InvalidAmount);
        }

        // 3. Risk-Based Tiered Checks
        // Further constraints based on the specific risk level assigned by Admin
        match verification.risk_level {
            InvestorRiskLevel::VeryHigh => {
                // Individual bid amount caps for high-risk profiles
                if investment_amount > 10000 {
                    return Err(QuickLendXError::InvalidAmount);
                }
            }
            InvestorRiskLevel::High => {
                // High risk investors have moderate restrictions
                if investment_amount > 50000 {
                    return Err(QuickLendXError::InvalidAmount);
                }
            }
            _ => {
                // Medium and low risk investors can invest up to their limit
            }
        }

        Ok(())
    } else {
        Err(QuickLendXError::KYCNotFound)
    }
}

/// Recompute investor rating from on-chain history deterministically.
///
/// Recalculates the risk score via `calculate_investor_risk_score`, then
/// derives the tier, risk level, and investment limit from the updated score
/// and the investor's accumulated performance counters.  Updates the stored
/// verification record and returns the full updated record.
///
/// # Arguments
/// * `env` - The contract environment
/// * `admin` - The admin address (must be authorized)
/// * `investor` - The investor address to recompute the rating for
///
/// # Returns
/// * `Ok(InvestorVerification)` - The updated verification record with the new rating
///
/// # Errors
/// * `NotAdmin` if the caller is not the current admin
/// * `KYCNotFound` if the investor has no verification record
pub fn investor_rating_recompute(
    env: &Env,
    admin: &Address,
    investor: &Address,
) -> Result<InvestorVerification, QuickLendXError> {
    admin.require_auth();
    if !crate::admin::AdminStorage::is_admin(env, admin) {
        return Err(QuickLendXError::NotAdmin);
    }

    let mut verification =
        InvestorVerificationStorage::get(env, investor).ok_or(QuickLendXError::KYCNotFound)?;

    if !matches!(verification.status, BusinessVerificationStatus::Verified) {
        return Err(QuickLendXError::InvalidKYCStatus);
    }

    let prior_tier = verification.tier.clone();
    let prior_risk_level = verification.risk_level.clone();
    let base_limit = recover_base_limit_from_current_limit(
        verification.investment_limit,
        &prior_tier,
        &prior_risk_level,
    )
    .max(1);

    let risk_score = calculate_investor_risk_score(env, investor, &verification.kyc_data)?;
    validate_risk_score(risk_score)?;

    let tier = compute_investor_tier_from_stats(
        verification.total_invested,
        verification.successful_investments,
        verification.defaulted_investments,
        risk_score,
    )?;
    let risk_level = determine_risk_level(risk_score);
    let investment_limit = calculate_investment_limit(&tier, &risk_level, base_limit);

    verification.risk_score = risk_score;
    verification.risk_level = risk_level;
    verification.tier = tier;
    verification.investment_limit = investment_limit;
    verification.compliance_notes = Some(String::from_str(
        env,
        "Investor rating recomputed from on-chain history",
    ));

    InvestorVerificationStorage::update(env, &verification);
    Ok(verification)
}

/// Set investment limit for a verified investor (admin only)
pub fn set_investment_limit(
    env: &Env,
    admin: &Address,
    investor: &Address,
    new_limit: i128,
) -> Result<(), QuickLendXError> {
    admin.require_auth();

    // Check admin authorization
    if !crate::admin::AdminStorage::is_admin(env, admin) {
        return Err(QuickLendXError::NotAdmin);
    }

    if new_limit <= 0 {
        return Err(QuickLendXError::InvalidAmount);
    }

    let mut verification =
        InvestorVerificationStorage::get(env, investor).ok_or(QuickLendXError::KYCNotFound)?;

    // Only allow setting limits for verified investors
    if !matches!(verification.status, BusinessVerificationStatus::Verified) {
        return Err(QuickLendXError::InvalidKYCStatus);
    }

    // Calculate final investment limit based on tier and risk
    let calculated_limit =
        calculate_investment_limit(&verification.tier, &verification.risk_level, new_limit);

    verification.investment_limit = calculated_limit;
    verification.compliance_notes =
        Some(String::from_str(env, "Investment limit updated by admin"));

    InvestorVerificationStorage::update(env, &verification);
    Ok(())
}

