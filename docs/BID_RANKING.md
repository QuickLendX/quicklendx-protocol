# Bid Ranking — Deterministic Ordering Function

> **Audience:** contributors who want to understand, modify, audit, or extend the
> bid-ranking logic in the QuickLendX Soroban contract.
>
> This document complements [`docs/contracts/bid-ranking.md`](contracts/bid-ranking.md),
> which is the lower-level tie-breaker specification. The page you are reading is
> the contributor-facing description of *why* the comparator is shaped the way
> it is and *how* to verify it.

## 1. Why a dedicated ordering function?

Investment ranking on QuickLendX is **consensus-critical**: every Stellar validator
must agree on the same "best bid" for a given invoice at the same ledger state, or
the entire bidding system becomes non-deterministic and unauditable.

Concretely, the bid-ranking comparator is the single source of truth for every
question that depends on bid preference:

- "Which bid is the business shown first when it accepts a winner?"
- "Which bid is selected by [`BidStorage::get_best_bid`](../quicklendx-contracts/src/bid.rs)?"
- "What is the order returned by [`BidStorage::rank_bids`](../quicklendx-contracts/src/bid.rs)?"

Because all three answers flow through one comparator
([`BidStorage::compare_bids`](../quicklendx-contracts/src/bid.rs)), the
contract cannot drift on tie handling — a property we test for explicitly
(see §5).

## 2. Where it lives

The comparator lives in `quicklendx-contracts/src/bid.rs` as a method on
`BidStorage`:

```rust
pub fn compare_bids(bid1: &Bid, bid2: &Bid) -> Ordering
```

It compares two `Bid` records (defined in
[`quicklendx-contracts/src/types.rs`](../quicklendx-contracts/src/types.rs)):

```rust
pub struct Bid {
    pub bid_id:               BytesN<32>,
    pub invoice_id:           BytesN<32>,
    pub investor:             Address,
    pub bid_amount:           i128,
    pub expected_return:      i128,
    pub timestamp:            u64,
    pub status:               BidStatus,
    pub expiration_timestamp: u64,
}
```

Only `Placed` bids participate in ranking. `Withdrawn`, `Accepted`, `Expired`,
and `Cancelled` bids are filtered out before comparison runs.

## 3. The five-tier comparator

The comparator walks five fields in order, returning the first non-tied result.
The same five tiers are enforced identically by every code path that sorts
bids (`get_best_bid`, `rank_bids`, the property tests, and the regression
tests).

| Tier | Field (derived from)        | Comparison                | Outcome                                      |
| ---: | :-------------------------- | :------------------------ | :-------------------------------------------- |
|   1  | **Profit** = `expected_return − bid_amount` | `>` (higher wins)         | Bid with the highest profit for the business wins. |
|   2  | `expected_return`           | `>` (higher wins)         | Use the higher expected return when profit is equal. |
|   3  | `bid_amount`                | `>` (higher wins)         | Use the higher bid amount when profit and expected return are both equal. |
|   4  | `timestamp`                 | `>` (newer wins)          | Use the more recently submitted bid when all economic fields are equal. |
|   5  | `bid_id` (32-byte array, lexicographic) | `>` (higher wins) | Use the higher bid_id as the final tiebreaker — guaranteed unique per bid. |

Profit is computed with `saturating_sub`, so a bid whose
`expected_return < bid_amount` (i.e. a loss) simply yields `0` profit and
falls through to the lower tiers — there is no negative-profit penalisation
beyond sorting it to the bottom of tier 1.

### 3.1 Worked example (the five tiers in action)

Consider four bids on the same invoice, all `Placed`. The table below shows
how the comparator resolves the ordering:

| Bid  | bid_amount | expected_return | profit (= ER − BA) | timestamp | bid_id (last byte) |
| :--- | ---------: | --------------: | -----------------: | --------: | :----------------- |
| A    | `5_000`    | `7_000`         | `2_000`            | `10`      | `0x01`             |
| B    | `5_300`    | `7_000`         | `1_700`            | `20`      | `0x02`             |
| C    | `5_000`    | `6_000`         | `1_000`            | `20`      | `0x09`             |
| D    | `5_000`    | `6_000`         | `1_000`            | `20`      | `0x02`             |

