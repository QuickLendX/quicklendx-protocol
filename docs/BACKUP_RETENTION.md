# Backup Retention Policy

**Audience:** Operator — deploying and maintaining the protocol on a Soroban network.

The retention policy controls how many active backups are kept and for how long. It prevents storage bloat from unbounded backup accumulation and ensures that restore candidates are recent enough to be useful. For the full contract backup API, see [`docs/contracts/backup.md`](contracts/backup.md); for format versioning, see [`docs/backup-format.md`](backup-format.md).

---

## Policy Structure

```rust
pub struct BackupRetentionPolicy {
    pub max_backups: u32,
    pub max_age_seconds: u64,
    pub auto_cleanup_enabled: bool,
}
```

| Field | Type | Default | Description |
| :--- | :--- | :--- | :--- |
| `max_backups` | `u32` | `5` | Maximum number of active backups to retain. `0` = unlimited. |
| `max_age_seconds` | `u64` | `0` | Maximum age of an active backup in seconds. `0` = unlimited. |
| `auto_cleanup_enabled` | `bool` | `true` | Whether cleanup runs automatically after each `create_backup` call. |

The default policy keeps the 5 most recent active backups indefinitely with auto-cleanup on.

---

## Configuring the Policy

### Set a custom policy

```bash
# Keep at most 10 backups, delete any older than 30 days, auto-cleanup on.
stellar contract invoke \
  --id <CONTRACT_ID> \
  --source <ADMIN_ACCOUNT> \
  --network <NETWORK> \
  -- set_backup_retention_policy \
  --admin <ADMIN_ADDRESS> \
  --max_backups 10 \
  --max_age_seconds 2592000 \
  --enabled true
```

Disable automatic cleanup and rely on manual triggers:

```bash
# Keep at most 3 backups, no age limit, manual cleanup only.
stellar contract invoke \
  --id <CONTRACT_ID> \
  --source <ADMIN_ACCOUNT> \
  --network <NETWORK> \
  -- set_backup_retention_policy \
  --admin <ADMIN_ADDRESS> \
  --max_backups 3 \
  --max_age_seconds 0 \
  --enabled false
```

### Read the current policy

```bash
stellar contract invoke \
  --id <CONTRACT_ID> \
  --source <READONLY_ACCOUNT> \
  --network <NETWORK> \
  -- get_backup_retention_policy
```

Expected output (default):

```json
{
  "max_backups": 5,
  "max_age_seconds": 0,
  "auto_cleanup_enabled": true
}
```

---

## How Cleanup Works

The cleanup algorithm runs in two phases and applies **both** thresholds when set:

### Phase 1: Age-Based Eviction

If `max_age_seconds > 0`, every active backup whose age exceeds the limit is purged. Age is computed as `current_ledger_timestamp - backup.timestamp`.

### Phase 2: Count-Based Eviction

If `max_backups > 0`, the remaining active backups are sorted oldest-first. The oldest are removed until the count is ≤ `max_backups`.

**Key rules:**
- Only backups with `status == Active` are considered. `Archived` and `Corrupted` backups are never touched by cleanup.
- Both phases run sequentially; a backup can be removed by either rule.
- When `auto_cleanup_enabled` is `true`, cleanup runs automatically inside `create_backup` after the new backup is stored.
- Manual cleanup via `cleanup_backups` (admin-only) always applies the current policy regardless of the `auto_cleanup_enabled` flag.

---

## Verifying Integrity

### Validate a backup

```bash
stellar contract invoke \
  --id <CONTRACT_ID> \
  --source <READONLY_ACCOUNT> \
  --network <NETWORK> \
  -- validate_backup \
  --backup_id <BACKUP_ID_HEX>
```

Returns `true` only when metadata exists, the payload is present, `invoice_count` matches, and every invoice has `amount > 0`.

### List active backups

```bash
stellar contract invoke \
  --id <CONTRACT_ID> \
  --source <READONLY_ACCOUNT> \
  --network <NETWORK> \
  -- get_backups
```

### Inspect a single backup

```bash
stellar contract invoke \
  --id <CONTRACT_ID> \
  --source <READONLY_ACCOUNT> \
  --network <NETWORK> \
  -- get_backup_details \
  --backup_id <BACKUP_ID_HEX>
```

Expected output:

```json
{
  "backup_id": "...",
  "timestamp": 1719000000,
  "description": "Automatic Backup",
  "invoice_count": 42,
  "status": "Active",
  "format_version": 2
}
```

### Audit cleanup events

Each cleanup emits a `BackupsCleaned` event (`bkup_cln`) with the number of purged backups. Monitor these in the event stream to verify retention is working as expected.

---

## Operational Examples

### Scenario: Default policy, 6 backups created

With the default policy (`max_backups: 5, auto_cleanup_enabled: true`):

1. Create backups 1–5 → all stay `Active`, list size = 5.
2. Create backup 6 → after the new backup is stored, cleanup runs and evicts the oldest (backup 1). Active list = [2, 3, 4, 5, 6].

### Scenario: Manual cleanup after policy change

```bash
# Step 1: tighten policy to keep only 2 most recent
stellar contract invoke \
  --id <CONTRACT_ID> \
  --source <ADMIN_ACCOUNT> \
  --network <NETWORK> \
  -- set_backup_retention_policy \
  --admin <ADMIN_ADDRESS> \
  --max_backups 2 \
  --max_age_seconds 0 \
  --enabled false

# Step 2: manually enforce the new policy
stellar contract invoke \
  --id <CONTRACT_ID> \
  --source <ADMIN_ACCOUNT> \
  --network <NETWORK> \
  -- cleanup_backups \
  --admin <ADMIN_ADDRESS>
```

The returned `u32` is the number of backups removed.

---

## Edge Cases

| Condition | Behaviour |
| :--- | :--- |
| `max_backups = 0` | No count limit; only age-based eviction applies (if `max_age_seconds > 0`). |
| `max_age_seconds = 0` | No age limit; only count-based eviction applies (if `max_backups > 0`). |
| `auto_cleanup_enabled = false` | Cleanup never runs automatically; call `cleanup_backups` manually. |
| Archived backups | Never evicted. Use `archive_backup` to preserve a backup indefinitely. |
| Corrupted backups | Never evicted by cleanup (removed by admin action or restore rejection). |

---

## Related Documents

- [`docs/contracts/backup.md`](contracts/backup.md) — Full contract API: create, restore, archive, validate, events.
- [`docs/backup-format.md`](backup-format.md) — Format versioning and compatibility (v1 → v2 migration).
- [`docs/RUNBOOK_INCIDENT_RESPONSE.md`](RUNBOOK_INCIDENT_RESPONSE.md) — Incident-mode recovery playbook.
