extern crate alloc;
use alloc::vec::Vec;
/// # Bid Ranking Determinism Tests  (Issue #1551)
///
/// Verifies that `BidStorage::rank_bids` and `BidStorage::compare_bids` produce
/// **identical output for identical input regardless of call repetition or insertion
/// order**.  These tests run on every CI matrix entry (plain `#[cfg(test)]`, no
/// feature gate).

#[cfg(test)]
mod test_bid_ranking_determinism {
    use crate::bid::{Bid, BidStatus, BidStorage};
    use crate::QuickLendXContract;
    use alloc::vec::Vec;
    use core::cmp::Ordering;
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        Address, BytesN, Env,
    };
//! # Bid Ranking Determinism and Pagination Regression Tests
//!
//! Verifies that bid acceptance, ranking, expiry, winner selection, and
//! pagination semantics provide deterministic guarantees under normal, invalid,
//! repeated, boundary, and concurrent conditions.

#![cfg(test)]

    fn setup(timestamp: u64) -> (Env, Address) {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = timestamp);
        let contract_id = env.register(QuickLendXContract, ());
        (env, contract_id)
    }

    /// Build a deterministic invoice ID from a single seed byte.
    fn invoice_id(env: &Env, seed: u8) -> BytesN<32> {
        let mut bytes = [0u8; 32];
        bytes[0] = 0xFF; // distinct namespace from bid IDs
        bytes[1] = seed;
        BytesN::from_array(env, &bytes)
    }

    /// Build a `Bid` with a deterministic ID derived from `id_byte`.
    fn make_bid(
        env: &Env,
        invoice: &BytesN<32>,
        bid_amount: i128,
        expected_return: i128,
        timestamp: u64,
        status: BidStatus,
        id_byte: u8,
    ) -> Bid {
        let mut id = [0u8; 32];
        id[0] = 0xB1;
        id[1] = 0xD0;
        id[2..10].copy_from_slice(&timestamp.to_be_bytes());
        id[30] = id_byte;
        id[31] = id_byte;
        Bid {
            bid_id: BytesN::from_array(env, &id),
            invoice_id: invoice.clone(),
            investor: Address::generate(env),
            bid_amount,
            expected_return,
            timestamp,
            status,
            expiration_timestamp: timestamp.saturating_add(7 * 24 * 3600),
        }
use alloc::vec::Vec;
use core::cmp::Ordering;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{testutils::Ledger, Address, BytesN, Env};

use crate::bid::{Bid, BidStatus, BidStorage};
use crate::pagination::{cap_query_limit, PageCursor, MAX_QUERY_LIMIT};
use crate::types::PaginatedBids;
use crate::QuickLendXContract;

// ============================================================================
// Helpers
// ============================================================================

fn setup() -> (Env, Address) {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());
    (env, contract_id)
}

fn invoice_id(env: &Env, seed: u8) -> BytesN<32> {
    let mut bytes = [0u8; 32];
    bytes[0] = seed;
    BytesN::from_array(env, &bytes)
}

fn make_bid(
    env: &Env,
    invoice_id: &BytesN<32>,
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
        investor: Address::generate(env),
        bid_amount,
        expected_return,
        timestamp,
        status,
        expiration_timestamp: timestamp.saturating_add(604_800),
    }
}

    /// Persist a bid and register it on the invoice index inside contract context.
    fn persist(env: &Env, contract_id: &Address, bid: &Bid) {
        env.as_contract(contract_id, || {
            BidStorage::store_bid(env, bid);
            BidStorage::add_bid_to_invoice(env, &bid.invoice_id, &bid.bid_id);
        });
    }

    fn rank_bids(env: &Env, contract_id: &Address, inv: &BytesN<32>) -> soroban_sdk::Vec<Bid> {
        env.as_contract(contract_id, || BidStorage::rank_bids(env, inv))
    }

    fn get_best_bid(env: &Env, contract_id: &Address, inv: &BytesN<32>) -> Option<Bid> {
        env.as_contract(contract_id, || BidStorage::get_best_bid(env, inv))
    }
fn persist(env: &Env, bid: &Bid, contract_id: &Address) {
    env.as_contract(contract_id, || {
        BidStorage::store_bid(env, bid);
        BidStorage::add_bid_to_invoice(env, &bid.invoice_id, &bid.bid_id);
    });
}

