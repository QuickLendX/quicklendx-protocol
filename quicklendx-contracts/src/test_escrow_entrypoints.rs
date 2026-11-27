//! Client-level tests for the escrow and completion entrypoints exposed on
//! `QuickLendXContract`: `release_escrow_funds`, `refund_escrow_funds`,
//! `approve_early_escrow_release`, `get_escrow_status`, `complete_invoice` and
//! `freeze_invoice`.
//!
//! These drive the deployed client surface (rather than the internal modules)
//! so that a regression that drops an entrypoint from `#[contractimpl]` fails
//! to compile here.

#![cfg(test)]

use super::*;
use crate::events::{TOPIC_ESCROW_REFUNDED, TOPIC_ESCROW_RELEASED, TOPIC_INVOICE_FROZEN};
use crate::invoice::{InvoiceCategory, InvoiceStatus};
use crate::payments::EscrowStatus;
use crate::types::BusinessFreezeReason;
use soroban_sdk::{
    testutils::{Address as _, Events as _, Ledger},
    token, xdr, Address, BytesN, Env, String, Symbol, TryFromVal, Vec,
};

const AMOUNT: i128 = 1_000;

/// Count events whose first topic matches `topic`. `env.events().all()` only
/// exposes the events emitted by the most recent contract invocation, so this
/// is a per-call count.
fn count_events(env: &Env, topic: Symbol) -> u32 {
    let topic_xdr = xdr::ScVal::try_from_val(env, &topic).expect("topic to xdr");
    env.events()
        .all()
        .events()
        .iter()
        .filter(|event| match &event.body {
            xdr::ContractEventBody::V0(body) => body.topics.first() == Some(&topic_xdr),
        })
        .count() as u32
}

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

