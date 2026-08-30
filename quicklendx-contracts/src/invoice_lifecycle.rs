//! Invoice creation and lifecycle: storage and migration compatibility.
//!
//! Issue #2438 (QE-2026-08).
//!
//! # Why this module exists
//!
//! An invoice record drives capital exposure: a record that cannot be read
//! after an upgrade, or that is left half-written by a failed migration, can
//! misprice exposure or permit settlement after the underlying obligation
//! changed. This module gives the invoice lifecycle a *documented* state
//! machine and a *durable, versioned* storage layout with an explicitly
//! resumable and observable migration path.
//!
//! # Design and invariants
//!
//! ## Lifecycle state machine
//!
//! ```text
//!            create
//!              │
//!              ▼
//!          ┌────────┐  amend (n times, Active only)
//!          │ Active │◄────────────┐
//!          └───┬────┘─────────────┘
//!       cancel │ complete
//!        ┌─────┴─────┐
//!        ▼           ▼
//!  ┌───────────┐ ┌───────────┐
//!  │ Cancelled │ │ Completed │   terminal - no further transitions
//!  └───────────┘ └───────────┘
//! ```
//!
//! Invariants enforced on every write:
//!
//! * **I1 - Terminal states are final.** A `Cancelled` or `Completed` invoice
//!   can never be amended, cancelled, or completed again. This is what stops a
//!   settlement from landing after the obligation changed.
//! * **I2 - Creation is unique.** Creating an invoice whose id already exists
//!   is rejected; an existing record is never silently overwritten.
//! * **I3 - Only the owning business mutates its invoice.** Every mutation
//!   requires the stored `business` address to authorise.
//! * **I4 - Monotonic bookkeeping.** `updated_at` never moves backwards and
//!   `amendment_count` only increases, so a stale replay cannot rewind state.
//! * **I5 - No partial state.** Every entrypoint validates *all* preconditions
//!   before the first storage write, so a rejected, stale, repeated, or failed
//!   operation leaves storage byte-identical to what it was.
//!
//! ## Storage compatibility
//!
//! * **Backward compatibility** (new code, old records): [`InvoiceRecordV1`]
//!   is the legacy shape. [`InvoiceLifecycle::load`] inspects the stored map
//!   for a `schema_version` field to classify the layout, then decodes it with
//!   the matching type and upgrades a V1 record *in memory*. A pre-migration
//!   record therefore stays readable and its lifecycle keeps working before
//!   the migration has run, and nothing is rewritten on read. The layout is
//!   probed rather than decoded speculatively because a typed read of the
//!   wrong shape raises a host `UnexpectedSize` error and panics instead of
//!   returning `None`.
//! * **Forward compatibility** (old code, new records): every stored record
//!   carries an explicit `schema_version`. A reader that encounters a version
//!   above [`INVOICE_SCHEMA_VERSION`] refuses it with
//!   `OperationNotAllowed` instead of silently misreading a newer layout.
//!   Future versions must therefore only *add* optional fields and bump the
//!   version, never repurpose an existing field.
//!
//! ## Migration: resumable and observable
//!
//! `begin_migration → migrate_step* → commit_migration` (or `rollback_migration`).
//!
//! * **Resumable.** [`MigrationState::cursor`] is committed after every page,
//!   so a failed or interrupted page resumes at the last checkpoint and never
//!   re-processes an already-migrated record.
//! * **Observable.** Every transition emits an event, and
//!   [`InvoiceLifecycle::migration_state`] / [`InvoiceLifecycle::schema_version`]
//!   expose progress for operators.
//! * **Rerun-safe.** Migrating an already-current record is a no-op that still
//!   advances the cursor, so re-running a page cannot double-count or corrupt.
//! * **Guarded.** While a migration is in progress, all lifecycle mutations are
//!   rejected with `OperationNotAllowed`. Without this a write could land on a
//!   page that was already migrated, re-introducing a legacy-shaped record
//!   behind the cursor - the classic partial-migration corruption.
//! * **Rollback.** [`InvoiceLifecycle::rollback_migration`] abandons an
//!   in-progress migration and clears migration state *without* bumping the
//!   committed version. Records already rewritten stay readable because the
//!   new shape is readable by the current build, and the committed version is
//!   only bumped once every record is done.
//!
//! # Failure behaviour and compatibility impact
//!
//! This module is **purely additive**: it introduces new entrypoints and new
//! storage keys, and changes no existing response shape or error code. It
//! reuses existing [`QuickLendXError`] variants (`OperationNotAllowed`,
//! `InvalidStatus`, `InvoiceNotFound`, `InvalidAmount`, `InvalidTimestamp`,
//! `NotAdmin`) because the error enum has no free slots - see `errors.rs`.
//!
//! # Operational limitations
//!
//! * Migration pages are operator-driven; `page_size` is bounded by
//!   [`MAX_MIGRATION_PAGE_SIZE`] so one call cannot exceed the resource budget.
//! * The invoice index is an append-only `Vec<BytesN<32>>`, which bounds the
//!   practical record count per contract instance; paging keeps per-call cost
//!   flat regardless of total size.
//!
//! # Security assumptions
//!
//! * Migration control is admin-only and the admin is set once at
//!   [`InvoiceLifecycle::init`]; every migration entrypoint calls
//!   `require_auth` on it.
//! * Lifecycle mutation is business-scoped (I3) and independent of the admin,
//!   so migration authority never confers the ability to alter invoice data.

