//! Tests for the bid-match helpers in `crate::bid`.
//!
//! Covers the public match surface used by `accept_bid` and the bid listing
//! pipeline: [`crate::bid::BidStorage::compare_bids`],
//! [`crate::bid::BidStorage::get_best_bid`], and
//! [`crate::bid::BidStorage::rank_bids`].
//!
//! These helpers together produce the deterministic "best bid" for an
//! invoice. The [`crate::bid::BidStorage::select_best_placed_bid`] and
//! [`crate::bid::BidStorage::select_best_index`] helpers are exercised
//! indirectly through `get_best_bid` and `rank_bids` respectively.
//!
//! # Organisation
//!
//! - **Match (happy path)**: ranking resolves to the highest-profit placed
//!   bid, with each of the five comparator tiers (`profit`, `expected_return`,
//!   `bid_amount`, `timestamp`, `bid_id`) selecting the right winner.
//! - **Mismatch (sad path)**: empty invoice and all-terminal-status invoices
//!   produce `None` / empty output. Status filter silently excludes higher
//!   economic bids when they are not `Placed`.
//! - **Edge conditions**: single bid, zero values, negative profit via
//!   `saturating_sub`, full tie resolved by `bid_id` lexicographic order,
//!   determinism independent of insertion order.
//!
//! These tests are NOT gated behind `legacy-tests` or `fuzz-tests` so they
//! run on every CI matrix entry. The fuzz-test property suite for
//! [`crate::bid::BidStorage::compare_bids`] total-order axioms is in
//! `test_bid_compare_order_props` (fuzz-tests feature).
//!
//! Determinism: no wall-clock inputs; timestamps are set explicitly via
//! `Env::ledger()`.
//!
//! See Issue #2083.

#![cfg(test)]

use crate::bid::{Bid, BidStatus, BidStorage};
use core::cmp::Ordering;
use soroban_sdk::{testutils::Ledger, Address, BytesN, Env};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn invoice_id(env: &Env, seed: u8) -> BytesN<32> {
    let mut bytes = [0u8; 32];
    bytes[0] = seed;
    BytesN::from_array(env, &bytes)
}

fn build_bid(
    env: &Env,
    invoice: &BytesN<32>,
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
        invoice_id: invoice.clone(),
        investor: investor.clone(),
        bid_amount,
        expected_return,
        timestamp,
        status,
        expiration_timestamp: timestamp.saturating_add(604_800),
    }
}

fn persist_bid(env: &Env, bid: &Bid) {
    BidStorage::store_bid(env, bid);
    BidStorage::add_bid_to_invoice(env, &bid.invoice_id, &bid.bid_id);
}

// ===========================================================================
// compare_bids — match (happy path)
// ===========================================================================

/// Tier 1: higher-profit bid ranks higher.
#[test]
fn compare_bids_prefers_higher_profit_when_profit_differs() {
    let env = Env::default();
    let invoice = invoice_id(&env, 1);
    let investor = Address::generate(&env);
    let a = build_bid(&env, &invoice, &investor, 1_000, 2_000, 100, BidStatus::Placed, 1);
    let b = build_bid(&env, &invoice, &investor, 1_000, 3_000, 100, BidStatus::Placed, 2);

    assert_eq!(BidStorage::compare_bids(&a, &b), Ordering::Less);
    assert_eq!(BidStorage::compare_bids(&b, &a), Ordering::Greater);
}

/// Tier 2: when profit ties, higher expected_return ranks higher.
#[test]
fn compare_bids_breaks_profit_tie_with_expected_return() {
    let env = Env::default();
    let invoice = invoice_id(&env, 2);
    let investor = Address::generate(&env);
    // Both have profit = 1_000.
    let lower = build_bid(&env, &invoice, &investor, 1_000, 2_000, 100, BidStatus::Placed, 1);
    let higher = build_bid(&env, &invoice, &investor, 1_500, 2_500, 100, BidStatus::Placed, 2);

    assert_eq!(BidStorage::compare_bids(&lower, &higher), Ordering::Less);
    assert_eq!(BidStorage::compare_bids(&higher, &lower), Ordering::Greater);
}

