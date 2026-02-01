/// Comprehensive test suite for dispute resolution system
/// Tests verify dispute creation, review, resolution, authorization, and state transitions
///
/// Test Categories:
/// 1. Dispute creation - business can create, unauthorized parties cannot, validation
/// 2. Authorization checks - only stakeholders can create
/// 3. Status transitions - proper state changes through lifecycle (Disputed → UnderReview → Resolved)
/// 4. Duplicate prevention - cannot create multiple disputes per invoice
/// 5. Parameter validation - reason and evidence length constraints
/// 6. Edge cases - empty strings, boundary values
use super::*;
use crate::errors::QuickLendXError;
use crate::invoice::{DisputeStatus, InvoiceCategory};
use soroban_sdk::{
    testutils::{Address as _, BytesN as _},
    Address, BytesN, Env, String, Vec,
};

// Helper: Setup contract with admin
fn setup() -> (Env, QuickLendXContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.set_admin(&admin);
    (env, client, admin)
}

// Helper: Create a verified business
fn create_verified_business(
    env: &Env,
    client: &QuickLendXContractClient,
    admin: &Address,
) -> Address {
    let business = Address::generate(env);
    client.submit_kyc_application(&business, &String::from_str(env, "KYC data"));
    client.verify_business(admin, &business);
    business
}

// Helper: Create a test invoice
fn create_test_invoice(
    env: &Env,
    client: &QuickLendXContractClient,
    business: &Address,
    amount: i128,
) -> BytesN<32> {
    let currency = Address::generate(env);
    let due_date = env.ledger().timestamp() + 30 * 24 * 60 * 60; // 30 days from now

    let invoice_id = client.store_invoice(
        business,
        &amount,
        &currency,
        &due_date,
        &String::from_str(env, "Test invoice for dispute"),
        &InvoiceCategory::Services,
        &Vec::new(env),
    );

    invoice_id
}

/// Test 1: Business can create a dispute
#[test]
fn test_create_dispute_by_business() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let invoice_id = create_test_invoice(&env, &client, &business, 100_000);

    // Business creates dispute
    let reason = String::from_str(&env, "Invoice amount discrepancy");
    let evidence = String::from_str(&env, "Supporting documentation provided");

    let result = client.try_create_dispute(&invoice_id, &business, &reason, &evidence);
    assert!(result.is_ok());

    // Verify dispute was created
    let dispute_result = client.try_get_dispute_details(&invoice_id);
    assert!(dispute_result.is_ok());
    let dispute_opt_result = dispute_result.unwrap();
    assert!(dispute_opt_result.is_ok());
    let dispute_opt = dispute_opt_result.unwrap();
    assert!(dispute_opt.is_some());

    let dispute = dispute_opt.unwrap();
    assert_eq!(dispute.created_by, business);
    assert_eq!(dispute.reason, reason);
    assert_eq!(dispute.evidence, evidence);
    assert_eq!(dispute.resolved_at, 0);
}

/// Test 2: Cannot create dispute for nonexistent invoice
#[test]
fn test_create_dispute_nonexistent_invoice() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let fake_invoice_id = BytesN::from_array(&env, &[0u8; 32]);

    let reason = String::from_str(&env, "Test reason");
    let evidence = String::from_str(&env, "Test evidence");

    let result = client.try_create_dispute(&fake_invoice_id, &business, &reason, &evidence);
    assert!(result.is_err());
}

/// Test 3: Unauthorized party cannot create dispute
#[test]
fn test_create_dispute_unauthorized() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let unauthorized = Address::generate(&env);
    let invoice_id = create_test_invoice(&env, &client, &business, 100_000);

    let reason = String::from_str(&env, "Unauthorized dispute attempt");
    let evidence = String::from_str(&env, "Evidence");

    let result = client.try_create_dispute(&invoice_id, &unauthorized, &reason, &evidence);
    assert!(result.is_err());
    // Should be DisputeNotAuthorized
    let err = result.err().unwrap();
    let contract_err = err.expect("expected contract error");
    assert_eq!(contract_err, QuickLendXError::DisputeNotAuthorized);
}

