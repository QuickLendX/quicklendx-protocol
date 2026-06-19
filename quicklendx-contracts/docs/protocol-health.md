# Protocol Health Endpoint

## Overview

The **Protocol Health** endpoint provides a canonical snapshot of the QuickLendX protocol's current state through a single struct-based API. This is designed as the **heartbeat endpoint for off-chain dashboards, monitoring systems, and governance tooling**.

Instead of calling a dozen separate getters (`get_total_invoice_count`, `get_treasury`, `get_fee_bps`, `is_paused`, `get_pending_emergency_withdraw`, etc.), integrators can now call:

```rust
let health = get_protocol_health(env);
```

And receive a single `ProtocolHealth` struct with all critical information.

## API Reference

### `get_protocol_health(env: Env) -> ProtocolHealth`

**Purpose**: Return a fresh snapshot of protocol health status.

**Security Model**:
- ✓ **Read-only**: No state mutations
- ✓ **No authentication**: Any caller can invoke
- ✓ **Pause-exempt**: Available even when protocol is paused
- ✓ **Safe**: Contains only aggregate counts and system configuration (no PII or per-user data)

**Returns**: A `ProtocolHealth` struct containing 8 fields (see below).

---

## ProtocolHealth Struct

```rust
pub struct ProtocolHealth {
    pub version: u32,                                              // Protocol version
    pub initialized: bool,                                        // Initialization flag
    pub paused: bool,                                             // Pause status
    pub emergency_withdraw_pending: Option<PendingEmergencyWithdrawal>, // Emergency withdrawal state
    pub treasury: Option<Address>,                                // Treasury address
    pub fee_bps: u32,                                             // Fee basis points (0-1000)
    pub total_invoice_count: u32,                                 // Total invoices
    pub currency_count: u32,                                      // Whitelisted currencies
}
```

### Field Documentation

#### `version: u32`
- **Description**: Protocol version number written at initialization time.
- **Semantics**: The value of `PROTOCOL_VERSION` constant from `src/init.rs` at the time the contract was deployed.
- **Range**: Currently `1` (will increment on major version changes).
- **Stability**: Immutable after initialization.

#### `initialized: bool`
- **Description**: Whether the protocol has completed its one-time initialization.
- **Semantics**:
  - `true` = protocol is operational and ready for business transactions
  - `false` = awaiting `initialize()` call; most operations will be blocked
- **Mutability**: Transitions from `false` → `true` exactly once, then immutable.

#### `paused: bool`
- **Description**: Current pause state of the protocol.
- **Semantics**:
  - `true` = emergency incident mode; most mutating operations are blocked
  - `false` = normal operation; all flows available
- **Mutability**: Can be toggled by admin via `pause()` and `unpause()`.
- **Note**: This field is advisory only. Off-chain systems should respect it but understand that state may change between reading this and executing a transaction.

#### `emergency_withdraw_pending: Option<PendingEmergencyWithdrawal>`
- **Description**: Optional details of a pending emergency withdrawal (if any).
- **Semantics**:
  - `None` = no emergency withdrawal in progress
  - `Some(pending)` = an admin has initiated a timelocked withdrawal
- **Fields (if Some)**:
  ```rust
  pub struct PendingEmergencyWithdrawal {
      pub token: Address,          // Token to withdraw
      pub amount: i128,            // Amount to withdraw
      pub target: Address,         // Recipient address
      pub unlock_at: u64,          // Ledger timestamp when unlock time has passed
      pub expires_at: u64,         // Ledger timestamp when withdrawal expires
      pub initiated_at: u64,       // Ledger timestamp when initiated
      pub initiated_by: Address,   // Admin who initiated
      pub nonce: u64,              // Unique nonce for this withdrawal
      pub cancelled: bool,         // Whether the withdrawal was cancelled
      pub cancelled_at: u64,       // Timestamp of cancellation (if any)
  }
  ```
- **Usage**: Off-chain systems can use `unlock_at` and `expires_at` to display countdown timers or alert dashboards to an incident in progress.

#### `treasury: Option<Address>`
- **Description**: The configured treasury address for fee collection.
- **Semantics**:
  - `None` = no treasury configured; fees are calculated but not transferred
  - `Some(addr)` = address where fees are collected on settlement
- **Mutability**: Can be set/updated by admin via `set_treasury()`.
- **Note**: Fee calculations proceed regardless of whether treasury is set; it's not a blocker.

#### `fee_bps: u32`
- **Description**: Fee basis points applied to profit settlements.
- **Range**: 0–1000 (0% to 10%)
- **Semantics**: When an investment settles with a profit, the investor receives `(profit * (10000 - fee_bps)) / 10000`.
- **Example**: `fee_bps = 200` → 2% fee retained by protocol
- **Mutability**: Can be updated by admin via `set_fee_config()`.
- **Validation**: Calls to update this field will fail if the new value exceeds 1000.

