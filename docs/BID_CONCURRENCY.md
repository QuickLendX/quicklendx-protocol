# Bid Concurrency & Race Safety

## Concurrency Model

Soroban executes transactions **sequentially** within a ledger. "Concurrent"
requests means different ledger orderings — the protocol must be safe
regardless of which transaction is ordered first.

## Invariants

| Invariant | Enforcement |
|---|---|
| At most one escrow per invoice | Two-layer guard in `escrow::load_accept_bid_context` (invoice status + escrow/investment record check) |
| Only Placed bids can be accepted | `bid.status == BidStatus::Placed` check in `load_accept_bid_context` |
| Stale/expired bids rejected explicitly | `BidStale` error for expired bids, `InvalidStatus` for non-Placed |
| Cancelled bids cannot be accepted | Status check rejects non-Placed bids |
| Ranking is deterministic | `compare_bids` total order with `bid_id` final tiebreaker |
| Cleanup is idempotent | `refresh_expired_bids` returns 0 on second call |

## Error Semantics

| Error | Meaning | Client Action |
|---|---|---|
| `BidStale` (2219) | Bid was cancelled, expired, or otherwise left `Placed` status between read and operation | Re-read bid state, select a new best bid if needed, resubmit |
| `InvalidStatus` (1401) | General lifecycle violation (e.g., invoice not in expected state) | Check invoice status before retrying |
| `InvoiceAlreadyFunded` (1002) | Invoice already has escrow — double-funding prevented | No retry needed; invoice is already funded |
| `StorageKeyNotFound` (1301) | Bid or invoice does not exist | Verify the ID is correct |

## Client Retry Contract

When a client receives `BidStale`:

1. Re-read the bid state via `get_bid(bid_id)`
2. If the bid is still `Placed` and not expired, retry the operation
3. If the bid is in a terminal state (`Cancelled`, `Accepted`, `Expired`, `Withdrawn`):
   - For acceptance: call `get_best_bid(invoice_id)` or `get_ranked_bids(invoice_id)` to find the current best bid
   - For cancellation: no further action needed (bid is already terminal)
4. Resubmit with the updated bid ID

When a client receives `InvoiceAlreadyFunded`:
- No retry needed; the invoice has been successfully funded by another bid

## Breaking Changes

### `cancel_bid` Return Type

**Before:** `cancel_bid(bid_id) -> bool`
- `true` if bid was cancelled
- `false` if bid not found or already in terminal state

**After:** `cancel_bid(bid_id) -> Result<(), QuickLendXError>`
- `Ok(())` if bid was successfully cancelled
- `Err(StorageKeyNotFound)` if bid does not exist
- `Err(BidStale)` if bid is not in `Placed` status

### New Error Variant

`BidStale = 2219` — replaces `InvalidStatus` for expired bid rejections in
`load_accept_bid_context` and `verify_bid_match`. Clients checking for
`InvalidStatus` on expired bids must update to check for `BidStale`.

## Migration Notes

- Update client code to handle `Result` from `cancel_bid`
- Match on `BidStale` error where `InvalidStatus` was previously used for expired bids
- No storage migration required; all changes are in error reporting only

## Security Assumptions

- Soroban's sequential execution model serializes all storage mutations within a ledger
- The two-layer guard in `load_accept_bid_context` is the primary defense against double-funding
- `BidStale` improves observability without changing the underlying execution model
- No new state variables or optimistic locking are added
