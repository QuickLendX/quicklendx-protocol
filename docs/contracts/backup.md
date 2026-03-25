# Backup System

## Overview

The QuickLendX backup module provides admin-controlled invoice state snapshots for recovery operations. This implementation now validates backup metadata at creation/update time, enforces unique backup identifiers, and applies retention cleanup to both backup metadata and stored invoice payloads.

## Public Contract API

### `create_backup`

```rust
pub fn create_backup(env: Env, admin: Address) -> Result<BytesN<32>, QuickLendXError>
```

- Requires `admin.require_auth()`
- Verifies the caller matches `AdminStorage`
- Snapshots all invoices currently reachable through invoice storage
- Creates canonical metadata with:
  - generated `backup_id`
  - current ledger timestamp
  - description `"Protocol state backup"`
  - invoice count
  - `BackupStatus::Active`
- Stores metadata and backup payload
- Adds the backup id to the active backup list
- Applies the current retention policy immediately

### `validate_backup`

```rust
pub fn validate_backup(env: Env, backup_id: BytesN<32>) -> bool
```

Returns `true` only when:

- backup metadata exists
- backup payload exists
- `backup_id` uses the protocol backup id format
- metadata description is non-empty and within bounds
- stored `invoice_count` matches the payload length
- all backed up invoices pass basic structural validation

### `restore_backup`

```rust
pub fn restore_backup(
    env: Env,
    admin: Address,
    backup_id: BytesN<32>,
) -> Result<(), QuickLendXError>
```

- Requires admin authorization
- Rejects invalid or tampered backups before restore
- Clears current invoice state
- Restores invoices from backup payload
- Rebuilds invoice metadata indexes

### `archive_backup`

```rust
pub fn archive_backup(
    env: Env,
    admin: Address,
    backup_id: BytesN<32>,
) -> Result<(), QuickLendXError>
```

- Requires admin authorization
- Marks the backup as `Archived`
- Removes the id from the active retention-managed list
- Preserves metadata and invoice payload for later restore/audit use

### `set_backup_retention_policy`

```rust
pub fn set_backup_retention_policy(
    env: Env,
    admin: Address,
    max_backups: u32,
    max_age_seconds: u64,
    auto_cleanup_enabled: bool,
) -> Result<(), QuickLendXError>
```

- Requires admin authorization
- Updates the active retention policy
- Does not retroactively clean backups until the next backup creation or manual cleanup

### `cleanup_backups`

```rust
pub fn cleanup_backups(env: Env, admin: Address) -> Result<u32, QuickLendXError>
```

- Requires admin authorization
- Applies the active retention policy manually
- Returns the number of purged active backups

## Validation Rules

Backup creation and update now enforce the following invariants:

- `backup_id` must match the protocol-generated backup id format
- descriptions must be non-empty
- descriptions must not exceed `MAX_BACKUP_DESCRIPTION_LENGTH`
- `invoice_count` must match the stored payload length
- duplicate backup ids are rejected

These checks run through the same storage validation path so initialization, admin operations, and cleanup all operate on a single authoritative rule set.

## Retention Policy Behavior

The retention policy is:

```rust
pub struct BackupRetentionPolicy {
    pub max_backups: u32,
    pub max_age_seconds: u64,
    pub auto_cleanup_enabled: bool,
}
```

Default values:

- `max_backups = 5`
- `max_age_seconds = 0`
- `auto_cleanup_enabled = true`

Cleanup rules:

- `max_age_seconds > 0` removes active backups older than the configured age
- `max_backups > 0` removes the oldest active backups until the count fits
- archived backups are excluded from retention cleanup
- purged backups now remove:
  - active-list entry
  - metadata record
  - stored invoice payload

## Security Notes

- Backup creation, restore, archive, retention updates, and manual cleanup are admin-only
- Runtime validation rejects tampered metadata before restore
- Unique backup id enforcement prevents metadata/data aliasing bugs
- Cleanup purges stale payload data instead of only removing list references, reducing orphaned state risk
- Archived backups remain recoverable and are not deleted by automatic cleanup

## Test Coverage

Issue-focused tests cover:

- admin-only backup creation
- backup id uniqueness
- active backup list deduplication
- metadata tamper detection
- count-based retention cleanup
- archived backup preservation
- state replacement during restore
- age-based cleanup thresholds

Recommended verification commands:

```bash
cd quicklendx-contracts
cargo test --test backup_retention_validation -- --quiet
```
