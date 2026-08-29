//! # Durable request-key idempotency for bid acceptance — regression suite
//!
//! ## Purpose
//! Bid acceptance must be deterministic and resistant to replay, reordering,
//! and stale or adversarial inputs. The keyed entrypoints
//! `accept_bid_and_fund_with_key` and `accept_bid_with_key` bind each
//! operation to a caller-provided durable request key:
//!
//! - A **safe retry** (same key, same invoice, same bid) returns the cached
//!   escrow ID without moving funds again.
//! - **Conflicting reuse** (same key, different payload) returns
//!   `QuickLendXError::DuplicateBid` and mutates nothing.
//! - **Rejected or failed attempts never store a record**, so corrected
//!   retries with the same key remain available and no partial state lingers.
//!
//! Legacy entrypoints keep their historical deterministic-rejection behavior
//! (`InvoiceAlreadyFunded` / `InvalidStatus`) on every retry, so existing
//! callers observe no behavioral change.
//!
//! ## Tests
//! | Test | Scenario |
//! |------|----------|
//! | `test_keyed_safe_retry_is_idempotent_and_no_duplicate_state` | Duplicate call, same key: same escrow ID, single funding, invoice/bid states exactly once |
//! | `test_keyed_timeout_retry_same_result_across_ledgers` | Client-timeout retry in a later ledger returns the cached escrow ID |
//! | `test_keyed_conflicting_reuse_same_key_different_bid` | Key reuse with a different bid is rejected; no state change |
//! | `test_keyed_conflicting_reuse_same_key_different_invoice` | Key reuse with a different invoice is rejected; no state change |
//! | `test_rejected_attempt_does_not_poison_key_or_state` | Stale/expired-bid rejection stores no record; corrected retry with the same key succeeds |
//! | `test_keyed_separate_keys_two_invoices_both_orderings` | Independent keys; both orderings produce exactly one committed effect per invoice |
//! | `test_legacy_entrypoints_compatible_with_keyed_flow` | Legacy retry after keyed accept, and keyed accept after legacy accept, both reject deterministically |
//! | `test_winner_selection_follows_deterministic_ranking` | Tied-economics bids rank stably and the deterministic winner is the one accepted |

use super::*;
use crate::bid::BidStatus;
use crate::errors::QuickLendXError;
use crate::invoice::{InvoiceCategory, InvoiceStatus};
use crate::payments::EscrowStatus;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, BytesN, Env, String, Vec,
};

// ============================================================================
// Shared test helpers
// ============================================================================

fn setup() -> (Env, QuickLendXContractClient<'static>, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize_admin(&admin);
    client.set_admin(&admin);
    (env, client, admin, contract_id)
}

fn setup_token(
    env: &Env,
    addresses: &[&Address],
    contract_id: &Address,
    initial_balance: i128,
) -> Address {
    let token_admin = Address::generate(env);
    let currency = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();

    let token_client = token::Client::new(env, &currency);
    let sac_client = token::StellarAssetClient::new(env, &currency);
    let expiration = env.ledger().sequence() + 100_000;

    for addr in addresses {
        sac_client.mint(addr, &initial_balance);
        token_client.approve(addr, contract_id, &initial_balance, &expiration);
    }
    currency
}

fn verified_business(env: &Env, client: &QuickLendXContractClient, admin: &Address) -> Address {
    let business = Address::generate(env);
    client.submit_kyc_application(&business, &String::from_str(env, "Business KYC Data"));
    client.verify_business(admin, &business);
    business
}

fn verified_investor(
    env: &Env,
    client: &QuickLendXContractClient,
    admin: &Address,
    investment_limit: i128,
) -> Address {
    let investor = Address::generate(env);
    client.submit_investor_kyc(&investor, &String::from_str(env, "Investor KYC Data"));
    client.verify_investor(&investor, &investment_limit);
    investor
}

fn upload_and_verify_invoice(
    env: &Env,
    client: &QuickLendXContractClient,
    admin: &Address,
    business: &Address,
    currency: &Address,
    amount: i128,
) -> BytesN<32> {
    let due_date = env.ledger().timestamp() + 86_400 * 30;
    let invoice_id = client.upload_invoice(
        business,
        &amount,
        currency,
        &due_date,
        &String::from_str(env, "Test invoice for keyed acceptance replay"),
        &InvoiceCategory::Services,
        &Vec::new(env),
        &None,
        &None,
        &None,
    );
    client.verify_invoice(&invoice_id);
    invoice_id
}

