// Tests for replay guard (nonce handling) in settlement.

use super::*;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, BytesN, Env, String, Vec};

/// Helper to initialize a token for testing.
fn init_currency_for_test(
    env: &Env,
    contract_id: &Address,
    business: &Address,
    investor: &Address,
) -> Address {
    let token_admin = Address::generate(env);
    let currency = env.register_stellar_asset_contract_v2(token_admin.clone()).address();
    let token_client = token::Client::new(env, &currency);
    let sac_client = token::StellarAssetClient::new(env, &currency);
    let initial_balance = 10_000i128;
    sac_client.mint(business, &initial_balance);
    sac_client.mint(investor, &initial_balance);
    sac_client.mint(contract_id, &1i128);
    let expiration = env.ledger().sequence() + 1_000;
    token_client.approve(business, contract_id, &initial_balance, &expiration);
    token_client.approve(investor, contract_id, &initial_balance, &expiration);
    currency
}

/// Helper to set up a funded invoice.
fn setup_funded_invoice(
    env: &Env,
    client: &QuickLendXContractClient,
    business: &Address,
    investor: &Address,
    currency: &Address,
    invoice_amount: i128,
    investment_amount: i128,
) -> BytesN<32> {
    let admin = Address::generate(env);
    client.set_admin(&admin);
    client.submit_kyc_application(business, &String::from_str(env, "KYC data"));
    client.verify_business(&admin, business);
    let due_date = env.ledger().timestamp() + 86_400;
    let invoice_id = client.store_invoice(
        business,
        &invoice_amount,
        currency,
        &due_date,
        &String::from_str(env, "Test invoice"),
        &InvoiceCategory::Services,
        &Vec::new(env),
        &None);
    client.verify_invoice(&invoice_id);
    // Investor KYC and investment.
    client.submit_investor_kyc(investor, &String::from_str(env, "Investor KYC"));
    client.verify_investor(investor, &10_000i128);
    let bid_id = client.place_bid(investor, &invoice_id, &investment_amount, &invoice_amount, &BytesN::from_array(&env, &[0u8; 32]));
    client.accept_bid(&invoice_id, &bid_id);
    invoice_id
}

#[test]
fn test_replay_guard_fresh_and_duplicate_nonce() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let business = Address::generate(&env);
    let investor = Address::generate(&env);
    let currency = init_currency_for_test(&env, &contract_id, &business, &investor);
    let invoice_id = setup_funded_invoice(&env, &client, &business, &investor, &currency, 1_000, 900);

    // First partial payment with a fresh nonce succeeds.
    let nonce = String::from_str(&env, "nonce-1");
    client.process_partial_payment(&invoice_id, 400, &nonce);
    let count_after_first = get_payment_count(&env, &invoice_id).unwrap();
    assert_eq!(count_after_first, 1);

    // Duplicate nonce must be rejected.
    let result = client.try_process_partial_payment(&invoice_id, 400, &nonce);
    assert!(result.is_err(), "Duplicate nonce must be rejected");
    let err = result.unwrap_err();
    assert_eq!(
        err,
        Ok(QuickLendXError::DuplicateNonce),
        "Expected DuplicateNonce error"
    );

    // Count must remain unchanged.
    let count_after_dup = get_payment_count(&env, &invoice_id).unwrap();
    assert_eq!(count_after_dup, 1, "Duplicate nonce must not increase count");
}

#[test]
fn test_replay_guard_cross_domain_nonce() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let business1 = Address::generate(&env);
    let investor1 = Address::generate(&env);
    let currency1 = init_currency_for_test(&env, &contract_id, &business1, &investor1);
    let invoice_id1 = setup_funded_invoice(&env, &client, &business1, &investor1, &currency1, 1_000, 900);

    let business2 = Address::generate(&env);
    let investor2 = Address::generate(&env);
    let currency2 = init_currency_for_test(&env, &contract_id, &business2, &investor2);
    let invoice_id2 = setup_funded_invoice(&env, &client, &business2, &investor2, &currency2, 1_500, 1_200);

    let shared_nonce = String::from_str(&env, "shared-nonce");
    client.process_partial_payment(&invoice_id1, 300, &shared_nonce);
    client.process_partial_payment(&invoice_id2, 400, &shared_nonce);

    let count1 = get_payment_count(&env, &invoice_id1).unwrap();
    let count2 = get_payment_count(&env, &invoice_id2).unwrap();
    assert_eq!(count1, 1, "First invoice should have one payment record");
    assert_eq!(count2, 1, "Second invoice should also have one payment record with same nonce string");
}
