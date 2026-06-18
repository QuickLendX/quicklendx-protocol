#![cfg(test)]
use soroban_sdk::{testutils::Address as _, Address, BytesN, Env, String, Vec};
use crate::storage::InvoiceStorage;
use crate::types::{InvoiceCategory, InvoiceMetadata, LineItemRecord, RebuildReport};
use crate::{QuickLendXContract, QuickLendXContractClient};

fn setup() -> (Env, Address, QuickLendXContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000_000);
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.set_admin(&admin);
    let _ = client.try_initialize_protocol_limits(&admin, &1i128, &365u64, &86_400u64);
    (env, contract_id, client, admin)
}

fn make_invoice(env: &Env, client: &QuickLendXContractClient, admin: &Address) -> BytesN<32> {
    let business = Address::generate(env);
    let currency = Address::generate(env);
    client.submit_kyc_application(&business, &String::from_str(env, "KYC"));
    client.verify_business(admin, &business);
    client.upload_invoice(
        &business,
        &1000,
        &currency,
        &(env.ledger().timestamp() + 86_400),
        &String::from_str(env, "Test invoice"),
        &InvoiceCategory::Services,
        &Vec::new(env),
    )
}

fn set_metadata(env: &Env, client: &QuickLendXContractClient, invoice_id: &BytesN<32>, name: &str) {
    client.update_invoice_metadata(
        invoice_id,
        &InvoiceMetadata {
            customer_name: String::from_str(env, name),
            customer_address: String::from_str(env, "Addr"),
            tax_id: String::from_str(env, "TAX-1"),
            line_items: Vec::from_array(
                env,
                [LineItemRecord(String::from_str(env, "Item"), 1, 1000, 1000)],
            ),
            notes: String::from_str(env, ""),
        },
    );
}

// 1. Corruption recovery
#[test]
fn test_rebuild_fixes_corrupted_indexes() {
    let (env, contract_id, client, admin) = setup();
    let invoice_id = make_invoice(&env, &client, &admin);
    let customer = String::from_str(&env, "Acme Corp");
    set_metadata(&env, &client, &invoice_id, "Acme Corp");
    // Corrupt customer index directly via storage
    env.as_contract(&contract_id, || {
        InvoiceStorage::remove_from_customer_index(&env, &customer, &invoice_id);
    });
    // Confirm corruption
    let ids_before = client.get_invoices_by_customer(&customer);
    assert!(!ids_before.iter().any(|id| id == invoice_id));
    // Rebuild
    let report = client.rebuild_invoice_indexes(&admin, &0, &50);
    assert_eq!(report.reindexed, 1);
    // Index restored
    let ids_after = client.get_invoices_by_customer(&customer);
    assert!(ids_after.iter().any(|id| id == invoice_id));
}

// 2. Empty range returns zero report
#[test]
fn test_rebuild_empty_range() {
    let (env, _cid, client, admin) = setup();
    make_invoice(&env, &client, &admin);
    let report = client.rebuild_invoice_indexes(&admin, &100, &50);
    assert_eq!(report.scanned, 0);
    assert_eq!(report.reindexed, 0);
    assert_eq!(report.next_offset, 100);
}

// 3. Two-page resume over 3 invoices
#[test]
fn test_rebuild_two_page_resume() {
    let (env, _cid, client, admin) = setup();
    for _ in 0..3 {
        make_invoice(&env, &client, &admin);
    }
    let p1: RebuildReport = client.rebuild_invoice_indexes(&admin, &0, &2);
    assert_eq!(p1.scanned, 2);
    assert_eq!(p1.next_offset, 2);
    let p2: RebuildReport = client.rebuild_invoice_indexes(&admin, &p1.next_offset, &2);
    assert_eq!(p2.scanned, 1);
    assert_eq!(p2.next_offset, 3);
    let p3: RebuildReport = client.rebuild_invoice_indexes(&admin, &p2.next_offset, &2);
    assert_eq!(p3.scanned, 0);
}

// 4. Idempotency — running twice gives same counts, no duplicate index entries
#[test]
fn test_rebuild_idempotent() {
    let (env, contract_id, client, admin) = setup();
    let invoice_id = make_invoice(&env, &client, &admin);
    set_metadata(&env, &client, &invoice_id, "Beta Ltd");
    let r1 = client.rebuild_invoice_indexes(&admin, &0, &50);
    let r2 = client.rebuild_invoice_indexes(&admin, &0, &50);
    assert_eq!(r1.scanned, r2.scanned);
    assert_eq!(r1.reindexed, r2.reindexed);
    // No duplicate entries in index
    let customer = String::from_str(&env, "Beta Ltd");
    env.as_contract(&contract_id, || {
        let ids = InvoiceStorage::get_invoices_by_customer(&env, &customer);
        let count = ids.iter().filter(|id| id == invoice_id).count();
        assert_eq!(count, 1);
    });
}

// 5. Non-admin rejected
#[test]
fn test_rebuild_rejects_non_admin() {
    let (env, _cid, client, _admin) = setup();
    let impostor = Address::generate(&env);
    let result = client.try_rebuild_invoice_indexes(&impostor, &0, &50);
    assert!(result.is_err());
}
