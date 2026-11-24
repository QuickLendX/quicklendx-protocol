#![cfg(test)]

//! Comprehensive tests for `get_invoice` and `get_bid` single entity retrieval.

use crate::bid::BidStatus;
use crate::errors::QuickLendXError;
use crate::invoice::{InvoiceCategory, InvoiceStatus};
use crate::{QuickLendXContract, QuickLendXContractClient};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    vec, Address, Bytes, BytesN, Env, Vec,
};

fn setup_contract() -> (Env, QuickLendXContractClient<'static>, Address) {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    env.mock_all_auths();
    let admin = Address::generate(&env);
    client.initialize_admin(&admin);
    (env, client, admin)
}

fn create_verified_business(
    env: &Env,
    client: &QuickLendXContractClient,
    admin: &Address,
) -> Address {
    let business = Address::generate(env);
    client.submit_kyc_application(&business, &Bytes::from_slice(env, b"Business KYC data"));
    client.verify_business(admin, &business);
    business
}

fn create_verified_investor(env: &Env, client: &QuickLendXContractClient, limit: i128) -> Address {
    let investor = Address::generate(env);
    client.submit_investor_kyc(&investor, &Bytes::from_slice(env, b"Investor KYC data"));
    client.verify_investor(&investor, &limit);
    investor
}

fn create_and_verify_invoice(
    env: &Env,
    client: &QuickLendXContractClient,
    admin: &Address,
    business: &Address,
    amount: i128,
    category: InvoiceCategory,
    nonce_byte: u8,
) -> BytesN<32> {
    let currency = Address::generate(env);
    let due_date = env.ledger().timestamp() + 86_400;
    let description = Bytes::from_slice(env, b"Test invoice");

    let invoice_id = client.store_invoice(
        business,
        &amount,
        &currency,
        &due_date,
        &description,
        &category,
        &Vec::new(env),
        &BytesN::from_array(env, &[nonce_byte; 32]),
    );

    client.verify_invoice(admin, &invoice_id);
    invoice_id
}

fn place_bid(
    env: &Env,
    client: &QuickLendXContractClient,
    investor: &Address,
    invoice_id: &BytesN<32>,
    bid_amount: i128,
    expected_return: i128,
    salt_byte: u8,
) -> BytesN<32> {
    client.place_bid(
        investor,
        invoice_id,
        &bid_amount,
        &expected_return,
        &BytesN::from_array(env, &[salt_byte; 32]),
    )
}

// ============================================================================
// GET_INVOICE TESTS
// ============================================================================

#[test]
fn test_get_invoice_ok_with_correct_data() {
    let (env, client, admin) = setup_contract();
    let business = create_verified_business(&env, &client, &admin);

    let currency = Address::generate(&env);
    let amount = 5_000i128;
    let due_date = env.ledger().timestamp() + 86_400;
    let description = Bytes::from_slice(&env, b"Test invoice for retrieval");

    let invoice_id = client.store_invoice(
        &business,
        &amount,
        &currency,
        &due_date,
        &description,
        &InvoiceCategory::Services,
        &Vec::new(&env),
        &BytesN::from_array(&env, &[1u8; 32]),
    );

    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.invoice_id, invoice_id);
    assert_eq!(invoice.business, business);
    assert_eq!(invoice.amount, amount);
    assert_eq!(invoice.currency, currency);
    assert_eq!(invoice.due_date, due_date);
    assert_eq!(invoice.description, description);
    assert_eq!(invoice.status, InvoiceStatus::Pending);
    assert_eq!(invoice.funded_amount, 0);
    assert!(invoice.investor.is_none());
}

