#![cfg(test)]

use crate::QuickLendXContract;
use soroban_sdk::{testutils::Address as _, Address, BytesN, Env, String, Vec};
use crate::invoice::{Invoice, InvoiceCategory, InvoiceMetadata, LineItemRecord};
use crate::storage::InvoiceStorage;

//
// ------------------------------------------------------------
// Helper
// ------------------------------------------------------------
//

fn create_invoice(env: &Env, business: &Address) -> Invoice {
    let currency = Address::random(env);
    let category = InvoiceCategory::Services;
    let tags = Vec::new(env);

    Invoice::new(
        env,
        business.clone(),
        1000,
        currency,
        env.ledger().timestamp() + 10000,
        String::from_str(env, "Test invoice"),
        category,
        tags,
    )
}

//
// ------------------------------------------------------------
// 1️⃣ Metadata validation tests
// ------------------------------------------------------------
//

#[test]
fn test_metadata_empty_line_items_valid() {
    let env = Env::default();

    let metadata = InvoiceMetadata {
        customer_name: String::from_str(&env, "Test Co"),
        customer_address: String::from_str(&env, "Addr"),
        tax_id: String::from_str(&env, "TAX1"),
        line_items: Vec::new(&env),
        notes: String::from_str(&env, "Notes"),
    };

    assert!(metadata.validate().is_ok());
}

#[test]
fn test_metadata_single_line_item_valid() {
    let env = Env::default();

    let metadata = InvoiceMetadata {
        customer_name: String::from_str(&env, "Client A"),
        customer_address: String::from_str(&env, "Street"),
        tax_id: String::from_str(&env, "TAX2"),
        line_items: vec![&env,
            LineItemRecord(
                String::from_str(&env, "Item1"),
                1,
                500,
                500
            )
        ],
        notes: String::from_str(&env, "OK"),
    };

    assert!(metadata.validate().is_ok());
}

#[test]
fn test_metadata_multiple_line_items_valid() {
    let env = Env::default();

    let metadata = InvoiceMetadata {
        customer_name: String::from_str(&env, "Client B"),
        customer_address: String::from_str(&env, "Addr2"),
        tax_id: String::from_str(&env, "TAX3"),
        line_items: vec![&env,
            LineItemRecord(String::from_str(&env, "Item1"), 1, 200, 200),
            LineItemRecord(String::from_str(&env, "Item2"), 2, 300, 600)
        ],
        notes: String::from_str(&env, "Multiple items"),
    };

    assert!(metadata.validate().is_ok());
}

#[test]
fn test_metadata_negative_amount_rejected() {
    let env = Env::default();

    let metadata = InvoiceMetadata {
        customer_name: String::from_str(&env, "Client C"),
        customer_address: String::from_str(&env, "Addr3"),
        tax_id: String::from_str(&env, "TAX4"),
        line_items: vec![&env,
            LineItemRecord(String::from_str(&env, "BadItem"), 1, -100, -100)
        ],
        notes: String::from_str(&env, "Invalid"),
    };

    assert!(metadata.validate().is_err());
}

#[test]
fn test_metadata_overflow_rejected() {
    let env = Env::default();

    let metadata = InvoiceMetadata {
        customer_name: String::from_str(&env, "Overflow"),
        customer_address: String::from_str(&env, "Addr"),
        tax_id: String::from_str(&env, "TAX5"),
        line_items: vec![&env,
            LineItemRecord(
                String::from_str(&env, "Huge"),
                i128::MAX,
                i128::MAX,
                i128::MAX
            )
        ],
        notes: String::from_str(&env, "Overflow test"),
    };

    assert!(metadata.validate().is_err());
}

//
// ------------------------------------------------------------
// 2️⃣ Ownership enforcement
// ------------------------------------------------------------
//

#[test]
fn test_metadata_update_requires_owner() {
    let env = Env::default();
    let business = Address::random(&env);
    let other = Address::random(&env);

    let mut invoice = create_invoice(&env, &business);

    let metadata = InvoiceMetadata {
        customer_name: String::from_str(&env, "OwnerTest"),
        customer_address: String::from_str(&env, "Addr"),
        tax_id: String::from_str(&env, "TAX6"),
        line_items: Vec::new(&env),
        notes: String::from_str(&env, "Test"),
    };

    // Owner succeeds
    assert!(invoice.update_metadata(&env, &business, metadata.clone()).is_ok());

    // Non-owner fails
    assert!(invoice.update_metadata(&env, &other, metadata).is_err());
}

//
// ------------------------------------------------------------
// 3️⃣ Store + index validation
// ------------------------------------------------------------
//

#[test]
fn test_invoice_metadata_and_indexing() {
    let env = Env::default();
    let business = Address::random(&env);

    let mut invoice = create_invoice(&env, &business);

    let metadata = InvoiceMetadata {
        customer_name: String::from_str(&env, "Alice Corp"),
        customer_address: String::from_str(&env, "123 Main St"),
        tax_id: String::from_str(&env, "TAX123"),
        line_items: vec![&env,
            LineItemRecord(String::from_str(&env, "Item1"), 1, 100, 100)
        ],
        notes: String::from_str(&env, "Urgent"),
    };

    assert!(invoice.update_metadata(&env, &business, metadata.clone()).is_ok());

    InvoiceStorage::store(&env, &invoice);

    let stored = InvoiceStorage::get(&env, &invoice.id).unwrap();
    assert_eq!(stored.metadata_customer_name, Some(metadata.customer_name.clone()));
    assert_eq!(stored.metadata_tax_id, Some(metadata.tax_id.clone()));

    // Index by customer
    let customer_index = crate::storage::Indexes::invoices_by_customer(&metadata.customer_name);
    let customer_ids: Vec<BytesN<32>> =
        env.storage().persistent().get(&customer_index).unwrap();

    assert!(customer_ids.iter().any(|id| id == invoice.id));

    // Clear metadata
    assert!(invoice.clear_metadata(&env, &business).is_ok());
    InvoiceStorage::update(&env, &invoice);

    let customer_ids_after: Vec<BytesN<32>> =
        env.storage().persistent().get(&customer_index).unwrap();

    assert!(!customer_ids_after.iter().any(|id| id == invoice.id));
}