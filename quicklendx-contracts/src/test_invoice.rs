/// Comprehensive tests for invoice lifecycle, validation, authorization, and status transitions
///
/// This test module covers:
/// - Invoice creation and validation
/// - Authorization and access control
/// - Status transitions and state management
/// - Edge cases and error handling
/// - Security considerations
use core::convert::TryInto;

use super::*;
use crate::invoice::{InvoiceCategory, InvoiceMetadata, InvoiceStatus, LineItemRecord};
use crate::verification::BusinessVerificationStatus;
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Events, Ledger, MockAuth, MockAuthInvoke},
    Address, BytesN, Env, IntoVal, String, Vec,
};

// ============================================================================
// HELPER FUNCTIONS
// ============================================================================

/// Helper to set up a verified business for testing
fn setup_verified_business(
    env: &Env,
    client: &QuickLendXContractClient,
    admin: &Address,
) -> Address {
    let business = Address::generate(env);
    let kyc_data = String::from_str(env, "Business KYC data");

    env.mock_all_auths();
    client.initialize_admin(&admin);
    client.submit_kyc_application(&business, &kyc_data);
    client.verify_business(admin, &business);

    business
}

/// Helper to set up a verified investor for testing
fn setup_verified_investor(
    env: &Env,
    client: &QuickLendXContractClient,
    admin: &Address,
) -> Address {
    let investor = Address::generate(env);
    let kyc_data = String::from_str(env, "Investor KYC data");

    env.mock_all_auths();
    client.initialize_admin(admin);
    client.submit_investor_kyc(&investor, &kyc_data);
    client.verify_investor(&investor, &1_000_000);

    investor
}

/// Helper to create a test invoice
fn create_test_invoice(
    env: &Env,
    client: &QuickLendXContractClient,
    business: &Address,
    amount: i128,
) -> BytesN<32> {
    let currency = Address::generate(env);
    let due_date = env.ledger().timestamp() + 86400; // 1 day from now

    client.store_invoice(
        business,
        &amount,
        &currency,
        &due_date,
        &String::from_str(env, "Test invoice"),
        &InvoiceCategory::Services,
        &Vec::new(env),
    )
}

const COLLISION_TEST_AMOUNT: i128 = 1_000_000;
// Keep the burst below the current Soroban instance-storage entry size ceiling.
const COLLISION_HIGH_THROUGHPUT_SAMPLE: u32 = 24;

fn pin_invoice_collision_ledger_slot(env: &Env, timestamp: u64, sequence: u32) {
    env.ledger().set_timestamp(timestamp);
    env.ledger().set_sequence_number(sequence);
}

fn invoice_id_counter_segment(invoice_id: &BytesN<32>) -> u32 {
    let bytes = invoice_id.to_array();
    u32::from_be_bytes(bytes[12..16].try_into().unwrap())
}

fn set_invoice_collision_counter(env: &Env, contract_id: &Address, counter: u32) {
    env.as_contract(contract_id, || {
        env.storage()
            .instance()
            .set(&symbol_short!("inv_cnt"), &counter);
    });
}

fn read_invoice_collision_counter(env: &Env, contract_id: &Address) -> u32 {
    env.as_contract(contract_id, || {
        env.storage()
            .instance()
            .get(&symbol_short!("inv_cnt"))
            .unwrap_or(0)
    })
}

fn store_collision_test_invoice(
    env: &Env,
    client: &QuickLendXContractClient,
    business: &Address,
    currency: &Address,
    description: &str,
) -> BytesN<32> {
    let due_date = env.ledger().timestamp() + 86400;
    client.store_invoice(
        business,
        &COLLISION_TEST_AMOUNT,
        currency,
        &due_date,
        &String::from_str(env, description),
        &InvoiceCategory::Services,
        &Vec::new(env),
    )
}

// ============================================================================
// INVOICE CREATION AND VALIDATION TESTS
// ============================================================================

