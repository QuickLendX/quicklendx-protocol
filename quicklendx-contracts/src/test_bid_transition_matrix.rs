//! Bid submission and auction selection: state-transition invariants
//! (issue #2443).
//!
//! This module pins the *legal transition matrix* for `BidStatus` and proves
//! it at the actual integration boundary — the callable entry points reachable
//! by off-chain clients:
//!
//!   - `place_bid`     -> `Placed`   (bid submission)
//!   - `withdraw_bid`  -> `Withdrawn`
//!   - `cancel_bid`    -> `Cancelled`
//!   - `accept_bid`    -> `Accepted` (auction selection / escrow funding)
//!   - expiry flows    -> `Expired`
//!
//! ## Legal matrix (mirrors `BidStatus::validate_transition`)
//!
//! | From      | To                                    | Entrypoint                   |
//! |-----------|---------------------------------------|------------------------------|
//! | Placed    | Accepted, Withdrawn, Cancelled, Expired | accept/withdraw/cancel/cleanup |
//! | Accepted  | Cancelled                             | refund / withdraw-investment |
//! | Withdrawn | *(terminal)*                          | -                            |
//! | Cancelled | *(terminal)*                          | -                            |
//! | Expired   | *(terminal)*                          | -                            |
//!
//! Everything else — self-transitions, repeats, skips, and backward moves —
//! must be rejected with no partial or unauthorized state.
//!
//! ## Selection invariants
//!
//! - `get_best_bid` / `rank_bids` only consider bids that are `Placed` **and**
//!   not yet raw-expired. A stale bid (past raw TTL, still awaiting its
//!   `Placed -> Expired` storage transition inside a grace window) is never
//!   surfaced as the winner, agreeing with `accept_bid` and `get_bids_by_status`.
//! - Ranking is deterministic: best bid == first ranked bid, even on ties.
//!
//! This module is declared `#[cfg(test)]` in `lib.rs` (no `legacy-tests` gate)
//! so it runs on every CI matrix entry.

use super::*;
use crate::bid::{BidStatus, BidStorage};
use crate::errors::QuickLendXError;
use crate::invoice::InvoiceCategory;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, BytesN, Env, String, Vec,
};

const DAY: u64 = 86_400;
const BID_AMOUNT: i128 = 5_000;
const EXPECTED_RETURN: i128 = 6_000;

fn setup() -> (Env, QuickLendXContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000_000);
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.set_admin(&admin);
    (env, client, admin)
}

fn make_token(
    env: &Env,
    client: &QuickLendXContractClient,
    business: &Address,
    investor: &Address,
) -> Address {
    let contract_id = client.address.clone();
    let token_admin = Address::generate(env);
    let currency = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();
    let sac = token::StellarAssetClient::new(env, &currency);
    let tok = token::Client::new(env, &currency);
    sac.mint(business, &100_000i128);
    sac.mint(investor, &100_000i128);
    sac.mint(&contract_id, &1i128);
    let exp = env.ledger().sequence() + 100_000;
    tok.approve(business, &contract_id, &400_000i128, &exp);
    tok.approve(investor, &contract_id, &400_000i128, &exp);
    currency
}

/// Register a verified invoice and KYC'd business/investor, returning all ids.
fn funded_setup(
    env: &Env,
    client: &QuickLendXContractClient,
    admin: &Address,
    amount: i128,
) -> (Address, Address, BytesN<32>) {
    let business = Address::generate(env);
    let investor = Address::generate(env);
    let currency = make_token(env, client, &business, &investor);

    client.submit_kyc_application(&business, &String::from_str(env, "KYC"));
    client.verify_business(admin, &business);
    client.submit_investor_kyc(&investor, &String::from_str(env, "KYC"));
    client.verify_investor(&investor, &200_000i128);

    let due_date = env.ledger().timestamp() + 30 * DAY;
    let invoice_id = client.upload_invoice(
        &business,
        &amount,
        &currency,
        &due_date,
        &String::from_str(env, "Bid transition matrix"),
        &InvoiceCategory::Services,
        &Vec::new(env),
        &None,
        &None,
        &None,
    );
    client.verify_invoice(&invoice_id);
    (business, investor, invoice_id)
}

/// Place a bid with a unique salt. `nonce` must differ between placements by
/// the same investor on the same invoice, otherwise the idempotency guard
/// rejects the duplicate as `DuplicateBid`.
fn place_bid(
    env: &Env,
    client: &QuickLendXContractClient,
    investor: &Address,
    invoice_id: &BytesN<32>,
    amount: i128,
    expected_return: i128,
    nonce: u8,
) -> BytesN<32> {
    let mut salt = [0u8; 32];
    salt[0] = nonce;
    salt[1] = (amount & 0xff) as u8;
    client.place_bid(
        investor,
        invoice_id,
        &amount,
        &expected_return,
        &BytesN::from_array(env, &salt),
    )
}

