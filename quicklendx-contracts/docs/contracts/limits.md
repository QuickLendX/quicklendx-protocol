# Protocol Limits: Bid Caps

## Maximum Bids Per Invoice
To prevent unbound storage growth and ensure deterministic execution costs, the maximum number of active bids allowed per invoice is capped at `MAX_BIDS_PER_INVOICE` (default: 50).

### Rationale
- **Gas Costs**: Soroban smart contracts have strict gas and instruction limits. Scanning unbounded arrays of bids can cause operations to exceed these limits, leading to failed transactions.
- **Storage Optimization**: Keeping the active list bounded ensures predictable reads and writes on the `bids` storage key for an invoice.

### Lifecycle & Cleanup
The cap limits the number of **active** (Placed) bids. Expired or terminal bids (Accepted, Withdrawn, Cancelled) are pruned from this limit using the `cleanup_expired_bids` mechanism.

When a new bid is placed, the contract first executes an automatic cleanup which transitions any expired bids from `Placed` to `Expired`, thus potentially freeing up capacity within the quota. If after cleanup the cap is still reached, the `MaxBidsPerInvoiceExceeded` error is thrown.
