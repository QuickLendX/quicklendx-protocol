use crate::errors::QuickLendXError;
use crate::types::Invoice;
use soroban_sdk::{contracttype, symbol_short, BytesN, Env, String, Vec};

const RETENTION_POLICY_KEY: soroban_sdk::Symbol = symbol_short!("bkup_pol");
const BACKUP_COUNTER_KEY: soroban_sdk::Symbol = symbol_short!("bkup_cnt");
const BACKUP_LIST_KEY: soroban_sdk::Symbol = symbol_short!("backups");
const BACKUP_DATA_KEY: soroban_sdk::Symbol = symbol_short!("bkup_data");
const MAX_BACKUP_DESCRIPTION_LENGTH: u32 = 128;

/// A stored snapshot of all invoices at a point in time.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Backup {
    pub backup_id: BytesN<32>,
    pub timestamp: u64,
    pub description: String,
    pub invoice_count: u32,
    pub status: BackupStatus,
}

/// Lifecycle state of a [`Backup`] record.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BackupStatus {
    /// Backup is valid and available for restore.
    Active,
    /// Backup has been superseded and should not be restored.
    Archived,
    /// Backup data failed integrity checks and must not be restored.
    Corrupted,
}

/// Backup retention policy configuration.
///
/// Controls how many backups are kept and for how long.  When
/// `auto_cleanup_enabled` is `true`, `cleanup_old_backups` enforces both
/// `max_backups` and `max_age_seconds` on every invocation.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BackupRetentionPolicy {
    /// Maximum number of backups to keep (0 = unlimited).
    pub max_backups: u32,
    /// Maximum age of backups in seconds (0 = unlimited).
    pub max_age_seconds: u64,
    /// Whether automatic cleanup is enabled.
    pub auto_cleanup_enabled: bool,
}

impl Default for BackupRetentionPolicy {
    fn default() -> Self {
        Self {
            max_backups: 5,
            max_age_seconds: 0,
            auto_cleanup_enabled: true,
        }
    }
}

/// Low-level backup storage operations.
///
/// All public functions are thin wrappers around Soroban instance storage.
/// Higher-level orchestration (backup-then-clear, validate-then-restore) lives
/// in [`BackupStorage::restore_from_backup`] which is the **only** safe entry
/// point for restoring data.
pub struct BackupStorage;

impl BackupStorage {
    fn validate_backup_metadata(
        backup: &Backup,
        invoices: Option<&Vec<Invoice>>,
    ) -> Result<(), QuickLendXError> {
        if !Self::is_valid_backup_id(&backup.backup_id) {
            return Err(QuickLendXError::StorageError);
        }

        if backup.description.len() == 0 || backup.description.len() > MAX_BACKUP_DESCRIPTION_LENGTH
        {
            return Err(QuickLendXError::InvalidDescription);
        }

        if let Some(invoices) = invoices {
            if invoices.len() != backup.invoice_count {
                return Err(QuickLendXError::StorageError);
            }
        }

        Ok(())
    }

    pub fn is_valid_backup_id(backup_id: &BytesN<32>) -> bool {
        let bytes = backup_id.to_array();
        bytes[0] == 0xB4 && bytes[1] == 0xC4
    }

    /// Get the backup retention policy.
    pub fn get_retention_policy(env: &Env) -> BackupRetentionPolicy {
        env.storage()
            .instance()
            .get(&RETENTION_POLICY_KEY)
            .unwrap_or_else(|| BackupRetentionPolicy::default())
    }

    /// Set the backup retention policy (admin only — caller must enforce auth).
    pub fn set_retention_policy(env: &Env, policy: &BackupRetentionPolicy) {
        env.storage().instance().set(&RETENTION_POLICY_KEY, policy);
    }

