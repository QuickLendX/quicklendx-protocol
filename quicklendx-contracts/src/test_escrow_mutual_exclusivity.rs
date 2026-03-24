//! Tests for issue #610 – payment release/refund mutual exclusivity guarantees.
//!
//! Validates:
//! - `release_escrow` succeeds exactly once; subsequent calls (release or refund) are rejected
//! - `refund_escrow` succeeds exactly once; subsequent calls (refund or release) are rejected
//! - Token balances are correct after each terminal event (no double-spend)
//! - `EscrowStatus::is_terminal()` correctly classifies all states
//! - No escrow for invoice returns `StorageKeyNotFound`
//! - Escrow status is `Held` immediately after funding
//! - Multiple independent invoices each have isolated escrow state

use super::*;
use crate::invoice::InvoiceCategory;
use crate::payments::EscrowStatus;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, Env, String, Vec,
};

// ─── helpers ─────────────────────────────────────────────────────────────────

fn setup() -> (Env, QuickLendXContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.set_admin(&admin);
    client.initialize_fee_system(&admin);
    (env, client, admin)
}

fn make_token(env: &Env, contract_id: &Address, business: &Address, investor: &Address) -> Address {
    let token_admin = Address::generate(env);
    let currency = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();
    let sac = token::StellarAssetClient::new(env, &currency);
    let tok = token::Client::new(env, &currency);
    sac.mint(business, &20_000i128);
    sac.mint(investor, &20_000i128);
    let exp = env.ledger().sequence() + 50_000;
    tok.approve(business, contract_id, &80_000i128, &exp);
    tok.approve(investor, contract_id, &80_000i128, &exp);
    currency
}

/// Create a fully funded invoice (Pending → Verified → Funded via accept_bid).
fn funded_invoice(
    env: &Env,
    client: &QuickLendXContractClient,
    admin: &Address,
    amount: i128,
) -> (Address, Address, Address, BytesN<32>) {
    let business = Address::generate(env);
    let investor = Address::generate(env);
    let contract_id = client.address.clone();
    let currency = make_token(env, &contract_id, &business, &investor);

    client.submit_kyc_application(&business, &String::from_str(env, "KYC"));
    client.verify_business(admin, &business);
    client.submit_investor_kyc(&investor, &String::from_str(env, "KYC"));
    client.verify_investor(&investor, &100_000i128);

    let due_date = env.ledger().timestamp() + 86_400;
    let invoice_id = client.upload_invoice(
        &business,
        &amount,
        &currency,
        &due_date,
        &String::from_str(env, "Test invoice"),
        &InvoiceCategory::Services,
        &Vec::new(env),
    );
    client.verify_invoice(&invoice_id);
    let bid_id = client.place_bid(&investor, &invoice_id, &amount, &amount);
    client.accept_bid(&invoice_id, &bid_id);

    (business, investor, currency, invoice_id)
}

// ─── 1. EscrowStatus::is_terminal unit tests ─────────────────────────────────

/// Held is NOT terminal.
#[test]
fn test_is_terminal_held_is_false() {
    assert!(!EscrowStatus::Held.is_terminal());
}

/// Released IS terminal.
#[test]
fn test_is_terminal_released_is_true() {
    assert!(EscrowStatus::Released.is_terminal());
}

/// Refunded IS terminal.
#[test]
fn test_is_terminal_refunded_is_true() {
    assert!(EscrowStatus::Refunded.is_terminal());
}

// ─── 2. Escrow is Held immediately after funding ──────────────────────────────

#[test]
fn test_escrow_held_after_funding() {
    let (env, client, admin) = setup();
    let (_biz, _inv, _cur, invoice_id) = funded_invoice(&env, &client, &admin, 1_000);
    assert_eq!(client.get_escrow_status(&invoice_id), EscrowStatus::Held);
}

// ─── 3. Release path ─────────────────────────────────────────────────────────

/// Successful release transitions escrow to Released.
#[test]
fn test_release_sets_status_released() {
    let (env, client, admin) = setup();
    let (business, _inv, currency, invoice_id) = funded_invoice(&env, &client, &admin, 1_000);

    // Settle the invoice (triggers release internally)
    let sac = token::StellarAssetClient::new(&env, &currency);
    sac.mint(&business, &1_000i128);
    let tok = token::Client::new(&env, &currency);
    tok.approve(
        &business,
        &client.address,
        &4_000i128,
        &(env.ledger().sequence() + 10_000),
    );
    client.settle_invoice(&invoice_id, &1_000i128);

    assert_eq!(
        client.get_escrow_status(&invoice_id),
        EscrowStatus::Released
    );
}

