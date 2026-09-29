# Backup Format Versioning and Compatibility

To ensure data integrity and prevent schema changes from silently corrupting contract restores, the QuickLendX protocol implements explicit backup format versioning.

## format_version Field

Each backup contains a `format_version` field, stored within the `Backup` metadata struct:

* **format_version**: `u32`
  * Represents the schema version of the stored backup structure and the associated invoice data payload.

---

## Compatibility Matrix

The contract defines strict restore compatibility rules:

| Source Format | Current Format | Restorable? | Handling Mechanism |
| :--- | :--- | :--- | :--- |
| **v1** (Legacy) | **v2** | **Yes** | Upgraded on-the-fly via adapter (`BackupV1` -> `Backup` v2) |
| **v2** (Current)| **v2** | **Yes** | Restored normally |
| **v3** (Future) | **v2** | **No** | Rejected with `BackupVersionUnsupported` |
| **v4+** (Future)| **v2** | **No** | Rejected with `BackupVersionUnsupported` |

---

## Upgrade Path

When restoring or retrieving a backup, the contract evaluates the format version:

1. **V1 Format Detection**: If the backup metadata map in storage lacks the `format_version` field (or has version `1`), the contract identifies it as a legacy `v1` backup.
2. **On-the-fly Translation**: The stored payload is deserialized into the legacy `BackupV1` struct and converted via the adapter into a `Backup` (v2) struct.
3. **Execution**: The restore process clears existing invoice state, registers the restored invoices, and marks the backup as `Archived`.

---

## Forward-Compatibility and Protection

* **Best-Effort Decoding Prohibited**: The contract will not attempt to decode or restore backup payloads containing unknown future versions (v3 and newer).
* **Deterministic Failures**: Any validation or restore attempt on unsupported versions immediately returns `QuickLendXError::BackupVersionUnsupported`.
* **Malformed Payloads**: Truncated or malformed payloads that fail structural deserialization fail safely with `QuickLendXError::StorageError` or `QuickLendXError::StorageKeyNotFound`.

---

## See also

For how `format_version` fits alongside the protocol version and analytics
schema version — and the overall rules for which contract versions interoperate
— see [CONTRACT_VERSION_COMPATIBILITY.md](CONTRACT_VERSION_COMPATIBILITY.md).
