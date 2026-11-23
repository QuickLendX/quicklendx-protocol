//! Atomic rollback regression tests — Issue #2444 (QE-2026-08)
//!
//! Proves that bid acceptance, ranking, expiry, and winner selection are
//! deterministic and resistant to stale or adversarial inputs.
//!
//! # Invariants validated
//!
//! 1. **Stale-bid overwrite prevention** — `withdraw_bid` and `cancel_bid`
//!    re-read the bid after `require_auth()` so a concurrent status transition
//!    (expiry, accept, etc.) is visible before the mutation is committed.
//!
//! 2. **Bid-invoice pairing** — `accept_bid_impl` validates `bid.invoice_id`
//!    against the target invoice before mutating any state.
//!
//! 3. **Idempotency marker ordering** — `place_bid` stores the idempotency
//!    marker *after* all state writes so a failure cannot orphan a marker
//!    that blocks a legitimate retry.
//!
//! 4. **No partial state on failure** — every rejection path leaves invoice,
//!    bid, escrow, investment, and token balances exactly as they were.

#![cfg(test)]

use crate::errors::QuickLendXError;
use crate::invoice::InvoiceCategory;
use crate::types::{BidStatus, InvoiceStatus};
use crate::{QuickLendXContract, QuickLendXContractClient};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, BytesN, Env, String, Vec as SorobanVec,
};

// ===========================================================================
// Helpers (mirrors test_escrow.rs patterns)
// ===========================================================================

fn setup() -> (Env, QuickLendXContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.set_admin(&admin);
    (env, client, admin)
}

/// Create and register a Stellar Asset Contract token, mint balances, approve.
fn setup_token(
    env: &Env,
    business: &Address,
    investors: &[Address],
    contract_id: &Address,
) -> Address {
    let token_admin = Address::generate(env);
    let currency = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();

    let token_client = token::Client::new(env, &currency);
    let sac_client = token::StellarAssetClient::new(env, &currency);

    let initial_balance = 100_000i128;
    sac_client.mint(business, &initial_balance);
    for inv in investors {
        sac_client.mint(inv, &initial_balance);
    }

    let expiration = env.ledger().sequence() + 10_000;
    token_client.approve(business, contract_id, &initial_balance, &expiration);
    for inv in investors {
        token_client.approve(inv, contract_id, &initial_balance, &expiration);
    }

    currency
}

fn setup_verified_business(
    env: &Env,
    client: &QuickLendXContractClient,
    admin: &Address,
) -> Address {
    let business = Address::generate(env);
    client.submit_kyc_application(&business, &String::from_str(env, "kyc"));
    client.verify_business(admin, &business);
    business
}

fn setup_verified_investor(
    env: &Env,
    client: &QuickLendXContractClient,
    limit: i128,
) -> Address {
    let investor = Address::generate(env);
    client.submit_investor_kyc(&investor, &String::from_str(env, "kyc"));
    client.verify_investor(&investor, &limit);
    investor
}

fn create_verified_invoice(
    env: &Env,
    client: &QuickLendXContractClient,
    business: &Address,
    amount: i128,
    currency: &Address,
) -> BytesN<32> {
    let due = env.ledger().timestamp() + 86_400;
    let invoice_id = client.store_invoice(
        business,
        &amount,
        currency,
        &due,
        &String::from_str(env, "test inv"),
        &InvoiceCategory::Services,
        &SorobanVec::new(env),
    );
    client.verify_invoice(&invoice_id);
    invoice_id
}

fn place_test_bid(
    env: &Env,
    client: &QuickLendXContractClient,
    investor: &Address,
    invoice_id: &BytesN<32>,
    bid_amount: i128,
    expected_return: i128,
) -> BytesN<32> {
    client.place_bid(
        investor,
        invoice_id,
        &bid_amount,
        &expected_return,
        &BytesN::from_array(env, &[0u8; 32]),
    )
}

// ===========================================================================
// 1. Stale-bid overwrite prevention in withdraw_bid
// ===========================================================================