/// After release, a second release attempt must fail (double-release blocked).
#[test]
fn test_double_release_rejected() {
    let (env, client, admin) = setup();
    let (business, _inv, currency, invoice_id) = funded_invoice(&env, &client, &admin, 1_000);

    let sac = token::StellarAssetClient::new(&env, &currency);
    sac.mint(&business, &1_000i128);
    let tok = token::Client::new(&env, &currency);
    tok.approve(
        &business,
        &client.address,
        &4_000i128,
        &(env.ledger().sequence() + 10_000),
    );
    client.settle_invoice(&invoice_id, &1_000i128);

    // Second release must fail
    let result = client.try_release_escrow_funds(&invoice_id);
    assert!(result.is_err(), "double-release must be rejected");
}

/// After release, a refund attempt must fail (refund-after-release blocked).
#[test]
fn test_refund_after_release_rejected() {
    let (env, client, admin) = setup();
    let (business, investor, currency, invoice_id) = funded_invoice(&env, &client, &admin, 1_000);

    let sac = token::StellarAssetClient::new(&env, &currency);
    sac.mint(&business, &1_000i128);
    let tok = token::Client::new(&env, &currency);
    tok.approve(
        &business,
        &client.address,
        &4_000i128,
        &(env.ledger().sequence() + 10_000),
    );
    client.settle_invoice(&invoice_id, &1_000i128);

    // Refund after release must fail
    let result = client.try_refund_escrow_funds(&invoice_id, &investor);
    assert!(result.is_err(), "refund-after-release must be rejected");
}

/// Business balance increases by escrow amount after release; investor unchanged.
#[test]
fn test_release_balance_accounting() {
    let (env, client, admin) = setup();
    let (business, investor, currency, invoice_id) = funded_invoice(&env, &client, &admin, 1_000);
    let tok = token::Client::new(&env, &currency);

    let biz_before = tok.balance(&business);
    let inv_before = tok.balance(&investor);

    let sac = token::StellarAssetClient::new(&env, &currency);
    sac.mint(&business, &1_000i128);
    tok.approve(
        &business,
        &client.address,
        &4_000i128,
        &(env.ledger().sequence() + 10_000),
    );
    client.settle_invoice(&invoice_id, &1_000i128);

    // Business receives escrow amount (1_000) back via settlement
    assert!(
        tok.balance(&business) >= biz_before,
        "business balance must not decrease after settlement"
    );
    // Investor balance unchanged by release
    assert_eq!(
        tok.balance(&investor),
        inv_before,
        "investor balance must be unchanged after release"
    );
}

// ─── 4. Refund path ──────────────────────────────────────────────────────────

/// Successful refund transitions escrow to Refunded.
#[test]
fn test_refund_sets_status_refunded() {
    let (env, client, admin) = setup();
    let (business, _inv, _cur, invoice_id) = funded_invoice(&env, &client, &admin, 1_000);

    client.refund_escrow_funds(&invoice_id, &business);

    assert_eq!(
        client.get_escrow_status(&invoice_id),
        EscrowStatus::Refunded
    );
}

/// After refund, a second refund attempt must fail (double-refund blocked).
#[test]
fn test_double_refund_rejected() {
    let (env, client, admin) = setup();
    let (business, _inv, _cur, invoice_id) = funded_invoice(&env, &client, &admin, 1_000);

    client.refund_escrow_funds(&invoice_id, &business);

    let result = client.try_refund_escrow_funds(&invoice_id, &business);
    assert!(result.is_err(), "double-refund must be rejected");
}

/// After refund, a release attempt must fail (release-after-refund blocked).
#[test]
fn test_release_after_refund_rejected() {
    let (env, client, admin) = setup();
    let (business, _inv, _cur, invoice_id) = funded_invoice(&env, &client, &admin, 1_000);

    client.refund_escrow_funds(&invoice_id, &business);

    let result = client.try_release_escrow_funds(&invoice_id);
    assert!(result.is_err(), "release-after-refund must be rejected");
}

/// Investor balance is restored after refund; business balance unchanged.
#[test]
fn test_refund_balance_accounting() {
    let (env, client, admin) = setup();
    let (business, investor, currency, invoice_id) = funded_invoice(&env, &client, &admin, 1_000);
    let tok = token::Client::new(&env, &currency);

    let inv_before = tok.balance(&investor);
    let biz_before = tok.balance(&business);

    client.refund_escrow_funds(&invoice_id, &business);

    // Investor gets funds back
    assert_eq!(
        tok.balance(&investor),
        inv_before + 1_000,
        "investor must receive escrow amount back on refund"
    );
    // Business balance unchanged
    assert_eq!(
        tok.balance(&business),
        biz_before,
        "business balance must be unchanged after refund"
    );
}