// ============================================================================
// 1. EXHAUSTIVE TRANSITION MATRIX (pure guard)
// ============================================================================

/// The legal set mirrors `BidStatus::validate_transition` exactly. Every
/// `(from, to)` pair, including self-transitions, is checked.
#[test]
fn validate_transition_exhaustive_matrix() {
    let all = [
        BidStatus::Placed,
        BidStatus::Accepted,
        BidStatus::Withdrawn,
        BidStatus::Expired,
        BidStatus::Cancelled,
    ];

    let mut legal = 0usize;
    let mut illegal = 0usize;
    for from in all.iter() {
        for to in all.iter() {
            let expected_legal = matches!(
                (from, to),
                (BidStatus::Placed, _) | (BidStatus::Accepted, BidStatus::Cancelled)
            );
            match BidStatus::validate_transition(from, to) {
                Ok(()) => {
                    legal += 1;
                    assert!(
                        expected_legal,
                        "guard allowed an illegal transition {from:?} -> {to:?}"
                    );
                }
                Err(QuickLendXError::InvalidStatus) => {
                    illegal += 1;
                    assert!(
                        !expected_legal,
                        "guard rejected a legal transition {from:?} -> {to:?}"
                    );
                }
                Err(other) => panic!("unexpected error {other:?} for {from:?} -> {to:?}"),
            }
        }
    }

    // 4 legal Placed edges + 1 legal Accepted->Cancelled edge.
    assert_eq!(legal, 5);
    // 25 total pairs - 5 legal = 20 illegal.
    assert_eq!(illegal, 20);
}

/// No self-transition is allowed, so a bid can never be "re-placed in place"
/// or double-actioned at the guard level.
#[test]
fn validate_transition_rejects_all_self_transitions() {
    for status in [
        BidStatus::Placed,
        BidStatus::Accepted,
        BidStatus::Withdrawn,
        BidStatus::Expired,
        BidStatus::Cancelled,
    ] {
        assert!(BidStatus::validate_transition(&status, &status).is_err());
    }
}

/// Terminal states are immutable: no transition *out* of Withdrawn/Cancelled/
/// Expired is legal.
#[test]
fn validate_transition_terminal_states_are_immutable() {
    for terminal in [
        BidStatus::Withdrawn,
        BidStatus::Cancelled,
        BidStatus::Expired,
    ] {
        for to in [
            BidStatus::Placed,
            BidStatus::Accepted,
            BidStatus::Withdrawn,
            BidStatus::Expired,
            BidStatus::Cancelled,
        ] {
            assert!(
                BidStatus::validate_transition(&terminal, &to).is_err(),
                "{terminal:?} -> {to:?} must be illegal (terminal state)"
            );
        }
    }
}

// ============================================================================
// 2. INTEGRATION: LEGAL EDGES
// ============================================================================

/// Placed -> Withdrawn via `withdraw_bid`.
#[test]
fn withdraw_legal_from_placed() {
    let (env, client, admin) = setup();
    let (_, investor, invoice_id) = funded_setup(&env, &client, &admin, 10_000);
    let bid_id = place_bid(
        &env,
        &client,
        &investor,
        &invoice_id,
        BID_AMOUNT,
        EXPECTED_RETURN,
        1,
    );

    let result = client.try_withdraw_bid(&bid_id);
    assert!(result.is_ok());
    assert_eq!(
        client.get_bid(&bid_id).unwrap().status,
        BidStatus::Withdrawn
    );
}

/// Placed -> Cancelled via `cancel_bid`.
#[test]
fn cancel_legal_from_placed() {
    let (env, client, admin) = setup();
    let (_, investor, invoice_id) = funded_setup(&env, &client, &admin, 10_000);
    let bid_id = place_bid(
        &env,
        &client,
        &investor,
        &invoice_id,
        BID_AMOUNT,
        EXPECTED_RETURN,
        1,
    );

    assert!(client.cancel_bid(&bid_id));
    assert_eq!(
        client.get_bid(&bid_id).unwrap().status,
        BidStatus::Cancelled
    );
}

