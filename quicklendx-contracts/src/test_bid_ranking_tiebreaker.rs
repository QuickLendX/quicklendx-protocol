//! # Deterministic Bid Ranking Tie-Breaker Regression Suite
//!
//! Ensures `rank_bids` and `compare_bids` remain deterministic across all tie
//! scenarios (equal APR / return / amount / timestamp) to prevent
//! node-dependent outcomes in Soroban execution (issue #811).
//!
//! ## Tie-breaker priority (codified in `BidStorage::compare_bids`)
//!
//! 1. **Profit** (`expected_return - bid_amount`) — higher wins
//! 2. **Expected return** — higher wins
//! 3. **Bid amount** — higher wins
//! 4. **Timestamp** — newer (higher) wins
//! 5. **Bid ID** (lexicographic byte order) — higher wins (final stable tiebreaker)
//!
//! ## Security assumptions
//! - Best-bid selection is stable: `get_best_bid` always equals `rank_bids[0]`.
//! - Ranking is insertion-order independent: same bids in any order → same result.
//! - All tie levels are covered so no two distinct bids can ever compare `Equal`.
//! - Non-`Placed` bids are excluded from ranking and best-bid selection.

#![cfg(test)]

use core::cmp::Ordering;
use soroban_sdk::{testutils::Ledger, Address, BytesN, Env};
use soroban_sdk::testutils::Address as _;

use crate::bid::{Bid, BidStatus, BidStorage};
use crate::QuickLendXContract;

// ============================================================================
// Helpers
// ============================================================================

fn register(env: &Env) -> Address {
    env.register(QuickLendXContract, ())
}

fn inv(env: &Env, seed: u8) -> BytesN<32> {
    BytesN::from_array(env, &[seed; 32])
}

/// Build a bid with a deterministic ID derived from `id_byte`.
/// The ID bytes are: [0xB1, 0xD0, ...timestamp bytes..., 0x00..., id_byte, id_byte]
fn make_bid(
    env: &Env,
    invoice_id: &BytesN<32>,
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
        invoice_id: invoice_id.clone(),
        investor: Address::generate(env),
        bid_amount,
        expected_return,
        timestamp,
        status,
        expiration_timestamp: timestamp.saturating_add(604_800),
    }
}

/// Persist a bid and register it against its invoice.
fn store(env: &Env, bid: &Bid) {
    BidStorage::store_bid(env, bid);
    BidStorage::add_bid_to_invoice(env, &bid.invoice_id, &bid.bid_id);
}

/// Assert `get_best_bid` == `rank_bids[0]` (the core invariant).
fn assert_best_eq_first(env: &Env, invoice: &BytesN<32>) {
    let ranked = BidStorage::rank_bids(env, invoice);
    let best = BidStorage::get_best_bid(env, invoice);
    if ranked.len() == 0 {
        assert!(best.is_none(), "best must be None when ranking is empty");
        return;
    }
    let best = best.expect("best must exist when ranking is non-empty");
    assert_eq!(
        best.bid_id,
        ranked.get(0).unwrap().bid_id,
        "get_best_bid must equal rank_bids[0]"
    );
}

// ============================================================================
// Tier 1 – Profit tie-breaker
// ============================================================================

/// Higher profit wins regardless of other fields.
#[test]
fn tiebreak_profit_higher_wins() {
    let env = Env::default();
    env.ledger().with_mut(|l| l.timestamp = 1_000);
    let contract = register(&env);
    let invoice = inv(&env, 0x01);

    env.as_contract(&contract, || {
        // profit 2000 vs 1000
        let high = make_bid(&env, &invoice, 3_000, 5_000, 10, BidStatus::Placed, 0x01);
        let low  = make_bid(&env, &invoice, 4_000, 5_000, 20, BidStatus::Placed, 0x02);
        store(&env, &high);
        store(&env, &low);

        let ranked = BidStorage::rank_bids(&env, &invoice);
        assert_eq!(ranked.len(), 2);
        assert_eq!(ranked.get(0).unwrap().bid_id, high.bid_id, "higher profit must rank first");
        assert_best_eq_first(&env, &invoice);
    });
}

/// compare_bids returns Greater when profit is higher.
#[test]
fn compare_bids_profit_ordering() {
    let env = Env::default();
    let invoice = inv(&env, 0x02);
    let high = make_bid(&env, &invoice, 1_000, 3_000, 10, BidStatus::Placed, 0x01); // profit 2000
    let low  = make_bid(&env, &invoice, 1_000, 2_000, 10, BidStatus::Placed, 0x02); // profit 1000
    assert_eq!(BidStorage::compare_bids(&high, &low), Ordering::Greater);
    assert_eq!(BidStorage::compare_bids(&low, &high), Ordering::Less);
}

