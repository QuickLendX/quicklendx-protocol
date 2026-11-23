# QuickLendX Protocol — Contract Emergency Response Runbook

> [!IMPORTANT]
> **Target Audience:** Protocol Operators, Incident Responders, and Support Engineers.
> This runbook documents common emergency triggers, operational invariants, and canonical step-by-step response procedures per smart contract module in the QuickLendX protocol.

---

## Document Overview & Audience

This runbook serves as the canonical reference for handling emergencies on the QuickLendX smart contracts on Stellar Soroban. 
When an incident occurs:
1. **Assess the scope** (contract component, severity, affected funds/users).
2. **Execute the canonical response** (using Soroban CLI or administrative contract invocations).
3. **Verify post-incident state** via read-only health and snapshot entrypoints.

---

## Summary of Emergency Entrypoints & Circuit Breakers

| Contract Module | Key Entrypoints | Primary Triggers | Action & Scope | Timelock / Expiry |
| :--- | :--- | :--- | :--- | :--- |
| **Coordinated Incident** | `enter_incident_mode`, `exit_incident_mode` | Severe vulnerability, multi-component exploit | Hard-pauses all writes and sets maintenance banner atomically | Immediate execution |
| **Protocol Pause** | `set_paused`, `require_not_paused` | Unexpected state mutation, reentrancy bug | Blocks state-mutating entrypoints (`store_invoice`, `place_bid`, etc.) | Auto-expires after 7 days |
| **Maintenance Mode** | `set_maintenance_mode`, `extend_protocol_ttl` | Database indexing backfill, routine migration | Read-only mode: mutating ops return `MaintenanceModeActive` | Operator controlled |
| **Emergency Withdraw** | `initiate`, `execute`, `cancel` | Stuck non-escrow tokens, balance surplus | Withdraws excess contract tokens to specified target address | 24h timelock (min 1h, max 30d); 7-day expiration |
| **Dispute Protection** | `create_dispute`, `resolve_dispute` | Fraudulent invoice, double-financing attempt | Freezes escrow funds for targeted invoice until resolution | Admin/Arbitrator governed |
| **Admin Handover** | `propose_admin`, `accept_admin`, `commit_admin_handover` | Admin key rotation, key compromise response | Transfers contract super-admin privileges (2-step or 1-step) | Immediate upon acceptance |

---

## 1. Coordinated Incident Mode (`incident.rs`)

### Common Triggers
- Active or suspected smart contract exploit affecting multiple workflow steps.
- Security vulnerability reported via audit/bug bounty requiring instant global isolation.
- Discrepancies between off-chain ledger indexes and on-chain escrow balances.

### Canonical Response Procedure
1. **Engage Incident Mode Immediately**:
   Call `enter_incident_mode` with the authenticated admin key and a clear reason string (max 256 bytes). This atomically sets both `paused = true` and `maintenance = true`.
2. **Inspect Incident Snapshot**:
   Verify the returned `IncidentSnapshot` struct to ensure both flags are active and the reason timestamp is recorded.
3. **Investigate & Remediate**:
   With protocol writes fully blocked, conduct forensic analysis or deploy updated contract WASM.
4. **Exit Incident Mode**:
   Call `exit_incident_mode` to atomically clear both pause and maintenance flags.

### Soroban SDK Code Example
```rust
use soroban_sdk::{Address, Env, String};
use quicklendx_contracts::incident::{IncidentControl, IncidentSnapshot};

/// Example: Operator engaging incident mode during an active audit alert.
pub fn trigger_emergency_incident(env: &Env, admin: &Address) -> Result<IncidentSnapshot, quicklendx_contracts::errors::QuickLendXError> {
    let reason = String::from_str(env, "CRITICAL: Investigating potential reentrancy report in bid acceptance");
    // Atomically sets hard pause + maintenance mode
    let snapshot = IncidentControl::enter_incident_mode(env, admin, &reason)?;
    
    // Invariant Check
    assert!(snapshot.is_paused);
    assert!(snapshot.is_maintenance);
    
    Ok(snapshot)
}

/// Example: Operator resolving incident mode after patch deployment.
pub fn resolve_emergency_incident(env: &Env, admin: &Address) -> Result<IncidentSnapshot, quicklendx_contracts::errors::QuickLendXError> {
    let snapshot = IncidentControl::exit_incident_mode(env, admin)?;
    
    assert!(!snapshot.is_paused);
    assert!(!snapshot.is_maintenance);
    
    Ok(snapshot)
}
```

