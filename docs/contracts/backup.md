# Backup System

## Overview

The QuickLendX backup system provides robust data protection and recovery capabilities for invoice data. It includes configurable retention policies to prevent unbounded storage growth while maintaining critical historical data.

## Features

- **Backup Creation**: Create point-in-time snapshots of all invoice data
- **Backup Restoration**: Restore invoice data from any valid backup
- **Backup Validation**: Verify backup integrity before restoration
- **Backup Archival**: Mark backups as archived for long-term storage
- **Configurable Retention**: Flexible retention policies by count and age
- **Automatic Cleanup**: Automated removal of old backups based on policy
- **Manual Cleanup**: Admin-triggered cleanup on demand

## Data Structures

### Backup

```rust
pub struct Backup {
    pub backup_id: BytesN<32>,      // Unique backup identifier
    pub timestamp: u64,              // Creation timestamp
    pub description: String,         // Human-readable description
    pub invoice_count: u32,          // Number of invoices in backup
    pub status: BackupStatus,        // Current backup status
}
```

### BackupStatus

```rust
pub enum BackupStatus {
    Active,      // Backup is active and available
    Archived,    // Backup is archived (not in active list)
    Corrupted,   // Backup failed validation
}
```

### BackupRetentionPolicy

```rust
pub struct BackupRetentionPolicy {
    pub max_backups: u32,           // Maximum number of backups (0 = unlimited)
    pub max_age_seconds: u64,       // Maximum age in seconds (0 = unlimited)
    pub auto_cleanup_enabled: bool, // Enable automatic cleanup
}
```

**Default Policy:**
- `max_backups`: 5
- `max_age_seconds`: 0 (unlimited)
- `auto_cleanup_enabled`: true

## Core Functions

### create_backup

Creates a new backup of all invoice data.

```rust
pub fn create_backup(env: Env, description: String) -> Result<BytesN<32>, QuickLendXError>
```

**Parameters:**
- `description`: Human-readable description of the backup

**Returns:**
- `Ok(BytesN<32>)`: Unique backup ID
- `Err(QuickLendXError::NotAdmin)`: Caller is not admin

**Behavior:**
1. Requires admin authorization
2. Collects all invoices from all statuses
3. Generates unique backup ID
4. Stores backup metadata and invoice data
5. Adds backup to active list
6. Triggers automatic cleanup based on retention policy
7. Emits `bkup_crt` event

**Security:**
- Admin-only operation
- Automatic cleanup prevents storage exhaustion

### restore_backup

Restores invoice data from a backup.

```rust
pub fn restore_backup(env: Env, backup_id: BytesN<32>) -> Result<(), QuickLendXError>
```

**Parameters:**
- `backup_id`: ID of the backup to restore

**Returns:**
- `Ok(())`: Restoration successful
- `Err(QuickLendXError::NotAdmin)`: Caller is not admin
- `Err(QuickLendXError::StorageKeyNotFound)`: Backup not found
- `Err(QuickLendXError::StorageError)`: Backup validation failed

**Behavior:**
1. Requires admin authorization
2. Validates backup integrity
3. Clears all current invoice data
4. Restores invoices from backup
5. Emits `bkup_rstr` event

**Security:**
- Admin-only operation
- Validates backup before restoration
- Destructive operation (clears current data)

### validate_backup

Validates backup integrity.

```rust
pub fn validate_backup(env: Env, backup_id: BytesN<32>) -> Result<bool, QuickLendXError>
```

**Parameters:**
- `backup_id`: ID of the backup to validate

**Returns:**
- `Ok(true)`: Backup is valid
- `Ok(false)`: Backup is corrupted

**Validation Checks:**
1. Backup metadata exists
2. Backup data exists
3. Invoice count matches actual data
4. All invoices have valid amounts (> 0)

### archive_backup

Archives a backup (removes from active list).

```rust
pub fn archive_backup(env: Env, backup_id: BytesN<32>) -> Result<(), QuickLendXError>
```

**Parameters:**
- `backup_id`: ID of the backup to archive

**Returns:**
- `Ok(())`: Archive successful
- `Err(QuickLendXError::NotAdmin)`: Caller is not admin
- `Err(QuickLendXError::StorageKeyNotFound)`: Backup not found

**Behavior:**
1. Requires admin authorization
2. Updates backup status to `Archived`
3. Removes from active backup list
4. Backup data remains accessible
5. Emits `bkup_ar` event

**Note:** Archived backups are not subject to automatic cleanup.

## Retention Policy Functions

### set_backup_retention_policy

Configures the backup retention policy.

```rust
pub fn set_backup_retention_policy(
    env: Env,
    max_backups: u32,
    max_age_seconds: u64,
    auto_cleanup_enabled: bool,
) -> Result<(), QuickLendXError>
```

**Parameters:**
- `max_backups`: Maximum number of active backups (0 = unlimited)
- `max_age_seconds`: Maximum age of backups in seconds (0 = unlimited)
- `auto_cleanup_enabled`: Enable automatic cleanup on backup creation

**Returns:**
- `Ok(())`: Policy updated successfully
- `Err(QuickLendXError::NotAdmin)`: Caller is not admin

**Behavior:**
1. Requires admin authorization
2. Updates retention policy configuration
3. Emits `ret_pol` event
4. Does not trigger immediate cleanup

**Examples:**

Keep last 10 backups:
```rust
client.set_backup_retention_policy(&10, &0, &true);
```

