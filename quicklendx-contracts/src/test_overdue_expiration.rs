#![cfg(test)]

extern crate std;

use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, BytesN, Env,
};

use crate::bid::{Bid, BidStatus, BidStorage};

fn new_invoice_id(env: &Env) -> BytesN<32> {
    // This is just for tests, using the bid ID generator is fine for uniqueness
    BidStorage::generate_unique_bid_id(env)
}

fn set_time(env: &Env, ts: u64) {
    env.ledger().with_mut(|li| li.timestamp = ts);
}

fn make_bid(env: &Env, invoice_id: &BytesN<32>, status: BidStatus, expiration: u64) -> BytesN<32> {
    let bid_id = BidStorage::generate_unique_bid_id(env);
    let bid = Bid {
        bid_id: bid_id.clone(),
        invoice_id: invoice_id.clone(),
        investor: Address::generate(env),
        bid_amount: 1_000,
        expected_return: 1_100,
        timestamp: env.ledger().timestamp(),
        status,
        expiration_timestamp: expiration,
    };
    BidStorage::store_bid(env, &bid);
    BidStorage::add_bid_to_invoice(env, invoice_id, &bid_id);
    bid_id
}

fn status_of(env: &Env, bid_id: &BytesN<32>) -> BidStatus {
    BidStorage::get_bid(env, bid_id).unwrap().status
}

fn test_env() -> (Env, Address) {
    let env = Env::default();
    let cid = env.register_contract(None, crate::QuickLendXContract);
    (env, cid)
}

// ── Invariant 2: Deadline ─────────────────────────────────────────────────────

#[test]
fn placed_bid_past_deadline_is_expired() {
    let (env, cid) = test_env();
    env.as_contract(&cid, || {
        set_time(&env, 500);
        let inv = new_invoice_id(&env);
        let bid_id = make_bid(&env, &inv, BidStatus::Placed, 1000);

        set_time(&env, 1001);
        let count = BidStorage::cleanup_expired_bids(&env, &inv);

        assert_eq!(count, 1);
        assert_eq!(status_of(&env, &bid_id), BidStatus::Expired);
    });
}

#[test]
fn placed_bid_exactly_at_deadline_is_not_expired() {
    let (env, cid) = test_env();
    env.as_contract(&cid, || {
        set_time(&env, 500);
        let inv = new_invoice_id(&env);
        let bid_id = make_bid(&env, &inv, BidStatus::Placed, 1000);

        set_time(&env, 1000);
        let count = BidStorage::cleanup_expired_bids(&env, &inv);

        assert_eq!(count, 0);
        assert_eq!(status_of(&env, &bid_id), BidStatus::Placed);
    });
}

#[test]
fn placed_bid_before_deadline_is_not_expired() {
    let (env, cid) = test_env();
    env.as_contract(&cid, || {
        set_time(&env, 500);
        let inv = new_invoice_id(&env);
        let bid_id = make_bid(&env, &inv, BidStatus::Placed, 99_999);

        set_time(&env, 1000);
        let count = BidStorage::cleanup_expired_bids(&env, &inv);

        assert_eq!(count, 0);
        assert_eq!(status_of(&env, &bid_id), BidStatus::Placed);
    });
}

// ── Invariant 1: Preservation ─────────────────────────────────────────────────

#[test]
fn accepted_bid_never_expired_by_cleanup() {
    let (env, cid) = test_env();
    env.as_contract(&cid, || {
        set_time(&env, 500);
        let inv = new_invoice_id(&env);
        let bid_id = make_bid(&env, &inv, BidStatus::Accepted, 100);

        set_time(&env, 99_999);
        let count = BidStorage::cleanup_expired_bids(&env, &inv);

        assert_eq!(count, 0);
        assert_eq!(status_of(&env, &bid_id), BidStatus::Accepted);
    });
}

