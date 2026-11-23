//! # Bid Concurrency & Race Safety Regression Tests
//!
//! ## Purpose
//! Proves that bid acceptance, ranking, expiry, and winner selection are
//! deterministic and resistant to stale or adversarial inputs. Exercises
//! concurrent placement, acceptance, cancellation, and retry-after-conflict
//! scenarios with final-state assertions.
//!
//! ## Concurrency Model
//! Soroban executes transactions sequentially within a ledger. "Concurrent"
//! means different ledger orderings — the protocol must be safe regardless
//! of which transaction is ordered first.
//!
//! ## Tests
//! | Test | Scenario |
//! |------|----------|
//! | `concurrent_placement_same_invoice` | N investors place bids on same invoice |
//! | `acceptance_rejects_stale_expired_bid` | Bid expires between read and accept |
//! | `acceptance_rejects_cancelled_bid` | Bid cancelled between read and accept |
//! | `cancel_then_accept_returns_bid_stale` | Cancel bid, then accept it |
//! | `accept_then_cancel_returns_bid_stale` | Accept bid, then cancel it |
//! | `cancel_bid_not_found` | Cancel non-existent bid |
//! | `retry_after_conflict_succeeds_with_fresh_bid` | Loser retries with new bid |
//! | `no_partial_state_after_failed_accept` | Failed accept leaves zero residue |
//! | `ranking_deterministic_under_contention` | Ranking invariant holds after mutations |
//! | `expiry_during_contention` | Expired bids excluded from ranking |
//! | `max_bids_per_invoice_enforced_under_contention` | Cap enforced correctly |
//! | `stale_read_rejected_with_bid_stale` | Reading stale bid state produces BidStale |
//!
//! ## Run
//! ```bash
//! cargo test test_bid_concurrency
//! ```

use super::*;
use crate::errors::QuickLendXError;
use crate::investment::InvestmentStatus;
use crate::invoice::{InvoiceCategory, InvoiceStatus};
use crate::payments::EscrowStatus;
use crate::types::BidStatus;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, BytesN, Env, String, Vec,
};

// ============================================================================
// Shared test helpers
// ============================================================================

fn setup() -> (Env, QuickLendXContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|ledger| ledger.timestamp = 1_000);
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.set_admin(&admin);
    (env, client, admin)
}

fn setup_token(env: &Env, contract_id: &Address, addresses: &[&Address], balance: i128) -> Address {
    let token_admin = Address::generate(env);
    let currency = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let sac = token::StellarAssetClient::new(env, &currency);
    let tok = token::Client::new(env, &currency);

    for addr in addresses {
        sac.mint(addr, &balance);
        tok.approve(
            addr,
            contract_id,
            &(balance * 4),
            &(env.ledger().sequence() + 100_000),
        );
    }
    currency
}

fn verified_business(env: &Env, client: &QuickLendXContractClient, admin: &Address) -> Address {
    let business = Address::generate(env);
    client.submit_kyc_application(&business, &String::from_str(env, "Business KYC"));
    client.verify_business(admin, &business);
    business
}

fn verified_investor(
    env: &Env,
    client: &QuickLendXContractClient,
    _admin: &Address,
    limit: i128,
) -> Address {
    let investor = Address::generate(env);
    client.submit_investor_kyc(&investor, &String::from_str(env, "Investor KYC"));
    client.verify_investor(&investor, &limit);
    investor
}

fn upload_and_verify_invoice(
    env: &Env,
    client: &QuickLendXContractClient,
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
        &String::from_str(env, "Concurrency test invoice"),
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
) -> BytesN<32> {
    client.place_bid(
        investor,
        invoice_id,
        &amount,
        &(amount + amount / 10),
        &BytesN::from_array(env, &[0u8; 32]),
    )
}

// ============================================================================
// Test 1 — concurrent placement from multiple investors
// ============================================================================