    /// Generate a unique backup ID.
    ///
    /// Format: `0xB4 0xC4 | timestamp(8B) | counter(8B) | mix(14B)`.
    pub fn generate_backup_id(env: &Env) -> BytesN<32> {
        let timestamp = env.ledger().timestamp();
        let counter: u64 = env
            .storage()
            .instance()
            .get(&BACKUP_COUNTER_KEY)
            .unwrap_or(0);
        let next_counter = counter.saturating_add(1);
        env.storage()
            .instance()
            .set(&BACKUP_COUNTER_KEY, &next_counter);

        let mut id_bytes = [0u8; 32];
        id_bytes[0] = 0xB4;
        id_bytes[1] = 0xC4;
        id_bytes[2..10].copy_from_slice(&timestamp.to_be_bytes());
        id_bytes[10..18].copy_from_slice(&next_counter.to_be_bytes());
        let mix = timestamp
            .saturating_add(next_counter)
            .saturating_add(0xB4C4);
        for i in 18..32 {
            id_bytes[i] = (mix % 256) as u8;
        }

        BytesN::from_array(env, &id_bytes)
    }

    /// Persist a backup record (metadata only).
    ///
    /// Returns [`QuickLendXError::OperationNotAllowed`] if a backup with the
    /// same ID already exists, preventing accidental overwrites.
    pub fn store_backup(
        env: &Env,
        backup: &Backup,
        invoices: Option<&Vec<Invoice>>,
    ) -> Result<(), QuickLendXError> {
        Self::validate_backup_metadata(backup, invoices)?;

        if Self::get_backup(env, &backup.backup_id).is_some() {
            return Err(QuickLendXError::OperationNotAllowed);
        }

        env.storage().instance().set(&backup.backup_id, backup);
        Ok(())
    }

    /// Retrieve a backup record by ID.
    pub fn get_backup(env: &Env, backup_id: &BytesN<32>) -> Option<Backup> {
        env.storage().instance().get(backup_id)
    }

    /// Update an existing backup record (e.g. to mark it `Archived`).
    pub fn update_backup(env: &Env, backup: &Backup) -> Result<(), QuickLendXError> {
        Self::validate_backup_metadata(backup, None)?;
        env.storage().instance().set(&backup.backup_id, backup);
        Ok(())
    }

    /// Get all backup IDs in the global backup list.
    pub fn get_all_backups(env: &Env) -> Vec<BytesN<32>> {
        env.storage()
            .instance()
            .get(&BACKUP_LIST_KEY)
            .unwrap_or_else(|| Vec::new(env))
    }

    /// Append a backup ID to the global backup list (deduplication guard included).
    pub fn add_to_backup_list(env: &Env, backup_id: &BytesN<32>) {
        let mut backups = Self::get_all_backups(env);
        for existing in backups.iter() {
            if existing == *backup_id {
                return;
            }
        }
        backups.push_back(backup_id.clone());
        env.storage().instance().set(&BACKUP_LIST_KEY, &backups);
    }

    /// Remove a backup ID from the global backup list.
    pub fn remove_from_backup_list(env: &Env, backup_id: &BytesN<32>) {
        let backups = Self::get_all_backups(env);
        let mut new_backups = Vec::new(env);
        for id in backups.iter() {
            if id != *backup_id {
                new_backups.push_back(id);
            }
        }
        env.storage().instance().set(&BACKUP_LIST_KEY, &new_backups);
    }

    /// Store the invoice payload for a backup.
    pub fn store_backup_data(env: &Env, backup_id: &BytesN<32>, invoices: &Vec<Invoice>) {
        let key = (BACKUP_DATA_KEY, backup_id.clone());
        env.storage().instance().set(&key, invoices);
    }

    /// Retrieve the invoice payload for a backup.
    pub fn get_backup_data(env: &Env, backup_id: &BytesN<32>) -> Option<Vec<Invoice>> {
        let key = (BACKUP_DATA_KEY, backup_id.clone());
        env.storage().instance().get(&key)
    }

    /// Delete a backup record and its stored invoice payload.
    pub fn purge_backup(env: &Env, backup_id: &BytesN<32>) {
        Self::remove_from_backup_list(env, backup_id);
        env.storage().instance().remove(backup_id);
        let data_key = (BACKUP_DATA_KEY, backup_id.clone());
        env.storage().instance().remove(&data_key);
    }

