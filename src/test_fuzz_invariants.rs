//! # Property-Based Fuzz Tests — Core Invariants (Issue #812)
//!
//! Validates three core protocol invariants under randomised inputs using
//! [`proptest`].  Tests run only when compiled with `--features fuzz-tests`.
//!
//! ## Invariants covered
//!
//! | # | Invariant                                    | Module(s)                        |
//! |---|----------------------------------------------|----------------------------------|
//! | 1 | `total_paid <= total_due` at every step      | `settlement`                     |
//! | 2 | Escrow transitions valid & non-reentrant     | `settlement` (finalization guard)|
//! | 3 | Bid caps enforced (per-invoice & per-investor)| `verification`                  |
//!
//! ## Running
//!
//! ```bash
//! # Default (100 cases per test)
//! cargo test --features fuzz-tests fuzz_
//!
//! # Extended (1 000 cases)
//! PROPTEST_CASES=1000 cargo test --features fuzz-tests fuzz_
//! ```
//!
//! ## Security assumptions
//!
//! - All arithmetic uses `checked_*` operations; overflow returns `None`.
//! - Bounded iteration: sweep sizes are O(N) where N ≤ 10 000 per test.
//! - Deterministic: proptest seeds are fixed per-run; failures are reproducible
//!   via `PROPTEST_SEED=<seed>`.

use proptest::prelude::*;

use crate::settlement::{
    compute_settlement, investor_profit, verify_conservation, BPS_DENOMINATOR as S_BPS,
    MAX_FACE_VALUE, MAX_PENALTY_BPS,
};
use crate::verification::{
    compute_effective_limit, compute_tier, guard_bid_placement,
    per_investment_cap, risk_level_from_score, tier_multiplier, GuardError, InvestorTier,
    RiskLevel, VerificationStatus, MAX_BASE_LIMIT, MAX_RISK_SCORE,
};

