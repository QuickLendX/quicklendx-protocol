//! Hardened admin role management for the QuickLendX protocol.
//!
//! This module provides a secure, single-admin system with robust initialization
//! and role transfer protections. It enforces one-time initialization, explicit
//! authorization checks, and comprehensive audit trails.
//!
//! # Security Model
//!
//! - **Single admin address**: MVP design with clear ownership
//! - **One-time initialization**: Admin can only be set once during protocol setup
//! - **Authenticated transfers**: Role transfers require current admin authorization
//! - **Explicit authorization**: All privileged operations require admin auth
//! - **Audit trail**: All admin operations emit events for transparency
//!
//! # Invariants
//!
//! 1. Admin can only be initialized once (atomic check-and-set)
//! 2. Only the current admin can transfer the role
//! 3. Admin transfers are atomic (no intermediate states)
//! 4. All admin operations require explicit authorization
//! 5. Admin state is always consistent across storage keys
//!
//! # Storage Design
//!
//! Uses instance storage with isolated keys:
//! - `ADMIN_KEY`: Current admin address (single source of truth)
//! - `ADMIN_INITIALIZED_KEY`: Initialization flag (prevents re-initialization)
//! - `ADMIN_TRANSFER_LOCK_KEY`: Transfer lock (prevents concurrent transfers)

use crate::errors::QuickLendXError;
use soroban_sdk::{symbol_short, Address, Env, Symbol};

/// Storage keys for admin management
pub const ADMIN_KEY: Symbol = symbol_short!("admin");
pub const ADMIN_INITIALIZED_KEY: Symbol = symbol_short!("adm_init");
pub const ADMIN_TRANSFER_LOCK_KEY: Symbol = symbol_short!("adm_lock");

/// Admin storage and management operations with hardened security
pub struct AdminStorage;

impl AdminStorage {
    /// Initialize the admin address with hardened security checks.
    ///
    /// This function performs atomic initialization with comprehensive validation:
    /// - Requires explicit authorization from the admin address
    /// - Enforces one-time initialization (cannot be called twice)
    /// - Uses atomic check-and-set to prevent race conditions
    /// - Emits audit event for transparency
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `admin` - The address to set as admin
    ///
    /// # Returns
    /// * `Ok(())` if initialization succeeds
    /// * `Err(QuickLendXError::OperationNotAllowed)` if admin was already set
    ///
    /// # Security Invariants
    /// - Admin must authorize their own appointment (prevents third-party admin setting)
    /// - Initialization flag is checked atomically before any state changes
    /// - All storage operations are atomic (no partial state)
    /// - Event emission provides audit trail
    pub fn initialize(env: &Env, admin: &Address) -> Result<(), QuickLendXError> {
        // SECURITY: Require explicit authorization from the admin address
        // This prevents third parties from setting arbitrary admin addresses
        admin.require_auth();

        // INVARIANT: Check initialization state atomically
        let is_initialized: bool = env
            .storage()
            .instance()
            .get(&ADMIN_INITIALIZED_KEY)
            .unwrap_or(false);

        if is_initialized {
            return Err(QuickLendXError::OperationNotAllowed);
        }

        // ATOMIC: Set admin and initialization flag together
        // This ensures no intermediate state where admin is set but not marked initialized
        env.storage().instance().set(&ADMIN_KEY, admin);
        env.storage().instance().set(&ADMIN_INITIALIZED_KEY, &true);

        // AUDIT: Emit initialization event
        emit_admin_initialized(env, admin);

        Ok(())
    }

