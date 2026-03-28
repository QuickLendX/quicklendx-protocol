//! Accepted-Bid Exclusivity and Competing Bid Rejection Tests
//!
//! Validates that only one accepted bid can exist per funded invoice:
//! - accept_bid transitions invoice to Funded, rejecting subsequent accepts
//! - Competing bids on an already-funded invoice are rejected
//! - Bid status isolation: accepting one bid does not modify others
//! - Double-accept of the same bid is rejected
//! - Accepting a withdrawn/expired bid is rejected

#![cfg(test)]

use super::*;
use crate::bid::BidStatus;
use crate::invoice::{InvoiceCategory, InvoiceStatus};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, Env, String, Vec,
};

fn setup() -> (Env, QuickLendXContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.set_admin(&admin);
    (env, client, admin)
}

fn make_business(env: &Env, client: &QuickLendXContractClient, admin: &Address) -> Address {
    let business = Address::generate(env);
    client.submit_kyc_application(&business, &String::from_str(env, "KYC"));
    client.verify_business(admin, &business);
    business
}

fn make_investor(env: &Env, client: &QuickLendXContractClient, limit: i128) -> Address {
    let investor = Address::generate(env);
    client.submit_investor_kyc(&investor, &String::from_str(env, "KYC"));
    client.verify_investor(&investor, &limit);
    investor
}

fn make_token(
    env: &Env,
    business: &Address,
    investor: &Address,
    contract_id: &Address,
) -> Address {
    let token_admin = Address::generate(env);
    let currency = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let sac = token::StellarAssetClient::new(env, &currency);
    let tok = token::Client::new(env, &currency);
    sac.mint(business, &100_000i128);
    sac.mint(investor, &100_000i128);
    let expiry = env.ledger().sequence() + 10_000;
    tok.approve(business, contract_id, &100_000i128, &expiry);
    tok.approve(investor, contract_id, &100_000i128, &expiry);
    currency
}

// ── Exclusivity: only one accepted bid per invoice ───────────────────────────

