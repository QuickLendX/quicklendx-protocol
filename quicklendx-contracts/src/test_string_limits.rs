#![cfg(test)]
extern crate std;

use crate::errors::QuickLendXError;
use crate::invoice::{InvoiceCategory, InvoiceMetadata};
use crate::protocol_limits::*;
use crate::{QuickLendXContract, QuickLendXContractClient};
use soroban_sdk::{testutils::Address as _, Address, BytesN, Env, String, Vec};

fn setup() -> (Env, QuickLendXContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.set_admin(&admin);
    (env, client, admin)
}

fn create_long_string(env: &Env, len: u32) -> String {
    let mut s = std::string::String::with_capacity(len as usize);
    for _ in 0..len {
        s.push('a');
    }
    String::from_str(env, &s)
}

#[test]
fn test_invoice_description_limits() {
    let (env, client, _admin) = setup();
    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;

    // At limit
    let desc_at_limit = create_long_string(&env, MAX_DESCRIPTION_LENGTH);
    let res = client.try_store_invoice(
        &business,
        &1000,
        &currency,
        &due_date,
        &desc_at_limit,
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    assert!(res.is_ok());

    // Over limit
    let desc_over_limit = create_long_string(&env, MAX_DESCRIPTION_LENGTH + 1);
    let res = client.try_store_invoice(
        &business,
        &1000,
        &currency,
        &due_date,
        &desc_over_limit,
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    assert!(res.is_err());
    assert_eq!(
        res.err().unwrap().unwrap(),
        QuickLendXError::InvalidDescription
    );
}

#[test]
fn test_invoice_metadata_limits() {
    let (env, client, _admin) = setup();
    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;

    let invoice_id = client.store_invoice(
        &business,
        &1000,
        &currency,
        &due_date,
        &String::from_str(&env, "Test"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    let mut metadata = InvoiceMetadata {
        customer_name: String::from_str(&env, "Valid Name"),
        customer_address: String::from_str(&env, "Valid Address"),
        tax_id: String::from_str(&env, "Valid Tax ID"),
        notes: String::from_str(&env, "Valid Notes"),
        line_items: Vec::new(&env),
    };

    // Test Name
    metadata.customer_name = create_long_string(&env, MAX_NAME_LENGTH + 1);
    let res = client.try_update_invoice_metadata(&invoice_id, &metadata);
    assert!(res.is_err());
    assert_eq!(
        res.err().unwrap().unwrap(),
        QuickLendXError::InvalidDescription
    );
    metadata.customer_name = String::from_str(&env, "Valid Name");

    // Test Address
    metadata.customer_address = create_long_string(&env, MAX_ADDRESS_LENGTH + 1);
    let res = client.try_update_invoice_metadata(&invoice_id, &metadata);
    assert!(res.is_err());
    assert_eq!(
        res.err().unwrap().unwrap(),
        QuickLendXError::InvalidDescription
    );
    metadata.customer_address = String::from_str(&env, "Valid Address");

    // Test Tax ID
    metadata.tax_id = create_long_string(&env, MAX_TAX_ID_LENGTH + 1);
    let res = client.try_update_invoice_metadata(&invoice_id, &metadata);
    assert!(res.is_err());
    assert_eq!(
        res.err().unwrap().unwrap(),
        QuickLendXError::InvalidDescription
    );
    metadata.tax_id = String::from_str(&env, "Valid Tax ID");

    // Test Notes
    metadata.notes = create_long_string(&env, MAX_NOTES_LENGTH + 1);
    let res = client.try_update_invoice_metadata(&invoice_id, &metadata);
    assert!(res.is_err());
    assert_eq!(
        res.err().unwrap().unwrap(),
        QuickLendXError::InvalidDescription
    );
    metadata.notes = String::from_str(&env, "Valid Notes");
}

#[test]
fn test_kyc_limits() {
    let (env, client, admin) = setup();
    let business = Address::generate(&env);

    let kyc_over = create_long_string(&env, MAX_KYC_DATA_LENGTH + 1);
    let res = client.try_submit_kyc_application(&business, &kyc_over);
    assert!(res.is_err());
    assert_eq!(
        res.err().unwrap().unwrap(),
        QuickLendXError::InvalidDescription
    );

    // Rejection reason
    client.submit_kyc_application(&business, &String::from_str(&env, "valid"));
    let reason_over = create_long_string(&env, MAX_REJECTION_REASON_LENGTH + 1);
    let res = client.try_reject_business(&admin, &business, &reason_over);
    assert!(res.is_err());
    assert_eq!(
        res.err().unwrap().unwrap(),
        QuickLendXError::InvalidDescription
    );
}

// ============================================================================
// TAG NORMALIZATION + STRING LIMIT INTERACTION TESTS (#527)
// ============================================================================

/// A 50-char uppercase tag normalizes to a 50-char lowercase tag — still valid.
#[test]
fn test_tag_at_limit_uppercase_normalizes_valid() {
    let (env, client, _admin) = setup();
    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;

    // 50 uppercase 'A' characters — normalizes to 50 lowercase 'a' characters.
    let mut s = std::string::String::with_capacity(50);
    for _ in 0..50 {
        s.push('A');
    }
    let mut tags = Vec::new(&env);
    tags.push_back(String::from_str(&env, &s));

    let res = client.try_store_invoice(
        &business,
        &1000,
        &currency,
        &due_date,
        &String::from_str(&env, "Desc"),
        &crate::invoice::InvoiceCategory::Services,
        &tags,
    );
    assert!(res.is_ok(), "50-char uppercase tag should normalize to valid 50-char lowercase");
}

/// A tag with leading/trailing spaces that trims to exactly 50 chars is valid.
#[test]
fn test_tag_trim_to_limit_valid() {
    let (env, client, _admin) = setup();
    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;

    // Build " " + 50 'a' chars + " " = 52 bytes, normalizes to 50 bytes.
    let mut s = std::string::String::with_capacity(52);
    s.push(' ');
    for _ in 0..50 {
        s.push('a');
    }
    s.push(' ');
    let mut tags = Vec::new(&env);
    tags.push_back(String::from_str(&env, &s));

    let res = client.try_store_invoice(
        &business,
        &1000,
        &currency,
        &due_date,
        &String::from_str(&env, "Desc"),
        &crate::invoice::InvoiceCategory::Services,
        &tags,
    );
    assert!(res.is_ok(), "tag that trims to exactly 50 chars should be valid");
}

/// A tag with spaces only is rejected after normalization.
#[test]
fn test_tag_spaces_only_invalid_after_norm() {
    let (env, client, _admin) = setup();
    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;

    let mut tags = Vec::new(&env);
    tags.push_back(String::from_str(&env, "     "));

    let res = client.try_store_invoice(
        &business,
        &1000,
        &currency,
        &due_date,
        &String::from_str(&env, "Desc"),
        &crate::invoice::InvoiceCategory::Services,
        &tags,
    );
    assert!(res.is_err());
    assert_eq!(
        res.err().unwrap().unwrap(),
        crate::errors::QuickLendXError::InvalidTag
    );
}