#### `total_invoice_count: u32`
- **Description**: Sum of all invoices across all statuses.
- **Calculation**: `Pending + Verified + Funded + Paid + Defaulted + Cancelled + Refunded`
- **Semantics**: A holistic measure of protocol activity and invoice churn.
- **Note**: Does not distinguish by status; use `get_invoice_count_by_status()` for per-status breakdowns.
- **Range**: 0 to 2^32 - 1

#### `currency_count: u32`
- **Description**: Number of whitelisted currencies (token contracts).
- **Semantics**: Operations require at least one whitelisted currency. If count is 0, no invoices or payments can be accepted.
- **Mutability**: Increases when admin calls `add_currency()`.
- **Note**: Currently, currencies cannot be removed (permanent whitelist model).

---

## Usage Patterns

### Real-Time Dashboard Heartbeat

```rust
let health = get_protocol_health(env);

if !health.initialized {
    display_banner("Protocol not yet initialized");
    return;
}

if health.paused {
    display_alert("PROTOCOL PAUSED - Emergency incident mode active");
}

display_metrics(
    "Version: {}",
    "Status: {}",
    "Invoices: {}",
    "Currencies: {}",
    "Fee: {} bps",
    health.version,
    if health.paused { "PAUSED" } else { "RUNNING" },
    health.total_invoice_count,
    health.currency_count,
    health.fee_bps,
);

if let Some(pending) = health.emergency_withdraw_pending {
    display_incident(
        "Emergency withdraw pending\n\
         Token: {}\n\
         Amount: {}\n\
         Unlock at: {}\n\
         Expires at: {}",
        pending.token,
        pending.amount,
        pending.unlock_at,
        pending.expires_at,
    );
}
```

### Monitoring & Alerting

```rust
fn check_protocol_health(env: Env) -> HealthCheckResult {
    let health = get_protocol_health(env);

    // Alert if protocol is paused
    if health.paused {
        alert!("Protocol entered incident mode");
    }

    // Alert if treasury is not configured
    if health.treasury.is_none() {
        warn!("Treasury address not set; fee collection disabled");
    }

    // Alert if no currencies whitelisted
    if health.currency_count == 0 {
        alert!("No currencies whitelisted; operations blocked");
    }

    // Alert if emergency withdrawal is pending
    if health.emergency_withdraw_pending.is_some() {
        alert!("Emergency withdrawal in progress");
    }

    HealthCheckResult {
        initialized: health.initialized,
        paused: health.paused,
        invoices: health.total_invoice_count,
        currencies: health.currency_count,
    }
}
```

### Configuration Audit

```rust
fn audit_protocol_config(env: Env) {
    let health = get_protocol_health(env);

    println!("=== Protocol Configuration ===");
    println!("Version:        {}", health.version);
    println!("Initialized:    {}", health.initialized);
    println!("Paused:         {}", health.paused);
    println!("Treasury:       {:?}", health.treasury);
    println!("Fee (bps):      {}", health.fee_bps);
    println!("Total Invoices: {}", health.total_invoice_count);
    println!("Currencies:     {}", health.currency_count);
}
```

---

## Security Considerations

### Data Confidentiality

✓ **No PII or user data**: The endpoint returns only aggregate counts and system configuration.

✓ **No secret leakage**: Internal contract state, investment details, or payment history are not exposed.

✓ **Safe for public APIs**: Off-chain services can expose this endpoint to third parties without risk.

### Consistency & Freshness

⚠ **Advisory snapshot**: The returned data reflects the state at the moment of reading. Subsequent transactions may change protocol state before callers can react.

**Do not rely on this endpoint for critical security decisions.** Use it for:
- Dashboard display (informational)
- Monitoring thresholds (alerting)
- Governance transparency (audit trails)
- But NOT for:
  - Access control (use `require_auth()` and explicit checks instead)
  - Financial calculations (query specific balances and escrows directly)

### Read-Only Guarantee

✓ **Zero state mutations**: Calling `get_protocol_health()` multiple times in sequence does not change any protocol state.

✓ **No reentrancy risk**: This endpoint cannot trigger downstream calls that might mutate state.

✓ **Pause-exempt**: Remains available even during incident mode, ensuring off-chain systems stay informed.

---

## Testing

The protocol health endpoint is covered by comprehensive tests in [`src/test_protocol_health.rs`](../quicklendx-contracts/src/test_protocol_health.rs):

- ✓ **Uninitialized state**: All fields return defaults
- ✓ **Initialized state**: All fields populated correctly
- ✓ **Pause transitions**: Paused flag reflects state changes
- ✓ **Fee updates**: `fee_bps` reflects admin configuration changes
- ✓ **Treasury updates**: `treasury` field changes after `set_treasury()`
- ✓ **Currency count**: Increases as admin adds currencies
- ✓ **Invoice count**: Tracks total invoices (base coverage; full integration tested elsewhere)
- ✓ **Emergency withdrawal**: `emergency_withdraw_pending` field populated correctly
- ✓ **Read-only guarantee**: Multiple calls produce consistent results
- ✓ **No side effects**: Calling endpoint does not mutate admin, pause, or fee state
- ✓ **Full workflow**: Complex multi-step scenarios (add currencies, pause, update fees, etc.)