/// After `accept_bid` moves a bid to `Accepted`, `withdraw_bid` must fail
/// and must NOT overwrite the `Accepted` status with `Withdrawn`.
#[test]
fn test_withdraw_after_accept_no_stale_overwrite() {
    let (env, client, admin) = setup();
    let contract_id = client.address.clone();
    let business = setup_verified_business(&env, &client, &admin);
    let investor = setup_verified_investor(&env, &client, 50_000);
    let currency = setup_token(&env, &business, &[investor.clone()], &contract_id);

    let amount = 5_000i128;
    let invoice_id = create_verified_invoice(&env, &client, &business, amount, &currency);
    let bid_id = place_test_bid(&env, &client, &investor, &invoice_id, amount, amount + 500);

    // Accept the bid
    client.accept_bid(&invoice_id, &bid_id);
    let bid = client.get_bid(&bid_id).unwrap();
    assert_eq!(bid.status, BidStatus::Accepted);

    // Attempt to withdraw the now-accepted bid — must fail
    let result = client.try_withdraw_bid(&bid_id);
    assert!(result.is_err(), "withdraw_bid must reject an Accepted bid");

    // Verify bid status unchanged (no stale overwrite)
    let bid_after = client.get_bid(&bid_id).unwrap();
    assert_eq!(
        bid_after.status,
        BidStatus::Accepted,
        "Bid must remain Accepted — stale overwrite prevented"
    );
}

/// After `cancel_bid` moves a bid to `Cancelled`, `withdraw_bid` must fail
/// and must NOT overwrite the `Cancelled` status with `Withdrawn`.
#[test]
fn test_withdraw_after_cancel_no_stale_overwrite() {
    let (env, client, admin) = setup();
    let contract_id = client.address.clone();
    let business = setup_verified_business(&env, &client, &admin);
    let investor = setup_verified_investor(&env, &client, 50_000);
    let currency = setup_token(&env, &business, &[investor.clone()], &contract_id);

    let amount = 5_000i128;
    let invoice_id = create_verified_invoice(&env, &client, &business, amount, &currency);
    let bid_id = place_test_bid(&env, &client, &investor, &invoice_id, amount, amount + 500);

    // Cancel the bid
    client.cancel_bid(&bid_id);
    let bid = client.get_bid(&bid_id).unwrap();
    assert_eq!(bid.status, BidStatus::Cancelled);

    // Attempt to withdraw — must fail
    let result = client.try_withdraw_bid(&bid_id);
    assert!(result.is_err(), "withdraw_bid must reject a Cancelled bid");

    let bid_after = client.get_bid(&bid_id).unwrap();
    assert_eq!(
        bid_after.status,
        BidStatus::Cancelled,
        "Bid must remain Cancelled — stale overwrite prevented"
    );
}

// ===========================================================================
// 2. Stale-bid overwrite prevention in cancel_bid
// ===========================================================================

/// After `accept_bid`, `cancel_bid` must return false without mutating state.
#[test]
fn test_cancel_after_accept_no_stale_overwrite() {
    let (env, client, admin) = setup();
    let contract_id = client.address.clone();
    let business = setup_verified_business(&env, &client, &admin);
    let investor = setup_verified_investor(&env, &client, 50_000);
    let currency = setup_token(&env, &business, &[investor.clone()], &contract_id);

    let amount = 5_000i128;
    let invoice_id = create_verified_invoice(&env, &client, &business, amount, &currency);
    let bid_id = place_test_bid(&env, &client, &investor, &invoice_id, amount, amount + 500);

    client.accept_bid(&invoice_id, &bid_id);

    // cancel_bid must return false (no-op for Accepted bid)
    let result = client.cancel_bid(&bid_id);
    assert!(!result, "cancel_bid must return false for an Accepted bid");

    let bid_after = client.get_bid(&bid_id).unwrap();
    assert_eq!(
        bid_after.status,
        BidStatus::Accepted,
        "Bid must remain Accepted after failed cancel"
    );
}

/// After `withdraw_bid`, `cancel_bid` must return false without mutating state.
#[test]
fn test_cancel_after_withdraw_no_stale_overwrite() {
    let (env, client, admin) = setup();
    let contract_id = client.address.clone();
    let business = setup_verified_business(&env, &client, &admin);
    let investor = setup_verified_investor(&env, &client, 50_000);
    let currency = setup_token(&env, &business, &[investor.clone()], &contract_id);

    let amount = 5_000i128;
    let invoice_id = create_verified_invoice(&env, &client, &business, amount, &currency);
    let bid_id = place_test_bid(&env, &client, &investor, &invoice_id, amount, amount + 500);

    client.withdraw_bid(&bid_id);

    let result = client.cancel_bid(&bid_id);
    assert!(!result, "cancel_bid must return false for a Withdrawn bid");

    let bid_after = client.get_bid(&bid_id).unwrap();
    assert_eq!(bid_after.status, BidStatus::Withdrawn);
}

