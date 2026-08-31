//! Maximum-capacity stress suite for `MAX_BIDS_PER_INVOICE` (50).
//!
//! See issue #1299. Worst-case ordering bugs, off-by-one pagination errors,
//! and instruction-budget blow-ups surface at the documented ceiling. A
//! protocol can be correct with three bids and still mis-rank or run out of
//! budget at fifty. This suite drives the documented hot paths to the
//! documented maximum.
//!
//! # Coverage
//!
//! - `place_bid` accepts exactly `MAX_BIDS_PER_INVOICE` and rejects the 51st
//!   with the canonical `MaxBidsPerInvoiceExceeded` error.
//! - `rank_bids` returns a fully-ordered 50-element ranking obeying the
//!   documented comparator chain
//!   (`profit → expected_return → bid_amount → timestamp → bid_id`).
//! - `get_best_bid` equals the head of `rank_bids` at full capacity.
//! - `cleanup_expired_bids_paged` drains a 50-bid set across multiple
//!   pages and stays consistent across re-runs.
//!
//! # Edge cases
//!
//! - **Pure final tiebreaker**: 50 identical bids differing only in
//!   `bid_id` (placed back-to-back so the ledger timestamp is constant),
//!   forcing every comparator except bid_id to tie.
//! - **All 50 expired in a single ledger slot**: full-coverage sweep
//!   cleans the lot and a re-run is fully idempotent.
//! - **Alternating expired/active across page boundaries**: chunked
//!   page-by-page cleanup isolates the partial-coverage path. A full
//!   coverage follow-up settles the storage counter; a final pass then
//!   observes the compacted, idempotent state.
//!
//! # Note on storage vs. index
//!
//! `count_bids_by_status` walks the per-invoice **index** (`count_key`
//! + per-position bid entries), not the full bid struct namespace.
//! `cleanup_expired_bids_paged` updates the index (and removes expired
//! entries from it on full coverage) but does **not** physically delete
//! the `Bid` structs from storage. So after a cleanup:
//!
//! - `get_ranked_bids` / `get_best_bid` / `count_bids_by_status` only
//!   see the surviving index entries (Placed bids only).
//! - `BidStorage::get_bid(&env, &original_bid_id)` continues to return
//!   the original `Bid` struct, with `status == Expired` for any bid
//!   that was cleaned.
//!
//! Tests that need to confirm the `Placed → Expired` transition on the
//! underlying `Bid` struct use `BidStorage::get_bid` directly.

use super::*;
use crate::bid::{Bid, BidStatus, BidStorage, MAX_BIDS_PER_INVOICE};
use crate::errors::QuickLendXError;
use crate::invoice::InvoiceCategory;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, BytesN, Env, String, Vec,
};

// ============================================================================
// Helpers
// ============================================================================

const SECONDS_PER_DAY: u64 = 86_400;

fn setup()
-> (Env, QuickLendXContractClient<'static>, Address, Address, BytesN<32>, Address) {
    let env = Env::default();
    env.cost_estimate().budget().reset_unlimited();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_700_000_000);

    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    client.set_admin(&admin);
    client.initialize_fee_system(&admin);

    let business = Address::generate(&env);
    client.submit_kyc_application(&business, &String::from_str(&env, "stress-kyc"));
    client.verify_business(&admin, &business);

    let investor = Address::generate(&env);
    client.submit_investor_kyc(&investor, &String::from_str(&env, "stress-kyc"));
    client.verify_investor(&investor, &1_000_000_000_000i128);

    client.set_max_active_bids_per_investor(&0u32);

    let token_admin = Address::generate(&env);
    let currency = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();
    let sac = token::StellarAssetClient::new(&env, &currency);
    let tok = token::Client::new(&env, &currency);
    sac.mint(&business, &10_000_000_000i128);
    sac.mint(&contract_id, &1i128);
    let exp = env.ledger().sequence() + 1_000_000;
    tok.approve(&business, &contract_id, &10_000_000_000i128, &exp);
    sac.mint(&investor, &1_000_000_000i128);
    tok.approve(&investor, &contract_id, &1_000_000_000i128, &exp);

    let due_date = env.ledger().timestamp() + 30 * SECONDS_PER_DAY;
    let invoice_id = client.upload_invoice(
        &business,
        &10_000_000_000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Stress ceiling invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    client.verify_invoice(&invoice_id);

    (env, client, admin, investor, invoice_id, contract_id)
}

