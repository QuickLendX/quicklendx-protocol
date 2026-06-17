#![cfg(test)]

//! Tests for `rebuild_invoice_indexes` / `InvoiceStorage::rebuild_indexes_page`.
//!
//! Covered scenarios
//! -----------------
//! 1. Basic rebuild — after deliberately corrupting indexes, a single-page
//!    rebuild restores them.
//! 2. Empty range — offset past end returns a zero-count report without
//!    panicking.
//! 3. Two-page resume — a dataset of 3 invoices can be rebuilt in two calls
//!    of limit=2 and the results compose correctly.
//! 4. Idempotency — running rebuild twice yields the same index state.
//! 5. Non-admin rejected — a random address gets `NotAdmin`.

use soroban_sdk::{testutils::Address as _, Address, Env, String, Vec};

use crate::storage::{Indexes, InvoiceStorage};
use crate::types::{InvoiceCategory, InvoiceStatus, RebuildReport};
use crate::{QuickLendXContract, QuickLendXContractClient};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Register the contract and return (env, client, admin).
fn setup() -> (Env, QuickLendXContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    client.initialize_admin(&admin);

    (env, client, admin)
}

/// Upload one invoice via the contract client; returns its ID.
fn upload_invoice(
    env: &Env,
    client: &QuickLendXContractClient,
    admin: &Address,
    business: &Address,
) -> soroban_sdk::BytesN<32> {
    // KYC
    client.submit_kyc_application(business, &String::from_str(env, "KYC"));
    client.verify_business(admin, business);

    let currency = Address::generate(env);
    client.upload_invoice(
        business,
        &1000,
        &currency,
        &(env.ledger().timestamp() + 86_400),
        &String::from_str(env, "Test invoice"),
        &InvoiceCategory::Services,
        &Vec::new(env),
    )
}

/// Directly corrupt the customer-name index for an invoice by removing its ID.
fn corrupt_customer_index(
    env: &Env,
    contract_id: &Address,
    customer_name: &String,
    invoice_id: &soroban_sdk::BytesN<32>,
) {
    env.as_contract(contract_id, || {
        InvoiceStorage::remove_from_customer_index(env, customer_name, invoice_id);
    });
}

/// Directly corrupt the category index.
fn corrupt_category_index(
    env: &Env,
    contract_id: &Address,
    category: InvoiceCategory,
    invoice_id: &soroban_sdk::BytesN<32>,
) {
    env.as_contract(contract_id, || {
        InvoiceStorage::remove_category_index(env, &category, invoice_id);
    });
}

/// Assert the customer index contains `invoice_id`.
fn assert_in_customer_index(
    env: &Env,
    contract_id: &Address,
    customer_name: &String,
    invoice_id: &soroban_sdk::BytesN<32>,
) {
    env.as_contract(contract_id, || {
        let ids = InvoiceStorage::get_invoices_by_customer(env, customer_name);
        assert!(
            ids.iter().any(|id| id == *invoice_id),
            "invoice should be in customer index"
        );
    });
}

/// Assert the category index contains `invoice_id`.
fn assert_in_category_index(
    env: &Env,
    contract_id: &Address,
    category: InvoiceCategory,
    invoice_id: &soroban_sdk::BytesN<32>,
) {
    env.as_contract(contract_id, || {
        let ids = InvoiceStorage::get_invoices_by_category(env, &category);
        assert!(
            ids.iter().any(|id| id == *invoice_id),
            "invoice should be in category index"
        );
    });
}

// ---------------------------------------------------------------------------
// Test helpers for metadata
// ---------------------------------------------------------------------------

fn set_customer_metadata(
    env: &Env,
    client: &QuickLendXContractClient,
    invoice_id: &soroban_sdk::BytesN<32>,
    customer_name: &str,
) {
    use crate::types::{InvoiceMetadata, LineItemRecord};
    let meta = InvoiceMetadata {
        customer_name: String::from_str(env, customer_name),
        customer_address: String::from_str(env, "Addr"),
        tax_id: String::from_str(env, "TAX-1"),
        line_items: soroban_sdk::Vec::from_array(
            env,
            [LineItemRecord(String::from_str(env, "Item"), 1, 1000, 1000)],
        ),
        notes: String::from_str(env, ""),
    };
    client.update_invoice_metadata(invoice_id, &meta);
}

// ---------------------------------------------------------------------------
// 1. Corruption recovery — rebuild restores drifted customer + category index
// ---------------------------------------------------------------------------