#[test]
fn test_get_invoice_ok_all_categories() {
    let (env, client, admin) = setup_contract();
    let business = create_verified_business(&env, &client, &admin);

    let categories = vec![
        &env,
        InvoiceCategory::Services,
        InvoiceCategory::Products,
        InvoiceCategory::Consulting,
        InvoiceCategory::Manufacturing,
        InvoiceCategory::Technology,
        InvoiceCategory::Healthcare,
        InvoiceCategory::Other,
    ];

    for (idx, category) in categories.iter().enumerate() {
        env.ledger().set_timestamp(100 + idx as u64);
        let invoice_id = create_and_verify_invoice(
            &env,
            &client,
            &admin,
            &business,
            1_000,
            category.clone(),
            (idx + 10) as u8,
        );

        let invoice = client.get_invoice(&invoice_id);
        assert_eq!(invoice.category, category);
    }
}

#[test]
fn test_get_invoice_ok_after_status_transitions() {
    let (env, client, admin) = setup_contract();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, 100_000);

    let invoice_id = create_and_verify_invoice(
        &env,
        &client,
        &admin,
        &business,
        5_000,
        InvoiceCategory::Services,
        20,
    );

    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Verified);

    let bid_id = place_bid(&env, &client, &investor, &invoice_id, 5_000, 5_500, 1);
    client.accept_bid(&invoice_id, &bid_id);

    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Funded);
    assert_eq!(invoice.funded_amount, 5_000);
    assert_eq!(invoice.investor, Some(investor));
}

#[test]
fn test_get_invoice_err_nonexistent_invoice() {
    let (env, client, _admin) = setup_contract();
    let nonexistent_id = BytesN::from_array(&env, &[1u8; 32]);

    let result = client.try_get_invoice(&nonexistent_id);
    assert_eq!(result, Err(Ok(QuickLendXError::InvoiceNotFound)));
}

#[test]
fn test_get_invoice_err_multiple_random_bytesn32() {
    let (env, client, _admin) = setup_contract();

    for i in 0..5u8 {
        let mut random_bytes = [0u8; 32];
        random_bytes[0] = i;
        let random_id = BytesN::from_array(&env, &random_bytes);

        let result = client.try_get_invoice(&random_id);
        assert_eq!(result, Err(Ok(QuickLendXError::InvoiceNotFound)));
    }
}

#[test]
fn test_get_invoice_ok_multiple_invoices() {
    let (env, client, admin) = setup_contract();
    let business = create_verified_business(&env, &client, &admin);
    let mut invoice_ids = Vec::new(&env);

    for i in 0..5u8 {
        env.ledger().set_timestamp(200 + i as u64);
        let amount = 1_000 + (i as i128) * 100;
        let invoice_id = create_and_verify_invoice(
            &env,
            &client,
            &admin,
            &business,
            amount,
            InvoiceCategory::Services,
            30 + i,
        );
        invoice_ids.push_back(invoice_id.clone());

        let invoice = client.get_invoice(&invoice_id);
        assert_eq!(invoice.amount, amount);
    }

    for invoice_id in invoice_ids.iter() {
        assert!(client.try_get_invoice(&invoice_id).is_ok());
    }
}

#[test]
fn test_get_invoice_ok_with_tags() {
    let (env, client, admin) = setup_contract();
    let business = create_verified_business(&env, &client, &admin);

    let currency = Address::generate(&env);
    let amount = 3_000i128;
    let due_date = env.ledger().timestamp() + 86_400;
    let description = Bytes::from_slice(&env, b"Invoice with tags");

    let mut tags = Vec::new(&env);
    tags.push_back(Bytes::from_slice(&env, b"urgent"));
    tags.push_back(Bytes::from_slice(&env, b"services"));

    let invoice_id = client.store_invoice(
        &business,
        &amount,
        &currency,
        &due_date,
        &description,
        &InvoiceCategory::Services,
        &tags,
        &BytesN::from_array(&env, &[40u8; 32]),
    );

    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.tags.len(), 2);
    assert_eq!(
        invoice.tags.get(0).unwrap(),
        Bytes::from_slice(&env, b"urgent")
    );
    assert_eq!(
        invoice.tags.get(1).unwrap(),
        Bytes::from_slice(&env, b"services")
    );
}