fn get_active_bid_count(env: &Env, contract_id: &Address, invoice_id: &BytesN<32>) -> u32 {
    env.as_contract(contract_id, || BidStorage::get_active_bid_count(env, invoice_id))
}

fn get_bid_records_for_invoice(env: &Env, contract_id: &Address, invoice_id: &BytesN<32>) -> Vec<Bid> {
    env.as_contract(contract_id, || BidStorage::get_bid_records_for_invoice(env, invoice_id))
}

fn count_bids_by_status(env: &Env, contract_id: &Address, invoice_id: &BytesN<32>) -> (u32, u32, u32, u32, u32) {
    env.as_contract(contract_id, || BidStorage::count_bids_by_status(env, invoice_id))
}

fn get_bid(env: &Env, contract_id: &Address, bid_id: &BytesN<32>) -> Option<Bid> {
    env.as_contract(contract_id, || BidStorage::get_bid(env, bid_id))
}

fn update_bid(env: &Env, contract_id: &Address, bid: &Bid) {
    env.as_contract(contract_id, || BidStorage::update_bid(env, bid));
}

// ============================================================================
// Test 1: Full capacity + 51st rejection
// ============================================================================

#[test]
fn test_full_capacity_accepts_50_rejects_51st() {
    let (env, client, _admin, investor, invoice_id, contract_id) = setup();

    for i in 0..MAX_BIDS_PER_INVOICE {
        let bid_amount = 1_000i128 + i as i128;
        let expected_return = bid_amount + 100;
        client.place_bid(&investor, &invoice_id, &bid_amount, &expected_return, &BytesN::from_array(&env, &[0u8; 32]));
    }

    assert_eq!(
        get_active_bid_count(&env, &contract_id, &invoice_id),
        MAX_BIDS_PER_INVOICE,
        "active bid count must equal MAX_BIDS_PER_INVOICE at the ceiling"
    );

    let records = get_bid_records_for_invoice(&env, &contract_id, &invoice_id);
    assert_eq!(
        records.len() as u32,
        MAX_BIDS_PER_INVOICE,
        "all 50 bids must be recorded"
    );

    let err = client
        .try_place_bid(&investor, &invoice_id, &1_100i128, &1_200i128, &BytesN::from_array(&env, &[0u8; 32]))
        .unwrap_err()
        .expect("contract error");
    assert_eq!(
        err,
        QuickLendXError::MaxBidsPerInvoiceExceeded,
        "the 51st bid must be rejected with MaxBidsPerInvoiceExceeded"
    );

    assert_eq!(
        get_active_bid_count(&env, &contract_id, &invoice_id),
        MAX_BIDS_PER_INVOICE,
        "active bid count must remain at MAX_BIDS_PER_INVOICE after rejection"
    );
}

// ============================================================================
// Test 2: rank_bids full-chain ordering at the ceiling
// ============================================================================

#[test]
fn test_rank_bids_full_capacity_orders_by_documented_chain() {
    let (env, client, _admin, investor, invoice_id, _contract_id) = setup();

    let mut first_bid_id: Option<BytesN<32>> = None;
    let mut last_bid_id: Option<BytesN<32>> = None;
    for i in 0..MAX_BIDS_PER_INVOICE {
        let profit_units = (MAX_BIDS_PER_INVOICE - i) as i128;
        let bid_amount = 5_000i128;
        let expected_return = bid_amount + profit_units * 100;
        let bid_id =
            client.place_bid(&investor, &invoice_id, &bid_amount, &expected_return, &BytesN::from_array(&env, &[0u8; 32]));
        if i == 0 {
            first_bid_id = Some(bid_id.clone());
        }
        if i as u32 == MAX_BIDS_PER_INVOICE - 1 {
            last_bid_id = Some(bid_id);
        }
    }

    let ranked = client.get_ranked_bids(&invoice_id);
    assert_eq!(
        ranked.len() as u32,
        MAX_BIDS_PER_INVOICE,
        "rank_bids must return all 50 bids"
    );

    assert_eq!(
        ranked.get(0).unwrap().bid_id,
        first_bid_id.expect("first_bid_id"),
        "ranked[0] must be the highest-profit bid"
    );
    assert_eq!(
        ranked.get(MAX_BIDS_PER_INVOICE - 1).unwrap().bid_id,
        last_bid_id.expect("last_bid_id"),
        "ranked[49] must be the lowest-profit bid"
    );

    for i in 1..ranked.len() {
        let prev = ranked.get(i as u32 - 1).unwrap();
        let cur = ranked.get(i as u32).unwrap();
        assert!(
            BidStorage::compare_bids(&prev, &cur) != core::cmp::Ordering::Less,
            "chain ordering violated at index {}",
            i
        );
    }
}

