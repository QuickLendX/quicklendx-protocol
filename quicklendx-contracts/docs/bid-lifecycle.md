# Bid Lifecycle, Ranking, and TTL Configuration

This guide documents the bidding behavior implemented in
[`src/bid.rs`](../src/bid.rs) and exposed through
[`src/lib.rs`](../src/lib.rs). Use it when building investor UIs, operator
jobs, or off-chain indexers that need to explain why a bid can be placed,
when it expires, and how the protocol chooses the best bid.

## Status Machine

Only `BidStatus::Placed` bids are active. All other states are terminal for
ranking and acceptance.

```text
Placed
  |-- accept_bid / accept_bid_and_fund --> Accepted
  |-- withdraw_bid by investor ---------> Withdrawn
  |-- cancel_bid by investor -----------> Cancelled
  |-- cleanup after strict expiry ------> Expired
```

Terminal bids are preserved for audit history. Cleanup does not mutate
`Accepted`, `Withdrawn`, or `Cancelled` records, even when their expiration
timestamp is in the past.

## Placement Preconditions

`place_bid` creates a `BidStatus::Placed` bid after these checks pass:

- the contract is not paused;
- the investor authorizes the call;
- `bid_amount` is positive;
- the invoice exists and is `InvoiceStatus::Verified`;
- the invoice currency is still whitelisted;
- the investor verification status is `Verified`;
- `bid_amount` does not exceed the investor's investment limit;
- stale bids for the invoice have been cleaned;
- the invoice has fewer than `MAX_BIDS_PER_INVOICE` active placed bids;
- the investor has fewer active placed bids than the configured per-investor
  limit, unless that limit is disabled.

The bid timestamp is the current ledger timestamp. The expiration timestamp is
computed from the active bid TTL configuration at placement time; later TTL
updates do not retroactively change existing bid expirations.

## TTL Configuration

Bid TTL is stored in whole days and controls the expiration timestamp assigned
to new bids.

| Constant | Value | Meaning |
| --- | ---: | --- |
| `DEFAULT_BID_TTL_DAYS` | `7` | Used when no admin override exists |
| `MIN_BID_TTL_DAYS` | `1` | Shortest accepted TTL |
| `MAX_BID_TTL_DAYS` | `30` | Longest accepted TTL |

Relevant entrypoints:

- `set_bid_ttl_days(days)` sets an admin override and rejects values outside
  `1..=30`.
- `get_bid_ttl_days()` returns the active TTL, falling back to `7`.
- `get_bid_ttl_config()` returns the active value, min, max, default, and
  whether the value is custom.
- `reset_bid_ttl_to_default()` removes the override and returns to `7`.

Expiry uses a strict comparison in `Bid::is_expired`:

```text
current_timestamp > expiration_timestamp
```

At exactly the expiration timestamp the bid remains active. One ledger-second
later it is expired and cleanup may transition it to `BidStatus::Expired`.

## Limits

Two caps keep bidding bounded and predictable:

| Limit | Value | Behavior |
| --- | ---: | --- |
| `MAX_BIDS_PER_INVOICE` | `50` | Maximum active placed bids accepted for one invoice |
| `DEFAULT_MAX_ACTIVE_BIDS_PER_INVESTOR` | `20` | Default active placed bids allowed across all invoices per investor |

The per-investor limit is configurable through the max-active-bids entrypoints
in `lib.rs`. A configured value of `0` means the investor limit is disabled.
The invoice cap remains enforced independently.

## Ranking Rules

`get_ranked_bids(invoice_id)` returns placed bids from best to worst.
`get_best_bid(invoice_id)` returns the same bid as the first ranked element
when the ranking is non-empty.

The comparator in `BidStorage::compare_bids` is deterministic and applies this
chain, with higher values preferred:

1. Profit: `expected_return - bid_amount` using saturating subtraction.
2. `expected_return`.
3. `bid_amount`.
4. `timestamp`, so newer bids win timestamp ties.
5. `bid_id` byte order as the final stable tiebreaker.

Non-placed bids are excluded from ranking, including `Accepted`, `Withdrawn`,
`Expired`, and `Cancelled`.

### Worked Ranking Example

Assume these active placed bids on the same invoice:

| Bid | `bid_amount` | `expected_return` | Profit | Timestamp | Result |
| --- | ---: | ---: | ---: | ---: | --- |
| A | `950` | `1_050` | `100` | `1000` | Beats B on expected return |
| B | `900` | `1_000` | `100` | `1001` | Loses to A on expected return |
| C | `950` | `1_050` | `100` | `1002` | Beats A on timestamp |
| D | `980` | `1_090` | `110` | `999` | Wins on profit before all lower-profit bids |

Final order is:

1. D, because profit `110` beats profit `100`.
2. C, because it ties A on profit, expected return, and amount, then wins on
   newer timestamp.
3. A, because it ties B on profit but has higher expected return.
4. B.

If two bids also tie on timestamp, the lexicographic `bid_id` comparison gives
validators a stable final order without relying on storage iteration.

## Cleanup Paths

Use cleanup to keep active bid indexes compact and to free invoice capacity
after expirations.

### `cleanup_expired_bids(invoice_id)`

Use the non-paged cleanup for normal invoices. It scans the current invoice
bid index, transitions expired placed bids to `Expired`, removes expired or
orphaned entries from the active index, preserves terminal audit records, and
returns the number of cleaned entries. It is idempotent for the same ledger
timestamp and invoice state.

### `cleanup_expired_bids_paged(invoice_id, offset, limit)`

Use paged cleanup for operator jobs that want predictable per-call work near
the `MAX_BIDS_PER_INVOICE = 50` ceiling. The `limit` parameter is capped at
`MAX_BIDS_PER_INVOICE`; `offset` is zero-based. The return value is
`(cleaned_count, total_count)` after the call.

Suggested operator pattern:

1. Start with `offset = 0`.
2. Choose a conservative `limit` such as `10` or `25`.
3. Repeat until calls return `cleaned_count = 0` for the ranges you monitor.

Because cleanup can compact the index as it removes expired entries, operators
that need to sweep a full invoice should be prepared to restart at `offset = 0`
after a cleaning pass.

## Integration Guidance

- Explain ranking using the five-step comparator above; do not sort by amount
  alone in UIs.
- Display the active TTL from `get_bid_ttl_config()` rather than hard-coding
  `7` days.
- Treat `Placed` as the only actionable bid state for acceptance or investor
  withdrawal.
- Run cleanup before presenting capacity-sensitive actions when an invoice is
  near `MAX_BIDS_PER_INVOICE`.
- For dashboards, show both invoice-level cap pressure and investor-level
  active-bid limit pressure; they fail independently.
