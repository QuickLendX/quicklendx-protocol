use crate::admin::AdminStorage;
use crate::errors::QuickLendXError;
use soroban_sdk::{symbol_short, Address, Env, String, Symbol};

const PAUSED_KEY: Symbol = symbol_short!("paused");
const PAUSED_AT_KEY: Symbol = symbol_short!("paused_at");
const MAX_PAUSE_DURATION: u64 = 7 * 24 * 3600;

/// Set of contract entrypoint names that are guarded by the protocol pause.
///
/// Compared at runtime via [`soroban_sdk::String`] equality because Soroban
/// `String` is a host type without a direct `as_str()` accessor.
const ALL_ENTRYPOINTS: &[&str] = &[
    "store_invoice",
    "verify_invoice",
    "place_bid",
    "accept_bid",
    "verify_business",
    "verify_investor",
    "create_dispute",
    "resolve_dispute",
];

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
        // Check current paused state to ensure idempotency
        let current: bool = Self::is_paused(env);
        if current == paused {
            return Ok(());
        }
        Self::apply_paused(env, paused);
        // Emit appropriate event based on state transition
        if paused {
            crate::events::emit_paused(env, admin);
        } else {
            crate::events::emit_unpaused(env, admin);
        }
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
        if !Self::is_paused(env) {
            return false;
        }
        for ep in ALL_ENTRYPOINTS {
            if entrypoint == String::from_str(env, ep) {
                return true;
            }
        }
        false
    }
}
