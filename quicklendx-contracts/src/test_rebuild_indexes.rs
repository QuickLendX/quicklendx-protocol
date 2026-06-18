#![cfg(test)]

use soroban_sdk::{testutils::Address as _, Address, BytesN, Env, String, Vec};

use crate::types::{RebuildReport, InvoiceCategory};
use crate::{QuickLendXContract, QuickLendXContractClient};

fn setup() -> (Env, Address, QuickLendXContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000_000);
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize_admin(&admin);
    (env, contract_id, client, admin)
}

fn make_invoice(env: &Env, client: &QuickLendXContractClient) -> BytesN<32> {
    let business = Address::generate(env);
    let currency = Address::generate(env);
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

#[test]
fn test_rebuild_empty_range() {
    let (env, _cid, client, admin) = setup();
    make_invoice(&env, &client);

    let report = client.rebuild_invoice_indexes(&admin, &100, &50);
    assert_eq!(report.scanned, 0);
    assert_eq!(report.reindexed, 0);
    assert_eq!(report.next_offset, 100);
}

#[test]
fn test_rebuild_pagination() {
    let (env, _cid, client, admin) = setup();
    
    for _ in 0..3 {
        make_invoice(&env, &client);
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

#[test] 
fn test_rebuild_single_invoice() {
    let (env, _cid, client, admin) = setup();
    let _invoice_id = make_invoice(&env, &client);

    let report = client.rebuild_invoice_indexes(&admin, &0, &50);
    assert_eq!(report.scanned, 1);
    assert_eq!(report.reindexed, 1);
    assert_eq!(report.next_offset, 1);
}

#[test]
fn test_rebuild_non_admin_rejected() {
    let (env, _cid, client, admin) = setup();
    make_invoice(&env, &client);

    let impostor = Address::generate(&env);
    let result = client.try_rebuild_invoice_indexes(&impostor, &0, &50);
    assert!(result.is_err());
}