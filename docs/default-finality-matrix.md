# Default Finality Matrix

This matrix is the canonical decision table for whether `mark_invoice_defaulted`
may transition an invoice into `Defaulted`.

It cross-checks three modules:

- `quicklendx-contracts/src/defaults.rs`
- `quicklendx-contracts/src/settlement.rs`
- `quicklendx-contracts/src/payments.rs`

The matrix is intentionally exhaustive across:

- every `InvoiceStatus`
- both settlement finality states
- every escrow terminal/active state

Security note: the matrix covers status/finality/escrow interactions. Duplicate
default prevention is stricter and is enforced separately by the transition
guard, which must reject a second default attempt with
`DuplicateDefaultTransition` to avoid double-default side effects.

<!-- DEFAULT_FINALITY_MATRIX:START -->
| Invoice status | Settlement finalized | Escrow status | Expected default outcome | Reason |
| --- | --- | --- | --- | --- |
| Pending | false | Held | Deny: InvoiceNotAvailableForFunding | Only funded invoices are eligible for default review. |
| Pending | false | Released | Deny: InvoiceNotAvailableForFunding | Status gate blocks default before escrow finality is considered. |
| Pending | false | Refunded | Deny: InvoiceNotAvailableForFunding | Status gate blocks default before escrow finality is considered. |
| Pending | true | Held | Deny: InvoiceNotAvailableForFunding | Status gate blocks default before settlement finality is considered. |
| Pending | true | Released | Deny: InvoiceNotAvailableForFunding | Status gate blocks default before other finality checks. |
| Pending | true | Refunded | Deny: InvoiceNotAvailableForFunding | Status gate blocks default before other finality checks. |
| Verified | false | Held | Deny: InvoiceNotAvailableForFunding | Verified invoices are not yet funded, so they cannot default. |
| Verified | false | Released | Deny: InvoiceNotAvailableForFunding | Status gate blocks default before escrow finality is considered. |
| Verified | false | Refunded | Deny: InvoiceNotAvailableForFunding | Status gate blocks default before escrow finality is considered. |
| Verified | true | Held | Deny: InvoiceNotAvailableForFunding | Status gate blocks default before settlement finality is considered. |
| Verified | true | Released | Deny: InvoiceNotAvailableForFunding | Status gate blocks default before other finality checks. |
| Verified | true | Refunded | Deny: InvoiceNotAvailableForFunding | Status gate blocks default before other finality checks. |
| Funded | false | Held | Allow | This is the only open finality combination for defaulting after grace expiry. |
| Funded | false | Released | Deny: InvalidStatus | Released escrow means funds already left escrow, so default must be refused. |
| Funded | false | Refunded | Deny: InvalidStatus | Refunded escrow means capital already returned, so default must be refused. |
| Funded | true | Held | Deny: InvalidStatus | Settlement finality must block default even if the invoice status still says Funded. |
| Funded | true | Released | Deny: InvalidStatus | Both settlement and escrow indicate a terminal path, so default must be refused. |
| Funded | true | Refunded | Deny: InvalidStatus | Both settlement and escrow indicate a terminal path, so default must be refused. |
| Paid | false | Held | Deny: InvoiceNotAvailableForFunding | Paid invoices are terminal and cannot default. |
| Paid | false | Released | Deny: InvoiceNotAvailableForFunding | Status gate blocks default before escrow finality is considered. |
| Paid | false | Refunded | Deny: InvoiceNotAvailableForFunding | Status gate blocks default before escrow finality is considered. |
| Paid | true | Held | Deny: InvoiceNotAvailableForFunding | Status gate blocks default before settlement finality is considered. |
| Paid | true | Released | Deny: InvoiceNotAvailableForFunding | Status gate blocks default before other finality checks. |
| Paid | true | Refunded | Deny: InvoiceNotAvailableForFunding | Status gate blocks default before other finality checks. |
| Defaulted | false | Held | Deny: InvoiceAlreadyDefaulted | A pre-existing defaulted status must be rejected immediately. |
| Defaulted | false | Released | Deny: InvoiceAlreadyDefaulted | Status gate wins before escrow finality on synthetic pre-defaulted rows. |
| Defaulted | false | Refunded | Deny: InvoiceAlreadyDefaulted | Status gate wins before escrow finality on synthetic pre-defaulted rows. |
| Defaulted | true | Held | Deny: InvoiceAlreadyDefaulted | Status gate wins before settlement finality on synthetic pre-defaulted rows. |
| Defaulted | true | Released | Deny: InvoiceAlreadyDefaulted | Status gate wins before other finality checks on synthetic pre-defaulted rows. |
| Defaulted | true | Refunded | Deny: InvoiceAlreadyDefaulted | Status gate wins before other finality checks on synthetic pre-defaulted rows. |
| Cancelled | false | Held | Deny: InvoiceNotAvailableForFunding | Cancelled invoices are terminal and cannot default. |
| Cancelled | false | Released | Deny: InvoiceNotAvailableForFunding | Status gate blocks default before escrow finality is considered. |
| Cancelled | false | Refunded | Deny: InvoiceNotAvailableForFunding | Status gate blocks default before escrow finality is considered. |
| Cancelled | true | Held | Deny: InvoiceNotAvailableForFunding | Status gate blocks default before settlement finality is considered. |
| Cancelled | true | Released | Deny: InvoiceNotAvailableForFunding | Status gate blocks default before other finality checks. |
| Cancelled | true | Refunded | Deny: InvoiceNotAvailableForFunding | Status gate blocks default before other finality checks. |
| Refunded | false | Held | Deny: InvoiceNotAvailableForFunding | Refunded invoices are terminal and cannot default. |
| Refunded | false | Released | Deny: InvoiceNotAvailableForFunding | Status gate blocks default before escrow finality is considered. |
| Refunded | false | Refunded | Deny: InvoiceNotAvailableForFunding | Status gate blocks default before escrow finality is considered. |
| Refunded | true | Held | Deny: InvoiceNotAvailableForFunding | Status gate blocks default before settlement finality is considered. |
| Refunded | true | Released | Deny: InvoiceNotAvailableForFunding | Status gate blocks default before other finality checks. |
| Refunded | true | Refunded | Deny: InvoiceNotAvailableForFunding | Status gate blocks default before other finality checks. |
<!-- DEFAULT_FINALITY_MATRIX:END -->

Implementation notes:

- `defaults::mark_invoice_defaulted` remains the canonical default path.
- `defaults::handle_default` must refuse defaulting once settlement has finalized
  or the escrow status is no longer `Held`.
- Any admin/testing shortcut that wants to produce `Defaulted` must route through
  the defaults module rather than mutating `InvoiceStatus::Defaulted` directly.