/// Tier 3: when profit and expected_return tie, higher bid_amount ranks higher.
///
/// Tier 3 is only reachable when profit ties AND expected_return ties, which
/// mathematically forces bid_amount equality — UNLESS profit ties are reached
/// via `saturating_sub` clamping negative profits to zero. With both profits
/// clamped to 0 (because `bid_amount >= expected_return`), equal expected
/// returns allow differing bid amounts to reach tier 3.
#[test]
fn compare_bids_breaks_with_bid_amount_when_profit_and_return_tie() {
    let env = Env::default();
    let invoice = invoice_id(&env, 3);
    let investor = Address::generate(&env);
    // Both bids have profit clamped to 0 via saturating_sub (return <= bid_amount);
    // expected_return is identical so tier 2 ties and tier 3 (bid_amount) decides.
    let smaller_amount = build_bid(&env, &invoice, &investor, 1_000, 1_000, 100, BidStatus::Placed, 1);
    let larger_amount = build_bid(&env, &invoice, &investor, 1_500, 1_000, 100, BidStatus::Placed, 2);

    assert_eq!(
        BidStorage::compare_bids(&smaller_amount, &larger_amount),
        Ordering::Less
    );
    assert_eq!(
        BidStorage::compare_bids(&larger_amount, &smaller_amount),
        Ordering::Greater
    );
}

/// Tier 4: when economics tie, newer timestamp ranks higher.
#[test]
fn compare_bids_prefers_newer_timestamp_on_economic_tie() {
    let env = Env::default();
    let invoice = invoice_id(&env, 4);
    let investor = Address::generate(&env);
    let older = build_bid(&env, &invoice, &investor, 1_000, 2_000, 100, BidStatus::Placed, 1);
    let newer = build_bid(&env, &invoice, &investor, 1_000, 2_000, 200, BidStatus::Placed, 2);

    assert_eq!(BidStorage::compare_bids(&older, &newer), Ordering::Less);
    assert_eq!(BidStorage::compare_bids(&newer, &older), Ordering::Greater);
}

/// Tier 5: at a full economic + temporal tie, higher lexicographic bid_id wins.
#[test]
fn compare_bids_uses_bid_id_at_full_tie() {
    let env = Env::default();
    let invoice = invoice_id(&env, 5);
    let investor = Address::generate(&env);
    let lower_id = build_bid(&env, &invoice, &investor, 1_000, 2_000, 100, BidStatus::Placed, 1);
    let higher_id = build_bid(&env, &invoice, &investor, 1_000, 2_000, 100, BidStatus::Placed, 9);

    assert_eq!(BidStorage::compare_bids(&lower_id, &higher_id), Ordering::Less);
    assert_eq!(BidStorage::compare_bids(&higher_id, &lower_id), Ordering::Greater);
}

// ===========================================================================
// compare_bids — mismatch / identity
// ===========================================================================

/// Boundary: identical bids compare Equal (antisymmetry + reflexivity).
#[test]
fn compare_bids_returns_equal_for_identical_bids() {
    let env = Env::default();
    let invoice = invoice_id(&env, 6);
    let investor = Address::generate(&env);
    let a = build_bid(&env, &invoice, &investor, 1_000, 2_000, 100, BidStatus::Placed, 1);
    let b = build_bid(&env, &invoice, &investor, 1_000, 2_000, 100, BidStatus::Placed, 1);

    assert_eq!(BidStorage::compare_bids(&a, &b), Ordering::Equal);
    assert_eq!(BidStorage::compare_bids(&b, &a), Ordering::Equal);
}

