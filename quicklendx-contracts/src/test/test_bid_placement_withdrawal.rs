use super::*;
use crate::errors::QuickLendXError;
use soroban_sdk::{testutils::Address as _, Address, BytesN, Env, String, Vec};

fn create_verified_invoice(
    env: &Env,
    client: &QuickLendXContractClient,
    business: &Address,
    currency: &Address,
    amount: i128,
) -> BytesN<32> {
    let invoice_id = client.store_invoice(
        business,
        &amount,
        currency,
        &(env.ledger().timestamp() + 86_400),
        &String::from_str(env, "Issue #271 invoice"),
        &InvoiceCategory::Services,
        &Vec::new(env),
    );
    client.verify_invoice(&invoice_id);
    invoice_id
}

#[test]
fn test_place_bid_valid_succeeds() {
    let (env, client, admin, contract_addr) = setup_env();
    let business = setup_verified_business(&env, &client, &admin);
    let investor = setup_verified_investor(&env, &client, 25_000);
    let currency = setup_token(&env, &business, &investor, &contract_addr);
    let invoice_id = create_verified_invoice(&env, &client, &business, &currency, 10_000);

    let bid_id = client.place_bid(&investor, &invoice_id, &5_000, &5_700);
    let bid = client.get_bid(&bid_id).expect("bid should exist");

    assert_eq!(bid.status, BidStatus::Placed);
    assert_eq!(bid.investor, investor);
}

#[test]
fn test_place_bid_unverified_investor_fails() {
    let (env, client, admin, contract_addr) = setup_env();
    let business = setup_verified_business(&env, &client, &admin);
    let verified_investor = setup_verified_investor(&env, &client, 25_000);
    let unverified_investor = Address::generate(&env);

    let currency = setup_token(&env, &business, &verified_investor, &contract_addr);
    let invoice_id = create_verified_invoice(&env, &client, &business, &currency, 10_000);

    let result = client.try_place_bid(&unverified_investor, &invoice_id, &5_000, &5_700);
    assert!(result.is_err());
    let contract_err = result
        .err()
        .expect("expected contract error")
        .expect("expected contract-level error");
    assert_eq!(contract_err, QuickLendXError::BusinessNotVerified);
}

#[test]
fn test_place_bid_over_limit_fails() {
    let (env, client, admin, contract_addr) = setup_env();
    let business = setup_verified_business(&env, &client, &admin);
    let investor = setup_verified_investor(&env, &client, 1_000);
    let currency = setup_token(&env, &business, &investor, &contract_addr);
    let invoice_id = create_verified_invoice(&env, &client, &business, &currency, 10_000);

    let result = client.try_place_bid(&investor, &invoice_id, &1_500, &1_700);
    assert!(result.is_err());
    let contract_err = result
        .err()
        .expect("expected contract error")
        .expect("expected contract-level error");
    assert_eq!(contract_err, QuickLendXError::InvalidAmount);
}

#[test]
fn test_place_bid_wrong_invoice_status_fails() {
    let (env, client, admin, contract_addr) = setup_env();
    let business = setup_verified_business(&env, &client, &admin);
    let investor = setup_verified_investor(&env, &client, 25_000);
    let currency = setup_token(&env, &business, &investor, &contract_addr);

    let invoice_id = client.store_invoice(
        &business,
        &10_000,
        &currency,
        &(env.ledger().timestamp() + 86_400),
        &String::from_str(&env, "Pending status invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    let result = client.try_place_bid(&investor, &invoice_id, &5_000, &5_700);
    assert!(result.is_err());
    let contract_err = result
        .err()
        .expect("expected contract error")
        .expect("expected contract-level error");
    assert_eq!(contract_err, QuickLendXError::InvalidStatus);
}
