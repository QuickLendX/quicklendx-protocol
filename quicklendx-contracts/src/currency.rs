//! Currency whitelist storage for the QuickLendX invoice factoring protocol.
//!
//! Only whitelisted currencies can be used for invoices and bids.
//! When the whitelist is empty, all currencies are rejected.

use soroban_sdk::{symbol_short, Address, Env, Symbol, Vec};

pub struct CurrencyWhitelistStorage;

impl CurrencyWhitelistStorage {
    const WHITELIST_KEY: Symbol = symbol_short!("curr_wl");

    /// Add a currency to the whitelist (internal helper; no auth check).
    /// Idempotent: if already present, does nothing.
    pub fn add_currency(env: &Env, currency: &Address) {
        let mut list = Self::get_whitelisted_currencies(env);
        if !list.contains(currency) {
            list.push_back(currency.clone());
            env.storage().instance().set(&Self::WHITELIST_KEY, &list);
        }
    }

    /// Remove a currency from the whitelist (internal helper; no auth check).
    /// No-op if not present.
    pub fn remove_currency(env: &Env, currency: &Address) {
        let mut list = Self::get_whitelisted_currencies(env);
        if let Some(pos) = list.iter().position(|c| c == *currency) {
            list.remove(pos as u32);
            env.storage().instance().set(&Self::WHITELIST_KEY, &list);
        }
    }

    /// Check if a currency is whitelisted.
    pub fn is_allowed_currency(env: &Env, currency: &Address) -> bool {
        let list = Self::get_whitelisted_currencies(env);
        list.contains(currency)
    }

    /// Get all whitelisted currencies.
    pub fn get_whitelisted_currencies(env: &Env) -> Vec<Address> {
        env.storage()
            .instance()
            .get(&Self::WHITELIST_KEY)
            .unwrap_or_else(|| Vec::new(env))
    }
}
