# Admin Access Control

This document describes the admin model used by the QuickLendX Soroban contract, including which privileged operations remain available while the protocol is paused.

## Design Goals

- Enforce a single effective admin identity across legacy and current storage paths.
- Keep emergency and governance controls available during incident response.
- Keep business and market state mutations frozen while pause mode is active.
- Make pause semantics explicit so reviews can verify there is no contradictory behavior.

## Storage Model

Primary admin state lives in [`quicklendx-contracts/src/admin.rs`](/c:/Users/ADMIN/Desktop/remmy-drips/quicklendx-protocol/quicklendx-contracts/src/admin.rs):

- `ADMIN_KEY` (`"admin"`): current canonical admin.
- `ADMIN_INITIALIZED_KEY` (`"adm_init"`): one-time initialization flag.

Legacy verification storage still exposes compatibility reads and writes through `set_admin(...)` / `get_admin(...)`, but those paths synchronize with the canonical admin state so pause, emergency, and verification entrypoints all consult the same authority.

## Initialization And Rotation

`initialize_admin(admin)`:

- Requires `admin.require_auth()`.
- Can only succeed once.
- Establishes the canonical admin used by pause, emergency, whitelist, fee, and verification flows.

`transfer_admin(new_admin)`:

- Requires auth from the currently stored admin.
- Rotates the canonical admin atomically.
- Takes effect immediately for pause-exempt governance operations.

Legacy `set_admin(admin)` remains for backward compatibility and synchronizes the same effective admin used by newer entrypoints.

## Pause Policy

Pause mode is enforced by explicit entrypoint guards, not by a blanket runtime switch. The result is an intentional policy split:

Allowed while paused:

- `pause(admin)` and `unpause(admin)`
- `transfer_admin(new_admin)`
- Emergency recovery lifecycle:
  - `initiate_emergency_withdraw(admin, ...)`
  - `cancel_emergency_withdraw(admin)`
  - `execute_emergency_withdraw(admin)`
- Governance and config updates:
  - `set_bid_ttl_days(...)`
  - `set_platform_fee(...)`
  - currency whitelist management
  - protocol limit updates
- Admin KYC review actions:
  - `verify_business(...)`
  - `reject_business(...)`
  - `verify_investor(...)`
  - `reject_investor(...)`
  - `set_investment_limit(...)`

Blocked while paused:

- Invoice creation and upload
- Bid placement, acceptance, and withdrawal
- Invoice verification and other business-state mutations guarded by `PauseControl::require_not_paused`

Important nuance:

- Not every admin-only entrypoint is pause-exempt.
- `verify_invoice(...)` is intentionally still blocked while paused because it mutates live protocol business state rather than governance configuration.

## Security Notes

- Pause-exempt governance and emergency paths now use a shared admin-auth helper so they all require both authentication and current-admin membership.
- Currency whitelist updates explicitly require admin auth; callers can no longer rely on passing the stored admin address value alone.
- Emergency withdrawal remains pause-exempt, but its timelock is still enforced and should be treated as the primary operational safeguard.
- Tests in [`quicklendx-contracts/src/test_pause.rs`](/c:/Users/ADMIN/Desktop/remmy-drips/quicklendx-protocol/quicklendx-contracts/src/test_pause.rs) and [`quicklendx-contracts/src/test_admin.rs`](/c:/Users/ADMIN/Desktop/remmy-drips/quicklendx-protocol/quicklendx-contracts/src/test_admin.rs) document the intended pause matrix and the admin authority assumptions behind it.