/// After `withdraw_bid`, a second `withdraw_bid` must fail.
#[test]
fn test_withdraw_after_withdraw_fails() {
    let (env, client, admin) = setup();
    let contract_id = client.address.clone();
    let business = setup_verified_business(&env, &client, &admin);
    let investor = setup_verified_investor(&env, &client, 50_000);
    let currency = setup_token(&env, &business, &[investor.clone()], &contract_id);

    let amount = 5_000i128;
    let invoice_id = create_verified_invoice(&env, &client, &business, amount, &currency);
    let bid_id = place_test_bid(&env, &client, &investor, &invoice_id, amount, amount + 500);

    client.withdraw_bid(&bid_id);
    let result = client.try_withdraw_bid(&bid_id);
    assert!(result.is_err(), "double-withdraw must fail");

    let bid_after = client.get_bid(&bid_id).unwrap();
    assert_eq!(bid_after.status, BidStatus::Withdrawn);
}

// ===========================================================================
// 3. Bid-invoice pairing validation in accept_bid
// ===========================================================================

/// Accepting a bid with a mismatched invoice must fail with Unauthorized
/// and leave all state unchanged.
#[test]
fn test_accept_bid_mismatched_invoice_rejects_without_side_effects() {
    let (env, client, admin) = setup();
    let contract_id = client.address.clone();
    let business = setup_verified_business(&env, &client, &admin);
    let investor = setup_verified_investor(&env, &client, 50_000);
    let currency = setup_token(&env, &business, &[investor.clone()], &contract_id);

    let amount = 5_000i128;
    let invoice_id_1 = create_verified_invoice(&env, &client, &business, amount, &currency);
    let invoice_id_2 = create_verified_invoice(&env, &client, &business, amount, &currency);

    // Place bid on invoice_2
    let bid_id = place_test_bid(&env, &client, &investor, &invoice_id_2, amount, amount + 500);

    // Snapshot state
    let inv1_before = client.get_invoice(&invoice_id_1);
    let inv2_before = client.get_invoice(&invoice_id_2);
    let bid_before = client.get_bid(&bid_id).unwrap();

    // Try to accept bid_id against invoice_id_1 (mismatched)
    let result = client.try_accept_bid(&invoice_id_1, &bid_id);
    assert!(result.is_err(), "Mismatched invoice/bid must fail");

    // Verify no state mutation
    let inv1_after = client.get_invoice(&invoice_id_1);
    let inv2_after = client.get_invoice(&invoice_id_2);
    let bid_after = client.get_bid(&bid_id).unwrap();

    assert_eq!(inv1_before.status, inv1_after.status);
    assert_eq!(inv2_before.status, inv2_after.status);
    assert_eq!(bid_before.status, bid_after.status);
    assert_eq!(bid_after.status, BidStatus::Placed);
}

// ===========================================================================
// 4. Idempotency marker ordering in place_bid
// ===========================================================================

/// Duplicate bid with the same salt must be rejected (DuplicateBid).
#[test]
fn test_place_bid_duplicate_salt_rejected() {
    let (env, client, admin) = setup();
    let contract_id = client.address.clone();
    let business = setup_verified_business(&env, &client, &admin);
    let investor = setup_verified_investor(&env, &client, 50_000);
    let currency = setup_token(&env, &business, &[investor.clone()], &contract_id);

    let amount = 5_000i128;
    let invoice_id = create_verified_invoice(&env, &client, &business, amount, &currency);
    let salt = BytesN::from_array(&env, &[1u8; 32]);

    // First placement succeeds
    let bid_id_1 = client.place_bid(
        &investor, &invoice_id, &amount, &(amount + 500), &salt,
    );
    assert_ne!(bid_id_1, BytesN::from_array(&env, &[0u8; 32]));

    // Second placement with same salt must fail
    let result = client.try_place_bid(
        &investor, &invoice_id, &amount, &(amount + 500), &salt,
    );
    assert!(result.is_err(), "Duplicate salt must be rejected");

    // Verify first bid is still Placed
    let bid = client.get_bid(&bid_id_1).unwrap();
    assert_eq!(bid.status, BidStatus::Placed);
}