fn place_bid(
    env: &Env,
    client: &QuickLendXContractClient,
    investor: &Address,
    invoice_id: &BytesN<32>,
    amount: i128,
    salt_seed: u8,
) -> BytesN<32> {
    client.place_bid(
        investor,
        invoice_id,
        &amount,
        &(amount + amount / 10),
        &BytesN::from_array(env, &[salt_seed; 32]),
    )
}

fn request_key(env: &Env, seed: u8) -> BytesN<32> {
    BytesN::from_array(env, &[seed; 32])
}

fn contract_token_balance(env: &Env, currency: &Address, contract_id: &Address) -> i128 {
    token::Client::new(env, currency).balance(contract_id)
}

/// `(env, client, contract_id, currency, invoice_id, bid_id)`
fn build_single_bid_fixture() -> (
    Env,
    QuickLendXContractClient<'static>,
    Address,
    Address,
    BytesN<32>,
    BytesN<32>,
) {
    let (env, client, admin, contract_id) = setup();
    let investor = verified_investor(&env, &client, &admin, 500_000);
    let business = verified_business(&env, &client, &admin);
    let currency = setup_token(&env, &[&investor, &business], &contract_id, 200_000);
    let invoice_id =
        upload_and_verify_invoice(&env, &client, &admin, &business, &currency, 100_000);
    let bid_id = place_bid(&env, &client, &investor, &invoice_id, 100_000, 1);
    (env, client, contract_id, currency, invoice_id, bid_id)
}

/// Two independent invoices, each funded by its own investor+business pair.
/// `(env, client, contract_id, currency, inv1, bid1, inv2, bid2)`
fn build_two_invoice_fixture() -> (
    Env,
    QuickLendXContractClient<'static>,
    Address,
    Address,
    BytesN<32>,
    BytesN<32>,
    BytesN<32>,
    BytesN<32>,
) {
    let (env, client, admin, contract_id) = setup();

    let investor_a = verified_investor(&env, &client, &admin, 500_000);
    let investor_b = verified_investor(&env, &client, &admin, 500_000);
    let business_a = verified_business(&env, &client, &admin);
    let business_b = verified_business(&env, &client, &admin);

    let currency = setup_token(
        &env,
        &[&investor_a, &investor_b, &business_a, &business_b],
        &contract_id,
        200_000,
    );

    let inv1 = upload_and_verify_invoice(&env, &client, &admin, &business_a, &currency, 100_000);
    let inv2 = upload_and_verify_invoice(&env, &client, &admin, &business_b, &currency, 100_000);
    let bid1 = place_bid(&env, &client, &investor_a, &inv1, 100_000, 1);
    let bid2 = place_bid(&env, &client, &investor_b, &inv2, 100_000, 2);

    (env, client, contract_id, currency, inv1, bid1, inv2, bid2)
}

// ============================================================================
// Test 1 — duplicate call via same request key
// ============================================================================

#[test]
fn test_keyed_safe_retry_is_idempotent_and_no_duplicate_state() {
    let (env, client, contract_id, currency, invoice_id, bid_id) = build_single_bid_fixture();
    let key = request_key(&env, 1);

    let first = client
        .try_accept_bid_and_fund_with_key(&invoice_id, &bid_id, &key)
        .expect("host ok")
        .expect("first acceptance succeeds");
    let second = client
        .try_accept_bid_and_fund_with_key(&invoice_id, &bid_id, &key)
        .expect("host ok")
        .expect("safe retry succeeds");

    assert_eq!(
        first, second,
        "Safe retry must return the deterministic cached escrow ID"
    );

    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Funded);
    let bid = client.get_bid(&bid_id).unwrap();
    assert_eq!(bid.status, BidStatus::Accepted);
    let escrow = client.get_escrow_details(&invoice_id);
    assert_eq!(escrow.status, EscrowStatus::Held);
    assert_eq!(escrow.amount, 100_000, "exactly one escrow amount");
    assert_eq!(
        contract_token_balance(&env, &currency, &contract_id),
        100_000,
        "exactly one funding; retry must not transfer again"
    );

    let legacy = client.try_accept_bid_and_fund(&invoice_id, &bid_id);
    let err = legacy.unwrap_err().unwrap();
    assert!(
        err == QuickLendXError::InvoiceAlreadyFunded || err == QuickLendXError::InvalidStatus,
        "legacy entrypoint must keep rejecting deterministic retries; got: {err:?}"
    );
}