    /// Transfer admin role with hardened security and atomic operations.
    ///
    /// This function implements secure admin role transfer with:
    /// - Current admin authorization requirement
    /// - Atomic role transfer (no intermediate states)
    /// - Transfer lock to prevent concurrent operations
    /// - Comprehensive validation and audit trail
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `current_admin` - The current admin address (must authorize)
    /// * `new_admin` - The new admin address
    ///
    /// # Returns
    /// * `Ok(())` if transfer succeeds
    /// * `Err(QuickLendXError::NotAdmin)` if caller is not current admin
    /// * `Err(QuickLendXError::OperationNotAllowed)` if admin not initialized or transfer locked
    ///
    /// # Security Invariants
    /// - Only current admin can initiate transfer
    /// - Transfer is atomic (no partial state)
    /// - Transfer lock prevents concurrent operations
    /// - New admin address is validated
    /// - Audit event is emitted
    pub fn transfer_admin(
        env: &Env,
        current_admin: &Address,
        new_admin: &Address,
    ) -> Result<(), QuickLendXError> {
        // SECURITY: Require authorization from current admin
        current_admin.require_auth();

        // INVARIANT: Ensure admin system is initialized
        if !Self::is_initialized(env) {
            return Err(QuickLendXError::OperationNotAllowed);
        }

        // SECURITY: Verify caller is actually the current admin
        Self::require_admin(env, current_admin)?;

        // CONCURRENCY: Check for transfer lock
        if Self::is_transfer_locked(env) {
            return Err(QuickLendXError::OperationNotAllowed);
        }

        // VALIDATION: Ensure new admin is different from current
        if current_admin == new_admin {
            return Err(QuickLendXError::OperationNotAllowed);
        }

        // ATOMIC: Set transfer lock, update admin, clear lock
        Self::set_transfer_lock(env, true);
        env.storage().instance().set(&ADMIN_KEY, new_admin);
        Self::set_transfer_lock(env, false);

        // AUDIT: Emit transfer event
        emit_admin_transferred(env, current_admin, new_admin);

        Ok(())
    }

    /// Legacy set_admin function for backward compatibility.
    ///
    /// This function provides the same interface as the original set_admin
    /// but routes to the appropriate hardened function based on state.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `current_admin` - The current admin address (must authorize)
    /// * `new_admin` - The new admin address
    ///
    /// # Returns
    /// * `Ok(())` if operation succeeds
    /// * Appropriate error if validation fails
    pub fn set_admin(
        env: &Env,
        current_admin: &Address,
        new_admin: &Address,
    ) -> Result<(), QuickLendXError> {
        Self::transfer_admin(env, current_admin, new_admin)
    }

    /// Get the current admin address.
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

    /// Check if the admin system has been initialized.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    ///
    /// # Returns
    /// * `true` if admin has been initialized
    /// * `false` otherwise
    pub fn is_initialized(env: &Env) -> bool {
        env.storage()
            .instance()
            .get(&ADMIN_INITIALIZED_KEY)
            .unwrap_or(false)
    }

    /// Check if an address is the current admin.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `address` - The address to check
    ///
    /// # Returns
    /// * `true` if the address is the current admin
    /// * `false` otherwise (including if admin not initialized)
    pub fn is_admin(env: &Env, address: &Address) -> bool {
        if let Some(admin) = Self::get_admin(env) {
            admin == *address
        } else {
            false
        }
    }

    /// Require that an address is the admin with comprehensive validation.
    ///
    /// This function provides hardened admin verification:
    /// - Checks if admin system is initialized
    /// - Verifies the address matches current admin
    /// - Returns specific error codes for different failure modes
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `address` - The address to verify
    ///
    /// # Returns
    /// * `Ok(())` if the address is the admin
    /// * `Err(QuickLendXError::NotAdmin)` if not admin
    /// * `Err(QuickLendXError::OperationNotAllowed)` if admin not initialized
    ///
    /// # Usage
    /// Use this helper in functions that require admin privileges:
    /// ```ignore
    /// AdminStorage::require_admin(&env, &caller)?;
    /// ```
    pub fn require_admin(env: &Env, address: &Address) -> Result<(), QuickLendXError> {
        // INVARIANT: Admin system must be initialized
        if !Self::is_initialized(env) {
            return Err(QuickLendXError::OperationNotAllowed);
        }

        // SECURITY: Verify address is current admin
        if !Self::is_admin(env, address) {
            return Err(QuickLendXError::NotAdmin);
        }

        Ok(())
    }

    pub fn require_admin_auth(env: &Env, address: &Address) -> Result<(), QuickLendXError> {
        address.require_auth();
        Self::require_admin(env, address)
    }