use soroban_sdk::{
    contractevent, contracttype, Address, BytesN, Env, Map, Symbol, TryFromVal, Val, Vec,
};

use crate::errors::QuickLendXError;

/// Schema version understood by this build.
///
/// **BREAKING**: bumping this requires a migration path from every prior
/// version and an entry in the compatibility table in this module's docs.
pub const INVOICE_SCHEMA_VERSION: u32 = 2;

/// The legacy schema version that [`InvoiceRecordV1`] describes.
pub const INVOICE_SCHEMA_VERSION_LEGACY: u32 = 1;

/// Upper bound on records processed by a single [`InvoiceLifecycle::migrate_step`]
/// call, so one invocation cannot blow the resource budget.
pub const MAX_MIGRATION_PAGE_SIZE: u32 = 50;

// --- Events (observability) ---
//
// Every lifecycle transition and every migration step emits one of these.
// The invoice id and the migration versions are marked `#[topic]` so indexers
// can filter on them without decoding the payload, which is what makes a
// migration externally auditable while it runs.

/// An invoice was created.
#[contractevent(topics = ["invoice", "created"])]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvoiceCreated {
    #[topic]
    pub id: BytesN<32>,
    pub business: Address,
    pub amount: i128,
}

/// An invoice's amount or due date was amended.
#[contractevent(topics = ["invoice", "amended"])]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvoiceAmended {
    #[topic]
    pub id: BytesN<32>,
    pub amendment_count: u32,
}

/// An invoice reached the terminal `Cancelled` state.
#[contractevent(topics = ["invoice", "cancelled"])]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvoiceCancelled {
    #[topic]
    pub id: BytesN<32>,
    pub at: u64,
}

/// An invoice reached the terminal `Completed` state.
#[contractevent(topics = ["invoice", "completed"])]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvoiceCompleted {
    #[topic]
    pub id: BytesN<32>,
    pub at: u64,
}

/// A migration was started.
#[contractevent(topics = ["migration", "begin"])]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MigrationBegun {
    #[topic]
    pub from_version: u32,
    #[topic]
    pub to_version: u32,
}

/// One migration page completed; carries the resumable cursor.
#[contractevent(topics = ["migration", "step"])]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MigrationStepped {
    pub cursor: u32,
    pub migrated_this_page: u32,
    pub total: u32,
}

/// A migration was committed and the schema version bumped.
#[contractevent(topics = ["migration", "committed"])]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MigrationCommitted {
    #[topic]
    pub to_version: u32,
    pub migrated: u32,
}

/// A migration was abandoned; the committed version is unchanged.
#[contractevent(topics = ["migration", "rolledback"])]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MigrationRolledBack {
    pub cursor: u32,
    pub migrated: u32,
}

/// Lifecycle state of an invoice.
///
/// `Cancelled` and `Completed` are terminal (invariant I1).
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InvoiceLifecycleStatus {
    /// Created and amendable.
    Active = 0,
    /// Terminated by the business before completion.
    Cancelled = 1,
    /// Obligation discharged.
    Completed = 2,
}

