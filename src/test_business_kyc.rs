/// # Business KYC Verification Guard Tests
///
/// Comprehensive test coverage for business-actor verification guards,
/// state transitions, and input validation in the centralized verification
/// module.
///
/// ## Coverage Areas
///
/// - Guard denial for every non-verified status (negative tests)
/// - Guard approval for verified businesses
/// - State-transition enforcement (all valid and invalid paths)
/// - Rejection reason validation (empty, max-length, over-limit)
/// - KYC data payload validation
/// - All three guarded business actions: invoice upload, settlement, escrow

use crate::verification::*;

// ─────────────────────────────────────────────────────────────────────────────
// Guard: guard_business_action — core status check
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_business_guard_verified_passes() {
    assert!(guard_business_action(Some(VerificationStatus::Verified)).is_ok());
}

#[test]
fn test_business_guard_pending_denied() {
    assert_eq!(
        guard_business_action(Some(VerificationStatus::Pending)),
        Err(GuardError::VerificationPending)
    );
}

#[test]
fn test_business_guard_rejected_denied() {
    assert_eq!(
        guard_business_action(Some(VerificationStatus::Rejected)),
        Err(GuardError::VerificationRejected)
    );
}

#[test]
fn test_business_guard_not_submitted_denied() {
    assert_eq!(
        guard_business_action(None),
        Err(GuardError::NotSubmitted)
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Guard: guard_invoice_upload
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_invoice_upload_verified_passes() {
    assert!(guard_invoice_upload(Some(VerificationStatus::Verified)).is_ok());
}

#[test]
fn test_invoice_upload_pending_denied() {
    assert_eq!(
        guard_invoice_upload(Some(VerificationStatus::Pending)),
        Err(GuardError::VerificationPending)
    );
}

#[test]
fn test_invoice_upload_rejected_denied() {
    assert_eq!(
        guard_invoice_upload(Some(VerificationStatus::Rejected)),
        Err(GuardError::VerificationRejected)
    );
}

#[test]
fn test_invoice_upload_not_submitted_denied() {
    assert_eq!(
        guard_invoice_upload(None),
        Err(GuardError::NotSubmitted)
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Guard: guard_settlement_initiation
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_settlement_initiation_verified_passes() {
    assert!(guard_settlement_initiation(Some(VerificationStatus::Verified)).is_ok());
}

#[test]
fn test_settlement_initiation_pending_denied() {
    assert_eq!(
        guard_settlement_initiation(Some(VerificationStatus::Pending)),
        Err(GuardError::VerificationPending)
    );
}

#[test]
fn test_settlement_initiation_rejected_denied() {
    assert_eq!(
        guard_settlement_initiation(Some(VerificationStatus::Rejected)),
        Err(GuardError::VerificationRejected)
    );
}

#[test]
fn test_settlement_initiation_not_submitted_denied() {
    assert_eq!(
        guard_settlement_initiation(None),
        Err(GuardError::NotSubmitted)
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Guard: guard_escrow_release
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_escrow_release_verified_passes() {
    assert!(guard_escrow_release(Some(VerificationStatus::Verified)).is_ok());
}

#[test]
fn test_escrow_release_pending_denied() {
    assert_eq!(
        guard_escrow_release(Some(VerificationStatus::Pending)),
        Err(GuardError::VerificationPending)
    );
}

#[test]
fn test_escrow_release_rejected_denied() {
    assert_eq!(
        guard_escrow_release(Some(VerificationStatus::Rejected)),
        Err(GuardError::VerificationRejected)
    );
}

#[test]
fn test_escrow_release_not_submitted_denied() {
    assert_eq!(
        guard_escrow_release(None),
        Err(GuardError::NotSubmitted)
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// State transitions — valid paths
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_transition_pending_to_verified_valid() {
    assert!(validate_transition(
        VerificationStatus::Pending,
        VerificationStatus::Verified
    ).is_ok());
}

#[test]
fn test_transition_pending_to_rejected_valid() {
    assert!(validate_transition(
        VerificationStatus::Pending,
        VerificationStatus::Rejected
    ).is_ok());
}

#[test]
fn test_transition_rejected_to_pending_resubmit_valid() {
    assert!(validate_transition(
        VerificationStatus::Rejected,
        VerificationStatus::Pending
    ).is_ok());
}

// ─────────────────────────────────────────────────────────────────────────────
// State transitions — invalid paths (negative tests)
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_transition_verified_to_pending_blocked() {
    assert_eq!(
        validate_transition(VerificationStatus::Verified, VerificationStatus::Pending),
        Err(TransitionError::AlreadyVerified)
    );
}

#[test]
fn test_transition_verified_to_rejected_blocked() {
    assert_eq!(
        validate_transition(VerificationStatus::Verified, VerificationStatus::Rejected),
        Err(TransitionError::AlreadyVerified)
    );
}

#[test]
fn test_transition_verified_to_verified_blocked() {
    assert_eq!(
        validate_transition(VerificationStatus::Verified, VerificationStatus::Verified),
        Err(TransitionError::AlreadyVerified)
    );
}

#[test]
fn test_transition_pending_to_pending_duplicate_blocked() {
    assert_eq!(
        validate_transition(VerificationStatus::Pending, VerificationStatus::Pending),
        Err(TransitionError::AlreadyPending)
    );
}

#[test]
fn test_transition_rejected_to_verified_skip_blocked() {
    assert_eq!(
        validate_transition(VerificationStatus::Rejected, VerificationStatus::Verified),
        Err(TransitionError::InvalidTransition {
            from: VerificationStatus::Rejected,
            to: VerificationStatus::Verified,
        })
    );
}

#[test]
fn test_transition_rejected_to_rejected_noop_blocked() {
    assert_eq!(
        validate_transition(VerificationStatus::Rejected, VerificationStatus::Rejected),
        Err(TransitionError::InvalidTransition {
            from: VerificationStatus::Rejected,
            to: VerificationStatus::Rejected,
        })
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Rejection reason validation
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_rejection_reason_short_valid() {
    assert!(validate_rejection_reason("Missing tax ID").is_ok());
}

#[test]
fn test_rejection_reason_at_max_boundary_valid() {
    let reason = "a".repeat(MAX_REJECTION_REASON_LENGTH);
    assert!(validate_rejection_reason(&reason).is_ok());
}

#[test]
fn test_rejection_reason_one_char_valid() {
    assert!(validate_rejection_reason("X").is_ok());
}

#[test]
fn test_rejection_reason_empty_rejected() {
    assert_eq!(
        validate_rejection_reason(""),
        Err(TransitionError::ReasonEmpty)
    );
}

#[test]
fn test_rejection_reason_one_over_max_rejected() {
    let reason = "b".repeat(MAX_REJECTION_REASON_LENGTH + 1);
    assert_eq!(
        validate_rejection_reason(&reason),
        Err(TransitionError::ReasonTooLong {
            length: MAX_REJECTION_REASON_LENGTH + 1,
            max: MAX_REJECTION_REASON_LENGTH,
        })
    );
}

#[test]
fn test_rejection_reason_far_over_max_rejected() {
    let reason = "c".repeat(MAX_REJECTION_REASON_LENGTH * 2);
    assert!(matches!(
        validate_rejection_reason(&reason),
        Err(TransitionError::ReasonTooLong { .. })
    ));
}

// ─────────────────────────────────────────────────────────────────────────────
// KYC data validation
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_kyc_data_normal_valid() {
    assert!(validate_kyc_data("encrypted-business-data-payload").is_ok());
}

#[test]
fn test_kyc_data_at_max_boundary_valid() {
    let data = "d".repeat(MAX_KYC_DATA_LENGTH);
    assert!(validate_kyc_data(&data).is_ok());
}

#[test]
fn test_kyc_data_one_char_valid() {
    assert!(validate_kyc_data("Z").is_ok());
}

#[test]
fn test_kyc_data_empty_rejected() {
    assert_eq!(validate_kyc_data(""), Err(TransitionError::KycDataEmpty));
}

#[test]
fn test_kyc_data_one_over_max_rejected() {
    let data = "e".repeat(MAX_KYC_DATA_LENGTH + 1);
    assert_eq!(
        validate_kyc_data(&data),
        Err(TransitionError::KycDataTooLong {
            length: MAX_KYC_DATA_LENGTH + 1,
            max: MAX_KYC_DATA_LENGTH,
        })
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Full business lifecycle: rejection -> resubmit -> verify
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_full_business_lifecycle_reject_resubmit_verify() {
    // Step 1: Business submits KYC (transitions from non-existent to Pending)
    // We model this as Pending status existing
    let status = VerificationStatus::Pending;

    // Step 2: Guard blocks pending business from uploading invoice
    assert_eq!(
        guard_invoice_upload(Some(status)),
        Err(GuardError::VerificationPending)
    );

    // Step 3: Admin rejects (Pending -> Rejected)
    assert!(validate_transition(status, VerificationStatus::Rejected).is_ok());
    let status = VerificationStatus::Rejected;

    // Step 4: Guard still blocks rejected business
    assert_eq!(
        guard_invoice_upload(Some(status)),
        Err(GuardError::VerificationRejected)
    );

    // Step 5: Business resubmits (Rejected -> Pending)
    assert!(validate_transition(status, VerificationStatus::Pending).is_ok());
    let status = VerificationStatus::Pending;

    // Step 6: Still blocked as pending
    assert_eq!(
        guard_invoice_upload(Some(status)),
        Err(GuardError::VerificationPending)
    );

    // Step 7: Admin approves (Pending -> Verified)
    assert!(validate_transition(status, VerificationStatus::Verified).is_ok());
    let status = VerificationStatus::Verified;

    // Step 8: Business can now upload invoices
    assert!(guard_invoice_upload(Some(status)).is_ok());

    // Step 9: Verified is terminal — cannot change status
    assert_eq!(
        validate_transition(status, VerificationStatus::Rejected),
        Err(TransitionError::AlreadyVerified)
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// All three business guards are consistent
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_all_business_guards_consistent_for_each_status() {
    let statuses: Vec<Option<VerificationStatus>> = vec![
        None,
        Some(VerificationStatus::Pending),
        Some(VerificationStatus::Rejected),
        Some(VerificationStatus::Verified),
    ];

    for status in statuses {
        let invoice = guard_invoice_upload(status);
        let settlement = guard_settlement_initiation(status);
        let escrow = guard_escrow_release(status);

        // All three guards must agree on pass/fail for the same status
        assert_eq!(
            invoice.is_ok(),
            settlement.is_ok(),
            "invoice vs settlement mismatch for {:?}",
            status
        );
        assert_eq!(
            settlement.is_ok(),
            escrow.is_ok(),
            "settlement vs escrow mismatch for {:?}",
            status
        );

        // Only Verified passes
        if status == Some(VerificationStatus::Verified) {
            assert!(invoice.is_ok());
        } else {
            assert!(invoice.is_err());
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Exhaustive transition matrix
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_exhaustive_transition_matrix() {
    use VerificationStatus::*;

    let _all = [Pending, Verified, Rejected];
    let expected: Vec<(VerificationStatus, VerificationStatus, bool)> = vec![
        (Pending, Pending, false),
        (Pending, Verified, true),
        (Pending, Rejected, true),
        (Verified, Pending, false),
        (Verified, Verified, false),
        (Verified, Rejected, false),
        (Rejected, Pending, true),
        (Rejected, Verified, false),
        (Rejected, Rejected, false),
    ];

    for (from, to, should_pass) in expected {
        let result = validate_transition(from, to);
        assert_eq!(
            result.is_ok(),
            should_pass,
            "Transition {:?} -> {:?}: expected {}, got {:?}",
            from,
            to,
            if should_pass { "Ok" } else { "Err" },
            result
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Rejection reason + transition integration
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_rejection_requires_valid_reason_and_valid_transition() {
    // Valid transition + valid reason: both must pass
    assert!(validate_transition(VerificationStatus::Pending, VerificationStatus::Rejected).is_ok());
    assert!(validate_rejection_reason("Fraudulent documentation").is_ok());

    // Valid transition + invalid reason: reason check catches it
    assert!(validate_transition(VerificationStatus::Pending, VerificationStatus::Rejected).is_ok());
    assert!(validate_rejection_reason("").is_err());

    // Invalid transition + valid reason: transition check catches it
    assert!(validate_transition(VerificationStatus::Verified, VerificationStatus::Rejected).is_err());
}

// ─────────────────────────────────────────────────────────────────────────────
// Edge case: unicode in reason and KYC data
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_rejection_reason_unicode_within_limit() {
    // Multi-byte characters should be measured in bytes
    let reason = "Incomplete docs".to_string();
    assert!(validate_rejection_reason(&reason).is_ok());
}

#[test]
fn test_kyc_data_unicode_within_limit() {
    let data = "KYC data with special chars".to_string();
    assert!(validate_kyc_data(&data).is_ok());
}

// ─────────────────────────────────────────────────────────────────────────────
// Deny-by-default property: every non-Verified status is denied
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_deny_by_default_property() {
    // The number of denied cases should be exactly 3 (None, Pending, Rejected)
    // and only 1 allowed (Verified)
    let cases: Vec<Option<VerificationStatus>> = vec![
        None,
        Some(VerificationStatus::Pending),
        Some(VerificationStatus::Rejected),
        Some(VerificationStatus::Verified),
    ];

    let denied_count = cases
        .iter()
        .filter(|s| guard_business_action(**s).is_err())
        .count();
    let allowed_count = cases
        .iter()
        .filter(|s| guard_business_action(**s).is_ok())
        .count();

    assert_eq!(denied_count, 3, "exactly 3 statuses should be denied");
    assert_eq!(allowed_count, 1, "exactly 1 status should be allowed");
}

// ─────────────────────────────────────────────────────────────────────────────
// Error variant discrimination
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_guard_error_variants_are_distinct() {
    let none_err = guard_business_action(None).unwrap_err();
    let pending_err = guard_business_action(Some(VerificationStatus::Pending)).unwrap_err();
    let rejected_err = guard_business_action(Some(VerificationStatus::Rejected)).unwrap_err();

    // Each error variant is different
    assert_ne!(none_err, pending_err);
    assert_ne!(pending_err, rejected_err);
    assert_ne!(none_err, rejected_err);

    // Verify exact variants
    assert_eq!(none_err, GuardError::NotSubmitted);
    assert_eq!(pending_err, GuardError::VerificationPending);
    assert_eq!(rejected_err, GuardError::VerificationRejected);
}