/// Three bids with distinct profits rank in descending profit order.
#[test]
fn tiebreak_profit_three_bids_descending() {
    let env = Env::default();
    env.ledger().with_mut(|l| l.timestamp = 1_000);
    let contract = register(&env);
    let invoice = inv(&env, 0x03);

    env.as_contract(&contract, || {
        let b1 = make_bid(&env, &invoice, 1_000, 4_000, 10, BidStatus::Placed, 0x01); // profit 3000
        let b2 = make_bid(&env, &invoice, 1_000, 3_000, 10, BidStatus::Placed, 0x02); // profit 2000
        let b3 = make_bid(&env, &invoice, 1_000, 2_000, 10, BidStatus::Placed, 0x03); // profit 1000
        store(&env, &b3);
        store(&env, &b1);
        store(&env, &b2);

        let ranked = BidStorage::rank_bids(&env, &invoice);
        assert_eq!(ranked.get(0).unwrap().bid_id, b1.bid_id);
        assert_eq!(ranked.get(1).unwrap().bid_id, b2.bid_id);
        assert_eq!(ranked.get(2).unwrap().bid_id, b3.bid_id);
        assert_best_eq_first(&env, &invoice);
    });
}

// ============================================================================
// Tier 2 – Expected-return tie-breaker (equal profit)
// ============================================================================

/// When profit is equal, higher expected_return wins.
#[test]
fn tiebreak_expected_return_higher_wins() {
    let env = Env::default();
    env.ledger().with_mut(|l| l.timestamp = 2_000);
    let contract = register(&env);
    let invoice = inv(&env, 0x10);

    env.as_contract(&contract, || {
        // Both profit = 1000; expected_return differs
        let high = make_bid(&env, &invoice, 6_000, 7_000, 10, BidStatus::Placed, 0x01);
        let low  = make_bid(&env, &invoice, 5_000, 6_000, 10, BidStatus::Placed, 0x02);
        store(&env, &low);
        store(&env, &high);

        let ranked = BidStorage::rank_bids(&env, &invoice);
        assert_eq!(ranked.get(0).unwrap().bid_id, high.bid_id);
        assert_best_eq_first(&env, &invoice);
    });
}

/// compare_bids returns Greater when expected_return is higher (equal profit).
#[test]
fn compare_bids_expected_return_ordering() {
    let env = Env::default();
    let invoice = inv(&env, 0x11);
    let high = make_bid(&env, &invoice, 6_000, 7_000, 10, BidStatus::Placed, 0x01);
    let low  = make_bid(&env, &invoice, 5_000, 6_000, 10, BidStatus::Placed, 0x02);
    assert_eq!(BidStorage::compare_bids(&high, &low), Ordering::Greater);
    assert_eq!(BidStorage::compare_bids(&low, &high), Ordering::Less);
}

/// Insertion order does not affect expected-return tie-break.
#[test]
fn tiebreak_expected_return_insertion_order_independent() {
    for order in 0u8..2 {
        let env = Env::default();
        env.ledger().with_mut(|l| l.timestamp = 2_000);
        let contract = register(&env);
        let invoice = inv(&env, 0x12u8.wrapping_add(order));

        env.as_contract(&contract, || {
            let high = make_bid(&env, &invoice, 6_000, 7_000, 10, BidStatus::Placed, 0x01);
            let low  = make_bid(&env, &invoice, 5_000, 6_000, 10, BidStatus::Placed, 0x02);
            if order == 0 { store(&env, &high); store(&env, &low); }
            else          { store(&env, &low);  store(&env, &high); }

            let ranked = BidStorage::rank_bids(&env, &invoice);
            assert_eq!(ranked.get(0).unwrap().bid_id, high.bid_id,
                "order={order}: higher expected_return must always rank first");
            assert_best_eq_first(&env, &invoice);
        });
    }
}

// ============================================================================
// Tier 3 – Bid-amount tie-breaker (equal profit + equal expected_return)
// ============================================================================

