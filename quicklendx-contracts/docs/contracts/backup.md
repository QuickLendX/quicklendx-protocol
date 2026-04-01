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

Additionally, restoring backups follows this sequence:

Step 1  validate_backup(backup_id)
        ──────────────────────────
        Full integrity check performed BEFORE any mutation. Checks:
        - Backup record exists and has a valid ID prefix (0xB4 0xC4).
        - Backup description is non-empty and within length limits.
        - Invoice payload exists in storage.
        - Payload length matches backup.invoice_count.
        - Every invoice in the payload has amount > 0.

        If validation fails → return error, no storage is mutated.

Step 2  InvoiceStorage::clear_all(env)
        ────────────────────────────────
        Atomically wipes every invoice and every secondary index.
        After this step, instance storage contains no invoice data.

        ⚠ There is no rollback on a Soroban ledger. Reaching step 2
        means the caller has committed to discarding the current state.
        Always take a backup before clearing.

Step 3  InvoiceStorage::store_invoice(env, &invoice) per invoice
        ────────────────────────────────────────────────────────
        Re-registers each invoice from the backup payload, rebuilding
        all secondary indexes from scratch. The order of individual
        invoices within this step does not matter.

Step 4  Mark backup as Archived
        ─────────────────────────
        Sets backup.status = BackupStatus::Archived to prevent the same
        backup from being restored twice. Restoring the same backup twice
        without clearing in between would cause duplicate index entries.

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