// ============================================================================
// Test 3: get_best_bid == rank_bids[0] at full capacity
// ============================================================================

#[test]
fn test_get_best_bid_equals_rank_bids_head_at_full_capacity() {
    let (env, client, _admin, investor, invoice_id, _contract_id) = setup();

    for i in 0..MAX_BIDS_PER_INVOICE {
        let profit_units = (MAX_BIDS_PER_INVOICE - i) as i128;
        let bid_amount = 5_000i128;
        let expected_return = bid_amount + profit_units * 100;
        client.place_bid(&investor, &invoice_id, &bid_amount, &expected_return, &BytesN::from_array(&env, &[0u8; 32]));
    }

    let best = client
        .get_best_bid(&invoice_id)
        .expect("must have a best bid at the ceiling");
    let ranked = client.get_ranked_bids(&invoice_id);
    assert_eq!(ranked.len() as u32, MAX_BIDS_PER_INVOICE);
    assert_eq!(
        best.bid_id,
        ranked.get(0).unwrap().bid_id,
        "get_best_bid MUST equal rank_bids[0] at full capacity"
    );

    let second_bid_id = ranked.get(1).unwrap().bid_id.clone();
    assert_ne!(best.bid_id, second_bid_id);
    client.cancel_bid(&best.bid_id);

    let best2 = client
        .get_best_bid(&invoice_id)
        .expect("a new best exists after cancel");
    let ranked2 = client.get_ranked_bids(&invoice_id);
    assert_eq!(
        best2.bid_id, second_bid_id,
        "after cancelling the best, the next-best rises to the head"
    );
    assert_eq!(
        best2.bid_id,
        ranked2.get(0).unwrap().bid_id,
        "best == ranked[0] invariant holds post-cancel"
    );
}

// ============================================================================
// Test 4: Pure bid_id tiebreaker at full capacity
// ============================================================================

#[test]
fn test_full_capacity_pure_bid_id_tiebreaker() {
    let (env, client, _admin, investor, invoice_id, contract_id) = setup();

    for _ in 0..MAX_BIDS_PER_INVOICE {
        client.place_bid(&investor, &invoice_id, &5_000i128, &6_000i128, &BytesN::from_array(&env, &[0u8; 32]));
    }

    let records = get_bid_records_for_invoice(&env, &contract_id, &invoice_id);
    assert_eq!(records.len() as u32, MAX_BIDS_PER_INVOICE);
    let now = env.ledger().timestamp();
    for idx in 0..records.len() {
        let bid = records.get(idx as u32).unwrap();
        assert_eq!(
            bid.timestamp, now,
            "test invariant broken: timestamp differs at idx {} (now={}, bid_ts={})",
            idx, now, bid.timestamp
        );
    }

    let ranked = client.get_ranked_bids(&invoice_id);
    assert_eq!(
        ranked.len() as u32,
        MAX_BIDS_PER_INVOICE,
        "all 50 identical bids must be recorded"
    );

    for i in 1..ranked.len() {
        let prev = ranked.get(i as u32 - 1).unwrap();
        let cur = ranked.get(i as u32).unwrap();
        assert!(
            BidStorage::compare_bids(&prev, &cur) != core::cmp::Ordering::Less,
            "pure bid_id tiebreaker broken at index {}",
            i
        );
    }

    let best = client
        .get_best_bid(&invoice_id)
        .expect("must exist");
    assert_eq!(
        best.bid_id,
        ranked.get(0).unwrap().bid_id,
        "best == ranked[0] under pure tiebreaker"
    );
}