/// Placed -> Accepted via the business accept/funding path.
#[test]
fn accept_legal_from_placed() {
    let (env, client, admin) = setup();
    let (_, investor, invoice_id) = funded_setup(&env, &client, &admin, 10_000);
    let bid_id = place_bid(
        &env,
        &client,
        &investor,
        &invoice_id,
        BID_AMOUNT,
        EXPECTED_RETURN,
        1,
    );

    let result = client.try_accept_bid(&invoice_id, &bid_id);
    assert!(result.is_ok());
    assert_eq!(client.get_bid(&bid_id).unwrap().status, BidStatus::Accepted);
}

/// Placed -> Expired via the permissionless cleanup entrypoint.
#[test]
fn expire_legal_from_placed() {
    let (env, client, admin) = setup();
    client.set_bid_ttl_days(&crate::bid::MIN_BID_TTL_DAYS);
    let (_, investor, invoice_id) = funded_setup(&env, &client, &admin, 10_000);
    let bid_id = place_bid(
        &env,
        &client,
        &investor,
        &invoice_id,
        BID_AMOUNT,
        EXPECTED_RETURN,
        1,
    );
    let bid = client.get_bid(&bid_id).unwrap();
    env.ledger().set_timestamp(bid.expiration_timestamp + 1);

    assert_eq!(client.cleanup_expired_bids(&invoice_id), 1);
    assert_eq!(client.get_bid(&bid_id).unwrap().status, BidStatus::Expired);
}

// ============================================================================
// 3. INTEGRATION: ILLEGAL EDGES (stale / repeated / skipped / backward)
// ============================================================================

/// Repeated Withdrawn is rejected (already withdrawn -> no-op error), leaving
/// the terminal state and storage untouched.
#[test]
fn repeated_withdraw_is_rejected_atomic() {
    let (env, client, admin) = setup();
    let (_, investor, invoice_id) = funded_setup(&env, &client, &admin, 10_000);
    let bid_id = place_bid(
        &env,
        &client,
        &investor,
        &invoice_id,
        BID_AMOUNT,
        EXPECTED_RETURN,
        1,
    );

    assert!(client.try_withdraw_bid(&bid_id).is_ok());
    let after_first = client.get_bid(&bid_id).unwrap();

    let result = client.try_withdraw_bid(&bid_id);
    let contract_err = result
        .err()
        .expect("expected contract error")
        .expect("expected contract-level error");
    assert_eq!(contract_err, QuickLendXError::OperationNotAllowed);

    let after_second = client.get_bid(&bid_id).unwrap();
    assert_eq!(
        after_second, after_first,
        "rejected withdraw must not mutate"
    );
    assert_eq!(after_second.status, BidStatus::Withdrawn);
}

/// A Withdrawn bid cannot be Cancelled (skipped edge, terminal immutability).
#[test]
fn cancelled_from_withdrawn_is_rejected() {
    let (env, client, admin) = setup();
    let (_, investor, invoice_id) = funded_setup(&env, &client, &admin, 10_000);
    let bid_id = place_bid(
        &env,
        &client,
        &investor,
        &invoice_id,
        BID_AMOUNT,
        EXPECTED_RETURN,
        1,
    );

    client.withdraw_bid(&bid_id);
    let after_withdraw = client.get_bid(&bid_id).unwrap();

    assert!(
        !client.cancel_bid(&bid_id),
        "cancelling a Withdrawn bid is illegal and must be a no-op"
    );
    let after_cancel = client.get_bid(&bid_id).unwrap();
    assert_eq!(after_cancel, after_withdraw, "no partial state change");
    assert_eq!(after_cancel.status, BidStatus::Withdrawn);
}

/// An Expired bid cannot be silently resurrected via cleanup (no-op; count 0,
/// status stays Expired).
#[test]
fn repeated_expiry_is_idempotent_no_resurrection() {
    let (env, client, admin) = setup();
    client.set_bid_ttl_days(&crate::bid::MIN_BID_TTL_DAYS);
    let (_, investor, invoice_id) = funded_setup(&env, &client, &admin, 10_000);
    let bid_id = place_bid(
        &env,
        &client,
        &investor,
        &invoice_id,
        BID_AMOUNT,
        EXPECTED_RETURN,
        1,
    );
    let bid = client.get_bid(&bid_id).unwrap();
    env.ledger().set_timestamp(bid.expiration_timestamp + 1);

    assert_eq!(client.cleanup_expired_bids(&invoice_id), 1);
    assert_eq!(client.get_bid(&bid_id).unwrap().status, BidStatus::Expired);

    // Second sweep at the same ledger time must clean nothing and leave the
    // bid Expired (never re-placed).
    assert_eq!(client.cleanup_expired_bids(&invoice_id), 0);
    assert_eq!(client.get_bid(&bid_id).unwrap().status, BidStatus::Expired);
}

