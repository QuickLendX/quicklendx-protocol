# Invoice Lifecycle

This document describes the lifecycle of an invoice in the QuickLendX protocol,
from creation through settlement or default. Audience: **contributors and downstream
integrators** who need to understand how invoice state transitions work on-chain.

## State Diagram

```
              ┌──────────┐
              │  Pending │  ◄─── invoice created by a verified business
              └────┬─────┘
                   │  verify_invoice(admin, invoice_id)
                   ▼
              ┌──────────┐
              │ Verified │  ◄─── KYC-checked, open for investor bids
              └────┬─────┘
                   │  accept_bid(business, invoice_id, bid_id)
                   ▼
              ┌────────┐
              │ Funded │  ◄─── escrow locked; repayment window running
              └───┬────┘
       ┌──────────┴──────────┐
       ▼                     ▼
 ┌──────────┐          ┌────────────┐
 │   Paid   │          │  Defaulted │
 └──────────┘          └────────────┘

Before Funded, the invoice may also reach a terminal state via:
  cancel_invoice  → Cancelled
  resolve_dispute → Refunded  (Funded invoices only)
```

Terminal states (`Paid`, `Defaulted`, `Cancelled`, `Refunded`) are **irreversible**.

## Status Reference

| Status      | Terminal? | Description                                                      |
|-------------|-----------|------------------------------------------------------------------|
| `Pending`   | No        | Submitted; awaiting admin verification.                          |
| `Verified`  | No        | Verified by admin; investor bids may now be placed.              |
| `Funded`    | No        | Bid accepted; funds held in escrow.                              |
| `Paid`      | Yes       | Repaid by business; escrow released to investor less fees.       |
| `Defaulted` | Yes       | Grace period elapsed without repayment; escrow distributed.      |
| `Cancelled` | Yes       | Cancelled before funding by business or admin.                   |
| `Refunded`  | Yes       | Funded invoice refunded following a resolved dispute.            |

Source: `InvoiceStatus` in [`quicklendx-contracts/src/types.rs`](../quicklendx-contracts/src/types.rs).

## Entrypoints by Transition

### Pending → Verified

```rust
contract.verify_invoice(env, admin: Address, invoice_id: BytesN<32>)
  -> Result<(), QuickLendXError>
```

- **Caller**: admin (`AdminStorage::require_admin` enforced).
- **Precondition**: invoice exists and is `Pending`.
- **Effect**: `status = Verified`; emits `InvoiceVerified` event.

### Verified → Funded

```rust
contract.accept_bid(env, caller: Address, invoice_id: BytesN<32>, bid_id: BytesN<32>)
  -> Result<(), QuickLendXError>
```

- **Caller**: business owner of the invoice.
- **Precondition**: invoice is `Verified`; chosen bid is `Placed`; business is KYC-verified.
- **Effect**: bid amount transferred into escrow; `status = Funded`; bid marked `Accepted`;
  all other bids on this invoice marked `Cancelled`.

### Funded → Paid

```rust
contract.settle_invoice(
    env, caller: Address, invoice_id: BytesN<32>,
    payment_token: Address, payment_amount: i128,
) -> Result<(), QuickLendXError>
```

- **Caller**: business owner or admin.
- **Precondition**: invoice is `Funded`; `payment_amount >= invoice.amount`.
- **Effect**: escrow released to investor (minus platform fee); `status = Paid`; payment recorded.

### Funded → Defaulted

```rust
contract.trigger_default(env, caller: Address, invoice_id: BytesN<32>)
  -> Result<(), QuickLendXError>
```

- **Caller**: permissionless (anyone may trigger after the deadline).
- **Precondition**: invoice is `Funded`; `now > due_date + grace_period_seconds`.
- **Effect**: escrow distributed per default-finality policy; `status = Defaulted`.

### Any pre-Funded → Cancelled

```rust
contract.cancel_invoice(env, caller: Address, invoice_id: BytesN<32>)
  -> Result<(), QuickLendXError>
```

