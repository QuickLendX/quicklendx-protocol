# Default Handling and Grace Period

## Overview

The QuickLendX protocol implements strict access control and status validation for manual invoice default marking. A configurable grace period mechanism gives businesses additional time before an invoice is formally marked as defaulted, protecting all parties while maintaining accountability.

For the full default handling lifecycle and frontend integration guide, see [default-handling.md](./default-handling.md).

## Security Model

### Access Control

Manual default marking is **admin-only**:
- Requires `require_auth` on the configured admin address
- Non-admin callers receive `NotAdmin` error
- Authorization is enforced at the contract entry point in `lib.rs`

### Status Validation

Only invoices in **Funded** status can be manually defaulted:
- Prevents premature default marking on unbacked invoices
- Ensures investment relationship exists before default processing
- Returns `InvoiceNotAvailableForFunding` for non-Funded invoices

### Validation Order

Manual default marking validates in the following strict order:

1. **Invoice existence** - Must exist in storage
2. **Already defaulted** - Prevents double-default (returns `InvoiceAlreadyDefaulted`)
3. **Funded status** - Only `Funded` invoices eligible (returns `InvoiceNotAvailableForFunding`)
4. **Grace period expiry** - Current time must exceed deadline (returns `OperationNotAllowed`)

## Core Functions

### `mark_invoice_defaulted(invoice_id, grace_period)`

Public contract entry point for marking an invoice as defaulted.

**Authorization:** Admin only (`require_auth` on the configured admin address).

**Parameters:**

| Parameter | Type | Description |
|-----------|------|-------------|
| `invoice_id` | `BytesN<32>` | The invoice to mark as defaulted |
| `grace_period` | `Option<u64>` | Grace period in seconds. Defaults to 7 days (604,800s) if `None` |

**Validation order:**

1. Admin authentication check
2. Invoice existence check
3. Already-defaulted check (prevents double default)
4. Funded status check (only funded invoices can default)
5. Grace period expiry check (`current_timestamp > due_date + grace_period`)

**Errors:**

| Error | Code | Condition |
|-------|------|-----------|
| `NotAdmin` | 1103 | Caller is not the configured admin |
| `InvoiceNotFound` | 1000 | Invoice ID does not exist |
| `InvoiceAlreadyDefaulted` | 1006 | Invoice has already been defaulted |
| `InvoiceNotAvailableForFunding` | 1001 | Invoice is not in `Funded` status |
| `OperationNotAllowed` | 1402 | Grace period has not yet expired |

### `handle_default(invoice_id)`

Lower-level contract entry point that performs the default without grace period checks. Also requires admin authorization.

**Authorization:** Admin only.

**Behavior:**

1. Validates invoice exists and is in `Funded` status
2. Removes invoice from the `Funded` status list
3. Sets invoice status to `Defaulted`
4. Adds invoice to the `Defaulted` status list
5. Emits `invoice_expired` and `invoice_defaulted` events
6. Updates linked investment status to `Defaulted`
7. Processes insurance claims if coverage exists
8. Sends default notification
9. Updates investor analytics (failed investment)

### Validation Helper Functions

The module provides granular validation functions for external use:

#### `validate_invoice_for_default(env, invoice_id)`

Validates that an invoice exists and is eligible for default marking.

**Returns:**
- `Ok(())` if invoice is eligible
- `Err(InvoiceNotFound)` if not found
- `Err(InvoiceAlreadyDefaulted)` if already defaulted
- `Err(InvoiceNotAvailableForFunding)` if not in Funded status

#### `validate_grace_period_expired(env, invoice_id, grace_period)`

Validates that the grace period has expired for an invoice.

**Returns:**
- `Ok(())` if grace period has expired
- `Err(OperationNotAllowed)` if grace period has not expired

#### `can_mark_as_defaulted(env, invoice_id, grace_period)`

Read-only helper for UI pre-validation. Combines all checks.

**Returns:**
- `Ok(true)` if invoice can be defaulted
- `Err(QuickLendXError)` with specific reason if cannot be defaulted