/// Comparator is invariant over `status` field — only economics + metadata.
#[test]
fn compare_bids_ignores_status_field() {
    let env = Env::default();
    let invoice = invoice_id(&env, 7);
    let investor = Address::generate(&env);
    let placed = build_bid(&env, &invoice, &investor, 1_000, 2_000, 100, BidStatus::Placed, 1);
    let cancelled = build_bid(&env, &invoice, &investor, 1_000, 2_000, 100, BidStatus::Cancelled, 1);
    let accepted = build_bid(&env, &invoice, &investor, 1_000, 2_000, 100, BidStatus::Accepted, 1);

    assert_eq!(BidStorage::compare_bids(&placed, &cancelled), Ordering::Equal);
    assert_eq!(BidStorage::compare_bids(&placed, &accepted), Ordering::Equal);
    assert_eq!(BidStorage::compare_bids(&cancelled, &accepted), Ordering::Equal);
}

// ===========================================================================
// compare_bids — edge conditions
// ===========================================================================

/// Negative profit (expected_return < bid_amount) clamped to 0 by saturating_sub.
#[test]
fn compare_bids_handles_clamped_negative_profit() {
    let env = Env::default();
    let invoice = invoice_id(&env, 8);
    let investor = Address::generate(&env);
    // profit(a) = 500 - 1_000 -> saturating_sub -> 0.
    let a = build_bid(&env, &invoice, &investor, 1_000, 500, 100, BidStatus::Placed, 1);
    // profit(b) = 3_000 - 2_000 = 1_000.
    let b = build_bid(&env, &invoice, &investor, 2_000, 3_000, 100, BidStatus::Placed, 2);

    assert_eq!(BidStorage::compare_bids(&a, &b), Ordering::Less);
    assert_eq!(BidStorage::compare_bids(&b, &a), Ordering::Greater);
}

/// Zero-economics and zero-timestamp bids are still total-ordered by bid_id.
#[test]
fn compare_bids_handles_zero_values() {
    let env = Env::default();
    let invoice = invoice_id(&env, 9);
    let investor = Address::generate(&env);
    let zero_all = build_bid(&env, &invoice, &investor, 0, 0, 0, BidStatus::Placed, 1);
    let nonzero_return = build_bid(&env, &invoice, &investor, 0, 1, 0, BidStatus::Placed, 2);

    assert_eq!(BidStorage::compare_bids(&zero_all, &nonzero_return), Ordering::Less);
    assert_eq!(BidStorage::compare_bids(&nonzero_return, &zero_all), Ordering::Greater);
}

/// Tier dominance holds at extremes: massive values do not overflow.
/// `saturating_sub` clamps negative profit; comparison stays total.
#[test]
fn compare_bids_handles_large_economics_without_panicking() {
    let env = Env::default();
    let invoice = invoice_id(&env, 10);
    let investor = Address::generate(&env);
    let huge = build_bid(
        &env,
        &invoice,
        &investor,
        i128::MAX / 2,
        i128::MAX / 2 + 1,
        u64::MAX / 2,
        BidStatus::Placed,
        1,
    );
    let small = build_bid(&env, &invoice, &investor, 1, 2, 100, BidStatus::Placed, 2);

    assert_eq!(BidStorage::compare_bids(&huge, &small), Ordering::Greater);
    assert_eq!(BidStorage::compare_bids(&small, &huge), Ordering::Less);
}

// ===========================================================================
// get_best_bid — match (happy path)
// ===========================================================================