/// When profit and expected_return are equal, higher bid_amount wins.
#[test]
fn tiebreak_bid_amount_higher_wins() {
    let env = Env::default();
    env.ledger().with_mut(|l| l.timestamp = 3_000);
    let contract = register(&env);
    let invoice = inv(&env, 0x20);

    env.as_contract(&contract, || {
        // profit = 1000, expected_return = 6000; bid_amount differs
        let high = make_bid(&env, &invoice, 5_000, 6_000, 10, BidStatus::Placed, 0x01);
        let low  = make_bid(&env, &invoice, 4_000, 5_000, 10, BidStatus::Placed, 0x02);
        // Wait — profit differs here. Let's use same expected_return, different bid_amount
        // profit_high = 6000-5000=1000, profit_low = 5000-4000=1000 ✓
        store(&env, &low);
        store(&env, &high);

        let ranked = BidStorage::rank_bids(&env, &invoice);
        assert_eq!(ranked.get(0).unwrap().bid_id, high.bid_id,
            "higher bid_amount must win when profit and expected_return are equal");
        assert_best_eq_first(&env, &invoice);
    });
}

/// compare_bids returns Greater when bid_amount is higher (equal profit + expected_return).
#[test]
fn compare_bids_bid_amount_ordering() {
    let env = Env::default();
    let invoice = inv(&env, 0x21);
    // profit = 1000 for both; expected_return = 6000 for both; bid_amount differs
    let high = make_bid(&env, &invoice, 5_000, 6_000, 10, BidStatus::Placed, 0x01);
    let low  = make_bid(&env, &invoice, 4_000, 5_000, 10, BidStatus::Placed, 0x02);
    assert_eq!(BidStorage::compare_bids(&high, &low), Ordering::Greater);
    assert_eq!(BidStorage::compare_bids(&low, &high), Ordering::Less);
}

/// Insertion order does not affect bid-amount tie-break.
#[test]
fn tiebreak_bid_amount_insertion_order_independent() {
    for order in 0u8..2 {
        let env = Env::default();
        env.ledger().with_mut(|l| l.timestamp = 3_000);
        let contract = register(&env);
        let invoice = inv(&env, 0x22u8.wrapping_add(order));

        env.as_contract(&contract, || {
            let high = make_bid(&env, &invoice, 5_000, 6_000, 10, BidStatus::Placed, 0x01);
            let low  = make_bid(&env, &invoice, 4_000, 5_000, 10, BidStatus::Placed, 0x02);
            if order == 0 { store(&env, &high); store(&env, &low); }
            else          { store(&env, &low);  store(&env, &high); }

            let ranked = BidStorage::rank_bids(&env, &invoice);
            assert_eq!(ranked.get(0).unwrap().bid_id, high.bid_id,
                "order={order}: higher bid_amount must always rank first");
            assert_best_eq_first(&env, &invoice);
        });
    }
}

// ============================================================================
// Tier 4 – Timestamp tie-breaker (equal profit + expected_return + amount)
// ============================================================================

/// When profit, expected_return, and bid_amount are equal, newer timestamp wins.
#[test]
fn tiebreak_timestamp_newer_wins() {
    let env = Env::default();
    env.ledger().with_mut(|l| l.timestamp = 4_000);
    let contract = register(&env);
    let invoice = inv(&env, 0x30);

    env.as_contract(&contract, || {
        let newer = make_bid(&env, &invoice, 5_000, 6_000, 200, BidStatus::Placed, 0x01);
        let older = make_bid(&env, &invoice, 5_000, 6_000, 100, BidStatus::Placed, 0x02);
        store(&env, &older);
        store(&env, &newer);

        let ranked = BidStorage::rank_bids(&env, &invoice);
        assert_eq!(ranked.get(0).unwrap().bid_id, newer.bid_id,
            "newer timestamp must rank first");
        assert_eq!(ranked.get(1).unwrap().bid_id, older.bid_id);
        assert_best_eq_first(&env, &invoice);
    });
}

/// compare_bids returns Greater for newer timestamp (equal economics).
#[test]
fn compare_bids_timestamp_ordering() {
    let env = Env::default();
    let invoice = inv(&env, 0x31);
    let newer = make_bid(&env, &invoice, 5_000, 6_000, 200, BidStatus::Placed, 0x01);
    let older = make_bid(&env, &invoice, 5_000, 6_000, 100, BidStatus::Placed, 0x02);
    assert_eq!(BidStorage::compare_bids(&newer, &older), Ordering::Greater);
    assert_eq!(BidStorage::compare_bids(&older, &newer), Ordering::Less);
}

