/// # Verification Guard Module
///
/// Centralized guard coverage to prevent unverified actors from restricted
/// finance actions in the QuickLendX invoice-financing protocol.
///
/// ## Purpose
///
/// Every privileged operation (invoice upload, bid placement, settlement
/// initiation, escrow release) must pass through a verification gate before
/// execution.  This module provides the **single source of truth** for:
///
/// 1. Actor verification status evaluation
/// 2. State-transition validation (Pending -> Verified / Rejected)
/// 3. Investment-limit computation based on tier and risk level
/// 4. Action-specific guard checks that combine status + limits
///
/// ## Design Principles
///
/// - **Pure functions** — no blockchain or storage dependencies; the caller
///   supplies all inputs.  This keeps the module testable and portable.
/// - **Checked arithmetic** — all limit calculations use `checked_*`
///   operations; overflow returns `None`.
/// - **Deny-by-default** — every guard returns `Err` unless the actor is
///   explicitly `Verified`.  Pending, Rejected, and unknown actors are all
///   blocked.
/// - **Exhaustive error variants** — callers receive a typed error explaining
///   *why* the action was denied, enabling precise audit trails.
///
/// ## Guard Taxonomy
///
/// | Guard                        | Who           | Required Status | Extra Check        |
/// |------------------------------|---------------|-----------------|---------------------|
/// | `guard_invoice_upload`       | Business      | Verified        | —                   |
/// | `guard_bid_placement`        | Investor      | Verified        | amount ≤ limit      |
/// | `guard_settlement_initiation`| Business      | Verified        | —                   |
/// | `guard_escrow_release`       | Business      | Verified        | —                   |
/// | `guard_investment_action`    | Investor      | Verified        | amount ≤ limit      |
// ─────────────────────────────────────────────────────────────────────────────
// Constants
// ─────────────────────────────────────────────────────────────────────────────
/// Basis-point denominator (10_000 = 100%).
pub const BPS_DENOMINATOR: u128 = 10_000;

/// Maximum base investment limit accepted (prevents overflow in tier/risk
/// multiplier arithmetic).  Same ceiling as other modules (10^30).
pub const MAX_BASE_LIMIT: u128 = 1_000_000_000_000_000_000_000_000_000_000; // 10^30

/// Maximum risk score (0–100 scale).
pub const MAX_RISK_SCORE: u32 = 100;

/// Per-investment cap for High-risk investors (50_000 smallest units).
pub const HIGH_RISK_PER_INVESTMENT_CAP: u128 = 50_000;

/// Per-investment cap for VeryHigh-risk investors (10_000 smallest units).
pub const VERY_HIGH_RISK_PER_INVESTMENT_CAP: u128 = 10_000;

/// Maximum rejection reason length in bytes.
pub const MAX_REJECTION_REASON_LENGTH: usize = 512;

/// Maximum KYC data payload length in bytes.
pub const MAX_KYC_DATA_LENGTH: usize = 4_096;

// ─────────────────────────────────────────────────────────────────────────────
// Types
// ─────────────────────────────────────────────────────────────────────────────

/// Verification status shared by both business and investor actors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerificationStatus {
    /// KYC application submitted, awaiting admin review.
    Pending,
    /// Admin-approved; actor may perform restricted actions.
    Verified,
    /// Admin-rejected; actor must resubmit before any restricted action.
    Rejected,
}

/// Investor tier determines a multiplier on the base investment limit.
///
/// Tier advancement is earned through sustained, successful investment
/// activity on the platform.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvestorTier {
    /// Default tier — 1x multiplier.
    Basic,
    /// 2x multiplier — requires > 10_000 invested, > 3 successful.
    Silver,
    /// 3x multiplier — requires > 100_000 invested, > 10 successful.
    Gold,
    /// 5x multiplier — requires > 1_000_000 invested, > 20 successful.
    Platinum,
    /// 10x multiplier — requires > 5_000_000 invested, > 50 successful.
    Vip,
}

/// Risk classification derived from the investor's risk score (0–100).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RiskLevel {
    /// Score 0–25: full limit available.
    Low,
    /// Score 26–50: 75% of limit.
    Medium,
    /// Score 51–75: 50% of limit, per-investment cap 50_000.
    High,
    /// Score 76–100: 25% of limit, per-investment cap 10_000.
    VeryHigh,
}