/// Best bid is the placed bid with the highest profit.
#[test]
fn get_best_bid_returns_highest_profit_placed_bid() {
    let env = Env::default();
    env.ledger().with_mut(|li| li.timestamp = 1_000);
    let invoice = invoice_id(&env, 20);

    let inv1 = Address::generate(&env);
    let inv2 = Address::generate(&env);
    let inv3 = Address::generate(&env);
    let low = build_bid(&env, &invoice, &inv1, 1_000, 1_500, 50, BidStatus::Placed, 1); // profit 500
    let mid = build_bid(&env, &invoice, &inv2, 1_000, 2_500, 60, BidStatus::Placed, 2); // profit 1500
    let high = build_bid(&env, &invoice, &inv3, 1_000, 3_500, 70, BidStatus::Placed, 3); // profit 2500
    persist_bid(&env, &low);
    persist_bid(&env, &mid);
    persist_bid(&env, &high);

    let best = BidStorage::get_best_bid(&env, &invoice).unwrap();
    assert_eq!(best.bid_id, high.bid_id);
}

// ===========================================================================
// get_best_bid — mismatch (sad path)
// ===========================================================================

/// Empty invoice: no best bid.
#[test]
fn get_best_bid_returns_none_for_empty_invoice() {
    let env = Env::default();
    let invoice = invoice_id(&env, 21);
    let best = BidStorage::get_best_bid(&env, &invoice);
    assert!(best.is_none(), "empty invoice must yield no best bid");
}

/// All bids in terminal states: get_best_bid returns None
/// (status filter removes Cancelled/Withdrawn/Accepted/Expired).
#[test]
fn get_best_bid_returns_none_when_only_terminal_status_bids_exist() {
    let env = Env::default();
    env.ledger().with_mut(|li| li.timestamp = 1_000);
    let invoice = invoice_id(&env, 22);

    let investor = Address::generate(&env);
    let cancelled = build_bid(&env, &invoice, &investor, 1_000, 2_000, 100, BidStatus::Cancelled, 1);
    let withdrawn = build_bid(&env, &invoice, &investor, 1_000, 2_500, 100, BidStatus::Withdrawn, 2);
    let accepted = build_bid(&env, &invoice, &investor, 1_000, 3_000, 100, BidStatus::Accepted, 3);
    let expired = build_bid(&env, &invoice, &investor, 1_000, 3_500, 100, BidStatus::Expired, 4);
    persist_bid(&env, &cancelled);
    persist_bid(&env, &withdrawn);
    persist_bid(&env, &accepted);
    persist_bid(&env, &expired);

    let best = BidStorage::get_best_bid(&env, &invoice);
    assert!(
        best.is_none(),
        "no Placed bids means no best bid, regardless of terminal economics"
    );
}

/// Status filter trumps economic ranking: a higher-profit Cancelled bid is
/// rejected; the lowest-profit Placed bid wins.
#[test]
fn get_best_bid_prefers_placed_over_higher_economics_terminal_bid() {
    let env = Env::default();
    env.ledger().with_mut(|li| li.timestamp = 1_000);
    let invoice = invoice_id(&env, 23);

    let inv1 = Address::generate(&env);
    let inv2 = Address::generate(&env);
    let placed_low = build_bid(&env, &invoice, &inv1, 1_000, 1_500, 100, BidStatus::Placed, 1);
    let cancelled_high = build_bid(&env, &invoice, &inv2, 1_000, 9_999, 100, BidStatus::Cancelled, 2);
    persist_bid(&env, &placed_low);
    persist_bid(&env, &cancelled_high);

    let best = BidStorage::get_best_bid(&env, &invoice).unwrap();
    assert_eq!(
        best.bid_id, placed_low.bid_id,
        "status filter must exclude Cancelled regardless of economics"
    );
}

// ===========================================================================
// get_best_bid — edge conditions
// ===========================================================================

/// Single placed bid is the best bid by definition.
#[test]
fn get_best_bid_returns_single_bid_when_only_one_is_placed() {
    let env = Env::default();
    env.ledger().with_mut(|li| li.timestamp = 1_000);
    let invoice = invoice_id(&env, 24);
    let investor = Address::generate(&env);
    let only = build_bid(&env, &invoice, &investor, 1_000, 2_000, 100, BidStatus::Placed, 1);
    persist_bid(&env, &only);

    let best = BidStorage::get_best_bid(&env, &invoice).unwrap();
    assert_eq!(best.bid_id, only.bid_id);
}

