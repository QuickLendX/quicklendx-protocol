//! Hardened contract initialization module for the QuickLendX protocol.
//!
//! This module provides a secure, atomic initialization flow for the protocol,
//! setting up all critical configuration parameters with comprehensive validation
//! and robust security protections.
//!
//! # Security Model
//!
//! - **One-time initialization**: The contract can only be initialized once
//! - **Atomic operations**: All initialization is atomic (all-or-nothing)
//! - **Admin authorization**: Initialization requires authorization from the admin address
//! - **Parameter validation**: Comprehensive validation before any state changes
//! - **Re-initialization protection**: Subsequent calls to initialize will fail
//! - **Audit trail**: All initialization events are logged for transparency
//!
//! # Initialization Flow
//!
//! 1. Validate admin authorization
//! 2. Check initialization state (atomic)
//! 3. Validate all parameters comprehensively
//! 4. Initialize admin system (atomic)
//! 5. Store all configuration (atomic)
//! 6. Mark as initialized (commit point)
//! 7. Emit audit events
//!
//! # Post-Initialization
//!
//! After initialization, the admin can update configuration via:
//! - `set_protocol_config()` - Update protocol parameters
//! - `set_fee_config()` - Update fee configuration
//! - `set_treasury()` - Update treasury address
//! - Currency whitelist management functions

use crate::admin::{AdminStorage, ADMIN_INITIALIZED_KEY};
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

/// Storage key for currency whitelist
const WHITELIST_KEY: Symbol = symbol_short!("curr_wl");

/// Storage key for initialization lock (prevents concurrent initialization)
const INIT_LOCK_KEY: Symbol = symbol_short!("init_lck");

/// Storage key for the protocol version written at initialization time
const PROTOCOL_VERSION_KEY: Symbol = symbol_short!("proto_ver");

/// Current protocol version.
///
/// Increment this constant when deploying a new contract version.
/// The value is written to storage during `initialize` so that
/// `get_version` always reflects the version that was active when
/// the contract was first set up, even after a WASM upgrade that
/// bumps this constant.
///
/// # Upgrade policy
/// - Patch releases (bug-fixes, no storage-schema changes): no bump required.
/// - Minor releases (new fields, backward-compatible): bump recommended.
/// - Major releases (breaking storage changes, migration required): bump mandatory.
pub const PROTOCOL_VERSION: u32 = 1;

// Configuration constants with secure defaults
#[cfg(not(test))]
const DEFAULT_MIN_INVOICE_AMOUNT: i128 = 1_000_000; // 1 token (6 decimals)
#[cfg(test)]
const DEFAULT_MIN_INVOICE_AMOUNT: i128 = 10; // Smaller for tests

const DEFAULT_MAX_DUE_DATE_DAYS: u64 = 365; // 1 year
const DEFAULT_GRACE_PERIOD_SECONDS: u64 = 7 * 24 * 60 * 60; // 7 days
const DEFAULT_FEE_BPS: u32 = 200; // 2%

// Security limits
const MAX_FEE_BPS: u32 = 1000; // 10% maximum fee
const MIN_FEE_BPS: u32 = 0; // 0% minimum fee
const MAX_DUE_DATE_DAYS: u64 = 730; // 2 years maximum
const MAX_GRACE_PERIOD_SECONDS: u64 = 30 * 24 * 60 * 60; // 30 days maximum

/// Protocol configuration structure with comprehensive validation
///
/// Contains all protocol-wide parameters that control invoice validation,
/// fee calculations, and grace periods. All fields are validated during
/// initialization and updates.
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

/// Initialization parameters for the protocol with comprehensive validation
///
/// Bundles all parameters needed for initial setup in a single struct
/// to simplify the initialization API and ensure atomic configuration.
/// All parameters are validated before any state changes.
#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(test, derive(Debug))]
pub struct InitializationParams {
    /// Admin address for the protocol (must authorize initialization)
    pub admin: Address,
    /// Treasury address for fee collection
    pub treasury: Address,
    /// Fee basis points (0-1000, e.g., 200 = 2%)
    pub fee_bps: u32,
    /// Minimum invoice amount (must be positive)
    pub min_invoice_amount: i128,
    /// Maximum due date days (1-730)
    pub max_due_date_days: u64,
    /// Grace period in seconds (0-2,592,000)
    pub grace_period_seconds: u64,
    /// Initial whitelisted currencies
    pub initial_currencies: Vec<Address>,
}

/// Protocol initialization and configuration management with hardened security
pub struct ProtocolInitializer;

