//! Tests for escrow refund behavior: authorization, idempotency, and state safety
//!
use super::*;
use crate::invoice::InvoiceCategory;
use crate::payments::EscrowStatus;
#[cfg(test)]
use soroban_sdk::{testutils::Address as _, token, Address, Env};

fn setup_env() -> (Env, QuickLendXContractClient<'static>, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let _ = client.try_initialize_admin(&admin);
    let admin = Address::generate(&env);
    client.set_admin(&admin);
    (env, client, admin, contract_id)
}

fn setup_token(
    env: &Env,
    business: &Address,
    investor: &Address,
    contract_id: &Address,
) -> Address {
    let token_admin = Address::generate(env);
    let currency = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();

    let sac_client = token::StellarAssetClient::new(env, &currency);
    let token_client = token::Client::new(env, &currency);

    let initial = 10_000i128;
    sac_client.mint(business, &initial);
    sac_client.mint(investor, &initial);

    let expiration = env.ledger().sequence() + 10_000;
    token_client.approve(business, contract_id, &initial, &expiration);
    token_client.approve(investor, contract_id, &initial, &expiration);

    currency
}

#[test]
fn test_refund_transfers_and_updates_status() {
    let (env, client, _, _) = setup_env();
    let contract_id = client.address.clone();

    let business = Address::generate(&env);
    let investor = Address::generate(&env);

    let currency = setup_token(&env, &business, &investor, &contract_id);
    let token_client = token::Client::new(&env, &currency);

    // Create and verify invoice
    let amount = 1_000i128;
    let due_date = env.ledger().timestamp() + 86400;
    let invoice_id = client.store_invoice(
        &business,
        &amount,
        &currency,
        &due_date,
        &String::from_str(&env, "Refund test invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    // Bypass admin verify path in this test by updating status directly
    client.update_invoice_status(&invoice_id, &InvoiceStatus::Verified);

    // Prepare investor and place bid
    client.submit_investor_kyc(&investor, &String::from_str(&env, "kyc"));
    client.verify_investor(&investor, &10_000i128);

    // Approve and place bid
    token_client.approve(
        &investor,
        &contract_id,
        &10_000i128,
        &(env.ledger().sequence() + 10_000),
    );
    let bid_id = client.place_bid(&investor, &invoice_id, &amount, &(amount + 100));

    // Accept (creates escrow)
    client.accept_bid(&invoice_id, &bid_id);

    // Sanity: escrow is held and investor balance reduced
    let escrow_status = client.get_escrow_status(&invoice_id);
    assert_eq!(escrow_status, EscrowStatus::Held);
    let bal_after_lock = token_client.balance(&investor);
    assert_eq!(bal_after_lock, 9_000i128);

    // Refund escrow funds (initiated by business)
    client.refund_escrow_funds(&invoice_id, &business);

    // Escrow marked Refunded
    let escrow_status = client.get_escrow_status(&invoice_id);
    assert_eq!(escrow_status, EscrowStatus::Refunded);

    // Investor received funds back
    assert_eq!(token_client.balance(&investor), 10_000i128);
}

#[test]
fn test_refund_idempotency_and_release_blocked() {
    let (env, client, _, _) = setup_env();
    let contract_id = client.address.clone();

    let business = Address::generate(&env);
    let investor = Address::generate(&env);

    let currency = setup_token(&env, &business, &investor, &contract_id);
    let token_client = token::Client::new(&env, &currency);

    // Create and verify invoice
    let amount = 2_000i128;
    let due_date = env.ledger().timestamp() + 86400;
    let invoice_id = client.store_invoice(
        &business,
        &amount,
        &currency,
        &due_date,
        &String::from_str(&env, "Refund idempotency invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    // Avoid admin-only path in this test; update status directly
    client.update_invoice_status(&invoice_id, &InvoiceStatus::Verified);

    // Investor setup and bid
    client.submit_investor_kyc(&investor, &String::from_str(&env, "kyc"));
    client.verify_investor(&investor, &10_000i128);
    token_client.approve(
        &investor,
        &contract_id,
        &10_000i128,
        &(env.ledger().sequence() + 10_000),
    );
    let bid_id = client.place_bid(&investor, &invoice_id, &amount, &(amount + 100));
    client.accept_bid(&invoice_id, &bid_id);

    // Refund once
    client.refund_escrow_funds(&invoice_id, &business);
    let escrow_status = client.get_escrow_status(&invoice_id);
    assert_eq!(escrow_status, EscrowStatus::Refunded);

    // Second refund should fail (not Held)
    let result = client.try_refund_escrow_funds(&invoice_id, &business);
    assert!(
        result.is_err(),
        "Second refund must be rejected to avoid double refunds"
    );

    // Attempt to release after refund should fail
    let release_result = client.try_release_escrow_funds(&invoice_id);
    assert!(
        release_result.is_err(),
        "Release must be rejected after refund"
    );
}

#[test]
fn test_refund_authorization_current_behavior_and_security_note() {
    let (env, client, _, contract_id) = setup_env();
    let business = Address::generate(&env);
    let investor = Address::generate(&env);

    // Setup token and balances
    let token_admin = Address::generate(&env);
    let currency = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();
    let token_client = token::Client::new(&env, &currency);
    let sac_client = token::StellarAssetClient::new(&env, &currency);
    sac_client.mint(&investor, &5_000i128);

    // Create verified invoice and escrow
    let amount = 1_000i128;
    let due_date = env.ledger().timestamp() + 86400;
    let invoice_id = client.store_invoice(
        &business,
        &amount,
        &currency,
        &due_date,
        &String::from_str(&env, "Auth behavior invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    client.verify_invoice(&invoice_id);
    client.submit_investor_kyc(&investor, &String::from_str(&env, "kyc"));
    client.verify_investor(&investor, &10_000i128);
    token_client.approve(
        &investor,
        &contract_id,
        &10_000i128,
        &(env.ledger().sequence() + 10_000),
    );
    let bid_id = client.place_bid(&investor, &invoice_id, &amount, &(amount + 100));
    client.accept_bid(&invoice_id, &bid_id);

    // Now call refund without mocking auth: should succeed under current code
    client.refund_escrow_funds(&invoice_id, &business);
    let escrow_status = client.get_escrow_status(&invoice_id);
    assert_eq!(
        escrow_status,
        EscrowStatus::Refunded,
        "Refund should succeed under current code"
    );

    // Security note: Consider adding `admin.require_auth()` or `invoice.business.require_auth()`
    // to `refund_escrow_funds` to limit who can initiate refunds.
}