- **Caller**: business owner or admin.
- **Precondition**: invoice is `Pending` or `Verified` (not yet funded).
- **Effect**: `status = Cancelled`; outstanding bids marked `Withdrawn`.

### Funded → Refunded (dispute resolution)

```rust
contract.resolve_dispute(env, admin: Address, dispute_id: BytesN<32>, resolution: DisputeResolution)
  -> Result<(), QuickLendXError>
```

- **Caller**: admin.
- **Precondition**: dispute is `UnderReview`; underlying invoice is `Funded`.
- **Effect** (when `resolution == Refund`): escrow returned to investor; `status = Refunded`.

## Key Invariants

1. **Terminal states are final.** No entrypoint may change the status of an invoice
   that is `Paid`, `Defaulted`, `Cancelled`, or `Refunded`. Enforced in
   [`src/invariants.rs`](../quicklendx-contracts/src/invariants.rs).

2. **Escrow is atomic with funding.** An invoice is `Funded` iff a live escrow record
   exists. `settle_invoice` and `trigger_default` both drain the escrow in the same
   ledger transaction.

3. **Exactly one accepted bid per invoice.** `accept_bid` atomically accepts one bid
   and cancels the rest; a second call on a funded invoice is rejected with
   `InvoiceAlreadyFunded` (1002).

4. **KYC gating at submission.** `store_invoice` checks `BusinessVerificationStorage`
   and rejects unverified callers with `BusinessNotVerified` (1600).

## Secondary Indexes

`store_invoice` and `update_invoice_metadata` maintain four secondary indexes:

| Index storage key    | Indexed field     | Query entrypoint              |
|----------------------|-------------------|-------------------------------|
| `inv_bus:{address}`  | `business_owner`  | `get_invoices_by_customer`    |
| `inv_tax:{tax_id}`   | `tax_id`          | `get_invoices_by_tax_id`      |
| `inv_tag:{tag}`      | each tag in `tags`| `get_invoices_by_tag`         |
| `inv_cat:{category}` | `category`        | `get_invoices_by_category`    |

If indexes drift (e.g. after a backup restore), rebuild them with:

```rust
contract.rebuild_invoice_indexes(env, admin, offset: u32, limit: u32)
  -> Result<RebuildReport, QuickLendXError>
```

The call is **paginated**, **idempotent**, and **resumable** — run it in pages using
`report.next_offset` until `report.next_offset == total_invoices`.

## Error Codes

| Error                           | Code | Raised when                                              |
|---------------------------------|------|----------------------------------------------------------|
| `InvoiceNotFound`               | 1000 | Invoice ID absent from storage.                          |
| `InvoiceNotAvailableForFunding` | 1001 | Funding attempted on a non-`Verified` invoice.           |
| `InvoiceAlreadyFunded`          | 1002 | `accept_bid` called on an already-funded invoice.        |
| `InvoiceAmountInvalid`          | 1003 | Amount below `min_invoice_amount`.                       |
| `InvoiceDueDateInvalid`         | 1004 | Due date exceeds `max_due_date_days` from now.           |
| `InvoiceNotFunded`              | 1005 | Settlement attempted on non-`Funded` invoice.            |
| `InvoiceAlreadyDefaulted`       | 1006 | Default triggered on already-defaulted invoice.          |
| `InvoiceFrozen`                 | 1007 | Operation blocked on administratively frozen invoice.    |

Full error reference: [`docs/ERROR_CODES.md`](ERROR_CODES.md).

## Related Documentation

- [`docs/ESCROW.md`](ESCROW.md) — escrow lifecycle and release conditions.
- [`docs/DISPUTE.md`](DISPUTE.md) — dispute open / review / resolve flow.
- [`docs/STORAGE_LAYOUT.md`](STORAGE_LAYOUT.md) — on-chain storage key layout.
- [`docs/QUERIES.md`](QUERIES.md) — read-only query entrypoints.
- [`docs/ERROR_CODES.md`](ERROR_CODES.md) — complete typed error reference.