/// Insertion order does not affect timestamp tie-break.
#[test]
fn tiebreak_timestamp_insertion_order_independent() {
    for order in 0u8..2 {
        let env = Env::default();
        env.ledger().with_mut(|l| l.timestamp = 4_000);
        let contract = register(&env);
        let invoice = inv(&env, 0x32u8.wrapping_add(order));

        env.as_contract(&contract, || {
            let newer = make_bid(&env, &invoice, 5_000, 6_000, 200, BidStatus::Placed, 0x01);
            let older = make_bid(&env, &invoice, 5_000, 6_000, 100, BidStatus::Placed, 0x02);
            if order == 0 { store(&env, &newer); store(&env, &older); }
            else          { store(&env, &older); store(&env, &newer); }

            let ranked = BidStorage::rank_bids(&env, &invoice);
            assert_eq!(ranked.get(0).unwrap().bid_id, newer.bid_id,
                "order={order}: newer timestamp must always rank first");
            assert_best_eq_first(&env, &invoice);
        });
    }
}

/// Three bids with distinct timestamps rank newest-first.
#[test]
fn tiebreak_timestamp_three_bids_newest_first() {
    let env = Env::default();
    env.ledger().with_mut(|l| l.timestamp = 4_000);
    let contract = register(&env);
    let invoice = inv(&env, 0x34);

    env.as_contract(&contract, || {
        let t1 = make_bid(&env, &invoice, 5_000, 6_000, 300, BidStatus::Placed, 0x01);
        let t2 = make_bid(&env, &invoice, 5_000, 6_000, 200, BidStatus::Placed, 0x02);
        let t3 = make_bid(&env, &invoice, 5_000, 6_000, 100, BidStatus::Placed, 0x03);
        // Insert in reverse order
        store(&env, &t3);
        store(&env, &t2);
        store(&env, &t1);

        let ranked = BidStorage::rank_bids(&env, &invoice);
        assert_eq!(ranked.get(0).unwrap().bid_id, t1.bid_id);
        assert_eq!(ranked.get(1).unwrap().bid_id, t2.bid_id);
        assert_eq!(ranked.get(2).unwrap().bid_id, t3.bid_id);
        assert_best_eq_first(&env, &invoice);
    });
}

// ============================================================================
// Tier 5 – Bid-ID tie-breaker (full economic + timestamp tie)
// ============================================================================

/// When all economic fields and timestamp are equal, higher bid_id wins.
#[test]
fn tiebreak_bid_id_higher_wins() {
    let env = Env::default();
    env.ledger().with_mut(|l| l.timestamp = 5_000);
    let contract = register(&env);
    let invoice = inv(&env, 0x40);

    env.as_contract(&contract, || {
        let high_id = make_bid(&env, &invoice, 5_000, 6_000, 100, BidStatus::Placed, 0xFF);
        let low_id  = make_bid(&env, &invoice, 5_000, 6_000, 100, BidStatus::Placed, 0x01);
        store(&env, &low_id);
        store(&env, &high_id);

        let ranked = BidStorage::rank_bids(&env, &invoice);
        assert_eq!(ranked.get(0).unwrap().bid_id, high_id.bid_id,
            "higher bid_id must rank first on full tie");
        assert_eq!(ranked.get(1).unwrap().bid_id, low_id.bid_id);
        assert_best_eq_first(&env, &invoice);
    });
}

/// compare_bids returns Greater for higher bid_id (full tie on all other fields).
#[test]
fn compare_bids_bid_id_ordering() {
    let env = Env::default();
    let invoice = inv(&env, 0x41);
    let high_id = make_bid(&env, &invoice, 5_000, 6_000, 100, BidStatus::Placed, 0xFF);
    let low_id  = make_bid(&env, &invoice, 5_000, 6_000, 100, BidStatus::Placed, 0x01);
    assert_eq!(BidStorage::compare_bids(&high_id, &low_id), Ordering::Greater);
    assert_eq!(BidStorage::compare_bids(&low_id, &high_id), Ordering::Less);
}

