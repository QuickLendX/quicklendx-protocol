//! Contract initialization module for the QuickLendX protocol.
//!
//! This module provides a secure, one-time initialization flow for the protocol,
//! setting up all critical configuration parameters including admin, fees, treasury,
//! currency whitelist, and protocol constants.
//!
//! # Security Model
//!
//! - **One-time initialization**: The contract can only be initialized once
//! - **Admin authorization**: Initialization requires authorization from the admin address
//! - **Re-initialization protection**: Subsequent calls to initialize will fail
//! - **Phased initialization**: Supports both single-shot and phased initialization patterns
//!
//! # Initialization Flow
//!
//! 1. Call `initialize()` with all required parameters
//! 2. The function validates inputs and checks initialization state
//! 3. On success, all configuration is stored atomically
//! 4. Events are emitted for audit trail
//!
//! # Post-Initialization
//!
//! After initialization, the admin can update configuration via:
//! - `set_protocol_config()` - Update protocol parameters
//! - `set_fee_config()` - Update fee configuration
//! - `add_currency()` - Add whitelisted currencies

use crate::admin::{ADMIN_INITIALIZED_KEY, ADMIN_KEY};

use crate::errors::QuickLendXError;
use soroban_sdk::{contracttype, symbol_short, Address, Env, Symbol, Vec};

/// Storage key for protocol initialization flag
const PROTOCOL_INITIALIZED_KEY: Symbol = symbol_short!("proto_in");

/// Storage key for protocol configuration
const PROTOCOL_CONFIG_KEY: Symbol = symbol_short!("proto_cf");

/// Storage key for treasury address
const TREASURY_KEY: Symbol = symbol_short!("treasury");

/// Storage key for fee basis points
const FEE_BPS_KEY: Symbol = symbol_short!("fee_bps");

/// Storage key for currency whitelist (re-exported from currency module)
const WHITELIST_KEY: Symbol = symbol_short!("curr_wl");

#[cfg(not(test))]
const DEFAULT_MIN_INVOICE_AMOUNT: i128 = 1_000_000; // 1 token (6 decimals)
#[cfg(test)]
const DEFAULT_MIN_INVOICE_AMOUNT: i128 = 10;
const DEFAULT_MAX_DUE_DATE_DAYS: u64 = 365;
const DEFAULT_GRACE_PERIOD_SECONDS: u64 = 7 * 24 * 60 * 60; // 7 days
const DEFAULT_FEE_BPS: u32 = 200; // 2%
const MAX_FEE_BPS: u32 = 1000; // 10%
const MIN_FEE_BPS: u32 = 0;

/// Protocol configuration structure
///
/// Contains all protocol-wide parameters that control invoice validation,
/// fee calculations, and grace periods.
#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(test, derive(Debug))]
pub struct ProtocolConfig {
    /// Minimum allowed invoice amount (in smallest currency unit)
    pub min_invoice_amount: i128,
    /// Maximum number of days until invoice due date
    pub max_due_date_days: u64,
    /// Grace period in seconds before default is triggered
    pub grace_period_seconds: u64,
    /// Timestamp when configuration was last updated
    pub updated_at: u64,
    /// Address that made the last update
    pub updated_by: Address,
}

/// Initialization parameters for the protocol
///
/// Bundles all parameters needed for initial setup in a single struct
/// to simplify the initialization API and ensure atomic configuration.
#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(test, derive(Debug))]
pub struct InitializationParams {
    /// Admin address for the protocol
    pub admin: Address,
    /// Treasury address for fee collection
    pub treasury: Address,
    /// Fee basis points (e.g., 200 = 2%)
    pub fee_bps: u32,
    /// Minimum invoice amount
    pub min_invoice_amount: i128,
    /// Maximum due date days
    pub max_due_date_days: u64,
    /// Grace period in seconds
    pub grace_period_seconds: u64,
    /// Initial whitelisted currencies
    pub initial_currencies: Vec<Address>,
}

/// Protocol initialization and configuration management
pub struct ProtocolInitializer;