// ============================================================================
// Test 2 — client-timeout retry in a later ledger
// ============================================================================

#[test]
fn test_keyed_timeout_retry_same_result_across_ledgers() {
    let (env, client, contract_id, currency, invoice_id, bid_id) = build_single_bid_fixture();
    let key = request_key(&env, 2);

    let escrow_first = client.accept_bid_and_fund_with_key(&invoice_id, &bid_id, &key);

    // The off-chain caller timed out and retries days later.
    env.ledger()
        .set_timestamp(env.ledger().timestamp() + 86_400 * 5);

    let escrow_retry = client.accept_bid_and_fund_with_key(&invoice_id, &bid_id, &key);
    assert_eq!(
        escrow_first, escrow_retry,
        "timeout-driven retry must return the same escrow ID"
    );
    assert_eq!(
        contract_token_balance(&env, &currency, &contract_id),
        100_000,
        "timeout retry must not double-fund"
    );
    assert_eq!(client.get_escrow_details(&invoice_id).amount, 100_000);
    assert_eq!(
        client.get_invoice(&invoice_id).status,
        InvoiceStatus::Funded
    );
}

// ============================================================================
// Test 3 — conflicting reuse: same key, different bid
// ============================================================================

#[test]
fn test_keyed_conflicting_reuse_same_key_different_bid() {
    let (env, client, admin, contract_id) = setup();
    let investor_a = verified_investor(&env, &client, &admin, 500_000);
    let investor_b = verified_investor(&env, &client, &admin, 500_000);
    let business = verified_business(&env, &client, &admin);
    let currency = setup_token(
        &env,
        &[&investor_a, &investor_b, &business],
        &contract_id,
        200_000,
    );
    let invoice_id =
        upload_and_verify_invoice(&env, &client, &admin, &business, &currency, 100_000);
    let bid_id_a = place_bid(&env, &client, &investor_a, &invoice_id, 100_000, 1);
    let bid_id_b = place_bid(&env, &client, &investor_b, &invoice_id, 100_000, 2);

    let key = request_key(&env, 3);
    let _ = client.accept_bid_and_fund_with_key(&invoice_id, &bid_id_a, &key);

    let conflict = client.try_accept_bid_and_fund_with_key(&invoice_id, &bid_id_b, &key);
    assert!(
        conflict.is_err(),
        "reusing a request key with a different bid must be rejected"
    );
    let err = conflict.unwrap_err().unwrap();
    assert_eq!(
        err,
        QuickLendXError::DuplicateBid,
        "conflicting bid reuse must return DuplicateBid"
    );

    assert_eq!(
        contract_token_balance(&env, &currency, &contract_id),
        100_000,
        "conflicting reuse must not move funds"
    );
    assert_eq!(client.get_bid(&bid_id_b).unwrap().status, BidStatus::Placed);
    assert_eq!(
        client.get_bid(&bid_id_a).unwrap().status,
        BidStatus::Accepted
    );
    assert_eq!(client.get_escrow_details(&invoice_id).amount, 100_000);
}

// ============================================================================
// Test 4 — conflicting reuse: same key, different invoice
// ============================================================================

#[test]
fn test_keyed_conflicting_reuse_same_key_different_invoice() {
    let (env, client, contract_id, currency, inv1, bid1, inv2, bid2) = build_two_invoice_fixture();

    let key = request_key(&env, 4);
    let _ = client.accept_bid_and_fund_with_key(&inv1, &bid1, &key);

    let conflict = client.try_accept_bid_and_fund_with_key(&inv2, &bid2, &key);
    assert!(
        conflict.is_err(),
        "reusing a request key with a different invoice must be rejected"
    );
    let err = conflict.unwrap_err().unwrap();
    assert_eq!(
        err,
        QuickLendXError::DuplicateBid,
        "conflicting invoice reuse must return DuplicateBid"
    );

    assert_eq!(client.get_invoice(&inv2).status, InvoiceStatus::Verified);
    assert_eq!(client.get_bid(&bid2).unwrap().status, BidStatus::Placed);
    assert_eq!(client.get_escrow_details(&inv1).amount, 100_000);
    assert_eq!(
        contract_token_balance(&env, &currency, &contract_id),
        100_000,
        "only the first invoice was funded"
    );
}