#[test]
fn withdrawn_bid_never_expired_by_cleanup() {
    let (env, cid) = test_env();
    env.as_contract(&cid, || {
        set_time(&env, 500);
        let inv = new_invoice_id(&env);
        let bid_id = make_bid(&env, &inv, BidStatus::Withdrawn, 100);

        set_time(&env, 99_999);
        let count = BidStorage::cleanup_expired_bids(&env, &inv);

        assert_eq!(count, 0);
        assert_eq!(status_of(&env, &bid_id), BidStatus::Withdrawn);
    });
}

#[test]
fn cancelled_bid_never_expired_by_cleanup() {
    let (env, cid) = test_env();
    env.as_contract(&cid, || {
        set_time(&env, 500);
        let inv = new_invoice_id(&env);
        let bid_id = make_bid(&env, &inv, BidStatus::Cancelled, 100);

        set_time(&env, 99_999);
        let count = BidStorage::cleanup_expired_bids(&env, &inv);

        assert_eq!(count, 0);
        assert_eq!(status_of(&env, &bid_id), BidStatus::Cancelled);
    });
}

#[test]
fn accepted_bid_fields_unchanged_after_cleanup() {
    let (env, cid) = test_env();
    env.as_contract(&cid, || {
        set_time(&env, 500);
        let inv = new_invoice_id(&env);
        let bid_id = make_bid(&env, &inv, BidStatus::Accepted, 100);
        let before = BidStorage::get_bid(&env, &bid_id).unwrap();

        set_time(&env, 99_999);
        BidStorage::cleanup_expired_bids(&env, &inv);
        let after = BidStorage::get_bid(&env, &bid_id).unwrap();

        assert_eq!(after.bid_id, before.bid_id);
        assert_eq!(after.investor, before.investor);
        assert_eq!(after.bid_amount, before.bid_amount);
        assert_eq!(after.expected_return, before.expected_return);
        assert_eq!(after.expiration_timestamp, before.expiration_timestamp);
        assert_eq!(after.status, BidStatus::Accepted);
    });
}

// ── Invariant 3: Idempotency ──────────────────────────────────────────────────

#[test]
fn cleanup_is_idempotent() {
    let (env, cid) = test_env();
    env.as_contract(&cid, || {
        set_time(&env, 500);
        let inv = new_invoice_id(&env);
        let bid_id = make_bid(&env, &inv, BidStatus::Placed, 1000);

        set_time(&env, 2000);
        assert_eq!(BidStorage::cleanup_expired_bids(&env, &inv), 1);
        assert_eq!(BidStorage::cleanup_expired_bids(&env, &inv), 0);
        assert_eq!(status_of(&env, &bid_id), BidStatus::Expired);
    });
}

// ── Invariant 4: Field integrity ──────────────────────────────────────────────

#[test]
fn only_status_changes_on_expiry() {
    let (env, cid) = test_env();
    env.as_contract(&cid, || {
        set_time(&env, 200);
        let inv = new_invoice_id(&env);
        let bid_id = make_bid(&env, &inv, BidStatus::Placed, 1000);
        let before = BidStorage::get_bid(&env, &bid_id).unwrap();

        set_time(&env, 2000);
        BidStorage::cleanup_expired_bids(&env, &inv);
        let after = BidStorage::get_bid(&env, &bid_id).unwrap();

        assert_eq!(after.bid_id, before.bid_id);
        assert_eq!(after.invoice_id, before.invoice_id);
        assert_eq!(after.investor, before.investor);
        assert_eq!(after.bid_amount, before.bid_amount);
        assert_eq!(after.expected_return, before.expected_return);
        assert_eq!(after.timestamp, before.timestamp);
        assert_eq!(after.expiration_timestamp, before.expiration_timestamp);
        assert_eq!(after.status, BidStatus::Expired);
    });
}

// ── Mixed status sets ─────────────────────────────────────────────────────────

