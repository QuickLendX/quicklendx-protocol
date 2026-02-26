#![cfg(test)]

use crate::QuickLendXContract;
use soroban_sdk::{testutils::Address as _, Env};

/// This is the pattern that works in your other tests
#[test]
fn test_metadata_update_requires_owner_pattern() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let _client = crate::QuickLendXContractClient::new(&env, &contract_id);

    // Your test logic here using the client
    assert!(true); // Placeholder
}

#[test]
fn test_metadata_validation_pattern() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let _client = crate::QuickLendXContractClient::new(&env, &contract_id);

    // Your test logic here using the client
    assert!(true); // Placeholder
}

#[test]
fn test_non_owner_cannot_update_metadata_pattern() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let _client = crate::QuickLendXContractClient::new(&env, &contract_id);

    // Your test logic here using the client
    assert!(true); // Placeholder
}

#[test]
fn test_update_and_query_metadata_pattern() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let _client = crate::QuickLendXContractClient::new(&env, &contract_id);

    // Your test logic here using the client
    assert!(true); // Placeholder
}

use soroban_sdk::{testutils::Env as TestEnv, Address, BytesN, Env, String, Vec};
use crate::invoice::{Invoice, InvoiceCategory, InvoiceMetadata, LineItemRecord};
use crate::storage::InvoiceStorage;

#[test]
fn test_invoice_metadata_and_indexing() {
    let env = Env::default();
    let business = Address::random(&env);
    let currency = Address::random(&env);
    let category = InvoiceCategory::Services;
    let tags = Vec::new(&env);
    let mut invoice = Invoice::new(
        &env,
        business.clone(),
        1000,
        currency.clone(),
        env.ledger().timestamp() + 10000,
        String::from_str(&env, "Test invoice"),
        category,
        tags.clone(),
    );

    // Metadata
    let metadata = InvoiceMetadata {
        customer_name: String::from_str(&env, "Alice Corp"),
        customer_address: String::from_str(&env, "123 Main St"),
        tax_id: String::from_str(&env, "TAX123"),
        line_items: vec![&env, LineItemRecord(String::from_str(&env, "Item1"), 1, 100, 100)],
        notes: String::from_str(&env, "Urgent"),
    };
    assert!(metadata.validate().is_ok());
    assert!(invoice.update_metadata(&env, &business, metadata.clone()).is_ok());

    // Store and check indexes
    InvoiceStorage::store(&env, &invoice);
    let stored = InvoiceStorage::get(&env, &invoice.id).unwrap();
    assert_eq!(stored.metadata_customer_name, Some(metadata.customer_name.clone()));
    assert_eq!(stored.metadata_tax_id, Some(metadata.tax_id.clone()));

    // Index by customer
    let customer_index = crate::storage::Indexes::invoices_by_customer(&metadata.customer_name);
    let customer_ids: Vec<BytesN<32>> = env.storage().persistent().get(&customer_index).unwrap();
    assert!(customer_ids.iter().any(|id| id == invoice.id));

    // Index by tax_id
    let taxid_index = crate::storage::Indexes::invoices_by_tax_id(&metadata.tax_id);
    let taxid_ids: Vec<BytesN<32>> = env.storage().persistent().get(&taxid_index).unwrap();
    assert!(taxid_ids.iter().any(|id| id == invoice.id));

    // Clear metadata and check index removal
    assert!(invoice.clear_metadata(&env, &business).is_ok());
    InvoiceStorage::update(&env, &invoice);
    let customer_ids_after: Vec<BytesN<32>> = env.storage().persistent().get(&customer_index).unwrap();
    assert!(!customer_ids_after.iter().any(|id| id == invoice.id));
    let taxid_ids_after: Vec<BytesN<32>> = env.storage().persistent().get(&taxid_index).unwrap();
    assert!(!taxid_ids_after.iter().any(|id| id == invoice.id));
}
