use crate::admin::AdminStorage;
use crate::errors::QuickLendXError;
use soroban_sdk::{symbol_short, Address, Env, Symbol};

/// Storage key for protocol pause flag.
const PAUSED_KEY: Symbol = symbol_short!("paused");

/// Pause controller for the QuickLendX protocol.
///
/// # Security Model
///
/// When the protocol is paused:
/// - **All user-facing state-mutating entrypoints** MUST reject with `OperationNotAllowed`.
/// - **Admin-level configuration** (e.g., fee updates, verification) is also restricted
///   during a total pause to ensure protocol stability while investigating issues.
/// - **Read methods (getters)** remain fully available.
/// - **Emergency recovery flows** (e.g., emergency withdraw) remain available as a last resort.
/// - Only the current Admin can call `pause` or `unpause`.
pub struct PauseControl;

impl PauseControl {
    /// Returns true if the protocol is currently paused.
    ///
    /// # Returns
    /// * `bool` - Current pause status
    pub fn is_paused(env: &Env) -> bool {
        env.storage().instance().get(&PAUSED_KEY).unwrap_or(false)
    }

    /// Set the pause flag (admin only).
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `admin` - The address of the caller (must be admin)
    /// * `paused` - The new pause state
    ///
    /// # Returns
    /// * `Ok(())` on success
    /// * `Err(QuickLendXError::NotAdmin)` if caller is not admin
    pub fn set_paused(env: &Env, admin: &Address, paused: bool) -> Result<(), QuickLendXError> {
        admin.require_auth();
        if !AdminStorage::is_admin(env, admin) {
            return Err(QuickLendXError::NotAdmin);
        }

        env.storage().instance().set(&PAUSED_KEY, &paused);
        Ok(())
    }

    /// Panic if the protocol is currently paused.
    ///
    /// This guard should be included at the beginning of any function that modifies
    /// the contract state, except for initialization and unpausing.
    ///
    /// # Panics
    /// * `QuickLendXError::OperationNotAllowed` - if the protocol is paused
    pub fn require_not_paused(env: &Env) {
        if Self::is_paused(env) {
            soroban_sdk::panic_with_error!(env, QuickLendXError::OperationNotAllowed);
        }
    }
}
