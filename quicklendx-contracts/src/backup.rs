use crate::errors::QuickLendXError;
use crate::invoice::Invoice;
use soroban_sdk::{contracttype, symbol_short, BytesN, Env, String, Vec};

const RETENTION_POLICY_KEY: soroban_sdk::Symbol = symbol_short!("bkup_pol");
const BACKUP_COUNTER_KEY: soroban_sdk::Symbol = symbol_short!("bkup_cnt");
const BACKUP_LIST_KEY: soroban_sdk::Symbol = symbol_short!("backups");
const BACKUP_DATA_KEY: soroban_sdk::Symbol = symbol_short!("bkup_data");
const MAX_BACKUP_DESCRIPTION_LENGTH: u32 = 128;

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Backup {
    pub backup_id: BytesN<32>,
    pub timestamp: u64,
    pub description: String,
    pub invoice_count: u32,
    pub status: BackupStatus,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BackupStatus {
    Active,
    Archived,
    Corrupted,
}

/// Backup retention policy configuration
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BackupRetentionPolicy {
    /// Maximum number of backups to keep (0 = unlimited)
    pub max_backups: u32,
    /// Maximum age of backups in seconds (0 = unlimited)
    pub max_age_seconds: u64,
    /// Whether automatic cleanup is enabled
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

pub struct BackupStorage;

impl BackupStorage {
    /// @notice Validate backup metadata before persisting it.
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

    /// @notice Validate the backup identifier prefix format.
    pub fn is_valid_backup_id(backup_id: &BytesN<32>) -> bool {
        let bytes = backup_id.to_array();
        bytes[0] == 0xB4 && bytes[1] == 0xC4
    }

    /// Get the backup retention policy
    pub fn get_retention_policy(env: &Env) -> BackupRetentionPolicy {
        env.storage()
            .instance()
            .get(&RETENTION_POLICY_KEY)
            .unwrap_or_else(|| BackupRetentionPolicy::default())
    }

    /// Set the backup retention policy (admin only)
    pub fn set_retention_policy(env: &Env, policy: &BackupRetentionPolicy) {
        env.storage().instance().set(&RETENTION_POLICY_KEY, policy);
    }

    /// Generate a unique backup ID
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
        // Add backup prefix
        id_bytes[0] = 0xB4; // 'B' for Backup
        id_bytes[1] = 0xC4; // 'C' for baCkup
                            // Embed timestamp
        id_bytes[2..10].copy_from_slice(&timestamp.to_be_bytes());
        // Embed counter
        id_bytes[10..18].copy_from_slice(&next_counter.to_be_bytes());
        // Fill remaining bytes (overflow-safe)
        let mix = timestamp
            .saturating_add(next_counter)
            .saturating_add(0xB4C4);
        for i in 18..32 {
            id_bytes[i] = (mix % 256) as u8;
        }

        BytesN::from_array(env, &id_bytes)
    }

    /// Store a backup record
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

    /// Get a backup by ID
    pub fn get_backup(env: &Env, backup_id: &BytesN<32>) -> Option<Backup> {
        env.storage().instance().get(backup_id)
    }

    /// Update a backup record
    pub fn update_backup(env: &Env, backup: &Backup) -> Result<(), QuickLendXError> {
        Self::validate_backup_metadata(backup, None)?;
        env.storage().instance().set(&backup.backup_id, backup);
        Ok(())
    }

    /// Get all backup IDs
    pub fn get_all_backups(env: &Env) -> Vec<BytesN<32>> {
        env.storage()
            .instance()
            .get(&BACKUP_LIST_KEY)
            .unwrap_or_else(|| Vec::new(env))
    }

    /// Add backup to the list of all backups
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

    /// Remove backup from the list (when archived or corrupted)
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

    /// Store invoice data for a backup
    pub fn store_backup_data(env: &Env, backup_id: &BytesN<32>, invoices: &Vec<Invoice>) {
        let key = (BACKUP_DATA_KEY, backup_id.clone());
        env.storage().instance().set(&key, invoices);
    }

    /// Get invoice data from a backup
    pub fn get_backup_data(env: &Env, backup_id: &BytesN<32>) -> Option<Vec<Invoice>> {
        let key = (BACKUP_DATA_KEY, backup_id.clone());
        env.storage().instance().get(&key)
    }

    /// @notice Delete a backup record and its stored data.
    pub fn purge_backup(env: &Env, backup_id: &BytesN<32>) {
        Self::remove_from_backup_list(env, backup_id);
        env.storage().instance().remove(backup_id);
        let data_key = (BACKUP_DATA_KEY, backup_id.clone());
        env.storage().instance().remove(&data_key);
    }

    /// Validate backup data integrity
    pub fn validate_backup(env: &Env, backup_id: &BytesN<32>) -> Result<(), QuickLendXError> {
        let backup = Self::get_backup(env, backup_id).ok_or(QuickLendXError::StorageKeyNotFound)?;
        Self::validate_backup_metadata(&backup, None)?;

        let data =
            Self::get_backup_data(env, backup_id).ok_or(QuickLendXError::StorageKeyNotFound)?;

        Self::validate_backup_metadata(&backup, Some(&data))?;

        // Check if count matches
        if data.len() as u32 != backup.invoice_count {
            return Err(QuickLendXError::StorageError);
        }

        // Check each invoice has valid data
        for invoice in data.iter() {
            if invoice.amount <= 0 {
                return Err(QuickLendXError::StorageError);
            }
        }

        Ok(())
    }

    /// Clean up old backups based on retention policy
    pub fn cleanup_old_backups(env: &Env) -> Result<u32, QuickLendXError> {
        let policy = Self::get_retention_policy(env);

        // If auto cleanup is disabled, do nothing
        if !policy.auto_cleanup_enabled {
            return Ok(0);
        }

        let backups = Self::get_all_backups(env);
        let current_time = env.ledger().timestamp();
        let mut removed_count = 0u32;

        // Create a vector of tuples (backup_id, timestamp) for sorting
        let mut backup_timestamps = Vec::new(env);
        for backup_id in backups.iter() {
            if let Some(backup) = Self::get_backup(env, &backup_id) {
                // Only consider active backups for cleanup
                if backup.status == BackupStatus::Active {
                    backup_timestamps.push_back((backup_id, backup.timestamp));
                }
            }
        }

        // Sort by timestamp (oldest first) using bubble sort
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

        // First, remove backups that exceed max age (if configured)
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

        // Then, remove oldest backups if we exceed max_backups (if configured)
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

    /// Retrieve all invoices from storage across all possible statuses
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

    /// Create a new backup of all invoices
    pub fn create_backup(env: &Env) -> Result<BytesN<32>, QuickLendXError> {
        let backup_id = Self::generate_backup_id(env);
        let invoices = Self::get_all_invoices(env);
        let b = Backup {
            backup_id: backup_id.clone(),
            timestamp: env.ledger().timestamp(),
            description: String::from_str(env, "Manual Backup"),
            invoice_count: invoices.len() as u32,
            status: BackupStatus::Active,
        };
        Self::store_backup(env, &b, Some(&invoices))?;
        Self::store_backup_data(env, &backup_id, &invoices);
        Self::add_to_backup_list(env, &backup_id);
        let _ = Self::cleanup_old_backups(env);
        Ok(backup_id)
    }

    /// Restore from a backup
    pub fn restore_backup(env: &Env, backup_id: &BytesN<32>) -> Result<(), QuickLendXError> {
        Self::validate_backup(env, backup_id)?;
        let invoices = Self::get_backup_data(env, backup_id)
            .ok_or(QuickLendXError::InvoiceNotFound)?;
        crate::invoice::InvoiceStorage::clear_all(env);
        for inv in invoices.iter() {
            crate::invoice::InvoiceStorage::store_invoice(env, &inv);
        }
        Ok(())
    }
}