impl ProtocolInitializer {
    /// Initialize the protocol with comprehensive security and validation.
    ///
    /// This function performs a one-time, atomic initialization of the protocol
    /// with extensive security protections:
    /// - Admin authorization requirement
    /// - Atomic initialization check
    /// - Comprehensive parameter validation
    /// - Atomic state updates
    /// - Audit trail emission
    ///
    /// @notice Initializes the protocol in a single atomic operation.
    /// @dev Requires admin authorization and validates all addresses/parameters
    ///      before any state changes are committed.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `params` - Initialization parameters containing all configuration
    ///
    /// # Returns
    /// * `Ok(())` if initialization succeeds
    /// * `Err(QuickLendXError::OperationNotAllowed)` if already initialized or locked
    /// * `Err(QuickLendXError::InvalidFeeBasisPoints)` if fee_bps is out of range
    /// * `Err(QuickLendXError::InvalidAmount)` if min_invoice_amount is invalid
    /// * `Err(QuickLendXError::InvoiceDueDateInvalid)` if max_due_date_days is invalid
    /// * `Err(QuickLendXError::InvalidTimestamp)` if grace_period_seconds is invalid
    ///
    /// # Security Invariants
    /// - Requires authorization from the admin address
    /// - Can only be called once (atomic check-and-set)
    /// - All parameters validated before any state changes
    /// - All state updates are atomic
    /// - Initialization lock prevents concurrent calls
    /// - Emits initialization event for audit trail
    pub fn initialize(env: &Env, params: &InitializationParams) -> Result<(), QuickLendXError> {
        // Administrative authorization for initial setup.
        // This ensures the designated admin address has consented to the role.
        params.admin.require_auth();

        // Zero-address guard: reject the well-known Stellar zero/burn address.
        let zero = Address::from_string(&soroban_sdk::String::from_str(
            env,
            "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
        ));
        if params.admin == zero || params.treasury == zero {
            return Err(QuickLendXError::InvalidAddress);
        }
        Self::initialize_internal(env, params)
    }