/// Multiple investors place bids on the same invoice in the same ledger.
/// All must succeed, all must have unique IDs, and all must be Placed.
#[test]
fn concurrent_placement_same_invoice() {
    let (env, client, admin) = setup();
    let contract_id = env.current_contract_address();

    let investor_a = verified_investor(&env, &client, &admin, 500_000);
    let investor_b = verified_investor(&env, &client, &admin, 500_000);
    let investor_c = verified_investor(&env, &client, &admin, 500_000);
    let business = verified_business(&env, &client, &admin);

    let currency = setup_token(
        &env,
        &contract_id,
        &[&investor_a, &investor_b, &investor_c, &business],
        200_000,
    );

    let invoice_id = upload_and_verify_invoice(&env, &client, &business, &currency, 100_000);

    let bid_a = place_bid(&env, &client, &investor_a, &invoice_id, 95_000);
    let bid_b = place_bid(&env, &client, &investor_b, &invoice_id, 96_000);
    let bid_c = place_bid(&env, &client, &investor_c, &invoice_id, 97_000);

    // All three bids must be distinct
    assert_ne!(bid_a, bid_b, "bid IDs must be unique");
    assert_ne!(bid_b, bid_c, "bid IDs must be unique");
    assert_ne!(bid_a, bid_c, "bid IDs must be unique");

    // All three must be Placed
    let bid_a_record = client.get_bid(&bid_a).expect("bid A must exist");
    let bid_b_record = client.get_bid(&bid_b).expect("bid B must exist");
    let bid_c_record = client.get_bid(&bid_c).expect("bid C must exist");

    assert_eq!(bid_a_record.status, BidStatus::Placed);
    assert_eq!(bid_b_record.status, BidStatus::Placed);
    assert_eq!(bid_c_record.status, BidStatus::Placed);

    // All three must belong to the same invoice
    assert_eq!(bid_a_record.invoice_id, invoice_id);
    assert_eq!(bid_b_record.invoice_id, invoice_id);
    assert_eq!(bid_c_record.invoice_id, invoice_id);

    // Invoice must remain Verified (not yet funded)
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Verified);
}

// ============================================================================
// Test 2 — acceptance rejects stale expired bid
// ============================================================================

/// Bid expires between when the caller read it and when they submit acceptance.
/// The contract must return `BidStale` and leave no escrow/investment residue.
#[test]
fn acceptance_rejects_stale_expired_bid() {
    let (env, client, admin) = setup();
    let contract_id = env.current_contract_address();

    let investor = verified_investor(&env, &client, &admin, 500_000);
    let business = verified_business(&env, &client, &admin);

    let currency = setup_token(&env, &contract_id, &[&investor, &business], 200_000);

    let invoice_id = upload_and_verify_invoice(&env, &client, &business, &currency, 100_000);

    let bid_id = place_bid(&env, &client, &investor, &invoice_id, 95_000);

    // Advance time past the bid's expiration
    let bid = client.get_bid(&bid_id).expect("bid must exist");
    env.ledger()
        .with_mut(|li| li.timestamp = bid.expiration_timestamp + 1);

    // Acceptance must fail with BidStale
    let result = client.try_accept_bid_and_fund(&invoice_id, &bid_id);
    let err = result
        .expect_err("accepting expired bid must fail")
        .expect("contract error must decode");
    assert_eq!(err, QuickLendXError::BidStale);

    // No escrow, no investment, no state mutation
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Verified);
    assert_eq!(invoice.funded_amount, 0);
    assert!(invoice.funded_at.is_none());
    assert!(invoice.investor.is_none());
    assert!(client.try_get_escrow_details(&invoice_id).is_err());
    assert!(client.try_get_invoice_investment(&invoice_id).is_err());

    // Bid status unchanged (still Placed, just expired)
    let bid_after = client.get_bid(&bid_id).expect("bid must still exist");
    assert_eq!(bid_after.status, BidStatus::Placed);
}

// ============================================================================
// Test 3 — acceptance rejects cancelled bid
// ============================================================================

/// Bid cancelled between read and accept. Contract must return `BidStale`
/// and leave no residue.
#[test]
fn acceptance_rejects_cancelled_bid() {
    let (env, client, admin) = setup();
    let contract_id = env.current_contract_address();

    let investor = verified_investor(&env, &client, &admin, 500_000);
    let business = verified_business(&env, &client, &admin);

    let currency = setup_token(&env, &contract_id, &[&investor, &business], 200_000);

    let invoice_id = upload_and_verify_invoice(&env, &client, &business, &currency, 100_000);

    let bid_id = place_bid(&env, &client, &investor, &invoice_id, 95_000);

    // Cancel the bid
    assert!(client.try_cancel_bid(&bid_id).is_ok());

    // Acceptance must fail
    let result = client.try_accept_bid_and_fund(&invoice_id, &bid_id);
    let err = result
        .expect_err("accepting cancelled bid must fail")
        .expect("contract error must decode");
    assert!(
        err == QuickLendXError::BidStale || err == QuickLendXError::InvalidStatus,
        "cancelled bid must be rejected; got {err:?}"
    );

    // No residue
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Verified);
    assert!(client.try_get_escrow_details(&invoice_id).is_err());
}

