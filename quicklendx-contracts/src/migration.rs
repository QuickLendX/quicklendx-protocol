//! Resumable storage migration runner with dry-run and per-version records.
//!
//! Coordinates bounded, offset-based schema migrations on top of
//! [`crate::storage::StorageMigration`], enforcing admin authorization,
//! single-active-run exclusivity, `protocol_limits::MAX_MIGRATION_STEP_LIMIT`
//! step ceilings, read-only dry-run previews, and per-version completion records.

use soroban_sdk::{contracttype, symbol_short, Address, Env, Symbol, Vec};

use crate::admin::AdminStorage;
use crate::backup::BackupStorage;
use crate::errors::QuickLendXError;
use crate::events::emit_migration_step_executed;
use crate::protocol_limits::MAX_MIGRATION_STEP_LIMIT;
use crate::storage::{InvoiceStorage, StorageMigration, STORAGE_SCHEMA_VERSION};

/// Instance-storage key for the timestamp when the active migration was planned.
const MIGRATION_STARTED_AT_KEY: Symbol = symbol_short!("mig_st_ts");

/// Instance-storage key for the admin who planned the active migration.
const MIGRATION_INITIATOR_KEY: Symbol = symbol_short!("mig_init");

/// Persistent-storage key prefix for per-version migration completion records.
const MIGRATION_VER_RECORD_PREFIX: Symbol = symbol_short!("mig_vrec");

/// Specification for a registered schema migration transition.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MigrationStepSpec {
    pub from_version: u32,
    pub to_version: u32,
    pub total_records: u32,
}

/// Persisted audit record for a completed schema version migration.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MigrationVersionRecord {
    pub schema_from: u32,
    pub schema_to: u32,
    pub records_migrated: u32,
    pub started_at: u64,
    pub completed_at: u64,
    pub initiated_by: Address,
    pub completed_by: Address,
}

/// Result returned by [`MigrationRunner::plan`].
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MigrationPlan {
    pub schema_from: u32,
    pub schema_to: u32,
    pub total_records: u32,
    pub already_completed: bool,
}

/// Result returned by [`MigrationRunner::run_step`].
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MigrationStepResult {
    pub schema_from: u32,
    pub schema_to: u32,
    pub processed: u32,
    pub offset: u32,
    pub total_records: u32,
    pub completed: bool,
}

/// Read-only preview returned by [`MigrationRunner::dry_run`].
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MigrationDryRunReport {
    pub schema_from: u32,
    pub schema_to: u32,
    pub current_offset: u32,
    pub next_offset: u32,
    pub would_process: u32,
    pub remaining_after_step: u32,
    pub total_records: u32,
    pub would_complete: bool,
}

/// Snapshot of the current migration runner state.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MigrationStatus {
    pub current_version: u32,
    pub pending_version: Option<u32>,
    pub in_progress: bool,
    pub offset: u32,
    pub records_migrated: u32,
    pub total_records: u32,
}

/// Registry of supported storage schema migration transitions.
pub struct MigrationRegistry;

impl MigrationRegistry {
    /// Return the list of registered migration transitions for this build.
    pub fn registered_transitions(env: &Env) -> Vec<MigrationStepSpec> {
        let mut specs = Vec::new(env);
        let total_records = BackupStorage::get_all_invoices(env).len();
        if STORAGE_SCHEMA_VERSION >= 1 {
            specs.push_back(MigrationStepSpec {
                from_version: 0,
                to_version: 1,
                total_records,
            });
        }
        specs
    }

    /// Resolve a migration transition `(from, to)` and compute its record count.
    ///
    /// Rejects non-forward transitions (`to <= from`).
    pub fn resolve(env: &Env, from: u32, to: u32) -> Result<MigrationStepSpec, QuickLendXError> {
        if to <= from {
            return Err(QuickLendXError::OperationNotAllowed);
        }
        let total_records = BackupStorage::get_all_invoices(env).len();
        Ok(MigrationStepSpec {
            from_version: from,
            to_version: to,
            total_records,
        })
    }
}