/// Typed error returned when a guard check fails.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GuardError {
    /// Actor has not submitted KYC at all.
    NotSubmitted,
    /// KYC application is still pending admin review.
    VerificationPending,
    /// KYC application was rejected.
    VerificationRejected,
    /// Bid / investment amount exceeds the investor's computed limit.
    InvestmentLimitExceeded {
        requested: u128,
        effective_limit: u128,
    },
    /// Bid / investment amount exceeds the per-investment risk cap.
    PerInvestmentCapExceeded { requested: u128, cap: u128 },
    /// Zero investment amount is not permitted.
    ZeroAmount,
    /// An arithmetic overflow occurred during limit computation.
    ArithmeticOverflow,
}

/// Typed error returned when a state transition is invalid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransitionError {
    /// The requested transition is not allowed by the protocol.
    InvalidTransition {
        from: VerificationStatus,
        to: VerificationStatus,
    },
    /// Cannot transition from Verified to any other status.
    AlreadyVerified,
    /// A Pending actor cannot re-submit (already pending).
    AlreadyPending,
    /// Rejection reason exceeds the maximum length.
    ReasonTooLong { length: usize, max: usize },
    /// Rejection reason must not be empty.
    ReasonEmpty,
    /// KYC data exceeds the maximum payload length.
    KycDataTooLong { length: usize, max: usize },
    /// KYC data must not be empty.
    KycDataEmpty,
}

// ─────────────────────────────────────────────────────────────────────────────
// Tier / risk helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Returns the multiplier for a given investor tier.
///
/// | Tier     | Multiplier |
/// |----------|-----------|
/// | Basic    | 1         |
/// | Silver   | 2         |
/// | Gold     | 3         |
/// | Platinum | 5         |
/// | Vip      | 10        |
pub fn tier_multiplier(tier: InvestorTier) -> u128 {
    match tier {
        InvestorTier::Basic => 1,
        InvestorTier::Silver => 2,
        InvestorTier::Gold => 3,
        InvestorTier::Platinum => 5,
        InvestorTier::Vip => 10,
    }
}

/// Derives the `RiskLevel` from a numeric risk score (0–100).
///
/// Returns `None` if `score > MAX_RISK_SCORE`.
pub fn risk_level_from_score(score: u32) -> Option<RiskLevel> {
    if score > MAX_RISK_SCORE {
        return None;
    }
    Some(match score {
        0..=25 => RiskLevel::Low,
        26..=50 => RiskLevel::Medium,
        51..=75 => RiskLevel::High,
        _ => RiskLevel::VeryHigh,
    })
}

/// Returns the risk-based limit multiplier in basis points.
///
/// | Risk Level | Multiplier bps | Effective % |
/// |------------|---------------|-------------|
/// | Low        | 10_000        | 100%        |
/// | Medium     | 7_500         | 75%         |
/// | High       | 5_000         | 50%         |
/// | VeryHigh   | 2_500         | 25%         |
pub fn risk_multiplier_bps(risk: RiskLevel) -> u128 {
    match risk {
        RiskLevel::Low => 10_000,
        RiskLevel::Medium => 7_500,
        RiskLevel::High => 5_000,
        RiskLevel::VeryHigh => 2_500,
    }
}

