//! Contract upgrade control: schedule, cancel, and execute WASM upgrades.
//!
//! When an upgrade is scheduled the contract enters a read-only state:
//! all writable entrypoints are blocked until the upgrade is either
//! executed or cancelled.  This prevents state mutations between the
//! decision to upgrade and the actual code replacement, avoiding
//! data-format or storage-key incompatibilities between versions.

use crate::admin::AdminStorage;
use crate::errors::QuickLendXError;
use crate::pause::PauseReason;
use soroban_sdk::{symbol_short, Address, BytesN, Env, Symbol};

/// Minimum delay before a scheduled upgrade can be executed (48 hours).
/// Prevents same-ledger execution and gives users a window to exit escrow
/// if they do not trust the new code.
pub const UPGRADE_DELAY_SECS: u64 = 48 * 60 * 60; // 48 hours

const PENDING_UPGRADE_WASM_KEY: Symbol = symbol_short!("upg_wasm");
const PENDING_UPGRADE_AT_KEY: Symbol = symbol_short!("upg_at");
const PRE_UPGRADE_PAUSED_KEY: Symbol = symbol_short!("upg_paus");
const PRE_UPGRADE_PAUSE_RSN_KEY: Symbol = symbol_short!("upg_prsn");

pub struct UpgradeControl;

impl UpgradeControl {
    /// Check whether a contract upgrade is currently pending.
    pub fn is_pending_upgrade(env: &Env) -> bool {
        env.storage().instance().has(&PENDING_UPGRADE_WASM_KEY)
    }

    /// Get the pending upgrade details, if any.
    ///
    /// Returns `(wasm_hash, scheduled_at)` when an upgrade is pending,
    /// or `None` when no upgrade is scheduled.
    pub fn get_pending_upgrade(env: &Env) -> Option<(BytesN<32>, u64)> {
        let wasm_hash: BytesN<32> = env.storage().instance().get(&PENDING_UPGRADE_WASM_KEY)?;
        let scheduled_at: u64 = env
            .storage()
            .instance()
            .get(&PENDING_UPGRADE_AT_KEY)
            .unwrap_or(0);
        Some((wasm_hash, scheduled_at))
    }

    /// Guard: return `Err(UpgradePending)` when a contract upgrade has
    /// been scheduled but not yet executed or cancelled.
    pub fn require_no_pending_upgrade(env: &Env) -> Result<(), QuickLendXError> {
        if Self::is_pending_upgrade(env) {
            return Err(QuickLendXError::UpgradePending);
        }
        Ok(())
    }

    /// Schedule a WASM upgrade.
    ///
    /// Stores the new WASM hash and records the current ledger timestamp.
    /// After this call all writable entrypoints are blocked until
    /// [`execute_upgrade`] or [`cancel_upgrade`] is called.
    ///
    /// # Arguments
    /// * `env` — The contract environment.
    /// * `admin` — The current contract admin (must sign).
    /// * `wasm_hash` — 32-byte hash of the new WASM blob.
    pub fn schedule_upgrade(
        env: &Env,
        admin: &Address,
        wasm_hash: &BytesN<32>,
    ) -> Result<(), QuickLendXError> {
        admin.require_auth();
        AdminStorage::require_admin(env, admin)?;

        if Self::is_pending_upgrade(env) {
            return Err(QuickLendXError::OperationNotAllowed);
        }

        // Migration safety: refuse to schedule an upgrade while a destructive
        // backfill (currently `restore_from_backup`) is mutating invoice
        // state. Letting the new contract code come online between clear and
        // rebuild would leave it reading half-restored state with no flag to
        // detect the partial view.
        crate::backup::BackupStorage::require_no_pending_backfill(env)?;

        // Store pre-existing pause state so we can restore it after cancel/execute
        let was_paused = crate::pause::PauseControl::is_paused(env);
        let prev_reason = crate::pause::PauseControl::pause_reason(env);
        
        env.storage()
            .instance()
            .set(&PRE_UPGRADE_PAUSED_KEY, &was_paused);
        if let Some(reason) = prev_reason {
            env.storage()
                .instance()
                .set(&PRE_UPGRADE_PAUSE_RSN_KEY, &reason);
        }

        let scheduled_at = env.ledger().timestamp();
        env.storage()
            .instance()
            .set(&PENDING_UPGRADE_WASM_KEY, wasm_hash);
        env.storage()
            .instance()
            .set(&PENDING_UPGRADE_AT_KEY, &scheduled_at);

        // Auto-pause: block state mutations until the upgrade is resolved.
        crate::pause::PauseControl::apply_paused(
            env,
            true,
            Some(crate::pause::PauseReason::PendingUpgrade),
        );

        let executable_at = scheduled_at.saturating_add(UPGRADE_DELAY_SECS);
        crate::events::emit_upgrade_scheduled(env, admin, wasm_hash, executable_at);
        Ok(())
    }

