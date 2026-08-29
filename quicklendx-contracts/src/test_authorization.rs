#![cfg(test)]

use soroban_sdk::{testutils::Address as _, Address, Bytes, BytesN, Env, Vec};
use crate::contract::{QuickLendXContract, QuickLendXContractClient};
use crate::errors::QuickLendXError;
use crate::types::{InvoiceCategory, InvoiceMetadata, InvoiceStatus};
use crate::admin::AdminStorage;

#[test]
fn test_unauthorized_metadata_update() {
    let env = Env::default();
    env.mock_all_auths();
    
    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    let business = Address::generate(&env);
    let other_business = Address::generate(&env);
    let currency = Address::generate(&env);
    
    let contract_id = env.register_contract(None, QuickLendXContract);
    let client = QuickLendXContractClient::new(&env, &contract_id);
    
    client.initialize(
        &admin,
        &treasury,
        &500, // fee bps
        &100, // min_invoice_amount
        &365, // max_due_date_days
        &86400, // grace period
        &Vec::new(&env),
        &Vec::new(&env),
    );
    
    // Setup limits
    client.initialize_protocol_limits(&admin);
    client.initialize_admin(&admin);

    // Mock business KYC setup logic here if needed...
    // For now we just test that the call fails if the caller is wrong.
    // However, since store_invoice also checks KYC, we might mock store_invoice or just use raw storage for the test if KYC is complex.
    
    // Instead of doing full setup which might fail on KYC checks, let's just create an invoice directly using storage.
    let invoice_id = BytesN::from_array(&env, &[1; 32]);
    let invoice = crate::types::Invoice {
        invoice_id: invoice_id.clone(),
        business: business.clone(),
        amount: 1000,
        currency: currency.clone(),
        due_date: env.ledger().timestamp() + 86400,
        description: Bytes::from_slice(&env, b"desc"),
        category: InvoiceCategory::Services,
        tags: Vec::new(&env),
        status: InvoiceStatus::Pending,
        metadata: None,
        metadata_customer_name: None,
        metadata_tax_id: None,
        total_paid: 0,
        funded_amount: 0,
        funded_at: None,
        average_rating: None,
        total_ratings: 0,
        investor: None,
        dispute_status: crate::types::DisputeStatus::None,
        dispute: None,
        payment_history: Vec::new(&env),
        ratings: Vec::new(&env),
        created_at: env.ledger().timestamp(),
        updated_at: env.ledger().timestamp(),
        settled_at: None,
    };
    
    crate::storage::InvoiceStorage::store_invoice(&env, &invoice);

    let metadata = InvoiceMetadata {
        customer_name: Bytes::from_slice(&env, b"cust"),
        customer_address: Bytes::from_slice(&env, b"addr"),
        tax_id: Bytes::from_slice(&env, b"tax"),
        line_items: Vec::new(&env),
        notes: Bytes::from_slice(&env, b"notes"),
    };
    
    let nonce = BytesN::from_array(&env, &[2; 32]);
    
    // other_business tries to update metadata
    let result = client.try_update_invoice_metadata(&other_business, &invoice_id, &metadata, &nonce);
    
    assert!(result.is_err(), "Expected authorization error");
    
    // the legitimate business succeeds
    let result = client.try_update_invoice_metadata(&business, &invoice_id, &metadata, &nonce);
    assert!(result.is_ok(), "Legitimate business should be able to update metadata");
}

#[test]
fn test_unauthorized_cancel() {
    let env = Env::default();
    env.mock_all_auths();
    
    let business = Address::generate(&env);
    let other_business = Address::generate(&env);
    
    let contract_id = env.register_contract(None, QuickLendXContract);
    let client = QuickLendXContractClient::new(&env, &contract_id);
    
    let invoice_id = BytesN::from_array(&env, &[3; 32]);
    let invoice = crate::types::Invoice {
        invoice_id: invoice_id.clone(),
        business: business.clone(),
        amount: 1000,
        currency: Address::generate(&env),
        due_date: env.ledger().timestamp() + 86400,
        description: Bytes::from_slice(&env, b"desc"),
        category: InvoiceCategory::Services,
        tags: Vec::new(&env),
        status: InvoiceStatus::Pending,
        metadata: None,
        metadata_customer_name: None,
        metadata_tax_id: None,
        total_paid: 0,
        funded_amount: 0,
        funded_at: None,
        average_rating: None,
        total_ratings: 0,
        investor: None,
        dispute_status: crate::types::DisputeStatus::None,
        dispute: None,
        payment_history: Vec::new(&env),
        ratings: Vec::new(&env),
        created_at: env.ledger().timestamp(),
        updated_at: env.ledger().timestamp(),
        settled_at: None,
    };
    
    crate::storage::InvoiceStorage::store_invoice(&env, &invoice);
    
    let nonce = BytesN::from_array(&env, &[4; 32]);
    let result = client.try_cancel_invoice(&other_business, &invoice_id, &nonce);
    
    assert!(result.is_err(), "Expected authorization error");
    
    let result = client.try_cancel_invoice(&business, &invoice_id, &nonce);
    assert!(result.is_ok(), "Legitimate business should be able to cancel");
}

