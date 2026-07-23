#![cfg(test)]

use crate::{
    errors::QuickLendXError,
    invoice::InvoiceCategory,
    types::{InvoiceMetadata, LineItemRecord},
    QuickLendXContract, QuickLendXContractClient,
};
use soroban_sdk::{testutils::Address as _, Address, Env, String, Vec};

fn setup() -> (Env, QuickLendXContractClient<'static>, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let business = Address::generate(&env);

    client.set_admin(&admin);
    client.submit_kyc_application(&business, &String::from_str(&env, "KYC"));
    client.verify_business(&admin, &business);

    (env, client, admin, business)
}

fn create_string(env: &Env, len: usize) -> String {
    let s = "a".repeat(len);
    String::from_str(env, &s)
}

fn create_unicode_string(env: &Env, len: usize) -> String {
    let mut s = alloc::string::String::new();
    s.push('🚀'); // 4 bytes
    while s.len() < len {
        s.push('a');
    }
    s.truncate(len);
    String::from_str(env, &s)
}

fn valid_line_items(env: &Env) -> Vec<LineItemRecord> {
    let mut items = Vec::new(env);
    items.push_back(LineItemRecord(
        String::from_str(env, "Service"),
        1,
        1000,
        1000,
    ));
    items
}

#[test]
fn test_metadata_customer_name_limits() {
    let (env, client, _admin, business) = setup();
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;

    let invoice_id = client.upload_invoice(
        &business,
        &1000,
        &currency,
        &due_date,
        &String::from_str(&env, "test"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    // At limit (150 bytes)
    let mut meta = InvoiceMetadata {
        customer_name: create_string(&env, 150),
        customer_address: String::from_str(&env, "Address"),
        tax_id: String::from_str(&env, "TAX-123"),
        line_items: valid_line_items(&env),
        notes: String::from_str(&env, "Notes"),
    };
    assert!(client
        .try_update_invoice_metadata(&invoice_id, &meta)
        .is_ok());

    // Over limit (151 bytes)
    meta.customer_name = create_string(&env, 151);
    let err = client
        .try_update_invoice_metadata(&invoice_id, &meta)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, QuickLendXError::InvalidDescription);

    // Unicode strings at boundary limit
    meta.customer_name = create_unicode_string(&env, 150);
    assert!(client
        .try_update_invoice_metadata(&invoice_id, &meta)
        .is_ok());

    // Empty string rejection
    meta.customer_name = String::from_str(&env, "");
    let err = client
        .try_update_invoice_metadata(&invoice_id, &meta)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, QuickLendXError::InvalidDescription);
}

#[test]
fn test_metadata_customer_address_limits() {
    let (env, client, _admin, business) = setup();
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;

    let invoice_id = client.upload_invoice(
        &business,
        &1000,
        &currency,
        &due_date,
        &String::from_str(&env, "test"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    let mut meta = InvoiceMetadata {
        customer_name: String::from_str(&env, "Name"),
        customer_address: create_string(&env, 300),
        tax_id: String::from_str(&env, "TAX-123"),
        line_items: valid_line_items(&env),
        notes: String::from_str(&env, "Notes"),
    };
    assert!(client
        .try_update_invoice_metadata(&invoice_id, &meta)
        .is_ok());

    // Over limit (301 bytes)
    meta.customer_address = create_string(&env, 301);
    let err = client
        .try_update_invoice_metadata(&invoice_id, &meta)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, QuickLendXError::InvalidDescription);
}

#[test]
fn test_metadata_tax_id_limits() {
    let (env, client, _admin, business) = setup();
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;

    let invoice_id = client.upload_invoice(
        &business,
        &1000,
        &currency,
        &due_date,
        &String::from_str(&env, "test"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    let mut meta = InvoiceMetadata {
        customer_name: String::from_str(&env, "Name"),
        customer_address: String::from_str(&env, "Address"),
        tax_id: create_string(&env, 50),
        line_items: valid_line_items(&env),
        notes: String::from_str(&env, "Notes"),
    };
    assert!(client
        .try_update_invoice_metadata(&invoice_id, &meta)
        .is_ok());

    // Over limit (51 bytes)
    meta.tax_id = create_string(&env, 51);
    let err = client
        .try_update_invoice_metadata(&invoice_id, &meta)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, QuickLendXError::InvalidDescription);
}
