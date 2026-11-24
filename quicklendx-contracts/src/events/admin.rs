#![allow(deprecated)]

use crate::audit::OpType;
use soroban_sdk::{contractevent, symbol_short, Address, BytesN, Env, String, Vec};

#[contractevent]
pub struct AuditValidation {
    pub invoice_id: BytesN<32>,
    pub is_valid: bool,
    pub timestamp: u64,
}

#[contractevent]
pub struct AuditQuery {
    pub query_type: OpType,
    pub result_count: u32,
}

#[contractevent]
pub struct TtlExtended {
    pub kind: String,
    pub count: u32,
}

pub fn emit_ttl_extended(env: &Env, kind: &String, count: u32) {
    TtlExtended {
        kind: kind.clone(),
        count,
    }
    .publish(env);
}

#[contractevent]
pub struct AdminInitialized {
    pub admin: Address,
}

#[contractevent]
pub struct ProtocolInitialized {
    pub admin: Address,
    pub treasury: Address,
    pub fee_bps: u32,
    pub min_invoice_amount: i128,
    pub max_due_date_days: u64,
    pub grace_period_seconds: u64,
    pub backfill_max_batch_size: u32,
    pub corridors: Vec<Address>,
    pub timestamp: u64,
}

#[contractevent]
pub struct ScanItemFailed {
    pub invoice_id: BytesN<32>,
    pub error_code: u32,
}

#[contractevent]
pub struct Paused {
    pub admin: Address,
}

#[contractevent]
pub struct Unpaused {
    pub admin: Address,
}

pub fn emit_paused(env: &Env, admin: &Address) {
    Paused {
        admin: admin.clone(),
    }
    .publish(env);
}

pub fn emit_unpaused(env: &Env, admin: &Address) {
    Unpaused {
        admin: admin.clone(),
    }
    .publish(env);
}

pub fn emit_scan_item_failed(env: &Env, invoice_id: &BytesN<32>, error_code: u32) {
    ScanItemFailed {
        invoice_id: invoice_id.clone(),
        error_code,
    }
    .publish(env);
}

