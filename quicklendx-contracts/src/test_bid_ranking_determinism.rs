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

    // =========================================================================
    // Helpers
    // =========================================================================

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

    /// Extract bid IDs from a ranked Vec for easy equality assertions.
    fn ids(ranked: &soroban_sdk::Vec<Bid>) -> Vec<[u8; 32]> {
        let mut out = Vec::new();
        let mut i = 0u32;
        while i < ranked.len() {
            out.push(ranked.get(i).unwrap().bid_id.to_array());
            i += 1;
        }
        out
    }

    // =========================================================================
    // Happy path — same ranking on repeated calls
    // =========================================================================

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

        assert_eq!(
            first_call, second_call,
            "rank_bids must be idempotent: first and second call differ"
        );
        assert_eq!(
            second_call, third_call,
            "rank_bids must be idempotent: second and third call differ"
        );
    }

    #[test]
    fn get_best_bid_equals_rank_bids_first_element_on_repeated_calls() {
        let (env, contract_id) = setup(2_000);
        let inv = invoice_id(&env, 2);

        let bid_x = make_bid(&env, &inv, 10_000, 12_000, 5, BidStatus::Placed, 10);
        let bid_y = make_bid(&env, &inv, 10_000, 11_500, 5, BidStatus::Placed, 20);
        persist(&env, &contract_id, &bid_x);
        persist(&env, &contract_id, &bid_y);

        for _ in 0..3 {
            let ranked = rank_bids(&env, &contract_id, &inv);
            let best = get_best_bid(&env, &contract_id, &inv).expect("best bid must be Some");
            assert_eq!(
                best.bid_id,
                ranked.get(0).unwrap().bid_id,
                "get_best_bid must equal rank_bids[0] on every call"
            );
        }
    }

    // =========================================================================
    // Happy path — insertion-order independence
    // =========================================================================

    #[test]
    fn rank_bids_is_insertion_order_independent_for_all_permutations() {
        const PERMUTATIONS: [[u8; 3]; 6] = [
            [0, 1, 2],
            [0, 2, 1],
            [1, 0, 2],
            [1, 2, 0],
            [2, 0, 1],
            [2, 1, 0],
        ];

        let mut expected_order: Option<Vec<[u8; 32]>> = None;

        for (perm_idx, order) in PERMUTATIONS.iter().enumerate() {
            let (env, contract_id) = setup(3_000);
            let inv = invoice_id(&env, 10u8 + perm_idx as u8);

            let bid_top = make_bid(&env, &inv, 5_000, 7_000, 100, BidStatus::Placed, 1);
            let bid_mid = make_bid(&env, &inv, 5_000, 6_500, 100, BidStatus::Placed, 2);
            let bid_low = make_bid(&env, &inv, 5_000, 6_000, 100, BidStatus::Placed, 3);
            let bids = [&bid_top, &bid_mid, &bid_low];

            for &idx in order.iter() {
                persist(&env, &contract_id, bids[idx as usize]);
            }

            let ranked = ids(&rank_bids(&env, &contract_id, &inv));

            match &expected_order {
                None => expected_order = Some(ranked),
                Some(exp) => assert_eq!(
                    *exp, ranked,
                    "permutation {perm_idx} produced a different ranking"
                ),
            }
        }
    }

    #[test]
    fn rank_bids_with_full_tie_is_insertion_order_independent() {
        let check = |inv_seed: u8, insert_ascending: bool| {
            let (env, contract_id) = setup(4_000);
            let inv = invoice_id(&env, inv_seed);

            let bid_lo = make_bid(&env, &inv, 5_000, 6_000, 50, BidStatus::Placed, 0x01);
            let bid_mi = make_bid(&env, &inv, 5_000, 6_000, 50, BidStatus::Placed, 0x05);
            let bid_hi = make_bid(&env, &inv, 5_000, 6_000, 50, BidStatus::Placed, 0x09);

            if insert_ascending {
                persist(&env, &contract_id, &bid_lo);
                persist(&env, &contract_id, &bid_mi);
                persist(&env, &contract_id, &bid_hi);
            } else {
                persist(&env, &contract_id, &bid_hi);
                persist(&env, &contract_id, &bid_mi);
                persist(&env, &contract_id, &bid_lo);
            }

            ids(&rank_bids(&env, &contract_id, &inv))
        };

        let ascending = check(30, true);
        let descending = check(31, false);

        assert_eq!(
            ascending, descending,
            "full-tie ranking must not depend on insertion order"
        );

        let bid_hi_id = {
            let (env, _) = setup(4_000);
            make_bid(&env, &invoice_id(&env, 31), 5_000, 6_000, 50, BidStatus::Placed, 0x09)
                .bid_id
                .to_array()
        };
        assert_eq!(
            ascending[0], bid_hi_id,
            "highest bid_id must be ranked first when all other fields tie"
        );
    }

    // =========================================================================
    // Happy path — `compare_bids` reflexivity and antisymmetry
    // =========================================================================

    #[test]
    fn compare_bids_is_reflexive_for_arbitrary_bid() {
        let (env, _) = setup(5_000);
        let inv = invoice_id(&env, 40);

        let bid = make_bid(&env, &inv, 3_333, 4_444, 99, BidStatus::Placed, 7);
        assert_eq!(
            BidStorage::compare_bids(&bid, &bid),
            Ordering::Equal,
            "compare_bids(a, a) must be Equal"
        );
    }

    #[test]
    fn compare_bids_is_antisymmetric() {
        let (env, _) = setup(6_000);
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

    #[test]
    fn compare_bids_ranks_higher_profit_first() {
        let (env, _) = setup(7_000);
        let inv = invoice_id(&env, 42);

        let high_profit = make_bid(&env, &inv, 5_000, 7_000, 1, BidStatus::Placed, 1);
        let low_profit = make_bid(&env, &inv, 5_000, 6_000, 1, BidStatus::Placed, 2);

        assert_eq!(
            BidStorage::compare_bids(&high_profit, &low_profit),
            Ordering::Greater
        );
    }

    #[test]
    fn compare_bids_ranks_higher_expected_return_when_profit_ties() {
        let (env, _) = setup(8_000);
        let inv = invoice_id(&env, 43);

        let high_return = make_bid(&env, &inv, 6_000, 7_000, 1, BidStatus::Placed, 1);
        let low_return = make_bid(&env, &inv, 5_000, 6_000, 1, BidStatus::Placed, 2);

        assert_eq!(
            BidStorage::compare_bids(&high_return, &low_return),
            Ordering::Greater,
            "higher expected_return must win when profit is equal"
        );
    }

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

        let newer = make_bid(&env, &inv, 5_000, 6_000, 200, BidStatus::Placed, 1);
        let older = make_bid(&env, &inv, 5_000, 6_000, 100, BidStatus::Placed, 2);

        assert_eq!(
            BidStorage::compare_bids(&newer, &older),
            Ordering::Greater,
            "newer timestamp must win when economics are equal"
        );
    }

    #[test]
    fn compare_bids_uses_bid_id_as_final_deterministic_tiebreaker() {
        let (env, _) = setup(11_000);
        let inv = invoice_id(&env, 46);

        let high_id = make_bid(&env, &inv, 5_000, 6_000, 50, BidStatus::Placed, 0xFF);
        let low_id = make_bid(&env, &inv, 5_000, 6_000, 50, BidStatus::Placed, 0x01);

        assert_eq!(
            BidStorage::compare_bids(&high_id, &low_id),
            Ordering::Greater,
            "higher bid_id must win when every other field is identical"
        );
        assert_eq!(
            BidStorage::compare_bids(&low_id, &high_id),
            Ordering::Less
        );
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
        );

        let best = get_best_bid(&env, &contract_id, &inv).expect("best must exist");
        assert_eq!(
            best.bid_id, placed.bid_id,
            "get_best_bid must also exclude non-Placed bids"
        );
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

        assert_eq!(
            first, second,
            "ranking with negative-profit bids must be deterministic across calls"
        );

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
}
