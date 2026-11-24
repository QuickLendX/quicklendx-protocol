# Frozen Storage Layout Fixtures

This directory holds frozen Soroban XDR snapshots of released on-chain storage
layouts used by `src/test_storage_migration_compat.rs`.

## Files

- `storage_v1_layout.json` — Raw base64-encoded `ScVal` keys and values for the
  v1 storage layout (`Invoice`, `Bid`, `Investment`, `BackupV1`).

## Refresh Policy

Do **not** edit or regenerate these fixtures to make a failing test pass. A test
failure indicates a backward-incompatible change to a stored struct, enum, or
storage key.

Only regenerate `storage_v1_layout.json` on an **intentional breaking storage
change** that bumps `STORAGE_SCHEMA_VERSION` / `PROTOCOL_VERSION` and includes a
migration path:

```bash
cd quicklendx-contracts
cargo test --lib generate_v1_storage_fixture -- --ignored
```