pub fn emit_audit_validation(env: &Env, invoice_id: &BytesN<32>, is_valid: bool) {
    AuditValidation {
        invoice_id: invoice_id.clone(),
        is_valid,
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}

pub fn emit_audit_query(env: &Env, query_type: OpType, result_count: u32) {
    AuditQuery {
        query_type,
        result_count,
    }
    .publish(env);
}

#[contractevent]
pub struct EmergencyWithdrawalInitiated {
    pub token: Address,
    pub amount: i128,
    pub target: Address,
    pub unlock_at: u64,
    pub admin: Address,
}

#[contractevent]
pub struct EmergencyWithdrawalExecuted {
    pub token: Address,
    pub amount: i128,
    pub target: Address,
    pub admin: Address,
}

#[contractevent]
pub struct EmergencyWithdrawalCancelled {
    pub token: Address,
    pub amount: i128,
    pub target: Address,
    pub admin: Address,
}

#[contractevent]
pub struct AdminSet {
    pub admin: Address,
    pub timestamp: u64,
}

pub fn emit_emergency_withdrawal_initiated(
    env: &Env,
    token: Address,
    amount: i128,
    target: Address,
    unlock_at: u64,
    admin: Address,
) {
    EmergencyWithdrawalInitiated {
        token,
        amount,
        target,
        unlock_at,
        admin,
    }
    .publish(env);
}

pub fn emit_emergency_withdrawal_executed(
    env: &Env,
    token: Address,
    amount: i128,
    target: Address,
    admin: Address,
) {
    EmergencyWithdrawalExecuted {
        token,
        amount,
        target,
        admin,
    }
    .publish(env);
}

pub fn emit_emergency_withdrawal_cancelled(
    env: &Env,
    token: Address,
    amount: i128,
    target: Address,
    admin: Address,
) {
    EmergencyWithdrawalCancelled {
        token,
        amount,
        target,
        admin,
    }
    .publish(env);
}

pub fn emit_admin_set(env: &Env, admin: &Address) {
    AdminSet {
        admin: admin.clone(),
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}

pub fn emit_admin_transferred(env: &Env, old_admin: &Address, new_admin: &Address) {
    env.events().publish(
        (symbol_short!("adm_trf"),),
        (
            old_admin.clone(),
            new_admin.clone(),
            env.ledger().timestamp(),
        ),
    );
}

pub fn emit_admin_transfer_initiated(env: &Env, current_admin: &Address, pending_admin: &Address) {
    env.events().publish(
        (symbol_short!("adm_req"),),
        (
            current_admin.clone(),
            pending_admin.clone(),
            env.ledger().timestamp(),
        ),
    );
}

pub fn emit_admin_transfer_cancelled(env: &Env, current_admin: &Address, pending_admin: &Address) {
    env.events().publish(
        (symbol_short!("adm_cnl"),),
        (
            current_admin.clone(),
            pending_admin.clone(),
            env.ledger().timestamp(),
        ),
    );
}

pub fn emit_admin_two_step_updated(env: &Env, admin: &Address, enabled: bool) {
    env.events().publish(
        (symbol_short!("adm_2st"),),
        (admin.clone(), enabled, env.ledger().timestamp()),
    );
}

pub fn emit_protocol_initialized(
    env: &Env,
    admin: &Address,
    treasury: &Address,
    fee_bps: u32,
    min_invoice_amount: i128,
    max_due_date_days: u64,
    grace_period_seconds: u64,
    backfill_max_batch_size: u32,
    corridors: &Vec<Address>,
) {
    ProtocolInitialized {
        admin: admin.clone(),
        treasury: treasury.clone(),
        fee_bps,
        min_invoice_amount,
        max_due_date_days,
        grace_period_seconds,
        backfill_max_batch_size,
        corridors: corridors.clone(),
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}

pub fn emit_admin_initialized(env: &Env, admin: &Address) {
    env.events()
        .publish((symbol_short!("adm_init"),), (admin.clone(),));
}

// ── Upgrade events ──────────────────────────────────────────────────────────

/// Emitted when an admin schedules a WASM contract upgrade.
pub fn emit_upgrade_scheduled(
    env: &Env,
    admin: &Address,
    wasm_hash: &BytesN<32>,
    executable_at: u64,
) {
    env.events().publish(
        (symbol_short!("upg_sch"),),
        (
            admin.clone(),
            wasm_hash.clone(),
            env.ledger().timestamp(),
            executable_at,
        ),
    );
}

/// Emitted when an admin cancels a pending WASM upgrade.
pub fn emit_upgrade_cancelled(env: &Env, admin: &Address, wasm_hash: &BytesN<32>) {
    env.events().publish(
        (symbol_short!("upg_can"),),
        (admin.clone(), wasm_hash.clone()),
    );
}

/// Emitted when a pending WASM upgrade is executed (contract code replaced).
pub fn emit_upgrade_executed(env: &Env, admin: &Address, wasm_hash: &BytesN<32>) {
    env.events().publish(
        (symbol_short!("upg_exe"),),
        (admin.clone(), wasm_hash.clone()),
    );
}

// ── Incident mode events ─────────────────────────────────────────────────────

/// Emitted when the admin enters coordinated incident mode (pause + maintenance).
#[contractevent]
pub struct IncidentModeEntered {
    pub admin: Address,
    pub reason: String,
    pub timestamp: u64,
}

/// Emitted when the admin exits coordinated incident mode (unpause + disable maintenance).
#[contractevent]
pub struct IncidentModeExited {
    pub admin: Address,
    pub timestamp: u64,
}

pub fn emit_incident_mode_entered(env: &Env, admin: &Address, reason: &String) {
    IncidentModeEntered {
        admin: admin.clone(),
        reason: reason.clone(),
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}

pub fn emit_incident_mode_exited(env: &Env, admin: &Address) {
    IncidentModeExited {
        admin: admin.clone(),
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}

// ── Backfill lifecycle events ────────────────────────────────────────────────

/// Emitted when a destructive backfill (e.g. `restore_from_backup`) begins.
/// While this flag is set, the contract refuses to schedule a WASM upgrade.
pub fn backfill_started(env: &Env, actor: &Address) {
    env.events().publish(
        (symbol_short!("bkf_sta"),),
        (actor.clone(), env.ledger().timestamp()),
    );
}

/// Emitted when a backfill finishes — flag is cleared and contracts are free
/// to migrate again.
pub fn backfill_finished(env: &Env, actor: &Address, restored_count: u32) {
    env.events().publish(
        (symbol_short!("bkf_end"),),
        (actor.clone(), restored_count, env.ledger().timestamp()),
    );
}