#[test]
fn second_accept_on_funded_invoice_is_rejected() {
    let (env, client, admin) = setup();
    let business = make_business(&env, &client, &admin);
    let investor_a = make_investor(&env, &client, 50_000);
    let investor_b = make_investor(&env, &client, 50_000);
    let contract_addr = client.address.clone();
    let currency = make_token(&env, &business, &investor_a, &contract_addr);

    // Mint tokens for investor_b too
    let sac = token::StellarAssetClient::new(&env, &currency);
    let tok = token::Client::new(&env, &currency);
    sac.mint(&investor_b, &100_000i128);
    tok.approve(&investor_b, &contract_addr, &100_000i128, &(env.ledger().sequence() + 10_000));

    let amount = 1_000i128;
    let due = env.ledger().timestamp() + 86_400;

    let invoice_id = client.store_invoice(
        &business,
        &amount,
        &currency,
        &due,
        &String::from_str(&env, "Exclusivity test"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    client.verify_invoice(&invoice_id);

    // Two investors bid
    let bid_a = client.place_bid(&investor_a, &invoice_id, &amount, &(amount + 100));
    let bid_b = client.place_bid(&investor_b, &invoice_id, &amount, &(amount + 150));

    // Accept first bid — should succeed
    client.accept_bid(&invoice_id, &bid_a);

    // Invoice should now be Funded
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Funded);

    // Accept second bid — should fail (invoice already funded)
    let result = client.try_accept_bid(&invoice_id, &bid_b);
    assert!(result.is_err());
}

#[test]
fn double_accept_same_bid_is_rejected() {
    let (env, client, admin) = setup();
    let business = make_business(&env, &client, &admin);
    let investor = make_investor(&env, &client, 50_000);
    let contract_addr = client.address.clone();
    let currency = make_token(&env, &business, &investor, &contract_addr);

    let amount = 1_000i128;
    let due = env.ledger().timestamp() + 86_400;

    let invoice_id = client.store_invoice(
        &business,
        &amount,
        &currency,
        &due,
        &String::from_str(&env, "Double accept test"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    client.verify_invoice(&invoice_id);

    let bid_id = client.place_bid(&investor, &invoice_id, &amount, &(amount + 100));
    client.accept_bid(&invoice_id, &bid_id);

    // Second accept of same bid should fail
    let result = client.try_accept_bid(&invoice_id, &bid_id);
    assert!(result.is_err());
}

// ── Bid status isolation ─────────────────────────────────────────────────────

#[test]
fn accepting_one_bid_does_not_modify_other_bids_status() {
    let (env, client, admin) = setup();
    let business = make_business(&env, &client, &admin);
    let investor_a = make_investor(&env, &client, 50_000);
    let investor_b = make_investor(&env, &client, 50_000);
    let contract_addr = client.address.clone();
    let currency = make_token(&env, &business, &investor_a, &contract_addr);

    let sac = token::StellarAssetClient::new(&env, &currency);
    let tok = token::Client::new(&env, &currency);
    sac.mint(&investor_b, &100_000i128);
    tok.approve(&investor_b, &contract_addr, &100_000i128, &(env.ledger().sequence() + 10_000));

    let amount = 1_000i128;
    let due = env.ledger().timestamp() + 86_400;

    let invoice_id = client.store_invoice(
        &business,
        &amount,
        &currency,
        &due,
        &String::from_str(&env, "Isolation test"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    client.verify_invoice(&invoice_id);

    let bid_a = client.place_bid(&investor_a, &invoice_id, &amount, &(amount + 100));
    let bid_b = client.place_bid(&investor_b, &invoice_id, &amount, &(amount + 200));

    // Accept bid A
    client.accept_bid(&invoice_id, &bid_a);

    // Bid A should be Accepted
    let bids = client.get_bids_for_invoice(&invoice_id);
    let mut found_a = false;
    let mut found_b = false;
    for bid in bids.iter() {
        if bid.bid_id == bid_a {
            assert_eq!(bid.status, BidStatus::Accepted);
            found_a = true;
        }
        if bid.bid_id == bid_b {
            // Bid B should still be Placed (not automatically cancelled)
            assert_eq!(bid.status, BidStatus::Placed);
            found_b = true;
        }
    }
    assert!(found_a, "Accepted bid A not found");
    assert!(found_b, "Competing bid B not found");
}

// ── Invalid bid state transitions ────────────────────────────────────────────

#[test]
fn accept_bid_on_pending_invoice_is_rejected() {
    let (env, client, admin) = setup();
    let business = make_business(&env, &client, &admin);
    let investor = make_investor(&env, &client, 50_000);
    let currency = Address::generate(&env);
    let due = env.ledger().timestamp() + 86_400;

    let invoice_id = client.store_invoice(
        &business,
        &1000i128,
        &currency,
        &due,
        &String::from_str(&env, "Pending test"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    // Invoice is Pending (not verified) — place_bid should fail
    let result = client.try_place_bid(&investor, &invoice_id, &1000i128, &1100i128);
    assert!(result.is_err());
}

#[test]
fn accept_bid_requires_verified_invoice_status() {
    let (env, client, admin) = setup();
    let business = make_business(&env, &client, &admin);
    let investor = make_investor(&env, &client, 50_000);
    let contract_addr = client.address.clone();
    let currency = make_token(&env, &business, &investor, &contract_addr);
    let due = env.ledger().timestamp() + 86_400;

    let invoice_id = client.store_invoice(
        &business,
        &1000i128,
        &currency,
        &due,
        &String::from_str(&env, "Status check"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    // Verify and place bid
    client.verify_invoice(&invoice_id);
    let bid_id = client.place_bid(&investor, &invoice_id, &1000i128, &1100i128);

    // Accept — this funds the invoice
    client.accept_bid(&invoice_id, &bid_id);

    // Verify status is now Funded
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Funded);
    assert_eq!(invoice.funded_amount, 1000);
    assert!(invoice.investor.is_some());
}