#[test]
fn test_unauthorized_complete() {
    let env = Env::default();
    env.mock_all_auths();
    
    let business = Address::generate(&env);
    let other_business = Address::generate(&env);
    
    let contract_id = env.register_contract(None, QuickLendXContract);
    let client = QuickLendXContractClient::new(&env, &contract_id);
    
    let invoice_id = BytesN::from_array(&env, &[5; 32]);
    let invoice = crate::types::Invoice {
        invoice_id: invoice_id.clone(),
        business: business.clone(),
        amount: 1000,
        currency: Address::generate(&env),
        due_date: env.ledger().timestamp() + 86400,
        description: Bytes::from_slice(&env, b"desc"),
        category: InvoiceCategory::Services,
        tags: Vec::new(&env),
        status: InvoiceStatus::Funded,
        metadata: None,
        metadata_customer_name: None,
        metadata_tax_id: None,
        total_paid: 0,
        funded_amount: 1000,
        funded_at: Some(env.ledger().timestamp()),
        average_rating: None,
        total_ratings: 0,
        investor: Some(Address::generate(&env)),
        dispute_status: crate::types::DisputeStatus::None,
        dispute: None,
        payment_history: Vec::new(&env),
        ratings: Vec::new(&env),
        created_at: env.ledger().timestamp(),
        updated_at: env.ledger().timestamp(),
        settled_at: None,
    };
    
    crate::storage::InvoiceStorage::store_invoice(&env, &invoice);
    
    let nonce = BytesN::from_array(&env, &[6; 32]);
    let result = client.try_complete_invoice(&other_business, &invoice_id, &nonce);
    
    assert!(result.is_err(), "Expected authorization error");
}

#[test]
fn test_admin_only_endpoints() {
    let env = Env::default();
    env.mock_all_auths();
    
    let admin = Address::generate(&env);
    let non_admin = Address::generate(&env);
    
    let contract_id = env.register_contract(None, QuickLendXContract);
    let client = QuickLendXContractClient::new(&env, &contract_id);
    
    client.initialize_admin(&admin);

    let invoice_id = BytesN::from_array(&env, &[7; 32]);
    let invoice = crate::types::Invoice {
        invoice_id: invoice_id.clone(),
        business: Address::generate(&env),
        amount: 1000,
        currency: Address::generate(&env),
        due_date: env.ledger().timestamp() + 86400,
        description: Bytes::from_slice(&env, b"desc"),
        category: InvoiceCategory::Services,
        tags: Vec::new(&env),
        status: InvoiceStatus::Pending,
        metadata: None,
        metadata_customer_name: None,
        metadata_tax_id: None,
        total_paid: 0,
        funded_amount: 0,
        funded_at: None,
        average_rating: None,
        total_ratings: 0,
        investor: None,
        dispute_status: crate::types::DisputeStatus::None,
        dispute: None,
        payment_history: Vec::new(&env),
        ratings: Vec::new(&env),
        created_at: env.ledger().timestamp(),
        updated_at: env.ledger().timestamp(),
        settled_at: None,
    };
    
    crate::storage::InvoiceStorage::store_invoice(&env, &invoice);

    let result = client.try_verify_invoice(&non_admin, &invoice_id);
    assert!(result.is_err(), "Expected authorization error");

    let result = client.try_update_invoice_status(&non_admin, &invoice_id, &InvoiceStatus::Verified);
    assert!(result.is_err(), "Expected authorization error");

    let result = client.try_verify_invoice(&admin, &invoice_id);
    assert!(result.is_ok(), "Admin should be able to verify invoice");
}
