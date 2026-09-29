# Bid Lifecycle

This document describes the complete lifecycle of a bid in the QuickLendX protocol,
from placement through its terminal states. Audience: **contributors and downstream
integrators** who need to understand how bid state transitions work on-chain.

## State Diagram

```
                        ┌──────────┐
                        │  Placed  │  ◄─── placed by a verified investor
                        └────┬─────┘
              ┌──────────────┼──────────────┐
              │              │              │
         cancel_bid    withdraw_bid     expiry (time)
              │              │              │
              ▼              ▼              ▼
        ┌──────────┐  ┌────────────┐  ┌──────────┐
        │Cancelled │  │ Withdrawn  │  │ Expired  │
        └──────────┘  └────────────┘  └──────────┘

        The only non-terminal transition:
        Placed ──accept_bid──▶ Accepted
                                 │
                     (invoice status → Funded)
```

All four terminal states (`Accepted`, `Cancelled`, `Withdrawn`, `Expired`) are
**irreversible**. Once a bid leaves `Placed`, no further status changes are allowed.

## Status Reference

| Status       | Terminal? | Description                                              |
|--------------|-----------|----------------------------------------------------------|
| `Placed`     | No        | Live bid visible to the business; may be accepted.       |
| `Accepted`   | Yes       | Business accepted the bid; funds locked in escrow.       |
| `Withdrawn`  | Yes       | Investor voluntarily withdrew before acceptance.         |
| `Expired`    | Yes       | Bid TTL elapsed without acceptance; pruned automatically.|
| `Cancelled`  | Yes       | Investor cancelled their own placed bid.                 |

Source: `BidStatus` in [`quicklendx-contracts/src/types.rs`](../quicklendx-contracts/src/types.rs).

## Entrypoints by Transition

### Placed (creation)

```rust
contract.place_bid(
    env, investor: Address, invoice_id: BytesN<32>,
    bid_amount: i128, expected_return: i128, salt: BytesN<32>,
) -> Result<BytesN<32>, QuickLendXError>
```

- **Caller**: investor (self-authorised via `investor.require_auth()`).
- **Preconditions**:
  - Invoice exists and is `Verified`.
  - Investor KYC is `Verified` and `bid_amount ≤ investment_limit`.
  - `bid_amount > 0` and `expected_return ≥ bid_amount`.
  - Invoice currency is still on the whitelist.
  - Active bid count on invoice `< MAX_BIDS_PER_INVOICE` (50).
  - Investor has not reached their active-bid limit (default 20).
  - Idempotency salt must not have been used before.
- **Effect**: `Bid { status: Placed, ... }` stored; bid ID indexed by invoice and investor;
  `expiration_timestamp = now + bid_ttl_days`; emits `bid_placed` event.
- **Pause-gated**: rejected with `ContractPaused` when emergency breaker is engaged.

### Placed → Accepted

```rust
contract.accept_bid(
    env, invoice_id: BytesN<32>, bid_id: BytesN<32>,
) -> Result<(), QuickLendXError>
```

- **Caller**: business owner of the invoice.
- **Precondition**: invoice is `Verified`; bid is `Placed`; business KYC is not pending.
- **Effect**: Funds transferred into escrow; `bid.status = Accepted`;
  `invoice.status = Funded`; `Investment { status: Active }` created;
  invoice moved from `Verified` to `Funded` status index;
  emits `bid_accepted` and `escrow_created` events.
- **Race safety**: protected by reentrancy guard; re-reads bid status after auth.

### Placed → Cancelled

```rust
contract.cancel_bid(env, bid_id: BytesN<32>) -> bool
```

- **Caller**: investor who placed the bid (self-authorised inside `BidStorage::cancel_bid`).
- **Precondition**: bid exists and is `Placed`.
- **Effect**: `bid.status = Cancelled`; emits `bid_cancelled` event; returns `true`.
  Returns `false` (no state mutation) if bid not found or not `Placed`.
- **Pause-gated**.

### Placed → Withdrawn

```rust
contract.withdraw_bid(env, bid_id: BytesN<32>) -> Result<(), QuickLendXError>
```

- **Caller**: investor who placed the bid.
- **Precondition**: bid is `Placed`; investor KYC is not `Pending`.
- **Effect**: `bid.status = Withdrawn`; emits `bid_withdrawn` event.
  Returns `OperationNotAllowed` if bid is already in a terminal state.
- **Pause-gated**.

### Placed → Expired (automatic)

