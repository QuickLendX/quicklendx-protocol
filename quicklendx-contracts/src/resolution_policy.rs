//! Resolution policy — documented per-contract effect of each
//! [`DisputeResolution`] outcome.
//!
//! # Purpose
//!
//! When a dispute reaches `Resolved` status the admin records a structured
//! outcome (`FavorBusiness`, `FavorInvestor`, `Split`, or `Dismissed`).
//! This module defines the **expected downstream behaviour** for every
//! contract that observes the dispute: invoice state, settlement, escrow,
//! bids, and investments.
//!
//! The mappings below are the single source of truth.  They are pure
//! (no storage mutation) so callers and off-chain tooling can reason
//! about outcomes without executing a transaction.
//!
//! # Usage
//!
//! ```ignore
//! use resolution_policy::*;
//!
//! let policy = ResolutionPolicy::for_outcome(DisputeResolution::FavorBusiness);
//! assert_eq!(policy.invoice_effect,     InvoiceEffect::StaysFunded);
//! assert_eq!(policy.settlement_effect,  SettlementEffect::Unblocked);
//! assert_eq!(policy.escrow_effect,      EscrowEffect::ReleaseToBusiness);
//! ```
//!
//! # Versioning
//!
//! The policy is versioned so that past resolutions remain interpretable
//! even if the policy is updated.  The current version is
//! [`RESOLUTION_POLICY_VERSION`].

use crate::types::DisputeResolution;
use soroban_sdk::contracttype;

// ---------------------------------------------------------------------------
// Version
// ---------------------------------------------------------------------------

/// Current resolution policy version.
///
/// Increment when the outcome→effect mappings change.  Old versions are
/// retained in the policy history so off-chain indexers can reconstruct
/// the rules that applied at the time of resolution.
pub const RESOLUTION_POLICY_VERSION: u32 = 1;

// ---------------------------------------------------------------------------
// Per-contract effect enums
// ---------------------------------------------------------------------------

/// Effect on the invoice after a dispute resolution.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InvoiceEffect {
    /// Invoice stays in its current status (typically `Funded`).
    /// Settlement may proceed normally.
    StaysFunded,
    /// Invoice should be transitioned to `Refunded` so escrow can be
    /// returned to the investor.
    TransitionToRefunded,
    /// Invoice stays in current status; next step determined by admin.
    PendingAdminAction,
    /// Invoice returns to the status it had before the dispute was
    /// opened (dispute treated as unfounded).
    RevertToPreDisputeStatus,
}

/// Effect on the settlement module after a dispute resolution.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SettlementEffect {
    /// Settlement is unblocked; normal business-payment flow resumes.
    Unblocked,
    /// Settlement is permanently blocked; refund path takes priority.
    PermanentlyBlocked,
    /// Settlement remains blocked until admin takes further action.
    BlockedPendingAdmin,
}

/// Effect on escrow after a dispute resolution.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EscrowEffect {
    /// Escrow may be released to the business (when invoice reaches `Paid`).
    ReleaseToBusiness,
    /// Escrow should be refunded to the investor.
    RefundToInvestor,
    /// Escrow remains `Held` until admin action.
    HeldPendingAdmin,
}

/// Effect on the associated bid after a dispute resolution.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BidEffect {
    /// No bid change; bid was already finalised at funding time.
    Unchanged,
}

/// Effect on the investment after a dispute resolution.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InvestmentEffect {
    /// Investment stays `Active`; eventual settlement leads to `Completed`.
    StaysActive,
    /// Investment transitions to `Refunded`.
    TransitionToRefunded,
    /// Investment stays `Active` pending admin action.
    PendingAdminAction,
}

// ---------------------------------------------------------------------------
// Aggregate policy
// ---------------------------------------------------------------------------

