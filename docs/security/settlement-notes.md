# Settlement Security Notes

## Threat Cases Tested
- Overpay attack:
  - Attempt payment larger than remaining due.
  - Verified only remaining amount is applied and recorded.
- Zero/negative payment values:
  - Verified both reject with `InvalidAmount`.
- Double-pay / replay attempt:
  - Nonce uniqueness is enforced per `(invoice_id, payer, nonce)` in settlement storage.
- Paying a closed invoice:
  - Verified payment rejection for `Paid` invoices.
  - Verified payment rejection for `Cancelled` invoices.

## Core Invariants
- `total_paid <= total_due` (`invoice.total_paid <= invoice.amount`) always.
- `total_paid` is monotonic (never decreases).
- Applied payment amount is strictly positive.
- Settlement records are append-only by `(invoice_id, payment_index)`.
- Fully settled invoices transition to `Paid` and cannot accept further payments.

## Authorization Assumptions
- Only the invoice business address can be the payer for settlement recording.
- Payer authorization is required before payment state updates.

## Arithmetic Safety
- Payment accumulation uses checked arithmetic.
- Remaining due and progress calculations use checked operations and reject invalid states.