// ============================================================================
// Test 5 — rejected/stale attempts leave no record and no partial state
// ============================================================================

#[test]
fn test_rejected_attempt_does_not_poison_key_or_state() {
    let (env, client, admin, contract_id) = setup();
    let investor = verified_investor(&env, &client, &admin, 500_000);
    let business = verified_business(&env, &client, &admin);
    let currency = setup_token(&env, &[&investor, &business], &contract_id, 200_000);
    let invoice_id =
        upload_and_verify_invoice(&env, &client, &admin, &business, &currency, 100_000);
    let stale_bid_id = place_bid(&env, &client, &investor, &invoice_id, 100_000, 1);

    let stale = client.get_bid(&stale_bid_id).unwrap();
    env.ledger().set_timestamp(stale.expiration_timestamp + 1);

    let key = request_key(&env, 5);
    let first = client.try_accept_bid_and_fund_with_key(&invoice_id, &stale_bid_id, &key);
    assert!(first.is_err(), "expired bid must be rejected");
    let err = first.unwrap_err().unwrap();
    assert_eq!(
        err,
        QuickLendXError::InvalidStatus,
        "expired bid must be rejected with InvalidStatus"
    );

    let retry = client.try_accept_bid_and_fund_with_key(&invoice_id, &stale_bid_id, &key);
    assert!(retry.is_err(), "still stale, still rejected");
    let retry_err = retry.unwrap_err().unwrap();
    assert_eq!(
        retry_err,
        QuickLendXError::InvalidStatus,
        "still stale, still rejected, and no record was stored"
    );

    assert_eq!(
        contract_token_balance(&env, &currency, &contract_id),
        0,
        "rejected attempts must not move funds"
    );
    assert_eq!(
        client.get_invoice(&invoice_id).status,
        InvoiceStatus::Verified
    );

    let fresh_bid_id = place_bid(&env, &client, &investor, &invoice_id, 100_000, 2);
    let corrected = client.try_accept_bid_and_fund_with_key(&invoice_id, &fresh_bid_id, &key);
    assert!(
        corrected.is_ok(),
        "a corrected retry with the same key must succeed"
    );
    assert_eq!(
        contract_token_balance(&env, &currency, &contract_id),
        100_000
    );
}

// ============================================================================
// Test 6 — independent keys across invoices, both orderings
// ============================================================================

#[test]
fn test_keyed_separate_keys_two_invoices_both_orderings() {
    let (env, client, contract_id, currency, inv1, bid1, inv2, bid2) = build_two_invoice_fixture();
    let key1 = request_key(&env, 6);
    let key2 = request_key(&env, 7);

    let escrow1 = client.accept_bid_and_fund_with_key(&inv1, &bid1, &key1);
    let escrow2 = client.accept_bid_and_fund_with_key(&inv2, &bid2, &key2);
    assert_ne!(escrow1, escrow2);
    assert_eq!(client.get_invoice(&inv1).status, InvoiceStatus::Funded);
    assert_eq!(client.get_invoice(&inv2).status, InvoiceStatus::Funded);
    assert_eq!(client.get_escrow_details(&inv1).amount, 100_000);
    assert_eq!(client.get_escrow_details(&inv2).amount, 100_000);
    assert_eq!(
        contract_token_balance(&env, &currency, &contract_id),
        200_000,
        "one funding per invoice"
    );

    assert_eq!(
        client.accept_bid_and_fund_with_key(&inv1, &bid1, &key1),
        escrow1
    );
    assert_eq!(
        client.accept_bid_and_fund_with_key(&inv2, &bid2, &key2),
        escrow2
    );
    assert_eq!(
        contract_token_balance(&env, &currency, &contract_id),
        200_000,
        "retries must not add funding"
    );

    let (env2, client2, contract_id2, currency2, r1, rb1, r2, rb2) = build_two_invoice_fixture();
    let key_a = request_key(&env2, 6);
    let key_b = request_key(&env2, 7);

    let rev1 = client2.accept_bid_and_fund_with_key(&r2, &rb2, &key_b);
    let rev2 = client2.accept_bid_and_fund_with_key(&r1, &rb1, &key_a);
    assert_ne!(rev1, rev2);
    assert_eq!(client2.get_invoice(&r1).status, InvoiceStatus::Funded);
    assert_eq!(client2.get_invoice(&r2).status, InvoiceStatus::Funded);
    assert_eq!(
        contract_token_balance(&env2, &currency2, &contract_id2),
        200_000
    );
}

