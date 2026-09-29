# Audit Trail

## Overview

Every state-changing operation in QuickLendX appends an immutable `AuditLogEntry`
to the on-chain audit trail. Entries are **append-only** — they can never be
overwritten or deleted — providing a tamper-evident history for compliance and
dispute resolution.

## Security Model

| Property | Guarantee |
|----------|-----------|
| Append-only | Entries are pushed to a growing list; no removal API exists |
| Immutable after write | The entry stored under its `audit_id` key is never overwritten |
| Actor attribution | Every entry records the `Address` that triggered the operation |
| Timestamp binding | `timestamp` is taken from `env.ledger().timestamp()` at write time |
| No secrets logged | Only operation type, actor, status strings, and amounts are stored |
| Deterministic ordering | Entries within an invoice trail are ordered by insertion (append) |

## Data Structures

### `AuditLogEntry`

```rust
pub struct AuditLogEntry {
    pub audit_id: BytesN<32>,          // Unique entry ID
    pub invoice_id: BytesN<32>,        // Invoice this entry belongs to
    pub operation: AuditOperation,     // What happened
    pub actor: Address,                // Who triggered it
    pub timestamp: u64,                // Ledger timestamp at write time
    pub old_value: Option<String>,     // Previous value (status changes)
    pub new_value: Option<String>,     // New value
    pub amount: Option<i128>,          // Monetary amount if applicable
    pub additional_data: Option<String>,
    pub block_height: u32,             // Ledger sequence number
    pub transaction_hash: Option<BytesN<32>>,
}
```

### `AuditOperation` variants

`InvoiceCreated`, `InvoiceUploaded`, `InvoiceVerified`, `InvoiceFunded`,
`InvoicePaid`, `InvoiceDefaulted`, `InvoiceStatusChanged`, `InvoiceRated`,
`BidPlaced`, `BidAccepted`, `BidWithdrawn`,
`EscrowCreated`, `EscrowReleased`, `EscrowRefunded`,
`PaymentProcessed`, `SettlementCompleted`

## Storage Layout

| Index key | Content |
|-----------|---------|
| `audit_id` | Full `AuditLogEntry` |
| `(inv_aud, invoice_id)` | Ordered list of `audit_id`s for that invoice |
| `(op_aud, operation)` | List of `audit_id`s for that operation type |
| `(act_aud, actor)` | List of `audit_id`s for that actor |
| `(ts_aud, day_bucket)` | List of `audit_id`s grouped by day |
| `all_aud` | Global list of all `audit_id`s |

## API Reference

### Query functions

| Function | Description |
|----------|-------------|
| `get_invoice_audit_trail(invoice_id)` | Ordered list of audit IDs for an invoice |
| `get_audit_entry(audit_id)` | Full entry by ID |
| `get_audit_entries_by_operation(op)` | All IDs for a given operation type |
| `get_audit_entries_by_actor(actor)` | All IDs attributed to an actor |
| `query_audit_logs(filter, limit)` | Filtered query (capped at `MAX_QUERY_LIMIT = 100`) |
| `get_audit_stats()` | Aggregate stats: total entries, unique actors, date range |
| `validate_invoice_audit_integrity(invoice_id)` | Integrity check for an invoice trail |

### `AuditQueryFilter`

```rust
pub struct AuditQueryFilter {
    pub invoice_id: Option<BytesN<32>>,
    pub operation: AuditOperationFilter,   // Any | Specific(op)
    pub actor: Option<Address>,
    pub start_timestamp: Option<u64>,
    pub end_timestamp: Option<u64>,
}
```

Filters are ANDed. The query engine selects the most selective index first
(invoice_id > operation > actor > global scan) then applies remaining filters.

## Test Coverage (Issue #823)

`src/test_audit.rs` — comprehensive suite covering:

| Group | Tests | What is verified |
|-------|-------|-----------------|
| Append-only behavior | 2 | Trail only grows; earlier entries survive later writes |
| Entry immutability | 1 | Stored entry fields unchanged after subsequent operations |
| Filter by invoice | 1 | Query returns only entries for the requested invoice |
| Filter by operation | 1 | Operation index returns correct entries |
| Filter by actor | 1 | Actor index returns correct entries |
| Time-range filter | 2 | Matching range returns results; future range returns empty |
| Combined filters | 1 | actor + operation filter returns only matching entries |
| Query limit cap | 1 | Result count never exceeds `MAX_QUERY_LIMIT` |
| Integrity check | 3 | Valid trail passes; empty invoice passes; full lifecycle passes |
| Stats — total entries | 6 | Incremental counts after create/verify/bid/escrow/withdraw |
| Stats — unique actors | 3 | Single, multiple, and duplicate-actor scenarios |
| Stats — date range | 2 | Single entry; time-progressed entries |
| Stats — empty state | 1 | Zero entries, zero actors, sentinel timestamps |
| Stats — comprehensive | 1 | Full workflow produces correct aggregate |

Run with:

```bash
cd quicklendx-contracts
cargo test test_audit
```