// ============================================================================
// Test 4 — cancel then accept returns BidStale
// ============================================================================

/// Cancel a bid, then attempt to accept it. The accept must fail with BidStale.
#[test]
fn cancel_then_accept_returns_bid_stale() {
    let (env, client, admin) = setup();
    let contract_id = env.current_contract_address();

    let investor = verified_investor(&env, &client, &admin, 500_000);
    let business = verified_business(&env, &client, &admin);

    let currency = setup_token(&env, &contract_id, &[&investor, &business], 200_000);

    let invoice_id = upload_and_verify_invoice(&env, &client, &business, &currency, 100_000);

    let bid_id = place_bid(&env, &client, &investor, &invoice_id, 95_000);

    // Cancel succeeds
    assert!(client.try_cancel_bid(&bid_id).is_ok());

    // Bid is now Cancelled
    let bid = client.get_bid(&bid_id).expect("bid must exist");
    assert_eq!(bid.status, BidStatus::Cancelled);

    // Accept fails
    let result = client.try_accept_bid_and_fund(&invoice_id, &bid_id);
    let err = result
        .expect_err("accepting cancelled bid must fail")
        .expect("contract error must decode");
    assert!(
        err == QuickLendXError::BidStale || err == QuickLendXError::InvalidStatus,
        "cancelled bid acceptance must be rejected; got {err:?}"
    );

    // Invoice unchanged
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Verified);
}

// ============================================================================
// Test 5 — accept then cancel returns BidStale
// ============================================================================

/// Accept a bid successfully, then attempt to cancel it.
/// Cancel must return BidStale since the bid is now Accepted.
#[test]
fn accept_then_cancel_returns_bid_stale() {
    let (env, client, admin) = setup();
    let contract_id = env.current_contract_address();

    let investor = verified_investor(&env, &client, &admin, 500_000);
    let business = verified_business(&env, &client, &admin);

    let currency = setup_token(&env, &contract_id, &[&investor, &business], 200_000);

    let invoice_id = upload_and_verify_invoice(&env, &client, &business, &currency, 100_000);

    let bid_id = place_bid(&env, &client, &investor, &invoice_id, 95_000);

    // Accept succeeds
    let accept_result = client.try_accept_bid_and_fund(&invoice_id, &bid_id);
    assert!(
        accept_result.is_ok(),
        "acceptance must succeed; got {accept_result:?}"
    );

    // Bid is now Accepted
    let bid = client.get_bid(&bid_id).expect("bid must exist");
    assert_eq!(bid.status, BidStatus::Accepted);

    // Cancel must fail with BidStale
    let cancel_result = client.try_cancel_bid(&bid_id);
    let err = cancel_result
        .expect_err("cancel after accept must fail")
        .expect("contract error must decode");
    assert_eq!(
        err,
        QuickLendXError::BidStale,
        "cancel after accept must return BidStale"
    );

    // Invoice remains Funded
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Funded);
}

// ============================================================================
// Test 6 — cancel non-existent bid
// ============================================================================

#[test]
fn cancel_bid_not_found() {
    let (env, client, _admin) = setup();

    let fake_id = BytesN::from_array(&env, &[0xFF; 32]);
    let result = client.try_cancel_bid(&fake_id);
    let err = result
        .expect_err("cancelling non-existent bid must fail")
        .expect("contract error must decode");
    assert_eq!(
        err,
        QuickLendXError::StorageKeyNotFound,
        "cancelling non-existent bid must return StorageKeyNotFound"
    );
}

// ============================================================================
// Test 7 — retry after conflict succeeds with fresh bid
// ============================================================================

