#![allow(deprecated)]

use soroban_sdk::{contractevent, Address, Env, String};

/// Emitted when the contract records a new storage schema version.
///
/// # Fields
/// - `schema_version` – The new schema version number.
/// - `set_by`         – The admin who triggered the version bump.
/// - `timestamp`      – Ledger timestamp at emission time.
///
/// # Compatibility
/// This event is additive.  Indexers that only understand earlier versions
/// can safely ignore the `schema_version` payload.
#[derive(Debug, PartialEq)]
#[contractevent]
pub struct SchemaVersionSet {
    pub schema_version: u32,
    pub set_by: Address,
    pub timestamp: u64,
}

/// Emitted when a paginated schema migration begins (first page of a run).
///
/// # Fields
/// - `schema_from`    – Version being migrated away from.
/// - `schema_to`      – Version being migrated to.
/// - `initiated_by`   – Admin who started the migration.
/// - `timestamp`      – Ledger timestamp at emission time.
///
/// # Design invariant
/// Only one migration may be in progress at a time.  If a `MigrationStarted`
/// event is observed without a subsequent `MigrationCompleted` or
/// `MigrationRolledBack`, the migration is considered "in progress" and
/// writes to migrated entities must be rejected until it finishes.
#[derive(Debug, PartialEq)]
#[contractevent]
pub struct MigrationStarted {
    pub schema_from: u32,
    pub schema_to: u32,
    pub initiated_by: Address,
    pub timestamp: u64,
}

/// Emitted when every record has been migrated and the schema version is
/// committed to the new value.
///
/// # Fields
/// - `schema_from`      – The old schema version.
/// - `schema_to`        – The new, committed schema version.
/// - `records_migrated` – Total number of records processed.
/// - `completed_by`     – Admin who committed the final page.
/// - `timestamp`        – Ledger timestamp at emission time.
#[derive(Debug, PartialEq)]
#[contractevent]
pub struct MigrationCompleted {
    pub schema_from: u32,
    pub schema_to: u32,
    pub records_migrated: u32,
    pub completed_by: Address,
    pub timestamp: u64,
}

/// Emitted when an in-progress migration is explicitly rolled back.
///
/// After this event the schema version is restored to `schema_from` and
/// storage is guaranteed to contain no partial new-schema records.
///
/// # Fields
/// - `schema_from`      – The version rolled back to.
/// - `schema_to`        – The version that was being migrated to.
/// - `rolled_back_by`   – Admin who performed the rollback.
/// - `timestamp`        – Ledger timestamp at emission time.
#[derive(Debug, PartialEq)]
#[contractevent]
pub struct MigrationRolledBack {
    pub schema_from: u32,
    pub schema_to: u32,
    pub rolled_back_by: Address,
    pub timestamp: u64,
}

/// Emitted when a migration page fails partway through.
///
/// The migration is resumable: pass `next_offset` as the starting offset
/// for the next invocation.
///
/// # Fields
/// - `schema_from`      – Version being migrated from.
/// - `schema_to`        – Version being migrated to.
/// - `records_migrated` – Number of records successfully migrated so far.
/// - `next_offset`      – Offset to resume from on the next call.
/// - `reason`           – Machine-readable failure label (no PII).
/// - `timestamp`        – Ledger timestamp at emission time.
#[derive(Debug, PartialEq)]
#[contractevent]
pub struct MigrationFailed {
    pub schema_from: u32,
    pub schema_to: u32,
    pub records_migrated: u32,
    pub next_offset: u32,
    pub reason: String,
    pub timestamp: u64,
}

// ── Emitter helpers ──────────────────────────────────────────────────────────

/// Emit a [`SchemaVersionSet`] event.
pub fn emit_schema_version_set(env: &Env, schema_version: u32, set_by: &Address) {
    SchemaVersionSet {
        schema_version,
        set_by: set_by.clone(),
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}

/// Emit a [`MigrationStarted`] event.
pub fn emit_migration_started(env: &Env, schema_from: u32, schema_to: u32, initiated_by: &Address) {
    MigrationStarted {
        schema_from,
        schema_to,
        initiated_by: initiated_by.clone(),
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}

/// Emit a [`MigrationCompleted`] event.
pub fn emit_migration_completed(
    env: &Env,
    schema_from: u32,
    schema_to: u32,
    records_migrated: u32,
    completed_by: &Address,
) {
    MigrationCompleted {
        schema_from,
        schema_to,
        records_migrated,
        completed_by: completed_by.clone(),
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}

/// Emit a [`MigrationRolledBack`] event.
pub fn emit_migration_rolled_back(
    env: &Env,
    schema_from: u32,
    schema_to: u32,
    rolled_back_by: &Address,
) {
    MigrationRolledBack {
        schema_from,
        schema_to,
        rolled_back_by: rolled_back_by.clone(),
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}

/// Emit a [`MigrationFailed`] event.
pub fn emit_migration_failed(
    env: &Env,
    schema_from: u32,
    schema_to: u32,
    records_migrated: u32,
    next_offset: u32,
    reason: &str,
) {
    MigrationFailed {
        schema_from,
        schema_to,
        records_migrated,
        next_offset,
        reason: String::from_str(env, reason),
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}