/// An Expired bid cannot be accepted (skipped edge).
#[test]
fn accept_from_expired_is_rejected() {
    let (env, client, admin) = setup();
    client.set_bid_ttl_days(&crate::bid::MIN_BID_TTL_DAYS);
    let (_, investor, invoice_id) = funded_setup(&env, &client, &admin, 10_000);
    let bid_id = place_bid(
        &env,
        &client,
        &investor,
        &invoice_id,
        BID_AMOUNT,
        EXPECTED_RETURN,
        1,
    );
    let bid = client.get_bid(&bid_id).unwrap();
    env.ledger().set_timestamp(bid.expiration_timestamp + 1);

    // Force the Placed -> Expired storage transition.
    assert_eq!(client.cleanup_expired_bids(&invoice_id), 1);
    assert_eq!(client.get_bid(&bid_id).unwrap().status, BidStatus::Expired);

    let result = client.try_accept_bid(&invoice_id, &bid_id);
    let contract_err = result
        .err()
        .expect("expected contract error")
        .expect("expected contract-level error");
    assert_eq!(contract_err, QuickLendXError::InvalidStatus);
    assert_eq!(client.get_bid(&bid_id).unwrap().status, BidStatus::Expired);
}

/// An already-Accepted bid cannot be withdrawn (repeated/terminal edge).
#[test]
fn withdraw_from_accepted_is_rejected() {
    let (env, client, admin) = setup();
    let (_, investor, invoice_id) = funded_setup(&env, &client, &admin, 10_000);
    let bid_id = place_bid(
        &env,
        &client,
        &investor,
        &invoice_id,
        BID_AMOUNT,
        EXPECTED_RETURN,
        1,
    );

    client.accept_bid(&invoice_id, &bid_id);
    assert_eq!(client.get_bid(&bid_id).unwrap().status, BidStatus::Accepted);

    let result = client.try_withdraw_bid(&bid_id);
    let contract_err = result
        .err()
        .expect("expected contract error")
        .expect("expected contract-level error");
    assert_eq!(contract_err, QuickLendXError::OperationNotAllowed);
}

// ============================================================================
// 4. SELECTION INVARIANTS (deterministic, stale-aware auction winner)
// ============================================================================

/// A stale bid — past its raw TTL but still awaiting its `Placed -> Expired`
/// storage transition inside a configured grace window — must NOT be selected
/// as the best bid, and must be absent from ranking, agreeing with
/// `accept_bid` and `get_bids_by_status`.
#[test]
fn stale_bid_not_selected_during_grace_window() {
    let (env, client, admin) = setup();
    client.set_bid_ttl_days(&crate::bid::MIN_BID_TTL_DAYS);
    // Non-zero grace window so a bid can remain stored as `Placed` past its
    // raw TTL without an eager `Placed -> Expired` storage transition.
    client.set_bid_expiry_grace_seconds(&(10 * 3600));
    let (_, _, invoice_id) = funded_setup(&env, &client, &admin, 10_000);

    // First investor: high bid that will go stale.
    let investor_a = Address::generate(&env);
    client.submit_investor_kyc(&investor_a, &String::from_str(env, "KYC"));
    client.verify_investor(&investor_a, &200_000i128);
    let bid_stale = place_bid(&env, &client, &investor_a, &invoice_id, 9_000, 10_000, 1);
    let stale_expiration = client.get_bid(&bid_stale).unwrap().expiration_timestamp;

    // Advance partway so a later-placed bid gets a later expiry and stays
    // fresh while the first bid ages out.
    env.ledger().set_timestamp(stale_expiration - 12 * 3600);

    // Second investor: competitive bid that stays fresh.
    let investor_b = Address::generate(&env);
    client.submit_investor_kyc(&investor_b, &String::from_str(env, "KYC"));
    client.verify_investor(&investor_b, &200_000i128);
    let bid_fresh = place_bid(&env, &client, &investor_b, &invoice_id, 6_000, 7_500, 1);

    // Advance so the first bid is past raw TTL but still inside the grace
    // window (grace = 10h, bid_age = just past the 1-day raw TTL).
    let ts = stale_expiration + 1;
    assert!(ts < stale_expiration + 10 * 3600, "inside grace window");
    env.ledger().set_timestamp(ts);

    // The fresh bid (placed later) is not yet expired at `ts`.
    assert!(
        stale_expiration + 12 * 3600 > ts,
        "fresh bid expiry is after the test timestamp"
    );

    // First bid still stored as Placed (grace window not yet elapsed -> no
    // storage transition), yet it is already past its raw TTL.
    assert_eq!(
        client.get_bid(&bid_stale).unwrap().status,
        BidStatus::Placed
    );

    // Selection must skip the stale bid and pick the fresh one.
    let best = client.get_best_bid(&invoice_id).expect("a winning bid");
    assert_eq!(best.bid_id, bid_fresh, "stale bid must never win");

    // rank_bids must also exclude the stale bid.
    let ranked = BidStorage::rank_bids(&env, &invoice_id);
    assert_eq!(ranked.len(), 1);
    assert_eq!(ranked.get(0).unwrap().bid_id, bid_fresh);

    // get_bids_by_status(Placed) agrees (excludes raw-expired).
    let placed = client.get_bids_by_status(&invoice_id, &BidStatus::Placed);
    assert_eq!(placed.len(), 1);
    assert_eq!(placed.get(0).unwrap().bid_id, bid_fresh);

    // And accept_bid agrees: the stale bid cannot be funded.
    let result = client.try_accept_bid(&invoice_id, &bid_stale);
    assert!(result.is_err());
    assert_eq!(
        client.get_bid(&bid_stale).unwrap().status,
        BidStatus::Placed,
        "rejected accept must not mutate the stale bid"
    );
}

