# Cursor Guarantees and Snapshot Generations

This document is written for **downstream integrators** querying the QuickLendX API or Soroban contracts for lists of invoices, bids, or protocol events.

When querying lists of records, the platform provides pagination cursors to fetch subsequent pages. Because the underlying blockchain state is constantly advancing, this document defines what guarantees are provided regarding data stability across pages.

## Cursor Stability

Cursors returned by the QuickLendX API/Contracts are **opaque tokens**. 

While you may observe a format like `1000_25` (representing `{indexed_ledger_seq}_{offset}`), **do not manually parse this format**. Treat the cursor as a raw string. The internal structure may change without warning in future upgrades.

### Guarantees
1. **Opaque usage:** If you pass the exact `cursor` string received from a previous response to a subsequent request, the system will correctly resume pagination.
2. **No Expiration:** Cursors do not mathematically "expire". However, because they are tied to historical blockchain state, nodes may prune old ledgers. If you attempt to use a cursor pointing to a ledger that has been garbage-collected by the RPC provider, the request will fail with a `StaleDataRejected` or `410 Gone` error.

## Snapshot Generations

To ensure that integrators don't see inconsistent data (such as missing a newly created invoice or seeing the same invoice twice if it was modified during pagination), QuickLendX uses **Snapshot Generations**.

When you begin a paginated request without a cursor, you are bound to the current ledger sequence—this becomes your Snapshot Generation.

### Example: Paginating Invoices

Suppose you are querying the `/invoices` endpoint, which currently has 100 records at ledger sequence `1000`.

1. **Request 1:** You request `limit=50`.
   - **Response:** Returns 50 invoices and `cursor: "1000_50"`.
2. **State Change:** An invoice is created. The ledger sequence advances to `1001`. There are now 101 invoices on-chain.
3. **Request 2:** You request `limit=50` and pass `cursor: "1000_50"`.
   - **Response:** The system resolves the cursor and continues reading from the snapshot at ledger `1000`. You receive the remaining 50 invoices from the original set. You do *not* see the new invoice created at ledger `1001`.

### End of Snapshot

When a response returns an empty cursor (or an explicit `has_next_page: false`), the snapshot generation is fully consumed. To see new records, you must initiate a new request without a cursor to establish a new snapshot generation at the latest ledger.

## Dealing with Stale Cursors

If you pause pagination for a long period (e.g., hours or days), the underlying RPC node may drop the historical state required to serve the snapshot.

If you receive a `StaleDataRejected` error while paginating:
1. Discard your current list state.
2. Initiate a new request without a cursor to start over at the latest Snapshot Generation.
