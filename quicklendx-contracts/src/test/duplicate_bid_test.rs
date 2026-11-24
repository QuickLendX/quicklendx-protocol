// src/test/duplicate_bid_test.rs
//! Test that submitting the same bid with identical `(invoice_id, investor, salt)` fails with
//! `QuickLendXError::DuplicateBid`.

#![cfg(test)]

use crate::errors::QuickLendXError;
use crate::invoice::InvoiceCategory;
use crate::{QuickLendXContract, QuickLendXContractClient};
use soroban_sdk::{testutils::Address as _, Address, Bytes, BytesN, Env, Vec};

#[test]
fn test_duplicate_bid_idempotency() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    client.initialize_admin(&admin);

    let business = Address::generate(&env);
    client.submit_kyc_application(&business, &Bytes::from_slice(&env, b"business-kyc"));
    client.verify_business(&admin, &business);

    let investor = Address::generate(&env);
    client.submit_investor_kyc(&investor, &Bytes::from_slice(&env, b"investor-kyc"));
    client.verify_investor(&investor, &10_000i128);

    let currency = Address::generate(&env);
    let amount: i128 = 1_000;
    let due_date = env.ledger().timestamp() + 86_400;
    let nonce = BytesN::from_array(&env, &[9u8; 32]);

    let invoice_id = client.store_invoice(
        &business,
        &amount,
        &currency,
        &due_date,
        &Bytes::from_slice(&env, b"Duplicate bid invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
        &nonce,
    );
    client.verify_invoice(&admin, &invoice_id);

    // Prepare a deterministic salt
    let salt = BytesN::from_array(&env, &[1u8; 32]);

    // First bid should succeed
    let first_bid = client.try_place_bid(&investor, &invoice_id, &amount, &(amount + 100), &salt);
    assert!(first_bid.is_ok());

    // Second bid with identical (invoice_id, investor, salt) must fail with DuplicateBid
    let duplicate_bid =
        client.try_place_bid(&investor, &invoice_id, &amount, &(amount + 100), &salt);
    assert_eq!(duplicate_bid, Err(Ok(QuickLendXError::DuplicateBid)));

    // A different salt from the same investor on the same invoice should succeed
    let other_salt = BytesN::from_array(&env, &[2u8; 32]);
    let second_bid = client.try_place_bid(
        &investor,
        &invoice_id,
        &amount,
        &(amount + 100),
        &other_salt,
    );
    assert!(second_bid.is_ok());
}
