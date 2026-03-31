# QuickLendX Invoice Lifecycle

This note documents the invoice cancellation rule that protects lifecycle
queries and analytics from invalid state transitions.

## Cancellation Constraints

- `cancel_invoice(invoice_id)` is only valid while an invoice is in
  `InvoiceStatus::Pending` or `InvoiceStatus::Verified`.
- Once an invoice reaches `InvoiceStatus::Funded`, cancellation is rejected with
  `QuickLendXError::InvalidStatus`.
- The same rejection applies to any other terminal or post-funding state such as
  `Paid`, `Defaulted`, `Cancelled`, or `Refunded`.

## Storage Safety

Cancellation validation happens before the contract mutates any status index.
If a cancellation attempt is rejected, the invoice:

- keeps its original status,
- remains in its original status query bucket,
- is not inserted into the cancelled bucket, and
- does not change aggregate counts derived from status indexes.

This preserves consistency for `get_invoices_by_status`,
`get_invoice_count_by_status`, and lifecycle analytics after failed
cancellation attempts.