#[test]
fn mixed_set_only_eligible_placed_bids_expired() {
    let (env, cid) = test_env();
    env.as_contract(&cid, || {
        set_time(&env, 100);
        let inv = new_invoice_id(&env);

        let placed_past  = make_bid(&env, &inv, BidStatus::Placed,    500);
        let placed_past2 = make_bid(&env, &inv, BidStatus::Placed,    800);
        let placed_future= make_bid(&env, &inv, BidStatus::Placed,    99_999);
        let accepted     = make_bid(&env, &inv, BidStatus::Accepted,  500);
        let withdrawn    = make_bid(&env, &inv, BidStatus::Withdrawn, 500);
        let cancelled    = make_bid(&env, &inv, BidStatus::Cancelled, 500);
        let already_exp  = make_bid(&env, &inv, BidStatus::Expired,   500);

        set_time(&env, 1000);
        let count = BidStorage::cleanup_expired_bids(&env, &inv);

        assert_eq!(count, 2);
        assert_eq!(status_of(&env, &placed_past),   BidStatus::Expired);
        assert_eq!(status_of(&env, &placed_past2),  BidStatus::Expired);
        assert_eq!(status_of(&env, &placed_future), BidStatus::Placed);
        assert_eq!(status_of(&env, &accepted),      BidStatus::Accepted);
        assert_eq!(status_of(&env, &withdrawn),     BidStatus::Withdrawn);
        assert_eq!(status_of(&env, &cancelled),     BidStatus::Cancelled);
        assert_eq!(status_of(&env, &already_exp),   BidStatus::Expired);
    });
}

#[test]
fn post_condition_invariants_hold_after_cleanup() {
    let (env, cid) = test_env();
    env.as_contract(&cid, || {
        set_time(&env, 100);
        let inv = new_invoice_id(&env);

        make_bid(&env, &inv, BidStatus::Placed,   500);
        make_bid(&env, &inv, BidStatus::Accepted, 300);
        make_bid(&env, &inv, BidStatus::Placed,   99_999);

        set_time(&env, 1000);
        BidStorage::cleanup_expired_bids(&env, &inv);

        assert!(BidStorage::assert_bid_invariants(&env, &inv, env.ledger().timestamp()));
    });
}

#[test]
fn empty_invoice_cleanup_returns_zero() {
    let (env, cid) = test_env();
    env.as_contract(&cid, || {
        set_time(&env, 1000);
        let inv = new_invoice_id(&env);
        assert_eq!(BidStorage::cleanup_expired_bids(&env, &inv), 0);
    });
}

#[test]
fn incremental_expiry_across_multiple_passes() {
    let (env, cid) = test_env();
    env.as_contract(&cid, || {
        set_time(&env, 100);
        let inv = new_invoice_id(&env);

        let early = make_bid(&env, &inv, BidStatus::Placed, 1000);
        let mid   = make_bid(&env, &inv, BidStatus::Placed, 2000);
        let late  = make_bid(&env, &inv, BidStatus::Placed, 99_999);

        set_time(&env, 1500);
        assert_eq!(BidStorage::cleanup_expired_bids(&env, &inv), 1);
        assert_eq!(status_of(&env, &early), BidStatus::Expired);
        assert_eq!(status_of(&env, &mid),   BidStatus::Placed);
        assert_eq!(status_of(&env, &late),  BidStatus::Placed);

        set_time(&env, 2500);
        assert_eq!(BidStorage::cleanup_expired_bids(&env, &inv), 1);
        assert_eq!(status_of(&env, &mid),  BidStatus::Expired);
        assert_eq!(status_of(&env, &late), BidStatus::Placed);

        assert!(BidStorage::assert_bid_invariants(&env, &inv, env.ledger().timestamp()));
    });
}

// ── Load Testing ─────────────────────────────────────────────────────────────