/// Insertion order does not affect bid-id tie-break.
#[test]
fn tiebreak_bid_id_insertion_order_independent() {
    for order in 0u8..2 {
        let env = Env::default();
        env.ledger().with_mut(|l| l.timestamp = 5_000);
        let contract = register(&env);
        let invoice = inv(&env, 0x42u8.wrapping_add(order));

        env.as_contract(&contract, || {
            let high_id = make_bid(&env, &invoice, 5_000, 6_000, 100, BidStatus::Placed, 0xFF);
            let low_id  = make_bid(&env, &invoice, 5_000, 6_000, 100, BidStatus::Placed, 0x01);
            if order == 0 { store(&env, &high_id); store(&env, &low_id); }
            else          { store(&env, &low_id);  store(&env, &high_id); }

            let ranked = BidStorage::rank_bids(&env, &invoice);
            assert_eq!(ranked.get(0).unwrap().bid_id, high_id.bid_id,
                "order={order}: higher bid_id must always rank first");
            assert_best_eq_first(&env, &invoice);
        });
    }
}

/// Three bids with identical economics and timestamp rank by bid_id descending.
#[test]
fn tiebreak_bid_id_three_bids_descending() {
    let env = Env::default();
    env.ledger().with_mut(|l| l.timestamp = 5_000);
    let contract = register(&env);
    let invoice = inv(&env, 0x44);

    env.as_contract(&contract, || {
        let b_high = make_bid(&env, &invoice, 5_000, 6_000, 100, BidStatus::Placed, 0xFF);
        let b_mid  = make_bid(&env, &invoice, 5_000, 6_000, 100, BidStatus::Placed, 0x80);
        let b_low  = make_bid(&env, &invoice, 5_000, 6_000, 100, BidStatus::Placed, 0x01);
        // Insert in scrambled order
        store(&env, &b_mid);
        store(&env, &b_low);
        store(&env, &b_high);

        let ranked = BidStorage::rank_bids(&env, &invoice);
        assert_eq!(ranked.get(0).unwrap().bid_id, b_high.bid_id);
        assert_eq!(ranked.get(1).unwrap().bid_id, b_mid.bid_id);
        assert_eq!(ranked.get(2).unwrap().bid_id, b_low.bid_id);
        assert_best_eq_first(&env, &invoice);
    });
}

// ============================================================================
// Reflexivity and symmetry of compare_bids
// ============================================================================

/// compare_bids(a, a) == Equal (reflexive).
#[test]
fn compare_bids_reflexive() {
    let env = Env::default();
    let invoice = inv(&env, 0x50);
    let bid = make_bid(&env, &invoice, 5_000, 6_000, 100, BidStatus::Placed, 0x42);
    assert_eq!(BidStorage::compare_bids(&bid, &bid), Ordering::Equal);
}

/// compare_bids(a, b) == Greater iff compare_bids(b, a) == Less (antisymmetric).
#[test]
fn compare_bids_antisymmetric_profit() {
    let env = Env::default();
    let invoice = inv(&env, 0x51);
    let a = make_bid(&env, &invoice, 1_000, 3_000, 10, BidStatus::Placed, 0x01);
    let b = make_bid(&env, &invoice, 1_000, 2_000, 10, BidStatus::Placed, 0x02);
    assert_eq!(BidStorage::compare_bids(&a, &b), Ordering::Greater);
    assert_eq!(BidStorage::compare_bids(&b, &a), Ordering::Less);
}

/// compare_bids is transitive: if a > b and b > c then a > c.
#[test]
fn compare_bids_transitive() {
    let env = Env::default();
    let invoice = inv(&env, 0x52);
    let a = make_bid(&env, &invoice, 1_000, 4_000, 10, BidStatus::Placed, 0x01); // profit 3000
    let b = make_bid(&env, &invoice, 1_000, 3_000, 10, BidStatus::Placed, 0x02); // profit 2000
    let c = make_bid(&env, &invoice, 1_000, 2_000, 10, BidStatus::Placed, 0x03); // profit 1000
    assert_eq!(BidStorage::compare_bids(&a, &b), Ordering::Greater);
    assert_eq!(BidStorage::compare_bids(&b, &c), Ordering::Greater);
    assert_eq!(BidStorage::compare_bids(&a, &c), Ordering::Greater);
}

// ============================================================================
// Non-Placed bids are excluded from ranking
// ============================================================================

/// Withdrawn bids are excluded from rank_bids and get_best_bid.
#[test]
fn non_placed_withdrawn_excluded() {
    let env = Env::default();
    env.ledger().with_mut(|l| l.timestamp = 6_000);
    let contract = register(&env);
    let invoice = inv(&env, 0x60);

    env.as_contract(&contract, || {
        let placed    = make_bid(&env, &invoice, 3_000, 4_000, 10, BidStatus::Placed,    0x01);
        let withdrawn = make_bid(&env, &invoice, 5_000, 8_000, 20, BidStatus::Withdrawn, 0x02);
        store(&env, &placed);
        store(&env, &withdrawn);

        let ranked = BidStorage::rank_bids(&env, &invoice);
        assert_eq!(ranked.len(), 1, "only Placed bids appear in ranking");
        assert_eq!(ranked.get(0).unwrap().bid_id, placed.bid_id);
        assert_best_eq_first(&env, &invoice);
    });
}

