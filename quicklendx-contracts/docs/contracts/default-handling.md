# Default Handling

## Overview

QuickLendX treats invoice defaulting as a strict post-deadline transition for funded invoices only.
An invoice may move to `Defaulted` only when the current ledger timestamp is greater than the
invoice due date plus the resolved grace period.

Effective grace period resolution follows this order:

1. Per-call override passed into `mark_invoice_defaulted` or `check_invoice_expiration`
2. Protocol configuration stored by `ProtocolInitializer`
3. `DEFAULT_GRACE_PERIOD` in [`src/defaults.rs`](../../src/defaults.rs)

This means an invoice at exactly `due_date + grace_period` is still inside the grace window and
must not default yet.

## Single-Shot Default Execution Guard

To prevent duplicate default execution and repeated side effects, QuickLendX implements a
"single-shot" guard mechanism in [`src/defaults.rs`](../../src/defaults.rs).

### Guard Mechanism

- **Storage**: Per-invoice guard flag stored in persistent storage using `DefaultGuardKey::DefaultExecuted(invoice_id)`
- **Immutability**: Once set to `true`, the guard flag cannot be reset by any caller
- **Scope**: Guard is scoped per-invoice; defaulting one invoice does not affect others
- **Pattern**: Follows Checks-Effects-Interactions pattern:
  1. **Check**: `is_default_executed()` returns early with `InvoiceAlreadyDefaulted` if guard is set
  2. **Effect**: `set_default_executed()` arms the guard *before* any state mutations or events
  3. **Interaction**: Side effects (events, insurance claims, status updates) occur only after guard is armed

### Security Properties

- **Duplicate Prevention**: Prevents re-execution of default logic even if invoice status is manipulated back to `Funded`
- **Side Effect Isolation**: Ensures events, insurance claims, and analytics are emitted exactly once
- **Reentrancy Protection**: Guard check occurs before any storage reads, preventing bypass via status manipulation
- **No Reset API**: No public interface exists to clear the guard flag

### Error Handling

- `InvoiceAlreadyDefaulted`: Returned when attempting to default an invoice that has already been defaulted
- Error is returned regardless of current invoice status, prioritizing guard state over status checks

## Boundary Rules

- `timestamp <= due_date + grace_period`: invoice remains non-defaulted
- `timestamp == due_date + grace_period`: default is rejected
- `timestamp == due_date + grace_period + 1`: default becomes eligible
- `grace_period == 0`: the invoice can default only after the due date, not at the due date

## Security Notes

- Defaulting is restricted to funded invoices, preventing invalid transitions from `Pending`,
  `Verified`, or unrelated statuses.
- Public `mark_invoice_defaulted` requires admin authorization before any state mutation.
- `handle_default` is only safe after callers enforce status and time checks.
- The cutoff uses a strict greater-than rule to prevent early liquidation at the grace boundary.
- Grace resolution prefers explicit inputs over mutable config, making caller intent deterministic.
- Deadline calculation uses `saturating_add`, avoiding arithmetic overflow during grace resolution.

## Regression Coverage

The contract test suite covers:

- default after grace expiry
- rejection before grace expiry
- exact-deadline rejection
- protocol-config grace resolution when `None` is supplied
- fallback to `DEFAULT_GRACE_PERIOD` when config is absent
- zero-grace handling
- investment status transition to `Defaulted`
- **single-shot guard functionality**:
  - successful first execution
  - rejection of duplicate attempts
  - guard immutability
  - per-invoice scoping
  - guard precedence over status checks

Relevant tests live in [`src/test_default.rs`](../../src/test_default.rs) and
[`src/test_overdue_expiration.rs`](../../src/test_overdue_expiration.rs).