/// Best bid == first ranked bid, and the winning bid is the highest profit.
#[test]
fn selection_is_deterministic_best_equals_first_ranked() {
    let (env, client, admin) = setup();
    let (_, investor, invoice_id) = funded_setup(&env, &client, &admin, 10_000);

    let _ = place_bid(&env, &client, &investor, &invoice_id, 5_000, 6_000, 1);
    place_bid(&env, &client, &investor, &invoice_id, 7_000, 8_500, 2);
    let bid_high = place_bid(&env, &client, &investor, &invoice_id, 8_000, 10_000, 3);

    let best = client.get_best_bid(&invoice_id).expect("a winning bid");
    // Highest profit is the high bid.
    assert_eq!(best.bid_id, bid_high);

    let ranked = BidStorage::rank_bids(&env, &invoice_id);
    assert!(!ranked.is_empty());
    assert_eq!(
        ranked.get(0).unwrap().bid_id,
        best.bid_id,
        "first ranked bid must equal get_best_bid"
    );

    // All three non-terminal bids participate.
    assert_eq!(ranked.len(), 3);
}

/// A Withdrawn / Cancelled bid is excluded from selection entirely.
#[test]
fn terminal_bids_excluded_from_selection() {
    let (env, client, admin) = setup();
    let (_, investor, invoice_id) = funded_setup(&env, &client, &admin, 10_000);

    let bid_withdrawn = place_bid(&env, &client, &investor, &invoice_id, 9_000, 11_000, 1);
    client.withdraw_bid(&bid_withdrawn);

    let bid_cancelled = place_bid(&env, &client, &investor, &invoice_id, 8_000, 10_000, 2);
    assert!(client.cancel_bid(&bid_cancelled));

    let bid_live = place_bid(&env, &client, &investor, &invoice_id, 5_000, 6_000, 3);

    let best = client.get_best_bid(&invoice_id).expect("live bid wins");
    assert_eq!(best.bid_id, bid_live);

    let ranked = BidStorage::rank_bids(&env, &invoice_id);
    assert_eq!(ranked.len(), 1);
    assert_eq!(ranked.get(0).unwrap().bid_id, bid_live);
}

/// Out-of-order bid ids (different placement order vs economic value) must
/// still rank deterministically by economics, then timestamp, then id.
#[test]
fn ranking_deterministic_across_placement_order() {
    let (env, client, admin) = setup();
    let (_, investor, invoice_id) = funded_setup(&env, &client, &admin, 10_000);

    // Bid placed first but with the best economics must still win.
    let bid_first_weak = place_bid(&env, &client, &investor, &invoice_id, 5_000, 5_500, 1);
    env.ledger().set_timestamp(env.ledger().timestamp() + 5);
    let bid_second_strong = place_bid(&env, &client, &investor, &invoice_id, 5_000, 6_000, 2);

    let best = client.get_best_bid(&invoice_id).expect("a winner");
    assert_eq!(best.bid_id, bid_second_strong);

    let ranked = BidStorage::rank_bids(&env, &invoice_id);
    assert_eq!(ranked.len(), 2);
    assert_eq!(ranked.get(0).unwrap().bid_id, bid_second_strong);
    assert_eq!(ranked.get(1).unwrap().bid_id, bid_first_weak);
}
