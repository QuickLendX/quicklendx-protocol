//! Contract upgrade control: schedule, cancel, and execute WASM upgrades.
//!
//! When an upgrade is scheduled the contract enters a read-only state:
//! all writable entrypoints are blocked until the upgrade is either
//! executed or cancelled.  This prevents state mutations between the
//! decision to upgrade and the actual code replacement, avoiding
//! data-format or storage-key incompatibilities between versions.

use crate::admin::AdminStorage;
use crate::errors::QuickLendXError;
use soroban_sdk::{symbol_short, Address, BytesN, Env, Symbol};

const PENDING_UPGRADE_WASM_KEY: Symbol = symbol_short!("upg_wasm");
const PENDING_UPGRADE_AT_KEY: Symbol = symbol_short!("upg_at");

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

        env.storage()
            .instance()
            .set(&PENDING_UPGRADE_WASM_KEY, wasm_hash);
        env.storage()
            .instance()
            .set(&PENDING_UPGRADE_AT_KEY, &env.ledger().timestamp());

        // Auto-pause: block state mutations until the upgrade is resolved.
        crate::pause::PauseControl::apply_paused(
            env,
            true,
            Some(crate::pause::PauseReason::PendingUpgrade),
        );

        crate::events::emit_upgrade_scheduled(env, admin, wasm_hash);
        Ok(())
    }

    /// Cancel a pending upgrade without executing it.
    ///
    /// Clears the upgrade state and un-pauses the contract so normal
    /// operations can resume.
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
        env.storage().instance().remove(&PENDING_UPGRADE_WASM_KEY);
        env.storage().instance().remove(&PENDING_UPGRADE_AT_KEY);

        // Restore write access.
        crate::pause::PauseControl::apply_paused(env, false, None);

        crate::events::emit_upgrade_cancelled(env, admin, &wasm_hash);
        Ok(())
    }

    /// Execute a pending WASM upgrade.
    ///
    /// Replaces the contract code with the new WASM blob identified by
    /// the hash stored during [`schedule_upgrade`].  After this call the
    /// contract runs the new code and writes are re-enabled.
    pub fn execute_upgrade(env: &Env, admin: &Address) -> Result<(), QuickLendXError> {
        admin.require_auth();
        AdminStorage::require_admin(env, admin)?;

        let wasm_hash: BytesN<32> = env
            .storage()
            .instance()
            .get(&PENDING_UPGRADE_WASM_KEY)
            .ok_or(QuickLendXError::OperationNotAllowed)?;

        env.storage().instance().remove(&PENDING_UPGRADE_WASM_KEY);
        env.storage().instance().remove(&PENDING_UPGRADE_AT_KEY);
        crate::pause::PauseControl::apply_paused(env, false, None);

        crate::events::emit_upgrade_executed(env, admin, &wasm_hash);

        env.deployer()
            .update_current_contract_wasm(wasm_hash.clone());

        Ok(())
    }
}
