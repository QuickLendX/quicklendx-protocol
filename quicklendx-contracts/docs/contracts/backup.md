# Backup and Restore System

## Overview

The QuickLendX protocol implements a comprehensive data backup and restore mechanism to ensure data safety, resilience, and fast disaster recovery. The `BackupStorage` and `QuickLendXContract` administrative functions allow authorized administrators to safely checkpoint the state of the protocol's invoices, validate backups, and restore them if necessary.

## Operations

### Creating Backups

Backups are created using the `create_backup` administrative endpoint. This process securely bundles all active, paid, funded, defaulted, and cancelled invoices.

- **Storage Optimization:** The protocol automatically ensures that a maximum of 5 active backups are maintained to minimize state bloat. Old backups are continuously pruned on every successful backup creation execution.

### Validating Backups

An administrator can at any point run `validate_backup` to ensure that a given backup ID maintains its data integrity. A corrupted backup is permanently marked as `Corrupted` to prevent accidental restoration.

### Restoring Backups

The `restore_backup` command is highly privileged and destructive to the active state. When invoked:
1. It validates the integrity of the requested backup.
2. It permanently clears all currently tracked invoice data, wiping the existing database indexes (including business mapping, tags, metadata, and status associations).
3. It reconstitutes the original invoice state from the backup payload.

### Archiving Backups

Backups that are older or explicitly meant for off-chain storage can be marked as archived using the `archive_backup` function. This cleanly removes them from the active list of 5 rolling backups and stops them from being accidentally overwritten by the rolling buffer.

## Security Considerations

1. **Destructive Execution:** `restore_backup` aggressively purges the current application state. Any invoices created since the backup will be lost. This function should only be invoked under extraordinary circumstances when the active ledger is irrecoverably compromised.
2. **Access Control:** All backup endpoints exclusively require `admin.require_auth()`. Regular business operators and investors cannot induce backups, manipulate states, or compromise other users' data.
3. **Immutability of Backups:** Once a backup is created, it natively inherits the Soroban immutability guarantees. Backup data payloads cannot be covertly edited to spoof invoice amounts or investor details prior to a restore operation.

## Test Coverage

Backup behavior is covered by contract tests in `src/test_backup.rs`, including:

- **`create_backup`**: admin-only enforcement and exact `invoice_count` capture.
- **`restore_backup`**: admin-only enforcement plus clear-and-restore state behavior.
- **`validate_backup`**: valid backup path and corruption path (metadata mismatch marks backup as `Corrupted`).
- **`archive_backup`**: admin-only enforcement and active-list removal behavior.
- **Backup listing/cleanup**: active backup ordering and retention policy (only latest 5 remain active).

Run backup-focused tests with:

```bash
cargo test test_backup -- --nocapture
```