/// After every placed bid expires and cleanup runs, get_best_bid returns None.
#[test]
fn get_best_bid_returns_none_after_all_placed_bids_expire_and_cleanup_runs() {
    let env = Env::default();
    env.ledger().with_mut(|li| li.timestamp = 100);
    let invoice = invoice_id(&env, 25);

    let inv1 = Address::generate(&env);
    let inv2 = Address::generate(&env);
    let mut bid1 = build_bid(&env, &invoice, &inv1, 1_000, 2_000, 100, BidStatus::Placed, 1);
    let mut bid2 = build_bid(&env, &invoice, &inv2, 1_000, 3_000, 100, BidStatus::Placed, 2);
    // Force expiration just after the current ledger timestamp.
    bid1.expiration_timestamp = 101;
    bid2.expiration_timestamp = 101;
    persist_bid(&env, &bid1);
    persist_bid(&env, &bid2);

    // Advance ledger past expiration and trigger cleanup.
    env.ledger().with_mut(|li| li.timestamp = 200);
    BidStorage::cleanup_expired_bids(&env, &invoice);

    let best = BidStorage::get_best_bid(&env, &invoice);
    assert!(
        best.is_none(),
        "all bids expired and removed from index -> no best bid"
    );
}

// ===========================================================================
// rank_bids — match (happy path)
// ===========================================================================

/// Ranking produces a strict descending order by profit.
#[test]
fn rank_bids_orders_by_profit_descending() {
    let env = Env::default();
    env.ledger().with_mut(|li| li.timestamp = 1_000);
    let invoice = invoice_id(&env, 30);

    let inv1 = Address::generate(&env);
    let inv2 = Address::generate(&env);
    let inv3 = Address::generate(&env);
    let low = build_bid(&env, &invoice, &inv1, 1_000, 1_500, 100, BidStatus::Placed, 1);
    let mid = build_bid(&env, &invoice, &inv2, 1_000, 2_500, 200, BidStatus::Placed, 2);
    let high = build_bid(&env, &invoice, &inv3, 1_000, 3_500, 300, BidStatus::Placed, 3);
    persist_bid(&env, &low);
    persist_bid(&env, &mid);
    persist_bid(&env, &high);

    let ranked = BidStorage::rank_bids(&env, &invoice);
    assert_eq!(ranked.len(), 3);
    assert_eq!(ranked.get(0).unwrap().bid_id, high.bid_id);
    assert_eq!(ranked.get(1).unwrap().bid_id, mid.bid_id);
    assert_eq!(ranked.get(2).unwrap().bid_id, low.bid_id);
}

/// Invariant: `rank_bids(...)[0]` always equals `get_best_bid(...)`.
#[test]
fn rank_bids_first_index_matches_get_best_bid() {
    let env = Env::default();
    env.ledger().with_mut(|li| li.timestamp = 1_000);
    let invoice = invoice_id(&env, 31);

    let inv_a = Address::generate(&env);
    let inv_b = Address::generate(&env);
    let inv_c = Address::generate(&env);
    let inv_d = Address::generate(&env);
    let placed_a = build_bid(&env, &invoice, &inv_a, 1_000, 2_500, 100, BidStatus::Placed, 1);
    let placed_b = build_bid(&env, &invoice, &inv_b, 1_000, 1_500, 100, BidStatus::Placed, 2);
    let cancelled = build_bid(&env, &invoice, &inv_c, 1_000, 9_500, 100, BidStatus::Cancelled, 3);
    let expired = build_bid(&env, &invoice, &inv_d, 1_000, 9_500, 100, BidStatus::Expired, 4);

    persist_bid(&env, &cancelled);
    persist_bid(&env, &placed_b);
    persist_bid(&env, &expired);
    persist_bid(&env, &placed_a);

    let ranked = BidStorage::rank_bids(&env, &invoice);
    assert_eq!(ranked.len(), 2);
    let best = BidStorage::get_best_bid(&env, &invoice).unwrap();
    assert_eq!(best.bid_id, ranked.get(0).unwrap().bid_id);
}