/// Resumable, admin-gated storage migration runner.
pub struct MigrationRunner;

impl MigrationRunner {
    /// Plan and initiate a migration from `from` to `to`.
    ///
    /// # Idempotency
    /// If the contract is already at `to`, no migration is in progress, and a
    /// completion record exists for `to`, returns a [`MigrationPlan`] with
    /// `already_completed: true` without mutating storage or emitting events.
    ///
    /// # Errors
    /// - `NotAdmin` if `admin` is not the configured contract admin.
    /// - `OperationNotAllowed` if a migration is already in progress, if `from`
    ///   does not match the committed schema version, or if `to <= from`.
    pub fn plan(
        env: &Env,
        admin: &Address,
        from: u32,
        to: u32,
    ) -> Result<MigrationPlan, QuickLendXError> {
        admin.require_auth();
        AdminStorage::require_admin(env, admin)?;

        if StorageMigration::is_migration_in_progress(env) {
            return Err(QuickLendXError::OperationNotAllowed);
        }

        let committed = StorageMigration::get_schema_version(env);
        if committed == to && Self::get_version_record(env, to).is_some() {
            return Ok(MigrationPlan {
                schema_from: from,
                schema_to: to,
                total_records: 0,
                already_completed: true,
            });
        }

        let spec = MigrationRegistry::resolve(env, from, to)?;
        StorageMigration::begin_migration(env, admin, from, to)?;

        env.storage()
            .instance()
            .set(&MIGRATION_STARTED_AT_KEY, &env.ledger().timestamp());
        env.storage().instance().set(&MIGRATION_INITIATOR_KEY, admin);

        Ok(MigrationPlan {
            schema_from: spec.from_version,
            schema_to: spec.to_version,
            total_records: spec.total_records,
            already_completed: false,
        })
    }

    /// Execute one bounded step of the active migration, processing at most
    /// `min(limit, MAX_MIGRATION_STEP_LIMIT)` records starting from the
    /// persisted offset cursor.
    ///
    /// When the final record is processed, automatically commits the migration,
    /// clears transient progress keys, and persists a [`MigrationVersionRecord`].
    ///
    /// # Errors
    /// - `NotAdmin` if `admin` is not the configured contract admin.
    /// - `OperationNotAllowed` if `limit == 0` or no migration is in progress.
    pub fn run_step(
        env: &Env,
        admin: &Address,
        limit: u32,
    ) -> Result<MigrationStepResult, QuickLendXError> {
        admin.require_auth();
        AdminStorage::require_admin(env, admin)?;

        if limit == 0 {
            return Err(QuickLendXError::OperationNotAllowed);
        }

        let schema_to = StorageMigration::get_pending_migration_version(env)
            .ok_or(QuickLendXError::OperationNotAllowed)?;
        let schema_from = StorageMigration::get_schema_version(env);

        let effective_limit = limit.min(MAX_MIGRATION_STEP_LIMIT);
        let invoices = BackupStorage::get_all_invoices(env);
        let total_records = invoices.len();
        let offset = StorageMigration::get_migration_offset(env);

        let end = offset.saturating_add(effective_limit).min(total_records);
        let mut processed: u32 = 0;

        let mut idx = offset;
        while idx < end {
            if let Some(invoice) = invoices.get(idx) {
                // Re-persist invoice to apply layout/TTL refresh idempotently.
                InvoiceStorage::update_invoice(env, &invoice);
                processed = processed.saturating_add(1);
            }
            idx = idx.saturating_add(1);
        }

        StorageMigration::advance_migration_page(env, end, processed);
        emit_migration_step_executed(
            env,
            schema_from,
            schema_to,
            end,
            processed,
            total_records,
            admin,
        );

        let completed = end >= total_records;
        if completed {
            let started_at: u64 = env
                .storage()
                .instance()
                .get(&MIGRATION_STARTED_AT_KEY)
                .unwrap_or_else(|| env.ledger().timestamp());
            let initiated_by: Address = env
                .storage()
                .instance()
                .get(&MIGRATION_INITIATOR_KEY)
                .unwrap_or_else(|| admin.clone());
            let total_migrated = StorageMigration::get_migration_records_migrated(env);

            StorageMigration::commit_migration(env, admin)?;

            let record = MigrationVersionRecord {
                schema_from,
                schema_to,
                records_migrated: total_migrated,
                started_at,
                completed_at: env.ledger().timestamp(),
                initiated_by,
                completed_by: admin.clone(),
            };
            Self::store_version_record(env, &record);
            Self::clear_runner_metadata(env);
        }

        Ok(MigrationStepResult {
            schema_from,
            schema_to,
            processed,
            offset: end,
            total_records,
            completed,
        })
    }