/// Test 4: Cannot create duplicate disputes per invoice
#[test]
fn test_create_dispute_duplicate() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let invoice_id = create_test_invoice(&env, &client, &business, 100_000);

    // Create first dispute
    let reason1 = String::from_str(&env, "First dispute");
    let evidence1 = String::from_str(&env, "Evidence 1");
    let result1 = client.try_create_dispute(&invoice_id, &business, &reason1, &evidence1);
    assert!(result1.is_ok());

    // Attempt to create second dispute
    let reason2 = String::from_str(&env, "Second dispute");
    let evidence2 = String::from_str(&env, "Evidence 2");
    let result2 = client.try_create_dispute(&invoice_id, &business, &reason2, &evidence2);
    assert!(result2.is_err());
    let err = result2.err().unwrap();
    let contract_err = err.expect("expected contract error");
    assert_eq!(contract_err, QuickLendXError::DisputeAlreadyExists);
}

/// Test 5: Reason validation - minimum length
#[test]
fn test_create_dispute_empty_reason() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let invoice_id = create_test_invoice(&env, &client, &business, 100_000);

    let reason = String::from_str(&env, "");
    let evidence = String::from_str(&env, "Valid evidence");

    let result = client.try_create_dispute(&invoice_id, &business, &reason, &evidence);
    assert!(result.is_err());
    let err = result.err().unwrap();
    let contract_err = err.expect("expected contract error");
    assert_eq!(contract_err, QuickLendXError::InvalidDisputeReason);
}

/// Test 6: Reason validation - maximum length (500 chars)
#[test]
fn test_create_dispute_reason_too_long() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let invoice_id = create_test_invoice(&env, &client, &business, 100_000);

    let long_reason_str = "a".repeat(501);
    let reason = String::from_str(&env, long_reason_str.as_str());
    let evidence = String::from_str(&env, "Valid evidence");

    let result = client.try_create_dispute(&invoice_id, &business, &reason, &evidence);
    assert!(result.is_err());
    let err = result.err().unwrap();
    let contract_err = err.expect("expected contract error");
    assert_eq!(contract_err, QuickLendXError::InvalidDisputeReason);
}

/// Test 7: Evidence validation - minimum length
#[test]
fn test_create_dispute_empty_evidence() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let invoice_id = create_test_invoice(&env, &client, &business, 100_000);

    let reason = String::from_str(&env, "Valid reason");
    let evidence = String::from_str(&env, "");

    let result = client.try_create_dispute(&invoice_id, &business, &reason, &evidence);
    assert!(result.is_err());
    let err = result.err().unwrap();
    let contract_err = err.expect("expected contract error");
    assert_eq!(contract_err, QuickLendXError::InvalidDisputeEvidence);
}

/// Test 8: Evidence validation - maximum length (1000 chars)
#[test]
fn test_create_dispute_evidence_too_long() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let invoice_id = create_test_invoice(&env, &client, &business, 100_000);

    let long_evidence_str = "x".repeat(1001);
    let reason = String::from_str(&env, "Valid reason");
    let evidence = String::from_str(&env, long_evidence_str.as_str());

    let result = client.try_create_dispute(&invoice_id, &business, &reason, &evidence);
    assert!(result.is_err());
    let err = result.err().unwrap();
    let contract_err = err.expect("expected contract error");
    assert_eq!(contract_err, QuickLendXError::InvalidDisputeEvidence);
}

/// Test 9: Put dispute under review - status transition
#[test]
fn test_put_under_review_status_transition() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let invoice_id = create_test_invoice(&env, &client, &business, 100_000);

    // Create dispute
    let reason = String::from_str(&env, "Valid reason");
    let evidence = String::from_str(&env, "Valid evidence");
    client.create_dispute(&invoice_id, &business, &reason, &evidence);

    // Get invoice to verify it's in Disputed status
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.dispute_status, DisputeStatus::Disputed);

    // Put under review
    let result = client.try_put_dispute_under_review(&invoice_id, &admin);
    assert!(result.is_ok());

    // Verify status changed to UnderReview
    let updated_invoice = client.get_invoice(&invoice_id);
    assert_eq!(updated_invoice.dispute_status, DisputeStatus::UnderReview);
}

