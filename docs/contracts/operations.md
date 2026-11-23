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

Full pause is the protocol's **emergency circuit breaker**. It stops all
business-state mutations immediately. Unlike maintenance mode, it is intended
for security incidents where writes must stop before the cause is understood.

See [`pause.rs`](../../quicklendx-contracts/src/pause.rs) and the
[security docs](../security.md) for details.

### API

- `pause(admin)` — enter full pause. Admin-only; exempt from its own guard.
- `unpause(admin)` — exit full pause. Admin-only; exempt from its own guard.
- `is_paused() → bool` — current pause status. Always available (no auth).
- `is_entrypoint_paused(entrypoint) → bool` — returns whether a specific guarded write entrypoint
  is currently blocked by pause. Use one of the stable `EP_*` symbols from `pause.rs`.

### How it works

When the protocol is paused, every state-mutating entrypoint calls
`PauseControl::require_not_paused` as its **first statement** and returns
`ContractPaused` (error code `2100`) before any state is read or mutated.
Read-only queries are never gated and continue to succeed. Unpausing restores
normal operation for every entrypoint with no residual state — the breaker is
fully reversible.

`pause` / `unpause` are intentionally **exempt** from the guard so an admin can
always enter and exit emergency mode while user and business flows are frozen.

### Pause matrix

The matrix below is the authoritative list of the core value-moving
entrypoints and their pause behavior. Each row is enforced by a test in
[`test_pause.rs`](../../quicklendx-contracts/src/test_pause.rs): a *blocked*
test asserting `ContractPaused` while paused, and an *unpause-recovery* test
asserting the same call succeeds afterward.

| Entrypoint | Kind | Paused? | Guard call | Blocked test | Recovery test |
|---|---|---|---|---|---|
| `store_invoice` | mutating | ❌ rejected (`ContractPaused`) | `require_not_paused` first | `test_pause_blocks_user_and_invoice_state_mutations` | `test_unpause_restores_store_invoice` |
| `place_bid` | mutating | ❌ rejected (`ContractPaused`) | `require_not_paused` first | `test_pause_blocks_place_bid` | `test_unpause_restores_place_bid` |
| `accept_bid_and_fund` | mutating | ❌ rejected (`ContractPaused`) | `require_not_paused` first | `test_pause_blocks_accept_bid_and_fund` | `test_unpause_restores_accept_bid_and_fund` |
| `process_partial_payment` | mutating | ❌ rejected (`ContractPaused`) | `require_not_paused` first | `test_pause_blocks_process_partial_payment` | `test_unpause_restores_process_partial_payment` |
| `make_payment` (alias) | mutating | ❌ rejected (`ContractPaused`) | `require_not_paused` first | `test_pause_blocks_make_payment_alias` | — |
| `settle_invoice` | mutating | ❌ rejected (`ContractPaused`) | `require_not_paused` first | `test_pause_blocks_settle_invoice` | `test_unpause_restores_settle_invoice` |
| `get_invoice` / `get_bid` / other getters | read-only | ✅ allowed | none | — | `test_pause_allows_all_query_functions` |

Mid-lifecycle behavior (pausing *after* an invoice is funded and partially
paid) is covered by `test_pause_mid_lifecycle_freezes_then_resumes_payment`:
the in-flight payment is frozen and `total_paid` is unchanged while paused, then
the remaining payment completes after unpause.

> **Circuit-breaker invariant:** every mutating entrypoint must call
> `require_not_paused` before touching state. A mutating path that silently
> ignores the flag would let value move while the protocol is supposed to be
> frozen, defeating the breaker. `process_partial_payment` and its
> `make_payment` alias previously lacked this guard; the guard and its
> regression tests close that gap. When adding a new mutating entrypoint, add it
> to this matrix and add a blocked + recovery test pair.

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

---

## Incident Mode (Coordinated Pause + Maintenance)

For **security incident response**, use the coordinated entrypoints instead of
calling `pause` and `set_maintenance_mode` separately. A single
`enter_incident_mode` invocation atomically engages both circuit breakers and
returns an auditable snapshot.

### API

#### `enter_incident_mode(admin, reason) → IncidentSnapshot`

| Field | Type | Description |
|---|---|---|
| `is_paused` | `bool` | Hard pause flag after the call |
| `is_maintenance` | `bool` | Maintenance flag after the call |
| `reason` | `String` | Stored maintenance reason (max 256 bytes) |
| `timestamp` | `u64` | Ledger timestamp when the snapshot was taken |

**Errors:** `NotAdmin`, `InvalidDescription` (reason too long).

#### `exit_incident_mode(admin) → IncidentSnapshot`

Clears both pause and maintenance. Idempotent when already in normal operation.

### Runbook

See [`reliability.md`](../../reliability.md#on-chain-incident-mode-protocol-runbook) for the operator checklist.