/// Returns the optional per-investment cap for a risk level.
///
/// `Low` and `Medium` have no per-investment cap (`None`).
/// `High` caps at 50_000, `VeryHigh` at 10_000.
pub fn per_investment_cap(risk: RiskLevel) -> Option<u128> {
    match risk {
        RiskLevel::Low | RiskLevel::Medium => None,
        RiskLevel::High => Some(HIGH_RISK_PER_INVESTMENT_CAP),
        RiskLevel::VeryHigh => Some(VERY_HIGH_RISK_PER_INVESTMENT_CAP),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Investment limit computation
// ─────────────────────────────────────────────────────────────────────────────

/// Computes the effective investment limit for an investor.
///
/// `effective_limit = base_limit * tier_multiplier * risk_multiplier_bps / BPS_DENOMINATOR`
///
/// # Parameters
/// - `base_limit`  — Platform-configured base investment limit.
/// - `tier`        — Investor's current tier.
/// - `risk`        — Investor's current risk level.
///
/// # Returns
/// `Some(limit)` or `None` on overflow / invalid base_limit.
pub fn compute_effective_limit(
    base_limit: u128,
    tier: InvestorTier,
    risk: RiskLevel,
) -> Option<u128> {
    if base_limit == 0 || base_limit > MAX_BASE_LIMIT {
        return None;
    }
    let t_mult = tier_multiplier(tier);
    let r_bps = risk_multiplier_bps(risk);

    base_limit
        .checked_mul(t_mult)?
        .checked_mul(r_bps)?
        .checked_div(BPS_DENOMINATOR)
}

// ─────────────────────────────────────────────────────────────────────────────
// State transition validation
// ─────────────────────────────────────────────────────────────────────────────

/// Validates whether a state transition is allowed.
///
/// ## Allowed Transitions
///
/// | From     | To       | Meaning              |
/// |----------|----------|----------------------|
/// | Pending  | Verified | Admin approves KYC   |
/// | Pending  | Rejected | Admin rejects KYC    |
/// | Rejected | Pending  | Actor resubmits KYC  |
///
/// ## Blocked Transitions
///
/// - `Verified -> *`   — verified is a terminal state.
/// - `Pending -> Pending` — duplicate submission.
/// - `Rejected -> Verified` — must go through Pending first.
/// - `Rejected -> Rejected` — no-op / invalid.
///
/// # Returns
/// `Ok(())` if the transition is valid, or a typed `TransitionError`.
pub fn validate_transition(
    from: VerificationStatus,
    to: VerificationStatus,
) -> Result<(), TransitionError> {
    match (from, to) {
        // Allowed transitions
        (VerificationStatus::Pending, VerificationStatus::Verified) => Ok(()),
        (VerificationStatus::Pending, VerificationStatus::Rejected) => Ok(()),
        (VerificationStatus::Rejected, VerificationStatus::Pending) => Ok(()),

        // Blocked: already verified (terminal state)
        (VerificationStatus::Verified, _) => Err(TransitionError::AlreadyVerified),

        // Blocked: already pending
        (VerificationStatus::Pending, VerificationStatus::Pending) => {
            Err(TransitionError::AlreadyPending)
        }

        // All other transitions are invalid
        (from, to) => Err(TransitionError::InvalidTransition { from, to }),
    }
}

/// Validates a rejection reason string.
///
/// Reasons must be non-empty and within `MAX_REJECTION_REASON_LENGTH` bytes.
pub fn validate_rejection_reason(reason: &str) -> Result<(), TransitionError> {
    if reason.is_empty() {
        return Err(TransitionError::ReasonEmpty);
    }
    if reason.len() > MAX_REJECTION_REASON_LENGTH {
        return Err(TransitionError::ReasonTooLong {
            length: reason.len(),
            max: MAX_REJECTION_REASON_LENGTH,
        });
    }
    Ok(())
}

/// Validates a KYC data payload string.
///
/// KYC data must be non-empty and within `MAX_KYC_DATA_LENGTH` bytes.
pub fn validate_kyc_data(data: &str) -> Result<(), TransitionError> {
    if data.is_empty() {
        return Err(TransitionError::KycDataEmpty);
    }
    if data.len() > MAX_KYC_DATA_LENGTH {
        return Err(TransitionError::KycDataTooLong {
            length: data.len(),
            max: MAX_KYC_DATA_LENGTH,
        });
    }
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Guard functions — deny-by-default
// ─────────────────────────────────────────────────────────────────────────────

/// Checks whether a business actor is allowed to perform a restricted action.
///
/// The actor must be in `Verified` status.  All other states are denied.
///
/// # Parameters
/// - `status` — `Some(status)` if the actor has a KYC record, or `None` if
///   no record exists.
///
/// # Returns
/// `Ok(())` if verified, or a typed `GuardError`.
pub fn guard_business_action(status: Option<VerificationStatus>) -> Result<(), GuardError> {
    match status {
        None => Err(GuardError::NotSubmitted),
        Some(VerificationStatus::Pending) => Err(GuardError::VerificationPending),
        Some(VerificationStatus::Rejected) => Err(GuardError::VerificationRejected),
        Some(VerificationStatus::Verified) => Ok(()),
    }
}

/// Guard: business may upload an invoice.
///
/// Requires `Verified` status.
pub fn guard_invoice_upload(status: Option<VerificationStatus>) -> Result<(), GuardError> {
    guard_business_action(status)
}

/// Guard: business may initiate settlement.
///
/// Requires `Verified` status.
pub fn guard_settlement_initiation(status: Option<VerificationStatus>) -> Result<(), GuardError> {
    guard_business_action(status)
}

/// Guard: business may trigger escrow release.
///
/// Requires `Verified` status.
pub fn guard_escrow_release(status: Option<VerificationStatus>) -> Result<(), GuardError> {
    guard_business_action(status)
}

/// Checks whether an investor is allowed to perform a restricted action
/// that involves a specific investment amount.
///
/// The investor must be `Verified` **and** the requested amount must not
/// exceed their effective investment limit or per-investment risk cap.
///
/// # Parameters
/// - `status`      — Investor's verification status (or `None`).
/// - `amount`      — Requested investment amount.
/// - `base_limit`  — Platform base investment limit.
/// - `tier`        — Investor's tier.
/// - `risk`        — Investor's risk level.
///
/// # Returns
/// `Ok(())` if all checks pass, or a typed `GuardError`.
pub fn guard_investment_action(
    status: Option<VerificationStatus>,
    amount: u128,
    base_limit: u128,
    tier: InvestorTier,
    risk: RiskLevel,
) -> Result<(), GuardError> {
    // Step 1: verification status check
    match status {
        None => return Err(GuardError::NotSubmitted),
        Some(VerificationStatus::Pending) => return Err(GuardError::VerificationPending),
        Some(VerificationStatus::Rejected) => return Err(GuardError::VerificationRejected),
        Some(VerificationStatus::Verified) => {}
    }

    // Step 2: zero-amount check
    if amount == 0 {
        return Err(GuardError::ZeroAmount);
    }

    // Step 3: compute effective limit
    let effective_limit =
        compute_effective_limit(base_limit, tier, risk).ok_or(GuardError::ArithmeticOverflow)?;

    // Step 4: check against effective limit
    if amount > effective_limit {
        return Err(GuardError::InvestmentLimitExceeded {
            requested: amount,
            effective_limit,
        });
    }

    // Step 5: check per-investment risk cap
    if let Some(cap) = per_investment_cap(risk) {
        if amount > cap {
            return Err(GuardError::PerInvestmentCapExceeded {
                requested: amount,
                cap,
            });
        }
    }

    Ok(())
}

/// Guard: investor may place a bid.
///
/// Alias for `guard_investment_action` — bid placement requires verification
/// and the bid amount must be within limits.
pub fn guard_bid_placement(
    status: Option<VerificationStatus>,
    bid_amount: u128,
    base_limit: u128,
    tier: InvestorTier,
    risk: RiskLevel,
) -> Result<(), GuardError> {
    guard_investment_action(status, bid_amount, base_limit, tier, risk)
}

// ─────────────────────────────────────────────────────────────────────────────
// Tier qualification helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Determines the appropriate tier for an investor based on their track record.
///
/// # Parameters
/// - `total_invested`          — Cumulative amount invested (smallest units).
/// - `successful_investments`  — Count of successfully settled investments.
///
/// # Returns
/// The highest tier the investor qualifies for.
pub fn compute_tier(total_invested: u128, successful_investments: u32) -> InvestorTier {
    if total_invested > 5_000_000 && successful_investments > 50 {
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

#[cfg(test)]
mod tests {
    use super::*;

    // ── tier_multiplier ────────────────────────────────────────────────────

    #[test]
    fn test_tier_multiplier_values() {
        assert_eq!(tier_multiplier(InvestorTier::Basic), 1);
        assert_eq!(tier_multiplier(InvestorTier::Silver), 2);
        assert_eq!(tier_multiplier(InvestorTier::Gold), 3);
        assert_eq!(tier_multiplier(InvestorTier::Platinum), 5);
        assert_eq!(tier_multiplier(InvestorTier::Vip), 10);
    }

    // ── risk_level_from_score ──────────────────────────────────────────────

    #[test]
    fn test_risk_level_boundaries() {
        assert_eq!(risk_level_from_score(0), Some(RiskLevel::Low));
        assert_eq!(risk_level_from_score(25), Some(RiskLevel::Low));
        assert_eq!(risk_level_from_score(26), Some(RiskLevel::Medium));
        assert_eq!(risk_level_from_score(50), Some(RiskLevel::Medium));
        assert_eq!(risk_level_from_score(51), Some(RiskLevel::High));
        assert_eq!(risk_level_from_score(75), Some(RiskLevel::High));
        assert_eq!(risk_level_from_score(76), Some(RiskLevel::VeryHigh));
        assert_eq!(risk_level_from_score(100), Some(RiskLevel::VeryHigh));
    }

    #[test]
    fn test_risk_level_exceeds_max_rejected() {
        assert!(risk_level_from_score(101).is_none());
        assert!(risk_level_from_score(u32::MAX).is_none());
    }

    // ── risk_multiplier_bps ────────────────────────────────────────────────

    #[test]
    fn test_risk_multiplier_values() {
        assert_eq!(risk_multiplier_bps(RiskLevel::Low), 10_000);
        assert_eq!(risk_multiplier_bps(RiskLevel::Medium), 7_500);
        assert_eq!(risk_multiplier_bps(RiskLevel::High), 5_000);
        assert_eq!(risk_multiplier_bps(RiskLevel::VeryHigh), 2_500);
    }

    // ── per_investment_cap ─────────────────────────────────────────────────

    #[test]
    fn test_per_investment_cap_values() {
        assert_eq!(per_investment_cap(RiskLevel::Low), None);
        assert_eq!(per_investment_cap(RiskLevel::Medium), None);
        assert_eq!(per_investment_cap(RiskLevel::High), Some(50_000));
        assert_eq!(per_investment_cap(RiskLevel::VeryHigh), Some(10_000));
    }

    // ── compute_effective_limit ────────────────────────────────────────────

    #[test]
    fn test_effective_limit_basic_low_risk() {
        // 100_000 * 1 * 10_000 / 10_000 = 100_000
        assert_eq!(
            compute_effective_limit(100_000, InvestorTier::Basic, RiskLevel::Low),
            Some(100_000)
        );
    }

    #[test]
    fn test_effective_limit_gold_medium_risk() {
        // 100_000 * 3 * 7_500 / 10_000 = 225_000
        assert_eq!(
            compute_effective_limit(100_000, InvestorTier::Gold, RiskLevel::Medium),
            Some(225_000)
        );
    }

    #[test]
    fn test_effective_limit_vip_low_risk() {
        // 100_000 * 10 * 10_000 / 10_000 = 1_000_000
        assert_eq!(
            compute_effective_limit(100_000, InvestorTier::Vip, RiskLevel::Low),
            Some(1_000_000)
        );
    }

    #[test]
    fn test_effective_limit_platinum_very_high_risk() {
        // 100_000 * 5 * 2_500 / 10_000 = 125_000
        assert_eq!(
            compute_effective_limit(100_000, InvestorTier::Platinum, RiskLevel::VeryHigh),
            Some(125_000)
        );
    }

    #[test]
    fn test_effective_limit_zero_base_rejected() {
        assert!(compute_effective_limit(0, InvestorTier::Basic, RiskLevel::Low).is_none());
    }

    #[test]
    fn test_effective_limit_exceeds_max_base_rejected() {
        assert!(
            compute_effective_limit(MAX_BASE_LIMIT + 1, InvestorTier::Basic, RiskLevel::Low)
                .is_none()
        );
    }

    #[test]
    fn test_effective_limit_max_base_succeeds() {
        // MAX_BASE_LIMIT * 1 * 10_000 / 10_000 = MAX_BASE_LIMIT
        assert_eq!(
            compute_effective_limit(MAX_BASE_LIMIT, InvestorTier::Basic, RiskLevel::Low),
            Some(MAX_BASE_LIMIT)
        );
    }

    // ── validate_transition ────────────────────────────────────────────────

    #[test]
    fn test_transition_pending_to_verified() {
        assert!(
            validate_transition(VerificationStatus::Pending, VerificationStatus::Verified).is_ok()
        );
    }

    #[test]
    fn test_transition_pending_to_rejected() {
        assert!(
            validate_transition(VerificationStatus::Pending, VerificationStatus::Rejected).is_ok()
        );
    }

    #[test]
    fn test_transition_rejected_to_pending() {
        assert!(
            validate_transition(VerificationStatus::Rejected, VerificationStatus::Pending).is_ok()
        );
    }

    #[test]
    fn test_transition_verified_to_anything_blocked() {
        assert_eq!(
            validate_transition(VerificationStatus::Verified, VerificationStatus::Pending),
            Err(TransitionError::AlreadyVerified)
        );
        assert_eq!(
            validate_transition(VerificationStatus::Verified, VerificationStatus::Rejected),
            Err(TransitionError::AlreadyVerified)
        );
        assert_eq!(
            validate_transition(VerificationStatus::Verified, VerificationStatus::Verified),
            Err(TransitionError::AlreadyVerified)
        );
    }

    #[test]
    fn test_transition_pending_to_pending_blocked() {
        assert_eq!(
            validate_transition(VerificationStatus::Pending, VerificationStatus::Pending),
            Err(TransitionError::AlreadyPending)
        );
    }

    #[test]
    fn test_transition_rejected_to_verified_blocked() {
        assert_eq!(
            validate_transition(VerificationStatus::Rejected, VerificationStatus::Verified),
            Err(TransitionError::InvalidTransition {
                from: VerificationStatus::Rejected,
                to: VerificationStatus::Verified,
            })
        );
    }

    #[test]
    fn test_transition_rejected_to_rejected_blocked() {
        assert_eq!(
            validate_transition(VerificationStatus::Rejected, VerificationStatus::Rejected),
            Err(TransitionError::InvalidTransition {
                from: VerificationStatus::Rejected,
                to: VerificationStatus::Rejected,
            })
        );
    }

    // ── validate_rejection_reason ──────────────────────────────────────────

    #[test]
    fn test_rejection_reason_valid() {
        assert!(validate_rejection_reason("Incomplete documentation").is_ok());
    }

    #[test]
    fn test_rejection_reason_empty_rejected() {
        assert_eq!(
            validate_rejection_reason(""),
            Err(TransitionError::ReasonEmpty)
        );
    }

    #[test]
    fn test_rejection_reason_too_long_rejected() {
        let long = "x".repeat(MAX_REJECTION_REASON_LENGTH + 1);
        assert_eq!(
            validate_rejection_reason(&long),
            Err(TransitionError::ReasonTooLong {
                length: MAX_REJECTION_REASON_LENGTH + 1,
                max: MAX_REJECTION_REASON_LENGTH,
            })
        );
    }

    #[test]
    fn test_rejection_reason_at_max_length_accepted() {
        let max_len = "x".repeat(MAX_REJECTION_REASON_LENGTH);
        assert!(validate_rejection_reason(&max_len).is_ok());
    }

    // ── validate_kyc_data ──────────────────────────────────────────────────

    #[test]
    fn test_kyc_data_valid() {
        assert!(validate_kyc_data("encrypted-kyc-payload-abc123").is_ok());
    }

    #[test]
    fn test_kyc_data_empty_rejected() {
        assert_eq!(validate_kyc_data(""), Err(TransitionError::KycDataEmpty));
    }

    #[test]
    fn test_kyc_data_too_long_rejected() {
        let long = "y".repeat(MAX_KYC_DATA_LENGTH + 1);
        assert_eq!(
            validate_kyc_data(&long),
            Err(TransitionError::KycDataTooLong {
                length: MAX_KYC_DATA_LENGTH + 1,
                max: MAX_KYC_DATA_LENGTH,
            })
        );
    }

    #[test]
    fn test_kyc_data_at_max_length_accepted() {
        let max_len = "y".repeat(MAX_KYC_DATA_LENGTH);
        assert!(validate_kyc_data(&max_len).is_ok());
    }

    // ── compute_tier ───────────────────────────────────────────────────────

    #[test]
    fn test_compute_tier_basic() {
        assert_eq!(compute_tier(0, 0), InvestorTier::Basic);
        assert_eq!(compute_tier(10_000, 3), InvestorTier::Basic);
    }

    #[test]
    fn test_compute_tier_silver() {
        assert_eq!(compute_tier(10_001, 4), InvestorTier::Silver);
        assert_eq!(compute_tier(100_000, 10), InvestorTier::Silver);
    }

    #[test]
    fn test_compute_tier_gold() {
        assert_eq!(compute_tier(100_001, 11), InvestorTier::Gold);
        assert_eq!(compute_tier(1_000_000, 20), InvestorTier::Gold);
    }

    #[test]
    fn test_compute_tier_platinum() {
        assert_eq!(compute_tier(1_000_001, 21), InvestorTier::Platinum);
        assert_eq!(compute_tier(5_000_000, 50), InvestorTier::Platinum);
    }

    #[test]
    fn test_compute_tier_vip() {
        assert_eq!(compute_tier(5_000_001, 51), InvestorTier::Vip);
        assert_eq!(compute_tier(100_000_000, 1_000), InvestorTier::Vip);
    }

    #[test]
    fn test_compute_tier_requires_both_thresholds() {
        // High invested but low count → stays at lower tier
        assert_eq!(compute_tier(10_000_000, 0), InvestorTier::Basic);
        // Low invested but high count → stays at lower tier
        assert_eq!(compute_tier(1, 1_000), InvestorTier::Basic);
    }
}
