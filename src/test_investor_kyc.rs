/// # Investor KYC Verification Guard Tests
///
/// Comprehensive test coverage for investor-actor verification guards,
/// investment limit computation, tier/risk mechanics, and bid placement
/// guards in the centralized verification module.
///
/// ## Coverage Areas
///
/// - Guard denial for every non-verified status (negative tests)
/// - Guard approval for verified investors within limits
/// - Investment limit computation across all tier/risk combinations
/// - Per-investment risk caps (High, VeryHigh)
/// - Bid placement guard (combines status + limit + cap)
/// - Tier qualification logic
/// - Zero-amount and overflow edge cases

use crate::verification::*;

// ─────────────────────────────────────────────────────────────────────────────
// Guard: guard_investment_action — status checks
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_investor_guard_verified_within_limit_passes() {
    assert!(guard_investment_action(
        Some(VerificationStatus::Verified),
        50_000,
        100_000,
        InvestorTier::Basic,
        RiskLevel::Low,
    ).is_ok());
}

#[test]
fn test_investor_guard_pending_denied() {
    assert_eq!(
        guard_investment_action(
            Some(VerificationStatus::Pending),
            1,
            100_000,
            InvestorTier::Basic,
            RiskLevel::Low,
        ),
        Err(GuardError::VerificationPending)
    );
}

#[test]
fn test_investor_guard_rejected_denied() {
    assert_eq!(
        guard_investment_action(
            Some(VerificationStatus::Rejected),
            1,
            100_000,
            InvestorTier::Basic,
            RiskLevel::Low,
        ),
        Err(GuardError::VerificationRejected)
    );
}

