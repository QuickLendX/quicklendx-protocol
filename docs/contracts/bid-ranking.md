# Bid Ranking — Deterministic Tie-Breaker Specification

## Overview

QuickLendX uses a deterministic, multi-level comparator (`BidStorage::compare_bids`) to rank
competing bids for an invoice. Determinism is a hard requirement in Soroban execution: every
validator node must produce the same ranking for the same ledger state, regardless of the order
in which bids were submitted or stored.

## Tie-Breaker Priority

When two bids are compared, the following criteria are applied in order. The first criterion
that differs between the two bids determines the winner.

| Priority | Criterion | Winner |
|----------|-----------|--------|
| 1 | **Profit** (`expected_return − bid_amount`) | Higher profit |
| 2 | **Expected return** | Higher expected return |
| 3 | **Bid amount** | Higher bid amount |
| 4 | **Timestamp** | Newer (higher) timestamp |
| 5 | **Bid ID** (lexicographic byte order) | Higher bid ID |

The bid ID is a deterministic, monotonically-generated 32-byte value that encodes a timestamp
and a counter (see `EscrowStorage::generate_unique_escrow_id` for the analogous pattern). Because
no two bids can share the same ID, the comparator never returns `Equal` for two distinct bids.

## Key Functions

### `BidStorage::compare_bids(bid1, bid2) -> Ordering`

Deterministically compares two bids using the five-level priority above.

```rust
/// @notice Deterministically compares two bids.
/// @dev Ordering priority: (1) profit, (2) expected_return, (3) bid_amount,
/// (4) timestamp with newer bids first, (5) bid_id as final stable tiebreaker.
pub fn compare_bids(bid1: &Bid, bid2: &Bid) -> Ordering
```

**Security properties:**
- Reflexive: `compare_bids(a, a) == Equal`
- Antisymmetric: `compare_bids(a, b) == Greater` iff `compare_bids(b, a) == Less`
- Transitive: if `a > b` and `b > c` then `a > c`
- Total: for any two distinct bids, exactly one of `Greater` or `Less` is returned

### `BidStorage::rank_bids(env, invoice_id) -> Vec<Bid>`

Returns all `Placed` bids for an invoice sorted from best to worst using `compare_bids`.
Non-`Placed` bids (Withdrawn, Accepted, Expired, Cancelled) are excluded.

**Invariant:** If the result is non-empty, `result[0]` equals the value returned by
`get_best_bid` for the same invoice and ledger state.

### `BidStorage::get_best_bid(env, invoice_id) -> Option<Bid>`

Returns the single highest-ranked `Placed` bid. Uses the same `compare_bids` comparator
as `rank_bids` so the two functions cannot drift on tie handling.

**Invariant:** `get_best_bid(env, id) == rank_bids(env, id).get(0)` always holds.

## State Machine

Only `Placed` bids participate in ranking:

```
Placed    ──► included in rank_bids / get_best_bid
Withdrawn ──► excluded
Accepted  ──► excluded
Expired   ──► excluded
Cancelled ──► excluded
```

## Security Assumptions

1. **Determinism**: The comparator is a pure function of bid fields. No randomness, no
   ledger-state reads, no node-dependent values. Every validator produces the same result.

2. **Stability**: `get_best_bid` and `rank_bids` share the same comparator path
   (`select_best_placed_bid` / `select_best_index`). A change to one automatically
   applies to the other.

3. **Fairness**: The bid-ID tiebreaker is lexicographic on a deterministic byte sequence.
   It does not favour any particular investor address or submission time beyond what is
   already captured by the timestamp tier.

4. **No equal outcomes**: Because bid IDs are unique, `compare_bids` never returns `Equal`
   for two distinct bids. This prevents any ambiguity in best-bid selection.

5. **Insertion-order independence**: The selection sort used in `rank_bids` and the linear
   scan in `select_best_placed_bid` both iterate over the full bid list and apply
   `compare_bids` to every pair. The result is identical regardless of the order in which
   bids were stored.

## Test Coverage

All invariants above are codified in `src/test_bid_ranking_tiebreaker.rs` (issue #811).

| Test group | Tests | What is verified |
|------------|-------|-----------------|
| Tier 1 – Profit | 3 | Higher profit wins; 3-bid descending order |
| Tier 2 – Expected return | 3 | Higher expected_return wins; insertion-order independence |
| Tier 3 – Bid amount | 3 | Higher bid_amount wins; insertion-order independence |
| Tier 4 – Timestamp | 4 | Newer timestamp wins; 3-bid descending; insertion-order independence |
| Tier 5 – Bid ID | 4 | Higher bid_id wins; 3-bid descending; insertion-order independence |
| Reflexivity / symmetry | 3 | Reflexive, antisymmetric, transitive |
| Non-Placed exclusion | 6 | Withdrawn, Accepted, Expired, Cancelled excluded; empty result |
| best == first-ranked | 5 | Invariant holds at every tie level |
| Large / stress | 2 | 10 bids distinct profits; 5 full-tie bids by bid_id |
| Cross-invoice isolation | 1 | Bids on different invoices do not interfere |
| Single bid | 1 | Single bid is its own best and sole ranked entry |
| **Total** | **35** | **35 passed, 0 failed** |

## Running the Tests

```bash
cd quicklendx-contracts
cargo test --lib test_bid_ranking_tiebreaker
```

Expected output:
```
running 35 tests
test result: ok. 35 passed; 0 failed; 0 ignored
```
