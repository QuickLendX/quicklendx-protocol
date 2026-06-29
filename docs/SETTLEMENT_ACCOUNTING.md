# Settlement Partial-Payment Accounting Model and Auto-Release Trigger

## Overview

QuickLendX supports partial payments on funded invoices. Payments accumulate
into `total_paid` until either the full amount is reached (triggering
auto-release) or the invoice expires/defaults.

## Accounting Fields

| Field | Location | Description |
|---|---|---|
| `amount` | `Invoice` | Face value of the invoice |
| `funded_amount` | `Invoice` | Amount locked in escrow at funding time |
| `total_paid` | `Invoice` | Cumulative sum of all accepted payments |
| `payment_history` | `Invoice` | Ordered list of `(timestamp, amount, payer)` entries |
| `escrow.amount` | `Escrow` | Funds currently locked (decreases on release/refund) |

## Partial Payment Flow

```
investor → accept_bid_and_fund() → escrow.amount = bid.amount
                                 → Invoice.status = Funded

business → record_payment(amount_i) → Invoice.total_paid += amount_i
                                     → payment_history.push(entry)

When total_paid >= funded_amount → auto-release trigger fires
```

## Auto-Release Trigger

Auto-release is evaluated **after every `record_payment` call**:

```
if Invoice.total_paid >= Invoice.funded_amount:
    release_escrow_funds(invoice_id)  // escrow → business
    Invoice.status = Paid
    emit PaymentSettled event
```

The trigger is idempotent: once the escrow is in `Released` state, further
calls to `release_escrow_funds` return `InvalidStatus` and no double-spend
can occur.

## Partial Under-Payment (Invoice Expiry)

If `due_date` passes before `total_paid >= funded_amount`:

1. Admin or the protocol timer calls `trigger_default(invoice_id)`.
2. The escrow transitions to `Refunded` and funds return to the investor.
3. `Invoice.status = Defaulted`.
4. The investor receives `funded_amount - total_paid` (net of any partial
   payments already credited to the business).

## Invariants

- `total_paid` is monotonically non-decreasing.
- `total_paid` is capped at `funded_amount` by the contract (over-payments
  are rejected).
- `payment_history.len()` equals the number of distinct `record_payment`
  calls accepted.
- `payment_history` entries sum to `total_paid`.

## Dispute Interaction

If a dispute is opened (`open_dispute(invoice_id)`) while the invoice is
`Funded`, auto-release is blocked until the dispute is resolved:

- `DisputeResolution::InFavorOfBusiness` → release proceeds normally.
- `DisputeResolution::InFavorOfInvestor` → escrow is refunded.
- Partial payments already accepted before the dispute are not reversed.