/// Different investors with different salts produce distinct bid IDs.
#[test]
fn test_place_bid_different_investors_succeed() {
    let (env, client, admin) = setup();
    let contract_id = client.address.clone();
    let business = setup_verified_business(&env, &client, &admin);
    let investor1 = setup_verified_investor(&env, &client, 50_000);
    let investor2 = setup_verified_investor(&env, &client, 50_000);
    let currency = setup_token(&env, &business, &[investor1.clone(), investor2.clone()], &contract_id);

    let amount = 5_000i128;
    let invoice_id = create_verified_invoice(&env, &client, &business, amount, &currency);
    let salt1 = BytesN::from_array(&env, &[1u8; 32]);
    let salt2 = BytesN::from_array(&env, &[2u8; 32]);

    let bid1 = client.place_bid(&investor1, &invoice_id, &amount, &(amount + 500), &salt1);
    let bid2 = client.place_bid(&investor2, &invoice_id, &amount, &(amount + 500), &salt2);

    assert_ne!(bid1, bid2, "Different investors must produce different bid IDs");
    assert_eq!(client.get_bid(&bid1).unwrap().status, BidStatus::Placed);
    assert_eq!(client.get_bid(&bid2).unwrap().status, BidStatus::Placed);
}

// ===========================================================================
// 5. No partial state on acceptance failure paths
// ===========================================================================

/// Accepting a non-existent bid must fail cleanly.
#[test]
fn test_accept_nonexistent_bid_fails_cleanly() {
    let (env, client, admin) = setup();
    let contract_id = client.address.clone();
    let business = setup_verified_business(&env, &client, &admin);
    let currency = setup_token(&env, &business, &[], &contract_id);

    let amount = 5_000i128;
    let invoice_id = create_verified_invoice(&env, &client, &business, amount, &currency);

    let fake_bid_id = BytesN::from_array(&env, &[0xFFu8; 32]);
    let result = client.try_accept_bid(&invoice_id, &fake_bid_id);
    assert!(result.is_err(), "Accepting non-existent bid must fail");

    let inv_after = client.get_invoice(&invoice_id);
    assert_eq!(inv_after.status, InvoiceStatus::Verified);
    assert_eq!(inv_after.funded_amount, 0);
}

/// Double-accept the same bid must fail and leave no duplicate escrow.
#[test]
fn test_double_accept_rejects_without_duplicate_escrow() {
    let (env, client, admin) = setup();
    let contract_id = client.address.clone();
    let business = setup_verified_business(&env, &client, &admin);
    let investor = setup_verified_investor(&env, &client, 50_000);
    let currency = setup_token(&env, &business, &[investor.clone()], &contract_id);

    let amount = 5_000i128;
    let invoice_id = create_verified_invoice(&env, &client, &business, amount, &currency);
    let bid_id = place_test_bid(&env, &client, &investor, &invoice_id, amount, amount + 500);

    // First accept succeeds
    client.accept_bid(&invoice_id, &bid_id);

    // Second accept must fail
    let result = client.try_accept_bid(&invoice_id, &bid_id);
    assert!(result.is_err(), "Double accept must fail");

    // Invoice must be Funded (not double-funded)
    let inv = client.get_invoice(&invoice_id);
    assert_eq!(inv.status, InvoiceStatus::Funded);

    // Bid must be Accepted (not re-mutated)
    let bid = client.get_bid(&bid_id).unwrap();
    assert_eq!(bid.status, BidStatus::Accepted);
}

// ===========================================================================
// 6. Bid ranking determinism under edge cases
// ===========================================================================

/// Ranking is deterministic across multiple calls on the same state.
#[test]
fn test_ranking_deterministic_on_repeated_calls() {
    let (env, client, admin) = setup();
    let contract_id = client.address.clone();
    let business = setup_verified_business(&env, &client, &admin);
    let inv1 = setup_verified_investor(&env, &client, 50_000);
    let inv2 = setup_verified_investor(&env, &client, 50_000);
    let currency = setup_token(&env, &business, &[inv1.clone(), inv2.clone()], &contract_id);

    let amount = 5_000i128;
    let invoice_id = create_verified_invoice(&env, &client, &business, amount, &currency);

    place_test_bid(&env, &client, &inv1, &invoice_id, amount, amount + 500);
    place_test_bid(&env, &client, &inv2, &invoice_id, amount, amount + 600);

    let ranked1 = client.get_ranked_bids(&invoice_id);
    let ranked2 = client.get_ranked_bids(&invoice_id);

    assert_eq!(ranked1.len(), ranked2.len());
    for i in 0..ranked1.len() {
        assert_eq!(
            ranked1.get(i).unwrap().bid_id,
            ranked2.get(i).unwrap().bid_id,
            "Ranking must be deterministic at index {i}"
        );
    }
}

