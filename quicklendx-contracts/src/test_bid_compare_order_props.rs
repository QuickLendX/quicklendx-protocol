//! Property-based tests for BidStorage::compare_bids total order axioms.
//!
//! ## Purpose
//!
//! `BidStorage::compare_bids` defines the ranking comparator used by `rank_bids`
//! and `get_best_bid`. It must be a valid **total order** (antisymmetry, transitivity,
//! and totality) to produce correct, deterministic bid rankings. A defective comparator
//! can cause non-deterministic sorts, picking the wrong winning bid—a direct economic bug.
//!
//! ## What we test
//!
//! 1. **Antisymmetry**: If `a < b`, then `!(b < a)` and `!(a == b)`.
//! 2. **Transitivity**: If `a < b` and `b < c`, then `a < c`.
//! 3. **Totality**: For any two bids, exactly one of `<`, `=`, or `>` holds.
//! 4. **Reflexivity**: `compare_bids(a, a) == Equal`.
//! 5. **rank_bids consistency**: The output is sorted according to `compare_bids`.
//! 6. **bid_id tiebreaker uniqueness**: When all higher-priority keys are equal,
//!    `bid_id` guarantees a unique, deterministic order.
//!
//! ## Coverage
//!
//! These tests exercise all five comparison levels in `compare_bids`:
//! - Profit (expected_return - bid_amount)
//! - Expected return
//! - Bid amount
//! - Timestamp (newer first)
//! - bid_id (stable tiebreaker)
//!
//! ## Running the tests
//!
//! ```bash
//! # With fixed seed for reproducibility
//! QUICKLENDX_SEED=42 cargo test --features fuzz-tests test_bid_compare_order_props
//!
//! # With OS random seed
//! cargo test --features fuzz-tests test_bid_compare_order_props
//! ```

#![cfg(feature = "fuzz-tests")]

use crate::bid::BidStorage;
use crate::types::{Bid, BidStatus};
use crate::test_seed;
use proptest::prelude::*;
use soroban_sdk::{testutils::Address as _, Address, BytesN, Env};
use std::cmp::Ordering;

// ─────────────────────────────────────────────────────────────────────────────
// Arbitrary bid generator
// ─────────────────────────────────────────────────────────────────────────────

/// Generate a random bid with full field coverage.
///
/// Constraints:
/// - `bid_amount` and `expected_return` are in `[0, 1_000_000_000]` to avoid overflow.
/// - `timestamp` is in `[0, u64::MAX]`.
/// - `bid_id` is a random 32-byte array.
/// - `status` is always `Placed` (only Placed bids are ranked).
fn arb_bid() -> impl Strategy<Value = Bid> {
    (
        any::<[u8; 32]>(),   // bid_id
        any::<[u8; 32]>(),   // invoice_id
        0i128..=1_000_000_000i128, // bid_amount
        0i128..=1_000_000_000i128, // expected_return
        any::<u64>(),        // timestamp
        any::<u64>(),        // expiration_timestamp
    )
        .prop_map(|(bid_id_bytes, invoice_id_bytes, bid_amount, expected_return, timestamp, expiration_timestamp)| {
            let env = Env::default();
            let investor = Address::generate(&env);
            Bid {
                bid_id: BytesN::from_array(&env, &bid_id_bytes),
                invoice_id: BytesN::from_array(&env, &invoice_id_bytes),
                investor,
                bid_amount,
                expected_return,
                timestamp,
                status: BidStatus::Placed,
                expiration_timestamp,
            }
        })
}

/// Generate a triple of arbitrary bids for transitivity testing.
fn arb_bid_triple() -> impl Strategy<Value = (Bid, Bid, Bid)> {
    (arb_bid(), arb_bid(), arb_bid())
}

/// Generate a vector of arbitrary bids for rank_bids consistency testing.
fn arb_bid_vec(min_size: usize, max_size: usize) -> impl Strategy<Value = Vec<Bid>> {
    prop::collection::vec(arb_bid(), min_size..=max_size)
}