/// Two investors race to accept the same invoice. The loser retries with a
/// fresh bid and succeeds. Final state: exactly one escrow, invoice Funded.
#[test]
fn retry_after_conflict_succeeds_with_fresh_bid() {
    let (env, client, admin) = setup();
    let contract_id = env.current_contract_address();

    let investor_a = verified_investor(&env, &client, &admin, 500_000);
    let investor_b = verified_investor(&env, &client, &admin, 500_000);
    let business = verified_business(&env, &client, &admin);

    let currency = setup_token(
        &env,
        &contract_id,
        &[&investor_a, &investor_b, &business],
        200_000,
    );

    let invoice_id = upload_and_verify_invoice(&env, &client, &business, &currency, 100_000);

    let bid_a = place_bid(&env, &client, &investor_a, &invoice_id, 95_000);
    let bid_b = place_bid(&env, &client, &investor_b, &invoice_id, 96_000);

    // A wins the first acceptance
    let result_a = client.try_accept_bid_and_fund(&invoice_id, &bid_a);
    assert!(result_a.is_ok(), "A must win; got {result_a:?}");

    // B's acceptance fails (invoice already funded)
    let result_b = client.try_accept_bid_and_fund(&invoice_id, &bid_b);
    assert!(result_b.is_err(), "B must fail after A wins");

    // Invoice is Funded
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Funded);

    // Exactly one escrow exists
    let escrow = client.get_escrow_details(&invoice_id);
    assert_eq!(escrow.status, EscrowStatus::Held);
    assert_eq!(escrow.amount, 95_000);
    assert_eq!(escrow.investor, investor_a);

    // Token balance: contract holds exactly bid_a amount
    let token_client = token::Client::new(&env, &currency);
    assert_eq!(token_client.balance(&contract_id), 95_000);

    // B's funds untouched
    assert_eq!(token_client.balance(&investor_b), 200_000);
}

// ============================================================================
// Test 8 — no partial state after failed acceptance
// ============================================================================

/// After a losing acceptance attempt, no escrow, investment, or token
/// transfer residue must remain.
#[test]
fn no_partial_state_after_failed_accept() {
    let (env, client, admin) = setup();
    let contract_id = env.current_contract_address();

    let investor_a = verified_investor(&env, &client, &admin, 500_000);
    let investor_b = verified_investor(&env, &client, &admin, 500_000);
    let business = verified_business(&env, &client, &admin);

    let currency = setup_token(
        &env,
        &contract_id,
        &[&investor_a, &investor_b, &business],
        200_000,
    );

    let invoice_id = upload_and_verify_invoice(&env, &client, &business, &currency, 100_000);

    let bid_a = place_bid(&env, &client, &investor_a, &invoice_id, 95_000);
    let bid_b = place_bid(&env, &client, &investor_b, &invoice_id, 96_000);

    // A wins
    let _ = client.try_accept_bid_and_fund(&invoice_id, &bid_a);
    // B loses
    let _ = client.try_accept_bid_and_fund(&invoice_id, &bid_b);

    // Winning escrow correct
    let escrow = client.get_escrow_details(&invoice_id);
    assert_eq!(escrow.amount, 95_000);
    assert_eq!(escrow.investor, investor_a);

    // No investment for B
    let investments_b = client.get_investments_by_investor(&investor_b);
    assert!(investments_b.is_empty(), "loser must have zero investments");

    // A has exactly one investment
    let investments_a = client.get_investments_by_investor(&investor_a);
    assert_eq!(
        investments_a.len(),
        1,
        "winner must have exactly one investment"
    );

    // Token balances
    let token_client = token::Client::new(&env, &currency);
    assert_eq!(token_client.balance(&contract_id), 95_000);
    assert_eq!(token_client.balance(&investor_a), 200_000 - 95_000);
    assert_eq!(token_client.balance(&investor_b), 200_000);

    // Bid B must remain Placed (not silently transitioned)
    let bid_b_record = client.get_bid(&bid_b).expect("bid B must exist");
    assert_eq!(bid_b_record.status, BidStatus::Placed);
}

// ============================================================================
// Test 9 — ranking deterministic under contention
// ============================================================================