// ===========================================================================
// rank_bids — mismatch (sad path)
// ===========================================================================

/// Empty invoice: rank produces an empty vec.
#[test]
fn rank_bids_returns_empty_for_empty_invoice() {
    let env = Env::default();
    let invoice = invoice_id(&env, 32);
    let ranked = BidStorage::rank_bids(&env, &invoice);
    assert!(ranked.is_empty(), "empty invoice must yield empty ranking");
}

/// All bids terminal: rank produces an empty vec.
#[test]
fn rank_bids_returns_empty_when_only_terminal_status_bids_exist() {
    let env = Env::default();
    env.ledger().with_mut(|li| li.timestamp = 1_000);
    let invoice = invoice_id(&env, 33);

    let investor = Address::generate(&env);
    let cancelled = build_bid(&env, &invoice, &investor, 1_000, 2_000, 100, BidStatus::Cancelled, 1);
    let withdrawn = build_bid(&env, &invoice, &investor, 2_000, 3_000, 100, BidStatus::Withdrawn, 2);
    let accepted = build_bid(&env, &invoice, &investor, 3_000, 4_000, 100, BidStatus::Accepted, 3);
    persist_bid(&env, &cancelled);
    persist_bid(&env, &withdrawn);
    persist_bid(&env, &accepted);

    let ranked = BidStorage::rank_bids(&env, &invoice);
    assert!(ranked.is_empty(), "no Placed bids means no ranked bids");
}

/// Status filter: Terminal bids are excluded even when present in the index.
#[test]
fn rank_bids_excludes_terminal_statuses_around_placed_bids() {
    let env = Env::default();
    env.ledger().with_mut(|li| li.timestamp = 1_000);
    let invoice = invoice_id(&env, 34);

    let inv1 = Address::generate(&env);
    let inv2 = Address::generate(&env);
    let placed = build_bid(&env, &invoice, &inv1, 1_000, 2_000, 100, BidStatus::Placed, 1);
    let cancelled = build_bid(&env, &invoice, &inv2, 1_000, 9_999, 100, BidStatus::Cancelled, 2);
    persist_bid(&env, &placed);
    persist_bid(&env, &cancelled);

    let ranked = BidStorage::rank_bids(&env, &invoice);
    assert_eq!(ranked.len(), 1);
    assert_eq!(ranked.get(0).unwrap().bid_id, placed.bid_id);
}

// ===========================================================================
// rank_bids — edge conditions
// ===========================================================================

/// Single placed bid: ranking contains exactly one entry.
#[test]
fn rank_bids_handles_single_placed_bid() {
    let env = Env::default();
    env.ledger().with_mut(|li| li.timestamp = 1_000);
    let invoice = invoice_id(&env, 35);
    let investor = Address::generate(&env);
    let only = build_bid(&env, &invoice, &investor, 1_000, 2_000, 100, BidStatus::Placed, 1);
    persist_bid(&env, &only);

    let ranked = BidStorage::rank_bids(&env, &invoice);
    assert_eq!(ranked.len(), 1);
    assert_eq!(ranked.get(0).unwrap().bid_id, only.bid_id);
}

