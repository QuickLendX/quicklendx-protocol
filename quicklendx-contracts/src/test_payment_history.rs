#![cfg(test)]

use super::*;
use crate::settlement::{get_payment_records, record_payment};
use soroban_sdk::{testutils::{Address as _, Ledger}, Address, BytesN, Env, String, Vec};

#[test]
fn test_payment_history_ordering_and_pagination() {
    let env = Env::default();
    env.mock_all_auths();

    // Register contract
    let contract_id = env.register_contract(None, QuickLendXContract);
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let business = Address::generate(&env);
    let investor = Address::generate(&env);
    let currency = Address::generate(&env);
    
    // Setup invoice
    let invoice_id = setup_test_invoice(&env, &client, &business, &investor, &currency);

    // Record 5 payments
    for i in 1..=5 {
        let nonce = String::from_str(&env, &format!("nonce_{}", i));
        client.record_payment(&invoice_id, &(i as i128 * 100), &env.ledger().timestamp(), &nonce);
        env.ledger().set_timestamp(env.ledger().timestamp() + 100);
    }

    // Test: Get all records (from 0, limit 10)
    let all_records = client.get_payment_records(&invoice_id, &0, &10);
    assert_eq!(all_records.len(), 5);
    for i in 0..5 {
        assert_eq!(all_records.get(i).unwrap().amount, (i as i128 + 1) * 100);
    }

    // Test: Pagination (from 2, limit 2)
    let page = client.get_payment_records(&invoice_id, &2, &2);
    assert_eq!(page.len(), 2);
    assert_eq!(page.get(0).unwrap().amount, 300);
    assert_eq!(page.get(1).unwrap().amount, 400);

    // Test: Out of bounds (from 10, limit 5)
    let empty_page = client.get_payment_records(&invoice_id, &10, &5);
    assert_eq!(empty_page.len(), 0);
}

#[test]
fn test_payment_history_deduplication() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, QuickLendXContract);
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let business = Address::generate(&env);
    let investor = Address::generate(&env);
    let currency = Address::generate(&env);
    
    let invoice_id = setup_test_invoice(&env, &client, &business, &investor, &currency);

    let nonce = String::from_str(&env, "duplicate_nonce");
    let timestamp = env.ledger().timestamp();

    // First payment
    client.record_payment(&invoice_id, &1000, &timestamp, &nonce);
    
    // Duplicate payment (same nonce)
    client.record_payment(&invoice_id, &1000, &timestamp, &nonce);

    // Should only have 1 record
    let records = client.get_payment_records(&invoice_id, &0, &10);
    assert_eq!(records.len(), 1);
}

fn setup_test_invoice(
    env: &Env,
    client: &QuickLendXContractClient,
    business: &Address,
    investor: &Address,
    currency: &Address,
) -> BytesN<32> {
    let admin = Address::generate(env);
    client.set_admin(&admin);

    client.submit_kyc_application(business, &String::from_str(env, "KYC"));
    client.verify_business(&admin, business);

    let invoice_id = client.store_invoice(
        business,
        &10000,
        currency,
        &(env.ledger().timestamp() + 86400),
        &String::from_str(env, "Test"),
        &crate::invoice::InvoiceCategory::Services,
        &Vec::new(env),
    );
    client.verify_invoice(&invoice_id);
    
    client.submit_investor_kyc(investor, &String::from_str(env, "Investor KYC"));
    client.verify_investor(investor, &10000);
    
    let bid_id = client.place_bid(investor, &invoice_id, &5000, &10000);
    client.accept_bid(&invoice_id, &bid_id);
    
    invoice_id
}