// ─────────────────────────────────────────────────────────────────────────────
// Helper: classify ordering for readability in assertions
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OrderClass {
    Less,
    Equal,
    Greater,
}

impl From<Ordering> for OrderClass {
    fn from(ord: Ordering) -> Self {
        match ord {
            Ordering::Less => OrderClass::Less,
            Ordering::Equal => OrderClass::Equal,
            Ordering::Greater => OrderClass::Greater,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Property 1: Antisymmetry
// ─────────────────────────────────────────────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig {
        rng_algorithm: proptest::test_runner::RngAlgorithm::ChaCha,
        rng: test_seed::seed().map(|s| proptest::test_runner::TestRng::from_seed(
            proptest::test_runner::RngAlgorithm::ChaCha,
            &s.to_le_bytes()
        )),
        ..ProptestConfig::default()
    })]

    /// **Antisymmetry**: If `compare_bids(a, b) == Less`, then
    /// `compare_bids(b, a) == Greater` and vice versa.
    ///
    /// If `compare_bids(a, b) == Equal`, then `compare_bids(b, a) == Equal`.
    ///
    /// This property is foundational for any valid ordering relation and prevents
    /// contradictory rankings that would cause `rank_bids` to produce inconsistent results.
    #[test]
    fn prop_antisymmetry(a in arb_bid(), b in arb_bid()) {
        let ab = BidStorage::compare_bids(&a, &b);
        let ba = BidStorage::compare_bids(&b, &a);

        match ab {
            Ordering::Less => {
                prop_assert_eq!(ba, Ordering::Greater, "Antisymmetry violated: a < b but b !> a");
            }
            Ordering::Greater => {
                prop_assert_eq!(ba, Ordering::Less, "Antisymmetry violated: a > b but b !< a");
            }
            Ordering::Equal => {
                prop_assert_eq!(ba, Ordering::Equal, "Antisymmetry violated: a == b but b != a");
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Property 2: Transitivity
// ─────────────────────────────────────────────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig {
        rng_algorithm: proptest::test_runner::RngAlgorithm::ChaCha,
        rng: test_seed::seed().map(|s| proptest::test_runner::TestRng::from_seed(
            proptest::test_runner::RngAlgorithm::ChaCha,
            &s.to_le_bytes()
        )),
        ..ProptestConfig::default()
    })]

    /// **Transitivity**: If `a < b` and `b < c`, then `a < c`.
    ///
    /// Similarly, if `a == b` and `b == c`, then `a == c`.
    #[test]
    fn prop_transitivity((a, b, c) in arb_bid_triple()) {
        let ab = BidStorage::compare_bids(&a, &b);
        let bc = BidStorage::compare_bids(&b, &c);
        let ac = BidStorage::compare_bids(&a, &c);

        let ab_class = OrderClass::from(ab);
        let bc_class = OrderClass::from(bc);
        let ac_class = OrderClass::from(ac);

        // Transitivity table:
        // ab | bc | ac (required)
        // ---|----|--------------
        // <  | <  | <
        // <  | =  | <
        // =  | <  | <
        // =  | =  | =
        // =  | >  | >
        // >  | =  | >
        // >  | >  | >

        match (ab_class, bc_class) {
            (OrderClass::Less, OrderClass::Less) => {
                prop_assert_eq!(ac_class, OrderClass::Less, "Transitivity violated: a < b < c but a !< c");
            }
            (OrderClass::Less, OrderClass::Equal) => {
                prop_assert_eq!(ac_class, OrderClass::Less, "Transitivity violated: a < b == c but a !< c");
            }
            (OrderClass::Equal, OrderClass::Less) => {
                prop_assert_eq!(ac_class, OrderClass::Less, "Transitivity violated: a == b < c but a !< c");
            }
            (OrderClass::Equal, OrderClass::Equal) => {
                prop_assert_eq!(ac_class, OrderClass::Equal, "Transitivity violated: a == b == c but a != c");
            }
            (OrderClass::Equal, OrderClass::Greater) => {
                prop_assert_eq!(ac_class, OrderClass::Greater, "Transitivity violated: a == b > c but a !> c");
            }
            (OrderClass::Greater, OrderClass::Equal) => {
                prop_assert_eq!(ac_class, OrderClass::Greater, "Transitivity violated: a > b == c but a !> c");
            }
            (OrderClass::Greater, OrderClass::Greater) => {
                prop_assert_eq!(ac_class, OrderClass::Greater, "Transitivity violated: a > b > c but a !> c");
            }
            // Mixed cases (e.g., a < b > c) have no transitivity requirement
            _ => {}
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Property 3: Totality
// ─────────────────────────────────────────────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig {
        rng_algorithm: proptest::test_runner::RngAlgorithm::ChaCha,
        rng: test_seed::seed().map(|s| proptest::test_runner::TestRng::from_seed(
            proptest::test_runner::RngAlgorithm::ChaCha,
            &s.to_le_bytes()
        )),
        ..ProptestConfig::default()
    })]

    /// **Totality**: For any two bids `a` and `b`, exactly one of the following holds:
    /// - `compare_bids(a, b) == Less`
    /// - `compare_bids(a, b) == Equal`
    /// - `compare_bids(a, b) == Greater`
    ///
    /// This is guaranteed by Rust's `Ordering` enum, but we verify it explicitly.
    #[test]
    fn prop_totality(a in arb_bid(), b in arb_bid()) {
        let ab = BidStorage::compare_bids(&a, &b);
        // By definition, Ordering is one of Less, Equal, or Greater.
        // This test documents the guarantee rather than testing Rust's enum.
        prop_assert!(matches!(ab, Ordering::Less | Ordering::Equal | Ordering::Greater));
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Property 4: Reflexivity (equality)
// ─────────────────────────────────────────────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig {
        rng_algorithm: proptest::test_runner::RngAlgorithm::ChaCha,
        rng: test_seed::seed().map(|s| proptest::test_runner::TestRng::from_seed(
            proptest::test_runner::RngAlgorithm::ChaCha,
            &s.to_le_bytes()
        )),
        ..ProptestConfig::default()
    })]

    /// **Reflexivity**: `compare_bids(a, a) == Equal` for any bid `a`.
    #[test]
    fn prop_reflexivity(a in arb_bid()) {
        let aa = BidStorage::compare_bids(&a, &a);
        prop_assert_eq!(aa, Ordering::Equal, "Reflexivity violated: a != a");
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Property 5: rank_bids consistency
// ─────────────────────────────────────────────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig {
        rng_algorithm: proptest::test_runner::RngAlgorithm::ChaCha,
        rng: test_seed::seed().map(|s| proptest::test_runner::TestRng::from_seed(
            proptest::test_runner::RngAlgorithm::ChaCha,
            &s.to_le_bytes()
        )),
        cases: 50, // Reduce cases for rank_bids (expensive operation)
        ..ProptestConfig::default()
    })]

    /// **rank_bids consistency**: The output of `rank_bids` must be sorted
    /// according to `compare_bids`, with no adjacent inversions.
    ///
    /// Specifically, for all adjacent pairs `(bids[i], bids[i+1])` in the result:
    /// - `compare_bids(bids[i], bids[i+1]) == Greater` (best-to-worst order)
    #[test]
    fn prop_rank_bids_consistency(bids_vec in arb_bid_vec(2, 10)) {
        // Skip empty or single-element vectors (no adjacent pairs to check)
        if bids_vec.len() < 2 {
            return Ok(());
        }

        let env = Env::default();
        let invoice_id = BytesN::from_array(&env, &[0u8; 32]);

        // Store all bids
        for bid in &bids_vec {
            BidStorage::store_bid(&env, bid);
            BidStorage::add_bid_to_invoice(&env, &invoice_id, &bid.bid_id);
        }

        // Rank the bids
        let ranked = BidStorage::rank_bids(&env, &invoice_id);

        // Verify no adjacent inversions
        for i in 0..(ranked.len() - 1) {
            let current = ranked.get_unchecked(i);
            let next = ranked.get_unchecked(i + 1);
            let cmp = BidStorage::compare_bids(&current, &next);

            prop_assert!(
                matches!(cmp, Ordering::Greater | Ordering::Equal),
                "rank_bids produced inversion at index {}: compare_bids returned {:?}",
                i,
                cmp
            );
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Property 6: bid_id tiebreaker uniqueness
// ─────────────────────────────────────────────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig {
        rng_algorithm: proptest::test_runner::RngAlgorithm::ChaCha,
        rng: test_seed::seed().map(|s| proptest::test_runner::TestRng::from_seed(
            proptest::test_runner::RngAlgorithm::ChaCha,
            &s.to_le_bytes()
        )),
        ..ProptestConfig::default()
    })]

    /// **bid_id tiebreaker uniqueness**: When all higher-priority keys
    /// (profit, expected_return, bid_amount, timestamp) are equal, `bid_id`
    /// guarantees a unique, deterministic order.
    ///
    /// We generate two bids with identical economic fields but different `bid_id`
    /// and verify that `compare_bids` returns a non-Equal result.
    #[test]
    fn prop_bid_id_tiebreaker(
        bid_id_1 in any::<[u8; 32]>(),
        bid_id_2 in any::<[u8; 32]>(),
        bid_amount in 0i128..=1_000_000_000i128,
        expected_return in 0i128..=1_000_000_000i128,
        timestamp in any::<u64>(),
    ) {
        // Skip identical bid_ids
        if bid_id_1 == bid_id_2 {
            return Ok(());
        }

        let env = Env::default();
        let invoice_id = BytesN::from_array(&env, &[0u8; 32]);
        let investor = Address::generate(&env);

        let bid1 = Bid {
            bid_id: BytesN::from_array(&env, &bid_id_1),
            invoice_id: invoice_id.clone(),
            investor: investor.clone(),
            bid_amount,
            expected_return,
            timestamp,
            status: BidStatus::Placed,
            expiration_timestamp: timestamp + 86400,
        };

        let bid2 = Bid {
            bid_id: BytesN::from_array(&env, &bid_id_2),
            invoice_id: invoice_id.clone(),
            investor: investor.clone(),
            bid_amount,
            expected_return,
            timestamp,
            status: BidStatus::Placed,
            expiration_timestamp: timestamp + 86400,
        };

        let cmp = BidStorage::compare_bids(&bid1, &bid2);

        // With different bid_ids, we expect a definitive ordering (not Equal)
        prop_assert_ne!(
            cmp,
            Ordering::Equal,
            "bid_id tiebreaker failed: bids with different IDs compared as Equal"
        );

        // Verify the ordering is consistent with the bid_id byte comparison
        let expected_cmp = bid_id_1.cmp(&bid_id_2);
        prop_assert_eq!(
            cmp,
            expected_cmp,
            "bid_id tiebreaker ordering mismatch: expected {:?}, got {:?}",
            expected_cmp,
            cmp
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Property 7: Compare levels (branch coverage)
// ─────────────────────────────────────────────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig {
        rng_algorithm: proptest::test_runner::RngAlgorithm::ChaCha,
        rng: test_seed::seed().map(|s| proptest::test_runner::TestRng::from_seed(
            proptest::test_runner::RngAlgorithm::ChaCha,
            &s.to_le_bytes()
        )),
        ..ProptestConfig::default()
    })]

    /// **Level 1: Profit comparison** (expected_return - bid_amount).
    ///
    /// When profits differ, the bid with higher profit ranks higher.
    #[test]
    fn prop_level1_profit(
        bid_amount_1 in 0i128..=1_000_000_000i128,
        expected_return_1 in 0i128..=1_000_000_000i128,
        bid_amount_2 in 0i128..=1_000_000_000i128,
        expected_return_2 in 0i128..=1_000_000_000i128,
    ) {
        let profit1 = expected_return_1.saturating_sub(bid_amount_1);
        let profit2 = expected_return_2.saturating_sub(bid_amount_2);

        // Skip equal profits (we test that in other properties)
        if profit1 == profit2 {
            return Ok(());
        }

        let env = Env::default();
        let invoice_id = BytesN::from_array(&env, &[0u8; 32]);
        let investor = Address::generate(&env);

        let bid1 = Bid {
            bid_id: BytesN::from_array(&env, &[1u8; 32]),
            invoice_id: invoice_id.clone(),
            investor: investor.clone(),
            bid_amount: bid_amount_1,
            expected_return: expected_return_1,
            timestamp: 1000,
            status: BidStatus::Placed,
            expiration_timestamp: 2000,
        };

        let bid2 = Bid {
            bid_id: BytesN::from_array(&env, &[2u8; 32]),
            invoice_id: invoice_id.clone(),
            investor: investor.clone(),
            bid_amount: bid_amount_2,
            expected_return: expected_return_2,
            timestamp: 1000,
            status: BidStatus::Placed,
            expiration_timestamp: 2000,
        };

        let cmp = BidStorage::compare_bids(&bid1, &bid2);
        let expected = profit1.cmp(&profit2);

        prop_assert_eq!(
            cmp,
            expected,
            "Level 1 (profit) comparison mismatch: profit1={}, profit2={}",
            profit1,
            profit2
        );
    }

    /// **Level 2: Expected return comparison**.
    ///
    /// When profits are equal but expected returns differ, the bid with
    /// higher expected return ranks higher.
    #[test]
    fn prop_level2_expected_return(
        expected_return_1 in 0i128..=1_000_000_000i128,
        expected_return_2 in 0i128..=1_000_000_000i128,
    ) {
        // Skip equal expected returns
        if expected_return_1 == expected_return_2 {
            return Ok(());
        }

        // Force equal profit: set bid_amount to 0 for both
        let bid_amount = 0i128;

        let env = Env::default();
        let invoice_id = BytesN::from_array(&env, &[0u8; 32]);
        let investor = Address::generate(&env);

        let bid1 = Bid {
            bid_id: BytesN::from_array(&env, &[1u8; 32]),
            invoice_id: invoice_id.clone(),
            investor: investor.clone(),
            bid_amount,
            expected_return: expected_return_1,
            timestamp: 1000,
            status: BidStatus::Placed,
            expiration_timestamp: 2000,
        };

        let bid2 = Bid {
            bid_id: BytesN::from_array(&env, &[2u8; 32]),
            invoice_id: invoice_id.clone(),
            investor: investor.clone(),
            bid_amount,
            expected_return: expected_return_2,
            timestamp: 1000,
            status: BidStatus::Placed,
            expiration_timestamp: 2000,
        };

        let cmp = BidStorage::compare_bids(&bid1, &bid2);
        let expected = expected_return_1.cmp(&expected_return_2);

        prop_assert_eq!(
            cmp,
            expected,
            "Level 2 (expected_return) comparison mismatch: er1={}, er2={}",
            expected_return_1,
            expected_return_2
        );
    }

    /// **Level 3: Bid amount comparison**.
    ///
    /// When profit and expected return are equal but bid amounts differ,
    /// the bid with higher bid amount ranks higher.
    #[test]
    fn prop_level3_bid_amount(
        bid_amount_1 in 0i128..=1_000_000_000i128,
        bid_amount_2 in 0i128..=1_000_000_000i128,
    ) {
        // Skip equal bid amounts
        if bid_amount_1 == bid_amount_2 {
            return Ok(());
        }

        // Force equal profit and expected_return: set expected_return = bid_amount for both
        let env = Env::default();
        let invoice_id = BytesN::from_array(&env, &[0u8; 32]);
        let investor = Address::generate(&env);

        let bid1 = Bid {
            bid_id: BytesN::from_array(&env, &[1u8; 32]),
            invoice_id: invoice_id.clone(),
            investor: investor.clone(),
            bid_amount: bid_amount_1,
            expected_return: bid_amount_1, // profit = 0
            timestamp: 1000,
            status: BidStatus::Placed,
            expiration_timestamp: 2000,
        };

        let bid2 = Bid {
            bid_id: BytesN::from_array(&env, &[2u8; 32]),
            invoice_id: invoice_id.clone(),
            investor: investor.clone(),
            bid_amount: bid_amount_2,
            expected_return: bid_amount_2, // profit = 0
            timestamp: 1000,
            status: BidStatus::Placed,
            expiration_timestamp: 2000,
        };

        let cmp = BidStorage::compare_bids(&bid1, &bid2);
        let expected = bid_amount_1.cmp(&bid_amount_2);

        prop_assert_eq!(
            cmp,
            expected,
            "Level 3 (bid_amount) comparison mismatch: ba1={}, ba2={}",
            bid_amount_1,
            bid_amount_2
        );
    }

    /// **Level 4: Timestamp comparison (newer first)**.
    ///
    /// When profit, expected return, and bid amount are equal but timestamps differ,
    /// the bid with the **larger** timestamp (newer) ranks higher.
    #[test]
    fn prop_level4_timestamp(
        timestamp_1 in any::<u64>(),
        timestamp_2 in any::<u64>(),
    ) {
        // Skip equal timestamps
        if timestamp_1 == timestamp_2 {
            return Ok(());
        }

        // Force equal profit, expected_return, and bid_amount
        let bid_amount = 1000i128;
        let expected_return = 1000i128; // profit = 0

        let env = Env::default();
        let invoice_id = BytesN::from_array(&env, &[0u8; 32]);
        let investor = Address::generate(&env);

        let bid1 = Bid {
            bid_id: BytesN::from_array(&env, &[1u8; 32]),
            invoice_id: invoice_id.clone(),
            investor: investor.clone(),
            bid_amount,
            expected_return,
            timestamp: timestamp_1,
            status: BidStatus::Placed,
            expiration_timestamp: timestamp_1 + 86400,
        };

        let bid2 = Bid {
            bid_id: BytesN::from_array(&env, &[2u8; 32]),
            invoice_id: invoice_id.clone(),
            investor: investor.clone(),
            bid_amount,
            expected_return,
            timestamp: timestamp_2,
            status: BidStatus::Placed,
            expiration_timestamp: timestamp_2 + 86400,
        };

        let cmp = BidStorage::compare_bids(&bid1, &bid2);
        let expected = timestamp_1.cmp(&timestamp_2);

        prop_assert_eq!(
            cmp,
            expected,
            "Level 4 (timestamp) comparison mismatch: ts1={}, ts2={}",
            timestamp_1,
            timestamp_2
        );
    }
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    /// Sanity check: ensure the test harness compiles and runs.
    #[test]
    fn test_harness_smoke() {
        let env = Env::default();
        let invoice_id = BytesN::from_array(&env, &[0u8; 32]);
        let investor = Address::generate(&env);

        let bid1 = Bid {
            bid_id: BytesN::from_array(&env, &[1u8; 32]),
            invoice_id: invoice_id.clone(),
            investor: investor.clone(),
            bid_amount: 1000,
            expected_return: 1100,
            timestamp: 1000,
            status: BidStatus::Placed,
            expiration_timestamp: 2000,
        };

        let bid2 = Bid {
            bid_id: BytesN::from_array(&env, &[2u8; 32]),
            invoice_id: invoice_id.clone(),
            investor: investor.clone(),
            bid_amount: 1000,
            expected_return: 1050,
            timestamp: 1000,
            status: BidStatus::Placed,
            expiration_timestamp: 2000,
        };

        let cmp = BidStorage::compare_bids(&bid1, &bid2);
        assert_eq!(cmp, Ordering::Greater, "bid1 should rank higher (profit 100 > 50)");
    }

    /// Test seed reproducibility: fixed seed produces fixed output.
    #[test]
    fn test_seed_reproducibility() {
        std::env::set_var("QUICKLENDX_SEED", "12345");
        let seed1 = test_seed::seed();
        std::env::remove_var("QUICKLENDX_SEED");

        std::env::set_var("QUICKLENDX_SEED", "12345");
        let seed2 = test_seed::seed();
        std::env::remove_var("QUICKLENDX_SEED");

        assert_eq!(seed1, seed2, "Fixed seed should produce deterministic output");
    }
}
