# QuickLendX Protocol — Pause Taxonomy & Operational Guidance

> [!IMPORTANT]
> **Target Audience:** Protocol Operators, Incident Responders, and Support Engineers.
> This document provides a complete taxonomy of every pause reason, circuit breaker mode, trigger condition, affected entrypoints, auto-expiry bounds, and step-by-step operational recovery guidance for the QuickLendX Soroban smart contracts.

---

## 1. Pause Reason Taxonomy Table

| Reason Code / Identifier | Category | Trigger Condition | Affected Entrypoints | Error / Event Symbol | Auto-Expiry Bound | Operational Guidance & Recovery |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| `EMERGENCY_CIRCUIT_BREAKER` | Hard Pause | Security exploit, reentrancy risk, oracle feed anomaly | All write entrypoints (`store_invoice`, `place_bid`, `accept_bid`, etc.) | `ContractPaused` (101) / `PauseBlocked` | 7 Days (`MAX_PAUSE_DURATION`) | Execute triage, deploy fix, invoke `PauseControl::set_paused(admin, false)` |
| `SCHEDULED_MAINTENANCE` | Soft Maintenance | Indexer backfilling, schema migration, TTL extension sweep | Mutating ops requiring write access | `MaintenanceModeActive` (102) | Manual operator toggle | Complete maintenance window, run `extend_protocol_ttl`, disable via `set_maintenance_mode(admin, false, "")` |
| `COORDINATED_INCIDENT_ISOLATION` | Atomic Combo | Critical multi-component vulnerability alert | All write ops + sets maintenance banner | `IncidentSnapshot` / `ContractPaused` | 7 Days (hard pause component) | Inspect `IncidentSnapshot`, conduct forensic analysis, invoke `IncidentControl::exit_incident_mode(admin)` |
| `BACKPRESSURE_LOAD_SHEDDING` | System Capacity | High transaction concurrency or storage queue saturation | Non-critical invoice uploads & bid placements | `BackpressureSheddingActive` / Event | Self-regulating upon queue drain | Monitor RPC throughput, scale relay nodes, wait for queue capacity normalisation |
| `TIMELOCKED_EMERGENCY_RECOVERY` | Recovery Delay | Emergency fund withdrawal initiated | Emergency withdrawal execution | `EmergencyWithdrawNotReady` (103) | 7 Days after unlock timestamp | Verify token surplus vs escrow reserves, wait for 24h timelock (`unlock_at`), call `execute_emergency_withdraw` |
| `ADMIN_HANDOVER_PENDING` | Governance | Admin rotation or key compromise handover initiated | Admin-only operations during handover | `NotAdmin` (1001) | N/A (Pending acceptance) | Verify `new_admin` identity, complete 2-step handover via `accept_admin(new_admin)` |

---

## 2. Detailed Reason Breakdown & Response Workflows

### 1. `EMERGENCY_CIRCUIT_BREAKER` (Hard Pause)

- **Description**: Activated by an authorized admin via `PauseControl::set_paused(env, admin, true)` during active attacks or critical logic flaws.
- **Guarded Entrypoints**:
  - `store_invoice`
  - `verify_invoice`
  - `place_bid`
  - `accept_bid`
  - `verify_business`
  - `verify_investor`
  - `create_dispute`
  - `resolve_dispute`
- **Telemetry Event**: Emits `PauseBlocked` containing `(entrypoint, caller, ledger_ts)` when a blocked call is rejected.
- **Auto-Expiry**: Enforces `MAX_PAUSE_DURATION = 604800` seconds (7 days). If unpause is not called within 7 days, `PauseControl::is_paused` evaluates to `false` to avoid permanent contract lock-up.

