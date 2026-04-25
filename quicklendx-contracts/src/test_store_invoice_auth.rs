/// # store_invoice Authentication Policy Tests (Issue #790)
///
/// This module locks the intended authentication and KYC-gating policy for
/// `store_invoice`. Every test here is a **policy regression test**: if the
/// policy changes without updating these tests, CI will fail.
///
/// ## Policy under test
///
/// `store_invoice` requires **both**:
/// 1. A valid Soroban authorization from the `business` address.
/// 2. A `Verified` KYC record for that business.
///
/// ## Security invariants validated
/// - Unverified businesses cannot create invoices (storage DoS prevention).
/// - Pending businesses are explicitly blocked with `KYCAlreadyPending`.
/// - Rejected businesses are blocked with `BusinessNotVerified`.
/// - No KYC record → `BusinessNotVerified`.
/// - Admin cannot bypass the business signature requirement.
/// - A third party cannot create invoices on behalf of a business.
/// - Only after KYC approval can a business write invoice data on-chain.
#![cfg(test)]

use crate::errors::QuickLendXError;
use crate::invoice::InvoiceCategory;
use crate::verification::BusinessVerificationStatus;
use crate::QuickLendXContract;
use soroban_sdk::{
    testutils::{Address as _, MockAuth, MockAuthInvoke},
    Address, Bytes, Env, IntoVal, Vec,
};

type Client<'a> = crate::QuickLendXContractClient<'a>;

// ============================================================================
// Test helpers
// ============================================================================

/// Minimal environment: contract registered, admin set, all auths mocked.
fn setup() -> (Env, Client<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let cid = env.register(QuickLendXContract, ());
    let client = Client::new(&env, &cid);
    let admin = Address::generate(&env);
    client.set_admin(&admin);
    (env, client, admin)
}

/// Create a business that has submitted KYC but is still **Pending**.
fn pending_business(env: &Env, client: &Client) -> Address {
    let business = Address::generate(env);
    let kyc = Bytes::from_slice(env, b"pending-kyc-data");
    client.submit_kyc_application(&business, &kyc);
    business
}

/// Create a business that is fully **Verified**.
fn verified_business(env: &Env, client: &Client, admin: &Address) -> Address {
    let business = Address::generate(env);
    let kyc = Bytes::from_slice(env, b"verified-kyc-data");
    client.submit_kyc_application(&business, &kyc);
    client.verify_business(admin, &business);
    business
}

/// Create a business that has been **Rejected**.
fn rejected_business(env: &Env, client: &Client, admin: &Address) -> Address {
    let business = Address::generate(env);
    let kyc = Bytes::from_slice(env, b"rejected-kyc-data");
    let reason = Bytes::from_slice(env, b"Fraudulent documents");
    client.submit_kyc_application(&business, &kyc);
    client.reject_business(admin, &business, &reason);
    business
}

/// Minimal valid invoice parameters (currency is a dummy address; no
/// whitelisting is enforced by `store_invoice` itself).
fn invoice_params(env: &Env) -> (i128, Address, u64, Bytes, InvoiceCategory, Vec<Bytes>) {
    let amount = 1_000i128;
    let currency = Address::generate(env);
    let due_date = env.ledger().timestamp() + 86_400;
    let description = Bytes::from_slice(env, b"Test invoice description");
    let category = InvoiceCategory::Services;
    let tags = Vec::new(env);
    (amount, currency, due_date, description, category, tags)
}

// ============================================================================
// POLICY LAYER 1 — Business signature requirement
// ============================================================================

/// A verified business that signs the transaction can create an invoice.
/// This is the happy-path baseline for the auth policy.
#[test]
fn test_verified_business_with_auth_can_store_invoice() {
    let (env, client, admin) = setup();
    let business = verified_business(&env, &client, &admin);
    let (amount, currency, due_date, description, category, tags) = invoice_params(&env);

    let result = client.try_store_invoice(
        &business,
        &amount,
        &currency,
        &due_date,
        &description,
        &category,
        &tags,
    );
    assert!(
        result.is_ok(),
        "Verified business with auth must succeed: {:?}",
        result.err()
    );

    let invoice_id = result.unwrap();
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.business, business);
    assert_eq!(invoice.amount, amount);
    assert_eq!(invoice.currency, currency);
    assert_eq!(invoice.due_date, due_date);
    assert_eq!(invoice.description, description);
    assert_eq!(invoice.status, crate::invoice::InvoiceStatus::Pending);
}