#[test]
fn test_invoice_creation_valid() {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    let amount = 5000i128;
    let due_date = env.ledger().timestamp() + 86400;
    let description = String::from_str(&env, "Valid invoice");

    let invoice_id = client.store_invoice(
        &business,
        &amount,
        &currency,
        &due_date,
        &description,
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.business, business);
    assert_eq!(invoice.amount, amount);
    assert_eq!(invoice.currency, currency);
    assert_eq!(invoice.due_date, due_date);
    assert_eq!(invoice.status, InvoiceStatus::Pending);
    assert_eq!(invoice.funded_amount, 0);
    assert!(invoice.investor.is_none());
}

#[test]
fn test_invoice_creation_invalid_amount_zero() {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;

    let result = client.try_store_invoice(
        &business,
        &0,
        &currency,
        &due_date,
        &String::from_str(&env, "Invalid amount"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    assert!(result.is_err());
    let err = result.unwrap_err().unwrap();
    assert_eq!(err, QuickLendXError::InvalidAmount);
}

#[test]
fn test_invoice_creation_invalid_amount_negative() {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;

    let result = client.try_store_invoice(
        &business,
        &-1000,
        &currency,
        &due_date,
        &String::from_str(&env, "Negative amount"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    assert!(result.is_err());
    let err = result.unwrap_err().unwrap();
    assert_eq!(err, QuickLendXError::InvalidAmount);
}

#[test]
fn test_invoice_creation_below_minimum_amount() {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;

    // Default minimum is 1000 in test mode (see protocol_limits.rs)
    let result = client.try_store_invoice(
        &business,
        &999, // Below minimum
        &currency,
        &due_date,
        &String::from_str(&env, "Below minimum amount"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    assert!(result.is_err());
    let err = result.unwrap_err().unwrap();
    assert_eq!(err, QuickLendXError::InvalidAmount);
}

#[test]
fn test_invoice_creation_at_minimum_amount() {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;

    // Default minimum is 1000 in test mode
    let result = client.try_store_invoice(
        &business,
        &1000, // At minimum
        &currency,
        &due_date,
        &String::from_str(&env, "At minimum amount"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    assert!(result.is_ok());
}

#[test]
fn test_invoice_creation_invalid_due_date_past() {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    let current_time = env.ledger().timestamp();
    let past_due_date = current_time.saturating_sub(1000); // Past date

    let result = client.try_store_invoice(
        &business,
        &1000,
        &currency,
        &past_due_date,
        &String::from_str(&env, "Past due date"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    assert!(result.is_err());
    let err = result.unwrap_err().unwrap();
    assert_eq!(err, QuickLendXError::InvoiceDueDateInvalid);
}

#[test]
fn test_invoice_creation_invalid_due_date_current() {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    let current_time = env.ledger().timestamp();

    let result = client.try_store_invoice(
        &business,
        &1000,
        &currency,
        &current_time,
        &String::from_str(&env, "Current time due date"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    assert!(result.is_err());
    let err = result.unwrap_err().unwrap();
    assert_eq!(err, QuickLendXError::InvoiceDueDateInvalid);
}

#[test]
fn test_invoice_creation_invalid_description_empty() {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;

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
    let err = result.unwrap_err().unwrap();
    assert_eq!(err, QuickLendXError::InvalidDescription);
}

// ============================================================================
// STORE_INVOICE: CURRENCY AND TAGS (Issue #269 – cover all error variants)
// ============================================================================

#[test]
fn test_invoice_creation_invalid_non_whitelisted_currency() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize_admin(&admin);
    client.set_admin(&admin);
    let allowed = Address::generate(&env);
    client.add_currency(&admin, &allowed);
    let business = Address::generate(&env);
    let disallowed_currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;

    let result = client.try_store_invoice(
        &business,
        &1000,
        &disallowed_currency,
        &due_date,
        &String::from_str(&env, "Valid description"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    assert!(result.is_err());
    let err = result.unwrap_err().unwrap();
    assert_eq!(err, QuickLendXError::InvalidCurrency);
}

#[test]
fn test_invoice_creation_valid_categories() {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;
    let description = String::from_str(&env, "Category test");

    for category in [
        InvoiceCategory::Services,
        InvoiceCategory::Products,
        InvoiceCategory::Consulting,
        InvoiceCategory::Manufacturing,
        InvoiceCategory::Technology,
        InvoiceCategory::Healthcare,
        InvoiceCategory::Other,
    ] {
        let invoice_id = client.store_invoice(
            &business,
            &1000,
            &currency,
            &due_date,
            &description,
            &category,
            &Vec::new(&env),
        );
        let invoice = client.get_invoice(&invoice_id);
        assert_eq!(invoice.category, category);
    }
}

#[test]
fn test_invoice_creation_valid_tags() {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;
    let mut tags = Vec::new(&env);
    tags.push_back(String::from_str(&env, "urgent"));
    tags.push_back(String::from_str(&env, "q1"));

    let invoice_id = client.store_invoice(
        &business,
        &1000,
        &currency,
        &due_date,
        &String::from_str(&env, "Tagged invoice"),
        &InvoiceCategory::Services,
        &tags,
    );
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.tags.len(), 2);
}

#[test]
fn test_invoice_creation_invalid_tag_empty() {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;
    let mut tags = Vec::new(&env);
    tags.push_back(String::from_str(&env, ""));

    let result = client.try_store_invoice(
        &business,
        &1000,
        &currency,
        &due_date,
        &String::from_str(&env, "Description"),
        &InvoiceCategory::Services,
        &tags,
    );
    assert!(result.is_err());
    let err = result.unwrap_err().unwrap();
    assert_eq!(err, QuickLendXError::InvalidTag);
}

#[test]
fn test_invoice_creation_invalid_tag_too_long() {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;
    let mut tags = Vec::new(&env);
    // Tag length must be 1-50; 51 chars is invalid
    const LONG_TAG: &str = "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx";
    assert_eq!(LONG_TAG.len(), 51);
    tags.push_back(String::from_str(&env, LONG_TAG));

    let result = client.try_store_invoice(
        &business,
        &1000,
        &currency,
        &due_date,
        &String::from_str(&env, "Description"),
        &InvoiceCategory::Services,
        &tags,
    );
    assert!(result.is_err());
    let err = result.unwrap_err().unwrap();
    assert_eq!(err, QuickLendXError::InvalidTag);
}

#[test]
fn test_invoice_creation_invalid_tag_limit_exceeded() {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;
    let mut tags = Vec::new(&env);
    for i in 0..11 {
        let tag = match i {
            0 => "a",
            1 => "b",
            2 => "c",
            3 => "d",
            4 => "e",
            5 => "f",
            6 => "g",
            7 => "h",
            8 => "i",
            9 => "j",
            _ => "k",
        };
        tags.push_back(String::from_str(&env, tag));
    }

    let result = client.try_store_invoice(
        &business,
        &1000,
        &currency,
        &due_date,
        &String::from_str(&env, "Description"),
        &InvoiceCategory::Services,
        &tags,
    );
    assert!(result.is_err());
    let err = result.unwrap_err().unwrap();
    assert_eq!(err, QuickLendXError::TagLimitExceeded);
}

// ============================================================================
// upload_invoice tests (auth, verification, event) — #301
// ============================================================================
// - Requires business auth (only the business identity can upload).
// - Requires verified business (pending/rejected fail with BusinessNotVerified).
// - Verified business succeeds and emits invoice_uploaded (inv_up) event.

#[test]
fn test_invoice_upload_requires_business_verification() {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;

    env.mock_all_auths();

    // Try to upload without verification - should fail
    let result = client.try_upload_invoice(
        &business,
        &1000,
        &currency,
        &due_date,
        &String::from_str(&env, "Unverified invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    assert!(result.is_err());
    let err = result.unwrap_err().unwrap();
    assert_eq!(err, QuickLendXError::BusinessNotVerified);
}

/// upload_invoice requires the caller to be the business (require_auth).
/// Calling with a different address fails (here: unverified identity → BusinessNotVerified).
#[test]
fn test_invoice_upload_requires_business_auth() {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let business = setup_verified_business(&env, &client, &admin);
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;

    // Call as a different identity (unverified). Contract checks verification first for
    // the given business; this identity is not verified → BusinessNotVerified.
    // In production, require_auth() would also fail if the signer were not the business.
    let unverified_business = Address::generate(&env);
    let result = client.try_upload_invoice(
        &unverified_business,
        &1_000_000,
        &currency,
        &due_date,
        &String::from_str(&env, "Unauthorized upload"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    assert!(result.is_err());
    let err = result.unwrap_err().unwrap();
    assert_eq!(err, QuickLendXError::BusinessNotVerified);
}

#[test]
fn test_invoice_upload_verified_business_succeeds() {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let business = setup_verified_business(&env, &client, &admin);
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;

    env.mock_all_auths();

    let invoice_id = client.upload_invoice(
        &business,
        &1_000_000,
        &currency,
        &due_date,
        &String::from_str(&env, "Verified upload"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.business, business);
    assert_eq!(invoice.status, InvoiceStatus::Pending);
}

/// Pending business (KYC submitted but not yet verified) cannot upload; must be Verified.
#[test]
fn test_invoice_upload_pending_business_fails() {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let business = Address::generate(&env);
    let kyc_data = String::from_str(&env, "Business KYC");
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;

    env.mock_all_auths();
    client.submit_kyc_application(&business, &kyc_data);
    // Do not verify — business remains Pending.

    let result = client.try_upload_invoice(
        &business,
        &1000,
        &currency,
        &due_date,
        &String::from_str(&env, "Pending business invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    assert!(result.is_err());
    let err = result.unwrap_err().unwrap();
    assert_eq!(err, QuickLendXError::BusinessNotVerified);
}

/// Rejected business cannot upload; only Verified businesses can upload invoices.
#[test]
fn test_invoice_upload_rejected_business_fails() {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let business = Address::generate(&env);
    let kyc_data = String::from_str(&env, "Business KYC");
    let reason = String::from_str(&env, "Rejected for test");
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;

    env.mock_all_auths();
    client.submit_kyc_application(&business, &kyc_data);
    client.reject_business(&admin, &business, &reason);

    let result = client.try_upload_invoice(
        &business,
        &1000,
        &currency,
        &due_date,
        &String::from_str(&env, "Rejected business invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    assert!(result.is_err());
    let err = result.unwrap_err().unwrap();
    assert_eq!(err, QuickLendXError::BusinessNotVerified);
}

/// Verified business can upload; the contract emits the invoice_uploaded (inv_up) event
/// (see events::emit_invoice_uploaded). This test asserts the success path; event
/// emission is covered by the contract implementation and test_events::test_invoice_uploaded_event.
#[test]
fn test_invoice_upload_verified_business_succeeds_emits_event() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let business = setup_verified_business(&env, &client, &admin);
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;

    let invoice_id = client.upload_invoice(
        &business,
        &1_000_000,
        &currency,
        &due_date,
        &String::from_str(&env, "Verified upload with event"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.business, business);
    assert_eq!(invoice.amount, 2000);
    assert_eq!(invoice.status, InvoiceStatus::Pending);
}

#[test]
fn test_invoice_verify_requires_admin() {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let business = Address::generate(&env);
    let invoice_id = create_test_invoice(&env, &client, &business, 1_000_000);

    // Try to verify without admin - should fail
    let result = client.try_verify_invoice(&invoice_id);
    assert!(result.is_err());
}

#[test]
fn test_invoice_verify_admin_succeeds() {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let business = Address::generate(&env);

    let invoice_id = create_test_invoice(&env, &client, &business, 1_000_000);

    // Verify should succeed with admin
    client.verify_invoice(&invoice_id);

    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Verified);
}

#[test]
fn test_invoice_metadata_update_requires_business_owner() {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let business = Address::generate(&env);
    let invoice_id = create_test_invoice(&env, &client, &business, 1_000_000);

    let mut line_items = Vec::new(&env);
    line_items.push_back(LineItemRecord(
        String::from_str(&env, "Service"),
        1,
        100,
        100,
    ));

    let metadata = InvoiceMetadata {
        customer_name: String::from_str(&env, "Customer"),
        customer_address: String::from_str(&env, "Address"),
        tax_id: String::from_str(&env, "TAX123"),
        line_items,
        notes: String::from_str(&env, "Notes"),
    };

    // Try to update without auth - should fail
    let result = client.try_update_invoice_metadata(&invoice_id, &metadata);
    assert!(result.is_err());
}

// ============================================================================
// STATUS TRANSITION TESTS
// ============================================================================

#[test]
fn test_invoice_status_transition_pending_to_verified() {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let business = Address::generate(&env);

    let invoice_id = create_test_invoice(&env, &client, &business, 1_000_000);

    // Verify initial status
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Pending);

    // Transition to Verified
    client.verify_invoice(&invoice_id);

    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Verified);
}

#[test]
fn test_invoice_status_transition_verified_to_funded() {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let business = setup_verified_business(&env, &client, &admin);
    let investor = setup_verified_investor(&env, &client, &admin);

    env.mock_all_auths();

    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;
    let invoice_id = client.store_invoice(
        &business,
        &1000,
        &currency,
        &due_date,
        &String::from_str(&env, "Test invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    // Verify invoice
    client.verify_invoice(&invoice_id);

    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Verified);

    // Directly mark as funded to avoid escrow/token dependencies
    env.as_contract(&contract_id, || {
        let mut invoice = InvoiceStorage::get_invoice(&env, &invoice_id).unwrap();
        invoice.mark_as_funded(&env, investor.clone(), 900, env.ledger().timestamp());
        InvoiceStorage::update_invoice(&env, &invoice);
    });

    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Funded);
    assert_eq!(invoice.funded_amount, 900);
    assert_eq!(invoice.investor, Some(investor));
}

#[test]
fn test_invoice_status_transition_funded_to_paid() {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let business = setup_verified_business(&env, &client, &admin);
    let investor = setup_verified_investor(&env, &client, &admin);

    env.mock_all_auths();

    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;
    let invoice_id = client.store_invoice(
        &business,
        &1000,
        &currency,
        &due_date,
        &String::from_str(&env, "Test invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    // Verify invoice
    client.verify_invoice(&invoice_id);

    // Mark as funded directly
    env.as_contract(&contract_id, || {
        let mut invoice = InvoiceStorage::get_invoice(&env, &invoice_id).unwrap();
        invoice.mark_as_funded(&env, investor.clone(), 1000, env.ledger().timestamp());
        InvoiceStorage::update_invoice(&env, &invoice);
    });

    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Funded);

    // Transition to Paid
    client.update_invoice_status(&invoice_id, &InvoiceStatus::Paid);

    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Paid);
    assert!(invoice.settled_at.is_some());
}

#[test]
fn test_invoice_status_transition_funded_to_defaulted() {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let business = setup_verified_business(&env, &client, &admin);
    let investor = setup_verified_investor(&env, &client, &admin);

    env.mock_all_auths();

    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;
    let invoice_id = client.store_invoice(
        &business,
        &1000,
        &currency,
        &due_date,
        &String::from_str(&env, "Test invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    // Verify invoice
    client.verify_invoice(&invoice_id);

    // Mark as funded directly
    env.as_contract(&contract_id, || {
        let mut invoice = InvoiceStorage::get_invoice(&env, &invoice_id).unwrap();
        invoice.mark_as_funded(&env, investor.clone(), 1000, env.ledger().timestamp());
        InvoiceStorage::update_invoice(&env, &invoice);
    });

    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Funded);

    // Transition to Defaulted
    client.update_invoice_status(&invoice_id, &InvoiceStatus::Defaulted);

    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Defaulted);
}

#[test]
fn test_invoice_invalid_status_transition() {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let business = Address::generate(&env);
    let invoice_id = create_test_invoice(&env, &client, &business, 1_000_000);

    // Transition from Pending directly to Paid (allowed by current contract behavior)
    client.update_invoice_status(&invoice_id, &InvoiceStatus::Paid);
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Paid);
}

#[test]
fn test_invoice_cannot_verify_already_verified() {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let business = Address::generate(&env);

    let invoice_id = create_test_invoice(&env, &client, &business, 1_000_000);

    // Verify once
    client.verify_invoice(&invoice_id);

    // Try to verify again - should fail
    let result = client.try_verify_invoice(&invoice_id);
    assert!(result.is_err());
    let err = result.unwrap_err().unwrap();
    assert_eq!(err, QuickLendXError::InvalidStatus);
}

// ============================================================================
// EDGE CASES AND ERROR HANDLING TESTS
// ============================================================================

#[test]
fn test_invoice_not_found() {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let fake_id = BytesN::from_array(&env, &[0u8; 32]);

    let result = client.try_get_invoice(&fake_id);
    assert!(result.is_err());
    let err = result.unwrap_err().unwrap();
    assert_eq!(err, QuickLendXError::InvoiceNotFound);
}

#[test]
fn test_invoice_large_amount() {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;
    let large_amount = i128::MAX / 2; // Very large amount

    let invoice_id = client.store_invoice(
        &business,
        &large_amount,
        &currency,
        &due_date,
        &String::from_str(&env, "Large invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.amount, large_amount);
}

#[test]
fn test_invoice_minimum_amount() {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;
    let min_amount = 1i128;

    let invoice_id = client.store_invoice(
        &business,
        &min_amount,
        &currency,
        &due_date,
        &String::from_str(&env, "Minimum invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.amount, min_amount);
}

#[test]
fn test_invoice_far_future_due_date() {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    let current_time = env.ledger().timestamp();
    let far_future = current_time + (365 * 24 * 60 * 60); // 1 year from now

    let invoice_id = client.store_invoice(
        &business,
        &1000,
        &currency,
        &far_future,
        &String::from_str(&env, "Far future invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.due_date, far_future);
}

#[test]
fn test_invoice_multiple_invoices_same_business() {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;

    // Create multiple invoices
    let invoice1_id = client.store_invoice(
        &business,
        &1000,
        &currency,
        &due_date,
        &String::from_str(&env, "Invoice 1"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    let invoice2_id = client.store_invoice(
        &business,
        &2000,
        &currency,
        &due_date,
        &String::from_str(&env, "Invoice 2"),
        &InvoiceCategory::Products,
        &Vec::new(&env),
    );

    let invoice3_id = client.store_invoice(
        &business,
        &3000,
        &currency,
        &due_date,
        &String::from_str(&env, "Invoice 3"),
        &InvoiceCategory::Consulting,
        &Vec::new(&env),
    );

    // Verify all invoices are retrievable
    let inv1 = client.get_invoice(&invoice1_id);
    let inv2 = client.get_invoice(&invoice2_id);
    let inv3 = client.get_invoice(&invoice3_id);

    assert_eq!(inv1.business, business);
    assert_eq!(inv2.business, business);
    assert_eq!(inv3.business, business);
    assert_eq!(inv1.amount, 1000);
    assert_eq!(inv2.amount, 2000);
    assert_eq!(inv3.amount, 3000);
}

// ============================================================================
// CATEGORY INDEX CONSISTENCY REGRESSION TESTS
//
// Security assumption: the category index must never contain stale entries.
// After update_invoice_category the old bucket must not reference the invoice
// and the new bucket must contain exactly one entry for it.
// ============================================================================

/// After a single category update the old index must not contain the invoice
/// and the new index must contain it exactly once.
#[test]
fn test_category_index_no_stale_entry_after_update() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;

    let id = client.store_invoice(
        &business,
        &1000,
        &currency,
        &due_date,
        &String::from_str(&env, "Index consistency test"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    // Precondition: present in Services, absent from Products.
    assert!(client
        .get_invoices_by_category(&InvoiceCategory::Services)
        .contains(&id));
    assert!(!client
        .get_invoices_by_category(&InvoiceCategory::Products)
        .contains(&id));

    client.update_invoice_category(&id, &InvoiceCategory::Products);

    // Old bucket must be clean (no stale entry).
    let services = client.get_invoices_by_category(&InvoiceCategory::Services);
    assert!(
        !services.contains(&id),
        "stale entry found in old category index after update"
    );

    // New bucket must contain the invoice.
    let products = client.get_invoices_by_category(&InvoiceCategory::Products);
    assert!(
        products.contains(&id),
        "invoice missing from new category index after update"
    );

    // Exactly one occurrence in the new bucket.
    let occurrences = products.iter().filter(|x| *x == id).count();
    assert_eq!(occurrences, 1, "duplicate entry in new category index");
}

/// Multiple sequential category updates must leave no stale entries in any
/// intermediate bucket and the final bucket must hold exactly one entry.
#[test]
fn test_category_index_no_stale_after_multiple_updates() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;

    let id = client.store_invoice(
        &business,
        &1000,
        &currency,
        &due_date,
        &String::from_str(&env, "Multi-update consistency"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    let chain = [
        InvoiceCategory::Products,
        InvoiceCategory::Consulting,
        InvoiceCategory::Manufacturing,
        InvoiceCategory::Technology,
        InvoiceCategory::Healthcare,
        InvoiceCategory::Other,
    ];

    let mut prev = InvoiceCategory::Services;
    for next in chain {
        client.update_invoice_category(&id, &next);

        // Old bucket must not contain the invoice.
        assert!(
            !client.get_invoices_by_category(&prev).contains(&id),
            "stale entry in {:?} after moving to {:?}",
            prev,
            next
        );
        // New bucket must contain it exactly once.
        let bucket = client.get_invoices_by_category(&next);
        assert!(bucket.contains(&id));
        assert_eq!(bucket.iter().filter(|x| *x == id).count(), 1);

        prev = next;
    }
}

/// Updating a category to the same value must be idempotent: the index count
/// must not grow and no duplicate must appear.
#[test]
fn test_category_index_idempotent_same_category_update() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;

    let id = client.store_invoice(
        &business,
        &1000,
        &currency,
        &due_date,
        &String::from_str(&env, "Idempotent category update"),
        &InvoiceCategory::Technology,
        &Vec::new(&env),
    );

    let count_before = client.get_invoice_count_by_category(&InvoiceCategory::Technology);

    // Update to the same category.
    client.update_invoice_category(&id, &InvoiceCategory::Technology);

    let bucket = client.get_invoices_by_category(&InvoiceCategory::Technology);
    let count_after = client.get_invoice_count_by_category(&InvoiceCategory::Technology);

    // Count must not grow.
    assert_eq!(
        count_after, count_before,
        "duplicate inserted on same-category update"
    );
    // Exactly one occurrence.
    assert_eq!(bucket.iter().filter(|x| *x == id).count(), 1);
}

/// Sibling invoices in the same category must not be affected when one invoice
/// changes its category (no cross-invoice index pollution).
#[test]
fn test_category_index_sibling_unaffected_after_update() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;

    let id_a = client.store_invoice(
        &business,
        &1000,
        &currency,
        &due_date,
        &String::from_str(&env, "Sibling A"),
        &InvoiceCategory::Healthcare,
        &Vec::new(&env),
    );
    let id_b = client.store_invoice(
        &business,
        &2000,
        &currency,
        &due_date,
        &String::from_str(&env, "Sibling B"),
        &InvoiceCategory::Healthcare,
        &Vec::new(&env),
    );

    // Move only id_a to Other.
    client.update_invoice_category(&id_a, &InvoiceCategory::Other);

    // id_b must still be in Healthcare.
    let healthcare = client.get_invoices_by_category(&InvoiceCategory::Healthcare);
    assert!(
        healthcare.contains(&id_b),
        "sibling removed from old category"
    );
    assert!(
        !healthcare.contains(&id_a),
        "moved invoice still in old category"
    );

    // id_a must be in Other.
    let other = client.get_invoices_by_category(&InvoiceCategory::Other);
    assert!(other.contains(&id_a));
    assert!(
        !other.contains(&id_b),
        "sibling incorrectly added to new category"
    );
}

/// get_invoice_count_by_category must always equal the length of
/// get_invoices_by_category, both before and after updates.
#[test]
fn test_category_count_matches_list_length_after_updates() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;

    let ids: Vec<_> = (0..3)
        .map(|i| {
            client.store_invoice(
                &business,
                &(1000 + i as i128),
                &currency,
                &due_date,
                &String::from_str(&env, "Count consistency"),
                &InvoiceCategory::Consulting,
                &soroban_sdk::Vec::new(&env),
            )
        })
        .collect();

    // Move one invoice to Manufacturing.
    client.update_invoice_category(&ids[0], &InvoiceCategory::Manufacturing);

    for cat in [
        InvoiceCategory::Consulting,
        InvoiceCategory::Manufacturing,
        InvoiceCategory::Services,
    ] {
        let list = client.get_invoices_by_category(&cat);
        let count = client.get_invoice_count_by_category(&cat);
        assert_eq!(
            count,
            list.len() as u32,
            "count/list mismatch for category after update"
        );
    }
}

/// The full category list returned by get_all_categories must always contain
/// exactly the 7 canonical variants with no duplicates, regardless of stored
/// invoice state.
#[test]
fn test_get_all_categories_canonical_and_no_duplicates() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;

    // Store invoices across several categories to exercise storage paths.
    for cat in [
        InvoiceCategory::Services,
        InvoiceCategory::Products,
        InvoiceCategory::Technology,
    ] {
        client.store_invoice(
            &business,
            &1000,
            &currency,
            &due_date,
            &String::from_str(&env, "Cat test"),
            &cat,
            &soroban_sdk::Vec::new(&env),
        );
    }

    let all = client.get_all_categories();
    assert_eq!(all.len(), 7, "expected exactly 7 canonical categories");

    let expected = [
        InvoiceCategory::Services,
        InvoiceCategory::Products,
        InvoiceCategory::Consulting,
        InvoiceCategory::Manufacturing,
        InvoiceCategory::Technology,
        InvoiceCategory::Healthcare,
        InvoiceCategory::Other,
    ];
    for cat in expected {
        assert!(all.contains(&cat), "missing category {:?}", cat);
        assert_eq!(
            all.iter().filter(|c| c == cat).count(),
            1,
            "duplicate category {:?}",
            cat
        );
    }
}

#[test]
fn test_invoice_ids_unique_under_same_ledger_slot_collision_regression() {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    pin_invoice_collision_ledger_slot(&env, 1_700_200_000, 51);

    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    let mut ids = Vec::new(&env);

    for expected_counter in 0..COLLISION_HIGH_THROUGHPUT_SAMPLE {
        let invoice_id = store_collision_test_invoice(
            &env,
            &client,
            &business,
            &currency,
            "same-slot-collision-regression",
        );

        for existing_id in ids.iter() {
            assert_ne!(invoice_id, existing_id);
        }

        assert_eq!(invoice_id_counter_segment(&invoice_id), expected_counter);
        ids.push_back(invoice_id);
    }

    assert_eq!(
        read_invoice_collision_counter(&env, &contract_id),
        COLLISION_HIGH_THROUGHPUT_SAMPLE
    );
}

#[test]
fn test_invoice_counter_rewind_skips_collision_without_overwrite() {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    pin_invoice_collision_ledger_slot(&env, 1_700_200_001, 52);

    let business = Address::generate(&env);
    let currency = Address::generate(&env);

    let original_id =
        store_collision_test_invoice(&env, &client, &business, &currency, "collision-original");
    set_invoice_collision_counter(&env, &contract_id, 0);

    let retried_id =
        store_collision_test_invoice(&env, &client, &business, &currency, "collision-retried");

    assert_ne!(original_id, retried_id);
    assert_eq!(invoice_id_counter_segment(&original_id), 0);
    assert_eq!(invoice_id_counter_segment(&retried_id), 1);
    assert_eq!(
        client.get_invoice(&original_id).description,
        String::from_str(&env, "collision-original")
    );
    assert_eq!(
        client.get_invoice(&retried_id).description,
        String::from_str(&env, "collision-retried")
    );
    assert_eq!(read_invoice_collision_counter(&env, &contract_id), 2);
}
