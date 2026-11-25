//! Schema version tracking and storage migration utilities.

use soroban_sdk::{symbol_short, Env, Symbol};

use crate::errors::QuickLendXError;
use crate::types::InvoiceStatus;

// ============================================================================
// Storage Schema Version
//
// This is the authoritative version of the on-chain storage layout.  Every
// deployed contract instance stores the version it was initialised with.  The
// upgrade / migration path must:
//   1. Increment STORAGE_SCHEMA_VERSION.
//   2. Provide a migration function that brings old records to the new shape.
//   3. Emit the appropriate migration events (started / completed / failed /
//      rolled_back) so off-chain tooling can reconcile every committed action.
//
// Forward compatibility:  the contract can read records from any previous
//   version by treating unknown fields as absent / default.
// Backward compatibility:  new code must not write the new format until the
//   migration is fully committed (schema version is bumped in storage).
// ============================================================================

/// The storage schema version understood by this build of the contract.
/// Increment this constant whenever the on-chain data layout changes in a
/// backward-incompatible way.
pub const STORAGE_SCHEMA_VERSION: u32 = 1;

/// Instance-storage key that records the committed schema version.
/// **BREAKING**: renaming this key loses the version record on every deployed
/// contract and forces a full re-migration.
const SCHEMA_VERSION_KEY: Symbol = symbol_short!("sch_ver");

/// Instance-storage key for the schema version currently being migrated to
/// (set while a migration is in-progress, absent otherwise).
const MIGRATION_PENDING_VER_KEY: Symbol = symbol_short!("mig_ver");

/// Persistent-storage key for the migration progress cursor (records migrated
/// so far in the current run).
const MIGRATION_OFFSET_KEY: Symbol = symbol_short!("mig_off");

/// Persistent-storage key for the cumulative record count migrated across all
/// pages in the current run.
const MIGRATION_RECORDS_KEY: Symbol = symbol_short!("mig_rec");


pub struct StorageMigration;

impl StorageMigration {
    // ── Read-only accessors ──────────────────────────────────────────────

    /// Return the committed schema version stored in instance storage.
    /// Returns `0` if no version has been set (fresh / pre-migration contract).
    pub fn get_schema_version(env: &Env) -> u32 {
        if !env.storage().instance().has(&SCHEMA_VERSION_KEY) {
            return 0u32;
        }
        env.storage()
            .instance()
            .get(&SCHEMA_VERSION_KEY)
            .unwrap_or(0u32)
    }

    /// Return the pending migration target version, if any.
    /// `None` means no migration is currently in progress.
    pub fn get_pending_migration_version(env: &Env) -> Option<u32> {
        if !env.storage().instance().has(&MIGRATION_PENDING_VER_KEY) {
            return None;
        }
        env.storage().instance().get(&MIGRATION_PENDING_VER_KEY)
    }

    /// Return `true` when a migration is in-progress (pending version is set).
    pub fn is_migration_in_progress(env: &Env) -> bool {
        env.storage().instance().has(&MIGRATION_PENDING_VER_KEY)
    }

    /// Return the migration progress cursor (next offset to process).
    /// Returns `0` when no migration is in progress or on the first page.
    pub fn get_migration_offset(env: &Env) -> u32 {
        persistent_get(env, &MIGRATION_OFFSET_KEY).unwrap_or(0u32)
    }

    /// Return the cumulative number of records migrated so far.
    pub fn get_migration_records_migrated(env: &Env) -> u32 {
        persistent_get(env, &MIGRATION_RECORDS_KEY).unwrap_or(0u32)
    }

    // ── Write operations ─────────────────────────────────────────────────

    /// Set the committed schema version in instance storage.
    ///
    /// # Security
    /// Callers must ensure the admin has authorised this change before calling.
    pub fn set_schema_version(env: &Env, version: u32, admin: &soroban_sdk::Address) {
        env.storage().instance().set(&SCHEMA_VERSION_KEY, &version);
        crate::events::emit_schema_version_set(env, version, admin);
    }