    /// Require admin authorization and return the verified admin address.
    ///
    /// This is a convenience function that combines authorization requirement
    /// with admin verification, returning the admin address for further use.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    ///
    /// # Returns
    /// * `Ok(Address)` - The verified admin address
    /// * `Err(QuickLendXError::NotAdmin)` if no admin or caller not admin
    /// * `Err(QuickLendXError::OperationNotAllowed)` if admin not initialized
    pub fn require_current_admin(env: &Env) -> Result<Address, QuickLendXError> {
        let admin = Self::get_admin(env).ok_or(QuickLendXError::OperationNotAllowed)?;
        admin.require_auth();
        Self::require_admin(env, &admin)?;
        Ok(admin)
    }

    /// Check if admin transfer is currently locked.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    ///
    /// # Returns
    /// * `true` if transfer is locked
    /// * `false` otherwise
    fn is_transfer_locked(env: &Env) -> bool {
        env.storage()
            .instance()
            .get(&ADMIN_TRANSFER_LOCK_KEY)
            .unwrap_or(false)
    }

    /// Set the admin transfer lock state.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `locked` - Whether to lock or unlock transfers
    fn set_transfer_lock(env: &Env, locked: bool) {
        if locked {
            env.storage()
                .instance()
                .set(&ADMIN_TRANSFER_LOCK_KEY, &true);
        } else {
            env.storage().instance().remove(&ADMIN_TRANSFER_LOCK_KEY);
        }
    }

    /// Legacy compatibility function for existing code.
    ///
    /// This function provides backward compatibility with existing `set_admin` calls
    /// while maintaining security invariants. It routes to either initialization
    /// or transfer based on current state.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `admin` - The admin address
    ///
    /// # Returns
    /// * `Ok(())` if operation succeeds
    /// * Appropriate error if validation fails
    pub fn set_admin_legacy(env: &Env, admin: &Address) -> Result<(), QuickLendXError> {
        if Self::is_initialized(env) {
            // If initialized, this is a transfer operation
            let current_admin = Self::get_admin(env).ok_or(QuickLendXError::NotAdmin)?;
            Self::transfer_admin(env, &current_admin, admin)
        } else {
            // If not initialized, this is initialization
            Self::initialize(env, admin)
        }
    }
}

// ============================================================================
// Events
// ============================================================================

/// Emit event when admin is first initialized
fn emit_admin_initialized(env: &Env, admin: &Address) {
    env.events().publish(
        (symbol_short!("adm_init"),),
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

// ============================================================================
// Security Utilities
// ============================================================================

/// Utility functions for admin-protected operations
impl AdminStorage {
    /// Execute a function with admin authorization check.
    ///
    /// This utility function provides a clean way to wrap admin-only operations
    /// with consistent authorization and error handling.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `admin` - The admin address to verify
    /// * `operation` - The function to execute if admin check passes
    ///
    /// # Returns
    /// * `Ok(T)` if admin check passes and operation succeeds
    /// * `Err(QuickLendXError)` if admin check fails or operation fails
    pub fn with_admin_auth<T, F>(
        env: &Env,
        admin: &Address,
        operation: F,
    ) -> Result<T, QuickLendXError>
    where
        F: FnOnce() -> Result<T, QuickLendXError>,
    {
        admin.require_auth();
        Self::require_admin(env, admin)?;
        operation()
    }

    /// Execute a function with current admin authorization.
    ///
    /// This utility automatically determines the current admin and requires
    /// their authorization before executing the operation.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `operation` - The function to execute with admin context
    ///
    /// # Returns
    /// * `Ok(T)` if admin check passes and operation succeeds
    /// * `Err(QuickLendXError)` if admin check fails or operation fails
    pub fn with_current_admin<T, F>(env: &Env, operation: F) -> Result<T, QuickLendXError>
    where
        F: FnOnce(&Address) -> Result<T, QuickLendXError>,
    {
        let admin = Self::require_current_admin(env)?;
        operation(&admin)
    }
}
