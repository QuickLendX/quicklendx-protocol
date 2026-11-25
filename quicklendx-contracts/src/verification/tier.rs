use super::{InvestorVerification, InvestorVerificationStorage};
use crate::errors::QuickLendXError;
use crate::investment::InvestmentStorage;
use soroban_sdk::{contracttype, Address, Env};

/// Check if actual investor KYC tier meets or exceeds required tier.
pub fn is_investor_kyc_tier_sufficient(actual: u32, required: u32) -> bool {
    actual >= required
}

#[contracttype]
#[derive(Clone, Eq, PartialEq, Debug, PartialOrd, Ord)]
pub enum InvestorTier {
    Basic,
    Silver,
    Gold,
    Platinum,
    VIP,
}

#[contracttype]
#[derive(Clone, PartialEq, Debug)]
pub enum InvestorRiskLevel {
    Low,
    Medium,
    High,
    VeryHigh,
}

pub fn validate_risk_score(score: u32) -> Result<(), QuickLendXError> {
    if score > 100 {
        return Err(QuickLendXError::InvalidAmount);
    }
    Ok(())
}

// from tracked performance counters plus the investor's risk score.
// These constants make the decision rules auditable and stable across runs.
const VIP_RISK_SCORE_MAX: u32 = 10;
const VIP_TOTAL_INVESTED_MIN: i128 = 5_000_000;
const VIP_SUCCESSFUL_INVESTMENTS_MIN: u32 = 50;
const VIP_DEFAULT_RATE_MAX_PCT: u32 = 5;

const PLATINUM_RISK_SCORE_MAX: u32 = 20;
const PLATINUM_TOTAL_INVESTED_MIN: i128 = 1_000_000;
const PLATINUM_SUCCESSFUL_INVESTMENTS_MIN: u32 = 20;
const PLATINUM_DEFAULT_RATE_MAX_PCT: u32 = 10;

const GOLD_RISK_SCORE_MAX: u32 = 40;
const GOLD_TOTAL_INVESTED_MIN: i128 = 100_000;
const GOLD_SUCCESSFUL_INVESTMENTS_MIN: u32 = 10;
const GOLD_DEFAULT_RATE_MAX_PCT: u32 = 15;

const SILVER_RISK_SCORE_MAX: u32 = 60;
const SILVER_TOTAL_INVESTED_MIN: i128 = 10_000;
const SILVER_SUCCESSFUL_INVESTMENTS_MIN: u32 = 3;
const SILVER_DEFAULT_RATE_MAX_PCT: u32 = 25;

/// Calculate investor risk score based on various factors
pub fn calculate_investor_risk_score(
    env: &Env,
    investor: &Address,
    kyc_data: &String,
) -> Result<u32, QuickLendXError> {
    let mut risk_score = 0u32;

    // Base risk score from KYC data analysis (simplified)
    // In a real implementation, this would analyze the KYC data
    let kyc_length = kyc_data.len();
    if kyc_length < 100 {
        risk_score += 30; // High risk for incomplete KYC
    } else if kyc_length < 500 {
        risk_score += 20; // Medium risk
    } else {
        risk_score += 10; // Lower risk for comprehensive KYC
    }

    // Check investment history if available
    if let Some(verification) = InvestorVerificationStorage::get(env, investor) {
        let total_investments =
            verification.successful_investments + verification.defaulted_investments;

        if total_investments > 0 {
            let default_rate = (verification.defaulted_investments * 100) / total_investments;
            risk_score += default_rate;
        }

        // Adjust based on total invested amount
        if verification.total_invested > 1000000 {
            // 1M+ invested
            risk_score = risk_score.saturating_sub(20);
        } else if verification.total_invested > 100000 {
            // 100K+ invested
            risk_score = risk_score.saturating_sub(10);
        }
    }

    // Cap risk score at 100
    if risk_score > 100 {
        risk_score = 100;
    }

    Ok(risk_score)
}

