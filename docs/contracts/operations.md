# Operations Guide

This document describes operational controls available to protocol admins:
maintenance mode, pause, and emergency recovery.

---

## Maintenance Mode

Maintenance mode is a **read-only switch** that blocks state-mutating operations
while keeping all query endpoints available. Use it for planned upgrades,
configuration changes, or any window where you need to freeze writes without
triggering a full emergency pause.

### When to use

| Scenario | Maintenance mode | Full pause |
|---|---|---|
| Scheduled contract upgrade | ✅ | ❌ |
| Routine fee/limit update | ✅ | ❌ |
| Security incident response | ❌ | ✅ |
| Emergency fund recovery | ❌ | ✅ |

### How it works

When maintenance mode is active:

- **Write operations** (`store_invoice`, `place_bid`, `accept_bid_and_fund`,
  `settle_invoice`, `submit_kyc_application`, etc.) return
  `MaintenanceModeActive` (error code `2201`).
- **Read operations** (`get_invoice`, `get_bid`, `get_escrow_details`,
  `is_maintenance_mode`, `get_maintenance_reason`, etc.) succeed normally.
- A **reason string** (max 256 bytes) is stored on-chain and returned by
  `get_maintenance_reason` so clients can display an explicit message to users.

### API

#### `set_maintenance_mode(admin, enabled, reason)`

Enables or disables maintenance mode. Admin-only; exempt from its own guard so
an admin can always exit.

| Parameter | Type | Notes |
|---|---|---|
| `admin` | `Address` | Must be the current protocol admin |
| `enabled` | `bool` | `true` = enter maintenance, `false` = exit |
| `reason` | `String` | Human-readable message; max 256 bytes; required on enable |

**Errors:**
- `NotAdmin` – caller is not the admin.
- `InvalidDescription` – reason exceeds 256 bytes.

**Events emitted:**
- `(MAINT, enabled)` → reason string (on enable)
- `(MAINT, disabled)` → admin address (on disable)

#### `is_maintenance_mode() → bool`

Returns the current maintenance flag. Always available (no auth required).

#### `get_maintenance_reason() → Option<String>`

Returns the stored reason string, or `None` if not in maintenance.
Always available (no auth required).

### Client integration

Clients should handle `MaintenanceModeActive` (error `2201`) explicitly and
display the reason to users:

```
if error.code == 2201:
    reason = contract.get_maintenance_reason()
    show_banner("Protocol maintenance: " + reason)
```

### Security assumptions

1. **Admin-only toggle** — only the current admin address can enable or disable.
   After an admin rotation the old admin loses authority immediately.
2. **Exempt from self** — `set_maintenance_mode` is not guarded by
   `require_write_allowed`, so an admin can always exit maintenance mode even
   while writes are frozen.
3. **Reason length bound** — reason strings are capped at 256 bytes to prevent
   storage abuse from a compromised admin key.
4. **Observable** — every toggle emits a contract event for off-chain
   monitoring and alerting.

---

## Pause (Emergency)

Full pause stops all business-state mutations immediately. Unlike maintenance
mode, it is intended for security incidents where writes must stop before the
cause is understood.

See [`pause.rs`](../../quicklendx-contracts/src/pause.rs) and the
[security docs](../security.md) for details.

### API

- `pause(admin)` — enter full pause.
- `unpause(admin)` — exit full pause.
- `is_paused() → bool` — current pause status.

---

## Maintenance vs Pause: coexistence

The two flags are **independent**. A protocol can be in maintenance mode, fully
paused, both, or neither. Each flag has its own storage key and its own guard
function:

| Flag | Guard function | Error on write |
|---|---|---|
| Maintenance | `MaintenanceControl::require_write_allowed` | `MaintenanceModeActive` (2201) |
| Pause | `PauseControl::require_not_paused` | `ContractPaused` (2100) |

Entrypoints that must respect both states should call both guards.
