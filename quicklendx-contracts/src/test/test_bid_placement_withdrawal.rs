#![cfg(test)]

//! Bid placement validation tests.
//!
//! Withdrawal authorization and status-transition matrix cases are covered by
//! `src/test_withdraw_bid_matrix.rs`, and accepted-bid exclusivity cases are
//! covered by `src/test_bid_exclusivity.rs`.

use crate::bid::BidStatus;
use crate::errors::QuickLendXError;
use crate::invoice::InvoiceCategory;
use crate::{QuickLendXContract, QuickLendXContractClient};
use soroban_sdk::{testutils::Address as _, Address, Bytes, BytesN, Env, Vec};

fn setup_env() -> (Env, QuickLendXContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize_admin(&admin);
    (env, client, admin)
}

fn setup_verified_business(
    env: &Env,
    client: &QuickLendXContractClient,
    admin: &Address,
) -> Address {
    let business = Address::generate(env);
    client.submit_kyc_application(&business, &Bytes::from_slice(env, b"Business KYC"));
    client.verify_business(admin, &business);
    business
}

fn setup_verified_investor(env: &Env, client: &QuickLendXContractClient, limit: i128) -> Address {
    let investor = Address::generate(env);
    client.submit_investor_kyc(&investor, &Bytes::from_slice(env, b"Investor KYC"));
    client.verify_investor(&investor, &limit);
    investor
}

fn create_verified_invoice(
    env: &Env,
    client: &QuickLendXContractClient,
    admin: &Address,
    business: &Address,
    currency: &Address,
    amount: i128,
    nonce_byte: u8,
) -> BytesN<32> {
    let invoice_id = client.store_invoice(
        business,
        &amount,
        currency,
        &(env.ledger().timestamp() + 86_400),
        &Bytes::from_slice(env, b"Bid placement invoice"),
        &InvoiceCategory::Services,
        &Vec::new(env),
        &BytesN::from_array(env, &[nonce_byte; 32]),
    );
    client.verify_invoice(admin, &invoice_id);
    invoice_id
}

#[test]
fn test_place_bid_valid_succeeds() {
    let (env, client, admin) = setup_env();
    let business = setup_verified_business(&env, &client, &admin);
    let investor = setup_verified_investor(&env, &client, 25_000);
    let currency = Address::generate(&env);
    let invoice_id =
        create_verified_invoice(&env, &client, &admin, &business, &currency, 10_000, 1);

    let bid_id = client.place_bid(
        &investor,
        &invoice_id,
        &5_000,
        &5_700,
        &BytesN::from_array(&env, &[0u8; 32]),
    );
    let bid = client.get_bid(&bid_id).expect("bid should exist");

    assert_eq!(bid.status, BidStatus::Placed);
    assert_eq!(bid.investor, investor);
    assert_eq!(bid.bid_amount, 5_000);
    assert_eq!(bid.expected_return, 5_700);
}

#[test]
fn test_place_bid_unverified_investor_fails() {
    let (env, client, admin) = setup_env();
    let business = setup_verified_business(&env, &client, &admin);
    let unverified_investor = Address::generate(&env);
    let currency = Address::generate(&env);
    let invoice_id =
        create_verified_invoice(&env, &client, &admin, &business, &currency, 10_000, 2);

    let result = client.try_place_bid(
        &unverified_investor,
        &invoice_id,
        &5_000,
        &5_700,
        &BytesN::from_array(&env, &[0u8; 32]),
    );
    assert_eq!(result, Err(Ok(QuickLendXError::BusinessNotVerified)));
}

#[test]
fn test_place_bid_over_limit_fails() {
    let (env, client, admin) = setup_env();
    let business = setup_verified_business(&env, &client, &admin);
    let investor = setup_verified_investor(&env, &client, 1_000);
    let currency = Address::generate(&env);
    let invoice_id =
        create_verified_invoice(&env, &client, &admin, &business, &currency, 10_000, 3);

    let result = client.try_place_bid(
        &investor,
        &invoice_id,
        &1_500,
        &1_700,
        &BytesN::from_array(&env, &[0u8; 32]),
    );
    assert_eq!(result, Err(Ok(QuickLendXError::InvalidAmount)));
}

#[test]
fn test_place_bid_wrong_invoice_status_fails() {
    let (env, client, admin) = setup_env();
    let business = setup_verified_business(&env, &client, &admin);
    let investor = setup_verified_investor(&env, &client, 25_000);
    let currency = Address::generate(&env);

    let invoice_id = client.store_invoice(
        &business,
        &10_000,
        &currency,
        &(env.ledger().timestamp() + 86_400),
        &Bytes::from_slice(&env, b"Pending status invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
        &BytesN::from_array(&env, &[4u8; 32]),
    );

    let result = client.try_place_bid(
        &investor,
        &invoice_id,
        &5_000,
        &5_700,
        &BytesN::from_array(&env, &[0u8; 32]),
    );
    assert_eq!(result, Err(Ok(QuickLendXError::InvalidStatus)));
}
