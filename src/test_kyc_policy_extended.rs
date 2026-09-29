#[cfg(test)]
mod extended_tests {
    use crate::kyc_policy::*;
    use crate::verification::VerificationStatus;

    fn kyc(status: VerificationStatus, expires_at: u64) -> Option<KycRecord> {
        Some(KycRecord {
            status,
            expires_at,
            version: 42,
        })
    }

    #[test]
    fn missing_at_zero_is_missing() {
        assert_eq!(
            require_eligible(None, 0, 0),
            Err(KycEligibilityError::Missing)
        );
    }
    #[test]
    fn missing_at_max_is_missing() {
        assert_eq!(
            require_eligible(None, u64::MAX, 0),
            Err(KycEligibilityError::Missing)
        );
    }
    #[test]
    fn pending_before_expiry_is_pending() {
        assert_eq!(
            require_eligible(kyc(VerificationStatus::Pending, 100), 1, 0),
            Err(KycEligibilityError::Pending)
        );
    }
    #[test]
    fn pending_at_expiry_is_pending() {
        assert_eq!(
            require_eligible(kyc(VerificationStatus::Pending, 100), 100, 0),
            Err(KycEligibilityError::Pending)
        );
    }
    #[test]
    fn pending_after_expiry_is_pending() {
        assert_eq!(
            require_eligible(kyc(VerificationStatus::Pending, 100), 101, 0),
            Err(KycEligibilityError::Pending)
        );
    }
    #[test]
    fn rejected_before_expiry_is_revoked() {
        assert_eq!(
            require_eligible(kyc(VerificationStatus::Rejected, 100), 1, 0),
            Err(KycEligibilityError::Revoked)
        );
    }
    #[test]
    fn rejected_at_expiry_is_revoked() {
        assert_eq!(
            require_eligible(kyc(VerificationStatus::Rejected, 100), 100, 0),
            Err(KycEligibilityError::Revoked)
        );
    }
    #[test]
    fn rejected_after_expiry_is_revoked() {
        assert_eq!(
            require_eligible(kyc(VerificationStatus::Rejected, 100), 101, 0),
            Err(KycEligibilityError::Revoked)
        );
    }
    #[test]
    fn verified_at_one_is_valid_at_zero() {
        assert!(require_eligible(kyc(VerificationStatus::Verified, 1), 0, 0).is_ok());
    }
    #[test]
    fn verified_at_one_is_expired_at_one() {
        assert_eq!(
            require_eligible(kyc(VerificationStatus::Verified, 1), 1, 0),
            Err(KycEligibilityError::Expired)
        );
    }
    #[test]
    fn verified_max_expiry_accepts_normal_time() {
        assert!(require_eligible(kyc(VerificationStatus::Verified, u64::MAX), 0, 0).is_ok());
    }
    #[test]
    fn verified_max_expiry_accepts_near_max() {
        assert!(
            require_eligible(kyc(VerificationStatus::Verified, u64::MAX), u64::MAX - 1, 0).is_ok()
        );
    }
    #[test]
    fn zero_expiry_precedes_status_evaluation() {
        assert_eq!(
            require_eligible(kyc(VerificationStatus::Pending, 0), 0, 0),
            Err(KycEligibilityError::InvalidExpiry)
        );
    }
    #[test]
    fn version_is_returned_unchanged() {
        assert_eq!(
            require_eligible(kyc(VerificationStatus::Verified, 10), 9, 0)
                .unwrap()
                .version,
            42
        );
    }
    #[test]
    fn business_create_requires_business() {
        assert!(authorize_action(
            kyc(VerificationStatus::Verified, 10),
            KycActor::Business,
            KycDependentAction::CreateInvoice,
            9,
            0
        )
        .is_ok());
    }
    #[test]
    fn business_create_rejects_investor() {
        assert!(authorize_action(
            kyc(VerificationStatus::Verified, 10),
            KycActor::Investor,
            KycDependentAction::CreateInvoice,
            9,
            0
        )
        .is_err());
    }
    #[test]
    fn investor_bid_requires_investor() {
        assert!(authorize_action(
            kyc(VerificationStatus::Verified, 10),
            KycActor::Investor,
            KycDependentAction::SubmitBid,
            9,
            0
        )
        .is_ok());
    }
    #[test]
    fn business_bid_rejects_business() {
        assert!(authorize_action(
            kyc(VerificationStatus::Verified, 10),
            KycActor::Business,
            KycDependentAction::SubmitBid,
            9,
            0
        )
        .is_err());
    }
    #[test]
    fn investor_fund_requires_investor() {
        assert!(authorize_action(
            kyc(VerificationStatus::Verified, 10),
            KycActor::Investor,
            KycDependentAction::FundInvoice,
            9,
            0
        )
        .is_ok());
    }
    #[test]
    fn business_fund_rejects_business() {
        assert!(authorize_action(
            kyc(VerificationStatus::Verified, 10),
            KycActor::Business,
            KycDependentAction::FundInvoice,
            9,
            0
        )
        .is_err());
    }
    #[test]
    fn settlement_is_terminal() {
        assert!(is_terminal_action(KycDependentAction::SettleInvoice));
    }
    #[test]
    fn creation_is_nonterminal() {
        assert!(!is_terminal_action(KycDependentAction::CreateInvoice));
    }
    #[test]
    fn bidding_is_nonterminal() {
        assert!(!is_terminal_action(KycDependentAction::SubmitBid));
    }
    #[test]
    fn funding_is_nonterminal() {
        assert!(!is_terminal_action(KycDependentAction::FundInvoice));
    }
    #[test]
    fn terminal_none_is_allowed() {
        assert!(authorize_before_side_effect(
            None,
            KycActor::Investor,
            KycDependentAction::SettleInvoice,
            u64::MAX,
            true,
            0
        )
        .is_ok());
    }
    #[test]
    fn terminal_expired_is_allowed() {
        assert!(authorize_before_side_effect(
            kyc(VerificationStatus::Verified, 1),
            KycActor::Investor,
            KycDependentAction::SettleInvoice,
            2,
            true,
            0
        )
        .is_ok());
    }
    #[test]
    fn terminal_pending_is_allowed() {
        assert!(authorize_before_side_effect(
            kyc(VerificationStatus::Pending, 1),
            KycActor::Investor,
            KycDependentAction::SettleInvoice,
            2,
            true,
            0
        )
        .is_ok());
    }
    #[test]
    fn nonterminal_none_is_denied() {
        assert!(authorize_before_side_effect(
            None,
            KycActor::Investor,
            KycDependentAction::FundInvoice,
            1,
            false,
            0
        )
        .is_err());
    }
    #[test]
    fn nonterminal_expired_is_denied() {
        assert!(authorize_before_side_effect(
            kyc(VerificationStatus::Verified, 1),
            KycActor::Investor,
            KycDependentAction::FundInvoice,
            1,
            false,
            0
        )
        .is_err());
    }
    #[test]
    fn nonterminal_pending_is_denied() {
        assert!(authorize_before_side_effect(
            kyc(VerificationStatus::Pending, 100),
            KycActor::Investor,
            KycDependentAction::FundInvoice,
            1,
            false,
            0
        )
        .is_err());
    }
    #[test]
    fn update_from_zero_to_one_is_allowed() {
        assert!(can_update_verification(false, 0, 1));
    }
    #[test]
    fn update_from_one_to_two_is_allowed() {
        assert!(can_update_verification(false, 1, 2));
    }
    #[test]
    fn update_same_version_is_denied() {
        assert!(!can_update_verification(false, 2, 2));
    }
    #[test]
    fn update_lower_version_is_denied() {
        assert!(!can_update_verification(false, 2, 1));
    }
    #[test]
    fn update_terminal_to_higher_is_denied() {
        assert!(!can_update_verification(true, 2, 3));
    }
    #[test]
    fn update_terminal_to_max_is_denied() {
        assert!(!can_update_verification(true, 0, u32::MAX));
    }
    #[test]
    fn status_error_does_not_depend_on_actor() {
        for actor in [KycActor::Business, KycActor::Investor] {
            assert_eq!(
                authorize_action(
                    kyc(VerificationStatus::Pending, 100),
                    actor,
                    KycDependentAction::SettleInvoice,
                    1,
                    0
                ),
                Err(KycEligibilityError::Pending)
            );
        }
    }
    #[test]
    fn expiry_error_does_not_depend_on_actor() {
        for actor in [KycActor::Business, KycActor::Investor] {
            assert_eq!(
                authorize_action(
                    kyc(VerificationStatus::Verified, 1),
                    actor,
                    KycDependentAction::SettleInvoice,
                    1,
                    0
                ),
                Err(KycEligibilityError::Expired)
            );
        }
    }
    #[test]
    fn missing_error_does_not_depend_on_action() {
        for action in [
            KycDependentAction::CreateInvoice,
            KycDependentAction::SubmitBid,
            KycDependentAction::FundInvoice,
            KycDependentAction::SettleInvoice,
        ] {
            assert_eq!(
                authorize_action(None, KycActor::Business, action, 1, 0),
                Err(KycEligibilityError::Missing)
            );
        }
    }
    #[test]
    fn rejected_error_does_not_depend_on_action() {
        for action in [
            KycDependentAction::CreateInvoice,
            KycDependentAction::SubmitBid,
            KycDependentAction::FundInvoice,
            KycDependentAction::SettleInvoice,
        ] {
            assert_eq!(
                authorize_action(
                    kyc(VerificationStatus::Rejected, 100),
                    KycActor::Investor,
                    action,
                    1,
                    0
                ),
                Err(KycEligibilityError::Revoked)
            );
        }
    }
    #[test]
    fn business_action_boundary_before_expiry() {
        for action in [
            KycDependentAction::CreateInvoice,
            KycDependentAction::SettleInvoice,
        ] {
            assert!(authorize_action(
                kyc(VerificationStatus::Verified, 500),
                KycActor::Business,
                action,
                499,
                0
            )
            .is_ok());
        }
    }
    #[test]
    fn investor_action_boundary_before_expiry() {
        for action in [
            KycDependentAction::SubmitBid,
            KycDependentAction::FundInvoice,
        ] {
            assert!(authorize_action(
                kyc(VerificationStatus::Verified, 500),
                KycActor::Investor,
                action,
                499,
                0
            )
            .is_ok());
        }
    }
    #[test]
    fn business_action_boundary_at_expiry() {
        assert_eq!(
            authorize_action(
                kyc(VerificationStatus::Verified, 500),
                KycActor::Business,
                KycDependentAction::CreateInvoice,
                500,
                0
            ),
            Err(KycEligibilityError::Expired)
        );
    }
    #[test]
    fn investor_action_boundary_at_expiry() {
        assert_eq!(
            authorize_action(
                kyc(VerificationStatus::Verified, 500),
                KycActor::Investor,
                KycDependentAction::SubmitBid,
                500,
                0
            ),
            Err(KycEligibilityError::Expired)
        );
    }
    #[test]
    fn terminal_flag_is_the_only_terminal_bypass() {
        assert!(authorize_before_side_effect(
            None,
            KycActor::Business,
            KycDependentAction::SettleInvoice,
            500,
            true,
            0
        )
        .is_ok());
        assert!(authorize_before_side_effect(
            None,
            KycActor::Business,
            KycDependentAction::SettleInvoice,
            500,
            false,
            0
        )
        .is_err());
    }
    #[test]
    fn version_update_does_not_change_terminal_policy() {
        assert!(!can_update_verification(true, 1, 2));
        assert!(is_terminal_action(KycDependentAction::SettleInvoice));
    }
    #[test]
    fn verified_record_copy_is_equal() {
        let record = kyc(VerificationStatus::Verified, 20).unwrap();
        assert_eq!(require_eligible(Some(record), 19, 0), Ok(record));
    }
    #[test]
    fn errors_are_copyable_and_comparable() {
        let left = KycEligibilityError::Pending;
        let right = left;
        assert_eq!(left, right);
    }
    #[test]
    fn actor_values_are_copyable() {
        let left = KycActor::Business;
        let right = left;
        assert_eq!(left, right);
    }
    #[test]
    fn action_values_are_copyable() {
        let left = KycDependentAction::FundInvoice;
        let right = left;
        assert_eq!(left, right);
    }
    #[test]
    fn record_values_are_copyable() {
        let left = kyc(VerificationStatus::Verified, 3).unwrap();
        let right = left;
        assert_eq!(left, right);
    }
}