Keep backups for 30 days:
```rust
client.set_backup_retention_policy(&0, &2592000, &true); // 30 days in seconds
```

Keep last 5 backups OR 7 days (whichever is more restrictive):
```rust
client.set_backup_retention_policy(&5, &604800, &true); // 7 days
```

Disable automatic cleanup:
```rust
client.set_backup_retention_policy(&0, &0, &false);
```

### get_backup_retention_policy

Retrieves the current retention policy.

```rust
pub fn get_backup_retention_policy(env: Env) -> BackupRetentionPolicy
```

**Returns:**
- Current retention policy configuration

### cleanup_backups

Manually triggers backup cleanup.

```rust
pub fn cleanup_backups(env: Env) -> Result<u32, QuickLendXError>
```

**Returns:**
- `Ok(u32)`: Number of backups removed
- `Err(QuickLendXError::NotAdmin)`: Caller is not admin

**Behavior:**
1. Requires admin authorization
2. Applies current retention policy
3. Removes old backups by age (if configured)
4. Removes oldest backups by count (if configured)
5. Emits `bkup_cln` event
6. Returns count of removed backups

**Note:** Only affects active backups; archived backups are preserved.

## Query Functions

### get_backups

Returns list of all active backup IDs.

```rust
pub fn get_backups(env: Env) -> Vec<BytesN<32>>
```

**Returns:**
- Vector of active backup IDs (excludes archived backups)

### get_backup_details

Retrieves detailed information about a backup.

```rust
pub fn get_backup_details(env: Env, backup_id: BytesN<32>) -> Option<Backup>
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

## Security Considerations

### Access Control

- All backup operations require admin authorization
- Only admin can create, restore, archive, and configure retention
- Query operations are public (read-only)

### Data Integrity

- Backups are validated before restoration
- Corrupted backups cannot be restored
- Validation checks invoice count and data consistency

### Storage Management

- Retention policies prevent unbounded storage growth
- Automatic cleanup runs on backup creation
- Manual cleanup available for immediate action
- Archived backups are protected from automatic cleanup

### Audit Trail

- All backup operations emit events
- Events include timestamps for audit purposes
- Retention policy changes are logged

## Best Practices

### Retention Policy Configuration

1. **Production Systems:**
   - Keep at least 5-10 backups
   - Set age limit to 30-90 days
   - Enable automatic cleanup

2. **Development/Testing:**
   - Unlimited backups (max_backups = 0)
   - Shorter age limits (7 days)
   - Manual cleanup as needed

3. **High-Volume Systems:**
   - Lower backup count (3-5)
   - Shorter age limits (7-14 days)
   - Regular archival of important backups

### Backup Strategy

1. **Regular Backups:**
   - Create backups before major operations
   - Schedule periodic backups (daily/weekly)
   - Document backup descriptions clearly

2. **Archive Important Backups:**
   - Archive backups before major upgrades
   - Archive end-of-period backups
   - Archived backups are protected from cleanup

3. **Validation:**
   - Validate backups after creation
   - Test restoration in non-production environment
   - Monitor backup events for failures

### Recovery Planning

1. **Test Restorations:**
   - Regularly test backup restoration
   - Verify data integrity after restoration
   - Document restoration procedures

2. **Backup Verification:**
   - Validate backups periodically
   - Monitor for corrupted backups
   - Remove corrupted backups promptly

3. **Disaster Recovery:**
   - Maintain off-chain backup copies
   - Document backup IDs and timestamps
   - Plan for complete data loss scenarios

## Example Workflows

### Daily Backup with 7-Day Retention

```rust
// Configure retention policy (once)
client.set_backup_retention_policy(&0, &604800, &true); // 7 days

// Create daily backup
let backup_id = client.create_backup(&String::from_str(&env, "Daily backup 2024-02-23"));
// Automatic cleanup removes backups older than 7 days
```

### Pre-Upgrade Backup

```rust
// Create backup before upgrade
let backup_id = client.create_backup(&String::from_str(&env, "Pre-upgrade v2.0"));

// Archive to protect from cleanup
client.archive_backup(&backup_id);

// Perform upgrade...

// If needed, restore from backup
client.restore_backup(&backup_id);
```

### Manual Cleanup

```rust
// Disable automatic cleanup temporarily
client.set_backup_retention_policy(&5, &0, &false);

// Create multiple backups without cleanup
for i in 0..10 {
    client.create_backup(&String::from_str(&env, "Backup"));
}

// Re-enable and trigger cleanup
client.set_backup_retention_policy(&5, &0, &true);
let removed = client.cleanup_backups(); // Removes 5 oldest backups
```

## Limitations

1. **Storage Constraints:**
   - Backups consume contract storage
   - Large invoice datasets may hit storage limits
   - Consider off-chain backup solutions for large datasets

2. **Performance:**
   - Backup creation time scales with invoice count
   - Restoration is a destructive operation
   - Cleanup algorithm is O(n²) for sorting (acceptable for small n)

3. **Scope:**
   - Only backs up invoice data
   - Does not backup bids, investments, or other data
   - Future versions may expand backup scope

## Future Enhancements

- Incremental backups (delta backups)
- Backup compression
- Off-chain backup integration
- Backup encryption
- Selective restoration (restore specific invoices)
- Backup of additional data types (bids, investments)
- Parallel backup creation for large datasets
