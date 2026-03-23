//! Deterministic bid ranking tests.
use crate::bid::{Bid, BidStatus, BidStorage};
use soroban_sdk::{testutils::Address as _, Address, BytesN, Env};

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