/// Same set of bids inserted in different orders produces the same ranking.
#[test]
fn rank_bids_is_deterministic_across_insertion_orders() {
    let build = |env: &Env, invoice: &BytesN<32>| {
        let inv1 = Address::generate(env);
        let inv2 = Address::generate(env);
        let inv3 = Address::generate(env);
        (
            build_bid(env, invoice, &inv1, 1_000, 2_000, 100, BidStatus::Placed, 1),
            build_bid(env, invoice, &inv2, 1_000, 3_000, 100, BidStatus::Placed, 2),
            build_bid(env, invoice, &inv3, 1_000, 4_000, 100, BidStatus::Placed, 3),
        )
    };

    let env_a = Env::default();
    env_a.ledger().with_mut(|li| li.timestamp = 1_000);
    let invoice_a = invoice_id(&env_a, 36);
    let (a1, a2, a3) = build(&env_a, &invoice_a);
    persist_bid(&env_a, &a1);
    persist_bid(&env_a, &a2);
    persist_bid(&env_a, &a3);
    let ranked_a: Vec<BytesN<32>> = BidStorage::rank_bids(&env_a, &invoice_a)
        .iter()
        .map(|b| b.bid_id)
        .collect();

    let env_b = Env::default();
    env_b.ledger().with_mut(|li| li.timestamp = 1_000);
    let invoice_b = invoice_id(&env_b, 37);
    let (b1, b2, b3) = build(&env_b, &invoice_b);
    persist_bid(&env_b, &b3);
    persist_bid(&env_b, &b1);
    persist_bid(&env_b, &b2);
    let ranked_b: Vec<BytesN<32>> = BidStorage::rank_bids(&env_b, &invoice_b)
        .iter()
        .map(|b| b.bid_id)
        .collect();

    assert_eq!(ranked_a.len(), ranked_b.len());
    for i in 0..ranked_a.len() {
        assert_eq!(ranked_a.get(i).unwrap(), ranked_b.get(i).unwrap());
    }
}

/// Full economic + temporal tie resolved by `bid_id` byte order.
#[test]
fn rank_bids_resolves_full_tie_via_bid_id_lexicographic() {
    let env = Env::default();
    env.ledger().with_mut(|li| li.timestamp = 1_000);
    let invoice = invoice_id(&env, 38);
    let investor = Address::generate(&env);

    let lower_id = build_bid(&env, &invoice, &investor, 1_000, 2_000, 100, BidStatus::Placed, 1);
    let higher_id = build_bid(&env, &invoice, &investor, 1_000, 2_000, 100, BidStatus::Placed, 9);
    persist_bid(&env, &lower_id);
    persist_bid(&env, &higher_id);

    let ranked = BidStorage::rank_bids(&env, &invoice);
    assert_eq!(ranked.get(0).unwrap().bid_id, higher_id.bid_id);
    assert_eq!(ranked.get(1).unwrap().bid_id, lower_id.bid_id);
}

