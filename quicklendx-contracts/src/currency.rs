//! Multi-currency whitelist: admin-managed list of token addresses allowed for invoice currency.
//! Rejects invoice creation and bids for non-whitelisted tokens (e.g. USDC, EURC, stablecoins).
//!
//! ## Empty-list semantics
//! When the whitelist contains **zero** entries every currency is accepted.  This preserves
//! backward compatibility for deployments that have not yet configured a whitelist.  The moment
//! at least one currency is added, the list becomes restrictive.
//!
//! ## Authorization model
//! All write operations require **two** independent checks:
//! 1. `AdminStorage::get_admin` — verifies an admin has been initialised and retrieves it.
//! 2. `admin.require_auth()` — the Soroban host enforces that the transaction is signed by
//!    that address.  Neither check alone is sufficient.
//!
use crate::admin::AdminStorage;
use crate::errors::QuickLendXError;
use soroban_sdk::{symbol_short, Address, Env, Vec};

const WHITELIST_KEY: soroban_sdk::Symbol = symbol_short!("curr_wl");

/// Currency whitelist storage and operations.
pub struct CurrencyWhitelist;

impl CurrencyWhitelist {
    /// Add a token address to the whitelist (admin only).
    ///
    /// # Parameters
    /// - `env`      — Soroban execution environment.
    /// - `admin`    — Address that must match the stored contract admin.
    /// - `currency` — Token contract address to allow.
    ///
    /// # Behaviour
    /// - **Idempotent**: if `currency` is already present the call succeeds without
    ///   modifying state.
    /// - Both the storage admin check and `require_auth()` must pass.
    ///
    /// # Errors
    /// - `NotAdmin` — `admin` does not match the stored admin or no admin is set.
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
    ///
    /// # Parameters
    /// - `env`      — Soroban execution environment.
    /// - `admin`    — Address that must match the stored contract admin.
    /// - `currency` — Token contract address to remove.
    ///
    /// # Behaviour
    /// - **No-op when absent**: if `currency` is not in the list the call succeeds and
    ///   state is unchanged.
    /// - Rebuilds the list without the target address in a single pass.
    ///
    /// # Errors
    /// - `NotAdmin` — `admin` does not match the stored admin or no admin is set.
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

    /// Return `true` if `currency` is present in the whitelist.
    ///
    /// # Parameters
    /// - `env`      — Soroban execution environment.
    /// - `currency` — Token contract address to test.
    ///
    /// # Security
    /// Read-only; no authentication required.  Does **not** apply the empty-list bypass
    /// rule — use `require_allowed_currency` for enforcement.
    pub fn is_allowed_currency(env: &Env, currency: &Address) -> bool {
        let list = Self::get_whitelisted_currencies(env);
        list.iter().any(|a| a == *currency)
    }

    /// Return the full whitelist as stored.
    ///
    /// Returns an empty `Vec` when no whitelist has been persisted yet.
    pub fn get_whitelisted_currencies(env: &Env) -> Vec<Address> {
        env.storage()
            .instance()
            .get(&WHITELIST_KEY)
            .unwrap_or_else(|| Vec::new(env))
    }

    /// Assert that `currency` is permitted, respecting empty-list backward compatibility.
    ///
    /// # Parameters
    /// - `env`      — Soroban execution environment.
    /// - `currency` — Token contract address being validated.
    ///
    /// # Behaviour
    /// - When the whitelist is **empty** the call succeeds unconditionally (allow-all mode).
    /// - When the whitelist is **non-empty** the currency must appear in it.
    ///
    /// # Errors
    /// - `InvalidCurrency` — whitelist is non-empty and `currency` is not in it.
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

    /// Atomically replace the entire whitelist (admin only).
    ///
    /// # Parameters
    /// - `env`        — Soroban execution environment.
    /// - `admin`      — Address that must match the stored contract admin.
    /// - `currencies` — New list of allowed token addresses.
    ///
    /// # Behaviour
    /// - **Atomic**: the old list is fully replaced in one storage write.
    /// - **Deduplicates**: duplicate addresses in `currencies` are silently collapsed
    ///   to a single entry, preserving first-occurrence order.
    /// - Prefer this over multiple `add_currency` calls to avoid partial-state
    ///   windows between transactions.
    ///
    /// # Errors
    /// - `NotAdmin` — `admin` does not match the stored admin or no admin is set.
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
    ///
    /// # Parameters
    /// - `env`   — Soroban execution environment.
    /// - `admin` — Address that must match the stored contract admin.
    ///
    /// # Behaviour
    /// After this call `currency_count()` returns 0 and `require_allowed_currency`
    /// succeeds for every token (empty-list backward-compat rule).
    ///
    /// # Errors
    /// - `NotAdmin` — `admin` does not match the stored admin or no admin is set.
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