/// A third party (not the business) must not be able to create an invoice
/// even if they provide their own valid signature.
///
/// This test bypasses `mock_all_auths` and explicitly mocks only the
/// attacker's signature to verify the contract checks the *business* address.
#[test]
fn test_third_party_cannot_store_invoice_for_another_business() {
    let env = Env::default();
    let cid = env.register(QuickLendXContract, ());
    let client = Client::new(&env, &cid);

    // Set up admin and verified business using mock_all_auths temporarily.
    env.mock_all_auths();
    let admin = Address::generate(&env);
    client.set_admin(&admin);
    let business = verified_business(&env, &client, &admin);

    // Now switch to targeted auth mocking — only the attacker signs.
    let attacker = Address::generate(&env);
    let (amount, currency, due_date, description, category, tags) = invoice_params(&env);

    env.mock_auths(&[MockAuth {
        address: &attacker,
        invoke: &MockAuthInvoke {
            contract: &cid,
            fn_name: "store_invoice",
            args: (
                business.clone(),
                amount,
                currency.clone(),
                due_date,
                description.clone(),
                category.clone(),
                tags.clone(),
            )
                .into_val(&env),
            sub_invokes: &[],
        },
    }]);

    let result = client.try_store_invoice(
        &business,
        &amount,
        &currency,
        &due_date,
        &description,
        &category,
        &tags,
    );
    assert!(
        result.is_err(),
        "Third-party signature must not satisfy business.require_auth()"
    );
}

/// The admin address must not be able to create invoices on behalf of a
/// business. Admin privileges are scoped to admin operations only.
#[test]
fn test_admin_cannot_bypass_business_auth_for_store_invoice() {
    let env = Env::default();
    let cid = env.register(QuickLendXContract, ());
    let client = Client::new(&env, &cid);

    env.mock_all_auths();
    let admin = Address::generate(&env);
    client.set_admin(&admin);
    let business = verified_business(&env, &client, &admin);

    // Only mock the admin's auth — not the business's.
    let (amount, currency, due_date, description, category, tags) = invoice_params(&env);

    env.mock_auths(&[MockAuth {
        address: &admin,
        invoke: &MockAuthInvoke {
            contract: &cid,
            fn_name: "store_invoice",
            args: (
                business.clone(),
                amount,
                currency.clone(),
                due_date,
                description.clone(),
                category.clone(),
                tags.clone(),
            )
                .into_val(&env),
            sub_invokes: &[],
        },
    }]);

    let result = client.try_store_invoice(
        &business,
        &amount,
        &currency,
        &due_date,
        &description,
        &category,
        &tags,
    );
    assert!(
        result.is_err(),
        "Admin signature alone must not satisfy business.require_auth()"
    );
}

// ============================================================================
// POLICY LAYER 2 — KYC gating
// ============================================================================

/// A business with **no KYC record** must receive `BusinessNotVerified`.
/// This is the baseline anti-spam gate: unknown addresses cannot write storage.
#[test]
fn test_no_kyc_record_returns_business_not_verified() {
    let (env, client, _admin) = setup();
    let business = Address::generate(&env); // no KYC submitted
    let (amount, currency, due_date, description, category, tags) = invoice_params(&env);

    let result = client.try_store_invoice(
        &business,
        &amount,
        &currency,
        &due_date,
        &description,
        &category,
        &tags,
    );

    assert!(result.is_err(), "Business with no KYC must be rejected");
    assert_eq!(
        result.unwrap_err().unwrap(),
        QuickLendXError::BusinessNotVerified,
        "Expected BusinessNotVerified for unknown business"
    );
}

/// A business with a **Pending** KYC application must receive
/// `KYCAlreadyPending`. This distinct error lets callers differentiate
/// "not yet approved" from "rejected/unknown", and prevents unvetted
/// entities from spamming on-chain storage while awaiting review.
#[test]
fn test_pending_kyc_returns_kyc_already_pending() {
    let (env, client, _admin) = setup();
    let business = pending_business(&env, &client);
    let (amount, currency, due_date, description, category, tags) = invoice_params(&env);

    let result = client.try_store_invoice(
        &business,
        &amount,
        &currency,
        &due_date,
        &description,
        &category,
        &tags,
    );

    assert!(result.is_err(), "Pending business must be blocked");
    assert_eq!(
        result.unwrap_err().unwrap(),
        QuickLendXError::KYCAlreadyPending,
        "Expected KYCAlreadyPending for pending business"
    );
}