---

## 2. Circuit Breaker & Hard Pause (`pause.rs`)

### Common Triggers
- Critical bug isolated to invoice verification, bidding, or dispute resolution.
- Price feed or oracle freshness delay exceeding protocol bounds.

### Operational Guarded Entrypoints
When `paused = true`, the following state-mutating entrypoints immediately revert with `QuickLendXError::ContractPaused`:
- `store_invoice`
- `verify_invoice`
- `place_bid`
- `accept_bid`
- `verify_business`
- `verify_investor`
- `create_dispute`
- `resolve_dispute`

> [!NOTE]
> Read-only query functions (such as `get_invoice`, `get_bid`, `health_check`) remain accessible during a hard pause to allow indexers and frontend apps to render state.

### Auto-Expiry Protection
- **Max Duration**: 7 days (`MAX_PAUSE_DURATION = 604800` seconds).
- If operators do not explicitly unpause within 7 days, `PauseControl::is_paused` automatically evaluates to `false` to prevent permanent protocol bricking.

### Canonical Response Procedure
1. Call `PauseControl::set_paused(env, admin, true)` to pause.
2. Monitor log event `emit_paused(env, admin)`.
3. Perform repairs or parameter updates.
4. Call `PauseControl::set_paused(env, admin, false)` to resume.

---

## 3. Soft Maintenance Mode (`maintenance.rs`)

### Common Triggers
- Scheduled backend database migration or indexer gap backfilling.
- Expiration of ledger storage persistent TTLs requiring maintenance sweep.

### Operational Behavior
- Mutating operations calling `require_write_allowed` return `QuickLendXError::MaintenanceModeActive`.
- Read queries remain fully operational.
- Admin TTL maintenance functions (`extend_protocol_ttl`) remain executable.

### TTL Maintenance Procedure
Operators run `extend_protocol_ttl` during maintenance windows to bump ledger TTL for all invoices, bids, investments, escrows, and whitelisted currencies:

```rust
use soroban_sdk::{Address, Env};
use quicklendx_contracts::maintenance::{MaintenanceControl, ExtendReport};

/// Bumps persistent storage TTL across all protocol records.
pub fn Maintenance_bump_ttl(env: &Env, admin: &Address) -> Result<ExtendReport, quicklendx_contracts::errors::QuickLendXError> {
    let report: ExtendReport = MaintenanceControl::extend_protocol_ttl(env, admin)?;
    // report.invoices_refreshed, report.bids_refreshed, etc.
    Ok(report)
}
```

---

## 4. Emergency Timelocked Fund Recovery (`emergency.rs`)

### Common Triggers
- ERC20/SAC tokens inadvertently transferred directly to the contract address.
- Yield or fee accumulation surplus needing rescue.

### Safety Invariants & Rules
1. **Escrow Protection Reserve**: Emergency withdrawals **CANNOT** touch funds reserved for active, completed escrows (`require_withdrawable_surplus`).
2. **Timelock Requirement**: Minimum 24-hour delay (`DEFAULT_EMERGENCY_TIMELOCK_SECS`) between initiation and execution.
3. **Expiration Window**: Standard 7-day expiration (`DEFAULT_EMERGENCY_EXPIRATION_SECS`) after the timelock unlocks. Unexecuted requests expire automatically.
4. **Single Slot & Nonce Tracking**: Global incrementing nonce invalidates cancelled or stale withdrawal attempts.

### Entrypoint Parameter Reference

