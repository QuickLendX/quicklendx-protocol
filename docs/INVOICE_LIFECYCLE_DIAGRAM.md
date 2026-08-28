# Invoice Lifecycle State-Machine Diagram

> **Audience:** Contributors who need to reason about invoice state transitions,
> verify implementation against design intent, or write tests that cover every
> valid path.  Operators and downstream integrators may also find the event
> and error tables useful.
>
> **Closes:** #1946

---

## Full State Machine

```
                           ┌──────────────────────────────────────────────┐
                           │  INVOICE CREATED                             │
                           │  store_invoice / upload_invoice              │
                           └────────────────────────┬─────────────────────┘
                                                    │
                                                    ▼
                        ┌───────────────────────────────────────────────────┐
            ┌──────────►│                    PENDING                        │
            │           │  Invoice submitted by a KYC-verified business.    │
            │           │  Metadata can be updated. No bids allowed yet.    │
            │           └───────────┬─────────────────────┬─────────────────┘
            │                       │                     │
            │     verify_invoice    │                     │  cancel_invoice
            │       (admin)         │                     │  (business / admin)
            │                       ▼                     ▼
            │    ┌──────────────────────────┐   ┌─────────────────────┐
            │    │         VERIFIED         │   │     CANCELLED  ✗    │
            │    │  Open for investor bids. │   │  (terminal)         │
            │    └──────────┬───────────────┘   └─────────────────────┘
            │               │                  │
            │               │ cancel_invoice   │
            │               │ (business/admin) │
            │               │                  ▼
            │               │        ┌─────────────────────┐
            │               │        │     CANCELLED  ✗    │
            │               │        │  (terminal)         │
            │               │        └─────────────────────┘
            │               │
            │   accept_bid  │
            │  (business)   │
            │               ▼
            │    ┌──────────────────────────────────────────────────────────┐
            │    │                       FUNDED                             │
            │    │  Bid accepted; investor funds held in escrow.            │
            │    │  Repayment window starts. Disputes may be opened.        │
            │    └──────────┬──────────────┬────────────────┬───────────────┘
            │               │              │                │
            │ settle_invoice│              │trigger_default │ resolve_dispute
            │  (business /  │              │  (anyone, after│ (admin, resolution
            │   admin)      │              │   due_date +   │  == Refund)
            │               ▼              │   grace_period)│
            │    ┌──────────────────┐      │                ▼
            │    │    PAID  ✗       │      ▼      ┌──────────────────────┐
            │    │  (terminal)      │  ┌───────────────────────────────────┐
            │    │  Full repayment  │  │         DEFAULTED  ✗             │
            │    │  received.       │  │  (terminal)                      │
            │    │  Investor paid   │  │  Grace period expired without     │
            │    │  principal+yield.│  │  repayment. Escrow distributed.  │
            │    └──────────────────┘  └───────────────────────────────────┘
            │                                       │
            │                                       │ resolve_dispute (admin)
            │                                       │ resolution == Refund
            │                                       ▼
            │                          ┌──────────────────────────────────┐
            │                          │          REFUNDED  ✗             │
            │                          │  (terminal)                      │
            │                          │  Funded invoice refunded after   │
            └──────────────────────────┘  dispute. Funds to investor.     │
                                          └──────────────────────────────────┘
```

> **Admin recovery path** (`update_invoice_status`): An admin may force
> `Pending→Verified`, `Verified→Funded`, `Funded→Paid`, or `Funded→Defaulted`
> without moving funds.  This is a bookkeeping-only recovery pathway — not a
> substitute for the normal operational flows above.

---

## Freeze Overlay

An invoice in any non-terminal state may be **frozen** by an admin.  The freeze
is a separate flag (`InvoiceLock::Frozen`) that overlays the status — it does
not change the status field.

```
  Any non-terminal state
          │
          │  freeze_invoice(admin, invoice_id, reason)
          ▼
    ┌─────────────┐
    │   FROZEN    │  ◄─── writes blocked (bid, settle, cancel, etc.)
    │  (overlay)  │       error: InvoiceFrozen (1007)
    └──────┬──────┘
           │  unfreeze_invoice(admin, invoice_id)
           ▼
  Resumes normal operation
```

When a freeze is applied, the contract emits an `InvoiceFrozen` event that
includes a `freeze_appeal_channel` pointer to the appeals process.  See the
[Appeals Process](APPEALS.md) for how to contest a freeze.

---

## Status Reference

| Status       | Terminal? | Description                                                         |
|--------------|-----------|---------------------------------------------------------------------|
| `Pending`    | No        | Submitted; awaiting admin verification. No bids allowed.            |
| `Verified`   | No        | Verified by admin; investors may place bids.                        |
| `Funded`     | No        | Bid accepted; funds in escrow. Repayment window running.            |
| `Paid`       | **Yes**   | Fully repaid. Escrow released to investor less fees.                |
| `Defaulted`  | **Yes**   | Grace period elapsed without repayment; escrow distributed.         |
| `Cancelled`  | **Yes**   | Cancelled before funding by business or admin.                      |
| `Refunded`   | **Yes**   | Funded invoice refunded after a resolved dispute.                   |

