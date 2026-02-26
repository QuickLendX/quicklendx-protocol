# Audit Trail and Integrity

On-chain audit trail for critical operations: every important state change is logged with actor, timestamp, and payload. Entries are keyed by invoice, global sequence, and indexes for efficient querying. All entries may be validated for integrity to ensure completeness and authenticity.

## Overview

The audit trail system provides:
- **Append-only audit logs** for all critical operations (invoice, bid, escrow, settlement, payment)
- **Efficient querying** by invoice, actor, operation type, or time range with bounded result sets
- **Integrity validation** to verify audit log completeness and detect missing or corrupted entries
- **Audit statistics** for comprehensive analysis of contract activities

## Entrypoints

| Entrypoint | Visibility | Description |
|------------|------------|--------------|
| `log_operation` | Internal | Append a single audit entry (actor, timestamp, operation, payload). Used by invoice, bid, escrow, and settlement flows. |
| `get_invoice_audit_trail` | Public | Return audit entry IDs for an invoice (chronological by append order). |
| `query_audit_logs` | Public | Query entries with filters (invoice, actor, operation type, time range) and a bounded limit (max 100). |
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

Appends are gas-efficient (one entry + index updates). Query results are bounded by the `limit` parameter and hard-capped to `100` entries (`min(limit, 100)`) to avoid unbounded reads.

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
## Query Filters Usage

Query audit logs using `AuditQueryFilter` with any combination of:

### By Invoice
Filter entries for a specific invoice:
```rust
AuditQueryFilter {
    invoice_id: Some(invoice_id),
    operation: AuditOperationFilter::Any,
    actor: None,
    start_timestamp: None,
    end_timestamp: None,
}
```

### By Operation Type
Filter entries for a specific operation:
```rust
AuditQueryFilter {
    invoice_id: None,
    operation: AuditOperationFilter::Specific(AuditOperation::BidPlaced),
    actor: None,
    start_timestamp: None,
    end_timestamp: None,
}
```

### By Actor
Filter entries by who performed the action:
```rust
AuditQueryFilter {
    invoice_id: None,
    operation: AuditOperationFilter::Any,
    actor: Some(investor_address),
    start_timestamp: None,
    end_timestamp: None,
}
```

### By Time Range
Filter entries within a time window (timestamps in seconds):
```rust
AuditQueryFilter {
    invoice_id: None,
    operation: AuditOperationFilter::Any,
    actor: None,
    start_timestamp: Some(start_ts),
    end_timestamp: Some(end_ts),
}
```

### Combined Filters
Combine multiple filters for precise queries:
```rust
AuditQueryFilter {
    invoice_id: Some(invoice_id),
    operation: AuditOperationFilter::Specific(AuditOperation::PaymentProcessed),
    actor: Some(admin),
    start_timestamp: Some(start_ts),
    end_timestamp: Some(end_ts),
}
```

**Note**: Query results are capped at 100 entries maximum to prevent unbounded reads and gas exhaustion.

## Integrity Validation Details

`validate_invoice_audit_integrity(env, invoice_id)` performs comprehensive validation:

**Per-Entry Checks:**
- **Timestamp Validity**: Ensures timestamp is not in the future compared to current ledger timestamp
- **Block Height Validity**: Ensures block height does not exceed current ledger sequence
- **Operation-Specific Data**:
  - For `InvoiceFunded` and `PaymentProcessed`: Amount must be present and > 0
  - For `InvoiceStatusChanged`: Old and new values must both be present

**Trail Completeness:**
- Verifies all audit IDs in the invoice trail can be retrieved
- Returns `false` if any audit entry is missing from storage
- Returns `false` if any validation check fails
- Returns `true` only if all entries are present and pass all checks

**Use Case**: Verify audit completeness before settlement or dispute resolution to ensure no operations were lost or corrupted.

## Audit Statistics

`get_audit_stats()` provides aggregate information:

- **total_entries**: Total number of audit log entries in the contract
- **unique_actors**: Count of distinct addresses that performed operations
- **date_range**: Tuple of (min_timestamp, max_timestamp) for all entries
  - min_timestamp = u64::MAX if no entries exist
  - max_timestamp = 0 if no entries exist

**Use Cases**:
- Auditing contract activity levels
- Understanding participation scope
- Determining audit log time windows

## Implementation Notes

### Audit ID Generation
Audit IDs are deterministically generated using:
- Audit prefix bytes (0xAD, 0x1F)
- Current ledger timestamp (8 bytes)
- Current ledger sequence (4 bytes)
- Global counter (8 bytes)
- Hash-based pattern fill (8 bytes)

This ensures unique, non-colliding IDs while preserving chronological information.

### Storage Optimization
- **Index-based filtering**: Per-invoice, per-operation, and per-actor indexes enable efficient filtered queries
- **Timestamp grouping**: Daily grouping reduces index size for time-range queries
- **Gas efficiency**: Appending a single entry requires minimal storage operations
- **Query limit**: Hard-capped at 100 results to prevent gas exhaustion on large result sets

### Missing Invoice Behavior
If no audit entries exist for an invoice ID:
- `get_invoice_audit_trail()` returns empty vector
- `validate_invoice_audit_integrity()` returns `true` (empty trail is valid)
- This allows querying non-existent invoices safely without errors

## Testing Coverage

The audit trail implementation includes 30+ comprehensive tests covering:
- **Basic operations**: Creating, storing, and retrieving audit entries
- **Query filters**: Single and combined filter scenarios
- **Time-range queries**: Past, present, and future timestamp ranges
- **Integrity validation**: Valid and invalid entry detection
- **Edge cases**: Empty trails, missing entries, future timestamps, invalid amounts
- **Batch operations**: Multiple invoices, actors, and operations
- **Statistics**: Entry counts, actor uniqueness, date ranges
- **Query limits**: Enforcement of 100-entry maximum

Target coverage: ≥95% of audit module code paths

## Security Best Practices

1. **Always validate integrity** before using audit data for critical decisions
2. **Use combined filters** when possible to reduce query result sizes
3. **Check audit stats** periodically to detect anomalies
4. **Monitor unique actors** to identify unexpected participants
5. **Archive old entries** in external systems if on-chain storage becomes a concern
6. **Verify timestamps** when audit logs span multiple blocks or transactions