/// Combined check: ranking walks each of the five tiebreaker tiers in order,
/// making each adjacent pair resolved by a different tier.
///
/// Tier 3 (bid_amount) is exercised via saturated profit == 0 cases
/// (expected_return ≤ bid_amount), so profit and return tie while amounts differ.
#[test]
fn rank_bids_resolves_each_of_five_tiebreaker_tiers() {
    let env = Env::default();
    env.ledger().with_mut(|li| li.timestamp = 1_000);
    let invoice = invoice_id(&env, 39);
    let investor = Address::generate(&env);

    // Tier 1 winner: profit = 4_000 (highest, distinctly above the rest).
    let high_profit = build_bid(&env, &invoice, &investor, 1_000, 5_000, 100, BidStatus::Placed, 1);
    // Tier 2 winner: lower profit than high_profit, higher return than the
    // four tier-3+ bids. Loses to high_profit on tier 1, wins the rest on tier 2.
    let higher_return = build_bid(&env, &invoice, &investor, 1_000, 3_000, 100, BidStatus::Placed, 2);
    // Tier 3 loser (lower amount): profit clamps to 0 via `saturating_sub`,
    // expected_return tied with tier3_high so only bid_amount can break the tie.
    let tier3_low = build_bid(&env, &invoice, &investor, 2_000, 500, 100, BidStatus::Placed, 3);
    // Tier 3 winner among the clamped pairs.
    let tier3_high = build_bid(&env, &invoice, &investor, 3_000, 500, 100, BidStatus::Placed, 4);
    // Tier 4 winner: matches tier3_high economically but newer timestamp breaks the tie at tier 4.
    let tier4_newer = build_bid(&env, &invoice, &investor, 3_000, 500, 200, BidStatus::Placed, 5);
    // Tier 5 winner: matches tier4_newer except higher bid_id suffix at byte 30/31.
    let tier5_higher_id = build_bid(&env, &invoice, &investor, 3_000, 500, 200, BidStatus::Placed, 9);

    // Insertion order is mixed deliberately: rank_bids must be insertion-order invariant
    // thanks to selection sort via `select_best_index`.
    persist_bid(&env, &high_profit);
    persist_bid(&env, &higher_return);
    persist_bid(&env, &tier3_low);
    persist_bid(&env, &tier3_high);
    persist_bid(&env, &tier4_newer);
    persist_bid(&env, &tier5_higher_id);

    let ranked = BidStorage::rank_bids(&env, &invoice);
    assert_eq!(ranked.len(), 6);
    assert_eq!(ranked.get(0).unwrap().bid_id, high_profit.bid_id, "tier 1 winner");
    assert_eq!(ranked.get(1).unwrap().bid_id, higher_return.bid_id, "tier 2 winner");
    assert_eq!(ranked.get(2).unwrap().bid_id, tier5_higher_id.bid_id, "tier 5 winner");
    assert_eq!(ranked.get(3).unwrap().bid_id, tier4_newer.bid_id, "tier 4 loser to tier 5");
    assert_eq!(ranked.get(4).unwrap().bid_id, tier3_high.bid_id, "tier 3 winner over tier3_low");
    assert_eq!(ranked.get(5).unwrap().bid_id, tier3_low.bid_id, "tier 3 loser");
}

/// Ranking is order-sensitive so no adjacent inversions occur
/// (i.e. `ranked[i] >= ranked[i+1]` for all i).
#[test]
fn rank_bids_produces_no_adjacent_inversions() {
    let env = Env::default();
    env.ledger().with_mut(|li| li.timestamp = 1_000);
    let invoice = invoice_id(&env, 40);

    let investors: Vec<Address> = (0..6).map(|_| Address::generate(&env)).collect();
    let mut bids: Vec<Bid> = Vec::new();
    bids.push_back(build_bid(&env, &invoice, &investors[0], 1_000, 1_200, 50, BidStatus::Placed, 1));
    bids.push_back(build_bid(&env, &invoice, &investors[1], 1_000, 4_500, 80, BidStatus::Placed, 2));
    bids.push_back(build_bid(&env, &invoice, &investors[2], 2_000, 3_500, 70, BidStatus::Placed, 3));
    bids.push_back(build_bid(&env, &invoice, &investors[3], 1_500, 2_200, 60, BidStatus::Placed, 4));
    bids.push_back(build_bid(&env, &invoice, &investors[4], 500, 700, 90, BidStatus::Placed, 5));
    bids.push_back(build_bid(&env, &invoice, &investors[5], 1_000, 2_000, 65, BidStatus::Placed, 6));

    for b in bids.iter() {
        persist_bid(&env, b);
    }

    let ranked = BidStorage::rank_bids(&env, &invoice);
    assert_eq!(ranked.len() as u32, bids.len());

    let mut i: u32 = 0;
    while i + 1 < ranked.len() {
        let cur = ranked.get(i).unwrap();
        let next = ranked.get(i + 1).unwrap();
        let cmp = BidStorage::compare_bids(&cur, &next);
        assert!(
            matches!(cmp, Ordering::Greater | Ordering::Equal),
            "inversion at index {}: compare_bids returned {:?}",
            i,
            cmp
        );
        i = i.saturating_add(1);
    }
}
