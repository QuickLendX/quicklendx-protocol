# Admin Access Control

This document describes the admin model used by the QuickLendX Soroban contract.

## Design Goals

- Enforce a single canonical admin address in contract storage.
- Allow one-time initialization only.
- Require authenticated transfer by the current admin.
- Guard all privileged entrypoints with consistent checks.

## Storage Model

Admin state is stored in `src/admin.rs` using instance storage:

- `ADMIN_KEY` (`"admin"`): current admin address.
- `ADMIN_INITIALIZED_KEY` (`"adm_init"`): boolean flag preventing re-initialization.

`AdminStorage` is the source of truth for admin checks.

## Initialization Rules

`initialize_admin(admin)` and protocol `initialize(...)` both enforce:

- `admin.require_auth()` must succeed.
- Admin can only be set once.
- Re-initialization returns `OperationNotAllowed`.

## Transfer Rules

`transfer_admin(new_admin)` enforces:

- Current admin must already exist.
- Current admin must authenticate (`require_auth`).
- Stored admin must match the authenticated caller.
- Admin is updated atomically and an admin transfer event is emitted.

## Privileged Operations

Privileged methods are guarded by one of two internal checks in `src/lib.rs`:

- `require_current_admin(&Env)`:
  - Loads the stored admin,
  - requires auth from that address,
  - returns the verified admin address.
- `require_specific_admin(&Env, &Address)`:
  - Validates caller address equals stored admin,
  - then requires auth for that exact address.

This is applied to admin-sensitive methods including:

- invoice verification and status mutation,
- platform fee updates and fee-system configuration,
- dispute review/resolution,
- investor verification/rejection and limit management,
- analytics export/update operations,
- revenue distribution controls,
- backup management,
- invoice clearing utilities.

## Backward Compatibility

Legacy `set_admin(...)` remains available for compatibility with existing tests/integrations.

Behavior:

- If admin is uninitialized, it performs authenticated initialization.
- If admin is initialized, it performs authenticated transfer from current admin.
- Legacy verification storage is synchronized after updates for compatibility reads.

## Security Notes

- Privileged wrappers no longer rely on caller-supplied addresses alone.
- Anonymous admin initialization is blocked.
- Admin-only comments now match actual runtime enforcement.
- Legacy compatibility path still preserves single-admin invariants.
