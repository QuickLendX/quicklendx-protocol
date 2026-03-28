use crate::admin::AdminStorage;
use crate::errors::QuickLendXError;
use soroban_sdk::{symbol_short, Address, Env, Symbol};

/// Storage key for protocol pause flag.
const PAUSED_KEY: Symbol = symbol_short!("paused");

/// Pause controller for the protocol.
///
/// When the protocol is paused:
/// - Non-view, non-admin entrypoints MUST reject with `OperationNotAllowed`
/// - Governance and emergency recovery entrypoints remain available
/// - Admin-only business-state mutations still decide explicitly whether they
///   are pause-gated at the entrypoint level
pub struct PauseControl;

impl PauseControl {
    /// @notice Return whether the protocol is currently paused.
    ///
    /// Returns true if the protocol is currently paused.
    pub fn is_paused(env: &Env) -> bool {
        env.storage().instance().get(&PAUSED_KEY).unwrap_or(false)
    }

    /// @notice Set the global pause flag.
    /// @dev This path is intentionally pause-exempt so an admin can both enter
    ///      and exit emergency mode while user/business flows are frozen.
    ///
    /// Set the pause flag (admin only).
    pub fn set_paused(env: &Env, admin: &Address, paused: bool) -> Result<(), QuickLendXError> {
        AdminStorage::require_admin_auth(env, admin)?;

        env.storage().instance().set(&PAUSED_KEY, &paused);
        Ok(())
    }

    /// @notice Reject business-state operations while the protocol is paused.
    /// @dev Entry points that intentionally remain available during an incident
    ///      must avoid calling this helper and document that exemption clearly.
    ///
    /// Require that the protocol is not paused.
    ///
    /// Returns `OperationNotAllowed` when paused.
    pub fn require_not_paused(env: &Env) -> Result<(), QuickLendXError> {
        if Self::is_paused(env) {
            return Err(QuickLendXError::ContractPaused);
        }
        Ok(())
    }
}