/// A business whose KYC was **Rejected** must receive `BusinessNotVerified`.
/// Rejected businesses must resubmit and be re-approved before creating invoices.
#[test]
fn test_rejected_kyc_returns_business_not_verified() {
    let (env, client, admin) = setup();
    let business = rejected_business(&env, &client, &admin);
    let (amount, currency, due_date, description, category, tags) = invoice_params(&env);

    let result = client.try_store_invoice(
        &business,
        &amount,
        &currency,
        &due_date,
        &description,
        &category,
        &tags,
    );

    assert!(result.is_err(), "Rejected business must be blocked");
    assert_eq!(
        result.unwrap_err().unwrap(),
        QuickLendXError::BusinessNotVerified,
        "Expected BusinessNotVerified for rejected business"
    );
}

// ============================================================================
// Anti-spam / storage DoS prevention
// ============================================================================

/// Multiple unverified addresses attempting to create invoices must all fail.
/// This validates that the KYC gate holds under concurrent spam attempts.
#[test]
fn test_multiple_unverified_businesses_all_blocked() {
    let (env, client, _admin) = setup();
    let (amount, currency, due_date, description, category, tags) = invoice_params(&env);

    for _ in 0..5 {
        let spammer = Address::generate(&env);
        let result = client.try_store_invoice(
            &spammer,
            &amount,
            &currency,
            &due_date,
            &description,
            &category,
            &tags,
        );
        assert!(
            result.is_err(),
            "Unverified spammer must not create invoices"
        );
        assert_eq!(
            result.unwrap_err().unwrap(),
            QuickLendXError::BusinessNotVerified
        );
    }
}

/// Multiple pending businesses attempting to create invoices must all fail
/// with `KYCAlreadyPending`, not a generic error.
#[test]
fn test_multiple_pending_businesses_all_blocked_with_correct_error() {
    let (env, client, _admin) = setup();
    let (amount, currency, due_date, description, category, tags) = invoice_params(&env);

    for i in 0..5u8 {
        let business = Address::generate(&env);
        let kyc = Bytes::from_slice(&env, &[b'k', b'y', b'c', i]);
        client.submit_kyc_application(&business, &kyc);

        let result = client.try_store_invoice(
            &business,
            &amount,
            &currency,
            &due_date,
            &description,
            &category,
            &tags,
        );
        assert!(result.is_err(), "Pending spammer must not create invoices");
        assert_eq!(
            result.unwrap_err().unwrap(),
            QuickLendXError::KYCAlreadyPending
        );
    }
}

// ============================================================================
// KYC lifecycle state-machine integration
// ============================================================================

/// Full lifecycle: submit → pending (blocked) → verify → store invoice (ok).
/// This is the canonical happy path and validates the state machine integration.
#[test]
fn test_full_kyc_lifecycle_unlocks_store_invoice() {
    let (env, client, admin) = setup();
    let business = Address::generate(&env);
    let kyc = Bytes::from_slice(&env, b"full-lifecycle-kyc");
    let (amount, currency, due_date, description, category, tags) = invoice_params(&env);

    // Step 1: No KYC → blocked.
    let r1 = client.try_store_invoice(
        &business,
        &amount,
        &currency,
        &due_date,
        &description,
        &category,
        &tags,
    );
    assert_eq!(
        r1.unwrap_err().unwrap(),
        QuickLendXError::BusinessNotVerified,
        "Step 1: no KYC must return BusinessNotVerified"
    );

    // Step 2: Submit KYC → pending → blocked with KYCAlreadyPending.
    client.submit_kyc_application(&business, &kyc);
    let r2 = client.try_store_invoice(
        &business,
        &amount,
        &currency,
        &due_date,
        &description,
        &category,
        &tags,
    );
    assert_eq!(
        r2.unwrap_err().unwrap(),
        QuickLendXError::KYCAlreadyPending,
        "Step 2: pending KYC must return KYCAlreadyPending"
    );

    // Step 3: Admin verifies → store invoice succeeds.
    client.verify_business(&admin, &business);
    let r3 = client.try_store_invoice(
        &business,
        &amount,
        &currency,
        &due_date,
        &description,
        &category,
        &tags,
    );
    assert!(
        r3.is_ok(),
        "Step 3: verified business must succeed: {:?}",
        r3.err()
    );
}