## Grace Period

### Configuration

The default grace period is defined in `src/defaults.rs`:

```rust
pub const DEFAULT_GRACE_PERIOD: u64 = 7 * 24 * 60 * 60; // 7 days
```

Grace period resolution order:
1. `grace_period` argument (per-call override)
2. Protocol config (`ProtocolInitializer::get_protocol_config`)
3. `DEFAULT_GRACE_PERIOD` (7 days)

### Calculation

```
grace_deadline = invoice.due_date + grace_period
can_default    = current_timestamp > grace_deadline
```

The check uses strict greater-than (`>`), meaning the invoice cannot be defaulted at exactly the deadline timestamp — only after it.

### Examples

| Scenario | Due Date | Grace Period | Deadline | Current Time | Can Default? |
|----------|----------|-------------|----------|-------------|-------------|
| Default 7-day grace | Day 0 | 7 days | Day 7 | Day 8 | Yes |
| Before grace expires | Day 0 | 7 days | Day 7 | Day 3 | No |
| Exactly at deadline | Day 0 | 7 days | Day 7 | Day 7 | No |
| Custom 3-day grace | Day 0 | 3 days | Day 3 | Day 4 | Yes |
| Zero grace period | Day 0 | 0 seconds | Day 0 | Day 0 + 1s | Yes |

## State Transitions

```
Invoice:    Funded ──→ Defaulted
Investment: Active ──→ Defaulted
```

When an invoice is defaulted:

- **Status lists** are updated (removed from `Funded`, added to `Defaulted`)
- **Investment status** is set to `Defaulted`
- **Insurance claims** are processed automatically if coverage exists
- **Investor analytics** are updated to reflect the failed investment
- **Events emitted:** `invoice_expired`, `invoice_defaulted`, and optionally `insurance_claimed`
- **Notifications** are sent to relevant parties

## Security Features

### Admin-Only Access

Both `mark_invoice_defaulted` and `handle_default` require `require_auth` from the configured admin address. This prevents unauthorized parties from triggering defaults.

### No Double Default

Attempting to default an already-defaulted invoice returns `InvoiceAlreadyDefaulted` (1006). This prevents duplicate default processing and ensures idempotent behavior.

### Check Ordering

The defaulted-status check runs before the funded-status check so that double-default attempts receive the correct, specific error.

### Grace Period Enforcement

Invoices cannot be defaulted before `due_date + grace_period` has elapsed. This protects borrowers during the grace period.

### Overflow Protection

`grace_deadline` uses `saturating_add` to prevent timestamp overflow when adding grace period to due date.

### Idempotent Operations

The validation functions can be safely called multiple times without side effects. Only `handle_default` performs state mutations.

## Test Coverage

Tests are in `src/test_default.rs` and `src/test_errors.rs`:

### Default Tests (`test_default.rs`)

| Test | Description |
|------|-------------|
| `test_default_after_grace_period` | Default succeeds after grace period expires |
| `test_no_default_before_grace_period` | Default rejected during grace period |
| `test_cannot_default_unfunded_invoice` | Verified-only invoice cannot be defaulted |
| `test_cannot_default_pending_invoice` | Pending invoice cannot be defaulted |
| `test_cannot_default_already_defaulted_invoice` | Double default returns `InvoiceAlreadyDefaulted` |
| `test_custom_grace_period` | Custom 3-day grace period works correctly |
| `test_default_uses_default_grace_period_when_none_provided` | `None` grace period uses 7-day default |
| `test_default_status_transition` | Status lists updated correctly |
| `test_default_investment_status_update` | Investment status changes to `Defaulted` |
| `test_default_exactly_at_grace_deadline` | Boundary: cannot default at exact deadline, can at deadline+1 |
| `test_multiple_invoices_default_handling` | Independent invoices default independently |
| `test_zero_grace_period_defaults_immediately_after_due_date` | Zero grace allows immediate default after due date |
| `test_cannot_default_paid_invoice` | Paid invoices cannot be defaulted |
| `test_mark_default_requires_admin_auth` | Admin authorization is enforced |
| `test_validate_invoice_for_default_rejects_not_found` | Invalid invoice ID rejected |
| `test_validate_invoice_for_default_rejects_cancelled` | Cancelled invoices rejected |
| `test_validate_invoice_for_default_rejects_refunded` | Refunded invoices rejected |
| `test_grace_period_exactly_one_second_before_deadline` | One second before deadline rejected |
| `test_grace_period_one_second_after_deadline` | One second after deadline accepted |
| `test_very_long_grace_period` | Very long grace periods work correctly |
| `test_double_default_returns_same_error` | Idempotent error for double default |
| `test_investment_status_transitions_on_default` | Investment status updates correctly |
| `test_status_lists_updated_atomically` | Status lists transition correctly |
| `test_grace_period_uses_protocol_config` | Protocol config is honored |
| `test_per_call_grace_overrides_protocol_config` | Per-call override works |

