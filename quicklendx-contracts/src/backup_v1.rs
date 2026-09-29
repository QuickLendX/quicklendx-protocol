use crate::backup::BackupStatus;
use soroban_sdk::{contracttype, BytesN, String};

/// Stored snapshot of all invoices at a point in time (V1 format).
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BackupV1 {
    pub backup_id: BytesN<32>,
    pub timestamp: u64,
    pub description: String,
    pub invoice_count: u32,
    pub status: BackupStatus,
}