#[test]
fn invoice_index_pruned_on_expiry_under_load() {
    let (env, cid) = test_env();
    env.as_contract(&cid, || {
        set_time(&env, 100);
        let inv = new_invoice_id(&env);

        // Fill invoice with many bids (up to limit)
        let mut bids = std::vec::Vec::new();
        for i in 0..50 {
            // Half will expire early, half will be long-lived
            let expiration = if i < 25 { 500 } else { 10_000 };
            bids.push(make_bid(&env, &inv, BidStatus::Placed, expiration));
        }

        // Verify initial count
        assert_eq!(BidStorage::get_bids_for_invoice(&env, &inv).len(), 50);

        // Advance time past first half
        set_time(&env, 600);
        let expired = BidStorage::cleanup_expired_bids(&env, &inv);

        assert_eq!(expired, 25);
        // After cleanup, the index should ONLY contain the 25 non-expired bids
        assert_eq!(BidStorage::get_bids_for_invoice(&env, &inv).len(), 25);

        // Advance time past second half
        set_time(&env, 11_000);
        let expired2 = BidStorage::cleanup_expired_bids(&env, &inv);

        assert_eq!(expired2, 25);
        assert_eq!(BidStorage::get_bids_for_invoice(&env, &inv).len(), 0);
    });
}

#[test]
fn investor_index_pruned_on_expiry_under_load() {
    let (env, cid) = test_env();
    env.as_contract(&cid, || {
        set_time(&env, 100);
        let investor = Address::generate(&env);

        // Place 50 bids for this investor
        for i in 0..50 {
            let inv = new_invoice_id(&env);
            let bid_id = BidStorage::generate_unique_bid_id(&env);
            let expiration = if i < 25 { 500 } else { 10_000 };
            
            let bid = Bid {
                bid_id: bid_id.clone(),
                invoice_id: inv,
                investor: investor.clone(),
                bid_amount: 100,
                expected_return: 110,
                timestamp: 100,
                status: BidStatus::Placed,
                expiration_timestamp: expiration,
            };
            BidStorage::store_bid(&env, &bid);
        }

        assert_eq!(BidStorage::get_bids_by_investor_all(&env, &investor).len(), 50);

        set_time(&env, 600);
        let active_count = BidStorage::count_active_placed_bids_for_investor(&env, &investor);

        assert_eq!(active_count, 25);
        assert_eq!(BidStorage::get_bids_by_investor_all(&env, &investor).len(), 25);

        set_time(&env, 11_000);
        let active_count2 = BidStorage::count_active_placed_bids_for_investor(&env, &investor);

        assert_eq!(active_count2, 0);
        assert_eq!(BidStorage::get_bids_by_investor_all(&env, &investor).len(), 0);
    });
}

#[test]
fn investor_index_preserves_terminal_states() {
    let (env, cid) = test_env();
    env.as_contract(&cid, || {
        set_time(&env, 100);
        let investor = Address::generate(&env);

        // 1 Accepted, 1 Withdrawn, 1 Cancelled, 1 Expired (pre-existing), 1 Placed (will expire)
        let bid_status_pairs = [
            (BidStatus::Accepted, 1000),
            (BidStatus::Withdrawn, 1000),
            (BidStatus::Cancelled, 1000),
            (BidStatus::Expired, 50),
            (BidStatus::Placed, 500),
        ];

        for (status, expiration) in bid_status_pairs {
            let inv = new_invoice_id(&env);
            let bid_id = BidStorage::generate_unique_bid_id(&env);
            let bid = Bid {
                bid_id,
                invoice_id: inv,
                investor: investor.clone(),
                bid_amount: 100,
                expected_return: 110,
                timestamp: 100,
                status,
                expiration_timestamp: expiration,
            };
            BidStorage::store_bid(&env, &bid);
        }

        // Initial count is 5
        assert_eq!(BidStorage::get_bids_by_investor_all(&env, &investor).len(), 5);

        // Advance time past 500
        set_time(&env, 600);
        let _ = BidStorage::count_active_placed_bids_for_investor(&env, &investor);

        // Should have pruned: 
        // - Pre-existing Expired
        // - Placed bit that just expired
        // Remaining: Accepted, Withdrawn, Cancelled
        let remaining_ids = BidStorage::get_bids_by_investor_all(&env, &investor);
        assert_eq!(remaining_ids.len(), 3);
        
        for rid in remaining_ids.iter() {
            let status = BidStorage::get_bid(&env, &rid).unwrap().status;
            assert!(status == BidStatus::Accepted || status == BidStatus::Withdrawn || status == BidStatus::Cancelled);
        }
    });
}