// ============================================================================
// Test 5: Full-coverage cleanup drains all 50 expired bids at full capacity
// ============================================================================

#[test]
fn test_full_coverage_cleanup_drains_all_expired_at_full_capacity() {
    let (env, client, _admin, investor, invoice_id, contract_id) = setup();
    client.set_bid_ttl_days(&1u64);

    let mut placed: Vec<BytesN<32>> = Vec::new(&env);
    for _ in 0..MAX_BIDS_PER_INVOICE {
        let bid_id = client.place_bid(&investor, &invoice_id, &5_000i128, &6_000i128, &BytesN::from_array(&env, &[0u8; 32]));
        placed.push_back(bid_id);
    }

    env.ledger().set_timestamp(env.ledger().timestamp() + 2 * SECONDS_PER_DAY);

    let (cleaned1, _) =
        client.cleanup_expired_bids_paged(&invoice_id, &0u32, &25u32);
    let (cleaned2, _) =
        client.cleanup_expired_bids_paged(&invoice_id, &25u32, &25u32);
    assert_eq!(
        cleaned1 + cleaned2, MAX_BIDS_PER_INVOICE,
        "first two pages must clean all 50 expired bids"
    );

    let (cleaned_full, remaining) =
        client.cleanup_expired_bids_paged(&invoice_id, &0u32, &MAX_BIDS_PER_INVOICE);
    assert_eq!(
        cleaned_full, MAX_BIDS_PER_INVOICE,
        "full coverage sweep settles all 50 expired bids"
    );
    assert_eq!(
        remaining, 0,
        "full coverage must leave 0 remaining bids in index"
    );

    let (cleaned_again, remaining_again) =
        client.cleanup_expired_bids_paged(&invoice_id, &0u32, &MAX_BIDS_PER_INVOICE);
    assert_eq!(
        cleaned_again, 0,
        "second pass must clean 0 (idempotent)"
    );
    assert_eq!(
        remaining_again, 0,
        "second pass must leave 0 in the index"
    );

    let (placed_count, accepted, withdrawn, expired, cancelled) =
        count_bids_by_status(&env, &contract_id, &invoice_id);
    assert_eq!(placed_count, 0, "no Placed bids in index");
    assert_eq!(accepted, 0, "no Accepted bids in index");
    assert_eq!(withdrawn, 0, "no Withdrawn bids in index");
    assert_eq!(expired, 0, "no Expired bids in index (compacted)");
    assert_eq!(cancelled, 0, "no Cancelled bids in index");

    assert!(client.get_best_bid(&invoice_id).is_none());
    assert_eq!(
        client.get_ranked_bids(&invoice_id).len(),
        0,
        "ranking must be empty after full cleanup"
    );

    for idx in 0..placed.len() as usize {
        let bid_id = placed.get(idx as u32).unwrap();
        let bid = get_bid(&env, &contract_id, &bid_id)
            .expect("Bid struct must remain in storage after cleanup");
        assert_eq!(
            bid.status,
            BidStatus::Expired,
            "underlying Bid struct at idx {} must carry Expired status",
            idx
        );
    }
}

// ============================================================================
// Test 6: Paged cleanup with mixed expired / active across pages
// ============================================================================