Source: `InvoiceStatus` in
[`quicklendx-contracts/src/types.rs`](../quicklendx-contracts/src/types.rs).

---

## Transition Table

| From       | To          | Entrypoint                | Caller              | Guard                                                    |
|------------|-------------|---------------------------|---------------------|----------------------------------------------------------|
| (new)      | `Pending`   | `store_invoice`           | Verified business   | `BusinessVerificationStorage` pass                       |
| `Pending`  | `Verified`  | `verify_invoice`          | Admin               | Invoice is `Pending`                                     |
| `Pending`  | `Cancelled` | `cancel_invoice`          | Business / admin    | Invoice is `Pending`                                     |
| `Verified` | `Funded`    | `accept_bid_and_fund`     | Business            | Invoice is `Verified`; chosen bid is `Placed`            |
| `Verified` | `Cancelled` | `cancel_invoice`          | Business / admin    | Invoice is `Verified`                                    |
| `Funded`   | `Paid`      | `settle_invoice`          | Business / admin    | Invoice is `Funded`; `payment_amount >= invoice.amount`  |
| `Funded`   | `Defaulted` | `mark_invoice_defaulted`  | Anyone              | `now > due_date + grace_period_seconds`                  |
| `Funded`   | `Refunded`  | `resolve_dispute`         | Admin               | Dispute is `UnderReview`; resolution is `Refund`         |

---

## Entrypoints: Concrete Rust Signatures

### `Pending` → `Verified`

```rust
// quicklendx-contracts/src/lib.rs (or contract.rs)
pub fn verify_invoice(
    env: Env,
    admin: Address,
    invoice_id: BytesN<32>,
) -> Result<(), QuickLendXError>
```

Requires `admin` to match the stored admin address.  Emits `InvoiceVerified`.

### `Verified` → `Funded`

```rust
pub fn accept_bid_and_fund(
    env: Env,
    caller: Address,        // must be invoice.business
    invoice_id: BytesN<32>,
    bid_id: BytesN<32>,
) -> Result<(), QuickLendXError>
```

- Transfers `bid.bid_amount` from investor into escrow atomically.
- Marks all other bids on this invoice `Cancelled`.
- Emits `EscrowCreated`, `BidAccepted`, `InvoiceFunded`.

### `Funded` → `Paid`

```rust
pub fn settle_invoice(
    env: Env,
    caller: Address,
    invoice_id: BytesN<32>,
    payment_token: Address,
    payment_amount: i128,
) -> Result<(), QuickLendXError>
```

- Requires `payment_amount >= invoice.amount`.
- Releases escrow to investor minus platform fee.
- Emits `InvoiceSettled` and `InvoiceSettledFinal`.

### `Funded` → `Defaulted`

```rust
pub fn mark_invoice_defaulted(
    env: Env,
    invoice_id: BytesN<32>,
) -> Result<(), QuickLendXError>
```

Permissionless — any caller may invoke after the deadline.  Emits
`InvoiceDefaulted`.

### `Pending` or `Verified` → `Cancelled`

```rust
pub fn cancel_invoice(
    env: Env,
    caller: Address,
    invoice_id: BytesN<32>,
) -> Result<(), QuickLendXError>
```

Emits `InvoiceCancelled`.

### `Funded` → `Refunded`

```rust
pub fn resolve_dispute(
    env: Env,
    admin: Address,
    dispute_id: BytesN<32>,
    resolution: DisputeResolution,
) -> Result<(), QuickLendXError>
```

When `resolution == DisputeResolution::FavorInvestor` with a refund flag, escrow
is returned to the investor and the invoice transitions to `Refunded`.  Emits
`DisputeResolved` and `EscrowRefunded`.

---

## Events Emitted per Transition

| Transition                | Events emitted                                      |
|---------------------------|-----------------------------------------------------|
| (new) → `Pending`         | `InvoiceUploaded`                                   |
| `Pending` → `Verified`    | `InvoiceVerified`                                   |
| `Pending` → `Cancelled`   | `InvoiceCancelled`                                  |
| `Verified` → `Funded`     | `EscrowCreated`, `BidAccepted`, `InvoiceFunded`     |
| `Verified` → `Cancelled`  | `InvoiceCancelled`                                  |
| `Funded` → `Paid`         | `InvoiceSettled`, `InvoiceSettledFinal`              |
| `Funded` → `Defaulted`    | `InvoiceDefaulted`                                  |
| `Funded` → `Refunded`     | `DisputeResolved`, `EscrowRefunded`                 |
| Freeze overlay applied    | `InvoiceFrozen` (includes `freeze_appeal_channel`)  |

Full event schema: [`docs/EVENTS_SCHEMA.md`](EVENTS_SCHEMA.md).

---

## Key Invariants

1. **Terminal states are irreversible.**  No entrypoint may change the status of
   a `Paid`, `Defaulted`, `Cancelled`, or `Refunded` invoice.  Enforced in
   `src/invariants.rs`.