fn make_token(env: &Env, business: &Address, investor: &Address, contract_id: &Address) -> Address {
    let token_admin = Address::generate(env);
    let currency = env
        .register_stellar_asset_contract_v2(token_admin)
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

/// Store and verify an invoice, then fund it with a single accepted bid.
fn funded_invoice(
    env: &Env,
    client: &QuickLendXContractClient,
    business: &Address,
    investor: &Address,
    currency: &Address,
) -> BytesN<32> {
    let due = env.ledger().timestamp() + 86_400;
    let invoice_id = client.store_invoice(
        business,
        &AMOUNT,
        currency,
        &due,
        &String::from_str(env, "Escrow entrypoint test"),
        &InvoiceCategory::Services,
        &Vec::new(env),
        &None,
    );
    client.verify_invoice(&invoice_id);
    let bid_id = client.place_bid(
        investor,
        &invoice_id,
        &AMOUNT,
        &(AMOUNT + 100),
        &BytesN::from_array(env, &[0u8; 32]),
    );
    client.accept_bid(&invoice_id, &bid_id);
    invoice_id
}

// ---------------------------------------------------------------------------
// get_escrow_status
// ---------------------------------------------------------------------------

#[test]
fn get_escrow_status_is_held_after_funding() {
    let (env, client, admin) = setup();
    let business = make_business(&env, &client, &admin);
    let investor = make_investor(&env, &client, 50_000);
    let contract_id = client.address.clone();
    let currency = make_token(&env, &business, &investor, &contract_id);

    let invoice_id = funded_invoice(&env, &client, &business, &investor, &currency);

    assert_eq!(client.get_escrow_status(&invoice_id), EscrowStatus::Held);
}

#[test]
fn get_escrow_status_without_escrow_returns_error() {
    let (env, client, admin) = setup();
    let business = make_business(&env, &client, &admin);
    let investor = make_investor(&env, &client, 50_000);
    let contract_id = client.address.clone();
    let currency = make_token(&env, &business, &investor, &contract_id);

    let due = env.ledger().timestamp() + 86_400;
    let invoice_id = client.store_invoice(
        &business,
        &AMOUNT,
        &currency,
        &due,
        &String::from_str(&env, "Never funded"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
        &None,
    );
    client.verify_invoice(&invoice_id);

    assert_eq!(
        client.try_get_escrow_status(&invoice_id).unwrap_err().unwrap(),
        QuickLendXError::StorageKeyNotFound
    );
}

// ---------------------------------------------------------------------------
// release_escrow_funds
// ---------------------------------------------------------------------------

#[test]
fn release_escrow_funds_pays_business_and_emits_once() {
    let (env, client, admin) = setup();
    let business = make_business(&env, &client, &admin);
    let investor = make_investor(&env, &client, 50_000);
    let contract_id = client.address.clone();
    let currency = make_token(&env, &business, &investor, &contract_id);
    let tok = token::Client::new(&env, &currency);

    let invoice_id = funded_invoice(&env, &client, &business, &investor, &currency);
    let balance_before = tok.balance(&business);

    client.release_escrow_funds(&invoice_id);

    // `env.events().all()` only exposes the most recent invocation's events, so
    // count before any further contract call.
    assert_eq!(count_events(&env, Symbol::new(&env, TOPIC_ESCROW_RELEASED)), 1);
    assert_eq!(tok.balance(&business), balance_before + AMOUNT);
    assert_eq!(client.get_escrow_status(&invoice_id), EscrowStatus::Released);
}

#[test]
fn release_escrow_funds_is_rejected_for_non_business_caller() {
    let (env, client, admin) = setup();
    let business = make_business(&env, &client, &admin);
    let investor = make_investor(&env, &client, 50_000);
    let contract_id = client.address.clone();
    let currency = make_token(&env, &business, &investor, &contract_id);

    let invoice_id = funded_invoice(&env, &client, &business, &investor, &currency);
    let stranger = Address::generate(&env);

    // Only the escrow's business may authorize a release.
    assert_eq!(
        client
            .try_release_escrow(&invoice_id, &stranger)
            .unwrap_err()
            .unwrap(),
        QuickLendXError::Unauthorized
    );
    assert_eq!(client.get_escrow_status(&invoice_id), EscrowStatus::Held);
}

#[test]
fn release_escrow_funds_requires_funded_invoice() {
    let (env, client, admin) = setup();
    let business = make_business(&env, &client, &admin);
    let investor = make_investor(&env, &client, 50_000);
    let contract_id = client.address.clone();
    let currency = make_token(&env, &business, &investor, &contract_id);

    let due = env.ledger().timestamp() + 86_400;
    let invoice_id = client.store_invoice(
        &business,
        &AMOUNT,
        &currency,
        &due,
        &String::from_str(&env, "Verified only"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
        &None,
    );
    client.verify_invoice(&invoice_id);

    assert_eq!(
        client.try_release_escrow_funds(&invoice_id).unwrap_err().unwrap(),
        QuickLendXError::InvalidStatus
    );
}

#[test]
fn double_release_is_rejected() {
    let (env, client, admin) = setup();
    let business = make_business(&env, &client, &admin);
    let investor = make_investor(&env, &client, 50_000);
    let contract_id = client.address.clone();
    let currency = make_token(&env, &business, &investor, &contract_id);

    let invoice_id = funded_invoice(&env, &client, &business, &investor, &currency);

    client.release_escrow_funds(&invoice_id);
    assert_eq!(client.get_escrow_status(&invoice_id), EscrowStatus::Released);

    // Held -> Released is the only legal release transition; a second release
    // must be rejected by the escrow state machine.
    assert_eq!(
        client.try_release_escrow_funds(&invoice_id).unwrap_err().unwrap(),
        QuickLendXError::InvalidStatus
    );
}

// ---------------------------------------------------------------------------
// refund_escrow_funds
// ---------------------------------------------------------------------------

#[test]
fn refund_escrow_funds_after_expiry_succeeds_and_emits_once() {
    let (env, client, admin) = setup();
    let business = make_business(&env, &client, &admin);
    let investor = make_investor(&env, &client, 50_000);
    let contract_id = client.address.clone();
    let currency = make_token(&env, &business, &investor, &contract_id);
    let tok = token::Client::new(&env, &currency);

    let invoice_id = funded_invoice(&env, &client, &business, &investor, &currency);
    let investor_balance_after_funding = tok.balance(&investor);

    // Advance past the invoice due date.
    env.ledger()
        .set_timestamp(env.ledger().timestamp() + 86_401);

    client.refund_escrow_funds(&invoice_id, &business);

    // `env.events().all()` only exposes the most recent invocation's events, so
    // count before any further contract call.
    assert_eq!(count_events(&env, Symbol::new(&env, TOPIC_ESCROW_REFUNDED)), 1);
    assert_eq!(tok.balance(&investor), investor_balance_after_funding + AMOUNT);
    assert_eq!(client.get_escrow_status(&invoice_id), EscrowStatus::Refunded);
    assert_eq!(
        client.get_invoice(&invoice_id).status,
        InvoiceStatus::Refunded
    );
}

#[test]
fn refund_escrow_funds_is_rejected_for_unauthorized_caller() {
    let (env, client, admin) = setup();
    let business = make_business(&env, &client, &admin);
    let investor = make_investor(&env, &client, 50_000);
    let contract_id = client.address.clone();
    let currency = make_token(&env, &business, &investor, &contract_id);

    let invoice_id = funded_invoice(&env, &client, &business, &investor, &currency);
    let stranger = Address::generate(&env);

    assert_eq!(
        client
            .try_refund_escrow_funds(&invoice_id, &stranger)
            .unwrap_err()
            .unwrap(),
        QuickLendXError::Unauthorized
    );
    assert_eq!(client.get_escrow_status(&invoice_id), EscrowStatus::Held);
}

#[test]
fn double_refund_is_rejected() {
    let (env, client, admin) = setup();
    let business = make_business(&env, &client, &admin);
    let investor = make_investor(&env, &client, 50_000);
    let contract_id = client.address.clone();
    let currency = make_token(&env, &business, &investor, &contract_id);

    let invoice_id = funded_invoice(&env, &client, &business, &investor, &currency);

    client.refund_escrow_funds(&invoice_id, &business);

    assert_eq!(
        client
            .try_refund_escrow_funds(&invoice_id, &business)
            .unwrap_err()
            .unwrap(),
        QuickLendXError::InvalidStatus
    );
}

// ---------------------------------------------------------------------------
// approve_early_escrow_release
// ---------------------------------------------------------------------------

#[test]
fn approve_early_escrow_release_is_callable() {
    let (env, client, admin) = setup();
    let business = make_business(&env, &client, &admin);
    let investor = make_investor(&env, &client, 50_000);
    let contract_id = client.address.clone();
    let currency = make_token(&env, &business, &investor, &contract_id);

    let invoice_id = funded_invoice(&env, &client, &business, &investor, &currency);

    client.approve_early_escrow_release(&invoice_id, &business);
    client.approve_early_escrow_release(&invoice_id, &investor);
    // Approval bookkeeping is not wired into the release path yet, so the
    // escrow remains held.
    assert_eq!(client.get_escrow_status(&invoice_id), EscrowStatus::Held);
}

// ---------------------------------------------------------------------------
// complete_invoice / freeze_invoice
// ---------------------------------------------------------------------------

#[test]
fn complete_invoice_marks_invoice_paid_and_is_idempotent() {
    let (env, client, admin) = setup();
    let business = make_business(&env, &client, &admin);
    let investor = make_investor(&env, &client, 50_000);
    let contract_id = client.address.clone();
    let currency = make_token(&env, &business, &investor, &contract_id);

    let due = env.ledger().timestamp() + 86_400;
    let invoice_id = client.store_invoice(
        &business,
        &AMOUNT,
        &currency,
        &due,
        &String::from_str(&env, "Completable"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
        &None,
    );
    client.verify_invoice(&invoice_id);

    let nonce = BytesN::from_array(&env, &[7u8; 32]);
    client.complete_invoice(&business, &invoice_id, &nonce);
    assert_eq!(client.get_invoice(&invoice_id).status, InvoiceStatus::Paid);

    // Replaying the same nonce is a no-op rather than an error.
    client.complete_invoice(&business, &invoice_id, &nonce);
    assert_eq!(client.get_invoice(&invoice_id).status, InvoiceStatus::Paid);
}

#[test]
fn freeze_invoice_is_admin_only_and_emits_once() {
    let (env, client, admin) = setup();
    let business = make_business(&env, &client, &admin);

    let invoice_id = BytesN::from_array(&env, &[9u8; 32]);
    client.freeze_invoice(&admin, &invoice_id, &BusinessFreezeReason::AdminAction);

    assert_eq!(count_events(&env, Symbol::new(&env, TOPIC_INVOICE_FROZEN)), 1);

    let stranger = Address::generate(&env);
    assert!(client
        .try_freeze_invoice(&stranger, &invoice_id, &BusinessFreezeReason::AdminAction)
        .is_err());
}