fn ids(bids: &soroban_sdk::Vec<Bid>) -> Vec<[u8; 32]> {
    let mut result = Vec::new();
    for bid in bids.iter() {
        result.push(bid.bid_id.to_array());
    }
    result
}

fn rank_bids(env: &Env, invoice: &BytesN<32>, contract_id: &Address) -> soroban_sdk::Vec<Bid> {
    env.as_contract(contract_id, || BidStorage::rank_bids(env, invoice))
}

    #[test]
    fn rank_bids_returns_same_sequence_on_repeated_calls() {
        let (env, contract_id) = setup(1_000);
        let inv = invoice_id(&env, 1);

        let bid_a = make_bid(&env, &inv, 5_000, 7_000, 10, BidStatus::Placed, 1);
        let bid_b = make_bid(&env, &inv, 4_000, 6_000, 20, BidStatus::Placed, 2);
        let bid_c = make_bid(&env, &inv, 6_000, 7_500, 30, BidStatus::Placed, 3);
        persist(&env, &contract_id, &bid_a);
        persist(&env, &contract_id, &bid_b);
        persist(&env, &contract_id, &bid_c);

        let first_call = ids(&rank_bids(&env, &contract_id, &inv));
        let second_call = ids(&rank_bids(&env, &contract_id, &inv));
        let third_call = ids(&rank_bids(&env, &contract_id, &inv));
fn get_best_bid(env: &Env, invoice: &BytesN<32>, contract_id: &Address) -> Option<Bid> {
    env.as_contract(contract_id, || BidStorage::get_best_bid(env, invoice))
}

fn rank_bids_paged(
    env: &Env,
    invoice: &BytesN<32>,
    offset: u32,
    limit: u32,
    contract_id: &Address,
) -> (soroban_sdk::Vec<Bid>, u32, bool) {
    env.as_contract(contract_id, || {
        BidStorage::rank_bids_paged(env, invoice, offset, limit)
    })
}

// ============================================================================
// Determinism and Tie-Breaker Ordering
// ============================================================================

/// `compare_bids(a, a)` must return `Equal` for any bid.
#[test]
fn compare_bids_is_reflexive_for_arbitrary_bid() {
    let env = Env::default();
    env.ledger().with_mut(|l| l.timestamp = 5_000);
    let inv = invoice_id(&env, 40);

    let bid = make_bid(&env, &inv, 3_333, 4_444, 99, BidStatus::Placed, 7);
    assert_eq!(
        BidStorage::compare_bids(&bid, &bid),
        Ordering::Equal,
        "compare_bids(a, a) must be Equal"
    );
}

/// `compare_bids(a, b)` is the inverse of `compare_bids(b, a)` (antisymmetry).
#[test]
fn compare_bids_is_antisymmetric() {
    let env = Env::default();
    env.ledger().with_mut(|l| l.timestamp = 6_000);
    let inv = invoice_id(&env, 41);

    let bid_a = make_bid(&env, &inv, 5_000, 8_000, 10, BidStatus::Placed, 1);
    let bid_b = make_bid(&env, &inv, 5_000, 7_000, 10, BidStatus::Placed, 2);

    let ab = BidStorage::compare_bids(&bid_a, &bid_b);
    let ba = BidStorage::compare_bids(&bid_b, &bid_a);

    assert_eq!(
        ab,
        Ordering::Greater,
        "bid_a should rank above bid_b (higher profit)"
    );
    assert_eq!(
        ba,
        Ordering::Less,
        "compare_bids(b, a) must be the reverse of compare_bids(a, b)"
    );
}

/// Profit tiebreaker: higher `expected_return - bid_amount` wins.
#[test]
fn compare_bids_ranks_higher_profit_first() {
    let env = Env::default();
    env.ledger().with_mut(|l| l.timestamp = 7_000);
    let inv = invoice_id(&env, 42);

    let high_profit = make_bid(&env, &inv, 5_000, 7_000, 1, BidStatus::Placed, 1); // profit 2000
    let low_profit = make_bid(&env, &inv, 5_000, 6_000, 1, BidStatus::Placed, 2); // profit 1000

    assert_eq!(
        BidStorage::compare_bids(&high_profit, &low_profit),
        Ordering::Greater
    );
}

/// Second tiebreaker: when profit is equal, higher `expected_return` wins.
#[test]
fn compare_bids_ranks_higher_expected_return_when_profit_ties() {
    let env = Env::default();
    env.ledger().with_mut(|l| l.timestamp = 8_000);
    let inv = invoice_id(&env, 43);

    let high_return = make_bid(&env, &inv, 6_000, 7_000, 1, BidStatus::Placed, 1);
    let low_return = make_bid(&env, &inv, 5_000, 6_000, 1, BidStatus::Placed, 2);

    assert_eq!(
        BidStorage::compare_bids(&high_return, &low_return),
        Ordering::Greater,
        "higher expected_return must win when profit is equal"
    );
}

/// Third tiebreaker: when economics tie via saturated profit, higher bid_amount wins.
#[test]
fn compare_bids_ranks_higher_bid_amount_when_profit_and_return_tie() {
    let env = Env::default();
    env.ledger().with_mut(|l| l.timestamp = 9_000);
    let inv = invoice_id(&env, 44);

    let high_amount = make_bid(&env, &inv, 3_000, 1_000, 1, BidStatus::Placed, 1);
    let low_amount = make_bid(&env, &inv, 2_000, 1_000, 1, BidStatus::Placed, 2);

    assert_eq!(
        BidStorage::compare_bids(&high_amount, &low_amount),
        Ordering::Greater,
        "higher bid_amount must win when profit and expected_return tie"
    );
}

/// Fourth tiebreaker: when all economic fields match, newer timestamp wins.
#[test]
fn compare_bids_ranks_newer_timestamp_when_all_economics_tie() {
    let env = Env::default();
    env.ledger().with_mut(|l| l.timestamp = 10_000);
    let inv = invoice_id(&env, 45);

    let newer = make_bid(&env, &inv, 5_000, 6_000, 200, BidStatus::Placed, 1);
    let older = make_bid(&env, &inv, 5_000, 6_000, 100, BidStatus::Placed, 2);

    assert_eq!(
        BidStorage::compare_bids(&newer, &older),
        Ordering::Greater,
        "newer timestamp must win when economics are equal"
    );
}

/// Fifth tiebreaker (final): bid_id lexicographic order — higher byte array wins.
#[test]
fn compare_bids_uses_bid_id_as_final_deterministic_tiebreaker() {
    let env = Env::default();
    env.ledger().with_mut(|l| l.timestamp = 11_000);
    let inv = invoice_id(&env, 46);

    let high_id = make_bid(&env, &inv, 5_000, 6_000, 50, BidStatus::Placed, 0xFF);
    let low_id = make_bid(&env, &inv, 5_000, 6_000, 50, BidStatus::Placed, 0x01);

    assert_eq!(
        BidStorage::compare_bids(&high_id, &low_id),
        Ordering::Greater,
        "higher bid_id must win when every other field is identical"
    );
    assert_eq!(BidStorage::compare_bids(&low_id, &high_id), Ordering::Less);
}

        let bid_hi_id = {
            let (env, _) = setup(4_000);
            make_bid(&env, &invoice_id(&env, 31), 5_000, 6_000, 50, BidStatus::Placed, 0x09)
                .bid_id
                .to_array()
/// Insertion order independence on full tie.
#[test]
fn rank_bids_insertion_order_independence_on_full_tie() {
    let check = |seed: u8, ascending: bool| -> Vec<[u8; 32]> {
        let (env, contract_id) = setup();
        env.ledger().with_mut(|l| l.timestamp = 4_000);
        let inv = invoice_id(&env, seed);

        let suffixes: [u8; 4] = if ascending {
            [0x01, 0x04, 0x07, 0x09]
        } else {
            [0x09, 0x07, 0x04, 0x01]
        };

        for &s in suffixes.iter() {
            let bid = make_bid(&env, &inv, 5_000, 6_000, 50, BidStatus::Placed, s);
            persist(&env, &bid, &contract_id);
        }

    #[test]
    fn compare_bids_is_reflexive_for_arbitrary_bid() {
        let (env, _) = setup(5_000);
        let inv = invoice_id(&env, 40);
        ids(&rank_bids(&env, &inv, &contract_id))
    };

    let ascending = check(30, true);
    let descending = check(31, false);

    assert_eq!(
        ascending, descending,
        "full-tie ranking must not depend on insertion order"
    );
    assert_eq!(ascending.len(), 4);
}

    #[test]
    fn compare_bids_is_antisymmetric() {
        let (env, _) = setup(6_000);
        let inv = invoice_id(&env, 41);

        let bid_a = make_bid(&env, &inv, 5_000, 8_000, 10, BidStatus::Placed, 1);
        let bid_b = make_bid(&env, &inv, 5_000, 7_000, 10, BidStatus::Placed, 2);
// ============================================================================
// Single-bid, Empty, and Non-Placed Exclusion
// ============================================================================

#[test]
fn rank_bids_returns_single_element_for_one_placed_bid() {
    let (env, contract_id) = setup();
    env.ledger().with_mut(|l| l.timestamp = 12_000);
    let inv = invoice_id(&env, 50);

    let bid = make_bid(&env, &inv, 1_000, 2_000, 1, BidStatus::Placed, 1);
    persist(&env, &bid, &contract_id);

    for call in 1..=3u8 {
        let ranked = rank_bids(&env, &inv, &contract_id);
        assert_eq!(
            ranked.len(),
            1,
            "call {call}: expected exactly 1 ranked bid"
        );
        assert_eq!(
            ranked.get(0).unwrap().bid_id,
            bid.bid_id,
            "call {call}: wrong bid returned"
        );
    }
}

    #[test]
    fn compare_bids_ranks_higher_profit_first() {
        let (env, _) = setup(7_000);
        let inv = invoice_id(&env, 42);

        let high_profit = make_bid(&env, &inv, 5_000, 7_000, 1, BidStatus::Placed, 1);
        let low_profit = make_bid(&env, &inv, 5_000, 6_000, 1, BidStatus::Placed, 2);
#[test]
fn rank_bids_returns_empty_vec_for_invoice_with_no_bids() {
    let (env, contract_id) = setup();
    env.ledger().with_mut(|l| l.timestamp = 13_000);
    let inv = invoice_id(&env, 51);

    for call in 1..=3u8 {
        let ranked = rank_bids(&env, &inv, &contract_id);
        assert_eq!(
            ranked.len(),
            0,
            "call {call}: rank_bids on empty invoice should return empty Vec"
        );
    }
}

    #[test]
    fn compare_bids_ranks_higher_expected_return_when_profit_ties() {
        let (env, _) = setup(8_000);
        let inv = invoice_id(&env, 43);

        let high_return = make_bid(&env, &inv, 6_000, 7_000, 1, BidStatus::Placed, 1);
        let low_return = make_bid(&env, &inv, 5_000, 6_000, 1, BidStatus::Placed, 2);
#[test]
fn get_best_bid_returns_none_for_invoice_with_no_bids() {
    let (env, contract_id) = setup();
    env.ledger().with_mut(|l| l.timestamp = 14_000);
    let inv = invoice_id(&env, 52);

    let result = get_best_bid(&env, &inv, &contract_id);
    assert!(
        result.is_none(),
        "get_best_bid must return None when no bids exist"
    );
}

#[test]
fn rank_bids_excludes_all_non_placed_statuses() {
    let (env, contract_id) = setup();
    env.ledger().with_mut(|l| l.timestamp = 15_000);
    let inv = invoice_id(&env, 60);

    let placed = make_bid(&env, &inv, 1_000, 2_000, 1, BidStatus::Placed, 1);
    let accepted = make_bid(&env, &inv, 9_000, 99_000, 2, BidStatus::Accepted, 2);
    let withdrawn = make_bid(&env, &inv, 9_000, 99_000, 3, BidStatus::Withdrawn, 3);
    let expired = make_bid(&env, &inv, 9_000, 99_000, 4, BidStatus::Expired, 4);
    let cancelled = make_bid(&env, &inv, 9_000, 99_000, 5, BidStatus::Cancelled, 5);

    persist(&env, &placed, &contract_id);
    persist(&env, &accepted, &contract_id);
    persist(&env, &withdrawn, &contract_id);
    persist(&env, &expired, &contract_id);
    persist(&env, &cancelled, &contract_id);

    let ranked = rank_bids(&env, &inv, &contract_id);
    assert_eq!(
        ranked.len(),
        1,
        "only Placed bids should appear in ranked output"
    );
    assert_eq!(
        ranked.get(0).unwrap().bid_id,
        placed.bid_id,
        "the single Placed bid must be the winner, not a non-Placed one"
    );

    let best = get_best_bid(&env, &inv, &contract_id).expect("best must exist");
    assert_eq!(
        best.bid_id, placed.bid_id,
        "get_best_bid must also exclude non-Placed bids"
    );
}

#[test]
fn rank_bids_excludes_expired_bids_before_storage_cleanup() {
    let (env, contract_id) = setup();
    env.ledger().with_mut(|l| l.timestamp = 100);
    let inv = invoice_id(&env, 62);

    #[test]
    fn compare_bids_ranks_higher_bid_amount_when_profit_and_return_tie() {
        let (env, _) = setup(9_000);
        let inv = invoice_id(&env, 44);

        let higher = make_bid(&env, &inv, 4_000, 6_000, 1, BidStatus::Placed, 1);
        let lower = make_bid(&env, &inv, 5_000, 7_000, 1, BidStatus::Placed, 2);

        assert_eq!(
            BidStorage::compare_bids(&lower, &higher),
            Ordering::Greater,
            "higher expected_return wins when profit is equal"
        );
        assert_eq!(
            BidStorage::compare_bids(&higher, &lower),
            Ordering::Less
        );
    }

    #[test]
    fn compare_bids_ranks_newer_timestamp_when_all_economics_tie() {
        let (env, _) = setup(10_000);
        let inv = invoice_id(&env, 45);
    let mut expired_placed = make_bid(&env, &inv, 5_000, 10_000, 100, BidStatus::Placed, 1);
    expired_placed.expiration_timestamp = 150; // Expired at timestamp 200

    let mut valid_placed = make_bid(&env, &inv, 5_000, 7_000, 100, BidStatus::Placed, 2);
    valid_placed.expiration_timestamp = 300; // Still valid at timestamp 200

    persist(&env, &expired_placed, &contract_id);
    persist(&env, &valid_placed, &contract_id);

    // Advance time past expired_placed deadline
    env.ledger().with_mut(|l| l.timestamp = 200);

    #[test]
    fn compare_bids_uses_bid_id_as_final_deterministic_tiebreaker() {
        let (env, _) = setup(11_000);
        let inv = invoice_id(&env, 46);

        let high_id = make_bid(&env, &inv, 5_000, 6_000, 50, BidStatus::Placed, 0xFF);
        let low_id = make_bid(&env, &inv, 5_000, 6_000, 50, BidStatus::Placed, 0x01);
    let ranked = rank_bids(&env, &inv, &contract_id);
    assert_eq!(
        ranked.len(),
        1,
        "expired bid must be excluded from ranking even if status is Placed"
    );
    assert_eq!(ranked.get(0).unwrap().bid_id, valid_placed.bid_id);

    let best = get_best_bid(&env, &inv, &contract_id).expect("best bid must exist");
    assert_eq!(best.bid_id, valid_placed.bid_id);
}

#[test]
fn rank_bids_is_deterministic_with_zero_and_negative_profit_bids() {
    let (env, contract_id) = setup();
    env.ledger().with_mut(|l| l.timestamp = 17_000);
    let inv = invoice_id(&env, 70);

    let positive = make_bid(&env, &inv, 5_000, 6_000, 1, BidStatus::Placed, 3); // profit 1000
    let zero = make_bid(&env, &inv, 5_000, 5_000, 1, BidStatus::Placed, 2); // profit 0
    let negative = make_bid(&env, &inv, 6_000, 5_000, 1, BidStatus::Placed, 1); // profit clamped 0

    persist(&env, &positive, &contract_id);
    persist(&env, &zero, &contract_id);
    persist(&env, &negative, &contract_id);

    let first = ids(&rank_bids(&env, &inv, &contract_id));
    let second = ids(&rank_bids(&env, &inv, &contract_id));

    assert_eq!(
        first, second,
        "ranking with negative-profit bids must be deterministic across calls"
    );

    assert_eq!(
        first[0],
        positive.bid_id.to_array(),
        "positive profit bid must rank first"
    );
}

// ============================================================================
// Pagination and Cursor Semantics Tests
// ============================================================================

#[test]
fn rank_bids_paged_empty_invoice() {
    let (env, contract_id) = setup();
    env.ledger().with_mut(|l| l.timestamp = 20_000);
    let inv = invoice_id(&env, 80);

    let (items, total_count, has_more) = rank_bids_paged(&env, &inv, 0, 10, &contract_id);
    assert_eq!(items.len(), 0);
    assert_eq!(total_count, 0);
    assert_eq!(has_more, false);
}

#[test]
fn rank_bids_paged_single_page() {
    let (env, contract_id) = setup();
    env.ledger().with_mut(|l| l.timestamp = 20_000);
    let inv = invoice_id(&env, 81);

    for i in 1..=5u8 {
        let bid = make_bid(
            &env,
            &inv,
            1_000,
            1_000 + (i as i128) * 1_000,
            100,
            BidStatus::Placed,
            i,
        );
        assert_eq!(
            BidStorage::compare_bids(&low_id, &high_id),
            Ordering::Less
        persist(&env, &bid, &contract_id);
    }

    let (items, total_count, has_more) = rank_bids_paged(&env, &inv, 0, 10, &contract_id);
    assert_eq!(items.len(), 5);
    assert_eq!(total_count, 5);
    assert_eq!(has_more, false);
}

#[test]
fn rank_bids_paged_multi_page_no_overlap_no_skips() {
    let (env, contract_id) = setup();
    env.ledger().with_mut(|l| l.timestamp = 20_000);
    let inv = invoice_id(&env, 82);

    let mut expected_all = Vec::new();
    for i in 1..=12u8 {
        let bid = make_bid(
            &env,
            &inv,
            1_000,
            1_000 + (i as i128) * 500,
            100,
            BidStatus::Placed,
            i,
        );
        persist(&env, &bid, &contract_id);
    }

    // =========================================================================
    // Happy path — single-bid and empty edge cases
    // =========================================================================

    #[test]
    fn rank_bids_returns_single_element_for_one_placed_bid() {
        let (env, contract_id) = setup(12_000);
        let inv = invoice_id(&env, 50);

        let bid = make_bid(&env, &inv, 1_000, 2_000, 1, BidStatus::Placed, 1);
        persist(&env, &contract_id, &bid);

        for call in 1..=3u8 {
            let ranked = rank_bids(&env, &contract_id, &inv);
            assert_eq!(ranked.len(), 1, "call {call}: expected exactly 1 ranked bid");
            assert_eq!(
                ranked.get(0).unwrap().bid_id,
                bid.bid_id,
                "call {call}: wrong bid returned"
            );
        }
    }

    #[test]
    fn rank_bids_returns_empty_vec_for_invoice_with_no_bids() {
        let (env, contract_id) = setup(13_000);
        let inv = invoice_id(&env, 51);

        for call in 1..=3u8 {
            let ranked = rank_bids(&env, &contract_id, &inv);
            assert_eq!(
                ranked.len(),
                0,
                "call {call}: rank_bids on empty invoice should return empty Vec"
            );
        }
    }

    #[test]
    fn get_best_bid_returns_none_for_invoice_with_no_bids() {
        let (env, contract_id) = setup(14_000);
        let inv = invoice_id(&env, 52);

        let result = get_best_bid(&env, &contract_id, &inv);
        assert!(
            result.is_none(),
            "get_best_bid must return None when no bids exist"
        );
    }

    // =========================================================================
    // Sad path — non-Placed bids are excluded from ranking
    // =========================================================================

    #[test]
    fn rank_bids_excludes_all_non_placed_statuses() {
        let (env, contract_id) = setup(15_000);
        let inv = invoice_id(&env, 60);

        let placed = make_bid(&env, &inv, 1_000, 2_000, 1, BidStatus::Placed, 1);
        let accepted = make_bid(&env, &inv, 9_000, 99_000, 2, BidStatus::Accepted, 2);
        let withdrawn = make_bid(&env, &inv, 9_000, 99_000, 3, BidStatus::Withdrawn, 3);
        let expired = make_bid(&env, &inv, 9_000, 99_000, 4, BidStatus::Expired, 4);
        let cancelled = make_bid(&env, &inv, 9_000, 99_000, 5, BidStatus::Cancelled, 5);

        persist(&env, &contract_id, &placed);
        persist(&env, &contract_id, &accepted);
        persist(&env, &contract_id, &withdrawn);
        persist(&env, &contract_id, &expired);
        persist(&env, &contract_id, &cancelled);

        let ranked = rank_bids(&env, &contract_id, &inv);
        assert_eq!(
            ranked.len(),
            1,
            "only Placed bids should appear in ranked output"
        );
        assert_eq!(
            ranked.get(0).unwrap().bid_id,
            placed.bid_id,
            "the single Placed bid must be the winner, not a non-Placed one"
    let all_ranked = rank_bids(&env, &inv, &contract_id);
    for bid in all_ranked.iter() {
        expected_all.push(bid.bid_id.to_array());
    }

    // Read in 3 pages of 5, 5, 2 items
    let (page1, total1, more1) = rank_bids_paged(&env, &inv, 0, 5, &contract_id);
    assert_eq!(page1.len(), 5);
    assert_eq!(total1, 12);
    assert_eq!(more1, true);

    let (page2, total2, more2) = rank_bids_paged(&env, &inv, 5, 5, &contract_id);
    assert_eq!(page2.len(), 5);
    assert_eq!(total2, 12);
    assert_eq!(more2, true);

    let (page3, total3, more3) = rank_bids_paged(&env, &inv, 10, 5, &contract_id);
    assert_eq!(page3.len(), 2);
    assert_eq!(total3, 12);
    assert_eq!(more3, false);

    // Concat pages
    let mut collected = Vec::new();
    for b in page1.iter() {
        collected.push(b.bid_id.to_array());
    }
    for b in page2.iter() {
        collected.push(b.bid_id.to_array());
    }
    for b in page3.iter() {
        collected.push(b.bid_id.to_array());
    }

    assert_eq!(
        collected, expected_all,
        "paginated collection must match full ranked list exactly"
    );
}

#[test]
fn rank_bids_paged_boundary_and_out_of_bounds_offsets() {
    let (env, contract_id) = setup();
    env.ledger().with_mut(|l| l.timestamp = 20_000);
    let inv = invoice_id(&env, 83);

    for i in 1..=3u8 {
        let bid = make_bid(&env, &inv, 1_000, 2_000, 100, BidStatus::Placed, i);
        persist(&env, &bid, &contract_id);
    }

    // Offset exactly equal to total_count -> returns empty items, has_more=false
    let (items_exact, total_exact, more_exact) = rank_bids_paged(&env, &inv, 3, 10, &contract_id);
    assert_eq!(items_exact.len(), 0);
    assert_eq!(total_exact, 3);
    assert_eq!(more_exact, false);

    // Offset strictly greater than total_count -> returns empty items, has_more=false
    let (items_oob, total_oob, more_oob) = rank_bids_paged(&env, &inv, 100, 10, &contract_id);
    assert_eq!(items_oob.len(), 0);
    assert_eq!(total_oob, 3);
    assert_eq!(more_oob, false);
}

#[test]
fn rank_bids_paged_limit_capping() {
    let (env, contract_id) = setup();
    env.ledger().with_mut(|l| l.timestamp = 20_000);
    let inv = invoice_id(&env, 84);

    for i in 1..=30u8 {
        let bid = make_bid(
            &env,
            &inv,
            1_000,
            1_000 + (i as i128) * 100,
            100,
            BidStatus::Placed,
            i,
        );
        persist(&env, &bid, &contract_id);
    }

        let best = get_best_bid(&env, &contract_id, &inv).expect("best must exist");
        assert_eq!(
            best.bid_id, placed.bid_id,
            "get_best_bid must also exclude non-Placed bids"
    // Request limit larger than MAX_QUERY_LIMIT
    let (items, total, has_more) =
        rank_bids_paged(&env, &inv, 0, MAX_QUERY_LIMIT + 50, &contract_id);
    assert_eq!(items.len(), 30);
    assert_eq!(total, 30);
    assert_eq!(has_more, false);
}

#[test]
fn test_contract_get_ranked_bids_paged_integration() {
    let (env, contract_id) = setup();
    env.ledger().with_mut(|l| l.timestamp = 30_000);
    let client = crate::QuickLendXContractClient::new(&env, &contract_id);
    let inv = invoice_id(&env, 90);

    for i in 1..=7u8 {
        let bid = make_bid(
            &env,
            &inv,
            1_000,
            1_000 + (i as i128) * 200,
            100,
            BidStatus::Placed,
            i,
        );
        persist(&env, &bid, &contract_id);
    }

    #[test]
    fn rank_bids_returns_empty_when_all_bids_are_non_placed() {
        let (env, contract_id) = setup(16_000);
        let inv = invoice_id(&env, 61);

        persist(&env, &contract_id, &make_bid(&env, &inv, 5_000, 9_000, 1, BidStatus::Accepted, 1));
        persist(&env, &contract_id, &make_bid(&env, &inv, 5_000, 9_000, 2, BidStatus::Withdrawn, 2));
        persist(&env, &contract_id, &make_bid(&env, &inv, 5_000, 9_000, 3, BidStatus::Expired, 3));
        persist(&env, &contract_id, &make_bid(&env, &inv, 5_000, 9_000, 4, BidStatus::Cancelled, 4));

        let ranked = rank_bids(&env, &contract_id, &inv);
        assert_eq!(ranked.len(), 0, "all non-Placed: ranked must be empty");

        let best = get_best_bid(&env, &contract_id, &inv);
        assert!(best.is_none(), "all non-Placed: get_best_bid must be None");
    }

    // =========================================================================
    // Sad path — negative / zero profit bids still rank deterministically
    // =========================================================================

    #[test]
    fn rank_bids_is_deterministic_with_zero_and_negative_profit_bids() {
        let (env, contract_id) = setup(17_000);
        let inv = invoice_id(&env, 70);

        let positive = make_bid(&env, &inv, 5_000, 6_000, 1, BidStatus::Placed, 3);
        let zero = make_bid(&env, &inv, 5_000, 5_000, 1, BidStatus::Placed, 2);
        let negative = make_bid(&env, &inv, 6_000, 5_000, 1, BidStatus::Placed, 1);

        persist(&env, &contract_id, &positive);
        persist(&env, &contract_id, &zero);
        persist(&env, &contract_id, &negative);

        let first = ids(&rank_bids(&env, &contract_id, &inv));
        let second = ids(&rank_bids(&env, &contract_id, &inv));
    let page1: PaginatedBids = client.get_ranked_bids_paged(&inv, &0, &4);
    assert_eq!(page1.items.len(), 4);
    assert_eq!(page1.total_count, 7);
    assert_eq!(page1.has_more, true);

    let page2: PaginatedBids = client.get_ranked_bids_paged(&inv, &4, &4);
    assert_eq!(page2.items.len(), 3);
    assert_eq!(page2.total_count, 7);
    assert_eq!(page2.has_more, false);

    assert_eq!(
        page1.items.get(0).unwrap().bid_id,
        client.get_best_bid(&inv).unwrap().bid_id,
        "first element of page 1 must equal get_best_bid"
    );
}

#[test]
fn test_contract_get_bid_history_paged_filters_expired_on_placed() {
    let (env, contract_id) = setup();
    env.ledger().with_mut(|l| l.timestamp = 100);
    let client = crate::QuickLendXContractClient::new(&env, &contract_id);
    let inv = invoice_id(&env, 91);

    let mut expired = make_bid(&env, &inv, 1_000, 2_000, 100, BidStatus::Placed, 1);
    expired.expiration_timestamp = 150;

    let mut active = make_bid(&env, &inv, 1_000, 3_000, 100, BidStatus::Placed, 2);
    active.expiration_timestamp = 500;

    let cancelled = make_bid(&env, &inv, 1_000, 4_000, 100, BidStatus::Cancelled, 3);

    persist(&env, &expired, &contract_id);
    persist(&env, &active, &contract_id);
    persist(&env, &cancelled, &contract_id);

        assert_eq!(
            first[0],
            positive.bid_id.to_array(),
            "positive profit bid must rank first"
        );
        assert_eq!(
            first[2],
            negative.bid_id.to_array(),
            "negative profit bid must rank last"
        );
    }
    // Advance time past expired bid
    env.ledger().with_mut(|l| l.timestamp = 200);

    // Filter by Placed: must return only active non-expired bid
    let placed_paged = client.get_bid_history_paged(&inv, &Some(BidStatus::Placed), &0, &10);
    assert_eq!(placed_paged.items.len(), 1);
    assert_eq!(placed_paged.total_count, 1);
    assert_eq!(placed_paged.items.get(0).unwrap().bid_id, active.bid_id);

    // Filter None: returns all historical bids
    let all_paged = client.get_bid_history_paged(&inv, &None, &0, &10);
    assert_eq!(all_paged.items.len(), 3);
    assert_eq!(all_paged.total_count, 3);
}

#[test]
fn test_page_cursor_generation_semantics() {
    let generation = 1725000000u64;
    let cursor = PageCursor::new(10, generation);

    assert_eq!(cursor.require_stable(generation), Ok(()));
    assert_eq!(
        cursor.require_stable(generation + 1),
        Err(crate::errors::QuickLendXError::UnstableCursor)
    );

    let encoded = cursor.encode();
    assert_eq!(encoded, "1725000000_10");

    let decoded = PageCursor::decode(&encoded).expect("valid cursor string");
    assert_eq!(decoded.offset, 10);
    assert_eq!(decoded.generation, generation);
}
