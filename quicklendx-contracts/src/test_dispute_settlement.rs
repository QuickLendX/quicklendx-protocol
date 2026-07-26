#![cfg(test)]

use crate::contract::{QuickLendXContract, QuickLendXContractClient};
use crate::errors::QuickLendXError;
use crate::types::{DisputeStatus, InvoiceCategory, InvoiceStatus};
use crate::test::{setup_verified_business, setup_verified_investor, setup_token};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{token, Address, Env, String, BytesN, Vec};

fn setup_funded_invoice(
    env: &Env,
    client: &QuickLendXContractClient,
    business: &Address,
    investor: &Address,
    admin: &Address,
    amount: i128,
) -> (BytesN<32>, Address) {
    let contract_id = client.address.clone();
    let currency = setup_token(env, business, investor, &contract_id);

    // Make sure limits allow this amount
    client.set_protocol_limits(admin, &amount, &365u64, &0u64);

    let invoice_id = client.store_invoice(
        business,
        &amount,
        &currency,
        &(env.ledger().timestamp() + 86400),
        &String::from_str(env, "Test invoice for dispute settlement test"),
        &InvoiceCategory::Services,
        &Vec::new(env),
        &None);

    client.verify_invoice(&invoice_id);

    let salt = BytesN::from_array(env, &[0u8; 32]);
    let bid_id = client.place_bid(investor, &invoice_id, &amount, &(amount + 1000), &salt);
    client.accept_bid(&invoice_id, &bid_id);

    (invoice_id, currency)
}

#[test]
fn test_settle_invoice_blocks_when_dispute_is_open() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    client.initialize_admin(&admin);
    client.set_admin(&admin);

    let business = setup_verified_business(&env, &client, &admin);
    let investor = setup_verified_investor(&env, &client, 200_000);

    let amount: i128 = 100_000;
    let (invoice_id, _currency) =
        setup_funded_invoice(&env, &client, &business, &investor, &admin, amount);

    // Business opens dispute
    client.create_dispute(
        &invoice_id,
        &business,
        &String::from_str(&env, "dispute reason"),
        &String::from_str(&env, "evidence description"),
    );

    // Verify dispute status is Disputed (Open dispute)
    let invoice = client.get_invoice(&invoice_id).unwrap();
    assert_eq!(invoice.dispute_status, DisputeStatus::Disputed);

    // Settle invoice should be BLOCKED (returns InvalidStatus)
    let result = client.try_settle_invoice(&invoice_id, &amount);
    assert_eq!(result, Err(Ok(QuickLendXError::InvalidStatus)));

    // Advance to UnderReview
    client.put_dispute_under_review(&invoice_id, &admin);
    let invoice_review = client.get_invoice(&invoice_id).unwrap();
    assert_eq!(invoice_review.dispute_status, DisputeStatus::UnderReview);

    // Settle invoice should STILL be BLOCKED under review
    let result_review = client.try_settle_invoice(&invoice_id, &amount);
    assert_eq!(result_review, Err(Ok(QuickLendXError::InvalidStatus)));
}

#[test]
fn test_settle_invoice_allows_when_dispute_is_resolved() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    client.initialize_admin(&admin);
    client.set_admin(&admin);

    let business = setup_verified_business(&env, &client, &admin);
    let investor = setup_verified_investor(&env, &client, 200_000);

    let amount: i128 = 100_000;
    let (invoice_id, currency) =
        setup_funded_invoice(&env, &client, &business, &investor, &admin, amount);

    // Business opens dispute
    client.create_dispute(
        &invoice_id,
        &business,
        &String::from_str(&env, "dispute reason"),
        &String::from_str(&env, "evidence description"),
    );

    // Review dispute
    client.put_dispute_under_review(&invoice_id, &admin);

    // Resolve dispute in favor of business (DisputeStatus becomes Resolved)
    client.resolve_dispute(&invoice_id, &admin, &String::from_str(&env, "dispute resolved"));
    let invoice = client.get_invoice(&invoice_id).unwrap();
    assert_eq!(invoice.dispute_status, DisputeStatus::Resolved);

    // Mint token allowance/balance for business to make payment
    let token_client = token::Client::new(&env, &currency);
    let token_admin = token::StellarAssetClient::new(&env, &currency);
    token_admin.mint(&business, &amount);
    let expiry = env.ledger().sequence() + 10_000;
    token_client.approve(&business, &contract_id, &amount, &expiry);

    // Settle invoice should SUCCEED
    let result = client.try_settle_invoice(&invoice_id, &amount);
    assert!(result.is_ok());

    let final_invoice = client.get_invoice(&invoice_id).unwrap();
    assert_eq!(final_invoice.status, InvoiceStatus::Paid);
}

#[test]
fn test_settle_invoice_allows_when_no_dispute_exists() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    client.initialize_admin(&admin);
    client.set_admin(&admin);

    let business = setup_verified_business(&env, &client, &admin);
    let investor = setup_verified_investor(&env, &client, 200_000);

    let amount: i128 = 100_000;
    let (invoice_id, currency) =
        setup_funded_invoice(&env, &client, &business, &investor, &admin, amount);

    // Verify dispute status is None (No dispute)
    let invoice = client.get_invoice(&invoice_id).unwrap();
    assert_eq!(invoice.dispute_status, DisputeStatus::None);

    // Mint token allowance/balance for business to make payment
    let token_client = token::Client::new(&env, &currency);
    let token_admin = token::StellarAssetClient::new(&env, &currency);
    token_admin.mint(&business, &amount);
    let expiry = env.ledger().sequence() + 10_000;
    token_client.approve(&business, &contract_id, &amount, &expiry);

    // Settle invoice should SUCCEED
    let result = client.try_settle_invoice(&invoice_id, &amount);
    assert!(result.is_ok());

    let final_invoice = client.get_invoice(&invoice_id).unwrap();
    assert_eq!(final_invoice.status, InvoiceStatus::Paid);
}