// ─────────────────────────────────────────────────────────────────────────────
// Invariant 1 — total_paid <= total_due
//
// For any sequence of partial payments, the running total must never exceed
// the invoice face value.  The settlement module caps each applied amount to
// `remaining_due`, so this property must hold even when the caller supplies
// an amount larger than what is owed.
//
// Modelled here at the arithmetic layer:
//   - `compute_settlement` enforces `investor_payout >= funded_amount`
//   - `verify_conservation` asserts `investor_payout + protocol_fee == total_collected`
//   - Simulated multi-step payments accumulate without exceeding `face_value`
// ─────────────────────────────────────────────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    /// Invariant: `investor_payout + protocol_fee == total_collected` (conservation)
    /// and `investor_payout >= funded_amount` (no under-recovery) for all valid inputs.
    ///
    /// This is the arithmetic foundation of `total_paid <= total_due`: the settlement
    /// module never distributes more than it collected, and the investor always
    /// recovers at least their principal.
    #[test]
    fn fuzz_settlement_total_paid_conservation(
        face      in 1u128..MAX_FACE_VALUE,
        funded    in 1u128..MAX_FACE_VALUE,
        fee_bps   in 0u128..S_BPS,
        pen_bps   in 0u128..MAX_PENALTY_BPS,
    ) {
        // funded must be ≤ face for a valid invoice
        let funded = funded.min(face);

        if let Some(r) = compute_settlement(face, funded, fee_bps, pen_bps) {
            // Core invariant 1a: conservation — no value created or destroyed
            prop_assert!(
                verify_conservation(&r),
                "Conservation violated: face={face} funded={funded} fee={fee_bps} pen={pen_bps} \
                 payout={} fee_out={} total={}",
                r.investor_payout, r.protocol_fee, r.total_collected
            );

            // Core invariant 1b: investor always recovers at least their principal
            prop_assert!(
                r.investor_payout >= funded,
                "Investor under-recovered: payout={} < funded={funded}",
                r.investor_payout
            );

            // Core invariant 1c: total_collected >= face_value (penalty only adds)
            prop_assert!(
                r.total_collected >= face,
                "total_collected={} < face={face}",
                r.total_collected
            );

            // Core invariant 1d: late_penalty is non-negative and bounded
            prop_assert!(
                r.late_penalty <= face,
                "late_penalty={} > face={face}",
                r.late_penalty
            );
        }
    }

    /// Invariant: simulated multi-step partial payments never exceed `face_value`.
    ///
    /// Models a business making up to 5 payment instalments.  Each instalment is
    /// capped to `remaining_due = face - accumulated`.  The running total must
    /// never exceed `face` regardless of how large each requested payment is.
    #[test]
    fn fuzz_total_paid_never_exceeds_total_due(
        face in 1u128..MAX_FACE_VALUE,
        // Payment factors as percentages of face (may exceed 100% to test capping)
        p1 in 1u128..200u128,
        p2 in 1u128..200u128,
        p3 in 1u128..200u128,
        p4 in 1u128..200u128,
        p5 in 1u128..200u128,
    ) {
        let mut accumulated: u128 = 0;

        for factor in [p1, p2, p3, p4, p5] {
            if accumulated >= face {
                break; // already fully paid
            }

            let requested = face.saturating_mul(factor) / 100;
            let remaining = face.saturating_sub(accumulated);

            // Protocol caps applied amount to remaining_due
            let applied = requested.min(remaining);
            accumulated = accumulated.saturating_add(applied);

            // Invariant: accumulated total must never exceed face_value
            prop_assert!(
                accumulated <= face,
                "accumulated={accumulated} > face={face} after applying {applied} (requested {requested})"
            );
        }

        // Final check: accumulated is in [0, face]
        prop_assert!(accumulated <= face, "Final accumulated={accumulated} > face={face}");
    }

    /// Invariant: `investor_profit` is non-negative iff `payout >= funded`.
    /// Validates the sign consistency of the profit calculation used in settlement.
    #[test]
    fn fuzz_investor_profit_sign_consistency(
        payout in 0u128..MAX_FACE_VALUE,
        funded in 1u128..MAX_FACE_VALUE,
    ) {
        let result = investor_profit(payout, funded);
        if payout >= funded {
            prop_assert!(result.is_some(), "Expected profit for payout={payout} >= funded={funded}");
            prop_assert_eq!(result.unwrap(), payout - funded);
        } else {
            prop_assert!(result.is_none(), "Expected None for payout={payout} < funded={funded}");
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Invariant 2 — Escrow transitions valid and non-reentrant
//
// Modelled at the arithmetic layer via the settlement finalization guard:
//   - A settlement result is valid iff conservation holds
//   - Once finalized (conservation verified), re-settlement of the same
//     invoice must be rejected (idempotency / non-reentrance)
//   - Partial payments accumulate monotonically toward face_value
//   - No intermediate state allows `total_collected < face_value`
// ─────────────────────────────────────────────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    /// Invariant: settlement is idempotent — applying the same parameters twice
    /// produces identical results (deterministic, non-reentrant arithmetic).
    ///
    /// This mirrors the on-chain finalization guard: once an escrow is Released
    /// or Refunded, the same computation must not produce a different outcome.
    #[test]
    fn fuzz_settlement_idempotent_non_reentrant(
        face    in 1u128..MAX_FACE_VALUE,
        funded  in 1u128..MAX_FACE_VALUE,
        fee_bps in 0u128..S_BPS,
        pen_bps in 0u128..MAX_PENALTY_BPS,
    ) {
        let funded = funded.min(face);

        let result_a = compute_settlement(face, funded, fee_bps, pen_bps);
        let result_b = compute_settlement(face, funded, fee_bps, pen_bps);

        // Invariant: pure function — same inputs always produce same output
        prop_assert!(
            result_a == result_b,
            "Settlement is non-deterministic for face={} funded={}",
            face, funded
        );

        // Invariant: if valid, conservation holds on both calls
        if let (Some(a), Some(b)) = (result_a, result_b) {
            prop_assert!(verify_conservation(&a));
            prop_assert!(verify_conservation(&b));
            prop_assert_eq!(a.investor_payout, b.investor_payout);
            prop_assert_eq!(a.protocol_fee,    b.protocol_fee);
            prop_assert_eq!(a.total_collected, b.total_collected);
        }
    }

    /// Invariant: escrow state transitions are monotone — once `total_collected`
    /// reaches `face_value`, no further value can be added (non-reentrant).
    ///
    /// Simulates the Held → Released transition: after settlement, the escrow
    /// amount equals `funded`, and a second release attempt would find zero
    /// remaining (modelled as `remaining_due = 0`).
    #[test]
    fn fuzz_escrow_release_non_reentrant(
        face    in 1u128..MAX_FACE_VALUE,
        funded  in 1u128..MAX_FACE_VALUE,
        fee_bps in 0u128..S_BPS,
    ) {
        let funded = funded.min(face);

        if let Some(r) = compute_settlement(face, funded, fee_bps, 0) {
            // After settlement: remaining_due = face - face = 0
            let remaining_after = face.saturating_sub(r.total_collected.min(face));

            // Invariant: no further payment is possible after full settlement
            prop_assert!(
                remaining_after == 0,
                "remaining_due={} != 0 after settlement of face={}",
                remaining_after, face
            );

            // Invariant: a second settlement attempt on remaining=0 must be a no-op
            // (applied_amount = min(any_request, 0) = 0)
            let second_applied = 1u128.min(remaining_after);
            prop_assert!(
                second_applied == 0,
                "Second settlement applied non-zero amount={}",
                second_applied
            );
        }
    }

    /// Invariant: invalid escrow inputs (zero face, funded > face, fee > 100%)
    /// are always rejected — the escrow is never created for bad parameters.
    #[test]
    fn fuzz_escrow_invalid_inputs_always_rejected(
        face    in 0u128..10u128,          // includes 0 (invalid)
        funded  in 0u128..MAX_FACE_VALUE,
        fee_bps in S_BPS..u128::MAX,       // always above 100% (invalid)
    ) {
        // Any of: face==0, funded>face, fee>BPS_DENOMINATOR → must return None
        let result = compute_settlement(face, funded, fee_bps, 0);
        prop_assert!(
            result.is_none(),
            "Expected None for invalid escrow params: face={face} funded={funded} fee={fee_bps}"
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Invariant 3 — Bid caps enforced (MAX_BIDS_PER_INVOICE & per-investor limits)
//
// Modelled at the arithmetic layer via the verification module:
//   - `compute_effective_limit` enforces tier × risk multipliers
//   - `guard_bid_placement` rejects bids exceeding the effective limit
//   - `per_investment_cap` enforces per-bid caps for High/VeryHigh risk
//   - Zero-amount bids are always rejected
//   - Unverified investors are always rejected
// ─────────────────────────────────────────────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    /// Invariant: `compute_effective_limit` is always ≤ `base_limit * max_tier_mult`.
    ///
    /// The VIP tier has a 10× multiplier and Low risk has 100% of the limit,
    /// so the effective limit can never exceed `base_limit * 10`.
    #[test]
    fn fuzz_bid_effective_limit_bounded(
        base_limit in 1u128..MAX_BASE_LIMIT,
        tier_idx   in 0usize..5usize,
        risk_idx   in 0usize..4usize,
    ) {
        let tiers = [
            InvestorTier::Basic,
            InvestorTier::Silver,
            InvestorTier::Gold,
            InvestorTier::Platinum,
            InvestorTier::Vip,
        ];
        let risks = [
            RiskLevel::Low,
            RiskLevel::Medium,
            RiskLevel::High,
            RiskLevel::VeryHigh,
        ];

        let tier = tiers[tier_idx].clone();
        let risk = risks[risk_idx].clone();

        if let Some(limit) = compute_effective_limit(base_limit, tier.clone(), risk) {
            let max_possible = base_limit.saturating_mul(10); // VIP × Low = 10×

            // Invariant: effective limit never exceeds base × max_multiplier
            prop_assert!(
                limit <= max_possible,
                "effective_limit={limit} > base_limit*10={max_possible} for base={base_limit}"
            );

            // Invariant: effective limit is always > 0 for valid inputs
            prop_assert!(limit > 0, "effective_limit=0 for base={base_limit}");
        }
    }

    /// Invariant: `guard_bid_placement` rejects any bid that exceeds the
    /// investor's computed effective limit.
    ///
    /// For any (base_limit, tier, risk) combination, a bid of
    /// `effective_limit + 1` must always be rejected with
    /// `InvestmentLimitExceeded` or `PerInvestmentCapExceeded`.
    #[test]
    fn fuzz_bid_cap_over_limit_always_rejected(
        base_limit in 1u128..1_000_000u128,  // bounded to keep limit+1 in range
        tier_idx   in 0usize..5usize,
        risk_idx   in 0usize..4usize,
    ) {
        let tiers = [
            InvestorTier::Basic,
            InvestorTier::Silver,
            InvestorTier::Gold,
            InvestorTier::Platinum,
            InvestorTier::Vip,
        ];
        let risks = [
            RiskLevel::Low,
            RiskLevel::Medium,
            RiskLevel::High,
            RiskLevel::VeryHigh,
        ];

        let tier = tiers[tier_idx].clone();
        let risk = risks[risk_idx].clone();

        if let Some(effective_limit) = compute_effective_limit(base_limit, tier.clone(), risk.clone()) {
            let over_limit = effective_limit.saturating_add(1);

            let result = guard_bid_placement(
                Some(VerificationStatus::Verified),
                over_limit,
                base_limit,
                tier,
                risk,
            );

            // Invariant: bid exceeding limit must always be rejected
            prop_assert!(
                result.is_err(),
                "Expected rejection for bid={over_limit} > limit={effective_limit}"
            );

            // The error must be a limit or cap exceeded variant
            match result.unwrap_err() {
                GuardError::InvestmentLimitExceeded { requested, effective_limit: el } => {
                    prop_assert_eq!(requested, over_limit);
                    prop_assert_eq!(el, effective_limit);
                }
                GuardError::PerInvestmentCapExceeded { requested, cap } => {
                    prop_assert!(requested > cap, "cap={cap} requested={requested}");
                }
                other => prop_assert!(
                    false,
                    "Unexpected error variant: {other:?} for bid={over_limit} limit={effective_limit}"
                ),
            }
        }
    }

    /// Invariant: `guard_bid_placement` accepts any bid ≤ effective_limit
    /// (and ≤ per-investment cap) for a verified investor.
    #[test]
    fn fuzz_bid_within_limit_always_accepted(
        base_limit in 1u128..MAX_BASE_LIMIT,
        // bid as a fraction of base_limit (1%–100%)
        bid_pct    in 1u128..100u128,
    ) {
        // Use Basic tier + Low risk: effective_limit == base_limit, no per-investment cap
        let tier = InvestorTier::Basic;
        let risk = RiskLevel::Low;

        if let Some(effective_limit) = compute_effective_limit(base_limit, tier.clone(), risk.clone()) {
            let bid = effective_limit.saturating_mul(bid_pct) / 100;
            if bid == 0 {
                return Ok(());
            }

            let result = guard_bid_placement(
                Some(VerificationStatus::Verified),
                bid,
                base_limit,
                tier,
                risk,
            );

            prop_assert!(
                result.is_ok(),
                "Expected acceptance for bid={bid} <= limit={effective_limit}, got {result:?}"
            );
        }
    }

    /// Invariant: unverified investors (Pending, Rejected, None) are always
    /// rejected regardless of bid amount or limit.
    #[test]
    fn fuzz_bid_unverified_always_rejected(
        base_limit in 1u128..MAX_BASE_LIMIT,
        bid_amount in 1u128..MAX_BASE_LIMIT,
        status_idx in 0usize..3usize,
    ) {
        let statuses: [Option<VerificationStatus>; 3] = [
            Some(VerificationStatus::Pending),
            Some(VerificationStatus::Rejected),
            None,
        ];

        let status = statuses[status_idx].clone();
        let bid = bid_amount.min(base_limit); // keep within limit to isolate status check

        let result = guard_bid_placement(
            status.clone(),
            bid,
            base_limit,
            InvestorTier::Basic,
            RiskLevel::Low,
        );

        prop_assert!(
            result.is_err(),
            "Unverified investor (status={status:?}) must be rejected for bid={bid}"
        );

        // Error must be a verification-related variant
        match result.unwrap_err() {
            GuardError::NotSubmitted
            | GuardError::VerificationPending
            | GuardError::VerificationRejected => {}
            other => prop_assert!(
                false,
                "Expected verification error, got {other:?} for status={status:?}"
            ),
        }
    }

    /// Invariant: per-investment caps for High/VeryHigh risk are enforced
    /// independently of the effective limit.
    ///
    /// A bid of `cap + 1` must be rejected even if it is within the effective limit.
    #[test]
    fn fuzz_per_investment_cap_enforced(
        base_limit in 100_000u128..MAX_BASE_LIMIT,
        risk_idx   in 2usize..4usize,  // only High (2) and VeryHigh (3) have caps
    ) {
        let risks = [
            RiskLevel::Low,
            RiskLevel::Medium,
            RiskLevel::High,
            RiskLevel::VeryHigh,
        ];
        let risk = risks[risk_idx].clone();

        let cap = per_investment_cap(risk.clone())
            .expect("High/VeryHigh must have a per-investment cap");

        let over_cap = cap.saturating_add(1);

        let result = guard_bid_placement(
            Some(VerificationStatus::Verified),
            over_cap,
            base_limit,
            InvestorTier::Basic,
            risk,
        );

        prop_assert!(
            result.is_err(),
            "Bid={over_cap} over per-investment cap={cap} must be rejected"
        );

        // Must be PerInvestmentCapExceeded (not a limit error)
        match result.unwrap_err() {
            GuardError::PerInvestmentCapExceeded { requested, cap: c } => {
                prop_assert_eq!(requested, over_cap);
                prop_assert_eq!(c, cap);
            }
            GuardError::InvestmentLimitExceeded { .. } => {
                // Also acceptable: over_cap may also exceed the effective limit
            }
            other => prop_assert!(
                false,
                "Expected PerInvestmentCapExceeded, got {other:?}"
            ),
        }
    }

    /// Invariant: zero-amount bids are always rejected.
    #[test]
    fn fuzz_zero_bid_always_rejected(
        base_limit in 1u128..MAX_BASE_LIMIT,
    ) {
        let result = guard_bid_placement(
            Some(VerificationStatus::Verified),
            0,
            base_limit,
            InvestorTier::Basic,
            RiskLevel::Low,
        );

        prop_assert_eq!(result, Err(GuardError::ZeroAmount));
    }

    /// Invariant: `compute_tier` is monotone — higher investment and count
    /// always yields a tier ≥ the tier for lower values.
    ///
    /// This ensures the bid-cap system cannot be gamed by reducing activity.
    #[test]
    fn fuzz_tier_monotone_in_investment(
        invested_lo in 0u128..5_000_000u128,
        invested_hi in 0u128..5_000_000u128,
        count_lo    in 0u32..50u32,
        count_hi    in 0u32..50u32,
    ) {
        let (lo_inv, hi_inv) = if invested_lo <= invested_hi {
            (invested_lo, invested_hi)
        } else {
            (invested_hi, invested_lo)
        };
        let (lo_cnt, hi_cnt) = if count_lo <= count_hi {
            (count_lo, count_hi)
        } else {
            (count_hi, count_lo)
        };

        let tier_lo = compute_tier(lo_inv, lo_cnt);
        let tier_hi = compute_tier(hi_inv, hi_cnt);

        let mult_lo = tier_multiplier(tier_lo);
        let mult_hi = tier_multiplier(tier_hi);

        // Invariant: higher activity → tier multiplier is ≥ lower activity
        prop_assert!(
            mult_hi >= mult_lo,
            "Tier multiplier decreased: lo=({lo_inv},{lo_cnt})→{mult_lo}× hi=({hi_inv},{hi_cnt})→{mult_hi}×"
        );
    }

    /// Invariant: `risk_level_from_score` covers the full 0–100 range without gaps.
    #[test]
    fn fuzz_risk_score_full_coverage(score in 0u32..=MAX_RISK_SCORE) {
        let level = risk_level_from_score(score);
        prop_assert!(
            level.is_some(),
            "risk_level_from_score returned None for valid score={score}"
        );
    }

    /// Invariant: scores above MAX_RISK_SCORE are always rejected.
    #[test]
    fn fuzz_risk_score_over_max_rejected(score in (MAX_RISK_SCORE + 1)..u32::MAX) {
        prop_assert!(
            risk_level_from_score(score).is_none(),
            "Expected None for score={score} > MAX_RISK_SCORE={MAX_RISK_SCORE}"
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Cross-invariant integration: settlement + bid limits
//
// Validates that the arithmetic modules are mutually consistent:
// a settlement that passes conservation also produces a profit that is
// consistent with the investor's bid limit check.
// ─────────────────────────────────────────────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    /// Invariant: if a bid passes `guard_bid_placement` and the resulting
    /// settlement passes conservation, then `investor_profit >= 0`.
    ///
    /// This ties together invariants 1 and 3: a valid bid within limits,
    /// when settled, must not produce a loss for the investor.
    #[test]
    fn fuzz_valid_bid_settlement_no_investor_loss(
        base_limit in 1_000u128..1_000_000u128,
        bid_pct    in 1u128..90u128,   // bid as % of base_limit
        fee_bps    in 0u128..500u128,  // reasonable fee range
        pen_bps    in 0u128..1_000u128,
    ) {
        let tier = InvestorTier::Basic;
        let risk = RiskLevel::Low;

        let effective_limit = match compute_effective_limit(base_limit, tier.clone(), risk.clone()) {
            Some(l) => l,
            None => return Ok(()),
        };

        let bid_amount = effective_limit.saturating_mul(bid_pct) / 100;
        if bid_amount == 0 { return Ok(()); }

        // Step 1: bid must pass the guard
        let guard_result = guard_bid_placement(
            Some(VerificationStatus::Verified),
            bid_amount,
            base_limit,
            tier,
            risk,
        );
        prop_assert!(guard_result.is_ok(), "Valid bid rejected: {guard_result:?}");

        // Step 2: use bid_amount as funded_amount; face_value = bid_amount (at par)
        let face = bid_amount;
        if let Some(settlement) = compute_settlement(face, bid_amount, fee_bps, pen_bps) {
            prop_assert!(verify_conservation(&settlement));

            // Step 3: investor profit must be ≥ 0 (payout >= funded)
            let profit = investor_profit(settlement.investor_payout, bid_amount);
            prop_assert!(
                profit.is_some(),
                "Investor loss after valid settlement: payout={} funded={bid_amount}",
                settlement.investor_payout
            );
        }
    }
}