    /// Cancel a pending upgrade without executing it.
    ///
    /// Clears the upgrade state and restores the pre-upgrade pause state.
    pub fn cancel_upgrade(env: &Env, admin: &Address) -> Result<(), QuickLendXError> {
        admin.require_auth();
        AdminStorage::require_admin(env, admin)?;

        if !Self::is_pending_upgrade(env) {
            return Err(QuickLendXError::OperationNotAllowed);
        }

        let wasm_hash: BytesN<32> = env
            .storage()
            .instance()
            .get(&PENDING_UPGRADE_WASM_KEY)
            .unwrap();
        
        // Restore the pause state that existed before scheduling
        let was_paused: bool = env
            .storage()
            .instance()
            .get(&PRE_UPGRADE_PAUSED_KEY)
            .unwrap_or(false);
        let prev_reason: Option<PauseReason> = env
            .storage()
            .instance()
            .get(&PRE_UPGRADE_PAUSE_RSN_KEY);

        env.storage().instance().remove(&PENDING_UPGRADE_WASM_KEY);
        env.storage().instance().remove(&PENDING_UPGRADE_AT_KEY);
        env.storage().instance().remove(&PRE_UPGRADE_PAUSED_KEY);
        env.storage().instance().remove(&PRE_UPGRADE_PAUSE_RSN_KEY);

        // Restore the pause state from before the upgrade was scheduled
        crate::pause::PauseControl::apply_paused(env, was_paused, prev_reason);

        crate::events::emit_upgrade_cancelled(env, admin, &wasm_hash);
        Ok(())
    }

    /// Execute a pending WASM upgrade.
    ///
    /// Replaces the contract code with the new WASM blob identified by
    /// the hash stored during [`schedule_upgrade`]. Can only be called
    /// after the timelock delay has elapsed. After this call the
    /// contract runs the new code and the pre-upgrade pause state is restored.
    pub fn execute_upgrade(env: &Env, admin: &Address) -> Result<(), QuickLendXError> {
        admin.require_auth();
        AdminStorage::require_admin(env, admin)?;

        let wasm_hash: BytesN<32> = env
            .storage()
            .instance()
            .get(&PENDING_UPGRADE_WASM_KEY)
            .ok_or(QuickLendXError::OperationNotAllowed)?;

        let scheduled_at: u64 = env
            .storage()
            .instance()
            .get(&PENDING_UPGRADE_AT_KEY)
            .unwrap_or(0);

        // Enforce the timelock: upgrade cannot execute until delay has elapsed
        let now = env.ledger().timestamp();
        if now < scheduled_at.saturating_add(UPGRADE_DELAY_SECS) {
            return Err(QuickLendXError::UpgradeTimelockNotElapsed);
        }

        // Restore the pause state that existed before scheduling
        let was_paused: bool = env
            .storage()
            .instance()
            .get(&PRE_UPGRADE_PAUSED_KEY)
            .unwrap_or(false);
        let prev_reason: Option<PauseReason> = env
            .storage()
            .instance()
            .get(&PRE_UPGRADE_PAUSE_RSN_KEY);

        env.storage().instance().remove(&PENDING_UPGRADE_WASM_KEY);
        env.storage().instance().remove(&PENDING_UPGRADE_AT_KEY);
        env.storage().instance().remove(&PRE_UPGRADE_PAUSED_KEY);
        env.storage().instance().remove(&PRE_UPGRADE_PAUSE_RSN_KEY);

        crate::pause::PauseControl::apply_paused(env, was_paused, prev_reason);

        crate::events::emit_upgrade_executed(env, admin, &wasm_hash);

        env.deployer()
            .update_current_contract_wasm(wasm_hash.clone());

        Ok(())
    }
}