/// The complete resolution policy for a single [`DisputeResolution`] outcome.
///
/// Each field describes the expected behaviour of the corresponding
/// contract module.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResolutionPolicy {
    /// Policy version that produced this mapping.
    pub version: u32,
    /// The structured outcome this policy applies to.
    pub outcome: DisputeResolution,
    /// Effect on the invoice status.
    pub invoice_effect: InvoiceEffect,
    /// Effect on settlement finalisation.
    pub settlement_effect: SettlementEffect,
    /// Effect on escrow disposition.
    pub escrow_effect: EscrowEffect,
    /// Effect on the bid (always `Unchanged` for current funding model).
    pub bid_effect: BidEffect,
    /// Effect on the investment record.
    pub investment_effect: InvestmentEffect,
}

impl ResolutionPolicy {
    /// Returns the policy that applies to `outcome`.
    pub fn for_outcome(outcome: DisputeResolution) -> Self {
        match outcome {
            DisputeResolution::FavorBusiness => Self {
                version: RESOLUTION_POLICY_VERSION,
                outcome,
                invoice_effect: InvoiceEffect::StaysFunded,
                settlement_effect: SettlementEffect::Unblocked,
                escrow_effect: EscrowEffect::ReleaseToBusiness,
                bid_effect: BidEffect::Unchanged,
                investment_effect: InvestmentEffect::StaysActive,
            },

            DisputeResolution::FavorInvestor => Self {
                version: RESOLUTION_POLICY_VERSION,
                outcome,
                invoice_effect: InvoiceEffect::TransitionToRefunded,
                settlement_effect: SettlementEffect::PermanentlyBlocked,
                escrow_effect: EscrowEffect::RefundToInvestor,
                bid_effect: BidEffect::Unchanged,
                investment_effect: InvestmentEffect::TransitionToRefunded,
            },

            DisputeResolution::Split => Self {
                version: RESOLUTION_POLICY_VERSION,
                outcome,
                invoice_effect: InvoiceEffect::PendingAdminAction,
                settlement_effect: SettlementEffect::BlockedPendingAdmin,
                escrow_effect: EscrowEffect::HeldPendingAdmin,
                bid_effect: BidEffect::Unchanged,
                investment_effect: InvestmentEffect::PendingAdminAction,
            },

            DisputeResolution::Dismissed => Self {
                version: RESOLUTION_POLICY_VERSION,
                outcome,
                invoice_effect: InvoiceEffect::StaysFunded,
                settlement_effect: SettlementEffect::Unblocked,
                escrow_effect: EscrowEffect::ReleaseToBusiness,
                bid_effect: BidEffect::Unchanged,
                investment_effect: InvestmentEffect::StaysActive,
            },

            DisputeResolution::None => Self {
                version: RESOLUTION_POLICY_VERSION,
                outcome,
                invoice_effect: InvoiceEffect::StaysFunded,
                settlement_effect: SettlementEffect::Unblocked,
                escrow_effect: EscrowEffect::ReleaseToBusiness,
                bid_effect: BidEffect::Unchanged,
                investment_effect: InvestmentEffect::StaysActive,
            },
        }
    }

    /// Returns the policy history (all versions) for the given outcome.
    /// Currently only v1 exists.
    pub fn history_for_outcome(outcome: DisputeResolution) -> soroban_sdk::Vec<Self> {
        let mut v = soroban_sdk::Vec::new(&soroban_sdk::Env::default());
        v.push_back(Self::for_outcome(outcome));
        v
    }
}

// ---------------------------------------------------------------------------
// Policy-level error constants (documented, not thrown by this module)
// ---------------------------------------------------------------------------