impl ProtocolInitializer {
    /// Initialize the protocol with all required configuration.
    /// @notice One-time setup for admin, treasury, fee, limits, and currency whitelist.
    /// @dev Requires admin authorization and rejects any partial pre-initialized state.
    ///
    /// This function performs a one-time initialization of the protocol,
    /// setting up admin, treasury, fees, protocol limits, and currency whitelist.
    /// It can only be called once - subsequent calls will fail.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `params` - Initialization parameters containing all configuration
    ///
    /// # Returns
    /// * `Ok(())` if initialization succeeds
    /// * `Err(QuickLendXError::OperationNotAllowed)` if already initialized
    /// * `Err(QuickLendXError::InvalidFeeBasisPoints)` if fee_bps is out of range
    /// * `Err(QuickLendXError::InvalidAmount)` if min_invoice_amount is invalid
    /// * `Err(QuickLendXError::InvoiceDueDateInvalid)` if max_due_date_days is invalid
    ///
    /// # Security
    /// - Requires authorization from the admin address
    /// - Can only be called once (atomic check-and-set)
    /// - Validates all parameters before any state changes
    /// - Emits initialization event for audit trail
    pub fn initialize(env: &Env, params: &InitializationParams) -> Result<(), QuickLendXError> {
        // Check if already initialized (re-initialization protection with idempotency)
        if Self::is_initialized(env) {
            let state = Self::load_initialized_state(env)?;
            if state.matches(params) {
                return Ok(());
            }
            return Err(QuickLendXError::OperationNotAllowed);
        }

        // Require explicit admin authorization for initialization.
        params.admin.require_auth();

        // Guard against partial initialization state before any writes.
        Self::ensure_clean_initialization_state(env)?;

        // Validate all parameters before making any state changes
        Self::validate_initialization_params(env, params)?;

        // Initialize admin (this also checks admin_initialized flag)
        // We set this first as it's the foundation for all admin operations
        env.storage().instance().set(&ADMIN_INITIALIZED_KEY, &true);
        env.storage().instance().set(&ADMIN_KEY, &params.admin);

        // Store treasury address
        env.storage()
            .instance()
            .set(&TREASURY_KEY, &params.treasury);

        // Store fee configuration
        env.storage().instance().set(&FEE_BPS_KEY, &params.fee_bps);

        // Store protocol configuration
        let config = ProtocolConfig {
            min_invoice_amount: params.min_invoice_amount,
            max_due_date_days: params.max_due_date_days,
            grace_period_seconds: params.grace_period_seconds,
            updated_at: env.ledger().timestamp(),
            updated_by: params.admin.clone(),
        };
        env.storage().instance().set(&PROTOCOL_CONFIG_KEY, &config);

        // Sync protocol limits used by invoice validation.
        // This ensures `store_invoice` / `upload_invoice` enforce the configured bounds
        // immediately after initialization (Issue #541).
        crate::protocol_limits::ProtocolLimitsContract::set_protocol_limits_authed(
            env,
            &params.admin,
            params.min_invoice_amount,
            crate::protocol_limits::DEFAULT_MIN_BID_AMOUNT,
            crate::protocol_limits::DEFAULT_MIN_BID_BPS,
            params.max_due_date_days,
            params.grace_period_seconds,
            crate::protocol_limits::DEFAULT_MAX_INVOICES_PER_BUSINESS,
        )?;

        // Initialize currency whitelist with provided currencies (even if empty).
        env.storage()
            .instance()
            .set(&WHITELIST_KEY, &params.initial_currencies);

        // Mark protocol as initialized (this is the atomic commit point)
        env.storage()
            .instance()
            .set(&PROTOCOL_INITIALIZED_KEY, &true);

        // Emit initialization event
        emit_protocol_initialized(
            env,
            &params.admin,
            &params.treasury,
            params.fee_bps,
            params.min_invoice_amount,
            params.max_due_date_days,
            params.grace_period_seconds,
        );

        Ok(())
    }

    /// Check if the protocol has been initialized.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    ///
    /// # Returns
    /// * `true` if the protocol has been initialized
    /// * `false` otherwise
    pub fn is_initialized(env: &Env) -> bool {
        env.storage()
            .instance()
            .get(&PROTOCOL_INITIALIZED_KEY)
            .unwrap_or(false)
    }

    /// Validate initialization parameters.
    ///
    /// Performs comprehensive validation of all parameters before
    /// any state changes are made.
    fn validate_initialization_params(
        env: &Env,
        params: &InitializationParams,
    ) -> Result<(), QuickLendXError> {
        // Validate admin/treasury addresses (must not be contract itself or identical).
        if params.admin == env.current_contract_address()
            || params.treasury == env.current_contract_address()
            || params.admin == params.treasury
        {
            return Err(QuickLendXError::InvalidAddress);
        }

        // Validate fee basis points (0% to 10%)
        if params.fee_bps < MIN_FEE_BPS || params.fee_bps > MAX_FEE_BPS {
            return Err(QuickLendXError::InvalidFeeBasisPoints);
        }

        // Validate minimum invoice amount (must be positive)
        if params.min_invoice_amount <= 0 {
            return Err(QuickLendXError::InvalidAmount);
        }

        // Validate max due date days (must be reasonable, 1-730 days)
        if params.max_due_date_days == 0 || params.max_due_date_days > 730 {
            return Err(QuickLendXError::InvoiceDueDateInvalid);
        }

        // Validate grace period (max 30 days = 2,592,000 seconds)
        if params.grace_period_seconds > 2_592_000 {
            return Err(QuickLendXError::InvalidTimestamp);
        }

        // Validate initial currency list (no duplicates, no self-references).
        if params
            .initial_currencies
            .iter()
            .any(|currency| currency == env.current_contract_address())
        {
            return Err(QuickLendXError::InvalidCurrency);
        }

        let list_len = params.initial_currencies.len();
        if list_len > 1 {
            let mut i = 0;
            while i < list_len {
                let current = params
                    .initial_currencies
                    .get(i)
                    .ok_or(QuickLendXError::StorageError)?;
                let mut j = i + 1;
                while j < list_len {
                    let other = params
                        .initial_currencies
                        .get(j)
                        .ok_or(QuickLendXError::StorageError)?;
                    if current == other {
                        return Err(QuickLendXError::InvalidCurrency);
                    }
                    j += 1;
                }
                i += 1;
            }
        }

        Ok(())
    }

