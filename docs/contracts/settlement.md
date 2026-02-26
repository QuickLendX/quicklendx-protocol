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
Overpayment is capped at the remaining due amount:

- `applied_amount = min(requested_amount, remaining_due)`
- `total_paid` is updated only by `applied_amount`
- `total_paid` can never exceed `amount`

Remainder handling:

- Remainder is not applied to invoice state.
- No refund transfer is needed because only applied amount is used for settlement accounting and payout.

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
```
