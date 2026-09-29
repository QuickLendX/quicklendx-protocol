# QLX Invoice Taxonomy

> Audience: contributors who need the canonical invoice categories and statuses as they appear on-chain. This document covers the `InvoiceStatus` and `InvoiceCategory` enums defined in the Soroban contract crate, their meanings, and the transitions between them.

## Statuses

The contract defines seven invoice statuses in `InvoiceStatus` ([`quicklendx-contracts/src/types.rs`](../quicklendx-contracts/src/types.rs)):

| Status      | Terminal? | Description |
|-------------|-----------|-------------|
| `Pending`   | No        | Invoice submitted by a verified business; awaiting admin verification. |
| `Verified`  | No        | Admin has verified the invoice; investors may now place bids. |
| `Funded`    | No        | A bid has been accepted; escrow is locked and the repayment window is active. |
| `Paid`      | Yes       | Business repaid the full amount; escrow released to investor less fees. |
| `Defaulted` | Yes       | Grace period elapsed without repayment; escrow distributed per default-finality policy. |
| `Cancelled` | Yes       | Invoice cancelled before funding by business or admin. |
| `Refunded`  | Yes       | Funded invoice refunded following a resolved dispute in favour of the investor. |

Terminal statuses (`Paid`, `Defaulted`, `Cancelled`, `Refunded`) are **irreversible**. No entrypoint may transition an invoice out of a terminal state.

### Concrete status-transition example

```rust
// InvoiceStatus is defined in types.rs as:
pub enum InvoiceStatus {
    Pending,
    Verified,
    Funded,
    Paid,
    Defaulted,
    Cancelled,
    Refunded,
}
```

A typical lifecycle for a single invoice:

```
Pending → Verified → Funded → Paid
                  ↘ Defaulted
```

Or, if cancelled before funding:

```
Pending → Cancelled
```

Or, if a dispute results in a refund:

```
Pending → Verified → Funded → Refunded
```

### Status transition entrypoints

| Transition | Entrypoint | Caller |
|---|---|---|
| Pending → Verified | `verify_invoice(env, admin, invoice_id)` | admin |
| Verified → Funded | `accept_bid(env, caller, invoice_id, bid_id)` | business owner |
| Funded → Paid | `settle_invoice(env, caller, invoice_id, payment_token, payment_amount)` | business owner or admin |
| Funded → Defaulted | `trigger_default(env, caller, invoice_id)` | anyone (after grace deadline) |
| Any pre-Funded → Cancelled | `cancel_invoice(env, caller, invoice_id)` | business owner or admin |
| Funded → Refunded | `resolve_dispute(env, admin, dispute_id, Refund)` | admin |

See [`docs/INVOICE_LIFECYCLE.md`](INVOICE_LIFECYCLE.md) for the full state diagram and invariants.

## Categories

The contract defines nine invoice categories in `InvoiceCategory` ([`quicklendx-contracts/src/types.rs`](../quicklendx-contracts/src/types.rs)):

| Category       | Description |
|----------------|-------------|
| `Services`     | Service-based invoices (e.g. consulting, professional services). |
| `Goods`        | Physical goods invoices. |
| `Consulting`   | Consulting engagements. |
| `Logistics`    | Supply-chain and logistics invoices. |
| `Products`     | Product-sale invoices. |
| `Manufacturing` | Manufacturing-related invoices. |
| `Technology`   | Technology and software invoices. |
| `Healthcare`   | Healthcare sector invoices. |
| `Other`        | Catch-all for categories not covered above. |

### Concrete example: creating an invoice with a category

```rust
use quicklendx_sdk::types::InvoiceCategory;

// When storing a new invoice, the business supplies the category:
contract.store_invoice(
    env,
    invoice_id,
    amount,
    currency,
    due_date,
    description,
    InvoiceCategory::Technology,  // <-- category choice
    tags,
    late_payment_penalty_bps,
    early_payment_discount_bps,
) -> Result<(), QuickLendXError>
```

### Category index

Categories are indexed on-chain for querying. The secondary index key is `inv_cat:{category}`, and the query entrypoint is `get_invoices_by_category`. See [`docs/INVOICE_LIFECYCLE.md`](INVOICE_LIFECYCLE.md) for the full index layout.

## Error codes related to invoice status

| Error | Code | When raised |
|---|---|---|
| `InvoiceNotFound` | 1000 | Invoice ID absent from storage. |
| `InvoiceNotAvailableForFunding` | 1001 | Funding attempted on a non-`Verified` invoice. |
| `InvoiceAlreadyFunded` | 1002 | `accept_bid` called on an already-funded invoice. |
| `InvoiceNotFunded` | 1005 | Settlement attempted on non-`Funded` invoice. |
| `InvoiceAlreadyDefaulted` | 1006 | Default triggered on already-defaulted invoice. |
| `InvoiceFrozen` | 1007 | Operation blocked on administratively frozen invoice. |

Full error reference: [`docs/ERROR_CODES.md`](ERROR_CODES.md).

## Related documentation

- [`docs/INVOICE_LIFECYCLE.md`](INVOICE_LIFECYCLE.md) — full invoice state machine, transitions, and invariants.
- [`docs/INVOICE_LIFECYCLE_DIAGRAM.md`](INVOICE_LIFECYCLE_DIAGRAM.md) — visual state diagram.
- [`docs/QLX_INVOICE_LOCK_TIME_LIMITS.md`](QLX_INVOICE_LOCK_TIME_LIMITS.md) — contributor-facing summary of invoice lock duration and auto-release behaviour.
- [`docs/INVOICE_LOCK.md`](INVOICE_LOCK.md) — lock duration, admin freeze auto-release, and escrow hold timing.
- [`docs/ERROR_CODES.md`](ERROR_CODES.md) — complete typed error reference.
- [`quicklendx-contracts/src/types.rs`](../quicklendx-contracts/src/types.rs) — source of truth for `InvoiceStatus` and `InvoiceCategory` enums.