/// Compute investor tier from the investor record and a risk score.
///
/// This wrapper is used by public contract entrypoints and internal business
/// logic, while the core deterministic tier rules are implemented in the
/// `compute_investor_tier_from_stats` helper.
pub fn compute_investor_tier(
    env: &Env,
    investor: &Address,
    risk_score: u32,
) -> Result<InvestorTier, QuickLendXError> {
    validate_risk_score(risk_score)?;

    if let Some(verification) = InvestorVerificationStorage::get(env, investor) {
        return compute_investor_tier_from_stats(
            verification.total_invested,
            verification.successful_investments,
            verification.defaulted_investments,
            risk_score,
        );
    }

    Ok(InvestorTier::Basic)
}

/// Determine investor tier based on risk score and investment history.
/// Calculate the investor tier using deterministic performance thresholds.
///
/// Promotion is based on the investor's accumulated performance counters and
/// risk score. The mapping is stable and idempotent: the same counters always
/// yield the same tier.
///
/// Threshold table:
/// | Tier | Risk Score | Total Invested | Successful Investments | Max Default Rate |
/// |------|------------|----------------|------------------------|------------------|
/// | VIP | <= 10 | >= 5,000,000 | >= 50 | <= 5% |
/// | Platinum | <= 20 | >= 1,000,000 | >= 20 | <= 10% |
/// | Gold | <= 40 | >= 100,000 | >= 10 | <= 15% |
/// | Silver | <= 60 | >= 10,000 | >= 3 | <= 25% |
/// | Basic | otherwise | - | - | - |
pub fn determine_investor_tier(
    env: &Env,
    investor: &Address,
    risk_score: u32,
) -> Result<InvestorTier, QuickLendXError> {
    validate_risk_score(risk_score)?;

    if let Some(verification) = InvestorVerificationStorage::get(env, investor) {
        return compute_investor_tier_from_stats(
            verification.total_invested,
            verification.successful_investments,
            verification.defaulted_investments,
            risk_score,
        );
    }

    Ok(InvestorTier::Basic)
}

pub fn compute_investor_tier_from_stats(
    total_invested: i128,
    successful_investments: u32,
    defaulted_investments: u32,
    risk_score: u32,
) -> Result<InvestorTier, QuickLendXError> {
    validate_risk_score(risk_score)?;

    let total_active_or_completed = successful_investments.saturating_add(defaulted_investments);
    let default_rate_pct = if total_active_or_completed > 0 {
        (defaulted_investments as u64)
            .saturating_mul(100)
            .checked_div(total_active_or_completed as u64)
            .unwrap_or(0) as u32
    } else {
        0
    };

    if risk_score <= VIP_RISK_SCORE_MAX
        && total_invested >= VIP_TOTAL_INVESTED_MIN
        && successful_investments >= VIP_SUCCESSFUL_INVESTMENTS_MIN
        && default_rate_pct <= VIP_DEFAULT_RATE_MAX_PCT
    {
        return Ok(InvestorTier::VIP);
    }

    if risk_score <= PLATINUM_RISK_SCORE_MAX
        && total_invested >= PLATINUM_TOTAL_INVESTED_MIN
        && successful_investments >= PLATINUM_SUCCESSFUL_INVESTMENTS_MIN
        && default_rate_pct <= PLATINUM_DEFAULT_RATE_MAX_PCT
    {
        return Ok(InvestorTier::Platinum);
    }

    if risk_score <= GOLD_RISK_SCORE_MAX
        && total_invested >= GOLD_TOTAL_INVESTED_MIN
        && successful_investments >= GOLD_SUCCESSFUL_INVESTMENTS_MIN
        && default_rate_pct <= GOLD_DEFAULT_RATE_MAX_PCT
    {
        return Ok(InvestorTier::Gold);
    }

    if risk_score <= SILVER_RISK_SCORE_MAX
        && total_invested >= SILVER_TOTAL_INVESTED_MIN
        && successful_investments >= SILVER_SUCCESSFUL_INVESTMENTS_MIN
        && default_rate_pct <= SILVER_DEFAULT_RATE_MAX_PCT
    {
        return Ok(InvestorTier::Silver);
    }

    Ok(InvestorTier::Basic)
}