/// Rejection → resubmission → re-verification → store invoice succeeds.
/// Validates that the full rejection/resubmission cycle restores access.
#[test]
fn test_rejection_resubmission_reverification_restores_access() {
    let (env, client, admin) = setup();
    let business = Address::generate(&env);
    let kyc_v1 = Bytes::from_slice(&env, b"initial-kyc-data");
    let kyc_v2 = Bytes::from_slice(&env, b"updated-kyc-data");
    let reason = Bytes::from_slice(&env, b"Incomplete documentation");
    let (amount, currency, due_date, description, category, tags) = invoice_params(&env);

    // Submit → reject.
    client.submit_kyc_application(&business, &kyc_v1);
    client.reject_business(&admin, &business, &reason);

    // Rejected → blocked.
    let r1 = client.try_store_invoice(
        &business,
        &amount,
        &currency,
        &due_date,
        &description,
        &category,
        &tags,
    );
    assert_eq!(
        r1.unwrap_err().unwrap(),
        QuickLendXError::BusinessNotVerified,
        "Rejected business must be blocked"
    );

    // Resubmit → pending → blocked.
    client.submit_kyc_application(&business, &kyc_v2);
    let r2 = client.try_store_invoice(
        &business,
        &amount,
        &currency,
        &due_date,
        &description,
        &category,
        &tags,
    );
    assert_eq!(
        r2.unwrap_err().unwrap(),
        QuickLendXError::KYCAlreadyPending,
        "Resubmitted (pending) business must be blocked"
    );

    // Re-verify → store invoice succeeds.
    client.verify_business(&admin, &business);
    let r3 = client.try_store_invoice(
        &business,
        &amount,
        &currency,
        &due_date,
        &description,
        &category,
        &tags,
    );
    assert!(
        r3.is_ok(),
        "Re-verified business must succeed: {:?}",
        r3.err()
    );
}

// ============================================================================
// Invoice data integrity after successful store
// ============================================================================

/// Verify that all fields are stored exactly as provided.
#[test]
fn test_stored_invoice_fields_match_inputs() {
    let (env, client, admin) = setup();
    let business = verified_business(&env, &client, &admin);

    let amount = 42_000i128;
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 7 * 86_400; // 7 days
    let description = Bytes::from_slice(&env, b"Consulting services Q1 2026");
    let category = InvoiceCategory::Consulting;
    let mut tags = Vec::new(&env);
    tags.push_back(Bytes::from_slice(&env, b"consulting"));
    tags.push_back(Bytes::from_slice(&env, b"q1-2026"));

    let invoice_id = client
        .try_store_invoice(
            &business,
            &amount,
            &currency,
            &due_date,
            &description,
            &category,
            &tags,
        )
        .expect("Verified business must succeed");

    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.business, business, "business mismatch");
    assert_eq!(invoice.amount, amount, "amount mismatch");
    assert_eq!(invoice.currency, currency, "currency mismatch");
    assert_eq!(invoice.due_date, due_date, "due_date mismatch");
    assert_eq!(invoice.description, description, "description mismatch");
    assert_eq!(invoice.category, category, "category mismatch");
    assert_eq!(invoice.tags.len(), 2, "tags count mismatch");
    assert_eq!(
        invoice.status,
        crate::invoice::InvoiceStatus::Pending,
        "initial status must be Pending"
    );
    assert_eq!(invoice.funded_amount, 0, "funded_amount must start at 0");
    assert!(invoice.investor.is_none(), "investor must be None initially");
    assert_eq!(
        invoice.dispute_status,
        crate::invoice::DisputeStatus::None,
        "dispute_status must be None initially"
    );
}

/// Two invoices created by the same verified business must have distinct IDs.
#[test]
fn test_two_invoices_from_same_business_have_distinct_ids() {
    let (env, client, admin) = setup();
    let business = verified_business(&env, &client, &admin);
    let (amount, currency, due_date, description, category, tags) = invoice_params(&env);

    // Advance ledger time between calls to guarantee distinct SHA-256 inputs.
    let id1 = client
        .try_store_invoice(
            &business,
            &amount,
            &currency,
            &due_date,
            &description,
            &category,
            &tags,
        )
        .expect("first invoice must succeed");

    env.ledger().with_mut(|li| li.timestamp += 1);

    let id2 = client
        .try_store_invoice(
            &business,
            &amount,
            &currency,
            &due_date,
            &description,
            &category,
            &tags,
        )
        .expect("second invoice must succeed");

    assert_ne!(id1, id2, "invoice IDs must be unique");
}

