//! Multi-currency whitelist: admin-managed list of token addresses allowed for invoice currency.
//! Rejects invoice creation and bids for non-whitelisted tokens (e.g. USDC, EURC, stablecoins).
//!
use crate::admin::AdminStorage;
use crate::errors::QuickLendXError;
use soroban_sdk::{symbol_short, Address, Env, Vec};

const WHITELIST_KEY: soroban_sdk::Symbol = symbol_short!("curr_wl");

/// Currency whitelist storage and operations.
pub struct CurrencyWhitelist;

impl CurrencyWhitelist {
    /// Add a token address to the whitelist (admin only).
    pub fn add_currency(
        env: &Env,
        admin: &Address,
        currency: &Address,
    ) -> Result<(), QuickLendXError> {
        AdminStorage::require_admin_auth(env, admin)?;

        let mut list = Self::get_whitelisted_currencies(env);
        if list.iter().any(|a| a == *currency) {
            return Ok(()); // idempotent: already present
        }
        list.push_back(currency.clone());
        env.storage().instance().set(&WHITELIST_KEY, &list);
        Ok(())
    }

    /// Remove a token address from the whitelist (admin only).
    pub fn remove_currency(
        env: &Env,
        admin: &Address,
        currency: &Address,
    ) -> Result<(), QuickLendXError> {
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

    /// Replace the entire whitelist atomically (admin only).
    /// Useful for bulk updates without multiple round-trips.
    pub fn set_currencies(
        env: &Env,
        admin: &Address,
        currencies: &Vec<Address>,
    ) -> Result<(), QuickLendXError> {
        AdminStorage::require_admin_auth(env, admin)?;

        let mut deduped: Vec<Address> = Vec::new(env);
        for currency in currencies.iter() {
            if !deduped.iter().any(|a| a == currency) {
                deduped.push_back(currency);
            }
        }
        env.storage().instance().set(&WHITELIST_KEY, &deduped);
        Ok(())
    }

    /// Clear the entire whitelist (admin only).
    /// After this call all currencies are allowed again (empty-list backward-compat rule).
    pub fn clear_currencies(env: &Env, admin: &Address) -> Result<(), QuickLendXError> {
        let current_admin = AdminStorage::get_admin(env).ok_or(QuickLendXError::NotAdmin)?;
        if *admin != current_admin {
            return Err(QuickLendXError::NotAdmin);
        }
        admin.require_auth();

        env.storage()
            .instance()
            .set(&WHITELIST_KEY, &Vec::<Address>::new(env));
        Ok(())
    }

    /// Return the number of whitelisted currencies.
    pub fn currency_count(env: &Env) -> u32 {
        Self::get_whitelisted_currencies(env).len()
    }

    /// @notice Return a paginated slice of the whitelist with hard cap enforcement
    /// @param env The contract environment
    /// @param offset Starting index for pagination (0-based)
    /// @param limit Maximum number of results to return (capped at MAX_QUERY_LIMIT)
    /// @return Vector of whitelisted currency addresses
    /// @dev Enforces MAX_QUERY_LIMIT hard cap for security and performance
    pub fn get_whitelisted_currencies_paged(env: &Env, offset: u32, limit: u32) -> Vec<Address> {
        // Import MAX_QUERY_LIMIT from parent module
        const MAX_QUERY_LIMIT: u32 = 100;

        // Validate query parameters for security
        if offset > u32::MAX - MAX_QUERY_LIMIT {
            return Vec::new(env);
        }

        let capped_limit = limit.min(MAX_QUERY_LIMIT);
        let list = Self::get_whitelisted_currencies(env);
        let mut page: Vec<Address> = Vec::new(env);
        let len = list.len();
        let end = (offset + capped_limit).min(len);
        if offset >= len {
            return page;
        }
        for i in offset..end {
            page.push_back(list.get(i).unwrap());
        }
        page
    }
}
