#![allow(deprecated)]

use soroban_sdk::{contractevent, BytesN, Env};

#[contractevent]
pub struct BackupCreated {
    pub backup_id: BytesN<32>,
    pub invoice_count: u32,
    pub timestamp: u64,
}

#[contractevent]
pub struct BackupRestored {
    pub backup_id: BytesN<32>,
    pub invoice_count: u32,
    pub timestamp: u64,
}

#[contractevent]
pub struct BackupValidated {
    pub backup_id: BytesN<32>,
    pub success: bool,
    pub timestamp: u64,
}

#[contractevent]
pub struct BackupArchived {
    pub backup_id: BytesN<32>,
    pub timestamp: u64,
}

#[contractevent]
pub struct RetentionPolicyUpdated {
    pub max_backups: u32,
    pub max_age_seconds: u64,
    pub auto_cleanup_enabled: bool,
    pub timestamp: u64,
}

#[contractevent]
pub struct BackupsCleaned {
    pub removed_count: u32,
    pub timestamp: u64,
}

pub fn emit_backup_created(env: &Env, backup_id: &BytesN<32>, invoice_count: u32) {
    BackupCreated {
        backup_id: backup_id.clone(),
        invoice_count,
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}

pub fn emit_backup_restored(env: &Env, backup_id: &BytesN<32>, invoice_count: u32) {
    BackupRestored {
        backup_id: backup_id.clone(),
        invoice_count,
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}

pub fn emit_backup_validated(env: &Env, backup_id: &BytesN<32>, success: bool) {
    BackupValidated {
        backup_id: backup_id.clone(),
        success,
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}

pub fn emit_backup_archived(env: &Env, backup_id: &BytesN<32>) {
    BackupArchived {
        backup_id: backup_id.clone(),
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}

pub fn emit_retention_policy_updated(
    env: &Env,
    max_backups: u32,
    max_age_seconds: u64,
    auto_cleanup_enabled: bool,
) {
    RetentionPolicyUpdated {
        max_backups,
        max_age_seconds,
        auto_cleanup_enabled,
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}

pub fn emit_backups_cleaned(env: &Env, removed_count: u32) {
    BackupsCleaned {
        removed_count,
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}
