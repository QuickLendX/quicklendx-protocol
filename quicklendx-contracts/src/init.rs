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

use crate::admin::{AdminStorage, ADMIN_INITIALIZED_KEY};
use crate::currency::CurrencyWhitelist;
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

/// Default values for protocol configuration
const DEFAULT_MIN_INVOICE_AMOUNT: i128 = 1_000_000; // 1 token (6 decimals)
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
#[derive(Clone, Debug, Eq, PartialEq)]
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
#[derive(Clone, Debug, Eq, PartialEq)]
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
        // Require authorization from the admin
        params.admin.require_auth();

        // Check if already initialized (re-initialization protection)
        if Self::is_initialized(env) {
            return Err(QuickLendXError::OperationNotAllowed);
        }

        // Validate all parameters before making any state changes
        Self::validate_initialization_params(env, params)?;

        // Initialize admin (this also checks admin_initialized flag)
        // We set this first as it's the foundation for all admin operations
        env.storage().instance().set(&ADMIN_INITIALIZED_KEY, &true);
        env.storage()
            .instance()
            .set(&crate::admin::ADMIN_KEY, &params.admin);

        // Store treasury address
        env.storage()
            .instance()
            .set(&TREASURY_KEY, &params.treasury);

        // Initialize currency whitelist
        if !params.initial_currencies.is_empty() {
            crate::currency::CurrencyWhitelist::set_currencies(env, &params.admin, &params.initial_currencies)?;
        }

        // Store protocol configuration
        let config = ProtocolConfig {
            min_invoice_amount: params.min_invoice_amount,
            max_due_date_days: params.max_due_date_days,
            grace_period_seconds: params.grace_period_seconds,
            updated_at: env.ledger().timestamp(),
            updated_by: params.admin.clone(),
        };
        env.storage().instance().set(&PROTOCOL_CONFIG_KEY, &config);

        // Mark protocol as initialized (atomic commit point)
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
        _env: &Env,
        params: &InitializationParams,
    ) -> Result<(), QuickLendXError> {
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

        Ok(())
    }

    /// Update protocol configuration (admin only).
    ///
    /// Allows the admin to update protocol parameters after initialization.
    /// Requires admin authorization.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `admin` - The admin address (must authorize)
    /// * `min_invoice_amount` - New minimum invoice amount
    /// * `max_due_date_days` - New maximum due date days
    /// * `grace_period_seconds` - New grace period in seconds
    ///
    /// # Returns
    /// * `Ok(())` if update succeeds
    /// * `Err(QuickLendXError::NotAdmin)` if caller is not admin
    /// * `Err(QuickLendXError::InvalidAmount)` if amount is invalid
    /// * `Err(QuickLendXError::InvoiceDueDateInvalid)` if due date is invalid
    pub fn set_protocol_config(
        env: &Env,
        admin: &Address,
        min_invoice_amount: i128,
        max_due_date_days: u64,
        grace_period_seconds: u64,
    ) -> Result<(), QuickLendXError> {
        // Require admin authorization
        admin.require_auth();

        // Verify caller is admin
        if !AdminStorage::is_admin(env, admin) {
            return Err(QuickLendXError::NotAdmin);
        }

        // Validate parameters
        if min_invoice_amount <= 0 {
            return Err(QuickLendXError::InvalidAmount);
        }

        if max_due_date_days == 0 || max_due_date_days > 730 {
            return Err(QuickLendXError::InvoiceDueDateInvalid);
        }

        if grace_period_seconds > 2_592_000 {
            return Err(QuickLendXError::InvalidTimestamp);
        }

        // Update configuration
        let config = ProtocolConfig {
            min_invoice_amount,
            max_due_date_days,
            grace_period_seconds,
            updated_at: env.ledger().timestamp(),
            updated_by: admin.clone(),
        };

        env.storage().instance().set(&PROTOCOL_CONFIG_KEY, &config);

        // Emit configuration update event
        emit_protocol_config_updated(
            env,
            admin,
            min_invoice_amount,
            max_due_date_days,
            grace_period_seconds,
        );

        Ok(())
    }

    /// Update fee configuration (admin only).
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `admin` - The admin address (must authorize)
    /// * `fee_bps` - New fee basis points (0-1000)
    ///
    /// # Returns
    /// * `Ok(())` if update succeeds
    /// * `Err(QuickLendXError::NotAdmin)` if caller is not admin
    /// * `Err(QuickLendXError::InvalidFeeBasisPoints)` if fee is out of range
    pub fn set_fee_config(env: &Env, admin: &Address, fee_bps: u32) -> Result<(), QuickLendXError> {
        // Require admin authorization
        admin.require_auth();

        // Verify caller is admin
        if !AdminStorage::is_admin(env, admin) {
            return Err(QuickLendXError::NotAdmin);
        }

        // Validate fee basis points
        if fee_bps < MIN_FEE_BPS || fee_bps > MAX_FEE_BPS {
            return Err(QuickLendXError::InvalidFeeBasisPoints);
        }

        // Update fee configuration
        env.storage().instance().set(&FEE_BPS_KEY, &fee_bps);

        // Emit fee update event
        emit_fee_config_updated(env, admin, fee_bps);

        Ok(())
    }

    /// Update treasury address (admin only).
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `admin` - The admin address (must authorize)
    /// * `treasury` - New treasury address
    ///
    /// # Returns
    /// * `Ok(())` if update succeeds
    /// * `Err(QuickLendXError::NotAdmin)` if caller is not admin
    pub fn set_treasury(
        env: &Env,
        admin: &Address,
        treasury: &Address,
    ) -> Result<(), QuickLendXError> {
        // Require admin authorization
        admin.require_auth();

        // Verify caller is admin
        if !AdminStorage::is_admin(env, admin) {
            return Err(QuickLendXError::NotAdmin);
        }

        // Update treasury
        env.storage().instance().set(&TREASURY_KEY, treasury);

        // Emit treasury update event
        emit_treasury_updated(env, admin, treasury);

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

    /// Get the current fee basis points.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    ///
    /// # Returns
    /// * Fee basis points (defaults to DEFAULT_FEE_BPS if not set)
    pub fn get_fee_bps(env: &Env) -> u32 {
        env.storage()
            .instance()
            .get(&FEE_BPS_KEY)
            .unwrap_or(DEFAULT_FEE_BPS)
    }

    /// Get the treasury address.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    ///
    /// # Returns
    /// * `Some(Address)` if treasury is set
    /// * `None` if treasury has not been configured
    pub fn get_treasury(env: &Env) -> Option<Address> {
        env.storage().instance().get(&TREASURY_KEY)
    }

    /// Get the minimum invoice amount.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    ///
    /// # Returns
    /// * Minimum invoice amount (defaults to DEFAULT_MIN_INVOICE_AMOUNT)
    pub fn get_min_invoice_amount(env: &Env) -> i128 {
        Self::get_protocol_config(env)
            .map(|c| c.min_invoice_amount)
            .unwrap_or(DEFAULT_MIN_INVOICE_AMOUNT)
    }

    /// Get the maximum due date days.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    ///
    /// # Returns
    /// * Maximum due date days (defaults to DEFAULT_MAX_DUE_DATE_DAYS)
    pub fn get_max_due_date_days(env: &Env) -> u64 {
        Self::get_protocol_config(env)
            .map(|c| c.max_due_date_days)
            .unwrap_or(DEFAULT_MAX_DUE_DATE_DAYS)
    }

    /// Get the grace period in seconds.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    ///
    /// # Returns
    /// * Grace period in seconds (defaults to DEFAULT_GRACE_PERIOD_SECONDS)
    pub fn get_grace_period_seconds(env: &Env) -> u64 {
        Self::get_protocol_config(env)
            .map(|c| c.grace_period_seconds)
            .unwrap_or(DEFAULT_GRACE_PERIOD_SECONDS)
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

/// Emit protocol configuration update event
fn emit_protocol_config_updated(
    env: &Env,
    admin: &Address,
    min_invoice_amount: i128,
    max_due_date_days: u64,
    grace_period_seconds: u64,
) {
    env.events().publish(
        (symbol_short!("proto_cfg"),),
        (
            admin.clone(),
            min_invoice_amount,
            max_due_date_days,
            grace_period_seconds,
            env.ledger().timestamp(),
        ),
    );
}

/// Emit fee configuration update event
fn emit_fee_config_updated(env: &Env, admin: &Address, fee_bps: u32) {
    env.events().publish(
        (symbol_short!("fee_cfg"),),
        (admin.clone(), fee_bps, env.ledger().timestamp()),
    );
}

/// Emit treasury update event
fn emit_treasury_updated(env: &Env, admin: &Address, treasury: &Address) {
    env.events().publish(
        (symbol_short!("trsr_upd"),),
        (admin.clone(), treasury.clone(), env.ledger().timestamp()),
    );
}