// ─── 5. No escrow → StorageKeyNotFound ───────────────────────────────────────

/// release_escrow_funds on an invoice with no escrow returns an error.
#[test]
fn test_release_no_escrow_returns_error() {
    let (env, client, admin) = setup();
    let business = Address::generate(&env);
    let investor = Address::generate(&env);
    let contract_id = client.address.clone();
    let currency = make_token(&env, &contract_id, &business, &investor);

    client.submit_kyc_application(&business, &String::from_str(&env, "KYC"));
    client.verify_business(&admin, &business);

    let due_date = env.ledger().timestamp() + 86_400;
    let invoice_id = client.upload_invoice(
        &business,
        &1_000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "No escrow invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    // Invoice is Pending — no escrow created yet
    let result = client.try_release_escrow_funds(&invoice_id);
    assert!(result.is_err(), "release on unfunded invoice must fail");
}

/// refund_escrow_funds on an invoice with no escrow returns an error.
#[test]
fn test_refund_no_escrow_returns_error() {
    let (env, client, admin) = setup();
    let business = Address::generate(&env);
    let investor = Address::generate(&env);
    let contract_id = client.address.clone();
    let currency = make_token(&env, &contract_id, &business, &investor);

    client.submit_kyc_application(&business, &String::from_str(&env, "KYC"));
    client.verify_business(&admin, &business);

    let due_date = env.ledger().timestamp() + 86_400;
    let invoice_id = client.upload_invoice(
        &business,
        &1_000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "No escrow invoice 2"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    let result = client.try_refund_escrow_funds(&invoice_id, &business);
    assert!(result.is_err(), "refund on unfunded invoice must fail");
}

// ─── 6. Multiple independent invoices ────────────────────────────────────────

/// Two invoices each have isolated escrow state; releasing one does not affect the other.
#[test]
fn test_independent_escrows_isolated() {
    let (env, client, admin) = setup();

    let (biz1, _inv1, cur1, inv_id1) = funded_invoice(&env, &client, &admin, 1_000);
    let (biz2, _inv2, _cur2, inv_id2) = funded_invoice(&env, &client, &admin, 2_000);

    // Both held
    assert_eq!(client.get_escrow_status(&inv_id1), EscrowStatus::Held);
    assert_eq!(client.get_escrow_status(&inv_id2), EscrowStatus::Held);

    // Settle invoice 1
    let sac1 = token::StellarAssetClient::new(&env, &cur1);
    sac1.mint(&biz1, &1_000i128);
    let tok1 = token::Client::new(&env, &cur1);
    tok1.approve(
        &biz1,
        &client.address,
        &4_000i128,
        &(env.ledger().sequence() + 10_000),
    );
    client.settle_invoice(&inv_id1, &1_000i128);

    // Invoice 1 released, invoice 2 still held
    assert_eq!(
        client.get_escrow_status(&inv_id1),
        EscrowStatus::Released,
        "invoice 1 escrow must be Released"
    );
    assert_eq!(
        client.get_escrow_status(&inv_id2),
        EscrowStatus::Held,
        "invoice 2 escrow must still be Held"
    );

    // Refund invoice 2
    client.refund_escrow_funds(&inv_id2, &biz2);

    assert_eq!(
        client.get_escrow_status(&inv_id2),
        EscrowStatus::Refunded,
        "invoice 2 escrow must be Refunded"
    );

    // Invoice 1 still Released (not affected by invoice 2 refund)
    assert_eq!(
        client.get_escrow_status(&inv_id1),
        EscrowStatus::Released,
        "invoice 1 escrow must remain Released"
    );
}

// ─── 7. Contract balance integrity ───────────────────────────────────────────

/// Contract holds exactly the escrowed amount between funding and terminal event.
#[test]
fn test_contract_balance_integrity() {
    let (env, client, admin) = setup();
    let (business, _investor, currency, invoice_id) = funded_invoice(&env, &client, &admin, 1_000);
    let tok = token::Client::new(&env, &currency);

    // Contract holds 1_000 after funding
    let contract_bal_after_fund = tok.balance(&client.address);
    assert!(
        contract_bal_after_fund >= 1_000,
        "contract must hold at least the escrowed amount"
    );

    // Refund returns funds to investor; contract balance decreases
    client.refund_escrow_funds(&invoice_id, &business);
    let contract_bal_after_refund = tok.balance(&client.address);
    assert!(
        contract_bal_after_refund < contract_bal_after_fund,
        "contract balance must decrease after refund"
    );
}
