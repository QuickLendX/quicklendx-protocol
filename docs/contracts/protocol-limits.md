# Protocol Limits

This document describes the protocol-wide boundary values enforced during invoice creation and related validation.

## Overview

Protocol limits are stored in contract instance storage and are used to validate:

- Minimum invoice amount on `store_invoice` and `upload_invoice`
- Maximum invoice due-date horizon (how far in the future `due_date` is allowed to be)
- Default grace period used to compute default/overdue deadlines
- (Related limits) minimum bid settings and max invoices per business

## Limit Fields

The on-chain configuration is represented by `ProtocolLimits` in `quicklendx-contracts/src/protocol_limits.rs`:

- `min_invoice_amount` (`i128`): Minimum allowed invoice amount (smallest unit)
- `min_bid_amount` (`i128`): Absolute minimum bid amount (smallest unit)
- `min_bid_bps` (`u32`): Minimum bid amount as a percent of invoice amount (basis points, 10_000 = 100%)
- `max_due_date_days` (`u64`): Maximum allowed future due-date horizon in days
- `grace_period_seconds` (`u64`): Grace period added to `due_date` to compute default deadline
- `max_invoices_per_business` (`u32`): Maximum active invoices allowed per business (`0` = unlimited)

## Defaults

When limits are not explicitly configured, the contract falls back to defaults:

- `min_invoice_amount`: `1_000_000` (1 token at 6 decimals; in tests this is `10`)
- `min_bid_amount`: `10`
- `min_bid_bps`: `100` (1%)
- `max_due_date_days`: `365`
- `grace_period_seconds`: `604_800` (7 days)
- `max_invoices_per_business`: `100`

## Enforcement Points

Invoice creation enforces these limits in the following flows:

- `QuickLendXContract::store_invoice` calls `protocol_limits::ProtocolLimitsContract::validate_invoice`
- `QuickLendXContract::upload_invoice` calls `verification::verify_invoice_data`, which also calls `validate_invoice`

Validation rules:

1. Amount must be `>= min_invoice_amount`
2. `due_date` must be **in the future** (`due_date > ledger.timestamp()`)
3. `due_date` must be **no later than** `ledger.timestamp() + max_due_date_days * 86_400`

Boundary behavior is inclusive for the maximum due-date (exactly at the computed max is allowed).

## Updating Limits

Admin-only entrypoints exist on `QuickLendXContract`:

- `initialize_protocol_limits(admin, min_invoice_amount, max_due_date_days, grace_period_seconds)`
- `set_protocol_limits(admin, min_invoice_amount, max_due_date_days, grace_period_seconds)`
- `update_protocol_limits(admin, min_invoice_amount, max_due_date_days, grace_period_seconds)`
- `update_limits_max_invoices(admin, min_invoice_amount, max_due_date_days, grace_period_seconds, max_invoices_per_business)`

Authorization is enforced via `admin::AdminStorage` (`admin.require_auth()` + admin check).

## Parameter Bounds

Updates are rejected unless:

- `min_invoice_amount > 0`
- `min_bid_amount > 0`
- `min_bid_bps <= 10_000`
- `1 <= max_due_date_days <= 730`
- `grace_period_seconds <= 2_592_000` (30 days)
- `grace_period_seconds <= max_due_date_days * 86_400` (sanity check against contradictory horizons)

The last rule prevents inconsistent configurations where the grace period
is longer than the allowed due-date horizon.

## Test Coverage Notes

`quicklendx-contracts/src/test_protocol_limits.rs` covers:

- admin-only authorization for all limit-update entrypoints
- rejection of invalid bounds (`min_invoice_amount`, `max_due_date_days`, `grace_period_seconds`)
- rejection of invalid parameter combinations (grace period exceeding due-date horizon)
- internal protocol limit update sanity for bid constraints (`min_bid_amount`, `min_bid_bps`)
- immediate application of updated limits on invoice validation and default-date computation
- immediate application of `max_invoices_per_business` updates
- initialization failure for invalid limit combinations before state commit

## Security Notes

- **Admin authorization**: limit updates require admin authorization and are checked against the stored admin address.
- **Timestamp trust**: validation uses `env.ledger().timestamp()`; it assumes ledger timestamps are monotonic and within normal network bounds.
- **Overflow safety**: due-date computations use saturating arithmetic (`saturating_add` / `saturating_mul`) to prevent wrap-around.
- **Configuration coherence**: updates reject contradictory limit combinations to avoid misconfigured risk windows.
