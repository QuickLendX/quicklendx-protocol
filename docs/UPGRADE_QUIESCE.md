# Upgrade Quiescence Policy

**Audience: operators** — this document describes the write-drain procedure that operators must follow before replacing the contract WASM on a live deployment. Contributors adding entrypoints should read "What requires a quiescence window?" below.

---

## Why quiescence matters

Soroban upgrades are atomic at the network level: the contract ID stays the same, the WASM hash flips, and all persistent/instance storage is preserved. If a write transaction is in flight when the WASM hash changes, one of two things happens:

| Outcome | Condition |
|---------|-----------|
| **Success (old WASM)** | Transaction was included in a ledger *before* the upgrade tx. State transition uses old code. |
| **Failure (new WASM)** | Transaction is included *after* the upgrade tx. New WASM executes; if the entrypoint signature or storage layout changed, the transaction fails with a deserialization or host error. |

The quiescence window guarantees **no in-flight writes** cross the upgrade boundary, so operators observe a clean cut-over.

---

## Drain procedure (canonical)

### 1. Announce maintenance window

Post the planned upgrade ledger (or wall-clock time) in the operator channel. Example:

```
[UPGRADE] v1.4.0 → v1.5.0 scheduled for ledger 12,345,678 (approx 2026-07-25 14:00 UTC).
Maintenance mode will be enabled at ledger 12,345,600 (≈ 78 ledgers / 5 min before).
```

### 2. Enable maintenance mode (drain start)

Call `set_maintenance_mode` with a reason that includes the target ledger:

```bash
stellar contract invoke \
  --id $CONTRACT_ID \
  --source $ADMIN_SECRET \
  --network $NETWORK \
  -- set_maintenance_mode \
  --admin $ADMIN_ADDRESS \
  --enabled true \
  --reason "Upgrade to v1.5.0 at ledger 12345678"
```

**Contract behavior** (see `src/maintenance.rs:122`):

```rust
pub fn require_write_allowed(env: &Env) -> Result<(), QuickLendXError> {
    if Self::is_maintenance_mode(env) {
        Err(QuickLendXError::MaintenanceModeActive)
    } else {
        Ok(())
    }
}
```

Every state-mutating entrypoint calls `require_write_allowed()` at the top. Once maintenance mode is on, **all user writes return `MaintenanceModeActive` (error 102)**. Reads remain available.

### 3. Verify write drain

Wait for the configured drain window (default **5 minutes ≈ 78 ledgers**). Confirm:

```bash
# 1. Health endpoint shows writes blocked
stellar contract invoke --id $CONTRACT_ID -- get_health_status
# { is_paused: false, is_maintenance: true, writes_allowed: false, ... }

# 2. No recent write events in the ledger range
stellar contract events --id $CONTRACT_ID --start-ledger 12345600 --end-ledger 12345678
# Should show only MAINT enabled, no user entrypoints
```

### 4. Replace WASM

```bash
# Install new WASM
WASM_HASH=$(soroban contract install \
  --wasm target/wasm32-unknown-unknown/release/quicklendx_contracts.wasm \
  --rpc-url $RPC_URL --network-passphrase "$NETWORK_PASSPHRASE" --source $ADMIN_SECRET)

# Upgrade contract
soroban contract invoke \
  --id $CONTRACT_ID --source $ADMIN_SECRET \
  --rpc-url $RPC_URL --network-passphrase "$NETWORK_PASSPHRASE" \
  -- upgrade_contract_wasm --new_wasm_hash "$WASM_HASH"
```

### 5. Disable maintenance mode (drain end)

```bash
stellar contract invoke \
  --id $CONTRACT_ID --source $ADMIN_SECRET \
  --network $NETWORK \
  -- set_maintenance_mode \
  --admin $ADMIN_ADDRESS --enabled false --reason "Upgrade complete"
```

### 6. Smoke test

```bash
# Read-only
stellar contract invoke --id $CONTRACT_ID -- get_version
# Write (low-value)
stellar contract invoke --id $CONTRACT_ID --source $TEST_USER \
  -- store_invoice ...minimal valid args...
```

---

## Drain window sizing

| Network | Ledger time | Recommended window | Ledger count |
|---------|-------------|-------------------|--------------|
| Mainnet | ~5 s | 5 min | 60 |
| Testnet | ~5 s | 3 min | 36 |
| Futurenet | ~5 s | 2 min | 24 |

**Rationale**: A 5-minute window covers:
- Maximum transaction submission latency (wallet → RPC → inclusion)
- One ledger where the upgrade tx itself is included
- Buffer for clock skew between operator and network

Operators **may extend** the window for high-value deployments; never shorten below 2 minutes.

---

## What requires a quiescence window?

| Change type | Quiescence required? | Reason |
|-------------|---------------------|--------|
| WASM-only patch (bug fix, gas optimization) | **No** | No storage layout or entrypoint signature change. |
| New entrypoint added | **No** | Old WASM ignores unknown calls; new WASM handles them. |
| Entrypoint signature changed | **Yes** | In-flight tx with old args fails on new WASM. |
| Storage key renamed / struct layout changed | **Yes** | Migration runs *after* WASM swap; old writes corrupt new layout. |
| `PROTOCOL_VERSION` bump | **Yes** | Implies storage migration (see `UPGRADE_PATHS.md`). |