### Error Tests (`test_errors.rs`)

| Test | Description |
|------|-------------|
| `test_manual_default_not_admin_error` | Non-admin returns NotAdmin error |
| `test_manual_default_invoice_not_found_error` | Invalid ID returns InvoiceNotFound |
| `test_manual_default_already_defaulted_error` | Double default returns InvoiceAlreadyDefaulted |
| `test_manual_default_not_funded_error` | Non-funded returns InvoiceNotAvailableForFunding |
| `test_manual_default_grace_period_not_expired_error` | Early default returns OperationNotAllowed |
| `test_default_cannot_mark_pending_invoice` | Pending invoices rejected |
| `test_default_cannot_mark_cancelled_invoice` | Cancelled invoices rejected |
| `test_default_cannot_mark_paid_invoice` | Paid invoices rejected |
| `test_default_cannot_mark_refunded_invoice` | Refunded invoices rejected |
| `test_default_error_codes_are_correct` | Error codes match expected values |
| `test_no_panic_on_invalid_invoice_in_default` | Invalid IDs return errors, no panics |

Run tests:

```bash
cd quicklendx-contracts
cargo test test_default -- --nocapture
cargo test test_errors -- --nocapture
```

Run with coverage:

```bash
cargo test -- --nocapture
```

## Frontend Integration

### Checking if Invoice Can Be Defaulted

```typescript
async function canMarkAsDefaulted(invoiceId: string): Promise<boolean> {
  try {
    const gracePeriod = 7 * 24 * 60 * 60; // 7 days
    await contract.mark_invoiceDefaulted(invoiceId, gracePeriod);
    return true;
  } catch (error) {
    if (error.code === 1006) { // InvoiceAlreadyDefaulted
      return false; // Already defaulted
    }
    if (error.code === 1402) { // OperationNotAllowed
      return false; // Grace period not expired
    }
    if (error.code === 1001) { // InvoiceNotAvailableForFunding
      return false; // Not in Funded status
    }
    throw error; // Unexpected error
  }
}
```

### Error Handling

```typescript
try {
  await contract.markInvoiceDefaulted(invoiceId, gracePeriod);
} catch (error) {
  switch (error.code) {
    case 1103: // NotAdmin
      console.error("Only admin can mark invoices as defaulted");
      break;
    case 1000: // InvoiceNotFound
      console.error("Invoice does not exist");
      break;
    case 1006: // InvoiceAlreadyDefaulted
      console.error("Invoice is already defaulted");
      break;
    case 1001: // InvoiceNotAvailableForFunding
      console.error("Invoice must be in Funded status");
      break;
    case 1402: // OperationNotAllowed
      console.error("Grace period has not expired");
      break;
  }
}
```

## Audit Trail

The following events are emitted during default operations:

| Event | Description |
|-------|-------------|
| `invoice_expired` | Invoice has passed its due date + grace period |
| `invoice_defaulted` | Invoice has been marked as defaulted |
| `insurance_claimed` | Insurance claim processed for defaulted invoice |

These events provide a complete audit trail for compliance and dispute resolution.
