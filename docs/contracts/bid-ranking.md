# Bid Ranking and Best Bid Selection

## Overview

Bids on an invoice are ranked by a deterministic algorithm so that businesses can select the best offer. The contract exposes `get_best_bid` (single best bid) and `get_ranked_bids` (all placed bids in rank order).

## Ranking Criteria (in order)

1. **Profit (absolute)** – `expected_return - bid_amount`  
   Higher profit ranks higher. Favors offers that give the investor more return per unit of funding.

2. **Expected return** – `expected_return`  
   If profit is equal, higher expected return ranks higher.

3. **Bid amount** – `bid_amount`  
   If profit and expected return are equal, higher bid amount ranks higher.

4. **Timestamp (recency preference)** – `timestamp`  
   If all of the above are equal, the newer bid (larger timestamp) ranks higher.

5. **Bid ID (deterministic final tie-breaker)** – `bid_id`  
   If economics and timestamp are equal, lexicographically larger `bid_id` ranks higher.

Comparison uses saturating arithmetic for profit to avoid overflow. The order is deterministic and does not depend on storage iteration order.

## Entrypoints

### `get_best_bid(env, invoice_id) -> Option<Bid>`

Returns the single highest-ranked bid for the invoice that is in `Placed` status. Returns `None` if the invoice does not exist, has no bids, or has no placed bids (e.g. all withdrawn or expired).

### `get_ranked_bids(env, invoice_id) -> Vec<Bid>`

Returns all bids for the invoice that are in `Placed` status, sorted from best to worst. Expired bids are refreshed before ranking; withdrawn, accepted, and expired bids are excluded. Returns an empty vector for a non-existent invoice or when there are no placed bids.

### `cleanup_expired_bids(env, invoice_id) -> u32`

Scans and prunes expired bids from an invoice's bid list. Transitions `Placed` bids that have passed their expiration timestamp to `Expired` status. Removes already-expired or orphaned bid records from the index. Returns the count of cleaned items. This operation is idempotent and safe to call multiple times.

## Behavior

- **Only Placed bids** are considered. Withdrawn, accepted, and expired bids are excluded from ranking and from `get_best_bid`.
- **Expiration**: Before ranking, the contract refreshes expired bids (updates status to `Expired`). So ranked results always reflect current placement status.
- **Consistency**: `get_best_bid(invoice_id)` is equal to the first element of `get_ranked_bids(invoice_id)` when the latter is non-empty.
- **Determinism**: Same set of bids always produces the same ranking; tie-breaks are fully specified above.

## Security Invariants

### Invariant 1: Best-Bid Equals First Ranked
```
∀ invoice_id: rank_bids(invoice_id).len() > 0 
  → get_best_bid(invoice_id) == rank_bids(invoice_id).get(0)
```
This invariant is maintained even after:
- Bid expiration (high-ranked bid expires, next-best becomes best)
- Cleanup operations (partial or full expiration cleanup)
- Multiple cleanup calls (idempotent behavior)
- Mixed bid statuses (Placed/Withdrawn/Accepted/Expired/Cancelled)

### Invariant 2: Expiration Cleanup Preservation
Terminal statuses (`Accepted`, `Withdrawn`, `Cancelled`) are **never mutated** by cleanup. Only `Placed` bids can transition to `Expired`.

### Invariant 3: Stable Ordering
The ranking algorithm uses only immutable bid fields and the ledger timestamp. No external state affects ordering, ensuring reproducible results across validators.

### Invariant 4: Idempotent Cleanup
Running `cleanup_expired_bids` multiple times at the same timestamp produces the same result. Already-expired bids are silently handled.

## Regression Tests Coverage

### Tie Scenarios
- Profit equality → expected_return order
- Profit + expected_return equality → bid_amount order
- All economics equal → timestamp order (newer wins)
- Full tie → bid_id lexicographic order (larger wins)
- Insertion-order variance (different storage order yields same result)

### Expiration and Cleanup Scenarios (NEW)
- Best bid remains correct after highest bid expires
- Expired bids excluded from rank_bids after cleanup
- Mixed bid statuses correctly filtered (only Placed considered)
- Partial expiration cleanup maintains invariant
- All bids expired → both return empty
- Multiple cleanup calls are idempotent

## Security and Testing

- Ranking logic is unit-tested in `test_bid_ranking.rs` (empty list, single bid, multiple bids, equal bids, tie-layer regressions, best-vs-ranked invariant, non-existent invoice).
- Expiration and cleanup scenarios tested in `test_bid_ranking.rs` (see `best_bid_matches_ranked_after_expiration`, `rank_bids_excludes_expired_after_cleanup`, `best_bid_matches_ranked_with_mixed_statuses`, `cleanup_after_partial_expiration_maintains_invariant`, `best_bid_returns_none_when_all_expired`, `best_bid_matches_ranked_idempotent_cleanup`).
- `compare_bids` uses `saturating_sub` for profit to avoid overflow (see `test_overflow.rs`).
- No external or mutable state is used for ordering beyond bid fields and ledger timestamp for expiration.
