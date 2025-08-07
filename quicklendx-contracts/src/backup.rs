use crate::errors::QuickLendXError;
use crate::invoice::{Invoice};
use soroban_sdk::{contracttype, symbol_short, BytesN, Env, String, Vec};

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

pub struct BackupStorage;

impl BackupStorage {
    /// Generate a unique backup ID
    pub fn generate_backup_id(env: &Env) -> BytesN<32> {
        let timestamp = env.ledger().timestamp();
        let counter_key = symbol_short!("bkup_cnt");
        let counter: u64 = env.storage().instance().get(&counter_key).unwrap_or(0);
        env.storage().instance().set(&counter_key, &(counter + 1));

        let mut id_bytes = [0u8; 32];
        // Add backup prefix
        id_bytes[0] = 0xB4; // 'B' for Backup
        id_bytes[1] = 0xC4; // 'C' for baCkup
                            // Embed timestamp
        id_bytes[2..10].copy_from_slice(&timestamp.to_be_bytes());
        // Embed counter
        id_bytes[10..18].copy_from_slice(&counter.to_be_bytes());
        // Fill remaining bytes
        for i in 18..32 {
            id_bytes[i] = ((timestamp + counter + 0xB4C4) % 256) as u8;
        }

        BytesN::from_array(env, &id_bytes)
    }

    /// Store a backup record
    pub fn store_backup(env: &Env, backup: &Backup) {
        env.storage().instance().set(&backup.backup_id, backup);
    }

    /// Get a backup by ID
    pub fn get_backup(env: &Env, backup_id: &BytesN<32>) -> Option<Backup> {
        env.storage().instance().get(backup_id)
    }

    /// Update a backup record
    pub fn update_backup(env: &Env, backup: &Backup) {
        env.storage().instance().set(&backup.backup_id, backup);
    }

    /// Get all backup IDs
    pub fn get_all_backups(env: &Env) -> Vec<BytesN<32>> {
        let key = symbol_short!("backups");
        env.storage()
            .instance()
            .get(&key)
            .unwrap_or_else(|| Vec::new(env))
    }

    /// Add backup to the list of all backups
    pub fn add_to_backup_list(env: &Env, backup_id: &BytesN<32>) {
        let mut backups = Self::get_all_backups(env);
        backups.push_back(backup_id.clone());
        env.storage()
            .instance()
            .set(&symbol_short!("backups"), &backups);
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
        env.storage()
            .instance()
            .set(&symbol_short!("backups"), &new_backups);
    }

    /// Store invoice data for a backup
    pub fn store_backup_data(env: &Env, backup_id: &BytesN<32>, invoices: &Vec<Invoice>) {
        let key = (symbol_short!("bkup_data"), backup_id.clone());
        env.storage().instance().set(&key, invoices);
    }

    /// Get invoice data from a backup
    pub fn get_backup_data(env: &Env, backup_id: &BytesN<32>) -> Option<Vec<Invoice>> {
        let key = (symbol_short!("bkup_data"), backup_id.clone());
        env.storage().instance().get(&key)
    }

    /// Validate backup data integrity
    pub fn validate_backup(env: &Env, backup_id: &BytesN<32>) -> Result<(), QuickLendXError> {
        let backup = Self::get_backup(env, backup_id).ok_or(QuickLendXError::StorageKeyNotFound)?;

        let data =
            Self::get_backup_data(env, backup_id).ok_or(QuickLendXError::StorageKeyNotFound)?;

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

    /// Clean up old backups (keep only the last N)
    pub fn cleanup_old_backups(env: &Env, max_backups: u32) -> Result<(), QuickLendXError> {
        let backups = Self::get_all_backups(env);
        let max_backups: usize = max_backups.try_into().unwrap();

        if backups.len() <= max_backups.try_into().unwrap() {
            return Ok(());
        }

        // Create a vector of tuples (backup_id, timestamp) for sorting
        let mut backup_timestamps = Vec::new(env);
        for backup_id in backups.iter() {
            if let Some(backup) = Self::get_backup(env, &backup_id) {
                backup_timestamps.push_back((backup_id, backup.timestamp));
            }
        }

        // Sort by timestamp (oldest first) using bubble sort
        // This is a simple sorting algorithm that works well for small lists
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

        // Remove oldest backups until we're under the limit
        while backup_timestamps.len() > max_backups.try_into().unwrap() {
            if let Some((oldest_id, _)) = backup_timestamps.first() {
                Self::remove_from_backup_list(env, &oldest_id);
                backup_timestamps.remove(0);
            }
        }

        Ok(())
    }
}