/// Human-readable description of what each outcome means in practice.
pub fn outcome_label(outcome: DisputeResolution) -> &'static str {
    match outcome {
        DisputeResolution::None => "Unspecified",
        DisputeResolution::FavorBusiness => "Business position upheld",
        DisputeResolution::FavorInvestor => "Investor position upheld",
        DisputeResolution::Split => "Liability or funds split between parties",
        DisputeResolution::Dismissed => "Dispute closed without finding for either side",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::DisputeResolution;

    #[test]
    fn test_policy_for_favor_business() {
        let p = ResolutionPolicy::for_outcome(DisputeResolution::FavorBusiness);
        assert_eq!(p.version, RESOLUTION_POLICY_VERSION);
        assert_eq!(p.outcome, DisputeResolution::FavorBusiness);
        assert_eq!(p.invoice_effect, InvoiceEffect::StaysFunded);
        assert_eq!(p.settlement_effect, SettlementEffect::Unblocked);
        assert_eq!(p.escrow_effect, EscrowEffect::ReleaseToBusiness);
        assert_eq!(p.bid_effect, BidEffect::Unchanged);
        assert_eq!(p.investment_effect, InvestmentEffect::StaysActive);
    }

    #[test]
    fn test_policy_for_favor_investor() {
        let p = ResolutionPolicy::for_outcome(DisputeResolution::FavorInvestor);
        assert_eq!(p.version, RESOLUTION_POLICY_VERSION);
        assert_eq!(p.outcome, DisputeResolution::FavorInvestor);
        assert_eq!(p.invoice_effect, InvoiceEffect::TransitionToRefunded);
        assert_eq!(p.settlement_effect, SettlementEffect::PermanentlyBlocked);
        assert_eq!(p.escrow_effect, EscrowEffect::RefundToInvestor);
        assert_eq!(p.bid_effect, BidEffect::Unchanged);
        assert_eq!(p.investment_effect, InvestmentEffect::TransitionToRefunded);
    }

    #[test]
    fn test_policy_for_split() {
        let p = ResolutionPolicy::for_outcome(DisputeResolution::Split);
        assert_eq!(p.version, RESOLUTION_POLICY_VERSION);
        assert_eq!(p.outcome, DisputeResolution::Split);
        assert_eq!(p.invoice_effect, InvoiceEffect::PendingAdminAction);
        assert_eq!(p.settlement_effect, SettlementEffect::BlockedPendingAdmin);
        assert_eq!(p.escrow_effect, EscrowEffect::HeldPendingAdmin);
        assert_eq!(p.bid_effect, BidEffect::Unchanged);
        assert_eq!(p.investment_effect, InvestmentEffect::PendingAdminAction);
    }

    #[test]
    fn test_policy_for_dismissed() {
        // Dismissed uses the `None` policy mapping since it has no
        // structured outcome code path (resolve_dispute_structured with
        // Dismissed emits DisputeRejected but the resolution_outcome is
        // still stored).
        let p = ResolutionPolicy::for_outcome(DisputeResolution::Dismissed);
        assert_eq!(p.version, RESOLUTION_POLICY_VERSION);
        assert_eq!(p.outcome, DisputeResolution::Dismissed);
        assert_eq!(p.invoice_effect, InvoiceEffect::StaysFunded);
        assert_eq!(p.settlement_effect, SettlementEffect::Unblocked);
        assert_eq!(p.escrow_effect, EscrowEffect::ReleaseToBusiness);
        assert_eq!(p.bid_effect, BidEffect::Unchanged);
        assert_eq!(p.investment_effect, InvestmentEffect::StaysActive);
    }

    #[test]
    fn test_all_outcomes_have_policy() {
        for outcome in &[
            DisputeResolution::None,
            DisputeResolution::FavorBusiness,
            DisputeResolution::FavorInvestor,
            DisputeResolution::Split,
            DisputeResolution::Dismissed,
        ] {
            let p = ResolutionPolicy::for_outcome(*outcome);
            assert_eq!(p.outcome, *outcome, "missing policy for {:?}", outcome);
        }
    }

    #[test]
    fn test_outcome_labels() {
        assert_eq!(outcome_label(DisputeResolution::None), "Unspecified");
        assert_eq!(
            outcome_label(DisputeResolution::FavorBusiness),
            "Business position upheld"
        );
        assert_eq!(
            outcome_label(DisputeResolution::FavorInvestor),
            "Investor position upheld"
        );
        assert_eq!(
            outcome_label(DisputeResolution::Split),
            "Liability or funds split between parties"
        );
        assert_eq!(
            outcome_label(DisputeResolution::Dismissed),
            "Dispute closed without finding for either side"
        );
    }

    #[test]
    fn test_policy_version_is_stable() {
        // The current version must be 1. When a new version is added,
        // the old version must be kept in history_for_outcome.
        assert_eq!(RESOLUTION_POLICY_VERSION, 1);
    }
}
