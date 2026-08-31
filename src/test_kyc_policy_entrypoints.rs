#[cfg(test)]
mod entrypoint_tests {
    use crate::kyc_policy::*;
    use crate::verification::VerificationStatus;

    fn valid() -> Option<KycRecord> {
        Some(KycRecord::V1(KycRecordV1 { status: VerificationStatus::Verified, expires_at: 1_000, version: 9 }))
    }

    #[test]
    fn create_invoice_checks_business_status() {
        let result = authorize_action(
            valid(),
            KycActor::Business,
            KycDependentAction::CreateInvoice,
            999,
            0,
        );
        assert_eq!(result.unwrap().version, 9);
    }

    #[test]
    fn submit_bid_checks_investor_status() {
        let result = authorize_action(
            valid(),
            KycActor::Investor,
            KycDependentAction::SubmitBid,
            999,
            0,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn fund_invoice_checks_investor_status() {
        let result = authorize_action(
            valid(),
            KycActor::Investor,
            KycDependentAction::FundInvoice,
            999,
            0,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn settlement_checks_nonterminal_status() {
        let result = authorize_before_side_effect(
            valid(),
            KycActor::Investor,
            KycDependentAction::SettleInvoice,
            999,
            false,
            0,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn missing_create_invoice_is_blocked() {
        let result = authorize_action(
            None,
            KycActor::Business,
            KycDependentAction::CreateInvoice,
            1,
            0,
        );
        assert_eq!(result, Err(KycEligibilityError::Missing));
    }

    #[test]
    fn missing_submit_bid_is_blocked() {
        let result = authorize_action(
            None,
            KycActor::Investor,
            KycDependentAction::SubmitBid,
            1,
            0,
        );
        assert_eq!(result, Err(KycEligibilityError::Missing));
    }

    #[test]
    fn missing_fund_invoice_is_blocked() {
        let result = authorize_action(
            None,
            KycActor::Investor,
            KycDependentAction::FundInvoice,
            1,
            0,
        );
        assert_eq!(result, Err(KycEligibilityError::Missing));
    }

    #[test]
    fn missing_nonterminal_settlement_is_blocked() {
        let result = authorize_before_side_effect(
            None,
            KycActor::Investor,
            KycDependentAction::SettleInvoice,
            1,
            false,
            0,
        );
        assert_eq!(result, Err(KycEligibilityError::Missing));
    }

    #[test]
    fn pending_create_invoice_is_blocked() {
        let record = Some(KycRecord::V1(KycRecordV1 { status: VerificationStatus::Pending, expires_at: 1_000, version: 1 }));
        assert_eq!(
            authorize_action(
                record,
                KycActor::Business,
                KycDependentAction::CreateInvoice,
                1,
                0
            ),
            Err(KycEligibilityError::Pending)
        );
    }

    #[test]
    fn pending_submit_bid_is_blocked() {
        let record = Some(KycRecord::V1(KycRecordV1 { status: VerificationStatus::Pending, expires_at: 1_000, version: 1 }));
        assert_eq!(
            authorize_action(
                record,
                KycActor::Investor,
                KycDependentAction::SubmitBid,
                1,
                0
            ),
            Err(KycEligibilityError::Pending)
        );
    }

    #[test]
    fn pending_fund_invoice_is_blocked() {
        let record = Some(KycRecord::V1(KycRecordV1 { status: VerificationStatus::Pending, expires_at: 1_000, version: 1 }));
        assert_eq!(
            authorize_action(
                record,
                KycActor::Investor,
                KycDependentAction::FundInvoice,
                1,
                0
            ),
            Err(KycEligibilityError::Pending)
        );
    }

    #[test]
    fn pending_settlement_is_blocked_before_side_effect() {
        let record = Some(KycRecord::V1(KycRecordV1 { status: VerificationStatus::Pending, expires_at: 1_000, version: 1 }));
        assert_eq!(
            authorize_before_side_effect(
                record,
                KycActor::Investor,
                KycDependentAction::SettleInvoice,
                1,
                false,
                0
            ),
            Err(KycEligibilityError::Pending)
        );
    }

    #[test]
    fn revoked_create_invoice_is_blocked() {
        let record = Some(KycRecord::V1(KycRecordV1 { status: VerificationStatus::Rejected, expires_at: 1_000, version: 1 }));
        assert_eq!(
            authorize_action(
                record,
                KycActor::Business,
                KycDependentAction::CreateInvoice,
                1,
                0
            ),
            Err(KycEligibilityError::Revoked)
        );
    }

    #[test]
    fn revoked_submit_bid_is_blocked() {
        let record = Some(KycRecord::V1(KycRecordV1 { status: VerificationStatus::Rejected, expires_at: 1_000, version: 1 }));
        assert_eq!(
            authorize_action(
                record,
                KycActor::Investor,
                KycDependentAction::SubmitBid,
                1,
                0
            ),
            Err(KycEligibilityError::Revoked)
        );
    }

    #[test]
    fn revoked_fund_invoice_is_blocked() {
        let record = Some(KycRecord::V1(KycRecordV1 { status: VerificationStatus::Rejected, expires_at: 1_000, version: 1 }));
        assert_eq!(
            authorize_action(
                record,
                KycActor::Investor,
                KycDependentAction::FundInvoice,
                1,
                0
            ),
            Err(KycEligibilityError::Revoked)
        );
    }

    #[test]
    fn revoked_settlement_is_blocked_before_side_effect() {
        let record = Some(KycRecord::V1(KycRecordV1 { status: VerificationStatus::Rejected, expires_at: 1_000, version: 1 }));
        assert_eq!(
            authorize_before_side_effect(
                record,
                KycActor::Investor,
                KycDependentAction::SettleInvoice,
                1,
                false,
                0
            ),
            Err(KycEligibilityError::Revoked)
        );
    }

    #[test]
    fn expired_create_invoice_is_blocked_at_boundary() {
        assert_eq!(
            authorize_action(
                valid(),
                KycActor::Business,
                KycDependentAction::CreateInvoice,
                1_000,
                0
            ),
            Err(KycEligibilityError::Expired)
        );
    }

    #[test]
    fn expired_submit_bid_is_blocked_at_boundary() {
        assert_eq!(
            authorize_action(
                valid(),
                KycActor::Investor,
                KycDependentAction::SubmitBid,
                1_000,
                0
            ),
            Err(KycEligibilityError::Expired)
        );
    }

    #[test]
    fn expired_fund_invoice_is_blocked_at_boundary() {
        assert_eq!(
            authorize_action(
                valid(),
                KycActor::Investor,
                KycDependentAction::FundInvoice,
                1_000,
                0
            ),
            Err(KycEligibilityError::Expired)
        );
    }

    #[test]
    fn expired_settlement_is_blocked_before_side_effect() {
        assert_eq!(
            authorize_before_side_effect(
                valid(),
                KycActor::Investor,
                KycDependentAction::SettleInvoice,
                1_000,
                false,
                0
            ),
            Err(KycEligibilityError::Expired)
        );
    }

    #[test]
    fn terminal_create_path_is_not_a_settlement_bypass() {
        assert!(authorize_before_side_effect(
            valid(),
            KycActor::Business,
            KycDependentAction::CreateInvoice,
            2_000,
            true,
            0
        )
        .is_ok());
    }

    #[test]
    fn terminal_settlement_path_is_explicit() {
        assert!(authorize_before_side_effect(
            None,
            KycActor::Investor,
            KycDependentAction::SettleInvoice,
            2_000,
            true,
            0
        )
        .is_ok());
    }

    #[test]
    fn terminal_flag_does_not_change_policy_function() {
        assert_eq!(
            require_eligible(valid(), 1_000, 0),
            Err(KycEligibilityError::Expired)
        );
    }

    #[test]
    fn version_update_before_terminal_record_is_monotonic() {
        assert!(can_update_verification(false, 9, 10));
        assert!(!can_update_verification(false, 10, 9));
    }

    #[test]
    fn version_update_after_terminal_record_is_frozen() {
        assert!(!can_update_verification(true, 9, 10));
    }

    #[test]
    fn action_error_precedes_actor_error() {
        let record = Some(KycRecord::V1(KycRecordV1 { status: VerificationStatus::Pending, expires_at: 1_000, version: 1 }));
        assert_eq!(
            authorize_action(
                record,
                KycActor::Investor,
                KycDependentAction::CreateInvoice,
                1,
                0
            ),
            Err(KycEligibilityError::Pending)
        );
    }

    #[test]
    fn action_error_precedes_actor_error_when_expired() {
        assert_eq!(
            authorize_action(
                valid(),
                KycActor::Investor,
                KycDependentAction::CreateInvoice,
                1_000,
                0
            ),
            Err(KycEligibilityError::Expired)
        );
    }

    #[test]
    fn invalid_expiry_precedes_status_error() {
        let record = Some(KycRecord::V1(KycRecordV1 { status: VerificationStatus::Pending, expires_at: 0, version: 1 }));
        assert_eq!(
            require_eligible(record, 0, 0),
            Err(KycEligibilityError::InvalidExpiry)
        );
    }

    #[test]
    fn result_contains_record_version_for_audit() {
        let record = authorize_action(
            valid(),
            KycActor::Investor,
            KycDependentAction::SubmitBid,
            999,
            0,
        )
        .unwrap();
        assert_eq!(record.version, 9);
    }

    #[test]
    fn settlement_action_is_terminal_for_both_actors() {
        assert!(authorize_before_side_effect(
            None,
            KycActor::Business,
            KycDependentAction::SettleInvoice,
            1,
            true,
            0
        )
        .is_ok());
        assert!(authorize_before_side_effect(
            None,
            KycActor::Investor,
            KycDependentAction::SettleInvoice,
            1,
            true,
            0
        )
        .is_ok());
    }

    #[test]
    fn business_and_investor_have_distinct_nonterminal_actions() {
        assert!(authorize_action(
            valid(),
            KycActor::Business,
            KycDependentAction::CreateInvoice,
            999,
            0
        )
        .is_ok());
        assert!(authorize_action(
            valid(),
            KycActor::Investor,
            KycDependentAction::FundInvoice,
            999,
            0
        )
        .is_ok());
    }

    #[test]
    fn replay_protection_identical_nonce_retry_is_idempotent() {
        crate::kyc_nonces::reset_nonces();
        let record = valid();
        let res1 = authorize_action(
            record,
            KycActor::Investor,
            KycDependentAction::SubmitBid,
            500,
            201,
        );
        assert!(res1.is_ok());
        let res2 = authorize_action(
            record,
            KycActor::Investor,
            KycDependentAction::SubmitBid,
            500,
            201,
        );
        assert_eq!(res1, res2);
    }

    #[test]
    fn replay_protection_conflicting_nonce_reuse_is_rejected() {
        crate::kyc_nonces::reset_nonces();
        let record = valid();
        let res1 = authorize_action(
            record,
            KycActor::Investor,
            KycDependentAction::SubmitBid,
            500,
            202,
        );
        assert!(res1.is_ok());
        let res2 = authorize_action(
            record,
            KycActor::Investor,
            KycDependentAction::FundInvoice,
            500,
            202,
        );
        assert_eq!(res2, Err(KycEligibilityError::ReplayedNonce));
    }

    #[test]
    fn failed_eligibility_leaves_no_nonce_state() {
        crate::kyc_nonces::reset_nonces();
        let pending_record = Some(KycRecord::V1(KycRecordV1 { status: VerificationStatus::Pending, expires_at: 1_000, version: 5 }));
        let res1 = authorize_action(
            pending_record,
            KycActor::Business,
            KycDependentAction::CreateInvoice,
            500,
            301,
        );
        assert_eq!(res1, Err(KycEligibilityError::Pending));

        let verified_record = Some(KycRecord::V1(KycRecordV1 { status: VerificationStatus::Verified, expires_at: 1_000, version: 5 }));
        let res2 = authorize_action(
            verified_record,
            KycActor::Business,
            KycDependentAction::CreateInvoice,
            500,
            301,
        );
        assert!(res2.is_ok());
    }
}