/// Determine risk level based on risk score
pub fn determine_risk_level(risk_score: u32) -> InvestorRiskLevel {
    match risk_score {
        0..=25 => InvestorRiskLevel::Low,
        26..=50 => InvestorRiskLevel::Medium,
        51..=75 => InvestorRiskLevel::High,
        76..=100 => InvestorRiskLevel::VeryHigh,
        _ => InvestorRiskLevel::VeryHigh, // fallback safety
    }
}

/// Calculate investment limit based on tier and risk level
pub fn calculate_investment_limit(
    tier: &InvestorTier,
    risk_level: &InvestorRiskLevel,
    base_limit: i128,
) -> i128 {
    let tier_multiplier = get_tier_multiplier(tier);
    let risk_multiplier = get_risk_multiplier(risk_level);

    let calculated_limit = base_limit.max(0).saturating_mul(tier_multiplier);
    calculated_limit
        .saturating_mul(risk_multiplier)
        .saturating_div(100)
}

fn get_tier_multiplier(tier: &InvestorTier) -> i128 {
    match tier {
        InvestorTier::VIP => 10,
        InvestorTier::Platinum => 5,
        InvestorTier::Gold => 3,
        InvestorTier::Silver => 2,
        InvestorTier::Basic => 1,
    }
}

fn get_risk_multiplier(risk_level: &InvestorRiskLevel) -> i128 {
    match risk_level {
        InvestorRiskLevel::Low => 100,     // 100% of calculated limit
        InvestorRiskLevel::Medium => 75,   // 75% of calculated limit
        InvestorRiskLevel::High => 50,     // 50% of calculated limit
        InvestorRiskLevel::VeryHigh => 25, // 25% of calculated limit
    }
}

pub(super) fn recover_base_limit_from_current_limit(
    current_limit: i128,
    tier: &InvestorTier,
    risk_level: &InvestorRiskLevel,
) -> i128 {
    let tier_multiplier = get_tier_multiplier(tier);
    let risk_multiplier = get_risk_multiplier(risk_level);
    let combined_multiplier = tier_multiplier.saturating_mul(risk_multiplier);
    if combined_multiplier <= 0 {
        return current_limit.max(0);
    }

    // Ceiling division avoids gradually shrinking the recovered base from integer truncation.
    current_limit
        .max(0)
        .saturating_mul(100)
        .saturating_add(combined_multiplier - 1)
        .saturating_div(combined_multiplier)
}

/// Enforce minimum investment amount based on tier.
/// Higher tiers can still invest small amounts, but minimums prevent dust bids.
pub fn require_tier_min_investment_amount(
    tier: &InvestorTier,
    amount: i128,
) -> Result<(), QuickLendXError> {
    let min_amount = match tier {
        InvestorTier::Basic => 100,
        InvestorTier::Silver => 200,
        InvestorTier::Gold => 300,
        InvestorTier::Platinum => 400,
        InvestorTier::VIP => 500,
    };
    if amount < min_amount {
        return Err(QuickLendXError::BidBelowTierMinimum);
    }
    Ok(())
}

/// Recompute investor tier and investment limit from tracked performance counters.
///
/// This function keeps the tier assignment deterministic and idempotent by
/// recomputing tier from `total_invested`, `successful_investments`,
/// `defaulted_investments`, and `risk_score`, then updating the stored record.
/// It preserves the investor's approved base limit and applies the new tier/risk
/// multipliers to derive the new investment limit.
pub fn recompute_investor_tier(
    env: &Env,
    admin: &Address,
    investor: &Address,
) -> Result<(), QuickLendXError> {
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
    verification.compliance_notes =
        Some(String::from_str(env, "Investor tier recomputed by admin"));

    InvestorVerificationStorage::update(env, &verification);
    Ok(())
}
