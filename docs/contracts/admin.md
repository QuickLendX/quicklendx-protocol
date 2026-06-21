# Admin Transfer Safety Model

This document describes the admin-role safety behavior implemented in `quicklendx-contracts/src/admin.rs`.

## Goals

- Enforce **single-admin ownership**.
- Prevent unauthorized admin replacement.
- Support **safe rotation** via optional two-step flow.
- Prevent stuck/overlapping transfers using a transfer lock.
- Emit auditable events for every admin-state transition.

## Storage Keys

- `ADMIN_KEY` (`"admin"`): active admin address.
- `ADMIN_INITIALIZED_KEY` (`"adm_init"`): one-time initialization flag.
- `ADMIN_TRANSFER_LOCK_KEY` (`"adm_lock"`): transfer-in-progress lock.
- `ADMIN_PENDING_KEY` (`"adm_pnd"`): pending admin in two-step mode.
- `ADMIN_TWO_STEP_KEY` (`"adm_2st"`): optional two-step mode toggle.

## Initialization

`AdminStorage::initialize(env, admin)`:

- Requires `admin.require_auth()`.
- Fails if already initialized (`OperationNotAllowed`).
- Writes admin + initialized flag atomically.
- Emits `adm_init`.

## Transfer Modes

### One-step transfer (default)

`AdminStorage::transfer_admin(env, current_admin, new_admin)`:

- Requires current admin auth and role check.
- Rejects self-transfer.
- Rejects transfer if lock/pending state exists.
- Performs atomic swap `current -> new`.
- Emits `adm_trf`.

### Two-step transfer (optional)

Enable: `AdminStorage::set_two_step_enabled(env, admin, true)`.

Flow:

1. Current admin initiates transfer via `transfer_admin` (or `initiate_admin_transfer`).
2. Contract stores `ADMIN_PENDING_KEY`, sets transfer lock, emits `adm_req`.
3. Pending admin must call `accept_admin_transfer`.
4. On accept, active admin is updated, pending+lock are cleared, emits `adm_trf`.

Cancel path:

- Current admin may call `cancel_admin_transfer` before acceptance.
- Pending state + lock are cleared.
- Emits `adm_cnl`.

Disable behavior:

- `set_two_step_enabled(..., false)` clears pending+lock to avoid stuck transfer state.
- Emits `adm_2st`.

## Event Topics

- `adm_init`: admin initialized.
- `adm_trf`: admin transfer completed.
- `adm_req`: two-step transfer initiated.
- `adm_cnl`: pending transfer cancelled.
- `adm_2st`: two-step mode updated.

## Security Assumptions Verified by Tests

- Admin initialization is one-time.
- Unauthorized callers cannot replace admin.
- Transfer lock blocks overlapping/reentrant transfer attempts.
- Pending transfer can be accepted only by the nominated address.
- Pending/lock state can be safely cancelled or cleared (no stuck transfer).
- Admin transition events are emitted on each state change.

## Coverage Gate

To keep admin transfer safety regressions visible while legacy modules are still being migrated, CI enforces a dedicated coverage threshold for `src/admin.rs`:

- Report generation: `cargo llvm-cov --lib --lcov --output-path coverage/lcov.info`
- Admin gate: `scripts/check-admin-coverage.sh coverage/lcov.info`
- Minimum required: `95%` line coverage (`ADMIN_COVERAGE_MIN`, default `95`)