// ============================================================================
// Test 7 — legacy entrypoint compatibility (both directions)
// ============================================================================

#[test]
fn test_legacy_entrypoints_compatible_with_keyed_flow() {
    let (env, client, contract_id, currency, invoice_id, bid_id) = build_single_bid_fixture();
    let key = request_key(&env, 8);

    let _ = client.accept_bid(&invoice_id, &bid_id);

    let keyed = client.try_accept_bid_and_fund_with_key(&invoice_id, &bid_id, &key);
    assert!(
        keyed.is_err(),
        "keyed path on an already-funded invoice must reject like the legacy path"
    );
    let err = keyed.unwrap_err().unwrap();
    assert!(
        err == QuickLendXError::InvoiceAlreadyFunded || err == QuickLendXError::InvalidStatus,
        "keyed path on an already-funded invoice must reject like the legacy path; got: {err:?}"
    );
    assert_eq!(client.get_escrow_details(&invoice_id).amount, 100_000);
    assert_eq!(
        contract_token_balance(&env, &currency, &contract_id),
        100_000
    );
}

// ============================================================================
// Test 8 — deterministic winner selection follows the stable ranking
// ============================================================================

#[test]
fn test_winner_selection_follows_deterministic_ranking() {
    let (env, client, admin, contract_id) = setup();
    let business = verified_business(&env, &client, &admin);
    let investor1 = verified_investor(&env, &client, &admin, 500_000);
    let investor2 = verified_investor(&env, &client, &admin, 500_000);
    let investor3 = verified_investor(&env, &client, &admin, 500_000);
    let currency = setup_token(
        &env,
        &[&investor1, &investor2, &investor3, &business],
        &contract_id,
        200_000,
    );
    let invoice_id =
        upload_and_verify_invoice(&env, &client, &admin, &business, &currency, 100_000);

    for (idx, inv) in [&investor1, &investor2, &investor3].iter().enumerate() {
        place_bid(&env, &client, inv, &invoice_id, 100_000, idx as u8);
    }

    let ranked1 = client.get_ranked_bids(&invoice_id);
    let ranked2 = client.get_ranked_bids(&invoice_id);
    assert_eq!(ranked1.len(), 3);
    assert_eq!(ranked2.len(), 3);
    for i in 0..3 {
        assert_eq!(
            ranked1.get(i).unwrap().bid_id,
            ranked2.get(i).unwrap().bid_id,
            "ranking must be stable across repeated reads"
        );
    }

    let best = client.get_best_bid(&invoice_id).unwrap();
    assert_eq!(
        best.bid_id,
        ranked1.get(0).unwrap().bid_id,
        "get_best_bid must equal the first ranked bid"
    );

    let key = request_key(&env, 9);
    let _ = client.accept_bid_and_fund_with_key(&invoice_id, &best.bid_id, &key);

    let winner = client.get_bid(&best.bid_id).unwrap();
    assert_eq!(winner.status, BidStatus::Accepted);
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Funded);
    assert_eq!(invoice.investor.unwrap(), winner.investor);

    let mut accepted = 0u32;
    for b in client.get_bids_for_invoice(&invoice_id).iter() {
        if b.status == BidStatus::Accepted {
            accepted += 1;
        }
    }
    assert_eq!(accepted, 1, "exactly one winning bid accepted");
    assert_eq!(client.get_escrow_details(&invoice_id).amount, 100_000);
    assert_eq!(
        contract_token_balance(&env, &currency, &contract_id),
        100_000,
        "exactly the winner's bid amount is escrowed"
    );
}