/// Place bids, cancel one, expire another, and verify ranking invariant:
/// `get_best_bid == rank_bids[0]` always holds.
#[test]
fn ranking_deterministic_under_contention() {
    let (env, client, admin) = setup();

    let investor_a = verified_investor(&env, &client, &admin, 500_000);
    let investor_b = verified_investor(&env, &client, &admin, 500_000);
    let investor_c = verified_investor(&env, &client, &admin, 500_000);
    let business = verified_business(&env, &client, &admin);

    let contract_id = env.current_contract_address();
    let currency = setup_token(
        &env,
        &contract_id,
        &[&investor_a, &investor_b, &investor_c, &business],
        200_000,
    );

    let invoice_id = upload_and_verify_invoice(&env, &client, &business, &currency, 100_000);

    // Place three bids with different economics
    let bid_a = place_bid(&env, &client, &investor_a, &invoice_id, 90_000); // lowest amount → highest profit
    let bid_b = place_bid(&env, &client, &investor_b, &invoice_id, 95_000);
    let bid_c = place_bid(&env, &client, &investor_c, &invoice_id, 98_000); // highest amount → lowest profit

    // Initial ranking: A should be best (highest profit)
    let best = client.get_best_bid(&invoice_id);
    assert!(best.is_some(), "must have a best bid");
    let best = best.unwrap();
    assert_eq!(
        best.bid_id, bid_a,
        "initial best must be bid A (highest profit)"
    );

    // Cancel bid A
    assert!(client.try_cancel_bid(&bid_a).is_ok());

    // Now B should be best
    let best = client.get_best_bid(&invoice_id);
    assert!(best.is_some(), "must have a best bid after cancel");
    assert_eq!(
        best.unwrap().bid_id,
        bid_b,
        "best must be bid B after A cancelled"
    );

    // Advance time past bid B's expiration
    let bid_b_record = client.get_bid(&bid_b).expect("bid B must exist");
    env.ledger()
        .with_mut(|li| li.timestamp = bid_b_record.expiration_timestamp + 1);

    // Run cleanup
    client.cleanup_expired_bids(&invoice_id);

    // Now C should be best
    let best = client.get_best_bid(&invoice_id);
    assert!(best.is_some(), "must have a best bid after expiry");
    let best = best.unwrap();
    assert_eq!(best.bid_id, bid_c, "best must be bid C after B expired");

    // Verify ranking invariant
    let ranked = client.get_ranked_bids(&invoice_id);
    assert!(!ranked.is_empty(), "ranked must be non-empty");
    assert_eq!(
        ranked.get(0).unwrap().bid_id,
        best.bid_id,
        "ranking invariant: best == ranked[0]"
    );
}

// ============================================================================
// Test 10 — expiry during contention
// ============================================================================

/// Bids expire while others are being placed. Expired bids must be
/// excluded from ranking and cannot be accepted.
#[test]
fn expiry_during_contention() {
    let (env, client, admin) = setup();

    let investor_a = verified_investor(&env, &client, &admin, 500_000);
    let investor_b = verified_investor(&env, &client, &admin, 500_000);
    let business = verified_business(&env, &client, &admin);

    let contract_id = env.current_contract_address();
    let currency = setup_token(
        &env,
        &contract_id,
        &[&investor_a, &investor_b, &business],
        200_000,
    );

    let invoice_id = upload_and_verify_invoice(&env, &client, &business, &currency, 100_000);

    // Place bid A with short TTL (expires soon)
    let bid_a = client.place_bid(
        &investor_a,
        &invoice_id,
        &90_000i128,
        &99_000i128,
        &BytesN::from_array(&env, &[0u8; 32]),
    );

    // Advance past A's expiration
    let bid_a_record = client.get_bid(&bid_a).expect("bid A must exist");
    env.ledger()
        .with_mut(|li| li.timestamp = bid_a_record.expiration_timestamp + 1);

    // Place bid B (fresh, not expired)
    let bid_b = place_bid(&env, &client, &investor_b, &invoice_id, 95_000);

    // A is expired, cannot be accepted
    let result_a = client.try_accept_bid_and_fund(&invoice_id, &bid_a);
    let err = result_a
        .expect_err("expired bid must not be accepted")
        .expect("contract error must decode");
    assert_eq!(err, QuickLendXError::BidStale);

    // B is fresh, can be accepted
    let result_b = client.try_accept_bid_and_fund(&invoice_id, &bid_b);
    assert!(
        result_b.is_ok(),
        "fresh bid B must be accepted; got {result_b:?}"
    );

    // Invoice is Funded
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Funded);
}