#### Operational Recovery Steps
```rust
use soroban_sdk::{Address, Env};
use quicklendx_contracts::pause::PauseControl;
use quicklendx_contracts::errors::QuickLendXError;

/// Step 1: Engage hard pause
pub fn pause_protocol(env: &Env, admin: &Address) -> Result<(), QuickLendXError> {
    PauseControl::set_paused(env, admin, true)
}

/// Step 2: Unpause protocol after patch verification
pub fn unpause_protocol(env: &Env, admin: &Address) -> Result<(), QuickLendXError> {
    PauseControl::set_paused(env, admin, false)
}
```

---

### 2. `SCHEDULED_MAINTENANCE` (Soft Maintenance Mode)

- **Description**: Activated via `MaintenanceControl::set_maintenance_mode(env, admin, true, reason)` for routine operations.
- **Behavior**:
  - Read-only queries remain 100% accessible to frontend/indexers.
  - Write attempts return `QuickLendXError::MaintenanceModeActive` with a human-readable banner reason.
  - TTL extension sweeps (`extend_protocol_ttl`) remain operational.
- **Reason String Limit**: Maximum 256 bytes (`MAX_REASON_LEN`).

#### Maintenance Operational Code Example
```rust
use soroban_sdk::{Address, Env, String};
use quicklendx_contracts::maintenance::MaintenanceControl;
use quicklendx_contracts::errors::QuickLendXError;

/// Enable maintenance mode with descriptive reason.
pub fn start_maintenance(env: &Env, admin: &Address) -> Result<(), QuickLendXError> {
    let reason = String::from_str(env, "Database migration v2.4 in progress. Read queries active.");
    MaintenanceControl::set_maintenance_mode(env, admin, true, &reason)
}

/// Disable maintenance mode when done.
pub fn end_maintenance(env: &Env, admin: &Address) -> Result<(), QuickLendXError> {
    let empty_reason = String::from_str(env, "");
    MaintenanceControl::set_maintenance_mode(env, admin, false, &empty_reason)
}
```

---

### 3. `COORDINATED_INCIDENT_ISOLATION` (Coordinated Incident Mode)

- **Description**: Activated via `IncidentControl::enter_incident_mode(env, admin, reason)`. Atomically engages both Hard Pause and Soft Maintenance Mode in a single transaction.
- **Snapshot Audit**: Returns an `IncidentSnapshot` struct recording `is_paused`, `is_maintenance`, `reason`, and `timestamp`.

#### Coordinated Incident Code Example
```rust
use soroban_sdk::{Address, Env, String};
use quicklendx_contracts::incident::{IncidentControl, IncidentSnapshot};
use quicklendx_contracts::errors::QuickLendXError;

/// Engage incident mode atomically.
pub fn engage_incident(env: &Env, admin: &Address) -> Result<IncidentSnapshot, QuickLendXError> {
    let reason = String::from_str(env, "CRITICAL: Investigating suspicious dispute resolution payload");
    IncidentControl::enter_incident_mode(env, admin, &reason)
}

/// Clear incident mode atomically.
pub fn clear_incident(env: &Env, admin: &Address) -> Result<IncidentSnapshot, QuickLendXError> {
    IncidentControl::exit_incident_mode(env, admin)
}
```

---

### 4. `TIMELOCKED_EMERGENCY_RECOVERY` (Emergency Withdrawal Standstill)

- **Description**: Occurs during emergency token rescue via `emergency.rs`.
- **Invariants**:
  - Minimum 24-hour timelock (`DEFAULT_EMERGENCY_TIMELOCK_SECS`).
  - 7-day expiration window after `unlock_at`.
  - Non-escrow surplus check (`require_withdrawable_surplus` protects active escrow reserves).

---

## 3. Operator Support & Decision Tree

```
                      Is there a threat to funds or contract state?
                                     │
                    ┌────────────────┴────────────────┐
                   YES                                NO
                    │                                 │
     Is the issue localized or global?      Is this routine maintenance?
        ┌───────────┴───────────┐                     │
      LOCAL                   GLOBAL                  ▼
        │                       │             Enable Maintenance Mode
   Pause specific       Call Incident Mode        (set_maintenance_mode)
   entrypoint or      (enter_incident_mode)
   create dispute
```
