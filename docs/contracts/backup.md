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

### Restore Workflow Ordering and Idempotency

### Ordering guarantee

`restore_backup` follows a strict **validate → clear → restore** sequence to ensure index integrity:

1. **Full Integrity Check**: `validate_backup` verifies the backup exists, is marked `Active`, and passes all payload checks (count, amounts) **before** any state change.
2. **Atomic Clear**: `InvoiceStorage::clear_all` removes all current invoices and *all* secondary indexes (business, status, customer, tax_id).
3. **Index Rebuild**: `InvoiceStorage::store_invoice` re-registers every invoice from the backup, which automatically rebuilds all secondary indexes from scratch.
4. **Final Archival**: The backup is marked as `Archived` to prevent accidental re-plays.

If validation fails, the contract state remains completely untouched.

### Idempotency and Safety

Repeated restore of the same backup is prevented by the status transition to `Archived`. This ensures:
- **No Stale Overlays**: Since storage is cleared before restore, backup data never overlays existing state.
- **Index Integrity**: All indexes are rebuilt from the source of truth in the backup.
- **Idempotency**: If a restore were repeated (e.g. by resetting the status to Active), it would produce the exact same state as the first restore due to the `clear_all` step.

### Rejecting Corrupted or Used Backups

A backup is rejected for restore if:
- It is not in `BackupStatus::Active` (i.e. it was already used or marked `Archived`/`Corrupted`).
- The stored `invoice_count` does not match the actual number of records.
- Any invoice in the backup has `amount ≤ 0`.
- The backup metadata or payload is missing from storage.

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

## Test Coverage (Issue #819)

### Unit Tests — `src/test_backup_safety.rs`

Low-level `BackupStorage` tests (no contract client):

| Test | What it validates |
| :--- | :--- |
| `test_generate_backup_id_has_correct_prefix` | ID prefix is always `0xB4 0xC4` |
| `test_generate_backup_id_uniqueness` | Consecutive IDs are distinct |
| `test_is_valid_backup_id_prefix_check` | Prefix validation logic |
| `test_store_backup_rejects_duplicate_id` | Duplicate ID → `OperationNotAllowed` |
| `test_store_backup_rejects_empty_description` | Empty description → `InvalidDescription` |
| `test_store_backup_rejects_count_mismatch` | Count mismatch → `StorageError` |
| `test_validate_backup_succeeds_for_valid_backup` | Happy path |
| `test_validate_backup_fails_when_record_missing` | Missing record → `StorageKeyNotFound` |
| `test_validate_backup_fails_when_data_missing` | Missing payload → `StorageKeyNotFound` |
| `test_validate_backup_fails_on_count_mismatch` | Count mismatch → `StorageError` |
| `test_validate_backup_fails_on_zero_amount_invoice` | Zero-amount invoice → `StorageError` |
| `test_validate_backup_fails_for_archived_backup` | Archived → `OperationNotAllowed` |
| `test_validate_backup_fails_for_corrupted_backup` | Corrupted → `OperationNotAllowed` |
| `test_restore_returns_correct_count` | Returns restored invoice count |
| `test_restore_clears_existing_invoices` | Stale invoices removed before restore |
| `test_restore_rebuilds_status_index` | Status index rebuilt from backup |
| `test_restore_marks_backup_archived` | Backup archived after restore |
| `test_restore_fails_for_archived_backup` | Idempotency guard via archival |
| `test_restore_fails_for_nonexistent_backup` | Non-existent ID fails safely |
| `test_cleanup_returns_zero_when_disabled` | Disabled policy → 0 removed |
| `test_cleanup_count_policy_removes_oldest` | Count policy removes oldest |
| `test_cleanup_age_policy_removes_expired` | Age policy removes expired |
| `test_cleanup_does_not_remove_archived_backups` | Archived backups survive cleanup |
| `test_add_to_backup_list_is_idempotent` | No duplicate list entries |
| `test_remove_from_backup_list` | Correct entry removed |
| `test_purge_backup_removes_all_traces` | Metadata + payload + list entry purged |

### Integration Tests — `tests/backup_retention_validation.rs`

End-to-end tests through the contract client:

| Test | What it validates |
| :--- | :--- |
| `test_restore_ordering_validate_before_clear` | Tampered backup leaves storage untouched |
| `test_restore_ordering_clear_before_restore` | Post-backup invoices are cleared |
| `test_restore_ordering_archive_after_restore` | Backup archived after successful restore |
| `test_restore_nonexistent_backup_fails_safely` | Non-existent ID fails without side effects |
| `test_repeated_restore_is_blocked_via_archival` | Second restore → `OperationNotAllowed` |
| `test_restore_produces_identical_state_when_repeated` | Idempotent state after repeated restore |
| `test_archived_backup_cannot_be_restored` | Archived backup → `OperationNotAllowed` |
| `test_restore_rebuilds_business_index` | Business index rebuilt correctly |
| `test_restore_rebuilds_status_index` | Status index rebuilt correctly |
| `test_restore_rebuilds_multiple_status_buckets` | Multiple status buckets rebuilt |
| `test_restore_empty_backup_clears_all_invoices` | Empty backup clears all state |
| `test_retention_count_based_cleanup` | Oldest active backup purged |
| `test_retention_age_based_cleanup` | Expired backup purged |
| `test_archived_backups_survive_cleanup` | Archived backups not touched by cleanup |
| `test_manual_cleanup_disabled_returns_zero` | Disabled policy → 0 removed |
| `test_manual_cleanup_enforces_policy_when_enabled` | Manual cleanup enforces policy |
| `test_create_backup_requires_admin` | Non-admin → `NotAdmin` |
| `test_restore_backup_requires_admin` | Non-admin → `NotAdmin` |
| `test_archive_backup_requires_admin` | Non-admin → `NotAdmin` |
| `test_cleanup_backups_requires_admin` | Non-admin → `NotAdmin` |
| `test_validate_backup_rejects_tampered_count` | Tampered count → validation fails |
| `test_validate_backup_rejects_missing_payload` | Missing payload → validation fails |
| `test_validate_backup_returns_false_for_nonexistent_id` | Non-existent → false |
| `test_backup_metadata_count_matches_payload` | Metadata count matches payload |
| `test_backup_ids_are_unique` | IDs unique across creations |
| `test_backup_list_deduplication` | No duplicate list entries |
| `test_backup_creation_stores_active_status` | New backup is Active |

Recommended verification commands:

```bash
cd quicklendx-contracts
cargo test test_backup_safety -- --quiet
cargo test --test backup_retention_validation -- --quiet
```
