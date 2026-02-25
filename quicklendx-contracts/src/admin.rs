//! Admin role management for the QuickLendX protocol.
//!
//! This module provides a centralized admin system for managing privileged operations
//! including invoice verification, fee configuration, and KYC/business verification.
//!
//! # Security Model
//!
//! - Single admin address (MVP design)
//! - Admin can only be set once during initialization
//! - Admin can transfer role to another address
//! - All privileged operations require admin authorization
//!
//! # Future Extensibility
//!
//! The current single-admin design can be extended to support:
//! - Multiple oracle addresses for automated verification
//! - Role-based access control (RBAC) for different privilege levels
//! - Multi-signature admin operations
//!
//! # Storage Design
//!
//! Uses instance storage for:
//! - Admin address (single source of truth)
//! - Initialization flag (prevents re-initialization)

use crate::errors::QuickLendXError;
use soroban_sdk::{symbol_short, Address, Env, Symbol};

/// Storage keys for admin management
pub const ADMIN_KEY: Symbol = symbol_short!("admin");
pub const ADMIN_INITIALIZED_KEY: Symbol = symbol_short!("adm_init");

/// Admin storage and management operations
pub struct AdminStorage;

impl AdminStorage {
    /// Initialize the admin address (can only be called once)
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `admin` - The address to set as admin
    ///
    /// # Returns
    /// * `Ok(())` if initialization succeeds
    /// * `Err(QuickLendXError::OperationNotAllowed)` if admin was already set
    ///
    /// # Security
    /// - Requires authorization from the admin address
    /// - Can only be called once (checked via ADMIN_INITIALIZED_KEY)
    /// - Emits AdminSet event for transparency
    pub fn initialize(env: &Env, admin: &Address) -> Result<(), QuickLendXError> {
        // Auth is handled by ProtocolInitializer::initialize

        // Check if already initialized
        let is_initialized: bool = env
            .storage()
            .instance()
            .get(&ADMIN_INITIALIZED_KEY)
            .unwrap_or(false);

        if is_initialized {
            return Err(QuickLendXError::OperationNotAllowed);
        }

        // Set admin and mark as initialized
        env.storage().instance().set(&ADMIN_KEY, admin);
        env.storage().instance().set(&ADMIN_INITIALIZED_KEY, &true);

        // Emit event
        emit_admin_set(env, admin);

        Ok(())
    }

    /// Transfer admin role to a new address
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `current_admin` - The current admin address (must authorize)
    /// * `new_admin` - The new admin address
    ///
    /// # Returns
    /// * `Ok(())` if transfer succeeds
    /// * `Err(QuickLendXError::NotAdmin)` if caller is not current admin
    ///
    /// # Security
    /// - Requires authorization from current admin
    /// - Verifies caller is actually the current admin
    /// - Emits AdminTransferred event for audit trail
    pub fn set_admin(
        env: &Env,
        current_admin: &Address,
        new_admin: &Address,
    ) -> Result<(), QuickLendXError> {
        current_admin.require_auth();

        // Verify caller is current admin
        if !Self::is_admin(env, current_admin) {
            return Err(QuickLendXError::NotAdmin);
        }

        // Set new admin
        env.storage().instance().set(&ADMIN_KEY, new_admin);

        // Emit event
        emit_admin_transferred(env, current_admin, new_admin);

        Ok(())
    }

    /// Get the current admin address
    ///
    /// # Arguments
    /// * `env` - The contract environment
    ///
    /// # Returns
    /// * `Some(Address)` if admin is set
    /// * `None` if admin has not been initialized
    pub fn get_admin(env: &Env) -> Option<Address> {
        env.storage().instance().get(&ADMIN_KEY)
    }

    /// Check if an address is the admin
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `address` - The address to check
    ///
    /// # Returns
    /// * `true` if the address is the current admin
    /// * `false` otherwise
    pub fn is_admin(env: &Env, address: &Address) -> bool {
        if let Some(admin) = Self::get_admin(env) {
            admin == *address
        } else {
            false
        }
    }

    /// Require that an address is the admin (authorization helper)
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `address` - The address to verify
    ///
    /// # Returns
    /// * `Ok(())` if the address is the admin
    /// * `Err(QuickLendXError::NotAdmin)` if not admin or admin not set
    ///
    /// # Usage
    /// Use this helper in functions that require admin privileges:
    /// ```ignore
    /// AdminStorage::require_admin(&env, &caller)?;
    /// ```
    pub fn require_admin(env: &Env, address: &Address) -> Result<(), QuickLendXError> {
        if !Self::is_admin(env, address) {
            return Err(QuickLendXError::NotAdmin);
        }
        Ok(())
    }
}

/// Emit event when admin is first initialized
fn emit_admin_set(env: &Env, admin: &Address) {
    env.events().publish(
        (symbol_short!("adm_set"),),
        (admin.clone(), env.ledger().timestamp()),
    );
}

/// Emit event when admin role is transferred
fn emit_admin_transferred(env: &Env, old_admin: &Address, new_admin: &Address) {
    env.events().publish(
        (symbol_short!("adm_trf"),),
        (
            old_admin.clone(),
            new_admin.clone(),
            env.ledger().timestamp(),
        ),
    );
}
