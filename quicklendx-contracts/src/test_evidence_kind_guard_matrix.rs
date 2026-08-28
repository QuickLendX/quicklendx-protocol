/// Matrix coverage for the dispute evidence guard (issue #1975).
///
/// # Regression this locks in
/// Before this change, `QuickLendXContract::create_dispute` (the real,
/// contract-exposed entrypoint in `lib.rs`) never called
/// `validate_dispute_evidence` or `validate_dispute_eligibility` at all - it
/// only rejected an empty `reason`. Both guards existed and were already unit
/// tested (`verification.rs`, `test_evidence_size_cap.rs`), but were wired
/// only into `dispute::create_dispute`, an `#[allow(dead_code)]` function
/// nothing calls. In practice this meant *any* authenticated address could
/// open a dispute, with empty or oversized evidence, on an invoice in *any*
/// status (`Cancelled`, `Refunded`, ...), as long as no dispute already
/// existed. `lib.rs::create_dispute` now calls `validate_dispute_reason`,
/// `validate_dispute_evidence`, and `validate_dispute_eligibility` in that
/// order, matching the order the (unused) `dispute.rs` copy always documented.
///
/// # Terminology mapping
/// This crate has no literal `DisputeType` or `EvidenceKind` enum. The closest
/// real analogues, used throughout this file:
/// - **dispute_type** -> `InvoiceStatus`, the field `validate_dispute_eligibility`
///   gates on to decide whether an invoice may enter the dispute lifecycle.
/// - **evidence_kind** -> the two independent evidence guards in
///   `verification.rs`: free-text evidence (`validate_dispute_evidence`, now
///   wired into `create_dispute`) and hash-referenced evidence
///   (`validate_evidence_hash`, a standalone format guard not currently called
///   from any entrypoint).
///
/// # Coverage
/// 1. `validate_dispute_eligibility` x every `InvoiceStatus` variant (7 total).
/// 2. `create_dispute` x {every publicly-reachable `InvoiceStatus`} x {valid,
///    invalid evidence} - locks in guard *ordering*: `validate_dispute_evidence`
///    runs before `validate_dispute_eligibility`, so invalid evidence always
///    reports `InvalidDisputeEvidence`, even on an invoice whose status alone
///    would already be rejected. Every ineligible-status and invalid-evidence
///    case in this group succeeded (wrongly) against the pre-fix entrypoint;
///    reverting the `lib.rs` change alone makes this test fail.
///
///    `Refunded` is exercised only at the unit level in (1): reaching it
///    requires a full escrow-funding-refund lifecycle with no direct
///    admin/testing entrypoint (unlike `Verified`/`Funded`/`Paid`/`Defaulted`,
///    which `update_invoice_status` forces directly "for testing purposes").
///
/// The hash-kind guard (`validate_evidence_hash`) already has exhaustive
/// boundary coverage in `test_evidence_hash_format.rs`; it is not repeated
/// here since it is not wired into `create_dispute` and has no `InvoiceStatus`
/// dimension to cross against.
#[cfg(test)]
mod test_evidence_kind_guard_matrix {
    use crate::defaults::DEFAULT_GRACE_PERIOD;
    use crate::errors::QuickLendXError;
    use crate::invoice::{Invoice, InvoiceCategory, InvoiceStatus};
    use crate::verification::validate_dispute_eligibility;
    use crate::{QuickLendXContract, QuickLendXContractClient};
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        Address, BytesN, Env, String, Vec,
    };

    /// `InvoiceStatus` values that permit dispute creation.
    const ELIGIBLE: [InvoiceStatus; 4] = [
        InvoiceStatus::Pending,
        InvoiceStatus::Verified,
        InvoiceStatus::Funded,
        InvoiceStatus::Paid,
    ];

    /// `InvoiceStatus` values that must reject dispute creation.
    const INELIGIBLE: [InvoiceStatus; 3] = [
        InvoiceStatus::Defaulted,
        InvoiceStatus::Cancelled,
        InvoiceStatus::Refunded,
    ];

    /// Build a bare `Invoice` value directly (no contract storage involved)
    /// with `status` forced to the requested variant.
    fn build_invoice(env: &Env, business: &Address, status: InvoiceStatus) -> Invoice {
        let currency = Address::generate(env);
        let due_date = env.ledger().timestamp() + 30 * 24 * 60 * 60;
        let mut invoice = Invoice::new(
env,
business.clone(),
100_000,
currency,
due_date,
String::from_str(env, "Matrix test invoice"),
InvoiceCategory::Services,
Vec::new(env),
None,
        None, /* early_payment_discount_bps */
        
)
        .unwrap();
        invoice.status = status;
        invoice
    }

    // ------------------------------------------------------------------
    // Group 1: eligibility guard x every InvoiceStatus ("dispute_type")
    // ------------------------------------------------------------------

    /// [TC-EG-01] Every disputable status is accepted by the eligibility guard.
    #[test]
    fn eligibility_guard_accepts_every_disputable_status() {
        let env = Env::default();
        let business = Address::generate(&env);
        for status in ELIGIBLE {
            let invoice = build_invoice(&env, &business, status);
            assert!(
                validate_dispute_eligibility(&invoice, &business).is_ok(),
                "{:?} must be eligible for dispute creation",
                status
            );
        }
    }

    /// [TC-EG-02] Every non-disputable status is rejected by the eligibility guard.
    #[test]
    fn eligibility_guard_rejects_every_non_disputable_status() {
        let env = Env::default();
        let business = Address::generate(&env);
        for status in INELIGIBLE {
            let invoice = build_invoice(&env, &business, status);
            let err = validate_dispute_eligibility(&invoice, &business).unwrap_err();
            assert_eq!(
                err,
                QuickLendXError::InvoiceNotAvailableForFunding,
                "{:?} must reject dispute creation",
                status
            );
        }
    }

    // ------------------------------------------------------------------
    // Group 2: create_dispute x InvoiceStatus x evidence validity
    // ------------------------------------------------------------------

    fn setup_contract(env: &Env) -> (QuickLendXContractClient<'static>, Address, Address) {
        env.mock_all_auths();
        let contract_id = env.register(QuickLendXContract, ());
        let client = QuickLendXContractClient::new(env, &contract_id);
        let admin = Address::generate(env);
        client.set_admin(&admin);
        let business = Address::generate(env);
        client.submit_kyc_application(&business, &String::from_str(env, "KYC data"));
        client.verify_business(&admin, &business);
        (client, admin, business)
    }

    /// Store a fresh invoice and drive it to `target` via the same
    /// admin/business entrypoints the rest of the suite uses (never by
    /// mutating storage directly), then return its id.
    fn store_invoice_at(
        env: &Env,
        client: &QuickLendXContractClient,
        business: &Address,
        target: InvoiceStatus,
    ) -> BytesN<32> {
        let currency = Address::generate(env);
        let due_date = env.ledger().timestamp() + 30 * 24 * 60 * 60;
        let invoice_id = client.store_invoice(
            business,
            &100_000_i128,
            &currency,
            &due_date,
            &String::from_str(env, "Matrix guard-order invoice"),
            &InvoiceCategory::Services,
            &Vec::new(env),
        );

        match target {
            InvoiceStatus::Pending => {}
            InvoiceStatus::Verified => {
                client.update_invoice_status(&invoice_id, &InvoiceStatus::Verified);
            }
            InvoiceStatus::Funded => {
                client.update_invoice_status(&invoice_id, &InvoiceStatus::Verified);
                client.update_invoice_status(&invoice_id, &InvoiceStatus::Funded);
            }
            InvoiceStatus::Paid => {
                client.update_invoice_status(&invoice_id, &InvoiceStatus::Verified);
                client.update_invoice_status(&invoice_id, &InvoiceStatus::Funded);
                client.update_invoice_status(&invoice_id, &InvoiceStatus::Paid);
            }
            InvoiceStatus::Defaulted => {
                client.update_invoice_status(&invoice_id, &InvoiceStatus::Verified);
                client.update_invoice_status(&invoice_id, &InvoiceStatus::Funded);
                // `mark_invoice_defaulted` only accepts a `Funded` invoice whose
                // grace window has strictly elapsed - advance the ledger past it.
                env.ledger()
                    .set_timestamp(due_date + DEFAULT_GRACE_PERIOD + 1);
                client.update_invoice_status(&invoice_id, &InvoiceStatus::Defaulted);
            }
            InvoiceStatus::Cancelled => {
                client.cancel_invoice(&invoice_id);
            }
            InvoiceStatus::Refunded => {
                panic!("Refunded is not publicly reachable; see Group 1 for unit-level coverage");
            }
        }
        invoice_id
    }

    /// [TC-EG-03] Full status x evidence-validity matrix through the real
    /// `create_dispute` entrypoint.
    ///
    /// Locks in that evidence validation is checked *before* eligibility:
    /// invalid evidence always yields `InvalidDisputeEvidence`, regardless of
    /// whether the invoice's status would independently be rejected.
    #[test]
    fn create_dispute_matrix_status_by_evidence_validity() {
        let env = Env::default();
        let (client, _admin, business) = setup_contract(&env);

        let reason = String::from_str(&env, "Invoice amount discrepancy");
        let valid_evidence = String::from_str(&env, "Supporting documentation provided");
        let empty_evidence = String::from_str(&env, "");

        let reachable_statuses = [
            InvoiceStatus::Pending,
            InvoiceStatus::Verified,
            InvoiceStatus::Funded,
            InvoiceStatus::Paid,
            InvoiceStatus::Defaulted,
            InvoiceStatus::Cancelled,
        ];

        for status in reachable_statuses {
            let is_eligible = ELIGIBLE.contains(&status);

            // valid evidence: outcome depends solely on status eligibility.
            let invoice_id = store_invoice_at(&env, &client, &business, status);
            let result =
                client.try_create_dispute(&invoice_id, &business, &reason, &valid_evidence);
            if is_eligible {
                assert!(
                    result.is_ok(),
                    "{:?} + valid evidence should succeed",
                    status
                );
            } else {
                assert_eq!(
                    result.unwrap_err().unwrap(),
                    QuickLendXError::InvoiceNotAvailableForFunding,
                    "{:?} + valid evidence should fail on eligibility",
                    status
                );
            }

            // invalid (empty) evidence: always rejected by the evidence guard,
            // which runs before the eligibility guard - independent of status.
            let invoice_id = store_invoice_at(&env, &client, &business, status);
            let result =
                client.try_create_dispute(&invoice_id, &business, &reason, &empty_evidence);
            assert_eq!(
                result.unwrap_err().unwrap(),
                QuickLendXError::InvalidDisputeEvidence,
                "{:?} + empty evidence should always fail evidence validation first",
                status
            );
        }
    }
}
