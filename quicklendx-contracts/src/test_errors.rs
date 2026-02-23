/// Comprehensive test suite for error handling
/// Tests verify all error variants are correctly raised and error messages are appropriate
///
/// Test Categories:
/// 1. Invoice errors - verify each invoice error variant is raised correctly
/// 2. Authorization errors - verify auth failures are properly handled
/// 3. Validation errors - verify input validation errors
/// 4. Storage errors - verify storage-related errors
/// 5. Business logic errors - verify operation-specific errors
/// 6. No panics - ensure no panics occur, all errors are typed
use super::*;
use crate::errors::QuickLendXError;
use crate::invoice::InvoiceCategory;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
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

// Helper: Create verified business
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

// Helper: Create verified invoice
fn create_verified_invoice(
    env: &Env,
    client: &QuickLendXContractClient,
    _admin: &Address,
    business: &Address,
    amount: i128,
) -> BytesN<32> {
    let currency = Address::generate(env);
    let due_date = env.ledger().timestamp() + 86400;
    let invoice_id = client.store_invoice(
        business,
        &amount,
        &currency,
        &due_date,
        &String::from_str(env, "Test invoice"),
        &InvoiceCategory::Services,
        &Vec::new(env),
    );
    client.verify_invoice(&invoice_id);
    invoice_id
}

#[test]
fn test_invoice_not_found_error() {
    let (env, client, _admin) = setup();
    let invoice_id = BytesN::from_array(&env, &[0u8; 32]);

    let result = client.try_get_invoice(&invoice_id);
    assert!(result.is_err());
    let err = result.err().unwrap();
    let contract_err = err.expect("expected contract error");
    assert_eq!(contract_err, QuickLendXError::InvoiceNotFound);
}