2. **Escrow is atomic with funding.**  An invoice is `Funded` iff a live escrow
   record exists.  Both `settle_invoice` and `mark_invoice_defaulted` drain the
   escrow in the same ledger transaction.

3. **At most one accepted bid per invoice.**  `accept_bid_and_fund` atomically
   accepts one bid and cancels all others.  A second call on a funded invoice
   is rejected with `InvoiceAlreadyFunded` (1002).

4. **KYC gate at submission.**  `store_invoice` verifies `BusinessVerificationStorage`
   and rejects unverified callers with `BusinessNotVerified` (1600).

5. **Freeze is write-blocking, not status-changing.**  A frozen invoice retains
   its current status.  All write entrypoints return `InvoiceFrozen` (1007) while
   the freeze is active.

6. **Grace period is mandatory before default.**  `mark_invoice_defaulted` panics
   with `InvoiceNotDefaultable` if invoked before `due_date + grace_period_seconds`.

---

## Error Codes

| Error                           | Code | Raised when                                                  |
|---------------------------------|------|--------------------------------------------------------------|
| `InvoiceNotFound`               | 1000 | Invoice ID absent from storage.                              |
| `InvoiceNotAvailableForFunding` | 1001 | Funding attempted on a non-`Verified` invoice.               |
| `InvoiceAlreadyFunded`          | 1002 | `accept_bid_and_fund` called on an already-funded invoice.   |
| `InvoiceAmountInvalid`          | 1003 | Amount below `min_invoice_amount`.                           |
| `InvoiceDueDateInvalid`         | 1004 | Due date exceeds `max_due_date_days` from now.               |
| `InvoiceNotFunded`              | 1005 | Settlement attempted on a non-`Funded` invoice.              |
| `InvoiceAlreadyDefaulted`       | 1006 | Default triggered on an already-defaulted invoice.           |
| `InvoiceFrozen`                 | 1007 | Write operation blocked — invoice is administratively frozen.|
| `InvalidFreezeReason`           | 1008 | Unknown or disallowed `BusinessFreezeReason` variant.        |

Full catalog: [`docs/ERROR_CODES.md`](ERROR_CODES.md).

---

## Investment Status Integration

When an invoice transitions, its associated investment must transition atomically:

| Invoice transition   | Investment transition                |
|----------------------|--------------------------------------|
| `Funded` → `Paid`    | `Active` → `Completed`               |
| `Funded` → `Defaulted` | `Active` → `Defaulted`             |
| `Funded` → `Refunded`  | `Active` → `Refunded`              |
| `Verified` → `Cancelled` | (no investment yet)              |

See [`docs/contracts/invoice-lifecycle.md`](contracts/invoice-lifecycle.md#investment-status-lifecycle-issue-556)
for the full investment state machine and orphan-prevention guarantees.

---

## Secondary Indexes

`store_invoice` and `update_invoice_metadata` maintain four secondary indexes
kept consistent through all transitions:

| Storage key          | Indexed field     | Query entrypoint               |
|----------------------|-------------------|--------------------------------|
| `inv_bus:{address}`  | `business_owner`  | `get_invoices_by_customer`     |
| `inv_tax:{tax_id}`   | `tax_id`          | `get_invoices_by_tax_id`       |
| `inv_tag:{tag}`      | each tag in `tags`| `get_invoices_by_tag`          |
| `inv_cat:{category}` | `category`        | `get_invoices_by_category`     |
| `inv_sts:{status}`   | `status`          | `get_invoices_by_status`       |

If indexes drift after a backup restore, rebuild them with the paginated admin
helper:

```rust
// Pass next_offset = report.next_offset until next_offset == total_invoices
contract.rebuild_invoice_indexes(env, admin, offset: u32, limit: u32)
    -> Result<RebuildReport, QuickLendXError>
```

---

## Related Documentation

- [`docs/contracts/invoice-lifecycle.md`](contracts/invoice-lifecycle.md) — Detailed
  per-entrypoint reference including auth model, validations, and failure cases.
- [`docs/INVOICE_LIFECYCLE.md`](INVOICE_LIFECYCLE.md) — Prose description of the
  lifecycle stages.
- [`docs/BID_LIFECYCLE_DIAGRAM.md`](BID_LIFECYCLE_DIAGRAM.md) — Companion diagram
  for the bid state machine.
- [`docs/DEFAULT_FLOW_DIAGRAM.md`](DEFAULT_FLOW_DIAGRAM.md) — Detail on the default
  path: grace period, finality guards, dispute interception.
- [`docs/ESCROW.md`](ESCROW.md) — Escrow creation, release, and refund flows.
- [`docs/DISPUTE.md`](DISPUTE.md) — Dispute open / review / resolve lifecycle.
- [`docs/APPEALS.md`](APPEALS.md) — How to appeal a freeze or dispute resolution.
- [`docs/EVENTS_SCHEMA.md`](EVENTS_SCHEMA.md) — Full event schema and subscription guide.
- [`docs/ERROR_CODES.md`](ERROR_CODES.md) — Complete typed error code catalog.
- [`docs/STORAGE_LAYOUT.md`](STORAGE_LAYOUT.md) — On-chain storage key layout.
