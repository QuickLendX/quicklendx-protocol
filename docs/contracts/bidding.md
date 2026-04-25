# Bid Cancellation Authorization

## Overview

`cancel_bid` transitions a bid from `Placed` to `Cancelled`. It is
**strictly investor-only** — only the investor who placed the bid may cancel it.
There is no admin override.

## Authorization Matrix

| Caller | Bid status | Outcome |
|--------|------------|---------|
| Bid owner (investor) | `Placed` | ✅ `true` — status → `Cancelled` |
| Bid owner | `Cancelled` | `false` (no-op, idempotent) |
| Bid owner | `Accepted` | `false` (no-op) |
| Bid owner | `Withdrawn` | `false` (no-op) |
| Bid owner | `Expired` | `false` (no-op) |
| Third party | `Placed` | ❌ Auth panic |
| Business owner | `Placed` | ❌ Auth panic |
| **Admin** | `Placed` | ❌ Auth panic — **no admin override** |
| Non-existent bid | — | `false` (no-op) |

## Implementation

```rust
// src/bid.rs — BidStorage::cancel_bid
pub fn cancel_bid(env: &Env, bid_id: &BytesN<32>) -> bool {
    if let Some(mut bid) = Self::get_bid(env, bid_id) {
        // SECURITY: investor must authorize their own cancellation
        bid.investor.require_auth();
        if bid.status == BidStatus::Placed {
            bid.status = BidStatus::Cancelled;
            Self::update_bid(env, &bid);
            return true;
        }
    }
    false
}
```

`require_auth()` is called on `bid.investor` — any other signer causes a
host-level authorization failure before any state is mutated.

## cancel_bid vs withdraw_bid

Both operations are investor-only and transition a `Placed` bid to a terminal
state. They produce **distinct** statuses:

| Operation | Terminal status | Notes |
|-----------|----------------|-------|
| `cancel_bid` | `Cancelled` | Simpler path, no KYC check |
| `withdraw_bid` | `Withdrawn` | Checks investor KYC is not Pending |

## Security Notes

- **No admin override**: Admin cannot cancel bids on behalf of investors.
  This prevents griefing attacks where a compromised admin key could
  disrupt active bids.
- **Fail-safe on missing bid**: Returns `false` rather than panicking.
- **Idempotent on terminal states**: Calling `cancel_bid` on an already
  `Cancelled`, `Accepted`, `Withdrawn`, or `Expired` bid returns `false`
  without mutating state.
- **Field immutability**: Only `status` changes; all other bid fields
  (`investor`, `bid_amount`, `invoice_id`, etc.) are preserved.
- **Isolation**: Cancelling one bid does not affect other bids on the
  same invoice.

## Test Coverage (Issue #793)

`src/test_bid.rs` — 15 tests across 6 groups:

| Group | Tests | What is verified |
|-------|-------|-----------------|
| Happy path | 2 | Investor cancels own Placed bid; returns true |
| Idempotency | 4 | Cancelled/Accepted/Withdrawn/non-existent → false |
| Authorization matrix | 4 | Third party, business, admin, other investor all rejected |
| State integrity | 2 | Fields preserved; other bids unaffected |
| withdraw vs cancel | 2 | Both enforce investor auth; produce distinct statuses |
| Multiple bids | 1 | Investor can cancel each of their own bids independently |

```bash
cd quicklendx-contracts
cargo test test_bid
```
