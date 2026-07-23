# Upgrade Paths

**Audience: operators** — this document is for people running a deployed QuickLendX contract who need to ship a new WASM binary or a storage-schema change. Contributors adding new features should also read the "What requires a migration?" section before merging.

Related docs: [`docs/contracts/initialization.md`](contracts/initialization.md), [`docs/contracts/backup.md`](contracts/backup.md), [`docs/contracts/storage-schema.md`](contracts/storage-schema.md).

---

## How Soroban upgrades work

QuickLendX is a single Soroban contract (`quicklendx-contracts`). Its identity on-chain is a stable 32-byte contract ID; the executing WASM bytecode is a separate object identified by a WASM hash. Upgrading the contract means replacing the WASM hash the contract ID points to — all persistent and instance storage is preserved automatically by the network.

There is no `upgrade()` function in this contract. A WASM replacement is performed at the network level by the contract administrator using the Soroban CLI:

```bash
# 1. Build the new binary
cargo build --target wasm32-unknown-unknown --release

# 2. Install the new WASM blob and capture its hash
WASM_HASH=$(soroban contract install \
  --wasm target/wasm32-unknown-unknown/release/quicklendx_contracts.wasm \
  --rpc-url $RPC_URL \
  --network-passphrase "$NETWORK_PASSPHRASE" \
  --source $ADMIN_SECRET_KEY)

# 3. Replace the running WASM on the existing contract
soroban contract invoke \
  --id $CONTRACT_ID \
  --source $ADMIN_SECRET_KEY \
  --rpc-url $RPC_URL \
  --network-passphrase "$NETWORK_PASSPHRASE" \
  -- upgrade_contract_wasm \
  --new_wasm_hash "$WASM_HASH"
```

> If the contract does not expose an `upgrade_contract_wasm` entrypoint, the WASM replacement must be authorised through Stellar's native contract-update mechanism (Auth invocation with `InvokeContractArgs` targeting the network's built-in upgrade host function). Refer to the Soroban documentation for the exact XDR encoding.

---

## Version numbers at a glance

| Versioned artefact | Constant / key | Current value | Where it lives |
|---|---|---|---|
| Protocol | `PROTOCOL_VERSION` / `proto_ver` instance key | **1** | `src/init.rs:71` |
| Backup format | `Backup.format_version` field | **2** (v1 still readable) | `src/backup.rs` |
| Analytics snapshot schema | `ANALYTICS_SCHEMA_VERSION` | **1** | `src/analytics.rs:35` |

To read the protocol version of a live deployment:

```bash
soroban contract invoke \
  --id $CONTRACT_ID \
  --source $ADMIN_SECRET_KEY \
  --rpc-url $RPC_URL \
  --network-passphrase "$NETWORK_PASSPHRASE" \
  -- get_version
# returns: 1
```

`get_version()` returns the value written to storage during `initialize()`, so it reflects the protocol version that was active when the contract was first set up — not the version of the currently-running WASM. After a WASM-only upgrade that does not bump `PROTOCOL_VERSION`, `get_version()` still returns the old value.

---

## Upgrade compatibility matrix

| From protocol version | To protocol version | Storage migration required? | Safe path |
|---|---|---|---|
| 1 | 1 (patch) | No | WASM-only redeploy |
| 1 | 1 (minor, new optional fields) | No (lazy reads return defaults) | WASM-only redeploy, new fields initialise on first write |
| 1 | 2 (major, breaking storage layout) | **Yes** | Create backup → run migration entrypoint → verify → redeploy WASM |
| — | fresh deploy | No | `initialize()` |

### Backup format compatibility matrix

| Stored format version | Current reader (v2) can read? | Auto-migrated on read? |
|---|---|---|
| v1 (`BackupV1`, no `format_version` field) | Yes | Yes — `Backup::from_v1()` is called automatically inside `get_backup()` |
| v2 (`Backup`, `format_version: 2`) | Yes | No migration needed |
| v3+ (hypothetical future) | No — returns `BackupVersionUnsupported` | No |

The auto-migration in `get_backup()` is **lazy**: stored bytes are not rewritten; every read of a v1 record deserialises the old struct and converts it to v2 in memory. A future explicit migration would need to rewrite those records if v1 support is to be dropped.

---

## What requires a migration?

### No migration needed (WASM-only redeploy)

- Bug fixes and gas optimisations that do not change any storage key or stored struct layout.
- New contract entrypoints (new `pub fn` in `lib.rs`).
- New optional fields on stored structs **if** read paths supply a safe default when the field is absent. (Soroban deserialisation of `#[contracttype]` structs is positional; adding a field at the end is backward-compatible with old-format reads only if the reader handles a missing tail field, which the SDK does not do by default — see "Adding fields" below.)
- Configuration-only changes: fee basis points, protocol limits, treasury address — these are admin-mutable without any code change.

### Migration required

Any change to a **storage key string** or **stored struct layout** is a breaking change. Examples:

| Change | Why it breaks | Migration |
|---|---|---|
| Rename `symbol_short!("inv_count")` to `"invoice_count"` | Old key is orphaned; counter resets to 0 | Read old key → write new key → remove old key |
| Add a required field to `Invoice` | Existing records cannot deserialise with the new struct | Read all invoices in old format → rewrite in new format |
| Remove a field from a stored struct | Reader panics on unexpected XDR | Read all records → rewrite without the field |
| Change `DataKey::Invoice(BytesN<32>)` discriminant | Every persistent invoice key is orphaned | Full re-index from backup or event replay |

