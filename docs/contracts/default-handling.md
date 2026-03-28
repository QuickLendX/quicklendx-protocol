# Invoice Default Handling Documentation

## Overview

The QuickLendX contract handles overdue funded invoices in two phases:

1. The invoice becomes **overdue** once `now > due_date`.
2. The invoice becomes **defaultable** once `now > due_date + grace_period`.

Default handling keeps investor and business state aligned by updating the invoice, the investment, and the funded/defaulted indexes in one deterministic flow.

## Lifecycle

```text
Pending -> Verified -> Funded -> [Due Date] -> [Grace Period] -> Defaulted
```

## Grace Period

- Default grace period: `604800` seconds (7 days)
- Source of truth: protocol config when set, otherwise `DEFAULT_GRACE_PERIOD`
- Per-call override: supported by `mark_invoice_defaulted`, `check_invoice_expiration`, and `scan_overdue_invoices`

## Entry Points

### `mark_invoice_defaulted`

Marks one funded invoice as defaulted after validating that the grace deadline has passed.

**Parameters**
- `invoice_id: BytesN<32>` - Invoice to default
- `grace_period: Option<u64>` - Optional grace-period override

**Returns**
- `Ok(())` on success
- `Err(QuickLendXError)` on validation failure

### `check_invoice_expiration`

Checks one invoice and defaults it when it is funded and past its grace deadline.

**Parameters**
- `invoice_id: BytesN<32>` - Invoice to inspect
- `grace_period: Option<u64>` - Optional grace-period override

**Returns**
- `true` when the invoice was defaulted by this call
- `false` when the invoice is not defaultable

### `check_overdue_invoices`

Runs the funded-invoice overdue scan with the protocol grace period and the default scan batch size.

**Returns**
- `u32` overdue invoices found in the scanned batch

### `check_overdue_invoices_grace`

Runs the same bounded funded-invoice scan with an explicit grace period.

**Parameters**
- `grace_period: u64` - Grace-period override in seconds

**Returns**
- `u32` overdue invoices found in the scanned batch

### `scan_overdue_invoices`

Detailed bounded scan entry point for operators, automation, and tests.

**Parameters**
- `grace_period: Option<u64>` - Optional grace-period override
- `limit: Option<u32>` - Optional scan size, clamped to `1..=100`

**Returns**
- `overdue_count: u32` - Overdue funded invoices found in the scanned batch
- `scanned_count: u32` - Funded invoices inspected this call
- `total_funded: u32` - Funded invoice count in the snapshot used by the scan
- `next_cursor: u32` - Cursor position to continue from on the next call

### `get_overdue_scan_cursor`

Returns the current funded-invoice scan cursor.

### `get_overdue_scan_batch_limit`

Returns the default funded-invoice scan batch size used by `check_overdue_invoices*`.

## Bounded Overdue Scanning

The funded overdue scan is intentionally bounded so one contract call cannot walk an unbounded number of funded invoices.

### Safeguards

- Default batch size: `25`
- Maximum explicit batch size: `100`
- Traversal order: funded index insertion order
- Progress tracking: persistent rotating cursor in instance storage
- Cursor normalization: if the funded set shrinks and the cursor is out of range, scanning restarts at `0`
- Snapshot semantics: one funded-index snapshot is read, then at most `limit` entries from that snapshot are processed

### Operational Consequences

- `check_overdue_invoices*` now reports overdue invoices found in the scanned batch, not the entire funded population
- Repeated calls are required to cover large funded sets
- The scan remains deterministic because the cursor always advances from the last stored position

## Default Handling Flow

When an invoice is defaulted:

1. It is removed from the `Funded` status index.
2. Its status changes to `Defaulted`.
3. It is written back to storage.
4. It is added to the `Defaulted` status index.
5. The linked investment is marked `Defaulted`.
6. Insurance claims are processed when coverage exists.
7. Expiration and default events are emitted.

## Security Notes

1. Only funded invoices can be defaulted.
2. Already-defaulted invoices are rejected, preventing double-default paths.
3. Grace-period checks use `saturating_add` through `grace_deadline`, avoiding timestamp overflow regressions.
4. Overdue scan loops are bounded, reducing excessive per-call work risk.
5. Cursor-based traversal is deterministic and auditable.
6. Each bounded scan uses snapshot semantics, so invoices added or removed mid-run are handled on later calls rather than through inconsistent partial state.

## Testing

The default and overdue flows are covered in:

- `quicklendx-contracts/src/test_default.rs`
- `quicklendx-contracts/src/test_overdue_expiration.rs`

Key safeguard scenarios include:

- Single-invoice expiration checks
- Grace-boundary behavior
- Custom protocol grace-period resolution
- Batch-limit enforcement over large funded sets
- Cursor advancement and wraparound across repeated scans
- Partial defaulting when only part of the funded set is scanned

## Configuration

The relevant contract constants are:

```rust
pub const DEFAULT_GRACE_PERIOD: u64 = 7 * 24 * 60 * 60;
pub const DEFAULT_OVERDUE_SCAN_BATCH_LIMIT: u32 = 25;
pub const MAX_OVERDUE_SCAN_BATCH_LIMIT: u32 = 100;
```

## Monitoring Example

```typescript
const scan = await contract.scan_overdue_invoices(null, 25);

console.log({
  overdueFound: scan.overdue_count,
  scanned: scan.scanned_count,
  totalFunded: scan.total_funded,
  nextCursor: scan.next_cursor,
});
```

## Best Practices

1. Run bounded overdue scans on a schedule instead of relying on one catch-up call.
2. Monitor `next_cursor` during automation to confirm progress across large funded sets.
3. Keep grace-period changes explicit and documented in operator workflows.
4. Review default-rate analytics alongside insurance-claim activity.