    /// Get the current protocol configuration.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    ///
    /// # Returns
    /// * `Some(ProtocolConfig)` if configuration exists
    /// * `None` if protocol has not been initialized
    pub fn get_protocol_config(env: &Env) -> Option<ProtocolConfig> {
        env.storage().instance().get(&PROTOCOL_CONFIG_KEY)
    }

    /// @notice Ensure initialization has not partially populated storage.
    /// @dev Rejects if any core initialization key is present without a full init flag.
    fn ensure_clean_initialization_state(env: &Env) -> Result<(), QuickLendXError> {
        let has_admin_flag = env.storage().instance().has(&ADMIN_INITIALIZED_KEY);
        let has_admin = env.storage().instance().has(&ADMIN_KEY);
        let has_treasury = env.storage().instance().has(&TREASURY_KEY);
        let has_fee = env.storage().instance().has(&FEE_BPS_KEY);
        let has_config = env.storage().instance().has(&PROTOCOL_CONFIG_KEY);
        let has_whitelist = env.storage().instance().has(&WHITELIST_KEY);
        let has_limits = crate::protocol_limits::has_protocol_limits(env);

        let any = has_admin_flag
            || has_admin
            || has_treasury
            || has_fee
            || has_config
            || has_whitelist
            || has_limits;
        let all = has_admin_flag
            && has_admin
            && has_treasury
            && has_fee
            && has_config
            && has_whitelist
            && has_limits;

        if any && !all {
            return Err(QuickLendXError::OperationNotAllowed);
        }

        if any && all {
            return Err(QuickLendXError::OperationNotAllowed);
        }

        Ok(())
    }

    /// @notice Load existing initialized state for idempotency comparison.
    /// @dev Returns StorageKeyNotFound if any core key is missing.
    fn load_initialized_state(env: &Env) -> Result<InitializedState, QuickLendXError> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&ADMIN_KEY)
            .ok_or(QuickLendXError::StorageKeyNotFound)?;
        let treasury: Address = env
            .storage()
            .instance()
            .get(&TREASURY_KEY)
            .ok_or(QuickLendXError::StorageKeyNotFound)?;
        let fee_bps: u32 = env
            .storage()
            .instance()
            .get(&FEE_BPS_KEY)
            .ok_or(QuickLendXError::StorageKeyNotFound)?;
        let config: ProtocolConfig = env
            .storage()
            .instance()
            .get(&PROTOCOL_CONFIG_KEY)
            .ok_or(QuickLendXError::StorageKeyNotFound)?;
        let whitelist: Vec<Address> = env
            .storage()
            .instance()
            .get(&WHITELIST_KEY)
            .unwrap_or_else(|| Vec::new(env));

        Ok(InitializedState {
            admin,
            treasury,
            fee_bps,
            config,
            whitelist,
        })
    }
}

/// @notice Snapshot of stored initialization state for idempotency comparison.
struct InitializedState {
    admin: Address,
    treasury: Address,
    fee_bps: u32,
    config: ProtocolConfig,
    whitelist: Vec<Address>,
}

impl InitializedState {
    fn matches(&self, params: &InitializationParams) -> bool {
        self.admin == params.admin
            && self.treasury == params.treasury
            && self.fee_bps == params.fee_bps
            && self.config.min_invoice_amount == params.min_invoice_amount
            && self.config.max_due_date_days == params.max_due_date_days
            && self.config.grace_period_seconds == params.grace_period_seconds
            && self.whitelist == params.initial_currencies
    }
}

// ============================================================================
// Events
// ============================================================================

/// Emit protocol initialization event
fn emit_protocol_initialized(
    env: &Env,
    admin: &Address,
    treasury: &Address,
    fee_bps: u32,
    min_invoice_amount: i128,
    max_due_date_days: u64,
    grace_period_seconds: u64,
) {
    env.events().publish(
        (symbol_short!("proto_in"),),
        (
            admin.clone(),
            treasury.clone(),
            fee_bps,
            min_invoice_amount,
            max_due_date_days,
            grace_period_seconds,
            env.ledger().timestamp(),
        ),
    );
}