### Adding fields safely

Soroban `#[contracttype]` structs are encoded as XDR tuples (positional). Adding a field at the end makes the new WASM unable to read existing records because the XDR length no longer matches. The correct approach for additive changes is to introduce a **new storage key** for the new field, keyed by the same entity ID, and read it with `.unwrap_or(default)`.

```rust
// Safe: store new optional data under a separate key, never modifying Invoice layout
fn get_invoice_risk_flag(env: &Env, invoice_id: &BytesN<32>) -> bool {
    env.storage()
        .persistent()
        .get(&(symbol_short!("inv_rf"), invoice_id.clone()))
        .unwrap_or(false)
}
```

---

## Protocol v1 → v2 upgrade procedure (template)

This is the checklist to follow when `PROTOCOL_VERSION` is bumped to 2. Until that happens, this section describes the intended procedure.

**Before the upgrade:**

1. Call `get_version()` and confirm the deployment is at v1.
2. Create a safety backup:
   ```bash
   soroban contract invoke --id $CONTRACT_ID ... -- create_backup
   # note the returned backup_id
   ```
3. Validate the backup:
   ```bash
   soroban contract invoke --id $CONTRACT_ID ... -- validate_backup \
     --backup_id $BACKUP_ID
   # must return: true
   ```
4. Review the migration notes in the release changelog for the specific fields and keys that changed.

**During the upgrade:**

5. Install the new WASM and obtain its hash (see "How Soroban upgrades work" above).
6. If a migration entrypoint is provided in the new WASM, invoke it **before** replacing the WASM hash — or immediately after, depending on whether the migrator reads the old or new schema.
7. Replace the WASM hash on the contract.

**After the upgrade:**

8. Call `get_version()` — it will still return 1 unless the migration entrypoint explicitly wrote the new version to `proto_ver`. Call the version-bump entrypoint if one is provided.
9. Smoke-test core flows: `upload_invoice`, `place_bid`, `get_investor_verification`.
10. Verify the backup from step 2 is still readable:
    ```bash
    soroban contract invoke --id $CONTRACT_ID ... -- validate_backup \
      --backup_id $BACKUP_ID
    ```

**Rollback:**

If the upgrade fails, restore from the pre-upgrade backup:
```bash
soroban contract invoke \
  --id $CONTRACT_ID \
  --source $ADMIN_SECRET_KEY \
  --rpc-url $RPC_URL \
  --network-passphrase "$NETWORK_PASSPHRASE" \
  -- restore_backup \
  --admin $ADMIN_ADDRESS \
  --backup_id $BACKUP_ID
```
Then reinstall the previous WASM hash.

---

## Analytics schema upgrades

The analytics snapshot export stamps `schema_version: ANALYTICS_SCHEMA_VERSION` (currently `1`) into every exported object. Off-chain indexers that consume this endpoint must reject payloads with an unrecognised schema version rather than silently processing them:

```typescript
const snapshot = await contract.exportAnalyticsSnapshot();
if (snapshot.schema_version !== SUPPORTED_SCHEMA_VERSION) {
  throw new Error(`Unsupported analytics schema: ${snapshot.schema_version}`);
}
```

When `ANALYTICS_SCHEMA_VERSION` is bumped, bump `SUPPORTED_SCHEMA_VERSION` in all indexers before deploying the new contract WASM.

---

## Storage key stability

The storage key strings defined in `src/storage.rs` and `src/init.rs` are on-chain constants. The source comments mark every key with `BREAKING: Rename Requires Migration`. A summary:

| Key string | Storage class | What it holds |
|---|---|---|
| `"fees"` | Instance | Platform fee config |
| `"inv_count"` | Persistent | Invoice counter |
| `"bid_count"` | Persistent | Bid counter |
| `"inv_cnt"` | Persistent | Investment counter |
| `"proto_in"` | Instance | Initialisation flag |
| `"proto_ver"` | Instance | Protocol version written at init |
| `"proto_cf"` | Instance | Protocol configuration |
| `"treasury"` | Instance | Treasury address |
| `"fee_bps"` | Instance | Fee basis points |
| `"curr_wl"` | Instance | Currency whitelist |
| `"bkup_pol"` | Instance | Backup retention policy |
| `"protocol_limits"` | Instance | Invoice/bid validation limits |

Any renaming of these strings requires an explicit migration — see `src/storage.rs` for the four-step migration checklist embedded in the `StorageKeys` and `Indexes` doc comments.

---

## Related documents

- [`docs/contracts/initialization.md`](contracts/initialization.md) — one-time setup and parameter validation
- [`docs/contracts/backup.md`](contracts/backup.md) — creating and restoring backups
- [`docs/contracts/storage-schema.md`](contracts/storage-schema.md) — full storage key catalogue
- [`docs/contracts/storage.md`](contracts/storage.md) — indexing strategy and integrity audit
- [`docs/RUNBOOK_INCIDENT_RESPONSE.md`](RUNBOOK_INCIDENT_RESPONSE.md) — what to do if an upgrade goes wrong
- Source: `src/init.rs` (`PROTOCOL_VERSION`, `get_version`, `initialize`)
- Source: `src/backup.rs` (`get_backup`, `Backup::from_v1`, `verify_backup_version`)
- Source: `src/analytics.rs` (`ANALYTICS_SCHEMA_VERSION`)