> **Rule of thumb**: if the change appears in the "Migration required" table of `docs/UPGRADE_PATHS.md`, it needs quiescence.

---

## Operator checklist

- [ ] Upgrade announcement posted with target ledger
- [ ] Maintenance mode enabled with reason containing target ledger
- [ ] `get_health_status().writes_allowed == false` confirmed
- [ ] No user write events observed for ≥ drain window
- [ ] New WASM installed and hash recorded
- [ ] `upgrade_contract_wasm` invoked
- [ ] Maintenance mode disabled
- [ ] `get_health_status().writes_allowed == true` confirmed
- [ ] Smoke test (read + write) passes
- [ ] Post-upgrade backup created (`create_backup`)
- [ ] Backup validated (`validate_backup`)

---

## Cross-references

- **Maintenance mode implementation**: `src/maintenance.rs`, `docs/contracts/operations.md`
- **Health status endpoint**: `src/monitor.rs`, `docs/QUERIES.md#get_health_status`
- **Upgrade procedure & storage migration**: `docs/UPGRADE_PATHS.md`
- **Incident mode (hard pause + maintenance)**: `docs/RUNBOOK_INCIDENT_RESPONSE.md`, `src/incident.rs`
- **Error codes**: `docs/ERROR_CODES.md` (search `MaintenanceModeActive`)

---

## Example: end-to-end upgrade runbook

```bash
#!/usr/bin/env bash
# upgrade-quiesce.sh — run as operator with ADMIN_SECRET in env

set -euo pipefail

CONTRACT_ID="CXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX"
RPC_URL="https://soroban-mainnet.stellar.org"
NETWORK_PASSPHRASE="Public Global Stellar Network ; September 2015"
TARGET_LEDGER=12345678
DRAIN_LEDGER=$((TARGET_LEDGER - 78))  # 5 min before

echo "=== 1. Enable maintenance mode at ledger $DRAIN_LEDGER ==="
soroban contract invoke \
  --id "$CONTRACT_ID" --source "$ADMIN_SECRET" \
  --rpc-url "$RPC_URL" --network-passphrase "$NETWORK_PASSPHRASE" \
  -- set_maintenance_mode \
  --admin "$ADMIN_ADDRESS" \
  --enabled true \
  --reason "Upgrade v1.5.0 at ledger $TARGET_LEDGER"

echo "=== 2. Wait for drain window (78 ledgers) ==="
# In production, poll get_health_status until ledger >= TARGET_LEDGER
sleep 300  # 5 min

echo "=== 3. Verify writes blocked ==="
soroban contract invoke --id "$CONTRACT_ID" --rpc-url "$RPC_URL" \
  --network-passphrase "$NETWORK_PASSPHRASE" -- get_health_status

echo "=== 4. Install & upgrade WASM ==="
WASM_HASH=$(soroban contract install \
  --wasm target/wasm32-unknown-unknown/release/quicklendx_contracts.wasm \
  --rpc-url "$RPC_URL" --network-passphrase "$NETWORK_PASSPHRASE" \
  --source "$ADMIN_SECRET")

soroban contract invoke \
  --id "$CONTRACT_ID" --source "$ADMIN_SECRET" \
  --rpc-url "$RPC_URL" --network-passphrase "$NETWORK_PASSPHRASE" \
  -- upgrade_contract_wasm --new_wasm_hash "$WASM_HASH"

echo "=== 5. Disable maintenance mode ==="
soroban contract invoke \
  --id "$CONTRACT_ID" --source "$ADMIN_SECRET" \
  --rpc-url "$RPC_URL" --network-passphrase "$NETWORK_PASSPHRASE" \
  -- set_maintenance_mode \
  --admin "$ADMIN_ADDRESS" --enabled false --reason "Upgrade v1.5.0 complete"

echo "=== 6. Smoke test ==="
soroban contract invoke --id "$CONTRACT_ID" --rpc-url "$RPC_URL" \
  --network-passphrase "$NETWORK_PASSPHRASE" -- get_version
```

---

## FAQ

**Q: Can I use `pause` instead of maintenance mode?**
A: No. `pause` blocks *reads* too (health checks, indexers), and auto-expires after 7 days. Maintenance mode keeps reads alive and is operator-controlled.

**Q: What if a user tx is in the mempool when maintenance mode enables?**
A: The tx will either be included before the maintenance-mode ledger (succeeds on old WASM) or after (fails with `MaintenanceModeActive`). The drain window absorbs this variance.

**Q: Do I need quiescence for a fee-bps change?**
A: No. Fee config lives in instance storage and is admin-mutable via `set_fee_config` — no WASM swap required.

**Q: What if the upgrade tx fails?**
A: Maintenance mode stays on. Investigate, then either retry `upgrade_contract_wasm` or disable maintenance mode to roll back to the old WASM (no state change occurred).

---

## Version history

| Version | Date | Author | Change |
|---------|------|--------|--------|
| 1.0 | 2026-07-24 | QuickLendX team | Initial quiescence policy |