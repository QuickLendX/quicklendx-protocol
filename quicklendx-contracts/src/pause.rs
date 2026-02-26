use crate::admin::AdminStorage;
use crate::errors::QuickLendXError;
use soroban_sdk::{symbol_short, Address, Env, Symbol};

/// Storage key for protocol pause flag.
const PAUSED_KEY: Symbol = symbol_short!("paused");

/// Pause controller for the protocol.
///
/// When the protocol is paused:
/// - Non-view, non-admin entrypoints MUST reject with `OperationNotAllowed`
/// - Admin configuration and emergency flows remain available
pub struct PauseControl;

impl PauseControl {
    /// Returns true if the protocol is currently paused.
    pub fn is_paused(env: &Env) -> bool {
        env.storage()
            .instance()
            .get(&PAUSED_KEY)
            .unwrap_or(false)
    }

    /// Set the pause flag (admin only).
    pub fn set_paused(env: &Env, admin: &Address, paused: bool) -> Result<(), QuickLendXError> {
        admin.require_auth();
        if !AdminStorage::is_admin(env, admin) {
            return Err(QuickLendXError::NotAdmin);
        }

        env.storage().instance().set(&PAUSED_KEY, &paused);
        Ok(())
    }

    /// Require that the protocol is not paused.
    ///
    /// Returns `OperationNotAllowed` when paused.
    pub fn require_not_paused(env: &Env) -> Result<(), QuickLendXError> {
        if Self::is_paused(env) {
            return Err(QuickLendXError::OperationNotAllowed);
        }
        Ok(())
    }
}