    /// Begin a migration from `schema_from` to `schema_to`.
    ///
    /// # Errors
    /// Returns `Err(QuickLendXError::OperationNotAllowed)` if:
    /// - A migration is already in progress.
    /// - `schema_from` does not match the committed schema version.
    /// - `schema_to` is not greater than `schema_from`.
    pub fn begin_migration(
        env: &Env,
        admin: &soroban_sdk::Address,
        schema_from: u32,
        schema_to: u32,
    ) -> Result<(), crate::errors::QuickLendXError> {
        if Self::is_migration_in_progress(env) {
            return Err(crate::errors::QuickLendXError::OperationNotAllowed);
        }
        let committed = Self::get_schema_version(env);
        if committed != schema_from {
            return Err(crate::errors::QuickLendXError::OperationNotAllowed);
        }
        if schema_to <= schema_from {
            return Err(crate::errors::QuickLendXError::OperationNotAllowed);
        }
        env.storage()
            .instance()
            .set(&MIGRATION_PENDING_VER_KEY, &schema_to);
        // Reset progress counters.
        persistent_set(env, &MIGRATION_OFFSET_KEY, &0u32);
        persistent_set(env, &MIGRATION_RECORDS_KEY, &0u32);
        crate::events::emit_migration_started(env, schema_from, schema_to, admin);
        Ok(())
    }

    /// Record progress after processing one migration page.
    ///
    /// Updates the offset cursor and cumulative record count.
    /// Does NOT commit the schema version — call [`commit_migration`] when all
    /// records have been processed.
    pub fn advance_migration_page(env: &Env, new_offset: u32, records_this_page: u32) {
        persistent_set(env, &MIGRATION_OFFSET_KEY, &new_offset);
        let prior: u32 = persistent_get(env, &MIGRATION_RECORDS_KEY).unwrap_or(0);
        let total = prior.saturating_add(records_this_page);
        persistent_set(env, &MIGRATION_RECORDS_KEY, &total);
    }

    /// Commit the migration: bump the schema version and clear migration state.
    ///
    /// # Errors
    /// Returns `Err(QuickLendXError::OperationNotAllowed)` if no migration is
    /// in progress.
    pub fn commit_migration(
        env: &Env,
        admin: &soroban_sdk::Address,
    ) -> Result<(), crate::errors::QuickLendXError> {
        let schema_to = Self::get_pending_migration_version(env)
            .ok_or(crate::errors::QuickLendXError::OperationNotAllowed)?;
        let schema_from = Self::get_schema_version(env);
        let records_migrated = Self::get_migration_records_migrated(env);
        // Commit.
        env.storage()
            .instance()
            .set(&SCHEMA_VERSION_KEY, &schema_to);
        Self::clear_migration_state(env);
        crate::events::emit_migration_completed(
            env,
            schema_from,
            schema_to,
            records_migrated,
            admin,
        );
        Ok(())
    }

    /// Roll back an in-progress migration without modifying any record data.
    ///
    /// Clears migration state and emits `MigrationRolledBack`.  The caller is
    /// responsible for reverting any partial writes made during migration pages
    /// before calling this function.
    ///
    /// # Errors
    /// Returns `Err(QuickLendXError::OperationNotAllowed)` if no migration is
    /// in progress.
    pub fn rollback_migration(
        env: &Env,
        admin: &soroban_sdk::Address,
    ) -> Result<(), crate::errors::QuickLendXError> {
        let schema_to = Self::get_pending_migration_version(env)
            .ok_or(crate::errors::QuickLendXError::OperationNotAllowed)?;
        let schema_from = Self::get_schema_version(env);
        Self::clear_migration_state(env);
        crate::events::emit_migration_rolled_back(env, schema_from, schema_to, admin);
        Ok(())
    }

    /// Emit a failure event for the current page and update the progress cursor
    /// so the migration is resumable.
    ///
    /// Storage is left at the last successfully checkpointed offset; partial
    /// writes within the failed page are the caller's responsibility to avoid.
    pub fn record_migration_failure(
        env: &Env,
        records_this_page: u32,
        next_offset: u32,
        reason: &str,
    ) {
        let schema_from = Self::get_schema_version(env);
        let schema_to = Self::get_pending_migration_version(env).unwrap_or(schema_from);
        Self::advance_migration_page(env, next_offset, records_this_page);
        let total_migrated = Self::get_migration_records_migrated(env);
        crate::events::emit_migration_failed(
            env,
            schema_from,
            schema_to,
            total_migrated,
            next_offset,
            reason,
        );
    }

    // ── Internal helpers ─────────────────────────────────────────────────

    fn clear_migration_state(env: &Env) {
        env.storage().instance().remove(&MIGRATION_PENDING_VER_KEY);
        env.storage().persistent().remove(&MIGRATION_OFFSET_KEY);
        env.storage().persistent().remove(&MIGRATION_RECORDS_KEY);
    }
}

/// Per-call cache for storage reads.
///
/// Caches a single `Invoice` lookup so that repeated reads of the same key
/// within a contract invocation return the cached value and skip redundant
/// host interface calls and duplicate TTL extensions.