**Test coverage**: Minimum 95% of health.rs and test_protocol_health.rs.

Run tests:
```bash
cd quicklendx-contracts
cargo test test_protocol_health -- --nocapture
```

---

## Integration Examples

### With Web Dashboard (JavaScript)

```javascript
async function displayProtocolHealth() {
  const health = await contract.methods.get_protocol_health().simulate();

  document.getElementById("version").textContent = health.version;
  document.getElementById("initialized").textContent = health.initialized ? "Yes" : "No";
  document.getElementById("paused").textContent = health.paused ? "PAUSED" : "Running";
  document.getElementById("invoices").textContent = health.total_invoice_count;
  document.getElementById("currencies").textContent = health.currency_count;
  document.getElementById("fee").textContent = `${health.fee_bps / 100}%`;
  document.getElementById("treasury").textContent = health.treasury ?? "Not configured";

  if (health.emergency_withdraw_pending) {
    showEmergencyAlert(health.emergency_withdraw_pending);
  }
}

// Poll every 5 seconds
setInterval(displayProtocolHealth, 5000);
```

### With Governance Proposal Simulator

```rust
fn preview_governance_impact(env: Env, proposed_fee_bps: u32) {
    let health_before = get_protocol_health(env);

    // Display impact summary without executing the change
    println!("Current fee:  {} bps", health_before.fee_bps);
    println!("Proposed fee: {} bps", proposed_fee_bps);
    println!("Impact: {} bps change", proposed_fee_bps as i64 - health_before.fee_bps as i64);
    
    if health_before.paused {
        println!("⚠️  Protocol is paused; fee update blocked");
    }
}
```

---

## Migration Guide

### For Operators Moving from Individual Getters

**Before** (multiple calls):
```rust
let is_init = is_initialized(env);
let is_paused = is_paused(env);
let fee = get_fee_bps(env);
let treasury = get_treasury(env);
let invoice_count = get_total_invoice_count(env);
let currency_count = currency_count(env);
let emergency = get_pending_emergency_withdraw(env);
let version = get_version(env);
```

**After** (single call):
```rust
let health = get_protocol_health(env);
// All fields available: health.version, health.initialized, health.paused, etc.
```

**Benefits**:
- ✓ Reduced RPC round trips (1 call instead of 8)
- ✓ Atomic snapshot (all fields consistent at a single moment in time)
- ✓ Simpler client code
- ✓ Better for real-time dashboards

---

## Changelog

### v1.0 (Current)
- Initial release of protocol health endpoint
- 8 fields: version, initialized, paused, emergency_withdraw_pending, treasury, fee_bps, total_invoice_count, currency_count
- 25+ test cases covering all scenarios and edge cases
- Zero-risk integration (read-only, no auth required)

---

## FAQ

**Q: Can I use this endpoint to decide whether to execute a critical transaction?**  
A: No. Use explicit validation functions (`require_not_paused()`, `require_initialized()`, etc.). This endpoint is advisory only.

**Q: What if the protocol is paused?**  
A: This endpoint remains available. You can use it to detect pause state and inform users.

**Q: Does this endpoint charge fees?**  
A: No. All reads from contract state are fee-free on Soroban.

**Q: Is the emergency_withdraw_pending field ever populated after initialization?**  
A: Only if an admin actively initiates an emergency withdrawal via `initiate_emergency_withdraw()`. Under normal operation, it will always be `None`.

**Q: Can I cache the result?**  
A: Not safely. State can change on every block. If you need a fresh snapshot, call the endpoint again. For dashboards, a 5-10 second cache is reasonable.

---

## References

- Implementation: [`src/health.rs`](../quicklendx-contracts/src/health.rs)
- Tests: [`src/test_protocol_health.rs`](../quicklendx-contracts/src/test_protocol_health.rs)
- Integration: [`src/lib.rs`](../quicklendx-contracts/src/lib.rs) (exposed as `get_protocol_health()` in `#[contractimpl]` block)
- Related modules:
  - [`src/init.rs`](../quicklendx-contracts/src/init.rs) — Version, initialization, fees, treasury
  - [`src/pause.rs`](../quicklendx-contracts/src/pause.rs) — Pause state
  - [`src/emergency.rs`](../quicklendx-contracts/src/emergency.rs) — Emergency withdrawals
  - [`src/invoice.rs`](../quicklendx-contracts/src/invoice.rs) — Invoice counts
  - [`src/currency.rs`](../quicklendx-contracts/src/currency.rs) — Currency whitelist
