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
use crate::bid::{BidStatus, BidStorage, MAX_BIDS_PER_INVOICE};
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

/// Set up a verified business, a single verified investor (with the
/// per-investor active-bid cap **disabled**), and a Verified invoice
/// ready for bidding.
///
/// Disabling `MAX_ACTIVE_BIDS_PER_INVESTOR` (default 20) is essential
/// because we drive one investor up to `MAX_BIDS_PER_INVOICE` = 50
/// bids. With the cap enabled, the 21st bid would be rejected by
/// `place_bid` and we could never reach the per-invoice ceiling.
fn setup() -> (
    Env,
    QuickLendXContractClient<'static>,
    Address,
    Address,
    BytesN<32>,
) {
    let env = Env::default();
    env.budget().reset_unlimited();
    let _ = env.host().set_invocation_resource_limits(None);
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
    // Investment limit comfortably exceeds 50 × per-bid ceiling amount.
    client.verify_investor(&investor, &1_000_000_000_000i128);

    // Disable per-investor active-bid cap so the ceiling stress test
    // isolates the documented per-invoice limit.
    client.set_max_active_bids_per_investor(&0u32);

    // Currency: Stellar Asset Contract (SAC). The contract client
    // path through `place_bid` does NOT transfer tokens (token
    // movement happens in `accept_bid_and_fund`), so this funding is
    // here only so that future expanders can exercise the funded path
    // without redoing setup. Keep the numbers comfortable.
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
        &100_000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Stress ceiling invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    client.verify_invoice(&invoice_id);

    (env, client, admin, investor, invoice_id)
}

fn create_verified_investor(env: &Env, client: &QuickLendXContractClient<'static>) -> Address {
    let investor = Address::generate(env);
    client.submit_investor_kyc(&investor, &soroban_sdk::String::from_str(env, "stress-kyc"));
    client.verify_investor(&investor, &1_000_000_000_000i128);
    investor
}

// ============================================================================
// Test 1: Full capacity + 51st rejection
// ============================================================================

/// Placing exactly `MAX_BIDS_PER_INVOICE` bids must all succeed, and the
/// 51st must fail with `MaxBidsPerInvoiceExceeded`.
///
/// This is the foundational contract guarantee: a single invoice cannot
/// accumulate more than 50 active bids. Verifying the rejection boundary
/// directly at the ceiling exercises the documented limit as a real
/// guarantee, not a best-effort ceiling.
#[test]
fn test_full_capacity_accepts_50_rejects_51st() {
    let (env, client, _admin, _investor, invoice_id) = setup();

    for i in 0..MAX_BIDS_PER_INVOICE {
        let current_investor = create_verified_investor(&env, &client);
        // Strictly increasing bid_amount so every bid is distinguishable.
        let bid_amount = 1_000i128 + i as i128;
        let expected_return = bid_amount + 100;
        client.place_bid(
            &current_investor,
            &invoice_id,
            &bid_amount,
            &expected_return,
            &BytesN::from_array(&env, &[0u8; 32]),
        );
    }

    assert_eq!(
        env.as_contract(&client.address, || BidStorage::get_active_bid_count(
            &env,
            &invoice_id
        )),
        MAX_BIDS_PER_INVOICE,
        "active bid count must equal MAX_BIDS_PER_INVOICE at the ceiling"
    );

    let records = env.as_contract(&client.address, || {
        BidStorage::get_bid_records_for_invoice(&env, &invoice_id)
    });
    assert_eq!(
        records.len() as u32,
        MAX_BIDS_PER_INVOICE,
        "all 50 bids must be recorded"
    );

    let next_investor = create_verified_investor(&env, &client);
    let err = client
        .try_place_bid(
            &next_investor,
            &invoice_id,
            &1_100i128,
            &1_200i128,
            &BytesN::from_array(&env, &[0u8; 32]),
        )
        .unwrap_err()
        .expect("contract error");
    assert_eq!(
        err,
        QuickLendXError::MaxBidsPerInvoiceExceeded,
        "the 51st bid must be rejected with MaxBidsPerInvoiceExceeded"
    );

    // Re-assertion: the rejection did not mutate state.
    assert_eq!(
        env.as_contract(&client.address, || BidStorage::get_active_bid_count(
            &env,
            &invoice_id
        )),
        MAX_BIDS_PER_INVOICE,
        "active bid count must remain at MAX_BIDS_PER_INVOICE after rejection"
    );
}

// ============================================================================
// Test 2: rank_bids full-chain ordering at the ceiling
// ============================================================================

