//! Deterministic bid ranking tests.

#[cfg(test)]
mod test_bid_ranking {
use crate::bid::{Bid, BidStatus, BidStorage};
use soroban_sdk::{testutils::{Address as _, Ledger}, Address, BytesN, Env};

fn invoice_id(env: &Env, seed: u8) -> BytesN<32> {
    let mut bytes = [0u8; 32];
    bytes[0] = seed;
    BytesN::from_array(env, &bytes)
}

fn build_bid(
    env: &Env,
    invoice_id: &BytesN<32>,
    investor: &Address,
    bid_amount: i128,
    expected_return: i128,
    timestamp: u64,
    status: BidStatus,
    id_suffix: u8,
) -> Bid {
    let mut bid_id_bytes = [0u8; 32];
    bid_id_bytes[0] = 0xB1;
    bid_id_bytes[1] = 0xD0;
    bid_id_bytes[2..10].copy_from_slice(&timestamp.to_be_bytes());
    bid_id_bytes[30] = id_suffix;
    bid_id_bytes[31] = id_suffix;

    Bid {
        bid_id: BytesN::from_array(env, &bid_id_bytes),
        invoice_id: invoice_id.clone(),
        investor: investor.clone(),
        bid_amount,
        expected_return,
        timestamp,
        status,
        expiration_timestamp: timestamp.saturating_add(604800),
    }
}

fn persist_bid(env: &Env, bid: &Bid) {
    BidStorage::store_bid(env, bid);
    BidStorage::add_bid_to_invoice(env, &bid.invoice_id, &bid.bid_id);
}

fn assert_best_matches_first_ranked(env: &Env, invoice: &BytesN<32>) {
    let ranked = BidStorage::rank_bids(env, invoice);
    let best = BidStorage::get_best_bid(env, invoice);

    if ranked.len() == 0 {
        assert!(best.is_none());
        return;
    }

    let best_bid = best.expect("best bid must exist when ranking is non-empty");
    assert_eq!(best_bid.bid_id, ranked.get(0).unwrap().bid_id);
}

#[test]
fn rank_bids_orders_by_profit_and_expected_return() {
    let env = Env::default();
    env.ledger().with_mut(|li| li.timestamp = 1_000);
    let invoice = invoice_id(&env, 1);

    let investor1 = Address::generate(&env);
    let investor2 = Address::generate(&env);
    let investor3 = Address::generate(&env);

    // Highest profit
    let bid_top = build_bid(
        &env,
        &invoice,
        &investor1,
        5_200,
        6_800, // profit 1_600
        50,
        BidStatus::Placed,
        1,
    );
    // Same profit as bid_mid but higher expected_return
    let bid_mid = build_bid(
        &env,
        &invoice,
        &investor2,
        5_500,
        7_000, // profit 1_500, higher expected_return
        60,
        BidStatus::Placed,
        2,
    );
    // Same profit as bid_mid, lower expected_return
    let bid_low = build_bid(
        &env,
        &invoice,
        &investor3,
        5_000,
        6_500, // profit 1_500, lower expected_return
        70,
        BidStatus::Placed,
        3,
    );
    persist_bid(&env, &bid_top);
    persist_bid(&env, &bid_mid);
    persist_bid(&env, &bid_low);

    let ranked = BidStorage::rank_bids(&env, &invoice);
    assert_eq!(ranked.len(), 3);
    assert_eq!(ranked.get(0).unwrap().bid_id, bid_top.bid_id);
    assert_eq!(ranked.get(1).unwrap().bid_id, bid_mid.bid_id);
    assert_eq!(ranked.get(2).unwrap().bid_id, bid_low.bid_id);
}

#[test]
fn rank_bids_prefers_newer_timestamp_on_full_tie() {
    let env = Env::default();
    env.ledger().with_mut(|li| li.timestamp = 2_000);
    let invoice = invoice_id(&env, 2);

    let investor1 = Address::generate(&env);
    let investor2 = Address::generate(&env);

    // Identical economics, newer timestamp should win
    let older = build_bid(
        &env,
        &invoice,
        &investor1,
        5_000,
        6_000,
        10,
        BidStatus::Placed,
        1,
    );
    let newer = build_bid(
        &env,
        &invoice,
        &investor2,
        5_000,
        6_000,
        20,
        BidStatus::Placed,
        2,
    );
    persist_bid(&env, &older);
    persist_bid(&env, &newer);

    let ranked = BidStorage::rank_bids(&env, &invoice);
    assert_eq!(ranked.len(), 2);
    assert_eq!(ranked.get(0).unwrap().timestamp, 20);
    assert_eq!(ranked.get(1).unwrap().timestamp, 10);
}

#[test]
fn rank_bids_uses_bid_id_as_final_tiebreaker() {
    let env = Env::default();
    env.ledger().with_mut(|li| li.timestamp = 3_000);
    let invoice = invoice_id(&env, 3);
    let investor = Address::generate(&env);

    // Perfect tie on value and timestamp; bid_id must decide
    let lower_id = build_bid(
        &env,
        &invoice,
        &investor,
        4_000,
        5_000,
        99,
        BidStatus::Placed,
        1,
    );
    let higher_id = build_bid(
        &env,
        &invoice,
        &investor,
        4_000,
        5_000,
        99,
        BidStatus::Placed,
        9,
    );
    persist_bid(&env, &lower_id);
    persist_bid(&env, &higher_id);

    let ranked = BidStorage::rank_bids(&env, &invoice);
    assert_eq!(ranked.len(), 2);
    assert_eq!(ranked.get(0).unwrap().bid_id, higher_id.bid_id);
    assert_eq!(ranked.get(1).unwrap().bid_id, lower_id.bid_id);
}

#[test]
fn get_best_bid_aligns_with_ranking_and_filters_non_placed() {
    let env = Env::default();
    env.ledger().with_mut(|li| li.timestamp = 4_000);
    let invoice = invoice_id(&env, 4);

    let investor1 = Address::generate(&env);
    let investor2 = Address::generate(&env);

    let placed = build_bid(
        &env,
        &invoice,
        &investor1,
        6_000,
        7_500, // profit 1_500
        10,
        BidStatus::Placed,
        1,
    );
    let cancelled = build_bid(
        &env,
        &invoice,
        &investor2,
        7_000,
        8_500, // profit 1_500 but status cancelled
        20,
        BidStatus::Cancelled,
        2,
    );
    persist_bid(&env, &placed);
    persist_bid(&env, &cancelled);

    let ranked = BidStorage::rank_bids(&env, &invoice);
    assert_eq!(ranked.len(), 1);
    assert_eq!(ranked.get(0).unwrap().bid_id, placed.bid_id);

    let best = BidStorage::get_best_bid(&env, &invoice).unwrap();
    assert_eq!(best.bid_id, placed.bid_id);
}

#[test]
fn best_bid_matches_first_ranked_on_expected_return_tie() {
    let env = Env::default();
    env.ledger().with_mut(|li| li.timestamp = 5_000);
    let invoice = invoice_id(&env, 5);

    let investor1 = Address::generate(&env);
    let investor2 = Address::generate(&env);

    // Equal profit (1000), different expected_return.
    let lower_expected = build_bid(
        &env,
        &invoice,
        &investor1,
        5_000,
        6_000,
        10,
        BidStatus::Placed,
        1,
    );
    let higher_expected = build_bid(
        &env,
        &invoice,
        &investor2,
        6_000,
        7_000,
        20,
        BidStatus::Placed,
        2,
    );

    persist_bid(&env, &lower_expected);
    persist_bid(&env, &higher_expected);

    let ranked = BidStorage::rank_bids(&env, &invoice);
    assert_eq!(ranked.get(0).unwrap().bid_id, higher_expected.bid_id);
    assert_best_matches_first_ranked(&env, &invoice);
}

#[test]
fn best_bid_matches_first_ranked_on_bid_amount_tie_breaker() {
    let env = Env::default();
    env.ledger().with_mut(|li| li.timestamp = 6_000);
    let invoice = invoice_id(&env, 6);

    let investor1 = Address::generate(&env);
    let investor2 = Address::generate(&env);

    // Equal profit and expected_return, different bid_amount.
    let lower_amount = build_bid(
        &env,
        &invoice,
        &investor1,
        4_000,
        6_000,
        10,
        BidStatus::Placed,
        1,
    );
    let higher_amount = build_bid(
        &env,
        &invoice,
        &investor2,
        5_000,
        6_000,
        20,
        BidStatus::Placed,
        2,
    );

    persist_bid(&env, &lower_amount);
    persist_bid(&env, &higher_amount);

    let ranked = BidStorage::rank_bids(&env, &invoice);
    assert_eq!(ranked.get(0).unwrap().bid_id, higher_amount.bid_id);
    assert_best_matches_first_ranked(&env, &invoice);
}

#[test]
fn best_bid_matches_first_ranked_on_timestamp_tie_breaker() {
    let env = Env::default();
    env.ledger().with_mut(|li| li.timestamp = 7_000);
    let invoice = invoice_id(&env, 7);

    let investor1 = Address::generate(&env);
    let investor2 = Address::generate(&env);

    // Equal economics and bid amount, timestamp decides.
    let older = build_bid(
        &env,
        &invoice,
        &investor1,
        5_000,
        6_000,
        10,
        BidStatus::Placed,
        1,
    );
    let newer = build_bid(
        &env,
        &invoice,
        &investor2,
        5_000,
        6_000,
        20,
        BidStatus::Placed,
        2,
    );

    persist_bid(&env, &older);
    persist_bid(&env, &newer);

    let ranked = BidStorage::rank_bids(&env, &invoice);
    assert_eq!(ranked.get(0).unwrap().bid_id, newer.bid_id);
    assert_best_matches_first_ranked(&env, &invoice);
}

#[test]
fn best_bid_matches_first_ranked_on_bid_id_final_tie_breaker() {
    let env = Env::default();
    env.ledger().with_mut(|li| li.timestamp = 8_000);
    let invoice = invoice_id(&env, 8);

    let investor = Address::generate(&env);

    // Full tie except bid_id.
    let lower_id = build_bid(
        &env,
        &invoice,
        &investor,
        5_000,
        6_000,
        15,
        BidStatus::Placed,
        1,
    );
    let higher_id = build_bid(
        &env,
        &invoice,
        &investor,
        5_000,
        6_000,
        15,
        BidStatus::Placed,
        9,
    );

    persist_bid(&env, &lower_id);
    persist_bid(&env, &higher_id);

    let ranked = BidStorage::rank_bids(&env, &invoice);
    assert_eq!(ranked.get(0).unwrap().bid_id, higher_id.bid_id);
    assert_best_matches_first_ranked(&env, &invoice);
}

#[test]
fn best_bid_matches_first_ranked_independent_of_insertion_order_on_ties() {
    // Dataset: equal profit and expected_return, timestamp/bid_id ties decide.
    for &order in &[0u8, 1u8] {
        let env = Env::default();
        env.ledger().with_mut(|li| li.timestamp = 9_000);
        let invoice = invoice_id(&env, 9u8.saturating_add(order));

        let investor1 = Address::generate(&env);
        let investor2 = Address::generate(&env);
        let investor3 = Address::generate(&env);

        let bid_a = build_bid(
            &env,
            &invoice,
            &investor1,
            5_000,
            6_000,
            10,
            BidStatus::Placed,
            1,
        );
        let bid_b = build_bid(
            &env,
            &invoice,
            &investor2,
            5_000,
            6_000,
            20,
            BidStatus::Placed,
            2,
        );
        let bid_c = build_bid(
            &env,
            &invoice,
            &investor3,
            5_000,
            6_000,
            20,
            BidStatus::Placed,
            9,
        );

        if order == 0 {
            persist_bid(&env, &bid_a);
            persist_bid(&env, &bid_b);
            persist_bid(&env, &bid_c);
        } else {
            persist_bid(&env, &bid_c);
            persist_bid(&env, &bid_b);
            persist_bid(&env, &bid_a);
        }

        let ranked = BidStorage::rank_bids(&env, &invoice);
        assert_eq!(ranked.get(0).unwrap().bid_id, bid_c.bid_id);
        assert_best_matches_first_ranked(&env, &invoice);
    }
}

// ─── Expiration and Cleanup Tests ───────────────────────────────────────────────

/// Best bid remains correct after the highest bid expires.
#[test]
fn best_bid_matches_ranked_after_expiration() {
    let env = Env::default();
    env.ledger().with_mut(|li| li.timestamp = 1_000);
    let invoice = invoice_id(&env, 20);

    let investor1 = Address::generate(&env);
    let investor2 = Address::generate(&env);
    let investor3 = Address::generate(&env);

    // High profit bid (will expire)
    let bid_high = build_bid(
        &env,
        &invoice,
        &investor1,
        5_000,
        7_000, // profit 2_000
        10,
        BidStatus::Placed,
        1,
    );
    // Mid profit bid
    let bid_mid = build_bid(
        &env,
        &invoice,
        &investor2,
        5_000,
        6_500, // profit 1_500
        20,
        BidStatus::Placed,
        2,
    );
    // Low profit bid
    let bid_low = build_bid(
        &env,
        &invoice,
        &investor3,
        5_000,
        6_000, // profit 1_000
        30,
        BidStatus::Placed,
        3,
    );
    persist_bid(&env, &bid_high);
    persist_bid(&env, &bid_mid);
    persist_bid(&env, &bid_low);

    // Verify initial ordering: high > mid > low
    let ranked = BidStorage::rank_bids(&env, &invoice);
    assert_eq!(ranked.len(), 3);
    assert_eq!(ranked.get(0).unwrap().bid_id, bid_high.bid_id);

    // Advance time past the high bid's expiration (it's set to expire at 1000 + 604800)
    // Set timestamp to after expiration
    let expired_timestamp = bid_high.expiration_timestamp + 1;
    env.ledger().with_mut(|li| li.timestamp = expired_timestamp);

    // Run cleanup to expire the high bid
    BidStorage::refresh_expired_bids(&env, &invoice);

    // Verify: high is now expired, get_best_bid should return mid
    let best = BidStorage::get_best_bid(&env, &invoice).unwrap();
    let ranked = BidStorage::rank_bids(&env, &invoice);

    assert_eq!(ranked.len(), 2, "expired bid should be excluded");
    assert_eq!(best.bid_id, bid_mid.bid_id, "best should be mid after high expired");
    assert_eq!(best.bid_id, ranked.get(0).unwrap().bid_id, "best must equal ranked[0]");
}

/// Expired bids are excluded from rank_bids after cleanup runs.
#[test]
fn rank_bids_excludes_expired_after_cleanup() {
    let env = Env::default();
    env.ledger().with_mut(|li| li.timestamp = 500);
    let invoice = invoice_id(&env, 21);

    let investor1 = Address::generate(&env);
    let investor2 = Address::generate(&env);

    // Will expire soon
    let bid_soon = build_bid(
        &env,
        &invoice,
        &investor1,
        3_000,
        4_000,
        10,
        BidStatus::Placed,
        1,
    );
    // Expired already (timestamp in the past)
    let bid_expired = build_bid(
        &env,
        &invoice,
        &investor2,
        5_000,
        6_000,
        20,
        BidStatus::Expired,
        2,
    );
    persist_bid(&env, &bid_soon);
    persist_bid(&env, &bid_expired);

    // Run cleanup
    let cleaned = BidStorage::cleanup_expired_bids(&env, &invoice);
    let ranked = BidStorage::rank_bids(&env, &invoice);

    // bid_soon should still be placed, bid_expired should be excluded
    assert_eq!(ranked.len(), 1, "expired bid should be excluded from ranking");
    assert_eq!(ranked.get(0).unwrap().bid_id, bid_soon.bid_id);
}

/// get_best_bid and rank_bids handle mixed bid statuses correctly.
#[test]
fn best_bid_matches_ranked_with_mixed_statuses() {
    let env = Env::default();
    env.ledger().with_mut(|li| li.timestamp = 1_000);
    let invoice = invoice_id(&env, 22);

    let investor1 = Address::generate(&env);
    let investor2 = Address::generate(&env);
    let investor3 = Address::generate(&env);
    let investor4 = Address::generate(&env);
    let investor5 = Address::generate(&env);

    let bid_placed = build_bid(
        &env,
        &invoice,
        &investor1,
        5_000,
        7_000, // profit 2_000 - highest
        10,
        BidStatus::Placed,
        1,
    );
    let bid_withdrawn = build_bid(
        &env,
        &invoice,
        &investor2,
        6_000,
        8_500, // profit 2_500 - actually highest profit
        20,
        BidStatus::Withdrawn, // terminal but excluded
        2,
    );
    let bid_accepted = build_bid(
        &env,
        &invoice,
        &investor3,
        7_000,
        9_000, // profit 2_000
        30,
        BidStatus::Accepted,
        3,
    );
    let bid_cancelled = build_bid(
        &env,
        &invoice,
        &investor4,
        4_000,
        6_000, // profit 2_000
        40,
        BidStatus::Cancelled,
        4,
    );
    let bid_expired = build_bid(
        &env,
        &invoice,
        &investor5,
        8_000,
        10_000, // profit 2_000
        50,
        BidStatus::Expired,
        5,
    );
    persist_bid(&env, &bid_placed);
    persist_bid(&env, &bid_withdrawn);
    persist_bid(&env, &bid_accepted);
    persist_bid(&env, &bid_cancelled);
    persist_bid(&env, &bid_expired);

    let best = BidStorage::get_best_bid(&env, &invoice).unwrap();
    let ranked = BidStorage::rank_bids(&env, &invoice);

    // Only Placed bids should be in ranked
    assert_eq!(ranked.len(), 1, "only Placed bids should be ranked");
    assert_eq!(ranked.get(0).unwrap().bid_id, bid_placed.bid_id);
    // get_best_bid must match rank_bids[0]
    assert_eq!(best.bid_id, ranked.get(0).unwrap().bid_id);
}

/// Partial expiration cleanup maintains the best-bid invariant.
#[test]
fn cleanup_after_partial_expiration_maintains_invariant() {
    let env = Env::default();
    env.ledger().with_mut(|li| li.timestamp = 100);
    let invoice = invoice_id(&env, 23);

    let inv1 = Address::generate(&env);
    let inv2 = Address::generate(&env);
    let inv3 = Address::generate(&env);
    let inv4 = Address::generate(&env);
    let inv5 = Address::generate(&env);

    // Setup 5 bids: 3 placed with different timestamps (will expire selectively)
    // bid1 expires first (timestamp 10), bid2 expires second (timestamp 20), etc.
    let bid1 = build_bid(&env, &invoice, &inv1, 3_000, 4_000, 10, BidStatus::Placed, 1);
    let bid2 = build_bid(&env, &invoice, &inv2, 3_000, 4_500, 20, BidStatus::Placed, 2);
    let bid3 = build_bid(&env, &invoice, &inv3, 3_000, 5_000, 30, BidStatus::Placed, 3);
    let bid4 = build_bid(&env, &invoice, &inv4, 3_000, 4_200, 40, BidStatus::Placed, 4);
    let bid5 = build_bid(&env, &invoice, &inv5, 3_000, 4_100, 50, BidStatus::Placed, 5);

    // Shorten expiration for earliest bids to make them expire first
    let mut bid1_expired = bid1.clone();
    bid1_expired.expiration_timestamp = 15; // expires at timestamp 15
    let mut bid2_expired = bid2.clone();
    bid2_expired.expiration_timestamp = 25; // expires at timestamp 25
    // bid3-bid5 keep default expiration (100 + 604800)

    persist_bid(&env, &bid1_expired);
    persist_bid(&env, &bid2_expired);
    persist_bid(&env, &bid3);
    persist_bid(&env, &bid4);
    persist_bid(&env, &bid5);

    // Initial state: 5 placed bids
    let ranked = BidStorage::rank_bids(&env, &invoice);
    assert_eq!(ranked.len(), 5);

    // Advance to just after bid1 expires but before bid2
    env.ledger().with_mut(|li| li.timestamp = 20);
    BidStorage::cleanup_expired_bids(&env, &invoice);

    let best = BidStorage::get_best_bid(&env, &invoice).unwrap();
    let ranked = BidStorage::rank_bids(&env, &invoice);
    assert_eq!(ranked.len(), 4, "1 expired");
    assert_eq!(best.bid_id, ranked.get(0).unwrap().bid_id, "invariant holds after 1 cleanup");

    // Advance to after bid2 expires
    env.ledger().with_mut(|li| li.timestamp = 30);
    BidStorage::cleanup_expired_bids(&env, &invoice);

    let best = BidStorage::get_best_bid(&env, &invoice).unwrap();
    let ranked = BidStorage::rank_bids(&env, &invoice);
    assert_eq!(ranked.len(), 3, "2 expired");
    assert_eq!(best.bid_id, ranked.get(0).unwrap().bid_id, "invariant holds after 2 cleanups");
}

/// When all bids expire, both get_best_bid and rank_bids return empty.
#[test]
fn best_bid_returns_none_when_all_expired() {
    let env = Env::default();
    env.ledger().with_mut(|li| li.timestamp = 500);
    let invoice = invoice_id(&env, 24);

    let inv1 = Address::generate(&env);
    let inv2 = Address::generate(&env);

    let bid1 = build_bid(&env, &invoice, &inv1, 5_000, 6_000, 10, BidStatus::Placed, 1);
    let bid2 = build_bid(&env, &invoice, &inv2, 5_000, 6_500, 20, BidStatus::Placed, 2);

    // Set both to expire immediately
    let mut bid1_exp = bid1.clone();
    bid1_exp.expiration_timestamp = 501;
    let mut bid2_exp = bid2.clone();
    bid2_exp.expiration_timestamp = 501;

    persist_bid(&env, &bid1_exp);
    persist_bid(&env, &bid2_exp);

    // Advance past expiration
    env.ledger().with_mut(|li| li.timestamp = 600);
    BidStorage::cleanup_expired_bids(&env, &invoice);

    let best = BidStorage::get_best_bid(&env, &invoice);
    let ranked = BidStorage::rank_bids(&env, &invoice);

    assert!(best.is_none(), "get_best_bid should return None when all expired");
    assert_eq!(ranked.len(), 0, "rank_bids should return empty when all expired");
}

/// Calling cleanup multiple times is idempotent and maintains invariant.
#[test]
fn best_bid_matches_ranked_idempotent_cleanup() {
    let env = Env::default();
    env.ledger().with_mut(|li| li.timestamp = 100);
    let invoice = invoice_id(&env, 25);

    let inv1 = Address::generate(&env);
    let inv2 = Address::generate(&env);
    let inv3 = Address::generate(&env);

    let bid1 = build_bid(&env, &invoice, &inv1, 5_000, 7_000, 10, BidStatus::Placed, 1);
    let bid2 = build_bid(&env, &invoice, &inv2, 5_000, 6_500, 20, BidStatus::Placed, 2);
    let bid3 = build_bid(&env, &invoice, &inv3, 5_000, 6_000, 30, BidStatus::Placed, 3);

    // Make bid1 expire
    let mut bid1_exp = bid1.clone();
    bid1_exp.expiration_timestamp = 150;

    persist_bid(&env, &bid1_exp);
    persist_bid(&env, &bid2);
    persist_bid(&env, &bid3);

    // Advance and run cleanup multiple times
    env.ledger().with_mut(|li| li.timestamp = 200);

    let cleaned1 = BidStorage::cleanup_expired_bids(&env, &invoice);
    let cleaned2 = BidStorage::cleanup_expired_bids(&env, &invoice);
    let cleaned3 = BidStorage::cleanup_expired_bids(&env, &invoice);

    assert_eq!(cleaned1, 1, "first cleanup should expire 1 bid");
    assert_eq!(cleaned2, 0, "second cleanup should be idempotent");
    assert_eq!(cleaned3, 0, "third cleanup should be idempotent");

    let best = BidStorage::get_best_bid(&env, &invoice).unwrap();
    let ranked = BidStorage::rank_bids(&env, &invoice);

    assert_eq!(ranked.len(), 2);
    assert_eq!(best.bid_id, ranked.get(0).unwrap().bid_id, "invariant holds after multiple cleanups");
}
}
