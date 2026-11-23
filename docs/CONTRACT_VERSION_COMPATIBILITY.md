# Contract Version Compatibility

Which versions of the QuickLendX Soroban contract can interoperate, and what
stays compatible across an upgrade.

**Audience:** operators and maintainers who deploy or upgrade the on-chain
contract, plus downstream integrators (off-chain indexers, backup tooling) that
read versioned data the contract produces. If you just want to know whether a
client built against version *N* still works after the contract is upgraded to
version *M*, this is the page.

For the subsystem-level detail behind each surface, follow the links in the
[Versioned surfaces](#versioned-surfaces) table.

## How the contract is versioned

QuickLendX is a single Soroban contract that is **upgraded in place**: a WASM
upgrade swaps the executable but keeps the same contract ID, the same address,
and the same persistent storage. There is no second contract to talk to and no
cross-contract version negotiation — "interoperability" here means three things:

1. **Client ↔ contract** — does an off-chain caller built against an older
   contract version still get correct results after the contract is upgraded?
2. **Contract ↔ its own stored data** — can a newer WASM still read state and
   backups written by an older WASM?
3. **Contract ↔ downstream consumers** — can indexers and tooling keep decoding
   the versioned payloads the contract exports?

The contract exposes a single integer **protocol version** plus two
independently-versioned data surfaces. They do **not** share a number; each is
bumped on its own schedule.

### Protocol version

The protocol version is a simple `u32` that increments (`1`, `2`, `3`, …). It is
defined once as a constant and written to instance storage during
`initialize`, so it reflects the version that set the contract up — see
[`PROTOCOL_VERSION`](../quicklendx-contracts/src/init.rs) and
`ProtocolInitializer::get_version`.

Read it on a live contract with the public `get_version` entrypoint:

```bash
stellar-cli contract invoke \
    --id <CONTRACT_ID> \
    --network mainnet \
    -- get_version
# -> 1
```

The current protocol version is **1**. All `0.1.x` deployments report `1`.

#### Upgrade policy

The bump rules are documented next to the constant in
[`src/init.rs`](../quicklendx-contracts/src/init.rs) and govern when the protocol
version changes:

| Release type | Example | Storage schema | Version bump |
| :--- | :--- | :--- | :--- |
| **Patch** | bug-fix, no storage-layout change | unchanged | not required |
| **Minor** | new fields, backward-compatible reads | additive | recommended |
| **Major** | breaking storage change, migration required | breaking | **mandatory** |

A caller built against an earlier protocol version remains compatible across
**patch** and **minor** upgrades: existing entrypoint signatures and return
shapes are preserved, so old clients keep working and simply do not see new
fields. A **major** upgrade may change storage layout or entrypoint behavior and
is the only case that can break an old client; majors are gated behind a
mandatory version bump precisely so consumers can detect them by reading
`get_version` before trusting results.

## Versioned surfaces

| Surface | Constant | Current | Exposed via | Interop rule | Detail |
| :--- | :--- | :--- | :--- | :--- | :--- |
| Protocol version | `PROTOCOL_VERSION` | `1` | `get_version` | Patch/minor backward-compatible; major requires migration | this page |
| Backup format | `format_version` | `2` | `Backup` metadata, `validate_backup` | v1 auto-upgraded on restore; v3+ rejected | [backup-format.md](backup-format.md) |
| Analytics schema | `ANALYTICS_SCHEMA_VERSION` | `1` | `export_analytics_snapshot` (`schema_version`) | Bumped only on breaking shape change; indexers reject mismatches | [analytics-snapshot.md](../quicklendx-contracts/docs/analytics-snapshot.md) |

### Backup format compatibility

Backups carry their own `format_version`, independent of the protocol version. A
newer contract can restore an **older** backup, but never a newer one:

| Stored backup | Running contract | Restorable? | Mechanism |
| :--- | :--- | :--- | :--- |
| v1 (legacy) | v2 | Yes | upgraded on-the-fly (`BackupV1` → `Backup`) |
| v2 (current) | v2 | Yes | restored directly |
| v3+ (future) | v2 | No | rejected with `BackupVersionUnsupported` (error `2203`) |

Check a stored backup before trusting it — `validate_backup` verifies its format
version and integrity, returning `false` for unsupported or corrupt payloads:

```bash
stellar-cli contract invoke \
    --id <CONTRACT_ID> \
    --network mainnet \
    -- validate_backup --backup_id <BACKUP_ID>
# -> true
```

Full matrix, the v1→v2 adapter, and the failure modes for malformed payloads are
in [backup-format.md](backup-format.md).

### Analytics schema compatibility

`export_analytics_snapshot` stamps every snapshot with `schema_version` (drawn
from `ANALYTICS_SCHEMA_VERSION`, currently `1`). Indexers should persist and
validate that field on every read and **reject** a snapshot whose
`schema_version` they do not recognize rather than silently misread it. The
constant is bumped only on a breaking shape change (field removal, rename, type
or semantic change); purely additive fields are coordinated with indexers
before a bump. See [analytics-snapshot.md](../quicklendx-contracts/docs/analytics-snapshot.md) for the
JSON-equivalent shape and the field-by-field contract.

## Checking versions before you trust a contract

Before an integration or tool relies on a deployment, read all three surfaces
and compare them to what your client expects:

```bash
# 1. Protocol version — gate major-version incompatibilities here.
stellar-cli contract invoke --id <CONTRACT_ID> --network mainnet -- get_version

# 2. Analytics schema — read schema_version off a live snapshot.
stellar-cli contract invoke --id <CONTRACT_ID> --network mainnet \
    -- export_analytics_snapshot

# 3. Backup format — validate before any restore.
stellar-cli contract invoke --id <CONTRACT_ID> --network mainnet \
    -- validate_backup --backup_id <BACKUP_ID>
```

> The invocations above are illustrative; substitute a real `<CONTRACT_ID>`,
> `<BACKUP_ID>`, and network. They call only public, read-only or
> validation entrypoints and do not mutate state.

## Operator upgrade checklist

When you bump the contract WASM:

- [ ] Decide the release type (patch / minor / major) using the
      [upgrade policy](#upgrade-policy) table.
- [ ] For a **major** release, bump `PROTOCOL_VERSION` in
      [`src/init.rs`](../quicklendx-contracts/src/init.rs) **before** building
      the WASM, and document the migration.
- [ ] If a struct that is backed up changes shape, bump `format_version` and add
      a `from_vN` adapter — see [backup-format.md](backup-format.md).
- [ ] If `AnalyticsSnapshot` changes shape, bump `ANALYTICS_SCHEMA_VERSION` and
      notify indexers — see [analytics-snapshot.md](../quicklendx-contracts/docs/analytics-snapshot.md).
- [ ] After upgrading, confirm `get_version` reports the expected value and that
      existing backups still pass `validate_backup`.

## Related documentation

- [backup-format.md](backup-format.md) — backup `format_version` matrix and the v1→v2 upgrade path.
- [analytics-snapshot.md](../quicklendx-contracts/docs/analytics-snapshot.md) — `schema_version` contract for off-chain indexers.
- [RUNBOOK_INCIDENT_RESPONSE.md](RUNBOOK_INCIDENT_RESPONSE.md) — operator recovery procedures.
- [`quicklendx-contracts/README.md`](../quicklendx-contracts/README.md) — full contract API and deployment guide.