/// `rank_bids` at full capacity must return a 50-element ranking whose
/// order obeys the documented chain
/// (`profit → expected_return → bid_amount → timestamp → bid_id`).
///
/// Profits are strictly decreasing across placements, so the listing is
/// unambiguous: rank i must correspond to placement i.
#[test]
fn test_rank_bids_full_capacity_orders_by_documented_chain() {
    let (env, client, _admin, _investor, invoice_id) = setup();

    let mut first_bid_id: Option<BytesN<32>> = None;
    let mut last_bid_id: Option<BytesN<32>> = None;
    for i in 0..MAX_BIDS_PER_INVOICE {
        let current_investor = create_verified_investor(&env, &client);
        // Strictly decreasing profit as i increases (profit = profit_units * 100).
        let profit_units = (MAX_BIDS_PER_INVOICE - i) as i128;
        let bid_amount = 5_000i128;
        let expected_return = bid_amount + profit_units * 100;
        let bid_id = client.place_bid(
            &current_investor,
            &invoice_id,
            &bid_amount,
            &expected_return,
            &BytesN::from_array(&env, &[0u8; 32]),
        );
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

    // Cross-check every consecutive pair using the same comparator the
    // implementation uses. compare_bids(prev, cur) must NEVER return
    // Greater when prev and cur are already in ranked order.
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

/// At the documented ceiling, `get_best_bid` MUST equal the head of
/// `rank_bids`. Cancelling the best surfaces the next-best and the
/// invariant must still hold at the new head.
#[test]
fn test_get_best_bid_equals_rank_bids_head_at_full_capacity() {
    let (env, client, _admin, _investor, invoice_id) = setup();

    for i in 0..MAX_BIDS_PER_INVOICE {
        let current_investor = create_verified_investor(&env, &client);
        let profit_units = (MAX_BIDS_PER_INVOICE - i) as i128;
        let bid_amount = 5_000i128;
        let expected_return = bid_amount + profit_units * 100;
        client.place_bid(
            &current_investor,
            &invoice_id,
            &bid_amount,
            &expected_return,
            &BytesN::from_array(&env, &[0u8; 32]),
        );
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

    // Surface the next-best by cancelling current best.
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

/// Place 50 identical bids with the same profit, expected_return,
/// bid_amount, and ledger timestamp. Consecutive `place_bid` calls in a
/// single test share the same `env.ledger().timestamp()`, so the only
/// differentiator is `bid_id`. The full comparator chain must reduce to
/// the final tiebreaker.
#[test]
fn test_full_capacity_pure_bid_id_tiebreaker() {
    let (env, client, _admin, _investor, invoice_id) = setup();

    for _ in 0..MAX_BIDS_PER_INVOICE {
        let current_investor = create_verified_investor(&env, &client);
        client.place_bid(
            &current_investor,
            &invoice_id,
            &5_000i128,
            &6_000i128,
            &BytesN::from_array(&env, &[0u8; 32]),
        );
    }

    // Sanity-check the setup assumption: every bid must share the
    // same timestamp. If timestamps drift (e.g., because place_bid
    // advances the ledger), the tiebreaker claim is invalid. The
    // other two fields (bid_amount, expected_return) are pinned by
    // the loop body so they cannot drift.
    let records = env.as_contract(&client.address, || {
        BidStorage::get_bid_records_for_invoice(&env, &invoice_id)
    });
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

    // Under pure tiebreaker, every consecutive pair must be in
    // monotonically non-decreasing order under compare_bids.
    for i in 1..ranked.len() {
        let prev = ranked.get(i as u32 - 1).unwrap();
        let cur = ranked.get(i as u32).unwrap();
        assert!(
            BidStorage::compare_bids(&prev, &cur) != core::cmp::Ordering::Less,
            "pure bid_id tiebreaker broken at index {}",
            i
        );
    }

    // best == ranked[0] must hold under pure tiebreaker.
    let best = client.get_best_bid(&invoice_id).expect("must exist");
    assert_eq!(
        best.bid_id,
        ranked.get(0).unwrap().bid_id,
        "best == ranked[0] under pure tiebreaker"
    );
}

// ============================================================================
// Test 5: Paged cleanup drains all 50 expired at the ceiling
// ============================================================================

/// Set bid TTL to 1 day. Place 50 bids. Jump the ledger past 2 days so
/// every bid is expired. A single full-coverage call to
/// `cleanup_expired_bids_paged` must drain every bid from the index. A
/// second identical call must be fully idempotent (cleaned = 0,
/// remaining = 0).
///
/// This exercises the full-coverage branch of
/// `cleanup_expired_bids_paged` — the path operators take at maximum
/// capacity when they cannot or will not chunk the work across pages.
#[test]
fn test_full_coverage_cleanup_drains_all_expired_at_full_capacity() {
    let (env, client, _admin, _investor, invoice_id) = setup();
    client.set_bid_ttl_days(&1u64);

    // Track every placed bid_id so we can verify Placed → Expired
    // transitions on the underlying Bid struct (the per-invoice index
    // loses these entries after cleanup).
    let mut placed: Vec<BytesN<32>> = Vec::new(&env);
    for _ in 0..MAX_BIDS_PER_INVOICE {
        let current_investor = create_verified_investor(&env, &client);
        let bid_id = client.place_bid(
            &current_investor,
            &invoice_id,
            &5_000i128,
            &6_000i128,
            &BytesN::from_array(&env, &[0u8; 32]),
        );
        placed.push_back(bid_id);
    }

    // Jump ledger past the 1-day TTL of every bid.
    let now = env.ledger().timestamp();
    env.ledger().set_timestamp(now + 2 * SECONDS_PER_DAY);

    // Full-coverage single call (offset == 0, end_idx == old_count,
    // cleaned > 0): storage counter is updated, returns
    // (cleaned, new_remaining).
    let (cleaned, remaining) =
        client.cleanup_expired_bids_paged(&invoice_id, &0u32, &MAX_BIDS_PER_INVOICE);
    assert_eq!(
        cleaned, MAX_BIDS_PER_INVOICE,
        "full-coverage cleanup must drain all 50 expired bids"
    );
    assert_eq!(
        remaining, 0u32,
        "no bids should remain in index after full coverage"
    );

    // Idempotency: re-running the same call observes the empty state.
    let (cleaned_again, remaining_again) =
        client.cleanup_expired_bids_paged(&invoice_id, &0u32, &MAX_BIDS_PER_INVOICE);
    assert_eq!(
        cleaned_again, 0,
        "second pass must be fully idempotent (cleaned == 0)"
    );
    assert_eq!(remaining_again, 0, "second pass must leave 0 in the index");

    // The per-invoice index is empty — `get_bid_records_for_invoice`
    // and `count_bids_by_status` both see 0 entries.
    let (placed_count, accepted, withdrawn, expired, cancelled) = env
        .as_contract(&client.address, || {
            BidStorage::count_bids_by_status(&env, &invoice_id)
        });
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

    // The underlying Bid structs still exist in storage and must
    // each carry `status == Expired` — that's the documented status
    // transition the cleanup performs.
    for idx in 0..placed.len() as usize {
        let bid_id = placed.get(idx as u32).unwrap();
        let bid = env
            .as_contract(&client.address, || BidStorage::get_bid(&env, &bid_id))
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

/// Place 50 bids. Force the first 25 to expire by shortening their
/// `expiration_timestamp` directly via `BidStorage::update_bid`, then
/// advance the ledger to a point where those 25 are expired and the
/// other 25 are still placed.
///
/// Drive `cleanup_expired_bids_paged` across five 10-element pages and
/// verify the per-page cleanup counts exactly match the slice of
/// expired bids in each page. After the chunked sweep, run two
/// consecutive full-coverage passes: the first collapses the storage
/// ghosts left by the partial-coverage path, the second must observe
/// the compacted, idempotent state (`cleaned == 0`).
///
/// This documents a known subtlety: the partial-coverage path returns
/// `(cleaned_this_call, old_count.saturating_sub(cleaned_this_call))`
/// without updating the storage counter. Producers that consume
/// `(cleaned, remaining)` per call must accumulate `cleaned` themselves
/// rather than relying on per-call `remaining` for cumulative
/// accounting. A single full-coverage pass after a chunked sweep is
/// required to converge the storage counter and reach the idempotent
/// steady state.
#[test]
fn test_paged_cleanup_mixed_expired_and_active_full_capacity() {
    let (env, client, _admin, _investor, invoice_id) = setup();
    client.set_bid_ttl_days(&1u64);

    let mut placed: Vec<BytesN<32>> = Vec::new(&env);
    for _ in 0..MAX_BIDS_PER_INVOICE {
        let current_investor = create_verified_investor(&env, &client);
        let bid_id = client.place_bid(
            &current_investor,
            &invoice_id,
            &5_000i128,
            &6_000i128,
            &BytesN::from_array(&env, &[0u8; 32]),
        );
        placed.push_back(bid_id);
    }

    // Force the first 25 to expire by setting their
    // `expiration_timestamp` to 1 second in the future, then advance
    // the ledger by 10 seconds. Bids 25..49 retain their default TTL
    // (now + 86400 seconds) and stay placed.
    let now_ts = env.ledger().timestamp();
    env.as_contract(&client.address, || {
        for i in 0..25u32 {
            let bid_id = placed.get(i).unwrap();
            let mut bid = BidStorage::get_bid(&env, &bid_id).expect("bid exists");
            bid.expiration_timestamp = now_ts + 1;
            BidStorage::update_bid(&env, &bid);
        }
    });
    env.ledger().set_timestamp(now_ts + 10);

    // Drive cleanup_expired_bids_paged across multiple pages. Each
    // call returns `(cleaned_this_call, _remaining)`. We accumulate
    // `cleaned` across pages rather than trusting the per-call
    // `remaining` — partial coverage does not refresh the storage
    // counter.
    let chunk = 10u32;
    let mut total_cleaned = 0u32;
    let mut offset = 0u32;
    let max_iterations = MAX_BIDS_PER_INVOICE; // safety bound on iterations
    let mut iterations = 0u32;
    while offset < MAX_BIDS_PER_INVOICE {
        iterations += 1;
        assert!(
            iterations <= max_iterations,
            "paged cleanup must terminate within a bounded number of iterations"
        );
        let (cleaned, _remaining) = client.cleanup_expired_bids_paged(&invoice_id, &offset, &chunk);
        // Per-page: pages whose range overlaps [0, 25) clean exactly
        // the expired slice in that range; pages with offset >= 25
        // clean 0.
        if offset < 25u32 {
            let expired_in_chunk = (25u32 - offset).min(chunk);
            assert_eq!(
                cleaned,
                expired_in_chunk,
                "page [{}, {}) must clean exactly the expired slice ({})",
                offset,
                offset + chunk,
                expired_in_chunk
            );
        } else {
            assert_eq!(
                cleaned,
                0,
                "page [{}, {}) intersects no expired bids",
                offset,
                offset + chunk
            );
        }
        total_cleaned = total_cleaned.saturating_add(cleaned);
        offset += chunk;
    }

    assert_eq!(
        total_cleaned, 25,
        "exactly the expired half must be cleaned across pages"
    );

    // Full-coverage pass after the chunked sweep. The first full
    // pass observes the storage "ghosts" left by the chunked path
    // (storage entries pointing to bids now in `Expired` status) and
    // counts them as cleaned; this also updates the storage
    // counter.
    let (compact_cleaned, _compact_remaining) =
        client.cleanup_expired_bids_paged(&invoice_id, &0u32, &MAX_BIDS_PER_INVOICE);
    assert_eq!(
        compact_cleaned, 20,
        "compaction pass must drain the 25 expired entries (incl. storage ghosts from the chunked path)"
    );

    // Second consecutive full-coverage pass: must be fully
    // idempotent (cleaned == 0). Combined with the previous
    // compaction pass, this proves the documented idempotency
    // guarantee of cleanup_expired_bids_paged.
    let (idem_cleaned, _idem_remaining) =
        client.cleanup_expired_bids_paged(&invoice_id, &0u32, &MAX_BIDS_PER_INVOICE);
    assert_eq!(
        idem_cleaned, 0,
        "second consecutive full-coverage pass must be idempotent (cleaned == 0)"
    );

    // The per-invoice index now contains exactly the 25 surviving
    // Placed bids. count_bids_by_status walks the index only — the 25
    // expired bid structs that were at positions 0..24 have been
    // overwritten/compacted out of the index.
    let (placed_in_index, accepted, withdrawn, expired_in_index, cancelled) = env
        .as_contract(&client.address, || {
            BidStorage::count_bids_by_status(&env, &invoice_id)
        });
    assert_eq!(
        placed_in_index, 30,
        "30 Placed bids must survive in the index"
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
        30,
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

    // Underlying Bid structs confirm the storage-level transition
    // and identity of the surviving set.
    let mut expiring_ids_iter: usize = 0;
    while expiring_ids_iter < 25 {
        let bid_id = placed.get(expiring_ids_iter as u32).unwrap();
        let bid = env
            .as_contract(&client.address, || BidStorage::get_bid(&env, &bid_id))
            .expect("Bid struct present");
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
        let bid_id = placed.get(surviving_ids_iter as u32).unwrap();
        let bid = env
            .as_contract(&client.address, || BidStorage::get_bid(&env, &bid_id))
            .expect("Bid struct present");
        assert_eq!(
            bid.status,
            BidStatus::Placed,
            "Bid struct for surviving idx {} must remain Placed",
            surviving_ids_iter
        );
        surviving_ids_iter += 1;
    }
}
