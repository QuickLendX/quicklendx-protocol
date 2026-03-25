# QuickLendX - Deterministic Bid Ranking

This note documents how bids are ranked and how ties are broken so every validator computes the same acceptance order.

## Ordering Rules

Bid ordering is **strictly deterministic** with the following priority (higher is better):
1. Profit: `expected_return - bid_amount`
2. Expected return
3. Bid amount
4. Timestamp (newer first)
5. `bid_id` lexicographically (newer IDs win because they embed timestamp + counter)

Only bids with `BidStatus::Placed` participate in ranking; `Withdrawn`, `Cancelled`, `Expired`, and `Accepted` bids are ignored.

## Key Functions

- `BidStorage::compare_bids(b1, b2)` — deterministic comparator implementing the rules above (uses `saturating_sub` for profits).
- `BidStorage::rank_bids(env, invoice_id)` — returns all placed bids for an invoice, sorted by the comparator.
- `BidStorage::get_best_bid(env, invoice_id)` — returns the top-ranked placed bid or `None`.

## Why a bid_id Tiebreaker?

When economics and timestamps match, ordering could otherwise depend on storage iteration. Using `bid_id` as the final tiebreaker ensures:
- Stable ordering across validators and replays.
- Reproducible simulations and fuzz tests.
- No reliance on insertion order or non-deterministic iteration.

`bid_id` already embeds `(timestamp, counter)` and is unique per bid, so it is safe to use as the final key.

## Security Notes

- No randomness is used; sorting is pure and deterministic.
- Profit math uses `saturating_sub` to avoid overflow.
- Expired bids are filtered via `refresh_expired_bids` before ranking.
- The maximum bids per invoice (`MAX_BIDS_PER_INVOICE`) limits sorting input to 50 elements, keeping the O(n²) selection pass inexpensive.

