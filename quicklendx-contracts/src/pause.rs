use crate::admin::AdminStorage;
use crate::errors::QuickLendXError;
use soroban_sdk::{symbol_short, Address, Env, String, Symbol};

const PAUSED_KEY: Symbol = symbol_short!("paused");
const PAUSED_AT_KEY: Symbol = symbol_short!("paused_at");
const MAX_PAUSE_DURATION: u64 = 7 * 24 * 3600;

pub struct PauseControl;

impl PauseControl {
    pub fn is_paused(env: &Env) -> bool {
        if !env.storage().instance().get(&PAUSED_KEY).unwrap_or(false) {
            return false;
        }
        let paused_at: u64 = env.storage().instance().get(&PAUSED_AT_KEY).unwrap_or(0);
        if paused_at > 0 && env.ledger().timestamp() >= paused_at + MAX_PAUSE_DURATION {
            env.storage().instance().set(&PAUSED_KEY, &false);
            return false;
        }
        true
    }

    pub fn set_paused(env: &Env, admin: &Address, paused: bool) -> Result<(), QuickLendXError> {
        admin.require_auth();
        AdminStorage::require_admin(env, admin)?;
        Self::apply_paused(env, paused);
        Ok(())
    }

    pub(crate) fn apply_paused(env: &Env, paused: bool) {
        env.storage().instance().set(&PAUSED_KEY, &paused);
        if paused {
            env.storage().instance().set(&PAUSED_AT_KEY, &env.ledger().timestamp());
        }
    }

    pub fn require_not_paused(env: &Env) -> Result<(), QuickLendXError> {
        if Self::is_paused(env) {
            return Err(QuickLendXError::ContractPaused);
        }
        Ok(())
    }

    /// Return whether a specific guarded entrypoint is currently blocked by pause.
    ///
    /// This is a frontend-friendly read-only getter that accepts a stable entrypoint
    /// symbol (`EP_*`) and returns `true` when the protocol is paused and the
    /// named entrypoint is part of the guarded set.
    pub fn is_entrypoint_paused(env: &Env, entrypoint: String) -> bool {
        Self::is_paused(env) && ALL_ENTRYPOINTS.contains(&entrypoint.as_str())
    }
}
