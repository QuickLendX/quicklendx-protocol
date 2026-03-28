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

Relevant tests live in [`src/test_default.rs`](../../src/test_default.rs) and
[`src/test_overdue_expiration.rs`](../../src/test_overdue_expiration.rs).