No explicit entrypoint transitions a bid to `Expired`. The transition happens
inside the following paths once the bid becomes **cleanup-eligible**
(`current_timestamp ≥ expiration_timestamp + bid_expiry_grace_seconds`; see
[Bid Expiry Grace Period](#bid-expiry-grace-period) below):

- `cleanup_expired_bids` / `cleanup_expired_bids_paged` — public permissionless
  cleanup that scans and prunes expired bids.
- `get_bid_records_for_invoice` — triggers `refresh_expired_bids` as a side effect.
- `refresh_investor_bids` — prunes expired bids from the per-investor index.

Emits `bid_expired` event on each transition from `Placed` → `Expired`.

```rust
// Permissionless — anyone may trigger cleanup for any invoice.
contract.cleanup_expired_bids(env, invoice_id: BytesN<32>) -> u32

// Paginated variant for gas safety on large bid lists.
contract.cleanup_expired_bids_paged(
    env, invoice_id: BytesN<32>, offset: u32, limit: u32,
) -> (u32 /* cleaned */, u32 /* remaining */)
```

## Bid TTL Configuration

| Constant                  | Value | Admin entrypoint            |
|---------------------------|-------|-----------------------------|
| `DEFAULT_BID_TTL_DAYS`    | 7     | `reset_bid_ttl_to_default`  |
| `MIN_BID_TTL_DAYS`        | 1     | —                           |
| `MAX_BID_TTL_DAYS`        | 30    | —                           |
| `MAX_BIDS_PER_INVOICE`    | 50    | —                           |
| `DEFAULT_MAX_ACTIVE_BIDS` | 20    | `set_max_active_bids_per_investor` |

TTL can be configured by admin between 1 and 30 days via `set_bid_ttl_days`.
The investor active-bid limit can be set to any `u32`; `0` disables enforcement.
Each change emits a `ttl_upd` event for auditability.

## Bid Expiry Grace Period

Once a bid's `expiration_timestamp` passes, it is immediately treated as
expired for **acceptance** (`accept_bid` rejects it) and for **active-bid
counting** (`get_active_bid_amount_sum_for_investor`,
`count_active_placed_bids_for_investor`) — this is unchanged and governed by
the raw `Bid::is_expired` predicate.

What *is* delayed is the permissionless cleanup path: the storage transition
from `Placed` to `Expired` (and the accompanying index pruning) performed by
`cleanup_expired_bids` / `cleanup_expired_bids_paged` / `refresh_expired_bids`
/ `refresh_investor_bids` only fires once `Bid::is_cleanup_eligible` is true,
i.e. `current_timestamp >= expiration_timestamp + bid_expiry_grace_seconds`.
This gives investors (or the wider system) a buffer window after raw expiry
before any third-party caller can force the cleanup, without ever making an
already-dead bid acceptable again.

| Constant                              | Value        | Admin entrypoint                        |
|----------------------------------------|--------------|------------------------------------------|
| `DEFAULT_BID_EXPIRY_GRACE_SECONDS`     | 0 (no grace — matches pre-existing immediate-cleanup behaviour) | `reset_bid_expiry_grace_to_default` |
| `MIN_BID_EXPIRY_GRACE_SECONDS`         | 0            | —                                          |
| `MAX_BID_EXPIRY_GRACE_SECONDS`         | 2592000 (30 days) | —                                     |

The default of `0` is intentional: it keeps out-of-the-box cleanup behaviour
byte-for-byte identical to before this feature existed. Operators opt into a
buffer window by calling `set_bid_expiry_grace_seconds` with a positive value.

```rust
// Admin-only.
contract.set_bid_expiry_grace_seconds(env, seconds: u64) -> Result<u64, QuickLendXError>
contract.reset_bid_expiry_grace_to_default(env) -> Result<u64, QuickLendXError>

// Read-only.
contract.get_bid_expiry_grace_seconds(env) -> u64
contract.get_bid_expiry_grace_config(env) -> BidExpiryGraceConfig
```

`set_bid_expiry_grace_seconds` rejects out-of-range values with
`InvalidTimestamp` (matching the convention used by the invoice-side
`defaults::resolve_grace_period`), and emits a `BidExpiryGraceUpdated` event
on every successful change (including resets).

## Expiry Semantics

```rust
// A bid is expired when current_timestamp >= expiration_timestamp.
// Valid until the second before expiry; expired at the expiry boundary.
pub fn is_expired(&self, current_timestamp: u64) -> bool {
    current_timestamp >= self.expiration_timestamp
}

// A bid becomes eligible for permissionless cleanup once the grace period
// has also elapsed on top of the raw expiry boundary.
pub fn is_cleanup_eligible(&self, current_timestamp: u64, grace_seconds: u64) -> bool {
    current_timestamp >= self.expiration_timestamp.saturating_add(grace_seconds)
}
```

- **Idempotent cleanup**: multiple calls on the same state return 0 on the second call.
- **Terminal bid preservation**: `Accepted`, `Withdrawn`, and `Cancelled` bids are
  never touched by cleanup, even if past their expiration timestamp.
- **Bounded iteration**: the invoice bid index is capped at `MAX_BIDS_PER_INVOICE` (50).

## Bid Ranking (deterministic)

The protocol uses a 5-tier comparator when presenting bids to the business:

| Tier | Field                        | Winner          |
|------|------------------------------|-----------------|
| 1    | Profit (`expected_return - bid_amount`) | Higher wins     |
| 2    | `expected_return`            | Higher wins     |
| 3    | `bid_amount`                 | Higher wins     |
| 4    | `timestamp`                  | Newer wins      |
| 5    | `bid_id` (32-byte lexicographic) | Higher wins |

Full specification: [`docs/BID_RANKING.md`](BID_RANKING.md).

Read-only entrypoints:

```rust
contract.get_best_bid(env, invoice_id: BytesN<32>) -> Option<Bid>
contract.get_ranked_bids(env, invoice_id: BytesN<32>) -> Vec<Bid>
```

## Cross-Module State Alignment

After `accept_bid`, the accepted bid locks the invoice and investment into a
consistent state:

| Invoice Status | Required Bid Status | Required Investment Status | Escrow      |
|----------------|--------------------|---------------------------|-------------|
| `Funded`       | `Accepted`         | `Active`                  | `Held`      |
| `Paid`         | `Accepted`         | `Completed`               | `Released`  |
| `Defaulted`    | `Accepted`         | `Defaulted`               | N/A         |
| `Refunded`     | `Cancelled`        | `Refunded`                | `Refunded`  |

Detailed cross-module invariants: [`docs/contracts/lifecycle.md`](contracts/lifecycle.md).

## Key Invariants

1. **Single non-terminal state.** `Placed` is the only status from which bids
   may transition. Terminal bids are immutable. Enforced by status checks in
   every mutating entrypoint.

2. **Exactly one accepted bid per funded invoice.** When `accept_bid`
   transitions an invoice to `Funded`, that bid becomes `Accepted`. No other
   bid on the same invoice may transition to `Accepted` afterwards.

3. **Expiry is monotonic.** Once a bid becomes `Expired`, time never moves
   backward; a cleaned-up expired bid is never resurrected by future calls.

4. **Investor active-bid cap.** An investor may not hold more than
   `MAX_ACTIVE_BIDS_PER_INVESTOR` concurrently `Placed` bids across all invoices
   (or unlimited when the limit is set to `0`).

5. **Capacity limit.** No more than `MAX_BIDS_PER_INVOICE` (50) active bids
   may exist on a single invoice at any time.

6. **Idempotent placement.** Calling `place_bid` with the same
   `(invoice_id, investor, salt)` triple is rejected with `DuplicateBid`.

7. **Terminal status atomicity.** `cancel_bid` and `withdraw_bid` both use a
   read-check-write pattern that validates the bid is still `Placed` before
   mutating. A concurrent race that has already moved the bid to a terminal
   status results in no state change (returns `false` or `OperationNotAllowed`).

## Query Entrypoints

| Entrypoint                     | Returns                      | Description                        |
|--------------------------------|------------------------------|------------------------------------|
| `get_bid`                      | `Option<Bid>`                | Single bid by ID.                  |
| `get_best_bid`                 | `Option<Bid>`                | Highest-ranked placed bid.         |
| `get_ranked_bids`              | `Vec<Bid>`                   | All placed bids sorted best-first. |
| `get_bids_by_status`           | `Vec<Bid>`                   | Filter by status.                  |
| `get_bids_by_investor`         | `Vec<Bid>`                   | Filter by investor for an invoice. |
| `get_bids_for_invoice`         | `Vec<Bid>`                   | All bids for an invoice.           |
| `get_all_bids_by_investor`     | `Vec<Bid>`                   | All bids across all invoices.      |
| `get_bid_ttl_config`           | `BidTtlConfig`               | Full TTL config snapshot.          |
| `get_bid_limit_config`         | `BidLimitConfig`             | Full bid limit policy snapshot.    |
| `get_bid_expiry_grace_config`  | `BidExpiryGraceConfig`       | Full cleanup grace-period snapshot.|

## Error Codes

| Error                             | Code | Raised when                                         |
|-----------------------------------|------|-----------------------------------------------------|
| `MaxBidsPerInvoiceExceeded`       | 1406 | 51st bid placed on a single invoice.                |
| `MaxActiveBidsPerInvestorExceeded`| 1407 | Investor exceeds their active-bid limit.            |
| `InvalidBidTtl`                   | 1409 | TTL out of range or zero.                           |
| `InvalidAmount`                   | 1400 | `bid_amount ≤ 0` or `bid_amount > investment_limit`.|
| `InvalidStatus`                   | 1401 | Bid or invoice in wrong status for the operation.   |
| `OperationNotAllowed`             | 1402 | Withdrawal attempted on a non-`Placed` bid.         |
| `DuplicateBid`                    | —    | Idempotency salt reused.                            |
| `InvestorNotVerified`             | 1605 | Investor KYC not completed.                         |
| `InvoiceNotFound`                 | 1000 | Invoice ID absent from storage.                     |

Full error reference: [`docs/ERROR_CODES.md`](ERROR_CODES.md).

## Related Documentation

- [`docs/BID_RANKING.md`](BID_RANKING.md) — deterministic 5-tier comparator spec.
- [`docs/INVOICE_LIFECYCLE.md`](INVOICE_LIFECYCLE.md) — invoice state machine.
- [`docs/contracts/lifecycle.md`](contracts/lifecycle.md) — cross-module invariants and sequence diagrams.
- [`docs/ERROR_CODES.md`](ERROR_CODES.md) — complete typed error reference.
- [`docs/QUERIES.md`](QUERIES.md) — read-only query entrypoints.
- [`quicklendx-contracts/src/bid.rs`](../quicklendx-contracts/src/bid.rs) — bid storage and logic.