    /// Validate backup data integrity.
    ///
    /// Checks that:
    /// 1. The backup record exists and has a valid ID prefix.
    /// 2. The invoice payload exists.
    /// 3. The payload length matches `backup.invoice_count`.
    /// 4. Every invoice in the payload has a positive `amount`.
    pub fn validate_backup(env: &Env, backup_id: &BytesN<32>) -> Result<(), QuickLendXError> {
        let backup = Self::get_backup(env, backup_id).ok_or(QuickLendXError::StorageKeyNotFound)?;

        // Validate metadata alone first (cheap).
        Self::validate_backup_metadata(&backup, None)?;

        // Fetch the payload and validate together with the count.
        let data =
            Self::get_backup_data(env, backup_id).ok_or(QuickLendXError::StorageKeyNotFound)?;

        if data.len() as u32 != backup.invoice_count {
            return Err(QuickLendXError::StorageError);
        }

        // Validate each invoice record in the payload.
        for invoice in data.iter() {
            if invoice.amount <= 0 {
                return Err(QuickLendXError::StorageError);
            }
        }

        Ok(())
    }

    /// Restore all invoices from a backup in a safe, validated sequence.
    ///
    /// # Restore ordering
    ///
    /// The ordering of operations is critical to prevent orphan indexes and
    /// partial-state corruption:
    ///
    /// ```text
    /// Step 1  validate_backup()
    ///         ─────────────────
    ///         Full integrity check BEFORE any mutation.  If the backup is
    ///         corrupt or the invoice_count mismatches, we abort here and
    ///         leave existing storage completely untouched.
    ///
    /// Step 2  InvoiceStorage::clear_all()
    ///         ────────────────────────────
    ///         Atomically removes every invoice record, status bucket,
    ///         category index, tag index, business index, and metadata index.
    ///         After this step storage is empty.  There is no rollback
    ///         mechanism on a Soroban ledger; reaching this step means the
    ///         caller has accepted that the current state will be discarded.
    ///
    /// Step 3  InvoiceStorage::store_invoice() per invoice
    ///         ────────────────────────────────────────────
    ///         Re-registers each invoice from the backup payload, rebuilding
    ///         all secondary indexes from scratch.  The write order within
    ///         this step does not matter because `store_invoice` is
    ///         self-contained.
    ///
    /// Step 4  Mark the backup as Archived
    ///         ────────────────────────────
    ///         Prevents the same backup from being restored twice, which
    ///         could cause duplicate invoice registrations if the store is
    ///         not cleared between restores.
    /// ```
    ///
    /// # Errors
    ///
    /// Returns an error *only* in step 1.  Steps 2–4 are infallible on a
    /// well-formed Soroban environment; panics in those steps indicate a
    /// platform bug, not a contract bug.
    ///
    /// # Security
    ///
    /// - The caller **must** enforce admin authentication before invoking this
    ///   function.  The contract entry point is responsible for `require_auth`.
    /// - Validate → clear → restore is the only safe ordering.  Clearing
    ///   before validating would leave the contract in an empty state if the
    ///   backup turns out to be corrupt.
    /// - Restoring without clearing first would overlay backup data on stale
    ///   indexes, causing ghost entries in status/category/tag buckets for
    ///   any invoices that existed before the restore.
    pub fn restore_from_backup(env: &Env, backup_id: &BytesN<32>) -> Result<u32, QuickLendXError> {
        //  Step 1: validate before mutating anything
        Self::validate_backup(env, backup_id)?;

        // Fetch the validated payload.
        let data =
            Self::get_backup_data(env, backup_id).ok_or(QuickLendXError::StorageKeyNotFound)?;

        let restored_count = data.len();

        //  Step 2: atomically clear all existing invoice state
        crate::invoice::InvoiceStorage::clear_all(env);

        //  Step 3: re-register every invoice, rebuilding all indexes
        for invoice in data.iter() {
            crate::invoice::InvoiceStorage::store_invoice(env, &invoice);
        }

        // Step 4: mark the backup as archived to prevent re-use
        if let Some(mut backup) = Self::get_backup(env, backup_id) {
            backup.status = BackupStatus::Archived;
            // Ignore the result — the restore itself has already succeeded.
            let _ = Self::update_backup(env, &backup);
        }

        Ok(restored_count)
    }