#### 1. `initiate`
- `admin`: Operator/Admin `Address` (requires auth).
- `token`: Contract `Address` of token to withdraw (cannot be contract's own address).
- `amount`: `i128` amount (must be > 0).
- `target`: Recipient `Address` (cannot be contract's own address).

#### 2. `execute`
- `admin`: Admin `Address` executing the withdrawal after `unlock_at`.

#### 3. `cancel`
- `admin`: Admin `Address` cancelling the pending withdrawal request.

### Soroban SDK Code Example
```rust
use soroban_sdk::{Address, Env};
use quicklendx_contracts::emergency::EmergencyWithdraw;
use quicklendx_contracts::errors::QuickLendXError;

/// Step 1: Initiate emergency withdrawal (starts 24h timelock).
pub fn initiate_rescue(
    env: &Env,
    admin: &Address,
    token: Address,
    amount: i128,
    target: Address,
) -> Result<(), QuickLendXError> {
    EmergencyWithdraw::initiate(env, admin, token, amount, target)
}

/// Step 2: Execute emergency withdrawal (after unlock_at timestamp).
pub fn execute_rescue(env: &Env, admin: &Address) -> Result<(), QuickLendXError> {
    EmergencyWithdraw::execute(env, admin)
}

/// Optional: Cancel emergency withdrawal prior to execution.
pub fn cancel_rescue(env: &Env, admin: &Address) -> Result<(), QuickLendXError> {
    EmergencyWithdraw::cancel(env, admin)
}
```

---

## 5. Dispute Escalation & Escrow Protection (`dispute.rs`)

### Common Triggers
- Investor or Business reports invoice default or double-factoring fraud.
- Discrepancy in invoice fulfillment proof.

### Canonical Response Workflow
1. **Initiate Dispute**: Caller invokes `create_dispute(env, reporter, invoice_id, reason)`. Escrow state is transitioned to locked.
2. **Audit Evidence**: Operator inspects on-chain event logs and dispute timeline.
3. **Resolve Dispute**: Admin issues `resolve_dispute`:
   - `Refund`: Reverts funds back to investor.
   - `Forfeit`: Disburses funds according to contract rules.
   - `Release`: Finalizes settlement to business.

---

## 6. Emergency Admin Key Rotation & Governance (`governance.rs`)

### Common Triggers
- Admin cold-wallet migration.
- Suspected compromise of active admin key requiring immediate handover.

### Handover Modes
- **Two-Step Handover (Recommended)**:
  1. Current Admin calls `propose_admin(env, admin, new_admin)`.
  2. New Admin calls `accept_admin(env, new_admin)`.
- **One-Step Handover**:
  - Current Admin calls `commit_admin_handover(env, admin, new_admin)`.

---

## Quick Reference: Error Codes & Meanings

| Error Symbol | Numeric Code | Cause & Canonical Action |
| :--- | :--- | :--- |
| `ContractPaused` | `101` | Protocol is hard-paused. Check incident logs; unpause when safe. |
| `MaintenanceModeActive` | `102` | Protocol is in read-only maintenance. Wait for maintenance window completion. |
| `EmergencyWithdrawNotReady` | `103` | Timelock has not passed (`now < unlock_at`). Wait until unlock timestamp. |
| `EmergencyWithdrawExpired` | `104` | Timelock expired (`now > expires_at`). Re-initiate withdrawal request. |
| `EmergencyWithdrawCancelled` | `105` | Withdrawal nonce was cancelled. Re-initiate if needed. |
| `EmergencyWithdrawInsufficientBalance` | `106` | Requested amount exceeds surplus balance (escrow reserves are protected). Adjust amount. |
| `NotAdmin` | `1001` | Caller is not the designated admin address. Verify signing key. |

---

## Support & On-Call Escalation Matrix

If an issue cannot be resolved using standard runbook entrypoints:
1. Engage **Coordinated Incident Mode** (`enter_incident_mode`) to halt protocol state mutations.
2. Notify core contract maintainers and security auditors.
3. Reference `docs/RUNBOOK_INCIDENT_RESPONSE.md` for high-level incident command structure.
