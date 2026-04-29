//! Hardened admin role management for the QuickLendX protocol.
//!
//! This module provides a secure, single-admin system with one-time
//! initialization, authenticated transfers, optional two-step handoff, and
//! transfer-lock protections.

#![allow(dead_code)]

use crate::errors::QuickLendXError;
use soroban_sdk::{symbol_short, Address, Env, Symbol};

/// Current admin storage key.
pub const ADMIN_KEY: Symbol = symbol_short!("admin");
/// Initialization flag storage key.
pub const ADMIN_INITIALIZED_KEY: Symbol = symbol_short!("adm_init");
/// Transfer-lock storage key.
pub const ADMIN_TRANSFER_LOCK_KEY: Symbol = symbol_short!("adm_lock");
/// Pending admin (for two-step transfer) storage key.
pub const ADMIN_PENDING_KEY: Symbol = symbol_short!("adm_pnd");
/// Two-step mode storage key.
pub const ADMIN_TWO_STEP_KEY: Symbol = symbol_short!("adm_2st");

/// Admin storage and management operations with hardened security checks.
pub struct AdminStorage;

impl AdminStorage {
    /// Initialize the admin once.
    ///
    /// # Security
    /// - `admin` must authorize their own appointment.
    /// - Initialization is one-time only.
    pub fn initialize(env: &Env, admin: &Address) -> Result<(), QuickLendXError> {
        admin.require_auth();

        if Self::is_initialized(env) {
            return Err(QuickLendXError::OperationNotAllowed);
        }

        env.storage().instance().set(&ADMIN_KEY, admin);
        env.storage().instance().set(&ADMIN_INITIALIZED_KEY, &true);

        crate::events::emit_admin_initialized(env, admin);
        Ok(())
    }

    /// Transfer admin role.
    ///
    /// In one-step mode, this updates `ADMIN_KEY` atomically.
    /// In two-step mode, this creates a pending transfer that must be accepted
    /// by `new_admin` through [`Self::accept_admin_transfer`].
    pub fn transfer_admin(
        env: &Env,
        current_admin: &Address,
        new_admin: &Address,
    ) -> Result<(), QuickLendXError> {
        current_admin.require_auth();

        if !Self::is_initialized(env) {
            return Err(QuickLendXError::OperationNotAllowed);
        }

        Self::require_admin(env, current_admin)?;

        if current_admin == new_admin {
            return Err(QuickLendXError::OperationNotAllowed);
        }

        if Self::is_two_step_enabled(env) {
            return Self::initiate_admin_transfer_internal(env, current_admin, new_admin);
        }

        if Self::is_transfer_locked(env) || Self::get_pending_admin(env).is_some() {
            return Err(QuickLendXError::OperationNotAllowed);
        }

        Self::set_transfer_lock(env, true);
        env.storage().instance().set(&ADMIN_KEY, new_admin);
        Self::set_transfer_lock(env, false);

        crate::events::emit_admin_transferred(env, current_admin, new_admin);
        Ok(())
    }

    /// Initiate two-step admin transfer by writing a pending admin and locking.
    pub fn initiate_admin_transfer(
        env: &Env,
        current_admin: &Address,
        pending_admin: &Address,
    ) -> Result<(), QuickLendXError> {
        current_admin.require_auth();

        if !Self::is_initialized(env) {
            return Err(QuickLendXError::OperationNotAllowed);
        }

        Self::require_admin(env, current_admin)?;
        Self::initiate_admin_transfer_internal(env, current_admin, pending_admin)
    }

    fn initiate_admin_transfer_internal(
        env: &Env,
        current_admin: &Address,
        pending_admin: &Address,
    ) -> Result<(), QuickLendXError> {
        if current_admin == pending_admin {
            return Err(QuickLendXError::OperationNotAllowed);
        }

        if Self::is_transfer_locked(env) || Self::get_pending_admin(env).is_some() {
            return Err(QuickLendXError::OperationNotAllowed);
        }

        env.storage()
            .instance()
            .set(&ADMIN_PENDING_KEY, pending_admin);
        Self::set_transfer_lock(env, true);

        crate::events::emit_admin_transfer_initiated(env, current_admin, pending_admin);
        Ok(())
    }

    /// Accept a pending two-step admin transfer.
    pub fn accept_admin_transfer(
        env: &Env,
        pending_admin: &Address,
    ) -> Result<(), QuickLendXError> {
        pending_admin.require_auth();

        if !Self::is_initialized(env) {
            return Err(QuickLendXError::OperationNotAllowed);
        }

        let current_admin = Self::get_admin(env).ok_or(QuickLendXError::OperationNotAllowed)?;
        let expected_pending =
            Self::get_pending_admin(env).ok_or(QuickLendXError::OperationNotAllowed)?;

        if expected_pending != *pending_admin {
            return Err(QuickLendXError::Unauthorized);
        }

        env.storage().instance().set(&ADMIN_KEY, pending_admin);
        env.storage().instance().remove(&ADMIN_PENDING_KEY);
        Self::set_transfer_lock(env, false);

        crate::events::emit_admin_transferred(env, &current_admin, pending_admin);
        Ok(())
    }