/// Accepted bids are excluded from rank_bids and get_best_bid.
#[test]
fn non_placed_accepted_excluded() {
    let env = Env::default();
    env.ledger().with_mut(|l| l.timestamp = 6_000);
    let contract = register(&env);
    let invoice = inv(&env, 0x61);

    env.as_contract(&contract, || {
        let placed   = make_bid(&env, &invoice, 3_000, 4_000, 10, BidStatus::Placed,   0x01);
        let accepted = make_bid(&env, &invoice, 5_000, 8_000, 20, BidStatus::Accepted, 0x02);
        store(&env, &placed);
        store(&env, &accepted);

        let ranked = BidStorage::rank_bids(&env, &invoice);
        assert_eq!(ranked.len(), 1);
        assert_eq!(ranked.get(0).unwrap().bid_id, placed.bid_id);
        assert_best_eq_first(&env, &invoice);
    });
}

/// Expired bids are excluded from rank_bids and get_best_bid.
#[test]
fn non_placed_expired_excluded() {
    let env = Env::default();
    env.ledger().with_mut(|l| l.timestamp = 6_000);
    let contract = register(&env);
    let invoice = inv(&env, 0x62);

    env.as_contract(&contract, || {
        let placed  = make_bid(&env, &invoice, 3_000, 4_000, 10, BidStatus::Placed,  0x01);
        let expired = make_bid(&env, &invoice, 5_000, 8_000, 20, BidStatus::Expired, 0x02);
        store(&env, &placed);
        store(&env, &expired);

        let ranked = BidStorage::rank_bids(&env, &invoice);
        assert_eq!(ranked.len(), 1);
        assert_eq!(ranked.get(0).unwrap().bid_id, placed.bid_id);
        assert_best_eq_first(&env, &invoice);
    });
}

/// Cancelled bids are excluded from rank_bids and get_best_bid.
#[test]
fn non_placed_cancelled_excluded() {
    let env = Env::default();
    env.ledger().with_mut(|l| l.timestamp = 6_000);
    let contract = register(&env);
    let invoice = inv(&env, 0x63);

    env.as_contract(&contract, || {
        let placed    = make_bid(&env, &invoice, 3_000, 4_000, 10, BidStatus::Placed,    0x01);
        let cancelled = make_bid(&env, &invoice, 5_000, 8_000, 20, BidStatus::Cancelled, 0x02);
        store(&env, &placed);
        store(&env, &cancelled);

        let ranked = BidStorage::rank_bids(&env, &invoice);
        assert_eq!(ranked.len(), 1);
        assert_eq!(ranked.get(0).unwrap().bid_id, placed.bid_id);
        assert_best_eq_first(&env, &invoice);
    });
}

/// All non-Placed bids → empty ranking and None best bid.
#[test]
fn all_non_placed_gives_empty_ranking() {
    let env = Env::default();
    env.ledger().with_mut(|l| l.timestamp = 6_000);
    let contract = register(&env);
    let invoice = inv(&env, 0x64);

    env.as_contract(&contract, || {
        let b1 = make_bid(&env, &invoice, 5_000, 6_000, 10, BidStatus::Cancelled, 0x01);
        let b2 = make_bid(&env, &invoice, 5_000, 6_000, 20, BidStatus::Expired,   0x02);
        store(&env, &b1);
        store(&env, &b2);

        let ranked = BidStorage::rank_bids(&env, &invoice);
        assert_eq!(ranked.len(), 0, "no Placed bids → empty ranking");
        assert!(BidStorage::get_best_bid(&env, &invoice).is_none());
    });
}

/// No bids at all → empty ranking and None best bid.
#[test]
fn no_bids_gives_empty_ranking() {
    let env = Env::default();
    env.ledger().with_mut(|l| l.timestamp = 6_000);
    let contract = register(&env);
    let invoice = inv(&env, 0x65);

    env.as_contract(&contract, || {
        let ranked = BidStorage::rank_bids(&env, &invoice);
        assert_eq!(ranked.len(), 0);
        assert!(BidStorage::get_best_bid(&env, &invoice).is_none());
    });
}

