# Default Handling and Grace Period

## Overview

The QuickLendX protocol implements configurable default handling for invoices that remain unpaid past their due date. A grace period mechanism gives businesses additional time before an invoice is formally marked as defaulted, protecting all parties while maintaining accountability.

For the full default handling lifecycle and frontend integration guide, see [default-handling.md](./default-handling.md).

## API Ordering and Overlap

The protocol exposes two admin-only entry points for defaulting an invoice. Both converge on the same internal state-transition helper (`do_handle_default`) so the outcome — status update, events, insurance processing — is identical and executed **exactly once**.

| Entry point | Grace-period check | When to use |
|---|---|---|
| `mark_invoice_defaulted(id, grace)` | Explicit `Option<u64>` override | Caller wants to supply or override the grace period |
| `handle_default(id)` | Protocol config / `DEFAULT_GRACE_PERIOD` | Caller wants the protocol-default grace period enforced automatically |

### No double-accounting guarantee

Both entry points check `invoice.status == Defaulted` before making any state change and return `InvoiceAlreadyDefaulted` immediately if the invoice has already been processed. This means:

- Calling `handle_default` after `mark_invoice_defaulted` (or vice-versa) on the same invoice is safe — the second call is a no-op error, not a second state transition.
- The funded-invoice list, investment status, and emitted events are updated exactly once regardless of which path is taken first.

### Ordering invariant

```
mark_invoice_defaulted  ──┐
                           ├──► do_mark_invoice_defaulted ──► do_handle_default ──► state written once
handle_default          ──┘
```

`handle_default` calls `do_mark_invoice_defaulted` (with `grace_period = None`) rather than `do_handle_default` directly, so it enforces the same time guard as `mark_invoice_defaulted`.

## Core Functions

### `mark_invoice_defaulted(invoice_id, grace_period)`

Public contract entry point for marking an invoice as defaulted.

**Authorization:** Admin only (`require_auth` on the configured admin address).

**Parameters:**

| Parameter | Type | Description |
|-----------|------|-------------|
| `invoice_id` | `BytesN<32>` | The invoice to mark as defaulted |
| `grace_period` | `Option<u64>` | Grace period override in seconds. If `None`, uses protocol config; falls back to `DEFAULT_GRACE_PERIOD` (7 days). |

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

Admin entry point that applies the default using the protocol-configured grace period.

**Authorization:** Admin only.

**Grace period:** Resolved from protocol config (`ProtocolInitializer::get_protocol_config`) or `DEFAULT_GRACE_PERIOD` (7 days) when not configured. Equivalent to calling `mark_invoice_defaulted(id, None)`.

**Behavior:**

1. Admin authentication check
2. Grace-period expiry check (same as `mark_invoice_defaulted`)
3. Validates invoice exists and is in `Funded` status
4. Removes invoice from the `Funded` status list
5. Sets invoice status to `Defaulted`
6. Adds invoice to the `Defaulted` status list
7. Emits `invoice_expired` and `invoice_defaulted` events
8. Updates linked investment status to `Defaulted`
9. Processes insurance claims if coverage exists

## Grace Period

### Configuration

Grace period resolution order:

1. `grace_period` argument (per-call override, `mark_invoice_defaulted` only)
2. Protocol config (`ProtocolInitializer::get_protocol_config`)
3. Default of 7 days (604,800 seconds)

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
- **Events emitted:** `invoice_expired`, `invoice_defaulted`, and optionally `insurance_claimed`

## Security

- **Admin-only access:** Both `mark_invoice_defaulted` and `handle_default` require `require_auth` from the configured admin address
- **No double default:** Attempting to default an already-defaulted invoice returns `InvoiceAlreadyDefaulted` (1049)
- **Grace period enforcement:** Both entry points enforce `current_timestamp > due_date + grace_period` before any state change
- **No bypass via `handle_default`:** `handle_default` routes through `do_mark_invoice_defaulted`, not directly to `do_handle_default`, so the time guard cannot be skipped
- **Overflow protection:** `grace_deadline` uses `saturating_add` to prevent timestamp overflow

## Test Coverage

Tests are in `src/test_default.rs`:

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
| `test_handle_default_respects_grace_period` | `handle_default` enforces the same time guard |
| `test_handle_default_succeeds_after_grace_period` | `handle_default` succeeds once grace elapsed |
| `test_no_double_accounting_handle_default_then_mark_defaulted` | No double-accounting: `handle_default` → `mark_invoice_defaulted` |
| `test_no_double_accounting_mark_defaulted_then_handle_default` | No double-accounting: `mark_invoice_defaulted` → `handle_default` |
| `test_both_paths_produce_identical_state` | Both entry points produce identical final state |

Run tests:

```bash
cd quicklendx-contracts
cargo test test_default -- --nocapture
```