    /// Cancel a pending two-step admin transfer.
    pub fn cancel_admin_transfer(
        env: &Env,
        current_admin: &Address,
    ) -> Result<(), QuickLendXError> {
        current_admin.require_auth();
        Self::require_admin(env, current_admin)?;

        let pending_admin =
            Self::get_pending_admin(env).ok_or(QuickLendXError::OperationNotAllowed)?;
        env.storage().instance().remove(&ADMIN_PENDING_KEY);
        Self::set_transfer_lock(env, false);

        crate::events::emit_admin_transfer_cancelled(env, current_admin, &pending_admin);
        Ok(())
    }

    /// Enable or disable two-step transfer mode.
    pub fn set_two_step_enabled(
        env: &Env,
        admin: &Address,
        enabled: bool,
    ) -> Result<(), QuickLendXError> {
        admin.require_auth();
        Self::require_admin(env, admin)?;

        if enabled {
            env.storage().instance().set(&ADMIN_TWO_STEP_KEY, &true);
        } else {
            env.storage().instance().remove(&ADMIN_TWO_STEP_KEY);
            env.storage().instance().remove(&ADMIN_PENDING_KEY);
            Self::set_transfer_lock(env, false);
        }

        crate::events::emit_admin_two_step_updated(env, admin, enabled);
        Ok(())
    }

    /// Returns true when two-step transfer mode is enabled.
    pub fn is_two_step_enabled(env: &Env) -> bool {
        env.storage()
            .instance()
            .get(&ADMIN_TWO_STEP_KEY)
            .unwrap_or(false)
    }

    /// Legacy set_admin function for compatibility.
    pub fn set_admin(
        env: &Env,
        current_admin: &Address,
        new_admin: &Address,
    ) -> Result<(), QuickLendXError> {
        Self::transfer_admin(env, current_admin, new_admin)
    }

    /// Get current admin.
    pub fn get_admin(env: &Env) -> Option<Address> {
        env.storage().instance().get(&ADMIN_KEY)
    }

    /// Check whether admin subsystem has been initialized.
    pub fn is_initialized(env: &Env) -> bool {
        env.storage()
            .instance()
            .get(&ADMIN_INITIALIZED_KEY)
            .unwrap_or(false)
    }

    /// Check whether `address` is current admin.
    pub fn is_admin(env: &Env, address: &Address) -> bool {
        if let Some(admin) = Self::get_admin(env) {
            admin == *address
        } else {
            false
        }
    }

    /// Require that `address` is the current admin.
    pub fn require_admin(env: &Env, address: &Address) -> Result<(), QuickLendXError> {
        if !Self::is_initialized(env) {
            return Err(QuickLendXError::OperationNotAllowed);
        }

        if !Self::is_admin(env, address) {
            return Err(QuickLendXError::NotAdmin);
        }

        Ok(())
    }

    /// Alias for [`Self::require_admin`] with explicit auth.
    #[inline]
    pub fn require_admin_auth(env: &Env, address: &Address) -> Result<(), QuickLendXError> {
        address.require_auth();
        Self::require_admin(env, address)
    }

    /// Require current admin auth and return the verified admin.
    pub fn require_current_admin(env: &Env) -> Result<Address, QuickLendXError> {
        let admin = Self::get_admin(env).ok_or(QuickLendXError::OperationNotAllowed)?;
        admin.require_auth();
        Self::require_admin(env, &admin)?;
        Ok(admin)
    }

    /// Return whether transfers are currently locked.
    pub fn is_transfer_locked(env: &Env) -> bool {
        env.storage()
            .instance()
            .get(&ADMIN_TRANSFER_LOCK_KEY)
            .unwrap_or(false)
    }

    /// Return pending admin when two-step transfer is in progress.
    pub fn get_pending_admin(env: &Env) -> Option<Address> {
        env.storage().instance().get(&ADMIN_PENDING_KEY)
    }

    fn set_transfer_lock(env: &Env, locked: bool) {
        if locked {
            env.storage()
                .instance()
                .set(&ADMIN_TRANSFER_LOCK_KEY, &true);
        } else {
            env.storage().instance().remove(&ADMIN_TRANSFER_LOCK_KEY);
        }
    }

    /// Legacy compatibility entrypoint.
    ///
    /// - If uninitialized: initialize with `admin`.
    /// - If initialized: transfer from current admin to `admin`.
    pub fn set_admin_legacy(env: &Env, admin: &Address) -> Result<(), QuickLendXError> {
        if Self::is_initialized(env) {
            let current_admin = Self::get_admin(env).ok_or(QuickLendXError::NotAdmin)?;
            Self::transfer_admin(env, &current_admin, admin)
        } else {
            Self::initialize(env, admin)
        }
    }
}

impl AdminStorage {
    /// Execute `operation` only if `admin` is authenticated and authorized.
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

    /// Execute `operation` with authenticated current admin.
    pub fn with_current_admin<T, F>(env: &Env, operation: F) -> Result<T, QuickLendXError>
    where
        F: FnOnce(&Address) -> Result<T, QuickLendXError>,
    {
        let admin = Self::require_current_admin(env)?;
        operation(&admin)
    }
}
