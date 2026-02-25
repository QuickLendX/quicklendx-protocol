# Default Handling and Grace Period

## Overview

The QuickLendX protocol implements configurable default handling for invoices that remain unpaid past their due date. A grace period mechanism gives businesses additional time before an invoice is formally marked as defaulted, protecting all parties while maintaining accountability.

For the full default handling lifecycle and frontend integration guide, see [default-handling.md](./default-handling.md).

## Core Functions

### `mark_invoice_defaulted(invoice_id, grace_period)`

Public contract entry point for marking an invoice as defaulted.

**Authorization:** Admin only (`require_auth` on the configured admin address).

**Parameters:**

| Parameter | Type | Description |
|-----------|------|-------------|
| `invoice_id` | `BytesN<32>` | The invoice to mark as defaulted |
| `grace_period` | `Option<u64>` | Grace period in seconds. If `None`, uses protocol config; if not configured, defaults to 7 days (604,800s). |

**Validation order:**

1. Admin authentication check
2. Invoice existence check
3. Already-defaulted check (prevents double default)
4. Funded status check (only funded invoices can default)
5. Grace period expiry check (`current_timestamp > due_date + grace_period`)

**Errors:**

| Error | Code | Condition |
|-------|------|-----------|
| `NotAdmin` | 1005 | Caller is not the configured admin |
| `InvoiceNotFound` | 1000 | Invoice ID does not exist |
| `InvoiceAlreadyDefaulted` | 1049 | Invoice has already been defaulted |
| `InvoiceNotAvailableForFunding` | 1047 | Invoice is not in `Funded` status |
| `OperationNotAllowed` | 1009 | Grace period has not yet expired |

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

## Grace Period

### Configuration

Grace period resolution order:

1. `grace_period` argument (per-call override)
2. Protocol config (`ProtocolInitializer::get_protocol_config`)
3. Default of 7 days (604,800 seconds)

Callers can override the protocol config per invocation by passing `Some(custom_seconds)`.

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

## Security

- **Admin-only access:** Both `mark_invoice_defaulted` and `handle_default` require `require_auth` from the configured admin address
- **No double default:** Attempting to default an already-defaulted invoice returns `InvoiceAlreadyDefaulted` (1049)
- **Check ordering:** The defaulted-status check runs before the funded-status check so that double-default attempts receive the correct, specific error
- **Grace period enforcement:** Invoices cannot be defaulted before `due_date + grace_period` has elapsed
- **Overflow protection:** `grace_deadline` uses `saturating_add` to prevent timestamp overflow

## Test Coverage

Tests are in `src/test_default.rs` (12 tests):

| Test | Description |
|------|-------------|
| `test_default_after_grace_period` | Default succeeds after grace period expires |
| `test_no_default_before_grace_period` | Default rejected during grace period |
| `test_cannot_default_unfunded_invoice` | Verified-only invoice cannot be defaulted |
| `test_cannot_default_pending_invoice` | Pending invoice cannot be defaulted |
| `test_cannot_default_already_defaulted_invoice` | Double default returns `InvoiceAlreadyDefaulted` |
| `test_custom_grace_period` | Custom 3-day grace period works correctly |
| `test_default_uses_default_grace_period_when_none_provided` | `None` grace period uses 7-day default |
| `test_default_uses_protocol_config_when_none` | `None` grace period uses protocol-configured grace |
| `test_check_invoice_expiration_uses_protocol_config_when_none` | Expiration checks honor protocol-configured grace |
| `test_per_invoice_grace_overrides_protocol_config` | Per-invoice grace period overrides protocol config |
| `test_default_status_transition` | Status lists updated correctly |
| `test_default_investment_status_update` | Investment status changes to `Defaulted` |
| `test_default_exactly_at_grace_deadline` | Boundary: cannot default at exact deadline, can at deadline+1 |
| `test_multiple_invoices_default_handling` | Independent invoices default independently |
| `test_zero_grace_period_defaults_immediately_after_due_date` | Zero grace allows immediate default after due date |
| `test_cannot_default_paid_invoice` | Paid invoices cannot be defaulted |

Run tests:

```bash
cd quicklendx-contracts
cargo test test_default -- --nocapture
```