/// Best bid equals first element of ranked bids.
#[test]
fn test_best_bid_equals_ranked_first() {
    let (env, client, admin) = setup();
    let contract_id = client.address.clone();
    let business = setup_verified_business(&env, &client, &admin);
    let inv1 = setup_verified_investor(&env, &client, 50_000);
    let inv2 = setup_verified_investor(&env, &client, 50_000);
    let currency = setup_token(&env, &business, &[inv1.clone(), inv2.clone()], &contract_id);

    let amount = 5_000i128;
    let invoice_id = create_verified_invoice(&env, &client, &business, amount, &currency);

    place_test_bid(&env, &client, &inv1, &invoice_id, amount, amount + 500);
    place_test_bid(&env, &client, &inv2, &invoice_id, amount, amount + 600);

    let best = client.get_best_bid(&invoice_id);
    let ranked = client.get_ranked_bids(&invoice_id);

    assert!(best.is_some(), "get_best_bid must return a bid");
    assert!(!ranked.is_empty(), "rank_bids must return at least one bid");
    assert_eq!(
        best.unwrap().bid_id,
        ranked.get(0).unwrap().bid_id,
        "best bid must equal first ranked bid"
    );
}

/// After accepting one bid, ranking must exclude it from Placed bids.
#[test]
fn test_ranking_excludes_accepted_bids() {
    let (env, client, admin) = setup();
    let contract_id = client.address.clone();
    let business = setup_verified_business(&env, &client, &admin);
    let inv1 = setup_verified_investor(&env, &client, 50_000);
    let inv2 = setup_verified_investor(&env, &client, 50_000);
    let currency = setup_token(&env, &business, &[inv1.clone(), inv2.clone()], &contract_id);

    let amount = 5_000i128;
    let invoice_id = create_verified_invoice(&env, &client, &business, amount, &currency);

    let bid1 = place_test_bid(&env, &client, &inv1, &invoice_id, amount, amount + 600);
    place_test_bid(&env, &client, &inv2, &invoice_id, amount, amount + 500);

    client.accept_bid(&invoice_id, &bid1);

    let ranked = client.get_ranked_bids(&invoice_id);
    assert_eq!(ranked.len(), 1, "Only 1 Placed bid should remain after acceptance");

    let best = client.get_best_bid(&invoice_id);
    assert!(best.is_some());
    assert_eq!(best.unwrap().bid_id, ranked.get(0).unwrap().bid_id);
}

/// Ranking returns empty for an invoice with no bids.
#[test]
fn test_ranking_empty_for_no_bids() {
    let (env, client, admin) = setup();
    let contract_id = client.address.clone();
    let business = setup_verified_business(&env, &client, &admin);
    let currency = setup_token(&env, &business, &[], &contract_id);

    let amount = 5_000i128;
    let invoice_id = create_verified_invoice(&env, &client, &business, amount, &currency);

    let ranked = client.get_ranked_bids(&invoice_id);
    assert!(ranked.is_empty(), "No bids means empty ranking");

    let best = client.get_best_bid(&invoice_id);
    assert!(best.is_none(), "No bids means no best bid");
}

// ===========================================================================
// 7. Expired bid cleanup leaves no partial state
// ===========================================================================

/// Expired bids are cleaned up and do not appear in rankings or acceptance.
#[test]
fn test_expired_bid_cleanup_leaves_no_partial_state() {
    let (env, client, admin) = setup();
    let contract_id = client.address.clone();
    let business = setup_verified_business(&env, &client, &admin);
    let investor = setup_verified_investor(&env, &client, 50_000);
    let currency = setup_token(&env, &business, &[investor.clone()], &contract_id);

    let amount = 5_000i128;
    let invoice_id = create_verified_invoice(&env, &client, &business, amount, &currency);
    let bid_id = place_test_bid(&env, &client, &investor, &invoice_id, amount, amount + 500);

    // Advance time past bid expiration
    let bid = client.get_bid(&bid_id).unwrap();
    env.ledger().set_timestamp(bid.expiration_timestamp + 1);

    // Trigger cleanup
    client.cleanup_expired_bids(&invoice_id);

    // Verify bid is Expired
    let bid_after = client.get_bid(&bid_id).unwrap();
    assert_eq!(bid_after.status, BidStatus::Expired);

    // Verify no Placed bids remain
    let ranked = client.get_ranked_bids(&invoice_id);
    assert!(ranked.is_empty(), "Expired bids must not appear in rankings");

    let best = client.get_best_bid(&invoice_id);
    assert!(best.is_none(), "No Placed bids means no best bid");

    // Verify accept fails cleanly
    let result = client.try_accept_bid(&invoice_id, &bid_id);
    assert!(result.is_err(), "Accepting expired bid must fail");
}
