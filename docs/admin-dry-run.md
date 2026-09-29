# Admin Dry-Run Preview — QuickLendX Protocol

> **Status:** Available from `feature/admin-dry-run` (resolves #1221)  
> **Related source:** `src/admin.rs` → `preview_protocol_config`, `preview_fee_config`  
> **Test coverage:** `src/test_admin.rs` — 36 tests, 100 % of dry-run paths

---

## Overview

`set_protocol_config` and `set_fee_config` apply changes atomically to on-chain
storage. Before signing an apply transaction on mainnet, operators can now call
the **dry-run companion functions** to obtain a projected before/after diff
without mutating any state:

| Mutating (apply)      | Read-only (dry-run)          | Return type          |
|-----------------------|------------------------------|----------------------|
| `set_protocol_config` | `preview_protocol_config`    | `ProtocolConfigDiff` |
| `set_fee_config`      | `preview_fee_config`         | `FeeConfigDiff`      |

---

## Diff Structs

### `ProtocolConfigDiff`

```rust
pub struct ProtocolConfigDiff {
    /// Current (before) protocol config stored on-chain.
    pub current: ProtocolConfig,
    /// Projected (after) config that *would* be applied.
    pub projected: ProtocolConfig,
    /// true when current == projected (no-op change).
    pub is_noop: bool,
}
```

### `FeeConfigDiff`

```rust
pub struct FeeConfigDiff {
    /// Current (before) fee config stored on-chain.
    pub current: FeeConfig,
    /// Projected (after) config that *would* be applied.
    pub projected: FeeConfig,
    /// true when current == projected (no-op change).
    pub is_noop: bool,
}
```

Both structs are `#[contracttype]` so they are XDR-serialisable and can be
inspected from any Soroban SDK client.

---

## Security Model

**Dry-run functions are admin-gated and read-only.**

- The caller must be the current admin and must provide valid Soroban
  authorization (`require_admin` is called first, identical to the apply path).
- This prevents arbitrary parties from probing parameter-validation logic
  against live mainnet state.
- No `env.storage().instance().set(…)` call is ever reached inside a dry-run
  function. The guarantee is structural: validation runs, current state is read
  via `get_*`, a diff struct is constructed in memory, and the function returns.
  The Soroban host enforces read-only semantics at the WASM boundary for any
  simulation invocation made with `auth_mode = ReadOnly`.

### Threat model note

An operator workflow where every `set_*` is preceded by a `preview_*`
simulation is **not** a substitute for multi-sig or timelocks. Dry-run
previews are a sanity-check tool; access control for the apply transactions
must still be enforced by your key management and governance layer.

---

## Usage

### Stellar CLI (simulation / mainnet dry-run)

```bash
# Simulate preview_protocol_config — no transaction is submitted
stellar contract invoke \
  --id  <CONTRACT_ID>     \
  --source-account <ADMIN_KEY_PAIR> \
  --network mainnet       \
  --simulate-only         \
  -- preview_protocol_config \
  --admin    <ADMIN_ADDRESS>   \
  --new_config '{"min_invoice_amount":2000000,"max_due_date_days":180,"grace_period_seconds":172800}'
```

The `--simulate-only` flag means the Stellar node evaluates the contract call
in a read-only simulation context. No ledger state is written even if the
operator accidentally omits the flag — the function itself performs no writes.

### JavaScript / TypeScript (soroban-client)

```typescript
import { Contract, Networks, TransactionBuilder, SorobanRpc } from "@stellar/stellar-sdk";

const server = new SorobanRpc.Server("https://soroban-mainnet.stellar.org");
const contract = new Contract(CONTRACT_ID);

// Build an unsigned transaction (dry-run — never submit)
const tx = new TransactionBuilder(sourceAccount, { fee: BASE_FEE, networkPassphrase: Networks.PUBLIC })
  .addOperation(
    contract.call("preview_protocol_config", adminAddress, newProtocolConfig)
  )
  .setTimeout(30)
  .build();

// simulateTransaction is always read-only
const simResult = await server.simulateTransaction(tx);
const diff = simResult.result?.retval; // XDR-decoded ProtocolConfigDiff
console.log("Before:", diff.current);
console.log("After: ", diff.projected);
console.log("No-op: ", diff.is_noop);
```

### Rust integration test pattern

```rust
// 1. Seed config
client.set_protocol_config(&admin, &original_cfg).unwrap();

// 2. Dry-run the proposed change
let diff = client.preview_protocol_config(&admin, &new_cfg).unwrap();
assert_eq!(diff.current, original_cfg);   // before
assert_eq!(diff.projected, new_cfg);      // after
assert!(!diff.is_noop);                   // something changed

// 3. Storage is UNCHANGED after preview
// (verify by running another preview and checking diff.current)
let guard = client.preview_protocol_config(&admin, &original_cfg).unwrap();
assert_eq!(guard.current, original_cfg, "dry-run must not write storage");

// 4. Only now sign and apply
client.set_protocol_config(&admin, &new_cfg).unwrap();
```

---

## Validation Parity

Dry-run functions run **exactly the same validation** as their apply
counterparts. If `new_config` would be rejected by `set_protocol_config`, the
preview call fails with the same `ContractError` before any storage read even
completes.

| Rule                                        | Validated in preview? |
|---------------------------------------------|-----------------------|
| `min_invoice_amount > 0`                    | ✅ yes                |
| `1 ≤ max_due_date_days ≤ 730`              | ✅ yes                |
| `grace_period_seconds ≤ 2_592_000`         | ✅ yes                |
| `fee_bps ≤ 1000`                            | ✅ yes                |

This parity means: if the preview succeeds, the apply will also pass
validation. (It may still fail at the auth layer if the signing key changes
between simulation and submission.)

---

## `is_noop` Flag

When `current == projected` the diff's `is_noop` field is set to `true`. Use
this to detect accidental re-application of the existing configuration:

```bash
# CLI — check is_noop before committing
DIFF=$(stellar contract invoke ... -- preview_protocol_config ...)
if echo "$DIFF" | jq -e '.is_noop == true'; then
  echo "WARNING: proposed config is identical to on-chain config. Aborting."
  exit 1
fi
```

---

## Error Reference

| Error                   | Meaning in dry-run context                                  |
|-------------------------|-------------------------------------------------------------|
| `NotAdmin` (3)          | Caller is not the current admin — auth rejected.            |
| `NotInitialized` (1)    | No config in storage yet (protocol not initialized).        |
| `InvalidAmount` (5)     | `min_invoice_amount == 0`                                   |
| `InvalidFee` (6)        | `fee_bps > 1000`                                            |
| `InvalidParameter` (7)  | `max_due_date_days` out of range or grace period too large. |

---

## Running Tests

```bash
# All admin tests (36 tests, including all dry-run scenarios)
cargo test test_admin

# Specific dry-run test groups
cargo test test_preview_protocol_config
cargo test test_preview_fee_config
cargo test test_preview_does_not_write
```

### Test scenarios covered

**`preview_protocol_config`**

| Test | Scenario |
|------|----------|
| `test_preview_protocol_config_returns_diff` | Returns correct before/after diff |
| `test_preview_protocol_config_noop_when_same` | `is_noop = true` for identical config |
| `test_preview_protocol_config_matches_apply_effect` | Projected == what apply would write |
| `test_preview_protocol_config_invalid_params_rejected` | `InvalidAmount` on bad amount |
| `test_preview_protocol_config_invalid_due_date_rejected` | `InvalidParameter` on bad due date |
| `test_preview_protocol_config_invalid_grace_period_rejected` | `InvalidParameter` on bad grace period |
| `test_preview_protocol_config_non_admin_blocked` | `NotAdmin` for impostor |
| `test_preview_protocol_config_not_initialized` | `NotInitialized` when no config seeded |
| `test_preview_protocol_config_boundary_min_amount_one` | Boundary: `min_invoice_amount = 1` |
| `test_preview_protocol_config_boundary_max_due_date` | Boundary: `max_due_date_days = 730` |
| `test_preview_protocol_config_boundary_max_grace_period` | Boundary: `grace_period_seconds = 2_592_000` |
| `test_preview_does_not_write_protocol_config` | **Storage guard** — no mutation |

**`preview_fee_config`**

| Test | Scenario |
|------|----------|
| `test_preview_fee_config_returns_diff` | Returns correct before/after diff |
| `test_preview_fee_config_noop_when_same` | `is_noop = true` for identical config |
| `test_preview_fee_config_matches_apply_effect` | Projected == what apply would write |
| `test_preview_fee_config_fee_too_high_rejected` | `InvalidFee` on `fee_bps > 1000` |
| `test_preview_fee_config_non_admin_blocked` | `NotAdmin` for impostor |
| `test_preview_fee_config_not_initialized` | `NotInitialized` when no config seeded |
| `test_preview_fee_config_boundary_max_fee` | Boundary: `fee_bps = 1000` |
| `test_preview_fee_config_boundary_zero_fee` | Boundary: `fee_bps = 0` |
| `test_preview_does_not_write_fee_config` | **Storage guard** — no mutation |

---

## Operator UX — Recommended Workflow

```
┌─────────────────────────────────────────────────────────┐
│               Safe Config Update Workflow               │
└─────────────────────────────────────────────────────────┘

 1. PREPARE  – Construct new_config off-chain.

 2. PREVIEW  – Call preview_protocol_config / preview_fee_config
               with --simulate-only (no ledger write).
               ┌─ Check is_noop ──────────────────────────┐
               │  true  → config unchanged; skip apply.   │
               │  false → review diff.current vs projected│
               └──────────────────────────────────────────┘

 3. REVIEW   – Confirm projected values match intent.
               Have a second operator reproduce the preview
               independently.

 4. APPLY    – Sign and submit set_protocol_config /
               set_fee_config only after review sign-off.

 5. VERIFY   – Run preview again. current should now equal
               the previously projected value.
```

> **Security note for operators:** Never skip step 2. Even for small parameter
> changes, the preview catches mistyped basis points, off-by-one day limits,
> and accidental no-ops before a transaction fee is paid and ledger state is
> mutated. On high-value mainnet contracts this is the difference between a
> routine update and an incident.

---

## File Map

```
src/
  admin.rs          ← ProtocolConfigDiff, FeeConfigDiff structs;
  │                    preview_protocol_config, preview_fee_config entry-points;
  │                    validate_*, get_*, apply_* helpers
  test_admin.rs     ← 36 tests covering all apply + dry-run paths
  storage_types.rs  ← ProtocolConfig, FeeConfig, DataKey
  errors.rs         ← ContractError enum
  init.rs           ← initialize_protocol (first-time setup)
  lib.rs            ← crate root; re-exports ProtocolConfigDiff, FeeConfigDiff
docs/
  admin-dry-run.md  ← this file
```