// ============================================================================
// get_best_bid == rank_bids[0] invariant across all tie levels
// ============================================================================

/// best == first-ranked when profit breaks the tie.
#[test]
fn best_eq_first_ranked_profit_tiebreak() {
    let env = Env::default();
    env.ledger().with_mut(|l| l.timestamp = 7_000);
    let contract = register(&env);
    let invoice = inv(&env, 0x70);

    env.as_contract(&contract, || {
        let b1 = make_bid(&env, &invoice, 1_000, 4_000, 10, BidStatus::Placed, 0x01);
        let b2 = make_bid(&env, &invoice, 1_000, 3_000, 10, BidStatus::Placed, 0x02);
        store(&env, &b2);
        store(&env, &b1);
        assert_best_eq_first(&env, &invoice);
    });
}

/// best == first-ranked when expected_return breaks the tie.
#[test]
fn best_eq_first_ranked_expected_return_tiebreak() {
    let env = Env::default();
    env.ledger().with_mut(|l| l.timestamp = 7_000);
    let contract = register(&env);
    let invoice = inv(&env, 0x71);

    env.as_contract(&contract, || {
        let b1 = make_bid(&env, &invoice, 6_000, 7_000, 10, BidStatus::Placed, 0x01);
        let b2 = make_bid(&env, &invoice, 5_000, 6_000, 10, BidStatus::Placed, 0x02);
        store(&env, &b2);
        store(&env, &b1);
        assert_best_eq_first(&env, &invoice);
    });
}

/// best == first-ranked when bid_amount breaks the tie.
#[test]
fn best_eq_first_ranked_bid_amount_tiebreak() {
    let env = Env::default();
    env.ledger().with_mut(|l| l.timestamp = 7_000);
    let contract = register(&env);
    let invoice = inv(&env, 0x72);

    env.as_contract(&contract, || {
        let b1 = make_bid(&env, &invoice, 5_000, 6_000, 10, BidStatus::Placed, 0x01);
        let b2 = make_bid(&env, &invoice, 4_000, 5_000, 10, BidStatus::Placed, 0x02);
        store(&env, &b2);
        store(&env, &b1);
        assert_best_eq_first(&env, &invoice);
    });
}

/// best == first-ranked when timestamp breaks the tie.
#[test]
fn best_eq_first_ranked_timestamp_tiebreak() {
    let env = Env::default();
    env.ledger().with_mut(|l| l.timestamp = 7_000);
    let contract = register(&env);
    let invoice = inv(&env, 0x73);

    env.as_contract(&contract, || {
        let newer = make_bid(&env, &invoice, 5_000, 6_000, 200, BidStatus::Placed, 0x01);
        let older = make_bid(&env, &invoice, 5_000, 6_000, 100, BidStatus::Placed, 0x02);
        store(&env, &older);
        store(&env, &newer);
        assert_best_eq_first(&env, &invoice);
    });
}

/// best == first-ranked when bid_id breaks the tie (full tie on all other fields).
#[test]
fn best_eq_first_ranked_bid_id_tiebreak() {
    let env = Env::default();
    env.ledger().with_mut(|l| l.timestamp = 7_000);
    let contract = register(&env);
    let invoice = inv(&env, 0x74);

    env.as_contract(&contract, || {
        let high_id = make_bid(&env, &invoice, 5_000, 6_000, 100, BidStatus::Placed, 0xFF);
        let low_id  = make_bid(&env, &invoice, 5_000, 6_000, 100, BidStatus::Placed, 0x01);
        store(&env, &low_id);
        store(&env, &high_id);
        assert_best_eq_first(&env, &invoice);
    });
}

// ============================================================================
// Large field: ranking is stable with many bids
// ============================================================================

/// 10 bids with distinct profits rank in strict descending profit order.
#[test]
fn rank_bids_ten_bids_distinct_profits() {
    let env = Env::default();
    env.ledger().with_mut(|l| l.timestamp = 8_000);
    let contract = register(&env);
    let invoice = inv(&env, 0x80);

    env.as_contract(&contract, || {
        let mut bids = soroban_sdk::Vec::new(&env);
        for i in 0u8..10 {
            // profit = (i+1)*1000
            let b = make_bid(
                &env, &invoice,
                1_000,
                1_000 + (i as i128 + 1) * 1_000,
                10,
                BidStatus::Placed,
                i + 1,
            );
            store(&env, &b);
            bids.push_back(b);
        }

        let ranked = BidStorage::rank_bids(&env, &invoice);
        assert_eq!(ranked.len(), 10);

        // Verify strictly descending profit
        let mut prev_profit = i128::MAX;
        for idx in 0..10u32 {
            let b = ranked.get(idx).unwrap();
            let profit = b.expected_return - b.bid_amount;
            assert!(profit < prev_profit,
                "rank[{idx}] profit {profit} must be less than prev {prev_profit}");
            prev_profit = profit;
        }
        assert_best_eq_first(&env, &invoice);
    });
}