    /// Simulate the next migration step without writing to storage or emitting events.
    ///
    /// # Errors
    /// - `NotAdmin` if `admin` is not the configured contract admin.
    /// - `OperationNotAllowed` if `limit == 0` or no migration is in progress.
    pub fn dry_run(
        env: &Env,
        admin: &Address,
        limit: u32,
    ) -> Result<MigrationDryRunReport, QuickLendXError> {
        admin.require_auth();
        AdminStorage::require_admin(env, admin)?;

        if limit == 0 {
            return Err(QuickLendXError::OperationNotAllowed);
        }

        let schema_to = StorageMigration::get_pending_migration_version(env)
            .ok_or(QuickLendXError::OperationNotAllowed)?;
        let schema_from = StorageMigration::get_schema_version(env);

        let effective_limit = limit.min(MAX_MIGRATION_STEP_LIMIT);
        let total_records = BackupStorage::get_all_invoices(env).len();
        let current_offset = StorageMigration::get_migration_offset(env);
        let next_offset = current_offset.saturating_add(effective_limit).min(total_records);
        let would_process = next_offset.saturating_sub(current_offset);
        let remaining_after_step = total_records.saturating_sub(next_offset);
        let would_complete = next_offset >= total_records;

        Ok(MigrationDryRunReport {
            schema_from,
            schema_to,
            current_offset,
            next_offset,
            would_process,
            remaining_after_step,
            total_records,
            would_complete,
        })
    }

    /// Return the current migration status without mutating storage.
    pub fn status(env: &Env) -> MigrationStatus {
        let total_records = BackupStorage::get_all_invoices(env).len();
        MigrationStatus {
            current_version: StorageMigration::get_schema_version(env),
            pending_version: StorageMigration::get_pending_migration_version(env),
            in_progress: StorageMigration::is_migration_in_progress(env),
            offset: StorageMigration::get_migration_offset(env),
            records_migrated: StorageMigration::get_migration_records_migrated(env),
            total_records,
        }
    }

    /// Roll back an in-progress migration (admin only).
    pub fn rollback(env: &Env, admin: &Address) -> Result<(), QuickLendXError> {
        admin.require_auth();
        AdminStorage::require_admin(env, admin)?;
        StorageMigration::rollback_migration(env, admin)?;
        Self::clear_runner_metadata(env);
        Ok(())
    }

    /// Retrieve the persisted completion record for `version`, if any.
    pub fn get_version_record(env: &Env, version: u32) -> Option<MigrationVersionRecord> {
        let key = (MIGRATION_VER_RECORD_PREFIX, version);
        env.storage().persistent().get(&key)
    }

    fn store_version_record(env: &Env, record: &MigrationVersionRecord) {
        let key = (MIGRATION_VER_RECORD_PREFIX, record.schema_to);
        env.storage().persistent().set(&key, record);
    }

    fn clear_runner_metadata(env: &Env) {
        env.storage().instance().remove(&MIGRATION_STARTED_AT_KEY);
        env.storage().instance().remove(&MIGRATION_INITIATOR_KEY);
    }
}