impl InvoiceLifecycleStatus {
    /// Whether this state accepts any further transition.
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            InvoiceLifecycleStatus::Cancelled | InvoiceLifecycleStatus::Completed
        )
    }
}

/// Legacy (v1) on-chain invoice record.
///
/// Retained verbatim so historical records stay decodable. Never write this
/// shape from new code - it exists only to be read and upgraded.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvoiceRecordV1 {
    pub id: BytesN<32>,
    pub business: Address,
    pub amount: i128,
    pub due_date: u64,
    pub status: InvoiceLifecycleStatus,
    pub created_at: u64,
}

/// Current (v2) on-chain invoice record.
///
/// v2 adds `schema_version`, `updated_at`, `amendment_count`, and the terminal
/// timestamps. All additions are additive; no v1 field changed meaning.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvoiceRecord {
    /// Layout version this record was written under.
    pub schema_version: u32,
    pub id: BytesN<32>,
    pub business: Address,
    pub amount: i128,
    pub due_date: u64,
    pub status: InvoiceLifecycleStatus,
    pub created_at: u64,
    /// Last mutation time; monotonic (invariant I4).
    pub updated_at: u64,
    /// Number of successful amendments; monotonic (invariant I4).
    pub amendment_count: u32,
    /// Set exactly when `status == Cancelled`.
    pub cancelled_at: Option<u64>,
    /// Set exactly when `status == Completed`.
    pub completed_at: Option<u64>,
}

impl InvoiceRecord {
    /// Upgrade a legacy record to the current shape.
    ///
    /// Defaults are chosen so the upgrade is lossless and total: a v1 record
    /// had no amendment history, so `updated_at` falls back to `created_at`
    /// and `amendment_count` to 0. Terminal timestamps are unknown for legacy
    /// terminal records and are left `None` rather than invented.
    pub fn from_v1(legacy: &InvoiceRecordV1) -> Self {
        InvoiceRecord {
            schema_version: INVOICE_SCHEMA_VERSION,
            id: legacy.id.clone(),
            business: legacy.business.clone(),
            amount: legacy.amount,
            due_date: legacy.due_date,
            status: legacy.status,
            created_at: legacy.created_at,
            updated_at: legacy.created_at,
            amendment_count: 0,
            cancelled_at: None,
            completed_at: None,
        }
    }
}

/// Progress of an in-flight migration.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MigrationState {
    /// Committed version the migration started from.
    pub from_version: u32,
    /// Target version, committed only by `commit_migration`.
    pub to_version: u32,
    /// Index into the invoice id list; every record below this is done.
    pub cursor: u32,
    /// Records actually rewritten (excludes already-current no-ops).
    pub migrated: u32,
    pub started_at: u64,
}

#[contracttype]
enum DataKey {
    /// Admin authorised to drive migrations.
    Admin,
    /// Committed schema version (absent = fresh deployment).
    SchemaVersion,
    /// In-flight migration progress (absent = no migration running).
    Migration,
    /// One invoice record.
    Invoice(BytesN<32>),
    /// Deterministic append-only id list, so paging is stable across calls.
    Index,
}

/// Invoice lifecycle and storage-migration operations.
pub struct InvoiceLifecycle;

impl InvoiceLifecycle {
    // --- Initialisation -------------------------------------------------