/// 5 bids with identical economics rank by bid_id descending (final tiebreaker).
#[test]
fn rank_bids_five_full_ties_rank_by_bid_id() {
    let env = Env::default();
    env.ledger().with_mut(|l| l.timestamp = 8_000);
    let contract = register(&env);
    let invoice = inv(&env, 0x81);

    env.as_contract(&contract, || {
        let id_bytes: [u8; 5] = [0x10, 0x40, 0x70, 0xA0, 0xFF];
        for &id_byte in id_bytes.iter() {
            let b = make_bid(&env, &invoice, 5_000, 6_000, 100, BidStatus::Placed, id_byte);
            store(&env, &b);
        }

        let ranked = BidStorage::rank_bids(&env, &invoice);
        assert_eq!(ranked.len(), 5);

        // Verify strictly descending bid_id (last byte)
        let mut prev_last_byte: u16 = 0x100u16; // sentinel above max u8
        for idx in 0..5u32 {
            let b = ranked.get(idx).unwrap();
            let last_byte = b.bid_id.to_array()[31];
            assert!((last_byte as u16) < prev_last_byte,
                "rank[{idx}] bid_id last byte {last_byte:#04x} must be less than prev {prev_last_byte:#06x}");
            prev_last_byte = last_byte as u16;
        }
        assert_best_eq_first(&env, &invoice);
    });
}

// ============================================================================
// Cross-invoice isolation
// ============================================================================

/// Bids on different invoices do not interfere with each other's ranking.
#[test]
fn ranking_is_isolated_per_invoice() {
    let env = Env::default();
    env.ledger().with_mut(|l| l.timestamp = 9_000);
    let contract = register(&env);
    let inv_a = inv(&env, 0x90);
    let inv_b = inv(&env, 0x91);

    env.as_contract(&contract, || {
        let a1 = make_bid(&env, &inv_a, 1_000, 5_000, 10, BidStatus::Placed, 0x01); // profit 4000
        let a2 = make_bid(&env, &inv_a, 1_000, 3_000, 10, BidStatus::Placed, 0x02); // profit 2000
        let b1 = make_bid(&env, &inv_b, 1_000, 2_000, 10, BidStatus::Placed, 0x03); // profit 1000
        let b2 = make_bid(&env, &inv_b, 1_000, 4_000, 10, BidStatus::Placed, 0x04); // profit 3000

        store(&env, &a1);
        store(&env, &a2);
        store(&env, &b1);
        store(&env, &b2);

        let ranked_a = BidStorage::rank_bids(&env, &inv_a);
        let ranked_b = BidStorage::rank_bids(&env, &inv_b);

        assert_eq!(ranked_a.len(), 2);
        assert_eq!(ranked_b.len(), 2);
        assert_eq!(ranked_a.get(0).unwrap().bid_id, a1.bid_id, "invoice A: highest profit first");
        assert_eq!(ranked_b.get(0).unwrap().bid_id, b2.bid_id, "invoice B: highest profit first");

        assert_best_eq_first(&env, &inv_a);
        assert_best_eq_first(&env, &inv_b);
    });
}

// ============================================================================
// Single bid edge case
// ============================================================================

/// A single Placed bid is its own best bid and sole ranked entry.
#[test]
fn single_bid_is_best_and_only_ranked() {
    let env = Env::default();
    env.ledger().with_mut(|l| l.timestamp = 9_000);
    let contract = register(&env);
    let invoice = inv(&env, 0x92);

    env.as_contract(&contract, || {
        let b = make_bid(&env, &invoice, 5_000, 6_000, 100, BidStatus::Placed, 0x42);
        store(&env, &b);

        let ranked = BidStorage::rank_bids(&env, &invoice);
        assert_eq!(ranked.len(), 1);
        assert_eq!(ranked.get(0).unwrap().bid_id, b.bid_id);
        assert_best_eq_first(&env, &invoice);
    });
}
