use super::*;
use crate::errors::QuickLendXError;
use soroban_sdk::{
    testutils::{Address as _, MockAuth, MockAuthInvoke},
    Address, BytesN, Env, IntoVal, String, Vec,
};

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

#[test]
fn test_withdraw_bid_owner_succeeds() {
    let (env, client, admin, contract_addr) = setup_env();
    let business = setup_verified_business(&env, &client, &admin);
    let investor = setup_verified_investor(&env, &client, 25_000);
    let currency = setup_token(&env, &business, &investor, &contract_addr);
    let invoice_id = create_verified_invoice(&env, &client, &business, &currency, 10_000);

    let bid_id = client.place_bid(&investor, &invoice_id, &5_000, &5_700);
    let result = client.try_withdraw_bid(&bid_id);

    assert!(result.is_ok());
    assert_eq!(
        client.get_bid(&bid_id).unwrap().status,
        BidStatus::Withdrawn
    );
}

#[test]
fn test_withdraw_bid_non_owner_fails_auth() {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    client.mock_all_auths().set_admin(&admin);

    let business = Address::generate(&env);
    client
        .mock_all_auths()
        .submit_kyc_application(&business, &String::from_str(&env, "Business KYC"));
    client.mock_all_auths().verify_business(&admin, &business);

    let investor = Address::generate(&env);
    client
        .mock_all_auths()
        .submit_investor_kyc(&investor, &String::from_str(&env, "Investor KYC"));
    client.mock_all_auths().verify_investor(&investor, &20_000);

    let contract_addr = client.address.clone();
    let currency = setup_token(&env, &business, &investor, &contract_addr);
    let invoice_id = client.mock_all_auths().store_invoice(
        &business,
        &10_000,
        &currency,
        &(env.ledger().timestamp() + 86_400),
        &String::from_str(&env, "Auth test invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    client.mock_all_auths().verify_invoice(&invoice_id);

    let place_auth = MockAuth {
        address: &investor,
        invoke: &MockAuthInvoke {
            contract: &contract_id,
            fn_name: "place_bid",
            args: (investor.clone(), invoice_id.clone(), 5_000i128, 5_700i128).into_val(&env),
            sub_invokes: &[],
        },
    };
    let bid_id = client
        .mock_auths(&[place_auth])
        .place_bid(&investor, &invoice_id, &5_000, &5_700);

    let attacker = Address::generate(&env);
    let withdraw_auth = MockAuth {
        address: &attacker,
        invoke: &MockAuthInvoke {
            contract: &contract_id,
            fn_name: "withdraw_bid",
            args: (bid_id.clone(),).into_val(&env),
            sub_invokes: &[],
        },
    };

    let result = client
        .mock_auths(&[withdraw_auth])
        .try_withdraw_bid(&bid_id);
    assert!(result.is_err());

    let auth_err = result.err().expect("expected auth failure");
    assert!(
        auth_err.err().is_some(),
        "expected invoke abort for bad auth"
    );
    assert_eq!(client.get_bid(&bid_id).unwrap().status, BidStatus::Placed);
}

#[test]
fn test_withdraw_bid_already_accepted_fails() {
    let (env, client, admin, contract_addr) = setup_env();
    let business = setup_verified_business(&env, &client, &admin);
    let investor = setup_verified_investor(&env, &client, 25_000);
    let currency = setup_token(&env, &business, &investor, &contract_addr);
    let invoice_id = create_verified_invoice(&env, &client, &business, &currency, 10_000);

    let bid_id = client.place_bid(&investor, &invoice_id, &5_000, &5_700);
    client.accept_bid(&invoice_id, &bid_id);

    let result = client.try_withdraw_bid(&bid_id);
    assert!(result.is_err());
    let contract_err = result
        .err()
        .expect("expected contract error")
        .expect("expected contract-level error");
    assert_eq!(contract_err, QuickLendXError::OperationNotAllowed);
}

#[test]
fn test_withdraw_bid_already_withdrawn_fails() {
    let (env, client, admin, contract_addr) = setup_env();
    let business = setup_verified_business(&env, &client, &admin);
    let investor = setup_verified_investor(&env, &client, 25_000);
    let currency = setup_token(&env, &business, &investor, &contract_addr);
    let invoice_id = create_verified_invoice(&env, &client, &business, &currency, 10_000);

    let bid_id = client.place_bid(&investor, &invoice_id, &5_000, &5_700);
    client.withdraw_bid(&bid_id);

    let result = client.try_withdraw_bid(&bid_id);
    assert!(result.is_err());
    let contract_err = result
        .err()
        .expect("expected contract error")
        .expect("expected contract-level error");
    assert_eq!(contract_err, QuickLendXError::OperationNotAllowed);
}
