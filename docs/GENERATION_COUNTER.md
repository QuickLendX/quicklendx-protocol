# Generation Counter (Protocol Version)

What the generation counter is, who consumes it, and how to read it on-chain.

---

## What it is

The **generation counter** is the on-chain **protocol version** — a single `u32` stored in instance storage under the key `proto_ver` (symbol `PROTOCOL_VERSION_KEY` in [`init.rs`](../quicklendx-contracts/src/init.rs:61)).

It is written **once**, at protocol initialization time, and **never updated** afterward. The value comes from the `PROTOCOL_VERSION` constant compiled into the WASM at the moment `initialize` runs.

```rust
// quicklendx-contracts/src/init.rs
pub const PROTOCOL_VERSION: u32 = 1;
pub(crate) const PROTOCOL_VERSION_KEY: Symbol = symbol_short!("proto_ver");

// Written during initialize (line ~396)
env.storage().instance().set(&PROTOCOL_VERSION_KEY, &PROTOCOL_VERSION);
```

Because it is stored in instance storage, the value survives WASM upgrades. A contract upgraded from v1 → v2 will still report the protocol version that originally initialized it (e.g., `1`) unless a storage migration explicitly rewrites the key.

---

## Who consumes it

| Consumer | How they read it | Why they care |
|----------|------------------|---------------|
| **Off-chain integrators** (SDKs, indexers, UIs) | `get_version` entrypoint (`stellar-cli contract invoke … -- get_version`) | Gate major-version incompatibilities before trusting results. |
| **Operators / maintainers** | Same `get_version` call after a WASM upgrade | Verify the upgrade did not silently change the stored version (storage migration skipped). |
| **Governance / timelock tooling** | Read `proto_ver` directly from storage via RPC | Confirm the contract version matches the governance proposal before executing a timelocked upgrade. |
| **Contract tests** | `ProtocolInitializer::get_version(&env)` | The `test_generation_bump_invariant_version_read_from_storage` test asserts the value is *read from storage*, not the compile-time constant — this is the "generation-bump invariant". |

---

## Reading it on-chain

### Via CLI (read-only)

```bash
# Mainnet example — replace with your contract ID
stellar-cli contract invoke \
  --id CDLZFC3SHJYVV6K7QGJQX3K5QZ7QK7QK7QK7QK7QK7QK7QK7QK7QK7QK \
  --network mainnet \
  -- get_version
# -> 1
```

### Via Soroban SDK (off-chain query)

```rust
use soroban_sdk::{Env, Address, contractclient};

#[contractclient(name = "QuickLendXClient")]
pub trait QuickLendX {
    fn get_version(env: Env) -> u32;
}

let contract_id = Address::from_string(&String::from_str(&env, "CDLZF..."));
let client = QuickLendXClient::new(&env, &contract_id);
let version = client.get_version();
assert_eq!(version, 1);
```

### Direct storage read (operator tooling)

```bash
# Raw storage read via stellar-cli (requires RPC access)
stellar-cli contract storage \
  --id <CONTRACT_ID> \
  --network mainnet \
  --instance proto_ver
# -> 1
```

---

## Generation-bump invariant (tested invariant)

The test `test_generation_bump_invariant_version_read_from_storage` in
[`test_init_invariants.rs`](../quicklendx-contracts/src/test_init_invariants.rs:159)
verifies:

1. Before `initialize`: `get_version` returns the compile-time constant (`PROTOCOL_VERSION`).
2. After `initialize`: `get_version` returns the value **read from storage** (which equals the constant at that moment).
3. After a simulated WASM upgrade (direct storage write of a different value): `get_version` **must** return the storage value, not the new constant.

This guarantees that `get_version` always reflects the generation that created the storage layout, not the generation currently executing.

---

## Upgrade policy (when the counter increments)

The `PROTOCOL_VERSION` constant is bumped according to the table below (documented in
[`init.rs`](../quicklendx-contracts/src/init.rs:55-75) and
[`CONTRACT_VERSION_COMPATIBILITY.md`](CONTRACT_VERSION_COMPATIBILITY.md#upgrade-policy)).

| Release type | Example | Storage schema | Version bump |
|--------------|---------|----------------|--------------|
| **Patch**    | Bug-fix, no storage-layout change | Unchanged | Not required |
| **Minor**    | New fields, backward-compatible reads | Additive | Recommended |
| **Major**    | Breaking storage change, migration required | Breaking | **Mandatory** |

Only a **major** release must increment `PROTOCOL_VERSION` before building the WASM. Patch and minor releases may leave it unchanged.

---

## Related documentation

- [`CONTRACT_VERSION_COMPATIBILITY.md`](CONTRACT_VERSION_COMPATIBILITY.md) — full interop matrix for protocol version, backup format, and analytics schema.
- [`quicklendx-contracts/src/init.rs`](../quicklendx-contracts/src/init.rs:61) — constant definition, storage key, and `get_version` implementation.
- [`quicklendx-contracts/src/test_init_invariants.rs`](../quicklendx-contracts/src/test_init_invariants.rs:159) — generation-bump invariant test.
- [`RUNBOOK_INCIDENT_RESPONSE.md`](RUNBOOK_INCIDENT_RESPONSE.md) — operator checklist after WASM upgrade (includes `get_version` verification).