Ranking step-by-step:

1. **Tier 1 — profit**: A (`2_000`) > B (`1_700`) > {C, D} (`1_000`). Among
   C and D, profit is tied; the comparator falls through to the lower tiers.
2. **Tier 2 — expected_return**: B beats C and D on tier 1, so this tier is
   never evaluated between A and C/D. Between C and D, expected_return ties
   (`6_000` = `6_000`); the comparator falls through.
3. **Tier 3 — bid_amount**: only reached when tiers 1 and 2 tie. Between C
   and D, bid_amount ties (`5_000` = `5_000`); the comparator falls through.
4. **Tier 4 — timestamp**: between C and D, timestamp ties (`20` = `20`);
   the comparator falls through.
5. **Tier 5 — bid_id**: lexicographic byte order — D (`…02`) < C (`…09`),
   so C ranks above D.

**Final order:** A → B → C → D.

This is the same invariant proven by `rank_bids_orders_by_profit_and_expected_return`
and `rank_bids_uses_bid_id_as_final_tiebreaker` in
[`src/test_bid_ranking.rs`](../quicklendx-contracts/src/test_bid_ranking.rs).

### 3.2 Seeded pseudo-bid table (compile-ready)

The following snippet is the **downstream-integration form** — copy it
verbatim into a `#[cfg(test)]` block of a downstream crate that depends
on `quicklendx-contracts` (e.g. an integration-test crate or a downstream
service's test suite). The `use quicklendx_contracts::bid::…` imports
match the public re-exports of the contract crate, so no other edit is
required.

> **Inside `quicklendx-contracts` itself**, do not paste this snippet —
> reuse the existing `build_bid` helper in
> [`src/test_bid_ranking.rs`](../quicklendx-contracts/src/test_bid_ranking.rs)
> and the canonical regression test `doc_example_full_tie_ladder` that
> already lives there. `seed_bid` is intentionally a separate helper so
> the snippet stays self-contained for downstream consumers; it is
> byte-equivalent to `build_bid` modulo the `604_800` numeric-literal
> formatting.

> Requires `features = ["testutils"]` on `soroban-sdk` (already enabled in
> `quicklendx-contracts` via the `[dev-dependencies]` block of `Cargo.toml`,
> so this is automatic when the snippet lives inside the contract crate).

```rust
use crate::bid::{Bid, BidStatus, BidStorage};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, BytesN, Env,
};

fn seed_bid(
    env: &Env,
    invoice_id: &BytesN<32>,
    investor: &Address,
    bid_amount: i128,
    expected_return: i128,
    timestamp: u64,
    id_suffix: u8,
) -> Bid {
    // Mirror the byte layout used by `src/test_bid_ranking.rs::build_bid`
    // so that bid_ids are unique per (timestamp, id_suffix) — both bytes
    // 30 and 31 are touched to match the existing test harness exactly.
    let mut bid_id_bytes = [0u8; 32];
    bid_id_bytes[0] = 0xB1;            // 'B'
    bid_id_bytes[1] = 0xD0;            // 'D' (biD)
    bid_id_bytes[2..10].copy_from_slice(&timestamp.to_be_bytes());
    bid_id_bytes[30] = id_suffix;
    bid_id_bytes[31] = id_suffix;

    Bid {
        bid_id:               BytesN::from_array(env, &bid_id_bytes),
        invoice_id:           invoice_id.clone(),
        investor:             investor.clone(),
        bid_amount,
        expected_return,
        timestamp,
        status:               BidStatus::Placed,
        expiration_timestamp: timestamp.saturating_add(604_800), // 7 days
    }
}

#[test]
fn doc_example_full_tie_ladder() {
    let env = Env::default();
    env.ledger().with_mut(|li| li.timestamp = 1_000);

    // Use a stable invoice_id so storage lookups hit.
    let mut invoice_bytes = [0u8; 32];
    invoice_bytes[31] = 0xAA;
    let invoice_id = BytesN::from_array(&env, &invoice_bytes);

    let investor = Address::generate(&env);

    // Profit in parentheses for readability:
    //   high_profit         (5_000 -> 7_000, profit 2_000, ts=10, id_suffix=0x01)
    //   higher_amount       (5_300 -> 7_000, profit 1_700, ts=20, id_suffix=0x02)
    //   equal_old_lower_id  (5_000 -> 6_000, profit 1_000, ts=20, id_suffix=0x02)
    //   equal_new_higher_id (5_000 -> 6_000, profit 1_000, ts=20, id_suffix=0x09)
    //
    // The two `equal_*` bids share both economic fields AND timestamp so that
    // tier 5 (bid_id lexicographic) is the only field that can break the tie.
    let high_profit         = seed_bid(&env, &invoice_id, &investor, 5_000, 7_000, 10, 0x01);
    let higher_amount       = seed_bid(&env, &invoice_id, &investor, 5_300, 7_000, 20, 0x02);
    let equal_old_lower_id  = seed_bid(&env, &invoice_id, &investor, 5_000, 6_000, 20, 0x02);
    let equal_new_higher_id = seed_bid(&env, &invoice_id, &investor, 5_000, 6_000, 20, 0x09);

    for bid in [&high_profit, &higher_amount, &equal_old_lower_id, &equal_new_higher_id] {
        BidStorage::store_bid(&env, bid);
        BidStorage::add_bid_to_invoice(&env, &invoice_id, &bid.bid_id);
    }

    let ranked = BidStorage::rank_bids(&env, &invoice_id);

    assert_eq!(ranked.len(), 4);
    // Tier 1 (profit): high_profit (2_000) beats every other bid.
    assert_eq!(ranked.get(0).unwrap().bid_id, high_profit.bid_id);
    // Tier 1 (profit): higher_amount (1_700) beats equal_* (1_000); expected
    // return and bid_amount only matter when the earlier tiers tie.
    assert_eq!(ranked.get(1).unwrap().bid_id, higher_amount.bid_id);
    // Tier 5 (bid_id): equal_new_higher_id (id_suffix 0x09) beats
    // equal_old_lower_id (id_suffix 0x02); all four earlier tiers tie here.
    assert_eq!(ranked.get(2).unwrap().bid_id, equal_new_higher_id.bid_id);
    assert_eq!(ranked.get(3).unwrap().bid_id, equal_old_lower_id.bid_id);

    // Invariant: best_bid == rank_bids[0] when ranking is non-empty.
    let best = BidStorage::get_best_bid(&env, &invoice_id).unwrap();
    assert_eq!(best.bid_id, ranked.get(0).unwrap().bid_id);
}
```

> **Why these values?** The two `equal_*` bids share economic fields AND
> timestamp so tier 5 (bid_id) is the only remaining tie-breaker — this
> matches the §3.1 worked example and exercises the lexicographic comparison
> path. The other three bid-field differences exercise tiers 1, 2, 3 (profit,
> expected_return, bid_amount — separated by profit alone in this dataset).
> Tier 4 (timestamp) is exercised by
> `best_bid_matches_first_ranked_on_timestamp_tie_breaker`, so the doc
> example does not duplicate that test.

## 4. `get_best_bid` and `rank_bids` share one comparator

Both public APIs route through one helper to guarantee the invariant
*“the best bid is identical to the first ranked bid for the same invoice and
ledger state”*:

```text
get_best_bid ─► select_best_placed_bid ─► compare_bids  (linear scan)
                  ▲
rank_bids   ─► select_best_index     ─► compare_bids  (selection sort)
```

A change in `compare_bids` automatically applies to both entry points; a
change elsewhere that breaks the invariant is caught by the property tests in
§5.

### 4.1 Edge cases covered by the comparator

| Edge                                              | Behavior                                              |
| :------------------------------------------------ | :---------------------------------------------------- |
| Identical bids (all fields equal)                 | Returns `Ordering::Equal` (cannot occur in production — `bid_id` is unique). |
| Bid with `expected_return < bid_amount`           | Profit = 0 via `saturating_sub`; falls through to lower tiers. |
| Two bids with same `bid_id`                       | Only possible with malformed state; comparator returns `Equal`. |
| Empty invoice bid list                            | `get_best_bid → None`, `rank_bids → empty Vec`.       |
| Vector iteration order differs between validators | **No effect** — selection sort + linear scan both compare every pair. |
| All bids non-`Placed`                             | Both APIs return `None` / empty `Vec`.                |

## 5. How the comparator is verified

The contract ships with three layers of tests that pin every tier of the
comparator in place. All three run from the `quicklendx-contracts/` directory.

### 5.1 Concrete assertions (`src/test_bid_ranking.rs`)

| Test                                                      | Tier exercised              |
| :-------------------------------------------------------- | :-------------------------- |
| `rank_bids_orders_by_profit_and_expected_return`          | 1 (profit) and 2 (ER)       |
| `best_bid_matches_first_ranked_on_expected_return_tie`    | 2 (ER)                      |
| `best_bid_matches_first_ranked_on_bid_amount_tie_breaker` | 3 (bid amount)              |
| `best_bid_matches_ranked_with_mixed_statuses`            | status filter               |
| `best_bid_matches_first_ranked_on_timestamp_tie_breaker`  | 4 (timestamp)               |
| `best_bid_matches_first_ranked_on_bid_id_final_tie_breaker` | 5 (bid_id)                |
| `best_bid_matches_first_ranked_independent_of_insertion_order_on_ties` | insertion-order independence |
| `best_bid_returns_none_when_all_expired`                 | empty + expiration filter  |
| `best_bid_matches_ranked_idempotent_cleanup`              | cleanup idempotency         |

### 5.2 Determinism / order axioms

Tier-1 contract invariants (reflexive, antisymmetric, transitive, total) are
encoded by `src/test_bid_ranking_tiebreaker.rs` (issue #811) and by the
property-test suite in [`src/test_bid_compare_order_props.rs`](../quicklendx-contracts/src/test_bid_compare_order_props.rs),
which is gated behind the `fuzz-tests` feature.

### 5.3 Running the tests locally

```bash
cd quicklendx-contracts

# Tier assertions + invariant checks (always runs in CI)
cargo test --lib test_bid_ranking

# Property-based axioms with a fixed seed (reproducible across machines)
QUICKLENDX_SEED=42 cargo test --features fuzz-tests test_bid_compare_order_props

# Or without the feature gate: works because the property tests are
# behind #[cfg(feature = "fuzz-tests")] and the unit tests are not.
cargo test --lib test_bid_ranking_tiebreaker
```

## 6. Cross-references

- **`docs/contracts/bid-ranking.md`** — the lower-level tie-breaker spec
  (tier details, security properties, full test matrix). Use this page for the
  exact wording of the comparison and the security argument; use the page you
  are reading for the *purpose*, *structure*, and *contribution workflow*.
- **`quicklendx-contracts/src/bid.rs`** — source of `compare_bids`,
  `get_best_bid`, and `rank_bids`. Update this file when changing the
  comparator and update the docs in the same PR.
- **`quicklendx-contracts/src/test_bid_ranking.rs`** — concrete-per-tier
  tests; extend this file when adding a new comparison tier.
- **`quicklendx-contracts/src/test_bid_compare_order_props.rs`** —
  antisymmetry / transitivity / totality properties. Extend it whenever a
  new comparator field is introduced.
- **[`README.md`](../README.md)** and
  **[`quicklendx-contracts/README.md`](../quicklendx-contracts/README.md)** —
  entry points for contributors from outside the contracts package; both
  link to this page from their docs index.

## 7. Contributing — change checklist

When you modify the comparator, walk every step below so reviewers can verify
that determinism is preserved:

1. Update `BidStorage::compare_bids` in
   [`quicklendx-contracts/src/bid.rs`](../quicklendx-contracts/src/bid.rs)
   and document the new tier in **both** docs:
   [`docs/contracts/bid-ranking.md`](contracts/bid-ranking.md) (the
   lower-level spec) and [`docs/BID_RANKING.md`](BID_RANKING.md) (this
   file). Keeping both in lock-step is the single biggest contributor
   guard against stale documentation.
2. Add a concrete-per-tier unit test in `src/test_bid_ranking.rs` that pins
   the new ordering.
3. If the tier is data-driven, extend `test_bid_compare_order_props.rs` to
   keep antisymmetry / transitivity / totality for the new field.
4. Re-run the three commands in §5.3 and confirm `clippy` is clean (see
   `AGENTS.md`).
5. Reference `Closes #1506` in the PR description (the PR itself — not the
   doc body) and link the touched doc page in the PR summary.
