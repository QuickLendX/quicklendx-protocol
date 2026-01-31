//! Multi-currency whitelist: admin-managed list of token addresses allowed for invoice currency.
//! Rejects invoice creation and bids for non-whitelisted tokens (e.g. USDC, EURC, stablecoins).

use crate::admin::AdminStorage;
use crate::errors::QuickLendXError;
use soroban_sdk::{symbol_short, Address, Env, Vec};

const WHITELIST_KEY: soroban_sdk::Symbol = symbol_short!("curr_wl");

/// Currency whitelist storage and operations.
pub struct CurrencyWhitelist;

impl CurrencyWhitelist {
    /// Add a token address to the whitelist (admin only).
    pub fn add_currency(env: &Env, admin: &Address, currency: &Address) -> Result<(), QuickLendXError> {
        let current_admin = AdminStorage::get_admin(env).ok_or(QuickLendXError::NotAdmin)?;
        if *admin != current_admin {
            return Err(QuickLendXError::NotAdmin);
        }
        admin.require_auth();

        let mut list = Self::get_whitelisted_currencies(env);
        if list.iter().any(|a| a == *currency) {
            return Ok(()); // idempotent: already present
        }
        list.push_back(currency.clone());
        env.storage().instance().set(&WHITELIST_KEY, &list);
        Ok(())
    }

    /// Remove a token address from the whitelist (admin only).
    pub fn remove_currency(env: &Env, admin: &Address, currency: &Address) -> Result<(), QuickLendXError> {
        let current_admin = AdminStorage::get_admin(env).ok_or(QuickLendXError::NotAdmin)?;
        if *admin != current_admin {
            return Err(QuickLendXError::NotAdmin);
        }
        admin.require_auth();

        let list = Self::get_whitelisted_currencies(env);
        let mut new_list = Vec::new(env);
        for a in list.iter() {
            if a != *currency {
                new_list.push_back(a);
            }
        }
        env.storage().instance().set(&WHITELIST_KEY, &new_list);
        Ok(())
    }

    /// Check if a token is allowed for invoice currency.
    pub fn is_allowed_currency(env: &Env, currency: &Address) -> bool {
        let list = Self::get_whitelisted_currencies(env);
        list.iter().any(|a| a == *currency)
    }

    /// Return all whitelisted token addresses.
    pub fn get_whitelisted_currencies(env: &Env) -> Vec<Address> {
        env.storage()
            .instance()
            .get(&WHITELIST_KEY)
            .unwrap_or_else(|| Vec::new(env))
    }

    /// Require that the currency is whitelisted; otherwise return InvalidCurrency.
    /// When the whitelist is empty, all currencies are allowed (backward compatibility).
    pub fn require_allowed_currency(env: &Env, currency: &Address) -> Result<(), QuickLendXError> {
        let list = Self::get_whitelisted_currencies(env);
        if list.len() == 0 {
            return Ok(());
        }
        if Self::is_allowed_currency(env, currency) {
            Ok(())
        } else {
            Err(QuickLendXError::InvalidCurrency)
        }
    }
}