    /// Record the migration admin and stamp the current schema version.
    ///
    /// Idempotent by rejection: a second call is refused so the admin cannot
    /// be silently replaced.
    pub fn init(env: &Env, admin: &Address) -> Result<(), QuickLendXError> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(QuickLendXError::OperationNotAllowed);
        }
        admin.require_auth();
        env.storage().instance().set(&DataKey::Admin, admin);
        env.storage()
            .instance()
            .set(&DataKey::SchemaVersion, &INVOICE_SCHEMA_VERSION);
        Ok(())
    }

    fn admin(env: &Env) -> Result<Address, QuickLendXError> {
        env.storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(QuickLendXError::NotAdmin)
    }

    fn require_admin(env: &Env) -> Result<Address, QuickLendXError> {
        let admin = Self::admin(env)?;
        admin.require_auth();
        Ok(admin)
    }

    // --- Observability --------------------------------------------------

    /// Committed schema version. `0` means "never initialised" (fresh
    /// deployment or a legacy instance predating versioning).
    pub fn schema_version(env: &Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::SchemaVersion)
            .unwrap_or(0)
    }

    /// In-flight migration progress, or `None` when idle.
    pub fn migration_state(env: &Env) -> Option<MigrationState> {
        env.storage().instance().get(&DataKey::Migration)
    }

    /// Whether a migration is currently running.
    pub fn migration_in_progress(env: &Env) -> bool {
        Self::migration_state(env).is_some()
    }

    /// Total invoices tracked by the index.
    pub fn invoice_count(env: &Env) -> u32 {
        Self::index(env).len()
    }

    fn index(env: &Env) -> Vec<BytesN<32>> {
        env.storage()
            .persistent()
            .get(&DataKey::Index)
            .unwrap_or_else(|| Vec::new(env))
    }

    // --- Reads ----------------------------------------------------------

    /// Load an invoice, transparently upgrading a legacy record in memory.
    ///
    /// Read order matters: the current shape is tried first so the common path
    /// costs one decode. A v1 record only falls through to the legacy decode
    /// when the current decode fails. Nothing is written back here - a read
    /// must never mutate state, which is what keeps queries usable while a
    /// migration is in progress.
    ///
    /// # Errors
    /// * `InvoiceNotFound` - no record under this id.
    /// * `OperationNotAllowed` - the record declares a schema version newer
    ///   than this build understands (forward-compatibility refusal).
    pub fn load(env: &Env, id: &BytesN<32>) -> Result<InvoiceRecord, QuickLendXError> {
        let key = DataKey::Invoice(id.clone());
        match Self::stored_is_current(env, &key) {
            None => Err(QuickLendXError::InvoiceNotFound),
            Some(true) => {
                let record: InvoiceRecord = env
                    .storage()
                    .persistent()
                    .get(&key)
                    .ok_or(QuickLendXError::InvoiceNotFound)?;
                if record.schema_version > INVOICE_SCHEMA_VERSION {
                    // Refuse rather than misread a layout from a newer build.
                    return Err(QuickLendXError::OperationNotAllowed);
                }
                Ok(record)
            }
            Some(false) => {
                let legacy: InvoiceRecordV1 = env
                    .storage()
                    .persistent()
                    .get(&key)
                    .ok_or(QuickLendXError::InvoiceNotFound)?;
                Ok(InvoiceRecord::from_v1(&legacy))
            }
        }
    }

    /// Classify the stored record's layout without decoding it into a struct.
    ///
    /// Returns `None` when absent, `Some(true)` for the current shape, and
    /// `Some(false)` for the legacy shape.
    ///
    /// Discriminating on the raw map is deliberate. A typed `get` of the wrong
    /// shape does not fail softly - the host raises `UnexpectedSize`
    /// ("differing host map and output slice lengths") and escalates it to a
    /// panic, so a "try current, fall back to legacy" decode would abort the
    /// call instead of falling through. Probing for the `schema_version` field
    /// keeps the read total and side-effect free.
    fn stored_is_current(env: &Env, key: &DataKey) -> Option<bool> {
        let raw: Val = env.storage().persistent().get::<DataKey, Val>(key)?;
        let fields: Map<Symbol, Val> = Map::try_from_val(env, &raw).ok()?;
        Some(fields.contains_key(Symbol::new(env, "schema_version")))
    }

    /// Whether a record exists under `id`, in either schema version.
    pub fn exists(env: &Env, id: &BytesN<32>) -> bool {
        env.storage()
            .persistent()
            .has(&DataKey::Invoice(id.clone()))
    }

    // --- Guards ---------------------------------------------------------

    /// Reject lifecycle mutations while a migration is running.
    ///
    /// This is the invariant that keeps a migration atomic from the caller's
    /// point of view: without it, a write landing behind the cursor would
    /// re-introduce an unmigrated record into a range the migration has
    /// already passed.
    fn require_no_migration(env: &Env) -> Result<(), QuickLendXError> {
        if Self::migration_in_progress(env) {
            return Err(QuickLendXError::OperationNotAllowed);
        }
        Ok(())
    }

    // --- Lifecycle ------------------------------------------------------

    /// Create an invoice in `Active`.
    ///
    /// # Errors
    /// * `OperationNotAllowed` - a migration is running, or the id already
    ///   exists (invariant I2 - never overwrite).
    /// * `InvalidAmount` - amount is not strictly positive.
    /// * `InvalidTimestamp` - `due_date` is not in the future.
    pub fn create(
        env: &Env,
        id: &BytesN<32>,
        business: &Address,
        amount: i128,
        due_date: u64,
    ) -> Result<InvoiceRecord, QuickLendXError> {
        // Every precondition is checked before the first write (invariant I5).
        Self::require_no_migration(env)?;
        if Self::exists(env, id) {
            return Err(QuickLendXError::OperationNotAllowed);
        }
        if amount <= 0 {
            return Err(QuickLendXError::InvalidAmount);
        }
        let now = env.ledger().timestamp();
        if due_date <= now {
            return Err(QuickLendXError::InvalidTimestamp);
        }
        business.require_auth();

        let record = InvoiceRecord {
            schema_version: INVOICE_SCHEMA_VERSION,
            id: id.clone(),
            business: business.clone(),
            amount,
            due_date,
            status: InvoiceLifecycleStatus::Active,
            created_at: now,
            updated_at: now,
            amendment_count: 0,
            cancelled_at: None,
            completed_at: None,
        };

        env.storage()
            .persistent()
            .set(&DataKey::Invoice(id.clone()), &record);
        let mut index = Self::index(env);
        index.push_back(id.clone());
        env.storage().persistent().set(&DataKey::Index, &index);

        InvoiceCreated {
            id: id.clone(),
            business: business.clone(),
            amount,
        }
        .publish(env);
        Ok(record)
    }

    /// Amend an `Active` invoice's amount and/or due date.
    ///
    /// # Errors
    /// * `OperationNotAllowed` - a migration is running.
    /// * `InvoiceNotFound` - unknown id.
    /// * `InvalidStatus` - the invoice is terminal (invariant I1).
    /// * `InvalidAmount` / `InvalidTimestamp` - invalid new values.
    pub fn amend(
        env: &Env,
        id: &BytesN<32>,
        new_amount: i128,
        new_due_date: u64,
    ) -> Result<InvoiceRecord, QuickLendXError> {
        Self::require_no_migration(env)?;
        let mut record = Self::load(env, id)?;
        if record.status.is_terminal() {
            return Err(QuickLendXError::InvalidStatus);
        }
        if new_amount <= 0 {
            return Err(QuickLendXError::InvalidAmount);
        }
        let now = env.ledger().timestamp();
        if new_due_date <= now {
            return Err(QuickLendXError::InvalidTimestamp);
        }
        // Ownership is enforced against the *stored* business (invariant I3),
        // never against a caller-supplied address.
        record.business.require_auth();

        record.amount = new_amount;
        record.due_date = new_due_date;
        // Monotonic bookkeeping (invariant I4).
        record.updated_at = core::cmp::max(record.updated_at, now);
        record.amendment_count += 1;
        record.schema_version = INVOICE_SCHEMA_VERSION;

        env.storage()
            .persistent()
            .set(&DataKey::Invoice(id.clone()), &record);
        InvoiceAmended {
            id: id.clone(),
            amendment_count: record.amendment_count,
        }
        .publish(env);
        Ok(record)
    }

    /// Cancel an `Active` invoice. Terminal.
    ///
    /// # Errors
    /// Same shape as [`InvoiceLifecycle::amend`]; `InvalidStatus` when already
    /// terminal, which is what makes a repeated cancel a no-op rejection
    /// rather than a second state write.
    pub fn cancel(env: &Env, id: &BytesN<32>) -> Result<InvoiceRecord, QuickLendXError> {
        Self::require_no_migration(env)?;
        let mut record = Self::load(env, id)?;
        if record.status.is_terminal() {
            return Err(QuickLendXError::InvalidStatus);
        }
        record.business.require_auth();

        let now = env.ledger().timestamp();
        record.status = InvoiceLifecycleStatus::Cancelled;
        record.cancelled_at = Some(now);
        record.updated_at = core::cmp::max(record.updated_at, now);
        record.schema_version = INVOICE_SCHEMA_VERSION;

        env.storage()
            .persistent()
            .set(&DataKey::Invoice(id.clone()), &record);
        InvoiceCancelled {
            id: id.clone(),
            at: now,
        }
        .publish(env);
        Ok(record)
    }

    /// Complete an `Active` invoice. Terminal.
    ///
    /// # Errors
    /// `InvalidStatus` when already terminal - this is the guard that stops a
    /// settlement landing after the obligation changed.
    pub fn complete(env: &Env, id: &BytesN<32>) -> Result<InvoiceRecord, QuickLendXError> {
        Self::require_no_migration(env)?;
        let mut record = Self::load(env, id)?;
        if record.status.is_terminal() {
            return Err(QuickLendXError::InvalidStatus);
        }
        record.business.require_auth();

        let now = env.ledger().timestamp();
        record.status = InvoiceLifecycleStatus::Completed;
        record.completed_at = Some(now);
        record.updated_at = core::cmp::max(record.updated_at, now);
        record.schema_version = INVOICE_SCHEMA_VERSION;

        env.storage()
            .persistent()
            .set(&DataKey::Invoice(id.clone()), &record);
        InvoiceCompleted {
            id: id.clone(),
            at: now,
        }
        .publish(env);
        Ok(record)
    }

    // --- Migration ------------------------------------------------------

    /// Begin a migration from `from_version` to `to_version`.
    ///
    /// `from_version` is supplied by the caller and checked against the
    /// committed version, so a stale or concurrent operator call - one built
    /// against a version that has since been migrated - is rejected instead of
    /// starting a second, conflicting migration.
    ///
    /// # Errors
    /// * `OperationNotAllowed` - a migration is already running (repeat
    ///   rejection), `from_version` does not match the committed version
    ///   (stale rejection), `to_version <= from_version`, or `to_version`
    ///   exceeds this build's [`INVOICE_SCHEMA_VERSION`].
    /// * `NotAdmin` - the contract was never initialised.
    pub fn begin_migration(
        env: &Env,
        from_version: u32,
        to_version: u32,
    ) -> Result<MigrationState, QuickLendXError> {
        Self::require_admin(env)?;
        if Self::migration_in_progress(env) {
            return Err(QuickLendXError::OperationNotAllowed);
        }
        if Self::schema_version(env) != from_version {
            return Err(QuickLendXError::OperationNotAllowed);
        }
        if to_version <= from_version || to_version > INVOICE_SCHEMA_VERSION {
            return Err(QuickLendXError::OperationNotAllowed);
        }

        let state = MigrationState {
            from_version,
            to_version,
            cursor: 0,
            migrated: 0,
            started_at: env.ledger().timestamp(),
        };
        env.storage().instance().set(&DataKey::Migration, &state);
        MigrationBegun {
            from_version,
            to_version,
        }
        .publish(env);
        Ok(state)
    }

    /// Migrate up to `page_size` records, starting at the committed cursor.
    ///
    /// The cursor is persisted after the page, so an interrupted or failed
    /// call resumes exactly where it stopped and never reprocesses a committed
    /// record. Re-running a page whose records are already current is a
    /// no-op that still advances the cursor, so a rerun cannot double-count.
    ///
    /// # Errors
    /// * `OperationNotAllowed` - no migration in progress, or `page_size` is
    ///   zero or above [`MAX_MIGRATION_PAGE_SIZE`].
    pub fn migrate_step(env: &Env, page_size: u32) -> Result<MigrationState, QuickLendXError> {
        Self::require_admin(env)?;
        if page_size == 0 || page_size > MAX_MIGRATION_PAGE_SIZE {
            return Err(QuickLendXError::OperationNotAllowed);
        }
        let mut state = Self::migration_state(env).ok_or(QuickLendXError::OperationNotAllowed)?;

        let index = Self::index(env);
        let total = index.len();
        let start = state.cursor;
        let end = core::cmp::min(start.saturating_add(page_size), total);

        let mut migrated_this_page: u32 = 0;
        let mut i = start;
        while i < end {
            let id = index.get(i).expect("cursor within index bounds");
            let key = DataKey::Invoice(id.clone());
            // Already-current records are skipped, which is what makes a
            // rerun of the same page idempotent.
            if Self::stored_is_current(env, &key) == Some(false) {
                if let Some(legacy) = env
                    .storage()
                    .persistent()
                    .get::<DataKey, InvoiceRecordV1>(&key)
                {
                    let upgraded = InvoiceRecord::from_v1(&legacy);
                    env.storage().persistent().set(&key, &upgraded);
                    migrated_this_page += 1;
                }
            }
            i += 1;
        }

        state.cursor = end;
        state.migrated += migrated_this_page;
        env.storage().instance().set(&DataKey::Migration, &state);
        MigrationStepped {
            cursor: state.cursor,
            migrated_this_page,
            total,
        }
        .publish(env);
        Ok(state)
    }

    /// Commit a fully-processed migration and bump the committed version.
    ///
    /// # Errors
    /// * `OperationNotAllowed` - no migration in progress, or the cursor has
    ///   not reached the end of the index. Committing early would declare the
    ///   store migrated while legacy records remain, so it is refused.
    pub fn commit_migration(env: &Env) -> Result<u32, QuickLendXError> {
        Self::require_admin(env)?;
        let state = Self::migration_state(env).ok_or(QuickLendXError::OperationNotAllowed)?;
        if state.cursor < Self::index(env).len() {
            return Err(QuickLendXError::OperationNotAllowed);
        }
        env.storage()
            .instance()
            .set(&DataKey::SchemaVersion, &state.to_version);
        env.storage().instance().remove(&DataKey::Migration);
        MigrationCommitted {
            to_version: state.to_version,
            migrated: state.migrated,
        }
        .publish(env);
        Ok(state.to_version)
    }

    /// Abandon an in-progress migration.
    ///
    /// The committed version is left untouched, so the store still declares
    /// the old version. Records already rewritten to the new shape remain
    /// readable, because the current build reads both shapes - that asymmetry
    /// is deliberate and is why rollback is safe without rewriting data back.
    ///
    /// # Errors
    /// * `OperationNotAllowed` - no migration in progress.
    pub fn rollback_migration(env: &Env) -> Result<MigrationState, QuickLendXError> {
        Self::require_admin(env)?;
        let state = Self::migration_state(env).ok_or(QuickLendXError::OperationNotAllowed)?;
        env.storage().instance().remove(&DataKey::Migration);
        MigrationRolledBack {
            cursor: state.cursor,
            migrated: state.migrated,
        }
        .publish(env);
        Ok(state)
    }

    // --- Test-only fixture support --------------------------------------

    /// Write a legacy (v1) record directly, simulating data written by a
    /// pre-migration deployment.
    ///
    /// Test-only: this is the only way to produce a v1 record now that all
    /// production writes stamp v2, and it is what the legacy-data fixtures in
    /// `test_invoice_lifecycle.rs` are built from.
    #[cfg(any(test, feature = "testutils"))]
    pub fn seed_legacy_record(env: &Env, record: &InvoiceRecordV1) {
        env.storage()
            .persistent()
            .set(&DataKey::Invoice(record.id.clone()), record);
        let mut index = Self::index(env);
        index.push_back(record.id.clone());
        env.storage().persistent().set(&DataKey::Index, &index);
    }

    /// Force the committed schema version, simulating an instance deployed by
    /// an older build.
    ///
    /// Test-only: production code only ever moves the version forward through
    /// [`InvoiceLifecycle::commit_migration`], so this is the only way to
    /// construct a pre-upgrade instance for the migration fixtures.
    #[cfg(any(test, feature = "testutils"))]
    pub fn force_schema_version(env: &Env, version: u32) {
        env.storage()
            .instance()
            .set(&DataKey::SchemaVersion, &version);
    }

    /// Report whether the stored record under `id` is still in the legacy
    /// shape. Test-only; used to prove a migration actually rewrote records.
    #[cfg(any(test, feature = "testutils"))]
    pub fn is_legacy_shape(env: &Env, id: &BytesN<32>) -> bool {
        Self::stored_is_current(env, &DataKey::Invoice(id.clone())) == Some(false)
    }
}
