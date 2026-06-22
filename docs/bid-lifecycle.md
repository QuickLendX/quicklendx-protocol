# Bid Lifecycle, Ranking, and TTL Configuration

This guide documents the bidding rules implemented in
`quicklendx-contracts/src/bid.rs` for integrators, indexers, and operators.

## Lifecycle

1. A bid is created with status `Placed`.
2. The bid id is indexed globally, under the investor, and under the invoice.
3. Ranking helpers only consider bids still in `Placed` status.
4. A placed bid becomes expired once the ledger timestamp moves strictly past
   its `expires_at` boundary.
5. Cleanup helpers transition expired placed bids to `Expired` and remove them
   from active invoice/investor indexes.
6. Terminal bids such as `Accepted`, `Withdrawn`, and `Cancelled` are preserved
   for audit history and are not modified by cleanup.

At the exact expiry timestamp the bid is still valid. Expiry begins after that
boundary, which avoids off-by-one surprises for callers checking the same
ledger second.

## Ranking rules

`BidStorage::compare_bids` applies a deterministic five-level comparator. Higher
values win at every economic level.

| Priority | Field | Rule |
| --- | --- | --- |
| 1 | Profit | `expected_return - bid_amount`; larger profit ranks higher. |
| 2 | Expected return | Larger `expected_return` ranks higher. |
| 3 | Bid amount | Larger `bid_amount` ranks higher. |
| 4 | Timestamp | Newer bid timestamp ranks higher. |
| 5 | Bid id | Byte ordering of `bid_id` is the final deterministic tie-breaker. |

`get_best_bid` and `rank_bids` both use the same comparator, so the first item
returned by `rank_bids` must match `get_best_bid` for the same invoice and
ledger state.

## TTL configuration

| Setting | Value |
| --- | --- |
| Default | 7 days |
| Minimum | 1 day |
| Maximum | 30 days |
| Getter | `get_bid_ttl_days` |
| Config snapshot | `get_bid_ttl_config` |
| Admin setter | `set_bid_ttl_days` |
| Reset | `reset_bid_ttl_to_default` |

TTL changes affect newly created bids; existing bids keep their stored
`expires_at` value.

## Active bid limits

- `MAX_BIDS_PER_INVOICE = 50` limits the number of bids indexed per invoice.
- `DEFAULT_MAX_ACTIVE_BIDS_PER_INVESTOR = 20` limits concurrent `Placed` bids
  per investor across invoices.
- `set_max_active_bids_per_investor(0)` disables the investor cap.
- `reset_max_active_bids_per_investor` restores the compile-time default.

Expired bids are refreshed before active counts are calculated, so stale placed
bids should not block an investor once cleanup has observed their expiry.

## Cleanup semantics

| Helper | Scope | Notes |
| --- | --- | --- |
| `refresh_expired_bids` | Invoice | Transitions expired placed bids and compacts invoice index. |
| `cleanup_expired_bids` | Invoice | Public wrapper for full invoice cleanup. |
| `cleanup_expired_bids_paged` | Invoice page | Processes bounded ranges for predictable instruction usage. |
| `refresh_investor_bids` | Investor | Prunes expired bids from the investor active index. |
| `count_active_placed_bids_for_investor` | Investor | Refreshes first, then counts non-expired placed bids. |

Cleanup never changes accepted, withdrawn, or cancelled records. Indexers should
treat terminal states as historical facts and expired states as routine
maintenance transitions.

## Operator guidance

- Keep the default 7-day TTL unless investor UX or market liquidity requires a
  shorter or longer window.
- Use shorter TTLs for fast-moving invoices where stale bids create confusion.
- Use the paged cleanup helper when an invoice is close to the 50-bid cap.
- Monitor bid expiry and TTL update events so frontends can explain why a bid
  disappeared from active ranking.
- Do not duplicate ranking logic off-chain without following the five-level
  comparator above.
