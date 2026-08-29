//! One deterministic KYC eligibility predicate for every KYC-dependent action.
use crate::verification::VerificationStatus;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KycRecord {
    pub status: VerificationStatus,
    pub expires_at: u64,
    pub version: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KycEligibilityError {
    Missing,
    Pending,
    Revoked,
    Expired,
    InvalidExpiry,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KycActor {
    Business,
    Investor,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KycDependentAction {
    CreateInvoice,
    SubmitBid,
    FundInvoice,
    SettleInvoice,
}

/// The shared predicate. Expiry is exclusive: `now == expires_at` is expired.
pub fn require_eligible(record: Option<KycRecord>, now: u64) -> Result<KycRecord, KycEligibilityError> {
    let record = record.ok_or(KycEligibilityError::Missing)?;
    if record.expires_at == 0 { return Err(KycEligibilityError::InvalidExpiry); }
    match record.status {
        VerificationStatus::Verified if now < record.expires_at => Ok(record),
        VerificationStatus::Verified => Err(KycEligibilityError::Expired),
        VerificationStatus::Pending => Err(KycEligibilityError::Pending),
        VerificationStatus::Rejected => Err(KycEligibilityError::Revoked),
    }
}

/// Apply the same predicate to business and investor entrypoints.
pub fn authorize_action(record: Option<KycRecord>, actor: KycActor, action: KycDependentAction, now: u64) -> Result<KycRecord, KycEligibilityError> {
    let record = require_eligible(record, now)?;
    let actor_allowed = match action {
        KycDependentAction::CreateInvoice => actor == KycActor::Business,
        KycDependentAction::SubmitBid | KycDependentAction::FundInvoice => actor == KycActor::Investor,
        KycDependentAction::SettleInvoice => true,
    };
    if actor_allowed { Ok(record) } else { Err(KycEligibilityError::Revoked) }
}

/// Check eligibility before a financial side effect or public bid mutation.
pub fn authorize_before_side_effect(record: Option<KycRecord>, actor: KycActor, action: KycDependentAction, now: u64, terminal: bool) -> Result<KycRecord, KycEligibilityError> {
    if terminal { return Ok(record.unwrap_or(KycRecord { status: VerificationStatus::Verified, expires_at: u64::MAX, version: 0 })); }
    authorize_action(record, actor, action, now)
}

/// Verification updates may be stored after a terminal financial record, but
/// they cannot change the terminal record's outcome.
pub fn can_update_verification(record_is_terminal: bool, current_version: u32, next_version: u32) -> bool {
    !record_is_terminal && next_version > current_version
}

pub fn is_terminal_action(action: KycDependentAction) -> bool { action == KycDependentAction::SettleInvoice }

#[cfg(test)]
mod tests {
    use super::*;

    fn verified(expires_at: u64) -> Option<KycRecord> { Some(KycRecord { status: VerificationStatus::Verified, expires_at, version: 1 }) }

    #[test] fn verified_before_expiry_is_accepted() { assert!(require_eligible(verified(100), 99).is_ok()); }
    #[test] fn exact_expiry_is_rejected() { assert_eq!(require_eligible(verified(100), 100), Err(KycEligibilityError::Expired)); }
    #[test] fn after_expiry_is_rejected() { assert_eq!(require_eligible(verified(100), 101), Err(KycEligibilityError::Expired)); }
    #[test] fn missing_is_distinct() { assert_eq!(require_eligible(None, 1), Err(KycEligibilityError::Missing)); }
    #[test] fn pending_is_distinct() { assert_eq!(require_eligible(Some(KycRecord { status: VerificationStatus::Pending, expires_at: 100, version: 1 }), 1), Err(KycEligibilityError::Pending)); }
    #[test] fn rejected_is_revoked() { assert_eq!(require_eligible(Some(KycRecord { status: VerificationStatus::Rejected, expires_at: 100, version: 1 }), 1), Err(KycEligibilityError::Revoked)); }
    #[test] fn zero_expiry_is_invalid() { assert_eq!(require_eligible(verified(0), 0), Err(KycEligibilityError::InvalidExpiry)); }
    #[test] fn business_can_create_invoice() { assert!(authorize_action(verified(10), KycActor::Business, KycDependentAction::CreateInvoice, 9).is_ok()); }
    #[test] fn investor_cannot_create_invoice() { assert_eq!(authorize_action(verified(10), KycActor::Investor, KycDependentAction::CreateInvoice, 9), Err(KycEligibilityError::Revoked)); }
    #[test] fn investor_can_bid() { assert!(authorize_action(verified(10), KycActor::Investor, KycDependentAction::SubmitBid, 9).is_ok()); }
    #[test] fn investor_can_fund() { assert!(authorize_action(verified(10), KycActor::Investor, KycDependentAction::FundInvoice, 9).is_ok()); }
    #[test] fn terminal_settlement_is_authorized_by_record_state() { assert!(authorize_before_side_effect(None, KycActor::Business, KycDependentAction::SettleInvoice, 1, true).is_ok()); }
    #[test] fn terminal_record_is_not_rechecked() { assert!(authorize_before_side_effect(verified(1), KycActor::Business, KycDependentAction::SettleInvoice, 2, true).is_ok()); }
    #[test] fn nonterminal_settlement_is_checked() { assert_eq!(authorize_before_side_effect(None, KycActor::Business, KycDependentAction::SettleInvoice, 1, false), Err(KycEligibilityError::Missing)); }
    #[test] fn versions_must_increase() { assert!(can_update_verification(false, 1, 2)); assert!(!can_update_verification(false, 2, 2)); }
    #[test] fn terminal_record_cannot_be_retroactively_updated() { assert!(!can_update_verification(true, 1, 2)); }
    #[test] fn action_classification_is_stable() { assert!(is_terminal_action(KycDependentAction::SettleInvoice)); assert!(!is_terminal_action(KycDependentAction::FundInvoice)); }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AmountError {
    InvalidScale,
    NegativeOrZero,
    Overflow,
    ExceedsLimit,
}

/// Enforce exact integer or decimal rules at the participant/amount boundary.
/// Rejects invalid scale, negative/zero signs, and arithmetic overflow before state changes.
pub fn validate_participant_amount(amount: i128, max_limit: u128, scale_factor: u128) -> Result<u128, AmountError> {
    if amount <= 0 {
        return Err(AmountError::NegativeOrZero);
    }
    
    let unsigned_amount = amount as u128;
    
    // Scale amount precisely, checking for multiplication overflow
    let scaled = unsigned_amount
        .checked_mul(scale_factor)
        .ok_or(AmountError::Overflow)?;

    if scaled > max_limit {
        return Err(AmountError::ExceedsLimit);
    }

    Ok(scaled)
}

#[cfg(test)]
mod precision_tests {
    use super::*;

    #[test]
    fn test_zero_and_negative_amounts_rejected() {
        assert_eq!(validate_participant_amount(0, 1000, 1), Err(AmountError::NegativeOrZero));
        assert_eq!(validate_participant_amount(-50, 1000, 1), Err(AmountError::NegativeOrZero));
    }

    #[test]
    fn test_exact_scale_and_limit_boundary() {
        let max_limit = 10_000u128;
        let scale = 100u128;

        // Valid boundary
        let res = validate_participant_amount(100, max_limit, scale);
        assert_eq!(res, Ok(10_000));

        // Exceeds limit boundary
        let over = validate_participant_amount(101, max_limit, scale);
        assert_eq!(over, Err(AmountError::ExceedsLimit));
    }

    #[test]
    fn test_near_overflow_and_overflow_rejection() {
        let max_limit = u128::MAX;
        let scale = 1_000_000u128;

        // Near-overflow value that multiplies beyond u128::MAX
        let massive_amount = i128::MAX;
        let result = validate_participant_amount(massive_amount, max_limit, scale);
        assert_eq!(result, Err(AmountError::Overflow));
    }
}