#[test]
fn test_rebuild_fixes_corrupted_customer_and_category_index() {
    let (env, client, admin) = setup();
    let contract_id = env.current_contract_address();

    let business = Address::generate(&env);
    let invoice_id = upload_invoice(&env, &client, &admin, &business);
    let customer = String::from_str(&env, "Acme Corp");
    set_customer_metadata(&env, &client, &invoice_id, "Acme Corp");

    // Verify both indexes are present before corruption
    assert_in_customer_index(&env, &contract_id, &customer, &invoice_id);
    assert_in_category_index(&env, &contract_id, InvoiceCategory::Services, &invoice_id);

    // Corrupt both indexes
    corrupt_customer_index(&env, &contract_id, &customer, &invoice_id);
    corrupt_category_index(&env, &contract_id, InvoiceCategory::Services, &invoice_id);

    // Rebuild
    let report = client.rebuild_invoice_indexes(&admin, &0, &50);
    assert_eq!(report.scanned, 1);
    assert_eq!(report.reindexed, 1);

    // Both indexes are restored
    assert_in_customer_index(&env, &contract_id, &customer, &invoice_id);
    assert_in_category_index(&env, &contract_id, InvoiceCategory::Services, &invoice_id);
}

// ---------------------------------------------------------------------------
// 2. Empty range — offset past all invoices returns zero counts
// ---------------------------------------------------------------------------

#[test]
fn test_rebuild_empty_range_returns_zero_report() {
    let (env, client, admin) = setup();

    let business = Address::generate(&env);
    upload_invoice(&env, &client, &admin, &business);

    // offset=100 is past the 1 invoice
    let report = client.rebuild_invoice_indexes(&admin, &100, &50);
    assert_eq!(report.scanned, 0);
    assert_eq!(report.reindexed, 0);
    assert_eq!(report.next_offset, 100);
}

// ---------------------------------------------------------------------------
// 3. Two-page resume — 3 invoices rebuilt via two calls of limit=2
// ---------------------------------------------------------------------------

#[test]
fn test_rebuild_resumes_across_two_pages() {
    let (env, client, admin) = setup();

    // Upload 3 invoices from 3 different businesses
    for _ in 0..3 {
        let business = Address::generate(&env);
        upload_invoice(&env, &client, &admin, &business);
    }

    // Page 1: offset=0, limit=2
    let page1: RebuildReport = client.rebuild_invoice_indexes(&admin, &0, &2);
    assert_eq!(page1.scanned, 2);
    assert_eq!(page1.reindexed, 2);
    assert_eq!(page1.next_offset, 2);

    // Page 2: offset from page1
    let page2: RebuildReport = client.rebuild_invoice_indexes(&admin, &page1.next_offset, &2);
    assert_eq!(page2.scanned, 1);
    assert_eq!(page2.reindexed, 1);
    // next_offset should equal total invoice count (3)
    assert_eq!(page2.next_offset, 3);

    // Page 3: nothing left
    let page3: RebuildReport = client.rebuild_invoice_indexes(&admin, &page2.next_offset, &2);
    assert_eq!(page3.scanned, 0);
    assert_eq!(page3.reindexed, 0);
}

// ---------------------------------------------------------------------------
// 4. Idempotency — running rebuild twice leaves indexes identical
// ---------------------------------------------------------------------------

#[test]
fn test_rebuild_is_idempotent() {
    let (env, client, admin) = setup();
    let contract_id = env.current_contract_address();

    let business = Address::generate(&env);
    let invoice_id = upload_invoice(&env, &client, &admin, &business);
    let customer = String::from_str(&env, "Beta Ltd");
    set_customer_metadata(&env, &client, &invoice_id, "Beta Ltd");

    // Run once
    let r1 = client.rebuild_invoice_indexes(&admin, &0, &50);
    // Run again
    let r2 = client.rebuild_invoice_indexes(&admin, &0, &50);

    // Reports are identical
    assert_eq!(r1.scanned, r2.scanned);
    assert_eq!(r1.reindexed, r2.reindexed);

    // Index still contains exactly one entry for this invoice (no duplicates)
    env.as_contract(&contract_id, || {
        let ids = InvoiceStorage::get_invoices_by_customer(&env, &customer);
        let count = ids.iter().filter(|id| id == invoice_id).count();
        assert_eq!(count, 1, "index must not accumulate duplicate entries");
    });
}

// ---------------------------------------------------------------------------
// 5. Non-admin is rejected
// ---------------------------------------------------------------------------

#[test]
fn test_rebuild_rejects_non_admin() {
    let (env, client, _admin) = setup();
    let impostor = Address::generate(&env);

    let result = client.try_rebuild_invoice_indexes(&impostor, &0, &50);
    assert!(result.is_err());
}
