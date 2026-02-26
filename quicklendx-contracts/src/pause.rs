//! Emergency pause: when the contract is paused, mutating operations are blocked;
//! getters remain allowed. Only admin can pause and unpause.

use crate::errors::QuickLendXError;
use crate::AdminStorage;
use soroban_sdk::{symbol_short, Address, Env, Symbol};

const PAUSED_KEY: Symbol = symbol_short!("paused");

/// Pause state and admin-only pause/unpause.
pub struct Pause;

impl Pause {
    /// Return whether the contract is currently paused.
    pub fn is_paused(env: &Env) -> bool {
        env.storage()
            .instance()
            .get(&PAUSED_KEY)
            .unwrap_or(false)
    }

    /// Set paused flag. Only admin. Used by pause() and unpause().
    fn set_paused(env: &Env, paused: bool) {
        env.storage().instance().set(&PAUSED_KEY, &paused);
    }

    /// Pause the contract (admin only). Mutating operations will return ContractPaused until unpause.
    pub fn pause(env: &Env, admin: &Address) -> Result<(), QuickLendXError> {
        AdminStorage::require_admin(env, admin)?;
        admin.require_auth();
        Self::set_paused(env, true);
        env.events()
            .publish((symbol_short!("paused"),), (admin.clone(), env.ledger().timestamp()));
        Ok(())
    }

    /// Unpause the contract (admin only).
    pub fn unpause(env: &Env, admin: &Address) -> Result<(), QuickLendXError> {
        AdminStorage::require_admin(env, admin)?;
        admin.require_auth();
        Self::set_paused(env, false);
        env.events()
            .publish((symbol_short!("unpaused"),), (admin.clone(), env.ledger().timestamp()));
        Ok(())
    }

    /// Require that the contract is not paused. Call at the start of mutating entrypoints.
    pub fn require_not_paused(env: &Env) -> Result<(), QuickLendXError> {
        if Self::is_paused(env) {
            return Err(QuickLendXError::ContractPaused);
        }
        Ok(())
    }
}
