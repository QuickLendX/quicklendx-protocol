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

4. **Timestamp (first-come)** – `timestamp`  
   If all of the above are equal, the earlier bid (smaller timestamp) ranks higher.

Comparison uses saturating arithmetic for profit to avoid overflow. The order is deterministic and does not depend on storage iteration order.

## Entrypoints

### `get_best_bid(env, invoice_id) -> Option<Bid>`

Returns the single highest-ranked bid for the invoice that is in `Placed` status. Returns `None` if the invoice does not exist, has no bids, or has no placed bids (e.g. all withdrawn or expired).

### `get_ranked_bids(env, invoice_id) -> Vec<Bid>`

Returns all bids for the invoice that are in `Placed` status, sorted from best to worst. Expired bids are refreshed before ranking; withdrawn, accepted, and expired bids are excluded. Returns an empty vector for a non-existent invoice or when there are no placed bids.

## Behavior

- **Only Placed bids** are considered. Withdrawn, accepted, and expired bids are excluded from ranking and from `get_best_bid`.
- **Expiration**: Before ranking, the contract refreshes expired bids (updates status to `Expired`). So ranked results always reflect current placement status.
- **Consistency**: `get_best_bid(invoice_id)` is equal to the first element of `get_ranked_bids(invoice_id)` when the latter is non-empty.
- **Determinism**: Same set of bids always produces the same ranking; tie-breaks are fully specified above.

## Security and Testing

- Ranking logic is unit-tested in `test_bid_ranking.rs` (empty list, single bid, multiple bids, equal bids, best-bid selection, non-existent invoice).
- `compare_bids` uses `saturating_sub` for profit to avoid overflow (see `test_overflow.rs`).
- No external or mutable state is used for ordering beyond bid fields and ledger timestamp for expiration.