/// Test 10: Put under review - can only transition from Disputed status
#[test]
fn test_put_under_review_invalid_transition() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let invoice_id = create_test_invoice(&env, &client, &business, 100_000);

    // Attempt to put under review without creating dispute first
    let result = client.try_put_dispute_under_review(&invoice_id, &admin);
    assert!(result.is_err());
    let err = result.err().unwrap();
    let contract_err = err.expect("expected contract error");
    assert_eq!(contract_err, QuickLendXError::DisputeNotFound);
}

/// Test 11: Resolve dispute - complete lifecycle
#[test]
fn test_complete_dispute_lifecycle() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let invoice_id = create_test_invoice(&env, &client, &business, 100_000);

    // Step 1: Create dispute
    let reason = String::from_str(&env, "Service quality issue");
    let evidence = String::from_str(&env, "Documentation attached");
    client.create_dispute(&invoice_id, &business, &reason, &evidence);

    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.dispute_status, DisputeStatus::Disputed);

    // Step 2: Put under review
    client.put_dispute_under_review(&invoice_id, &admin);

    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.dispute_status, DisputeStatus::UnderReview);

    // Step 3: Resolve dispute
    let resolution = String::from_str(&env, "Dispute resolved with partial refund");
    client.resolve_dispute(&invoice_id, &admin, &resolution);

    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.dispute_status, DisputeStatus::Resolved);

    // Verify all dispute details
    let dispute_opt = client.get_dispute_details(&invoice_id);
    assert!(dispute_opt.is_some());
    let dispute = dispute_opt.unwrap();
    assert_eq!(dispute.created_by, business);
    assert_eq!(dispute.reason, reason);
    assert_eq!(dispute.evidence, evidence);
    assert_eq!(dispute.resolution, resolution);
    assert_eq!(dispute.resolved_by, admin);
    // resolved_at should be set to current timestamp or be reasonable (> 0 is implementation dependent)
}

/// Test 12: Resolve dispute - can only transition from UnderReview
#[test]
fn test_resolve_dispute_invalid_transition() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let invoice_id = create_test_invoice(&env, &client, &business, 100_000);

    // Create dispute (status: Disputed)
    let reason = String::from_str(&env, "Test reason");
    let evidence = String::from_str(&env, "Test evidence");
    client.create_dispute(&invoice_id, &business, &reason, &evidence);

    // Attempt to resolve without putting under review first
    let resolution = String::from_str(&env, "Resolution attempt");
    let result = client.try_resolve_dispute(&invoice_id, &admin, &resolution);
    assert!(result.is_err());
    let err = result.err().unwrap();
    let contract_err = err.expect("expected contract error");
    assert_eq!(contract_err, QuickLendXError::DisputeNotUnderReview);
}

/// Test 13: Resolve dispute - resolution validation (empty)
#[test]
fn test_resolve_dispute_empty_resolution() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let invoice_id = create_test_invoice(&env, &client, &business, 100_000);

    // Create and move to under review
    let reason = String::from_str(&env, "Test reason");
    let evidence = String::from_str(&env, "Test evidence");
    client.create_dispute(&invoice_id, &business, &reason, &evidence);
    client.put_dispute_under_review(&invoice_id, &admin);

    // Attempt to resolve with empty resolution
    let resolution = String::from_str(&env, "");
    let result = client.try_resolve_dispute(&invoice_id, &admin, &resolution);
    assert!(result.is_err());
    let err = result.err().unwrap();
    let contract_err = err.expect("expected contract error");
    assert_eq!(contract_err, QuickLendXError::InvalidDisputeReason);
}