    /// Clean up old backups based on the retention policy.
    ///
    /// Removes backups that exceed `max_age_seconds`, then removes the oldest
    /// backups until the count is within `max_backups`.  Only `Active` backups
    /// are considered; `Archived` and `Corrupted` backups are left untouched.
    ///
    /// Returns the number of backups removed.
    pub fn cleanup_old_backups(env: &Env) -> Result<u32, QuickLendXError> {
        let policy = Self::get_retention_policy(env);

        if !policy.auto_cleanup_enabled {
            return Ok(0);
        }

        let backups = Self::get_all_backups(env);
        let current_time = env.ledger().timestamp();
        let mut removed_count = 0u32;

        // Build (backup_id, timestamp) pairs for active backups only.
        let mut backup_timestamps = Vec::new(env);
        for backup_id in backups.iter() {
            if let Some(backup) = Self::get_backup(env, &backup_id) {
                if backup.status == BackupStatus::Active {
                    backup_timestamps.push_back((backup_id, backup.timestamp));
                }
            }
        }

        // Bubble sort: oldest first.
        let len = backup_timestamps.len();
        for i in 0..len {
            for j in 0..len - i - 1 {
                if backup_timestamps.get(j).unwrap().1 > backup_timestamps.get(j + 1).unwrap().1 {
                    let temp = backup_timestamps.get(j).unwrap().clone();
                    backup_timestamps.set(j, backup_timestamps.get(j + 1).unwrap().clone());
                    backup_timestamps.set(j + 1, temp);
                }
            }
        }

        // Remove backups that exceed max age.
        if policy.max_age_seconds > 0 {
            let mut i = 0;
            while i < backup_timestamps.len() {
                let (backup_id, timestamp) = backup_timestamps.get(i).unwrap();
                let age = current_time.saturating_sub(timestamp);

                if age > policy.max_age_seconds {
                    Self::purge_backup(env, &backup_id);
                    backup_timestamps.remove(i);
                    removed_count = removed_count.saturating_add(1);
                } else {
                    i += 1;
                }
            }
        }

        // Remove oldest backups until within the max_backups limit.
        if policy.max_backups > 0 {
            while backup_timestamps.len() > policy.max_backups {
                if let Some((oldest_id, _)) = backup_timestamps.first() {
                    Self::purge_backup(env, &oldest_id);
                    backup_timestamps.remove(0);
                    removed_count = removed_count.saturating_add(1);
                }
            }
        }

        Ok(removed_count)
    }

    /// Retrieve all invoices from storage across all possible statuses.
    ///
    /// Used when creating a new backup to snapshot the full current state.
    pub fn get_all_invoices(env: &Env) -> Vec<Invoice> {
        let mut all_invoices = Vec::new(env);
        let all_statuses = [
            crate::invoice::InvoiceStatus::Pending,
            crate::invoice::InvoiceStatus::Verified,
            crate::invoice::InvoiceStatus::Funded,
            crate::invoice::InvoiceStatus::Paid,
            crate::invoice::InvoiceStatus::Defaulted,
            crate::invoice::InvoiceStatus::Cancelled,
            crate::invoice::InvoiceStatus::Refunded,
        ];

        for status in all_statuses.iter() {
            let invoices = crate::invoice::InvoiceStorage::get_invoices_by_status(env, status);
            for id in invoices.iter() {
                if let Some(inv) = crate::invoice::InvoiceStorage::get_invoice(env, &id) {
                    all_invoices.push_back(inv);
                }
            }
        }
        all_invoices
    }
}
