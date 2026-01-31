# Audit Trail

On-chain audit trail for critical operations: every important state change is logged with actor, timestamp, and payload. Entries are keyed by invoice, global sequence, and indexes for efficient querying.

## Entrypoints

| Entrypoint | Visibility | Description |
|------------|------------|--------------|
| `log_operation` | Internal | Append a single audit entry (actor, timestamp, operation, payload). Used by invoice, bid, escrow, and settlement flows. |
| `get_invoice_audit_trail` | Public | Return audit entry IDs for an invoice (chronological by append order). |
| `query_audit_logs` | Public | Query entries with filters (invoice, actor, operation type, time range) and a bounded limit. |
| `validate_invoice_audit_integrity` | Public | Verify that all entries for an invoice are present and pass integrity checks (timestamp, block height, operation-specific data). |
| `get_audit_entry` | Public | Fetch a single entry by ID. |
| `get_audit_stats` | Public | Return aggregate stats (total entries, unique actors, date range). |
| `get_audit_entries_by_operation` | Public | Return entry IDs for a given operation type. |
| `get_audit_entries_by_actor` | Public | Return entry IDs for a given actor. |

## Operation Types

- Invoice: `InvoiceCreated`, `InvoiceUploaded`, `InvoiceVerified`, `InvoiceFunded`, `InvoicePaid`, `InvoiceDefaulted`, `InvoiceStatusChanged`, `InvoiceRated`
- Bid: `BidPlaced`, `BidAccepted`, `BidWithdrawn`
- Escrow: `EscrowCreated`, `EscrowReleased`, `EscrowRefunded`
- Payment: `PaymentProcessed`, `SettlementCompleted`

## Storage and Indexes

- **Per-entry**: Stored by `audit_id` (unique per append).
- **Per-invoice**: List of `audit_id`s keyed by `(inv_aud, invoice_id)`.
- **Per-operation**: List of `audit_id`s keyed by `(op_aud, operation)`.
- **Per-actor**: List of `audit_id`s keyed by `(act_aud, actor)`.
- **Time**: Entries grouped by day for time-range queries.
- **Global**: Single list of all `audit_id`s for full scan when no filter narrows the set.

Appends are gas-efficient (one entry + index updates). Query results are bounded by the `limit` parameter to avoid unbounded reads.

## Integrity Validation

`validate_invoice_audit_integrity` checks for each entry on the invoice trail:

- Timestamp not in the future.
- Block height not beyond current ledger sequence.
- For amount-bearing operations, amount present and positive.
- For status-change operations, old/new value present.

If any check fails or an entry is missing, the function returns `false`.

## Security Notes

- Audit log is append-only; no deletion or modification of entries.
- Only the contract itself calls `log_operation`; no public write endpoint for arbitrary audit data.
- Query and integrity functions are read-only and do not alter state.
