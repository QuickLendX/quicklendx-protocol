#![no_std]

use soroban_sdk::{contract, contractimpl, symbol_short, Address, BytesN, Env, Symbol};

use crate::errors::QuickLendXError;
use crate::invoice_lifecycle::{InvoiceLifecycle, InvoiceRecord, MigrationState};

pub mod errors;
/// Invoice amount precision and overflow validation (Issue #2432).
///
/// See the module docs for the exact integer rules, invariants, compatibility
/// impact, and security assumptions. The invoice lifecycle entrypoints
/// (`contract.rs::store_invoice`, `invoice.rs::Invoice::new`) route their
/// amount checks through this module.
pub mod invoice_amount;

/// Invoice creation and lifecycle: storage and migration compatibility
/// (Issue #2438).
///
/// Owns the documented lifecycle state machine, the versioned invoice record
/// layout, and the resumable/observable migration path. See the module docs
/// for the invariants, the forward/backward compatibility contract, and the
/// rollback and security assumptions.
pub mod invoice_lifecycle;

#[cfg(test)]
mod test_invoice_amount_precision;

#[cfg(test)]
mod test_invoice_lifecycle;

#[contract]
pub struct QuickLendXContract;

#[contractimpl]
impl QuickLendXContract {
    pub fn hello(_env: Env) -> Symbol {
        symbol_short!("A1")
    }

    // --- Invoice lifecycle (Issue #2438) --------------------------------
    //
    // Each entrypoint is a thin delegation; all invariants, authorisation,
    // and the no-partial-state guarantee live in `invoice_lifecycle` so the
    // rules cannot drift between the module and the ABI.

    /// Record the migration admin and stamp the current schema version.
    pub fn init_invoice_lifecycle(env: Env, admin: Address) -> Result<(), QuickLendXError> {
        InvoiceLifecycle::init(&env, &admin)
    }

    /// Create an invoice in `Active`.
    pub fn create_invoice_record(
        env: Env,
        id: BytesN<32>,
        business: Address,
        amount: i128,
        due_date: u64,
    ) -> Result<InvoiceRecord, QuickLendXError> {
        InvoiceLifecycle::create(&env, &id, &business, amount, due_date)
    }

    /// Amend an `Active` invoice. Rejected once terminal.
    pub fn amend_invoice_record(
        env: Env,
        id: BytesN<32>,
        new_amount: i128,
        new_due_date: u64,
    ) -> Result<InvoiceRecord, QuickLendXError> {
        InvoiceLifecycle::amend(&env, &id, new_amount, new_due_date)
    }

    /// Cancel an `Active` invoice. Terminal.
    pub fn cancel_invoice_record(
        env: Env,
        id: BytesN<32>,
    ) -> Result<InvoiceRecord, QuickLendXError> {
        InvoiceLifecycle::cancel(&env, &id)
    }

    /// Complete an `Active` invoice. Terminal.
    pub fn complete_invoice_record(
        env: Env,
        id: BytesN<32>,
    ) -> Result<InvoiceRecord, QuickLendXError> {
        InvoiceLifecycle::complete(&env, &id)
    }

    /// Read an invoice, transparently upgrading a legacy record in memory.
    pub fn get_invoice_record(env: Env, id: BytesN<32>) -> Result<InvoiceRecord, QuickLendXError> {
        InvoiceLifecycle::load(&env, &id)
    }

    // --- Migration control and observability ----------------------------

    /// Committed storage schema version (`0` when never initialised).
    pub fn invoice_schema_version(env: Env) -> u32 {
        InvoiceLifecycle::schema_version(&env)
    }

    /// In-flight migration progress, or `None` when idle.
    pub fn invoice_migration_state(env: Env) -> Option<MigrationState> {
        InvoiceLifecycle::migration_state(&env)
    }

    /// Number of invoices tracked by the migration index.
    pub fn invoice_record_count(env: Env) -> u32 {
        InvoiceLifecycle::invoice_count(&env)
    }

    /// Begin a migration. `from_version` must match the committed version,
    /// which is what rejects stale and concurrent operator calls.
    pub fn begin_invoice_migration(
        env: Env,
        from_version: u32,
        to_version: u32,
    ) -> Result<MigrationState, QuickLendXError> {
        InvoiceLifecycle::begin_migration(&env, from_version, to_version)
    }

    /// Migrate one resumable page of records.
    pub fn step_invoice_migration(
        env: Env,
        page_size: u32,
    ) -> Result<MigrationState, QuickLendXError> {
        InvoiceLifecycle::migrate_step(&env, page_size)
    }

    /// Commit a fully-processed migration and bump the committed version.
    pub fn commit_invoice_migration(env: Env) -> Result<u32, QuickLendXError> {
        InvoiceLifecycle::commit_migration(&env)
    }

    /// Abandon an in-progress migration without bumping the version.
    pub fn rollback_invoice_migration(env: Env) -> Result<MigrationState, QuickLendXError> {
        InvoiceLifecycle::rollback_migration(&env)
    }
}