#[test]
fn test_investor_guard_not_submitted_denied() {
    assert_eq!(
        guard_investment_action(
            None,
            1,
            100_000,
            InvestorTier::Basic,
            RiskLevel::Low,
        ),
        Err(GuardError::NotSubmitted)
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Guard: zero-amount check
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_investor_guard_zero_amount_denied() {
    assert_eq!(
        guard_investment_action(
            Some(VerificationStatus::Verified),
            0,
            100_000,
            InvestorTier::Basic,
            RiskLevel::Low,
        ),
        Err(GuardError::ZeroAmount)
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Guard: investment limit enforcement
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_investor_guard_amount_at_limit_passes() {
    // Basic + Low = 100_000 * 1 * 100% = 100_000
    assert!(guard_investment_action(
        Some(VerificationStatus::Verified),
        100_000,
        100_000,
        InvestorTier::Basic,
        RiskLevel::Low,
    ).is_ok());
}

#[test]
fn test_investor_guard_amount_exceeds_limit_denied() {
    assert_eq!(
        guard_investment_action(
            Some(VerificationStatus::Verified),
            100_001,
            100_000,
            InvestorTier::Basic,
            RiskLevel::Low,
        ),
        Err(GuardError::InvestmentLimitExceeded {
            requested: 100_001,
            effective_limit: 100_000,
        })
    );
}

#[test]
fn test_investor_guard_silver_medium_limit() {
    // Silver + Medium = 100_000 * 2 * 75% = 150_000
    assert!(guard_investment_action(
        Some(VerificationStatus::Verified),
        150_000,
        100_000,
        InvestorTier::Silver,
        RiskLevel::Medium,
    ).is_ok());

    assert_eq!(
        guard_investment_action(
            Some(VerificationStatus::Verified),
            150_001,
            100_000,
            InvestorTier::Silver,
            RiskLevel::Medium,
        ),
        Err(GuardError::InvestmentLimitExceeded {
            requested: 150_001,
            effective_limit: 150_000,
        })
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Guard: per-investment risk cap enforcement
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_investor_guard_high_risk_cap_enforced() {
    // Gold + High = 100_000 * 3 * 50% = 150_000 (effective limit)
    // But High risk has per-investment cap of 50_000
    assert!(guard_investment_action(
        Some(VerificationStatus::Verified),
        50_000,
        100_000,
        InvestorTier::Gold,
        RiskLevel::High,
    ).is_ok());

    assert_eq!(
        guard_investment_action(
            Some(VerificationStatus::Verified),
            50_001,
            100_000,
            InvestorTier::Gold,
            RiskLevel::High,
        ),
        Err(GuardError::PerInvestmentCapExceeded {
            requested: 50_001,
            cap: HIGH_RISK_PER_INVESTMENT_CAP,
        })
    );
}

#[test]
fn test_investor_guard_very_high_risk_cap_enforced() {
    // Vip + VeryHigh = 100_000 * 10 * 25% = 250_000 (effective limit)
    // But VeryHigh risk has per-investment cap of 10_000
    assert!(guard_investment_action(
        Some(VerificationStatus::Verified),
        10_000,
        100_000,
        InvestorTier::Vip,
        RiskLevel::VeryHigh,
    ).is_ok());

    assert_eq!(
        guard_investment_action(
            Some(VerificationStatus::Verified),
            10_001,
            100_000,
            InvestorTier::Vip,
            RiskLevel::VeryHigh,
        ),
        Err(GuardError::PerInvestmentCapExceeded {
            requested: 10_001,
            cap: VERY_HIGH_RISK_PER_INVESTMENT_CAP,
        })
    );
}

#[test]
fn test_investor_guard_low_risk_no_cap() {
    // Low risk has no per-investment cap; only the effective limit applies
    // Basic + Low = 100_000
    assert!(guard_investment_action(
        Some(VerificationStatus::Verified),
        100_000,
        100_000,
        InvestorTier::Basic,
        RiskLevel::Low,
    ).is_ok());
}

#[test]
fn test_investor_guard_medium_risk_no_cap() {
    // Medium risk has no per-investment cap
    // Basic + Medium = 100_000 * 1 * 75% = 75_000
    assert!(guard_investment_action(
        Some(VerificationStatus::Verified),
        75_000,
        100_000,
        InvestorTier::Basic,
        RiskLevel::Medium,
    ).is_ok());
}

// ─────────────────────────────────────────────────────────────────────────────
// Guard: guard_bid_placement (alias for guard_investment_action)
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_bid_placement_verified_within_limit_passes() {
    assert!(guard_bid_placement(
        Some(VerificationStatus::Verified),
        50_000,
        100_000,
        InvestorTier::Basic,
        RiskLevel::Low,
    ).is_ok());
}

#[test]
fn test_bid_placement_pending_denied() {
    assert_eq!(
        guard_bid_placement(
            Some(VerificationStatus::Pending),
            1,
            100_000,
            InvestorTier::Basic,
            RiskLevel::Low,
        ),
        Err(GuardError::VerificationPending)
    );
}

#[test]
fn test_bid_placement_rejected_denied() {
    assert_eq!(
        guard_bid_placement(
            Some(VerificationStatus::Rejected),
            1,
            100_000,
            InvestorTier::Basic,
            RiskLevel::Low,
        ),
        Err(GuardError::VerificationRejected)
    );
}

#[test]
fn test_bid_placement_not_submitted_denied() {
    assert_eq!(
        guard_bid_placement(
            None,
            1,
            100_000,
            InvestorTier::Basic,
            RiskLevel::Low,
        ),
        Err(GuardError::NotSubmitted)
    );
}

#[test]
fn test_bid_placement_exceeds_limit_denied() {
    assert_eq!(
        guard_bid_placement(
            Some(VerificationStatus::Verified),
            100_001,
            100_000,
            InvestorTier::Basic,
            RiskLevel::Low,
        ),
        Err(GuardError::InvestmentLimitExceeded {
            requested: 100_001,
            effective_limit: 100_000,
        })
    );
}

#[test]
fn test_bid_placement_zero_amount_denied() {
    assert_eq!(
        guard_bid_placement(
            Some(VerificationStatus::Verified),
            0,
            100_000,
            InvestorTier::Basic,
            RiskLevel::Low,
        ),
        Err(GuardError::ZeroAmount)
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// compute_effective_limit — all tier x risk combinations
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_effective_limit_all_tier_risk_combinations() {
    let base = 100_000u128;

    // Expected: base * tier_mult * risk_bps / 10_000
    let expected: Vec<(InvestorTier, RiskLevel, u128)> = vec![
        (InvestorTier::Basic, RiskLevel::Low, 100_000),       // 1 * 100%
        (InvestorTier::Basic, RiskLevel::Medium, 75_000),     // 1 * 75%
        (InvestorTier::Basic, RiskLevel::High, 50_000),       // 1 * 50%
        (InvestorTier::Basic, RiskLevel::VeryHigh, 25_000),   // 1 * 25%
        (InvestorTier::Silver, RiskLevel::Low, 200_000),      // 2 * 100%
        (InvestorTier::Silver, RiskLevel::Medium, 150_000),   // 2 * 75%
        (InvestorTier::Silver, RiskLevel::High, 100_000),     // 2 * 50%
        (InvestorTier::Silver, RiskLevel::VeryHigh, 50_000),  // 2 * 25%
        (InvestorTier::Gold, RiskLevel::Low, 300_000),        // 3 * 100%
        (InvestorTier::Gold, RiskLevel::Medium, 225_000),     // 3 * 75%
        (InvestorTier::Gold, RiskLevel::High, 150_000),       // 3 * 50%
        (InvestorTier::Gold, RiskLevel::VeryHigh, 75_000),    // 3 * 25%
        (InvestorTier::Platinum, RiskLevel::Low, 500_000),    // 5 * 100%
        (InvestorTier::Platinum, RiskLevel::Medium, 375_000), // 5 * 75%
        (InvestorTier::Platinum, RiskLevel::High, 250_000),   // 5 * 50%
        (InvestorTier::Platinum, RiskLevel::VeryHigh, 125_000), // 5 * 25%
        (InvestorTier::Vip, RiskLevel::Low, 1_000_000),      // 10 * 100%
        (InvestorTier::Vip, RiskLevel::Medium, 750_000),     // 10 * 75%
        (InvestorTier::Vip, RiskLevel::High, 500_000),       // 10 * 50%
        (InvestorTier::Vip, RiskLevel::VeryHigh, 250_000),   // 10 * 25%
    ];

    for (tier, risk, expected_limit) in expected {
        let actual = compute_effective_limit(base, tier, risk);
        assert_eq!(
            actual,
            Some(expected_limit),
            "tier={:?} risk={:?}: expected {}, got {:?}",
            tier,
            risk,
            expected_limit,
            actual
        );
    }
}

#[test]
fn test_effective_limit_zero_base_rejected() {
    assert!(compute_effective_limit(0, InvestorTier::Vip, RiskLevel::Low).is_none());
}

#[test]
fn test_effective_limit_over_max_base_rejected() {
    assert!(
        compute_effective_limit(MAX_BASE_LIMIT + 1, InvestorTier::Basic, RiskLevel::Low).is_none()
    );
}

#[test]
fn test_effective_limit_max_base_basic_low_succeeds() {
    assert_eq!(
        compute_effective_limit(MAX_BASE_LIMIT, InvestorTier::Basic, RiskLevel::Low),
        Some(MAX_BASE_LIMIT)
    );
}

#[test]
fn test_effective_limit_min_base_succeeds() {
    // 1 * 1 * 10_000 / 10_000 = 1
    assert_eq!(
        compute_effective_limit(1, InvestorTier::Basic, RiskLevel::Low),
        Some(1)
    );
}

#[test]
fn test_effective_limit_min_base_very_high_risk_truncates_to_zero() {
    // 1 * 1 * 2_500 / 10_000 = 0 (integer division truncation)
    assert_eq!(
        compute_effective_limit(1, InvestorTier::Basic, RiskLevel::VeryHigh),
        Some(0)
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// compute_tier — tier qualification
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_tier_basic_default() {
    assert_eq!(compute_tier(0, 0), InvestorTier::Basic);
    assert_eq!(compute_tier(5_000, 1), InvestorTier::Basic);
    assert_eq!(compute_tier(10_000, 3), InvestorTier::Basic); // boundary: not >
}

#[test]
fn test_tier_silver_thresholds() {
    assert_eq!(compute_tier(10_001, 4), InvestorTier::Silver);
    assert_eq!(compute_tier(100_000, 4), InvestorTier::Silver);
    // High count but low invested: not Silver
    assert_eq!(compute_tier(10_000, 100), InvestorTier::Basic);
    // High invested but low count: not Silver
    assert_eq!(compute_tier(50_000, 3), InvestorTier::Basic);
}

#[test]
fn test_tier_gold_thresholds() {
    assert_eq!(compute_tier(100_001, 11), InvestorTier::Gold);
    assert_eq!(compute_tier(1_000_000, 11), InvestorTier::Gold);
    // At boundary: not >
    assert_eq!(compute_tier(100_000, 10), InvestorTier::Silver);
}

#[test]
fn test_tier_platinum_thresholds() {
    assert_eq!(compute_tier(1_000_001, 21), InvestorTier::Platinum);
    assert_eq!(compute_tier(5_000_000, 21), InvestorTier::Platinum);
    // At boundary: not >
    assert_eq!(compute_tier(1_000_000, 20), InvestorTier::Gold);
}

#[test]
fn test_tier_vip_thresholds() {
    assert_eq!(compute_tier(5_000_001, 51), InvestorTier::Vip);
    assert_eq!(compute_tier(u128::MAX, u32::MAX), InvestorTier::Vip);
    // At boundary: not >
    assert_eq!(compute_tier(5_000_000, 50), InvestorTier::Platinum);
}

#[test]
fn test_tier_requires_both_thresholds() {
    // All combinations where one threshold is met but not the other
    assert_eq!(compute_tier(10_000_000, 0), InvestorTier::Basic);
    assert_eq!(compute_tier(10_000_000, 3), InvestorTier::Basic);
    assert_eq!(compute_tier(0, 1_000), InvestorTier::Basic);
    assert_eq!(compute_tier(1, u32::MAX), InvestorTier::Basic);

    // Silver invested, but not enough count for Gold
    assert_eq!(compute_tier(500_000, 10), InvestorTier::Silver);
}

// ─────────────────────────────────────────────────────────────────────────────
// risk_level_from_score — boundary and invalid values
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_risk_score_all_boundaries() {
    // Low: 0-25
    for score in [0, 1, 12, 24, 25] {
        assert_eq!(
            risk_level_from_score(score),
            Some(RiskLevel::Low),
            "score {} should be Low",
            score
        );
    }
    // Medium: 26-50
    for score in [26, 27, 38, 49, 50] {
        assert_eq!(
            risk_level_from_score(score),
            Some(RiskLevel::Medium),
            "score {} should be Medium",
            score
        );
    }
    // High: 51-75
    for score in [51, 52, 63, 74, 75] {
        assert_eq!(
            risk_level_from_score(score),
            Some(RiskLevel::High),
            "score {} should be High",
            score
        );
    }
    // VeryHigh: 76-100
    for score in [76, 77, 88, 99, 100] {
        assert_eq!(
            risk_level_from_score(score),
            Some(RiskLevel::VeryHigh),
            "score {} should be VeryHigh",
            score
        );
    }
}

#[test]
fn test_risk_score_over_max_rejected() {
    assert!(risk_level_from_score(101).is_none());
    assert!(risk_level_from_score(200).is_none());
    assert!(risk_level_from_score(u32::MAX).is_none());
}

// ─────────────────────────────────────────────────────────────────────────────
// Full investor lifecycle
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_full_investor_lifecycle_submit_reject_resubmit_verify_bid() {
    let base_limit = 100_000u128;

    // Step 1: No record — cannot bid
    assert_eq!(
        guard_bid_placement(None, 1, base_limit, InvestorTier::Basic, RiskLevel::Low),
        Err(GuardError::NotSubmitted)
    );

    // Step 2: Submit KYC — now Pending
    let status = VerificationStatus::Pending;
    assert_eq!(
        guard_bid_placement(Some(status), 1, base_limit, InvestorTier::Basic, RiskLevel::Low),
        Err(GuardError::VerificationPending)
    );

    // Step 3: Admin rejects
    assert!(validate_transition(status, VerificationStatus::Rejected).is_ok());
    let status = VerificationStatus::Rejected;
    assert_eq!(
        guard_bid_placement(Some(status), 1, base_limit, InvestorTier::Basic, RiskLevel::Low),
        Err(GuardError::VerificationRejected)
    );

    // Step 4: Resubmit
    assert!(validate_transition(status, VerificationStatus::Pending).is_ok());
    let status = VerificationStatus::Pending;

    // Step 5: Admin verifies
    assert!(validate_transition(status, VerificationStatus::Verified).is_ok());
    let status = VerificationStatus::Verified;

    // Step 6: Can now bid within limits
    assert!(guard_bid_placement(
        Some(status), 50_000, base_limit, InvestorTier::Basic, RiskLevel::Low
    ).is_ok());

    // Step 7: Cannot bid over limit
    assert!(guard_bid_placement(
        Some(status), 100_001, base_limit, InvestorTier::Basic, RiskLevel::Low
    ).is_err());
}

// ─────────────────────────────────────────────────────────────────────────────
// Tier advancement affects limits
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_tier_advancement_increases_limit() {
    let base_limit = 100_000u128;
    let status = Some(VerificationStatus::Verified);

    // Basic: limit = 100_000
    assert!(guard_investment_action(status, 100_000, base_limit, InvestorTier::Basic, RiskLevel::Low).is_ok());
    assert!(guard_investment_action(status, 100_001, base_limit, InvestorTier::Basic, RiskLevel::Low).is_err());

    // Silver: limit = 200_000
    assert!(guard_investment_action(status, 200_000, base_limit, InvestorTier::Silver, RiskLevel::Low).is_ok());
    assert!(guard_investment_action(status, 200_001, base_limit, InvestorTier::Silver, RiskLevel::Low).is_err());

    // Gold: limit = 300_000
    assert!(guard_investment_action(status, 300_000, base_limit, InvestorTier::Gold, RiskLevel::Low).is_ok());
    assert!(guard_investment_action(status, 300_001, base_limit, InvestorTier::Gold, RiskLevel::Low).is_err());

    // Platinum: limit = 500_000
    assert!(guard_investment_action(status, 500_000, base_limit, InvestorTier::Platinum, RiskLevel::Low).is_ok());
    assert!(guard_investment_action(status, 500_001, base_limit, InvestorTier::Platinum, RiskLevel::Low).is_err());

    // Vip: limit = 1_000_000
    assert!(guard_investment_action(status, 1_000_000, base_limit, InvestorTier::Vip, RiskLevel::Low).is_ok());
    assert!(guard_investment_action(status, 1_000_001, base_limit, InvestorTier::Vip, RiskLevel::Low).is_err());
}

// ─────────────────────────────────────────────────────────────────────────────
// Risk level reduces limits
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_higher_risk_reduces_effective_limit() {
    let base = 100_000u128;
    let tier = InvestorTier::Basic;

    let low = compute_effective_limit(base, tier, RiskLevel::Low).unwrap();
    let med = compute_effective_limit(base, tier, RiskLevel::Medium).unwrap();
    let high = compute_effective_limit(base, tier, RiskLevel::High).unwrap();
    let vhigh = compute_effective_limit(base, tier, RiskLevel::VeryHigh).unwrap();

    assert!(low > med, "Low limit should exceed Medium");
    assert!(med > high, "Medium limit should exceed High");
    assert!(high > vhigh, "High limit should exceed VeryHigh");
}

// ─────────────────────────────────────────────────────────────────────────────
// Error priority: status check comes before amount check
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_error_priority_status_before_amount() {
    // Pending with zero amount — should get VerificationPending, not ZeroAmount
    assert_eq!(
        guard_investment_action(
            Some(VerificationStatus::Pending),
            0,
            100_000,
            InvestorTier::Basic,
            RiskLevel::Low,
        ),
        Err(GuardError::VerificationPending)
    );

    // Not submitted with oversized amount — should get NotSubmitted
    assert_eq!(
        guard_investment_action(
            None,
            u128::MAX,
            100_000,
            InvestorTier::Basic,
            RiskLevel::Low,
        ),
        Err(GuardError::NotSubmitted)
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Arithmetic overflow protection
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_guard_arithmetic_overflow_returns_error() {
    // base_limit that causes overflow in Vip * 10 * 10_000
    assert_eq!(
        guard_investment_action(
            Some(VerificationStatus::Verified),
            1,
            u128::MAX,
            InvestorTier::Vip,
            RiskLevel::Low,
        ),
        Err(GuardError::ArithmeticOverflow)
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Deny-by-default property for investor guards
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_investor_deny_by_default_property() {
    let base = 100_000u128;
    let statuses: Vec<Option<VerificationStatus>> = vec![
        None,
        Some(VerificationStatus::Pending),
        Some(VerificationStatus::Rejected),
        Some(VerificationStatus::Verified),
    ];

    let denied = statuses
        .iter()
        .filter(|s| {
            guard_investment_action(**s, 1, base, InvestorTier::Basic, RiskLevel::Low).is_err()
        })
        .count();

    assert_eq!(denied, 3, "exactly 3 statuses should be denied");
}

// ─────────────────────────────────────────────────────────────────────────────
// Risk cap is checked after limit (both can fail independently)
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_limit_exceeded_takes_precedence_over_cap() {
    // Vip + VeryHigh: effective_limit = 100_000 * 10 * 25% = 250_000
    // Per-investment cap = 10_000
    // Amount = 300_000 > 250_000 → limit exceeded (checked first)
    assert_eq!(
        guard_investment_action(
            Some(VerificationStatus::Verified),
            300_000,
            100_000,
            InvestorTier::Vip,
            RiskLevel::VeryHigh,
        ),
        Err(GuardError::InvestmentLimitExceeded {
            requested: 300_000,
            effective_limit: 250_000,
        })
    );
}

#[test]
fn test_cap_exceeded_when_under_limit() {
    // Vip + VeryHigh: effective_limit = 250_000
    // Per-investment cap = 10_000
    // Amount = 20_000 < 250_000 but > 10_000 → cap exceeded
    assert_eq!(
        guard_investment_action(
            Some(VerificationStatus::Verified),
            20_000,
            100_000,
            InvestorTier::Vip,
            RiskLevel::VeryHigh,
        ),
        Err(GuardError::PerInvestmentCapExceeded {
            requested: 20_000,
            cap: VERY_HIGH_RISK_PER_INVESTMENT_CAP,
        })
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Edge: effective limit of 0 (truncation from integer division)
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_effective_limit_zero_from_truncation_blocks_all_amounts() {
    // base=1, Basic, VeryHigh: 1 * 1 * 2500 / 10000 = 0
    assert_eq!(
        guard_investment_action(
            Some(VerificationStatus::Verified),
            1,
            1,
            InvestorTier::Basic,
            RiskLevel::VeryHigh,
        ),
        Err(GuardError::InvestmentLimitExceeded {
            requested: 1,
            effective_limit: 0,
        })
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Edge: amount = 1 (minimum valid amount)
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_minimum_amount_passes_with_sufficient_limit() {
    assert!(guard_investment_action(
        Some(VerificationStatus::Verified),
        1,
        100_000,
        InvestorTier::Basic,
        RiskLevel::Low,
    ).is_ok());
}
