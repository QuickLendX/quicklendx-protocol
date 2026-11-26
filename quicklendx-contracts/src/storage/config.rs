//! Configuration, TTL, and operational storage utilities.

use soroban_sdk::{symbol_short, Address, Env, IntoVal, Symbol, TryFromVal, Val};

use crate::errors::QuickLendXError;
use crate::types::PlatformFeeConfig;
use super::{DataKey, StorageKeys};

/// Soroban TTL policy based on 5-second ledgers.
///
/// 1 day  = 17,280 ledgers
/// 7 days = 120,960 ledgers
/// 30 days = 518,400 ledgers
///
/// Persistent entries are extended only when their remaining TTL falls
/// below the 7-day threshold, and are then extended to 30 days.
pub const PERSISTENT_BUMP_THRESHOLD: u32 = 120_960;
pub const PERSISTENT_EXTEND_TO: u32 = 518_400;

/// Instance storage uses the same 7-day/30-day TTL policy.
pub const INSTANCE_BUMP_THRESHOLD: u32 = 120_960;
pub const INSTANCE_EXTEND_TO: u32 = 518_400;

/// Maximum time limit for invoice locks in seconds (30 days)
pub const LOCK_TIME_LIMIT_SECONDS: u64 = 2_592_000;


pub fn extend_persistent_ttl<T>(env: &Env, key: &T)
where
    T: soroban_sdk::IntoVal<soroban_sdk::Env, soroban_sdk::Val>,
{
    env.storage()
        .persistent()
        .extend_ttl(key, PERSISTENT_BUMP_THRESHOLD, PERSISTENT_EXTEND_TO);
}

pub fn bump_persistent<T>(env: &Env, key: &T)
where
    T: soroban_sdk::IntoVal<soroban_sdk::Env, soroban_sdk::Val>,
{
    extend_persistent_ttl(env, key);
}

pub fn persistent_set<K, V>(env: &Env, key: &K, val: &V)
where
    K: soroban_sdk::IntoVal<soroban_sdk::Env, soroban_sdk::Val>,
    V: soroban_sdk::IntoVal<soroban_sdk::Env, soroban_sdk::Val>,
{
    env.storage().persistent().set(key, val);
    extend_persistent_ttl(env, key);
}

pub fn persistent_get<K, V>(env: &Env, key: &K) -> Option<V>
where
    K: soroban_sdk::IntoVal<soroban_sdk::Env, soroban_sdk::Val>,
    V: soroban_sdk::TryFromVal<soroban_sdk::Env, soroban_sdk::Val>,
{
    let val = env.storage().persistent().get(key);
    if val.is_some() {
        extend_persistent_ttl(env, key);
    }
    val
}

pub fn extend_instance_ttl(env: &Env) {
    env.storage()
        .instance()
        .extend_ttl(INSTANCE_BUMP_THRESHOLD, INSTANCE_EXTEND_TO);
}

/// Storage key for the pending treasury address during a rotation.
pub const PENDING_TREASURY_KEY: Symbol = symbol_short!("pnd_trs");
/// Storage key for the pending treasury execution timestamp.
pub const PENDING_TREASURY_TS_KEY: Symbol = symbol_short!("trs_ts");


impl StorageKeys {
    /// **Storage class**: Instance  
    /// **BREAKING**: Renaming `"fees"` loses the persisted platform-fee configuration.
    pub fn platform_fees() -> Symbol {
        symbol_short!("fees")
    }
}

pub struct ConfigStorage;
impl ConfigStorage {
    pub fn set_platform_fees(env: &Env, config: &PlatformFeeConfig) {
        env.storage()
            .instance()
            .set(&StorageKeys::platform_fees(), config);
    }
    pub fn get_platform_fees(env: &Env) -> Option<PlatformFeeConfig> {
        env.storage().instance().get(&StorageKeys::platform_fees())
    }
}

pub struct StorageManager;

/// Storage key for the view-only context flag.
const VIEW_ONLY_KEY: Symbol = symbol_short!("view_onl");

impl StorageManager {
    pub fn clear_all_mappings(env: &Env) -> Result<(), QuickLendXError> {
        crate::governance::require_no_open_governance_proposal(env)?;
        env.storage()
            .persistent()
            .remove(&StorageKeys::invoice_count());
        env.storage().persistent().remove(&StorageKeys::bid_count());
        env.storage()
            .persistent()
            .remove(&StorageKeys::investment_count());
        Ok(())
    }

    /// Return `true` if the current context is marked as view-only.
    pub fn is_view_only(env: &Env) -> bool {
        env.storage()
            .instance()
            .get(&VIEW_ONLY_KEY)
            .unwrap_or(false)
    }

    /// Mark the current context as view-only or normal.
    pub fn set_view_only(env: &Env, enabled: bool) {
        env.storage().instance().set(&VIEW_ONLY_KEY, &enabled);
    }

    /// Execute a closure within a view-only context.
    ///
    /// Sets the view-only flag, runs `f`, then restores the previous flag state.
    pub fn with_view_only<F, R>(env: &Env, f: F) -> R
    where
        F: FnOnce() -> R,
    {
        let previous = Self::is_view_only(env);
        Self::set_view_only(env, true);
        let result = f();
        Self::set_view_only(env, previous);
        result
    }
}

/// Check if a treasury rotation is currently pending.
pub fn has_pending_treasury(env: &Env) -> bool {
    env.storage().instance().has(&PENDING_TREASURY_KEY)
}

/// Remove the pending treasury address and timestamp from storage.
pub fn remove_pending_treasury(env: &Env) {
    env.storage().instance().remove(&PENDING_TREASURY_KEY);
    env.storage().instance().remove(&PENDING_TREASURY_TS_KEY);
}

/// Store a pending treasury rotation request (address + timestamp).
///
/// This is a low-level setter used by treasury rotation flows and by tests
/// that need to prime the pending state before exercising `cancel_treasury_rotation`.
pub fn set_pending_treasury(env: &Env, address: &Address, timestamp: u64) {
    env.storage().instance().set(&PENDING_TREASURY_KEY, address);
    env.storage().instance().set(&PENDING_TREASURY_TS_KEY, &timestamp);
}

/// Get the pending treasury address and its execution timestamp.
/// This is used by tests and potentially by UI components to show pending changes.
pub fn get_pending_treasury(env: &Env) -> Option<(Address, u64)> {
    if !has_pending_treasury(env) {
        return None;
    }
    let address = env.storage().instance().get(&PENDING_TREASURY_KEY)?;
    let timestamp = env.storage().instance().get(&PENDING_TREASURY_TS_KEY)?;
    Some((address, timestamp))
}

