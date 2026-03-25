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

**Parameters:**
- `backup_id`: ID of the backup

**Returns:**
- `Some(Backup)`: Backup details
- `None`: Backup not found

## Cleanup Algorithm

The cleanup algorithm runs in two phases:

### Phase 1: Age-Based Cleanup

If `max_age_seconds > 0`:
1. Calculate age of each active backup
2. Remove backups older than `max_age_seconds`
3. Count removed backups

### Phase 2: Count-Based Cleanup

If `max_backups > 0`:
1. Sort remaining backups by timestamp (oldest first)
2. Remove oldest backups until count ≤ `max_backups`
3. Count removed backups

**Important:** Archived backups are never cleaned up automatically.

## Events

### bkup_crt (Backup Created)

Emitted when a backup is created.

**Data:**
- `backup_id`: BytesN<32>
- `invoice_count`: u32
- `timestamp`: u64

### bkup_rstr (Backup Restored)

Emitted when a backup is restored.

**Data:**
- `backup_id`: BytesN<32>
- `invoice_count`: u32
- `timestamp`: u64

### bkup_vd (Backup Validated)

Emitted when a backup is validated.

**Data:**
- `backup_id`: BytesN<32>
- `success`: bool
- `timestamp`: u64

### bkup_ar (Backup Archived)

Emitted when a backup is archived.

**Data:**
- `backup_id`: BytesN<32>
- `timestamp`: u64

### ret_pol (Retention Policy Updated)

Emitted when retention policy is updated.

**Data:**
- `max_backups`: u32
- `max_age_seconds`: u64
- `auto_cleanup_enabled`: bool
- `timestamp`: u64

### bkup_cln (Backups Cleaned)

Emitted when backups are cleaned up.

**Data:**
- `removed_count`: u32
- `timestamp`: u64

## Restore Workflow Ordering and Idempotency

### Ordering guarantee

`restore_backup` validates the backup's data integrity **before** clearing any live
state. If validation fails (count mismatch, missing data, non-positive amounts) the
function returns an error and the live invoice state is left completely unchanged,
preventing partial-restore corruption.

```
1. caller.require_auth()           ← authorization check
2. AdminStorage::require_admin()   ← role check
3. BackupStorage::validate_backup() ← integrity check (BEFORE any state change)
4. InvoiceStorage::clear_all()     ← clear only after validation passes
5. InvoiceStorage::store_invoice() ← restore each invoice
6. emit bkup_rstr event
```

### Idempotency guarantee

`restore_backup` is **idempotent**: calling it multiple times with the same backup ID
always produces the same invoice state. Each call:

1. Clears all current invoices (via status-list clearing)
2. Re-stores exactly the invoices from the backup

The status-index layer (`add_to_status_invoices`) prevents duplicate entries in the
count lists, so `get_total_invoice_count` always returns the backup's invoice count
regardless of how many times restore is called.

### Rejecting corrupted backups

A backup is considered corrupted when:
- The stored `invoice_count` does not match the actual number of invoice records
- Any invoice in the backup has `amount ≤ 0`
- The backup metadata or data is missing from storage

`validate_backup` marks the backup status as `BackupStatus::Corrupted` when
validation fails. `restore_backup` rejects corrupted backups via the same
validation call, ensuring corrupt data is never written back to live state.

### Non-destructive failure

If `restore_backup` is called with a non-existent backup ID it returns
`StorageKeyNotFound` immediately without touching live state.

## Security Considerations

### Access Control

- All backup operations require admin authorization
- Only admin can create, restore, archive, and configure retention
- Query operations are public (read-only)

### Data Integrity

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
