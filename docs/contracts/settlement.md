# Settlement Contract Flow

## Overview
QuickLendX settlement now supports full and partial invoice payments with durable on-chain payment records.

- Partial payments accumulate per invoice.
- Payment progress is queryable at any time.
- Applied payment amount is capped so `total_paid` never exceeds invoice `amount` (total due).
- Every applied payment is persisted as a dedicated payment record with payer, amount, timestamp, and nonce/tx id.

## State Machine
QuickLendX uses existing invoice statuses. For settlement:

- `Funded`: open for repayment; may have zero or more partial payments.
- `Paid`: terminal settled state after full repayment and distribution.
- `Cancelled`: terminal non-payable state.

Partial repayment is represented by:

- `status == Funded`
- `total_paid > 0`
- `progress_percent < 100`

## Storage Layout
Settlement storage in `src/settlement.rs` uses keyed records (no large single-value payment vector as source of truth):

- `PaymentCount(invoice_id) -> u32`
- `Payment(invoice_id, idx) -> SettlementPaymentRecord`
- `PaymentNonce(invoice_id, payer, nonce) -> bool`

`SettlementPaymentRecord` fields:

- `payer: Address`
- `amount: i128` (applied amount)
- `timestamp: u64` (ledger timestamp)
- `nonce: String` (tx id / nonce)

Invoice fields used for progress:

- `amount` (total due)
- `total_paid`
- `status`

## Overpayment Behavior
Settlement and partial-payment paths intentionally behave differently:

- `process_partial_payment` safely bounds any excess request with `applied_amount = min(requested_amount, remaining_due)`.
- `settle_invoice` rejects explicit overpayment attempts with `InvalidAmount` unless the submitted amount exactly matches the remaining due.
- In both paths, `total_paid` can never exceed `amount`.

Accounting guarantees:

- Rejected settlement overpayments do not mutate invoice state, investment state, balances, or settlement events.
- Accepted final settlements emit `pay_rec` for the exact remaining due and `inv_stlf` for the final settled total.

## Events
Settlement emits:

- `pay_rec` (PaymentRecorded): `(invoice_id, payer, applied_amount, total_paid, status)`
- `inv_stlf` (InvoiceSettled): `(invoice_id, final_amount, paid_at)`

Backward-compatible events still emitted:

- `inv_pp` (partial payment event)
- `inv_set` (existing settlement event)

## Security Considerations
- Replay/idempotency:
  - Non-empty nonce is enforced unique per `(invoice, payer, nonce)`.
  - Duplicate nonce attempts are rejected.
- Overpayment integrity:
  - Final settlement requires an exact remaining-due payment to avoid ambiguous excess-value handling.
  - Partial-payment capping still protects incremental repayment flows without allowing accounting drift.
- Arithmetic safety:
  - Checked arithmetic is used for payment accumulation and progress calculations.
  - Invalid/overflowing states reject with contract errors.
- Authorization:
  - Payer must be the invoice business owner and must authorize payment.
- Closed invoice protection:
  - Payments are rejected for `Paid`, `Cancelled`, `Defaulted`, and `Refunded` states.
- Invariant:
  - `total_paid <= total_due` is enforced.

## Running Tests
From `quicklendx-contracts/`:

```bash
cargo test test_partial_payments -- --nocapture
cargo test test_settlement -- --nocapture
```