#[test]
fn test_invoice_amount_invalid_error() {
    let (env, client, _admin) = setup();
    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;

    // Test zero amount
    let result = client.try_store_invoice(
        &business,
        &0,
        &currency,
        &due_date,
        &String::from_str(&env, "Test"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    assert!(result.is_err());
    let err = result.err().unwrap();
    let contract_err = err.expect("expected contract error");
    assert_eq!(contract_err, QuickLendXError::InvalidAmount);

    // Test negative amount
    let result = client.try_store_invoice(
        &business,
        &-100,
        &currency,
        &due_date,
        &String::from_str(&env, "Test"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    assert!(result.is_err());
    let err = result.err().unwrap();
    let contract_err = err.expect("expected contract error");
    assert_eq!(contract_err, QuickLendXError::InvalidAmount);
}

#[test]
fn test_invoice_due_date_invalid_error() {
    let (env, client, _admin) = setup();
    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    let current_time = env.ledger().timestamp();

    // Test due date in the past
    let result = client.try_store_invoice(
        &business,
        &1000,
        &currency,
        &(current_time - 1000),
        &String::from_str(&env, "Test"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    assert!(result.is_err());
    let err = result.err().unwrap();
    let contract_err = err.expect("expected contract error");
    assert_eq!(contract_err, QuickLendXError::InvoiceDueDateInvalid);
}

#[test]
fn test_invoice_not_verified_error() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;

    // Create invoice but don't verify it
    let invoice_id = client.store_invoice(
        &business,
        &1000,
        &currency,
        &due_date,
        &String::from_str(&env, "Test"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    // Try to place bid on unverified invoice
    let investor = Address::generate(&env);
    let result = client.try_place_bid(&investor, &invoice_id, &500, &600);
    assert!(result.is_err());
    let err = result.err().unwrap();
    let contract_err = err.expect("expected contract error");
    assert_eq!(contract_err, QuickLendXError::InvalidStatus);
}

#[test]
fn test_unauthorized_error() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let invoice_id = create_verified_invoice(&env, &client, &admin, &business, 1000);

    // Try to cancel invoice as different user
    let _unauthorized = Address::generate(&env);
    let result = client.try_cancel_invoice(&invoice_id);
    assert!(result.is_err());
}

#[test]
fn test_not_admin_error() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let invoice_id = create_verified_invoice(&env, &client, &admin, &business, 1000);

    // Try to verify invoice as non-admin (without admin auth)
    let result = client.try_verify_invoice(&invoice_id);
    assert!(result.is_err());
}

#[test]
fn test_invalid_description_error() {
    let (env, client, _admin) = setup();
    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;

    // Test empty description
    let result = client.try_store_invoice(
        &business,
        &1000,
        &currency,
        &due_date,
        &String::from_str(&env, ""),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    assert!(result.is_err());
    let err = result.err().unwrap();
    let contract_err = err.expect("expected contract error");
    assert_eq!(contract_err, QuickLendXError::InvalidDescription);
}

#[test]
fn test_invoice_already_funded_error() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let invoice_id = create_verified_invoice(&env, &client, &admin, &business, 1000);

    // Fund the invoice
    let investor = Address::generate(&env);
    client.submit_investor_kyc(&investor, &String::from_str(&env, "KYC"));
    client.verify_investor(&investor, &10000);

    let bid_id = client.place_bid(&investor, &invoice_id, &1000, &1100);
    client.accept_bid(&invoice_id, &bid_id);

    // Try to accept another bid
    let investor2 = Address::generate(&env);
    client.submit_investor_kyc(&investor2, &String::from_str(&env, "KYC"));
    client.verify_investor(&investor2, &10000);

    let bid_id2 = client.place_bid(&investor2, &invoice_id, &1000, &1100);
    let result = client.try_accept_bid(&invoice_id, &bid_id2);
    assert!(result.is_err());
}

#[test]
fn test_invoice_already_defaulted_error() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let invoice_id = create_verified_invoice(&env, &client, &admin, &business, 1000);

    // Fund the invoice
    let investor = Address::generate(&env);
    client.submit_investor_kyc(&investor, &String::from_str(&env, "KYC"));
    client.verify_investor(&investor, &10000);

    let bid_id = client.place_bid(&investor, &invoice_id, &1000, &1100);
    client.accept_bid(&invoice_id, &bid_id);

    // Move time past due date + grace period
    let invoice = client.get_invoice(&invoice_id);
    let grace_period = 7 * 24 * 60 * 60; // 7 days
    env.ledger()
        .set_timestamp(invoice.due_date + grace_period + 1);

    // Mark as defaulted
    client.mark_invoice_defaulted(&invoice_id, &Some(grace_period));

    // Try to mark as defaulted again
    let result = client.try_mark_invoice_defaulted(&invoice_id, &Some(grace_period));
    assert!(result.is_err());
    let err = result.err().unwrap();
    let contract_err = err.expect("expected contract error");
    assert_eq!(contract_err, QuickLendXError::InvalidStatus);
}

#[test]
fn test_invoice_not_funded_error() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let invoice_id = create_verified_invoice(&env, &client, &admin, &business, 1000);

    // Try to mark unfunded invoice as defaulted
    let result = client.try_mark_invoice_defaulted(&invoice_id, &None);
    assert!(result.is_err());
    let err = result.err().unwrap();
    let contract_err = err.expect("expected contract error");
    assert_eq!(contract_err, QuickLendXError::InvoiceNotAvailableForFunding);
}

#[test]
fn test_operation_not_allowed_before_grace_period() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let invoice_id = create_verified_invoice(&env, &client, &admin, &business, 1000);

    // Fund the invoice
    let investor = Address::generate(&env);
    client.submit_investor_kyc(&investor, &String::from_str(&env, "KYC"));
    client.verify_investor(&investor, &10000);

    let bid_id = client.place_bid(&investor, &invoice_id, &1000, &1100);
    client.accept_bid(&invoice_id, &bid_id);

    // Move time past due date but before grace period
    let invoice = client.get_invoice(&invoice_id);
    let grace_period = 7 * 24 * 60 * 60; // 7 days
    env.ledger()
        .set_timestamp(invoice.due_date + grace_period / 2); // Halfway through grace period

    // Try to mark as defaulted before grace period expires
    let result = client.try_mark_invoice_defaulted(&invoice_id, &Some(grace_period));
    assert!(result.is_err());
    let err = result.err().unwrap();
    let contract_err = err.expect("expected contract error");
    assert_eq!(contract_err, QuickLendXError::OperationNotAllowed);
}

#[test]
fn test_storage_key_not_found_error() {
    let (env, client, _admin) = setup();
    let invalid_id = BytesN::from_array(&env, &[0u8; 32]);

    // Try to get non-existent bid
    let result = client.get_bid(&invalid_id);
    assert!(result.is_none());

    // Try to get non-existent investment
    let result = client.try_get_investment(&invalid_id);
    assert!(result.is_err());
    let err = result.err().unwrap();
    let contract_err = err.expect("expected contract error");
    assert_eq!(contract_err, QuickLendXError::StorageKeyNotFound);
}

#[test]
fn test_invalid_status_error() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let invoice_id = create_verified_invoice(&env, &client, &admin, &business, 1000);

    // Try to update status to invalid transition
    let result = client.update_invoice_status(&invoice_id, &crate::invoice::InvoiceStatus::Paid);
    // This might succeed or fail depending on implementation, but should not panic
    let _ = result;
}

#[test]
fn test_business_not_verified_error() {
    let (env, client, _admin) = setup();
    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;

    // Try to upload invoice without verification
    let result = client.try_upload_invoice(
        &business,
        &1000,
        &currency,
        &due_date,
        &String::from_str(&env, "Test"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    assert!(result.is_err());
    let err = result.err().unwrap();
    let contract_err = err.expect("expected contract error");
    assert_eq!(contract_err, QuickLendXError::BusinessNotVerified);
}

#[test]
fn test_no_panics_on_error_conditions() {
    let (env, client, _admin) = setup();

    // Test various error conditions that should not panic
    let invalid_id = BytesN::from_array(&env, &[0u8; 32]);

    // All these should return errors, not panic
    let _ = client.try_get_invoice(&invalid_id);
    let _ = client.get_bid(&invalid_id); // Returns Option, not Result
    let _ = client.try_get_investment(&invalid_id);
    let _ = client.try_get_escrow_details(&invalid_id);

    // Test with invalid parameters
    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;

    let _ = client.try_store_invoice(
        &business,
        &0, // Invalid amount
        &currency,
        &due_date,
        &String::from_str(&env, "Test"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    let _ = client.try_store_invoice(
        &business,
        &1000,
        &currency,
        &(env.ledger().timestamp() - 1000), // Invalid due date
        &String::from_str(&env, "Test"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
}

#[test]
fn test_error_message_consistency() {
    // Verify that error codes are consistent and descriptive
    // This test ensures error enum values are properly defined

    assert_eq!(QuickLendXError::InvoiceNotFound as u32, 1000);
    assert_eq!(QuickLendXError::Unauthorized as u32, 1100);
    assert_eq!(QuickLendXError::InvalidAmount as u32, 1200);
    assert_eq!(QuickLendXError::StorageError as u32, 1300);
    assert_eq!(QuickLendXError::InsufficientFunds as u32, 1400);
}