/// Test 14: Resolve dispute - resolution validation (too long)
#[test]
fn test_resolve_dispute_resolution_too_long() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let invoice_id = create_test_invoice(&env, &client, &business, 100_000);

    // Create and move to under review
    let reason = String::from_str(&env, "Test reason");
    let evidence = String::from_str(&env, "Test evidence");
    client.create_dispute(&invoice_id, &business, &reason, &evidence);
    client.put_dispute_under_review(&invoice_id, &admin);

    // Attempt to resolve with overly long resolution
    let long_resolution_str = "r".repeat(501);
    let resolution = String::from_str(&env, long_resolution_str.as_str());
    let result = client.try_resolve_dispute(&invoice_id, &admin, &resolution);
    assert!(result.is_err());
    let err = result.err().unwrap();
    let contract_err = err.expect("expected contract error");
    assert_eq!(contract_err, QuickLendXError::InvalidDisputeReason);
}

/// Test 15: Query dispute when none exists
#[test]
fn test_query_dispute_none_exists() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let invoice_id = create_test_invoice(&env, &client, &business, 100_000);

    // Query without creating a dispute
    let result = client.try_get_dispute_details(&invoice_id);
    assert!(result.is_ok());
    let dispute_opt = result.unwrap();
    assert!(dispute_opt.is_ok());
    assert!(dispute_opt.unwrap().is_none());
}

/// Test 16: Multiple disputes on different invoices
#[test]
fn test_multiple_disputes_different_invoices() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);

    // Create two invoices
    let invoice_id_1 = create_test_invoice(&env, &client, &business, 100_000);
    let invoice_id_2 = create_test_invoice(&env, &client, &business, 150_000);

    // Create disputes on both
    let reason1 = String::from_str(&env, "Dispute 1");
    let evidence1 = String::from_str(&env, "Evidence 1");
    client.create_dispute(&invoice_id_1, &business, &reason1, &evidence1);

    let reason2 = String::from_str(&env, "Dispute 2");
    let evidence2 = String::from_str(&env, "Evidence 2");
    client.create_dispute(&invoice_id_2, &business, &reason2, &evidence2);

    // Verify both disputes exist independently
    let dispute1_result = client.try_get_dispute_details(&invoice_id_1).unwrap();
    let dispute2_result = client.try_get_dispute_details(&invoice_id_2).unwrap();

    assert!(dispute1_result.is_ok());
    assert!(dispute2_result.is_ok());

    let dispute1_opt = dispute1_result.unwrap();
    let dispute2_opt = dispute2_result.unwrap();

    assert!(dispute1_opt.is_some());
    assert!(dispute2_opt.is_some());

    let dispute1 = dispute1_opt.unwrap();
    let dispute2 = dispute2_opt.unwrap();
    assert_eq!(dispute1.created_by, business);
    assert_eq!(dispute2.created_by, business);

    // Verify one invoice's dispute status doesn't affect the other
    client.put_dispute_under_review(&invoice_id_1, &admin);

    let invoice1 = client.get_invoice(&invoice_id_1);
    let invoice2 = client.get_invoice(&invoice_id_2);

    assert_eq!(invoice1.dispute_status, DisputeStatus::UnderReview);
    assert_eq!(invoice2.dispute_status, DisputeStatus::Disputed);
}

/// Test 17: Boundary test - reason exactly 1 character
#[test]
fn test_create_dispute_reason_boundary_min() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let invoice_id = create_test_invoice(&env, &client, &business, 100_000);

    let reason = String::from_str(&env, "A");
    let evidence = String::from_str(&env, "Valid evidence");

    let result = client.try_create_dispute(&invoice_id, &business, &reason, &evidence);
    assert!(result.is_ok());
}

/// Test 18: Boundary test - reason exactly 500 characters
#[test]
fn test_create_dispute_reason_boundary_max() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let invoice_id = create_test_invoice(&env, &client, &business, 100_000);

    let reason_str = "x".repeat(500);
    let reason = String::from_str(&env, reason_str.as_str());
    let evidence = String::from_str(&env, "Valid evidence");

    let result = client.try_create_dispute(&invoice_id, &business, &reason, &evidence);
    assert!(result.is_ok());
}