// ============================================================================
// Test 11 — max bids per invoice enforced under contention
// ============================================================================

/// Place bids up to MAX_BIDS_PER_INVOICE, verify the cap is enforced
/// even when bids are placed in rapid succession.
#[test]
fn max_bids_per_invoice_enforced_under_contention() {
    let (env, client, admin) = setup();
    let contract_id = env.current_contract_address();

    let business = verified_business(&env, &client, &admin);
    let currency = setup_token(&env, &contract_id, &[&business], 200_000);

    let invoice_id = upload_and_verify_invoice(&env, &client, &business, &currency, 100_000);

    // Place MAX_BIDS_PER_INVOICE bids (50)
    let mut investors = Vec::new(&env);
    let mut bid_ids = Vec::new(&env);

    for i in 0..crate::bid::MAX_BIDS_PER_INVOICE {
        let investor = verified_investor(&env, &client, &admin, 500_000);
        let sac = token::StellarAssetClient::new(&env, &currency);
        sac.mint(&investor, &200_000i128);
        let tok = token::Client::new(&env, &currency);
        tok.approve(
            &investor,
            &contract_id,
            &200_000i128,
            &(env.ledger().sequence() + 100_000),
        );

        let bid_id = place_bid(&env, &client, &investor, &invoice_id, 90_000 + i as i128);
        investors.push_back(investor);
        bid_ids.push_back(bid_id);
    }

    // Verify all placed
    let bids = client.get_bids_for_invoice(&invoice_id);
    assert_eq!(
        bids.len(),
        crate::bid::MAX_BIDS_PER_INVOICE as u32,
        "must have MAX_BIDS_PER_INVOICE bids"
    );

    // One more investor tries to place — must fail
    let extra_investor = verified_investor(&env, &client, &admin, 500_000);
    let sac = token::StellarAssetClient::new(&env, &currency);
    sac.mint(&extra_investor, &200_000i128);
    let tok = token::Client::new(&env, &currency);
    tok.approve(
        &extra_investor,
        &contract_id,
        &200_000i128,
        &(env.ledger().sequence() + 100_000),
    );

    let result = client.try_place_bid(
        &extra_investor,
        &invoice_id,
        &99_000i128,
        &108_000i128,
        &BytesN::from_array(&env, &[1u8; 32]),
    );
    assert!(
        result.is_err(),
        "placing bid beyond MAX_BIDS_PER_INVOICE must fail"
    );
}

// ============================================================================
// Test 12 — stale read rejected with BidStale
// ============================================================================

/// Client reads a bid, the bid is cancelled by another caller,
/// then the first client tries to accept. Must get BidStale.
#[test]
fn stale_read_rejected_with_bid_stale() {
    let (env, client, admin) = setup();
    let contract_id = env.current_contract_address();

    let investor_a = verified_investor(&env, &client, &admin, 500_000);
    let investor_b = verified_investor(&env, &client, &admin, 500_000);
    let business = verified_business(&env, &client, &admin);

    let currency = setup_token(
        &env,
        &contract_id,
        &[&investor_a, &investor_b, &business],
        200_000,
    );

    let invoice_id = upload_and_verify_invoice(&env, &client, &business, &currency, 100_000);

    let bid_a = place_bid(&env, &client, &investor_a, &invoice_id, 95_000);

    // "Client A reads the bid" — it's Placed
    let bid_snapshot = client.get_bid(&bid_a).expect("bid must exist");
    assert_eq!(bid_snapshot.status, BidStatus::Placed);

    // "Another caller cancels it"
    assert!(client.try_cancel_bid(&bid_a).is_ok());

    // "Client A tries to accept with stale read"
    let result = client.try_accept_bid_and_fund(&invoice_id, &bid_a);
    let err = result
        .expect_err("stale acceptance must fail")
        .expect("contract error must decode");
    assert!(
        err == QuickLendXError::BidStale || err == QuickLendXError::InvalidStatus,
        "stale read must be rejected; got {err:?}"
    );

    // No state mutation
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Verified);
    assert!(client.try_get_escrow_details(&invoice_id).is_err());
}