#[test]
fn test_paged_cleanup_mixed_expired_and_active_full_capacity() {
    let (env, client, _admin, investor, invoice_id, contract_id) = setup();
    client.set_bid_ttl_days(&1u64);

    let mut placed: Vec<BytesN<32>> = Vec::new(&env);
    for _ in 0..MAX_BIDS_PER_INVOICE {
        let bid_id = client.place_bid(&investor, &invoice_id, &5_000i128, &6_000i128, &BytesN::from_array(&env, &[0u8; 32]));
        placed.push_back(bid_id);
    }

    let now_ts = env.ledger().timestamp();
    for i in 0..25u32 {
        let bid_id = placed.get(i).unwrap();
        let mut bid = get_bid(&env, &contract_id, &bid_id).expect("bid exists");
        bid.expiration_timestamp = now_ts + 1;
        update_bid(&env, &contract_id, &bid);
    }
    env.ledger().set_timestamp(now_ts + 10);

    let chunk = 10u32;
    let mut total_cleaned = 0u32;
    let mut offset = 0u32;
    let max_iterations = MAX_BIDS_PER_INVOICE;
    let mut iterations = 0u32;
    while offset < MAX_BIDS_PER_INVOICE {
        iterations += 1;
        assert!(
            iterations <= max_iterations,
            "paged cleanup must terminate within a bounded number of iterations"
        );
        let (cleaned_this_page, _reported_rem) =
            client.cleanup_expired_bids_paged(&invoice_id, &offset, &chunk);
        let expected_expired_in_slice = if offset >= 25 {
            0u32
        } else {
            (25u32 - offset).min(chunk)
        };
        assert_eq!(
            cleaned_this_page, expected_expired_in_slice,
            "page at offset {} must clean exactly {} expired bids",
            offset, expected_expired_in_slice
        );
        total_cleaned += cleaned_this_page;
        offset += chunk;
    }
    assert_eq!(
        total_cleaned, 25,
        "sum of page-by-page cleanups must equal exactly 25"
    );

    let (cleaned_full, remaining_full) =
        client.cleanup_expired_bids_paged(&invoice_id, &0u32, &MAX_BIDS_PER_INVOICE);
    assert_eq!(
        cleaned_full, 25,
        "full coverage after chunked sweep settles the 25 expired positions"
    );
    assert_eq!(
        remaining_full,
        MAX_BIDS_PER_INVOICE - 25,
        "index must hold exactly the 25 surviving Placed bids"
    );

    let (cleaned_steady, remaining_steady) =
        client.cleanup_expired_bids_paged(&invoice_id, &0u32, &MAX_BIDS_PER_INVOICE);
    assert_eq!(
        cleaned_steady, 0,
        "second full-coverage pass must clean 0 (steady state)"
    );
    assert_eq!(
        remaining_steady,
        MAX_BIDS_PER_INVOICE - 25,
        "steady state preserves the surviving 25 Placed bids"
    );

    let (placed_in_index, accepted, withdrawn, expired_in_index, cancelled) =
        count_bids_by_status(&env, &contract_id, &invoice_id);
    assert_eq!(
        placed_in_index, MAX_BIDS_PER_INVOICE - 25,
        "25 Placed bids must survive in the index"
    );
    assert_eq!(accepted, 0, "no Accepted bids in index");
    assert_eq!(withdrawn, 0, "no Withdrawn bids in index");
    assert_eq!(
        expired_in_index, 0,
        "Expired bids are removed from the index by the compaction"
    );
    assert_eq!(cancelled, 0, "no Cancelled bids in index");

    let ranked = client.get_ranked_bids(&invoice_id);
    assert_eq!(
        ranked.len() as u32,
        MAX_BIDS_PER_INVOICE - 25,
        "post-cleanup ranking must contain only surviving Placed bids"
    );
    let best = client
        .get_best_bid(&invoice_id)
        .expect("surviving Placed bids exist");
    assert_eq!(
        best.bid_id,
        ranked.get(0).unwrap().bid_id,
        "best == ranked[0] invariant holds for surviving set"
    );

    let mut expiring_ids_iter: usize = 0;
    while expiring_ids_iter < 25 {
        let bid_id = placed
            .get(expiring_ids_iter as u32)
            .unwrap();
        let bid = get_bid(&env, &contract_id, &bid_id).expect("Bid struct present");
        assert_eq!(
            bid.status,
            BidStatus::Expired,
            "Bid struct for originally-expired idx {} must be Expired",
            expiring_ids_iter
        );
        expiring_ids_iter += 1;
    }
    let mut surviving_ids_iter: usize = 25;
    while surviving_ids_iter < MAX_BIDS_PER_INVOICE as usize {
        let bid_id = placed
            .get(surviving_ids_iter as u32)
            .unwrap();
        let bid = get_bid(&env, &contract_id, &bid_id).expect("Bid struct present");
        assert_eq!(
            bid.status,
            BidStatus::Placed,
            "Bid struct for surviving idx {} must remain Placed",
            surviving_ids_iter
        );
        surviving_ids_iter += 1;
    }
}