// ============================================================================
// GET_BID TESTS
// ============================================================================

#[test]
fn test_get_bid_some_with_correct_data() {
    let (env, client, admin) = setup_contract();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, 100_000);

    let invoice_id = create_and_verify_invoice(
        &env,
        &client,
        &admin,
        &business,
        5_000,
        InvoiceCategory::Services,
        50,
    );

    let bid_amount = 4_500i128;
    let expected_return = 5_000i128;
    let bid_id = place_bid(
        &env,
        &client,
        &investor,
        &invoice_id,
        bid_amount,
        expected_return,
        1,
    );

    let bid = client.get_bid(&bid_id).expect("bid should exist");
    assert_eq!(bid.bid_id, bid_id);
    assert_eq!(bid.invoice_id, invoice_id);
    assert_eq!(bid.investor, investor);
    assert_eq!(bid.bid_amount, bid_amount);
    assert_eq!(bid.expected_return, expected_return);
    assert_eq!(bid.status, BidStatus::Placed);
    assert!(bid.expiration_timestamp >= bid.timestamp);
}

#[test]
fn test_get_bid_some_multiple_bids_same_invoice() {
    let (env, client, admin) = setup_contract();
    let business = create_verified_business(&env, &client, &admin);
    let invoice_id = create_and_verify_invoice(
        &env,
        &client,
        &admin,
        &business,
        10_000,
        InvoiceCategory::Services,
        51,
    );

    let mut bid_ids = Vec::new(&env);
    let mut investors = Vec::new(&env);

    for i in 0..3u8 {
        let investor = create_verified_investor(&env, &client, 100_000);
        investors.push_back(investor.clone());
        let bid_amount = 9_000i128 + (i as i128) * 100;
        let expected_return = 10_000i128 + (i as i128) * 100;

        let bid_id = place_bid(
            &env,
            &client,
            &investor,
            &invoice_id,
            bid_amount,
            expected_return,
            i + 1,
        );
        bid_ids.push_back(bid_id);
    }

    for (i, bid_id) in bid_ids.iter().enumerate() {
        let bid = client.get_bid(&bid_id).expect("bid should exist");
        assert_eq!(bid.bid_id, bid_id);
        assert_eq!(bid.invoice_id, invoice_id);
        assert_eq!(bid.investor, investors.get(i as u32).unwrap());
    }
}

#[test]
fn test_get_bid_none_nonexistent_bid() {
    let (env, client, _admin) = setup_contract();
    let nonexistent_id = BytesN::from_array(&env, &[2u8; 32]);

    assert!(client.get_bid(&nonexistent_id).is_none());
}

#[test]
fn test_get_bid_none_multiple_random_bytesn32() {
    let (env, client, _admin) = setup_contract();

    for i in 0..5u8 {
        let mut random_bytes = [0u8; 32];
        random_bytes[0] = i;
        let random_id = BytesN::from_array(&env, &random_bytes);

        assert!(client.get_bid(&random_id).is_none());
    }
}

#[test]
fn test_get_bid_retrievable_after_expiration() {
    let (env, client, admin) = setup_contract();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, 100_000);

    let invoice_id = create_and_verify_invoice(
        &env,
        &client,
        &admin,
        &business,
        5_000,
        InvoiceCategory::Services,
        52,
    );
    let bid_id = place_bid(&env, &client, &investor, &invoice_id, 4_500, 5_000, 1);

    let bid = client.get_bid(&bid_id).expect("bid should exist");
    assert!(!bid.is_expired(env.ledger().timestamp()));

    env.ledger().set_timestamp(bid.expiration_timestamp + 1_000);

    let bid_after = client
        .get_bid(&bid_id)
        .expect("expired bid remains retrievable");
    assert_eq!(bid_after.bid_id, bid_id);
    assert!(bid_after.is_expired(env.ledger().timestamp()));
}
