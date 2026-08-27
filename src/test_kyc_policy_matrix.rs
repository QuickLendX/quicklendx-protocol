#[cfg(test)]
mod tests {
    use crate::kyc_policy::*;
    use crate::verification::VerificationStatus;

    fn record(status: VerificationStatus, expiry: u64, version: u32) -> Option<KycRecord> { Some(KycRecord { status, expires_at: expiry, version }) }

    #[test]
    fn every_status_has_a_stable_error() {
        assert_eq!(require_eligible(None, 50), Err(KycEligibilityError::Missing));
        assert_eq!(require_eligible(record(VerificationStatus::Pending, 100, 1), 50), Err(KycEligibilityError::Pending));
        assert_eq!(require_eligible(record(VerificationStatus::Rejected, 100, 1), 50), Err(KycEligibilityError::Revoked));
        assert_eq!(require_eligible(record(VerificationStatus::Verified, 49, 1), 50), Err(KycEligibilityError::Expired));
        assert_eq!(require_eligible(record(VerificationStatus::Verified, 0, 1), 0), Err(KycEligibilityError::InvalidExpiry));
    }

    #[test]
    fn verified_boundaries_are_identical_for_all_actions() {
        let actions = [KycDependentAction::CreateInvoice, KycDependentAction::SubmitBid, KycDependentAction::FundInvoice, KycDependentAction::SettleInvoice];
        for action in actions {
            let actor = if action == KycDependentAction::CreateInvoice { KycActor::Business } else { KycActor::Investor };
            assert!(authorize_action(record(VerificationStatus::Verified, 100, 7), actor, action, 99).is_ok());
            assert_eq!(authorize_action(record(VerificationStatus::Verified, 100, 7), actor, action, 100), Err(KycEligibilityError::Expired));
        }
    }

    #[test]
    fn pending_cannot_cross_any_nonterminal_entrypoint() {
        let actions = [KycDependentAction::CreateInvoice, KycDependentAction::SubmitBid, KycDependentAction::FundInvoice, KycDependentAction::SettleInvoice];
        for action in actions {
            assert_eq!(authorize_before_side_effect(record(VerificationStatus::Pending, 100, 1), KycActor::Investor, action, 1, false), Err(KycEligibilityError::Pending));
        }
    }

    #[test]
    fn revoked_cannot_cross_any_nonterminal_entrypoint() {
        let actions = [KycDependentAction::CreateInvoice, KycDependentAction::SubmitBid, KycDependentAction::FundInvoice, KycDependentAction::SettleInvoice];
        for action in actions {
            assert_eq!(authorize_before_side_effect(record(VerificationStatus::Rejected, 100, 1), KycActor::Investor, action, 1, false), Err(KycEligibilityError::Revoked));
        }
    }

    #[test]
    fn terminal_financial_records_remain_stable_across_kyc_updates() {
        for status in [VerificationStatus::Pending, VerificationStatus::Verified, VerificationStatus::Rejected] {
            let original = record(status, 1, 2);
            assert!(authorize_before_side_effect(original, KycActor::Business, KycDependentAction::SettleInvoice, u64::MAX, true).is_ok());
            assert!(!can_update_verification(true, 2, 3));
        }
    }

    #[test]
    fn version_replay_is_rejected() {
        for next in [0, 1, 2, 9] { assert_eq!(can_update_verification(false, 2, next), next > 2); }
    }

    #[test]
    fn actor_matrix_does_not_bypass_status_guard() {
        for actor in [KycActor::Business, KycActor::Investor] {
            for action in [KycDependentAction::CreateInvoice, KycDependentAction::SubmitBid, KycDependentAction::FundInvoice] {
                assert_ne!(authorize_action(record(VerificationStatus::Pending, 100, 1), actor, action, 1), Ok(KycRecord { status: VerificationStatus::Pending, expires_at: 100, version: 1 }));
            }
        }
    }
}
