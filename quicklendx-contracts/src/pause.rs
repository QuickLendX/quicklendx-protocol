//! Emergency pause control for state-changing contract operations.
//!
//! When paused, all mutating entrypoints must reject execution.
//! Read-only queries remain available.

use crate::admin::AdminStorage;
use crate::errors::QuickLendXError;
use soroban_sdk::{symbol_short, Address, Env, Symbol};

const PAUSED_KEY: Symbol = symbol_short!("paused");

pub struct PauseState;

impl PauseState {
    /// Returns true when the protocol is paused.
    pub fn is_paused(env: &Env) -> bool {
        env.storage().instance().get(&PAUSED_KEY).unwrap_or(false)
    }

    /// Pause the protocol (admin only).
    pub fn pause(env: &Env, admin: &Address) -> Result<(), QuickLendXError> {
        admin.require_auth();
        AdminStorage::require_admin(env, admin)?;
        env.storage().instance().set(&PAUSED_KEY, &true);
        Ok(())
    }

    /// Unpause the protocol (admin only).
    pub fn unpause(env: &Env, admin: &Address) -> Result<(), QuickLendXError> {
        admin.require_auth();
        AdminStorage::require_admin(env, admin)?;
        env.storage().instance().set(&PAUSED_KEY, &false);
        Ok(())
    }

    /// Reject state-changing operations when the protocol is paused.
    pub fn require_not_paused(env: &Env) -> Result<(), QuickLendXError> {
        if Self::is_paused(env) {
            return Err(QuickLendXError::ProtocolPaused);
        }
        Ok(())
    }
}
