# Repayment events and audit parity

Date: 2026-08-30  
Area: financial correctness / accounting  
Public boundary: `process_partial_payment` / `make_payment`

## Invariants

- `principal + investor_profit + platform_fee + late_penalty == total_paid` after every committed payment.
- Waterfall: principal → contractual profit (investor-favored floor fee) → investor late penalty.
- Late penalty is assessed once from remaining contractual due (`invoice.amount - total_paid`) at the first payment with `timestamp > due_date` and `late_payment_penalty_bps > 0`. It increases `total_due` and is paid to the investor, not the treasury.
- Versioned `repayment_allocated` events and `PaymentProcessed` audit rows are emitted only after storage commit and share `operation_id`. Schema version is 1; phase is `Committed`.
- Duplicate, stale, unauthorized, paused, frozen, cap-exceeded, and failed operations leave no ledger, event, or audit. Completing a payment while a dispute is active returns `DisputeActive` and rolls back.

## Compatibility

- Legacy `pay_rec` / `partial_payment` / `InvoiceSettled` payloads are unchanged.
- `Progress.total_due` grows by assessed late only when a non-zero penalty is configured and due. Callers that assumed `total_due == invoice.amount` must read progress for those invoices.
- `payments::repay_escrow` remains unused and is not exported.
- Duplicate nonces return `DuplicateNonce` (existing contract behavior).

## Migration and rollback

- Additive `SettlementDataKey::Allocation` key. Missing ledgers reconstruct from `invoice.total_paid` with `assessed_late = 0` (no retroactive late fee).
- Rollback is a WASM rollback. Mixed-version indexers should ignore unknown `repayment_allocated` until they support schema 1.

## Operational limitations

- Events are not a substitute for contract storage. Correlation ids are not transaction hashes.
- Late preview on `get_invoice_progress` after `due_date` includes the pending penalty before the first post-due payment is committed.

## Security assumptions

- Authorization, freeze, pause, and the payment reentrancy guard remain the source of truth.
- Audit `additional_data` is compact ASCII allocation fields only; no PII.