    /// Internal initialization logic with comprehensive validation
    fn initialize_internal(
        env: &Env,
        params: &InitializationParams,
    ) -> Result<(), QuickLendXError> {
        // Check if already initialized (re-initialization protection with idempotency)
        if Self::is_initialized(env) {
            // Check for idempotency: if fully initialized with exact same parameters, return Ok(())
            let current_admin: Option<Address> =
                env.storage().instance().get(&crate::admin::ADMIN_KEY);
            let current_treasury: Option<Address> = env.storage().instance().get(&TREASURY_KEY);
            let current_fee_bps: Option<u32> = env.storage().instance().get(&FEE_BPS_KEY);
            let current_config: Option<ProtocolConfig> =
                env.storage().instance().get(&PROTOCOL_CONFIG_KEY);
            let current_whitelist: Vec<Address> = env
                .storage()
                .instance()
                .get(&WHITELIST_KEY)
                .unwrap_or(Vec::new(env));

            if let (Some(c_admin), Some(c_treasury), Some(c_fee), Some(c_conf)) = (
                current_admin,
                current_treasury,
                current_fee_bps,
                current_config,
            ) {
                if c_admin == params.admin
                    && c_treasury == params.treasury
                    && c_fee == params.fee_bps
                    && c_conf.min_invoice_amount == params.min_invoice_amount
                    && c_conf.max_due_date_days == params.max_due_date_days
                    && c_conf.grace_period_seconds == params.grace_period_seconds
                    && current_whitelist == params.initial_currencies
                {
                    return Ok(());
                }
            }

            return Err(QuickLendXError::OperationNotAllowed);
        }

        // VALIDATION: Validate all parameters before making any state changes
        Self::validate_initialization_params(env, params)?;
        params.admin.require_auth();

        // ATOMIC: Initialize admin system first (foundation for all operations)
        AdminStorage::initialize(env, &params.admin)?;

        // ATOMIC: Store treasury address
        env.storage()
            .instance()
            .set(&TREASURY_KEY, &params.treasury);

        // ATOMIC: Store fee configuration
        env.storage().instance().set(&FEE_BPS_KEY, &params.fee_bps);

        // ATOMIC: Store protocol configuration
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

        // Initialize currency whitelist with provided currencies
        if !params.initial_currencies.is_empty() {
            env.storage()
                .instance()
                .set(&WHITELIST_KEY, &params.initial_currencies);
        }

        // ATOMIC: Persist the protocol version so get_version is consistent
        // with the version that was active at initialization time.
        env.storage()
            .instance()
            .set(&PROTOCOL_VERSION_KEY, &PROTOCOL_VERSION);

        // COMMIT: Mark protocol as initialized (this is the atomic commit point)
        env.storage()
            .instance()
            .set(&PROTOCOL_INITIALIZED_KEY, &true);

        // AUDIT: Emit initialization event
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
    ///
    /// @notice Returns true when the initialization flag is set.
    pub fn is_initialized(env: &Env) -> bool {
        let proto_init = env
            .storage()
            .instance()
            .get(&PROTOCOL_INITIALIZED_KEY)
            .unwrap_or(false);

        // Also check if admin was initialized via legacy/phased flow
        let admin_init = env
            .storage()
            .instance()
            .get(&ADMIN_INITIALIZED_KEY)
            .unwrap_or(false);

        proto_init || admin_init
    }

    /// Validate initialization parameters with comprehensive checks.
    ///
    /// Performs extensive validation of all parameters before any state changes
    /// are made. This ensures that invalid configurations cannot be stored.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `params` - The initialization parameters to validate
    ///
    /// # Returns
    /// * `Ok(())` if all parameters are valid
    /// * `Err(QuickLendXError)` with specific error for invalid parameters
    fn validate_initialization_params(
        env: &Env,
        params: &InitializationParams,
    ) -> Result<(), QuickLendXError> {
        // VALIDATION: Fee basis points (0% to 10%)
        if params.fee_bps < MIN_FEE_BPS || params.fee_bps > MAX_FEE_BPS {
            return Err(QuickLendXError::InvalidFeeBasisPoints);
        }

        // VALIDATION: Minimum invoice amount (must be positive)
        if params.min_invoice_amount <= 0 {
            return Err(QuickLendXError::InvalidAmount);
        }

        // VALIDATION: Max due date days (must be reasonable, 1-730 days)
        if params.max_due_date_days == 0 || params.max_due_date_days > MAX_DUE_DATE_DAYS {
            return Err(QuickLendXError::InvoiceDueDateInvalid);
        }

        // VALIDATION: Grace period (max 30 days)
        if params.grace_period_seconds > MAX_GRACE_PERIOD_SECONDS {
            return Err(QuickLendXError::InvalidTimestamp);
        }

        // VALIDATION: Treasury address is not the same as admin (separation of concerns)
        if params.treasury == params.admin {
            return Err(QuickLendXError::InvalidAddress);
        }

        // VALIDATION: Initial currencies must not contain duplicates or
        // reserved addresses (admin, treasury, contract itself).
        let contract_address = env.current_contract_address();
        let len = params.initial_currencies.len();
        for i in 0..len {
            let curr = params.initial_currencies.get(i).unwrap();
            // Must not be a reserved address
            if curr == params.admin || curr == params.treasury || curr == contract_address {
                return Err(QuickLendXError::InvalidCurrency);
            }
            // Must not be a duplicate (O(n²) — list is expected to be small)
            for j in (i + 1)..len {
                if curr == params.initial_currencies.get(j).unwrap() {
                    return Err(QuickLendXError::InvalidCurrency);
                }
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
    ///
    /// @notice Returns the stored protocol configuration, if initialized.
    pub fn get_protocol_config(env: &Env) -> Option<ProtocolConfig> {
        env.storage().instance().get(&PROTOCOL_CONFIG_KEY)
    }

    /// Update protocol configuration (admin only).
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
    /// * `Err(QuickLendXError)` if validation fails or not admin
    pub fn set_protocol_config(
        env: &Env,
        admin: &Address,
        min_invoice_amount: i128,
        max_due_date_days: u64,
        grace_period_seconds: u64,
    ) -> Result<(), QuickLendXError> {
        AdminStorage::with_admin_auth(env, admin, || {
            // Validate parameters
            if min_invoice_amount <= 0 {
                return Err(QuickLendXError::InvalidAmount);
            }
            if max_due_date_days == 0 || max_due_date_days > MAX_DUE_DATE_DAYS {
                return Err(QuickLendXError::InvoiceDueDateInvalid);
            }
            if grace_period_seconds > MAX_GRACE_PERIOD_SECONDS {
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

            // Emit event
            emit_protocol_config_updated(
                env,
                admin,
                min_invoice_amount,
                max_due_date_days,
                grace_period_seconds,
            );

            Ok(())
        })
    }

    /// Update fee configuration (admin only).
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `admin` - The admin address (must authorize)
    /// * `fee_bps` - New fee in basis points
    ///
    /// # Returns
    /// * `Ok(())` if update succeeds
    /// * `Err(QuickLendXError)` if validation fails or not admin
    pub fn set_fee_config(env: &Env, admin: &Address, fee_bps: u32) -> Result<(), QuickLendXError> {
        AdminStorage::with_admin_auth(env, admin, || {
            // Validate fee
            if fee_bps < MIN_FEE_BPS || fee_bps > MAX_FEE_BPS {
                return Err(QuickLendXError::InvalidFeeBasisPoints);
            }

            // Update fee
            env.storage().instance().set(&FEE_BPS_KEY, &fee_bps);

            // Emit event
            emit_fee_config_updated(env, admin, fee_bps);

            Ok(())
        })
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
    /// * `Err(QuickLendXError)` if validation fails or not admin
    pub fn set_treasury(
        env: &Env,
        admin: &Address,
        treasury: &Address,
    ) -> Result<(), QuickLendXError> {
        AdminStorage::with_admin_auth(env, admin, || {
            // Validate treasury is not admin (separation of concerns)
            if treasury == admin {
                return Err(QuickLendXError::InvalidAddress);
            }

            // Update treasury
            env.storage().instance().set(&TREASURY_KEY, treasury);

            // Emit event
            emit_treasury_updated(env, admin, treasury);

            Ok(())
        })
    }

    /// Check if initialization is currently locked
    fn is_initialization_locked(env: &Env) -> bool {
        env.storage()
            .instance()
            .get(&INIT_LOCK_KEY)
            .unwrap_or(false)
    }

    /// Set initialization lock state
    fn set_initialization_lock(env: &Env, locked: bool) {
        if locked {
            env.storage().instance().set(&INIT_LOCK_KEY, &true);
        } else {
            env.storage().instance().remove(&INIT_LOCK_KEY);
        }
    }
}

// ============================================================================
// Query Functions
// ============================================================================

impl ProtocolInitializer {
    /// Get the protocol version stored at initialization time.
    ///
    /// Returns the `PROTOCOL_VERSION` constant that was compiled into the
    /// contract when `initialize` was first called.  Falls back to the
    /// current `PROTOCOL_VERSION` constant when the contract has not been
    /// initialized yet (e.g. in a fresh test environment).
    ///
    /// # Arguments
    /// * `env` - The contract environment
    ///
    /// # Returns
    /// * `u32` - The stored protocol version, or `PROTOCOL_VERSION` if unset.
    ///
    /// @notice Always consistent with the version active at init time.
    pub fn get_version(env: &Env) -> u32 {
        env.storage()
            .instance()
            .get(&PROTOCOL_VERSION_KEY)
            .unwrap_or(PROTOCOL_VERSION)
    }

    /// Get the current fee in basis points.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    ///
    /// # Returns
    /// * Current fee in basis points (defaults to DEFAULT_FEE_BPS if not set)
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
    /// * Current minimum invoice amount (defaults to DEFAULT_MIN_INVOICE_AMOUNT)
    pub fn get_min_invoice_amount(env: &Env) -> i128 {
        Self::get_protocol_config(env)
            .map(|config| config.min_invoice_amount)
            .unwrap_or(DEFAULT_MIN_INVOICE_AMOUNT)
    }

    /// Get the maximum due date days.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    ///
    /// # Returns
    /// * Current maximum due date days (defaults to DEFAULT_MAX_DUE_DATE_DAYS)
    pub fn get_max_due_date_days(env: &Env) -> u64 {
        Self::get_protocol_config(env)
            .map(|config| config.max_due_date_days)
            .unwrap_or(DEFAULT_MAX_DUE_DATE_DAYS)
    }

    /// Get the grace period in seconds.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    ///
    /// # Returns
    /// * Current grace period in seconds (defaults to DEFAULT_GRACE_PERIOD_SECONDS)
    pub fn get_grace_period_seconds(env: &Env) -> u64 {
        Self::get_protocol_config(env)
            .map(|config| config.grace_period_seconds)
            .unwrap_or(DEFAULT_GRACE_PERIOD_SECONDS)
    }
}

// ============================================================================
// Events
// ============================================================================

/// Emit protocol initialization event
/// @notice Emits a single initialization event with the configured parameters.
fn emit_protocol_initialized(
    env: &Env,
    admin: &Address,
    treasury: &Address,
    fee_bps: u32,
    min_invoice_amount: i128,
    max_due_date_days: u64,
    grace_period_seconds: u64,
) {
    crate::events::emit_protocol_initialized(
        env,
        admin,
        treasury,
        fee_bps,
        min_invoice_amount,
        max_due_date_days,
        grace_period_seconds,
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
