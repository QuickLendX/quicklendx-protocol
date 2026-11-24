# Backup Retention Policy - Quick Start Guide

## TL;DR

Configure backup retention to prevent storage exhaustion:

```rust
// Keep last 10 backups
client.set_backup_retention_policy(&10, &0, &true);

// Keep backups for 30 days
client.set_backup_retention_policy(&0, &2592000, &true);

// Combined: 5 backups OR 7 days
client.set_backup_retention_policy(&5, &604800, &true);
```

## Default Behavior

Without configuration, the system uses:
- **max_backups**: 5
- **max_age_seconds**: 0 (unlimited)
- **auto_cleanup_enabled**: true

This means: **Keep last 5 backups, no age limit, automatic cleanup enabled.**

## Common Scenarios

### Scenario 1: Production System

Keep 10 backups or 30 days (whichever is more restrictive):

```rust
let admin = Address::generate(&env);
env.mock_all_auths();
client.set_admin(&admin);

// Configure retention
client.set_backup_retention_policy(&10, &2592000, &true);

// Create backups normally
let backup_id = client.create_backup(&String::from_str(&env, "Daily backup"));
// Automatic cleanup happens here
```

### Scenario 2: Development/Testing

Unlimited backups, 7-day age limit:

```rust
client.set_backup_retention_policy(&0, &604800, &true);
```

### Scenario 3: High-Volume System

Keep only 3 most recent backups:

```rust
client.set_backup_retention_policy(&3, &0, &true);
```

### Scenario 4: Manual Control

Disable automatic cleanup, trigger manually:

```rust
// Disable auto cleanup
client.set_backup_retention_policy(&5, &0, &false);

// Create multiple backups
for i in 0..10 {
    client.create_backup(&String::from_str(&env, "Backup"));
}

// Manually trigger cleanup when ready
client.set_backup_retention_policy(&5, &0, &true);
let removed = client.cleanup_backups(); // Returns number removed
```

### Scenario 5: Protect Important Backups

Archive backups to protect from automatic cleanup:

```rust
// Create backup before major operation
let backup_id = client.create_backup(&String::from_str(&env, "Pre-upgrade v2.0"));

// Archive to protect from cleanup
client.archive_backup(&backup_id);

// This backup will never be automatically cleaned up
```

## API Quick Reference

### Configure Retention

```rust
pub fn set_backup_retention_policy(
    env: Env,
    max_backups: u32,        // 0 = unlimited
    max_age_seconds: u64,    // 0 = unlimited
    auto_cleanup_enabled: bool,
) -> Result<(), QuickLendXError>
```

**Requires:** Admin authorization

### Query Policy

```rust
pub fn get_backup_retention_policy(env: Env) -> BackupRetentionPolicy
```

**Returns:**
```rust
BackupRetentionPolicy {
    max_backups: u32,
    max_age_seconds: u64,
    auto_cleanup_enabled: bool,
}
```

### Manual Cleanup

```rust
pub fn cleanup_backups(env: Env) -> Result<u32, QuickLendXError>
```

**Requires:** Admin authorization  
**Returns:** Number of backups removed

## Time Conversions

```rust
// Common time periods in seconds
let one_hour = 3600;
let one_day = 86400;
let one_week = 604800;
let one_month = 2592000;  // 30 days
let three_months = 7776000;  // 90 days
```

## Cleanup Behavior

### What Gets Cleaned

- ✅ Active backups older than `max_age_seconds` (if configured)
- ✅ Oldest active backups beyond `max_backups` (if configured)

### What's Protected

- ❌ Archived backups (never cleaned automatically)
- ❌ Backups within age limit
- ❌ Backups within count limit

### When Cleanup Runs

1. **Automatically**: After each `create_backup()` call (if `auto_cleanup_enabled = true`)
2. **Manually**: When admin calls `cleanup_backups()`

## Events

Monitor these events for audit trail:

```rust
// Policy updated
(symbol_short!("ret_pol"), max_backups, max_age_seconds, auto_cleanup_enabled, timestamp)

// Backups cleaned
(symbol_short!("bkup_cln"), removed_count, timestamp)
```

## Error Handling

```rust
match client.try_set_backup_retention_policy(&10, &0, &true) {
    Ok(_) => {
        // Policy updated successfully
    }
    Err(QuickLendXError::NotAdmin) => {
        // Caller is not admin
    }
    Err(e) => {
        // Other error
    }
}
```

## Best Practices

### ✅ DO

- Set reasonable limits based on storage capacity
- Archive important backups before major operations
- Monitor backup count regularly
- Test restoration procedures
- Document backup IDs for critical backups

### ❌ DON'T

- Set unlimited retention without monitoring
- Disable cleanup without a plan
- Forget to archive pre-upgrade backups
- Ignore cleanup events
- Assume backups are always valid (validate first)

## Testing

```rust
#[test]
fn test_my_retention_policy() {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    // Set up admin
    let admin = Address::generate(&env);
    env.mock_all_auths();
    client.set_admin(&admin);

    // Configure retention
    client.set_backup_retention_policy(&3, &0, &true);

    // Create backups
    for i in 0..5 {
        client.create_backup(&String::from_str(&env, "Backup"));
    }

    // Verify only 3 remain
    let backups = client.get_backups();
    assert_eq!(backups.len(), 3);
}
```

## Troubleshooting

### Problem: Backups not being cleaned up

**Check:**
1. Is `auto_cleanup_enabled = true`?
2. Are backups archived?
3. Are limits configured (not 0)?

```rust
let policy = client.get_backup_retention_policy();
println!("max_backups: {}", policy.max_backups);
println!("max_age_seconds: {}", policy.max_age_seconds);
println!("auto_cleanup_enabled: {}", policy.auto_cleanup_enabled);
```

### Problem: Too many backups being cleaned

**Solution:** Adjust limits or archive important backups

```rust
// Increase limits
client.set_backup_retention_policy(&20, &0, &true);

// Or archive specific backups
client.archive_backup(&important_backup_id);
```

### Problem: Cannot configure retention policy

**Check:** Are you admin?

```rust
let admin = client.get_admin();
// Verify you're using the correct admin address
```

## Migration from Old System

If you have existing code using the old cleanup function:

```rust
// Old way (deprecated)
BackupStorage::cleanup_old_backups(&env, 5)?;

// New way
client.set_backup_retention_policy(&5, &0, &true);
// Cleanup happens automatically on create_backup()
// Or manually:
client.cleanup_backups()?;
```

## Performance Tips

1. **Batch Operations**: If creating many backups, disable auto-cleanup and trigger manually:
   ```rust
   client.set_backup_retention_policy(&5, &0, &false);
   // Create many backups...
   client.set_backup_retention_policy(&5, &0, &true);
   client.cleanup_backups()?;
   ```

2. **Monitor Counts**: Check backup count before creating new ones:
   ```rust
   let count = client.get_backups().len();
   if count > 100 {
       // Consider manual cleanup or adjusting policy
   }
   ```

3. **Archive Strategically**: Archive only truly critical backups to avoid accumulation

## Security Notes

- Only admin can configure retention policy
- Only admin can trigger manual cleanup
- Archived backups are protected from automatic cleanup
- All operations emit events for audit trail
- Validation occurs before backup restoration

## Further Reading

- **Full Documentation**: `docs/contracts/backup.md`
- **Implementation Details**: `BACKUP_RETENTION_IMPLEMENTATION.md`
- **Security Analysis**: `BACKUP_RETENTION_SECURITY.md`
- **Feature Summary**: `FEATURE_331_SUMMARY.md`

## Support

For issues or questions:
1. Check the full documentation
2. Review test cases in `src/test.rs`
3. Examine event logs for audit trail
4. Verify admin authorization

---

**Quick Start Version:** 1.0  
**Last Updated:** February 23, 2024