/// A verified business's invoices appear in the business invoice index.
#[test]
fn test_stored_invoice_appears_in_business_index() {
    let (env, client, admin) = setup();
    let business = verified_business(&env, &client, &admin);
    let (amount, currency, due_date, description, category, tags) = invoice_params(&env);

    let invoice_id = client
        .try_store_invoice(
            &business,
            &amount,
            &currency,
            &due_date,
            &description,
            &category,
            &tags,
        )
        .expect("must succeed");

    let business_invoices = client.get_business_invoices(&business);
    assert!(
        business_invoices.contains(&invoice_id),
        "invoice must appear in business index"
    );
}

/// A verified business's invoice appears in the Pending status index.
#[test]
fn test_stored_invoice_appears_in_pending_status_index() {
    let (env, client, admin) = setup();
    let business = verified_business(&env, &client, &admin);
    let (amount, currency, due_date, description, category, tags) = invoice_params(&env);

    let invoice_id = client
        .try_store_invoice(
            &business,
            &amount,
            &currency,
            &due_date,
            &description,
            &category,
            &tags,
        )
        .expect("must succeed");

    let pending = client.get_invoice_count_by_status(&crate::invoice::InvoiceStatus::Pending);
    assert!(pending >= 1, "pending count must be at least 1");

    // Verify the specific invoice is retrievable and in Pending state.
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, crate::invoice::InvoiceStatus::Pending);
}

// ============================================================================
// Policy isolation: store_invoice vs upload_invoice
// ============================================================================

/// `upload_invoice` (the business-facing alias) must also enforce KYC gating.
/// This test ensures the policy is not accidentally bypassed via the alias.
#[test]
fn test_upload_invoice_also_requires_verified_kyc() {
    let (env, client, _admin) = setup();
    let business = Address::generate(&env); // no KYC
    let (amount, currency, due_date, description, category, tags) = invoice_params(&env);

    let result = client.try_upload_invoice(
        &business,
        &amount,
        &currency,
        &due_date,
        &description,
        &category,
        &tags,
    );
    assert!(
        result.is_err(),
        "upload_invoice must also enforce KYC gating"
    );
    assert_eq!(
        result.unwrap_err().unwrap(),
        QuickLendXError::BusinessNotVerified
    );
}

/// `upload_invoice` succeeds for a verified business (parity with store_invoice).
#[test]
fn test_upload_invoice_succeeds_for_verified_business() {
    let (env, client, admin) = setup();
    let business = verified_business(&env, &client, &admin);
    let (amount, currency, due_date, description, category, tags) = invoice_params(&env);

    let result = client.try_upload_invoice(
        &business,
        &amount,
        &currency,
        &due_date,
        &description,
        &category,
        &tags,
    );
    assert!(
        result.is_ok(),
        "upload_invoice must succeed for verified business: {:?}",
        result.err()
    );
}

// ============================================================================
// KYC status query correctness
// ============================================================================

/// After verification, the KYC record reflects `Verified` status.
#[test]
fn test_kyc_status_is_verified_after_admin_approval() {
    let (env, client, admin) = setup();
    let business = verified_business(&env, &client, &admin);

    let record = client
        .get_business_verification_status(&business)
        .expect("KYC record must exist");

    assert!(
        matches!(record.status, BusinessVerificationStatus::Verified),
        "KYC status must be Verified after admin approval"
    );
    assert!(record.verified_at.is_some(), "verified_at must be set");
    assert_eq!(record.verified_by, Some(admin), "verified_by must be admin");
}

/// After rejection, the KYC record reflects `Rejected` status.
#[test]
fn test_kyc_status_is_rejected_after_admin_rejection() {
    let (env, client, admin) = setup();
    let business = rejected_business(&env, &client, &admin);

    let record = client
        .get_business_verification_status(&business)
        .expect("KYC record must exist");

    assert!(
        matches!(record.status, BusinessVerificationStatus::Rejected),
        "KYC status must be Rejected"
    );
    assert!(
        record.rejection_reason.is_some(),
        "rejection_reason must be set"
    );
}
