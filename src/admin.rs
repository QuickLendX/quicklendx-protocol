/// Admin module for QuickLendX protocol.
///
/// Provides secure, admin-gated operations for protocol and fee configuration,
/// including **dry-run preview** functions that return a projected before/after
/// diff without writing to storage.
use soroban_sdk::{contract, contractimpl, contracttype, symbol_short, Address, Env};

use crate::errors::ContractError;
use crate::storage_types::{DataKey, FeeConfig, ProtocolConfig};

// ---------------------------------------------------------------------------
// Diff types (read-only output, never stored)
// ---------------------------------------------------------------------------

/// A projected before/after diff for a protocol configuration update.
///
/// Returned by [`preview_protocol_config`] so operators can sanity-check
/// parameters before signing the apply transaction.  No storage writes occur
/// when this type is produced.
#[contracttype]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProtocolConfigDiff {
    /// The current (before) protocol configuration stored on-chain.
    pub current: ProtocolConfig,
    /// The projected (after) protocol configuration that *would* be applied.
    pub projected: ProtocolConfig,
    /// `true` when `current == projected` (no-op change).
    pub is_noop: bool,
}

/// A projected before/after diff for a fee configuration update.
///
/// Returned by [`preview_fee_config`] so operators can sanity-check parameters
/// before signing the apply transaction.  No storage writes occur when this
/// type is produced.
#[contracttype]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FeeConfigDiff {
    /// The current (before) fee configuration stored on-chain.
    pub current: FeeConfig,
    /// The projected (after) fee configuration that *would* be applied.
    pub projected: FeeConfig,
    /// `true` when `current == projected` (no-op change).
    pub is_noop: bool,
}

// ---------------------------------------------------------------------------
// Internal admin helpers
// ---------------------------------------------------------------------------

/// Retrieve the current admin address from storage.
///
/// Returns `ContractError::NotInitialized` if no admin has been set yet.
pub fn get_admin(env: &Env) -> Result<Address, ContractError> {
    env.storage()
        .instance()
        .get::<DataKey, Address>(&DataKey::Admin)
        .ok_or(ContractError::NotInitialized)
}

/// Verify that `caller` is the current admin and has provided authorization.
///
/// # Errors
/// - `ContractError::NotInitialized` – admin not set.
/// - `ContractError::NotAdmin` – `caller` is not the current admin.
pub fn require_admin(env: &Env, caller: &Address) -> Result<(), ContractError> {
    let admin = get_admin(env)?;
    if admin != *caller {
        return Err(ContractError::NotAdmin);
    }
    caller.require_auth();
    Ok(())
}

// ---------------------------------------------------------------------------
// Protocol configuration helpers
// ---------------------------------------------------------------------------

/// Read the current [`ProtocolConfig`] from storage.
///
/// Returns `ContractError::NotInitialized` if the protocol has not been
/// initialized yet.
pub fn get_protocol_config(env: &Env) -> Result<ProtocolConfig, ContractError> {
    env.storage()
        .instance()
        .get::<DataKey, ProtocolConfig>(&DataKey::ProtocolConfig)
        .ok_or(ContractError::NotInitialized)
}

/// Validate a [`ProtocolConfig`] value without writing it.
///
/// Rules (aligned with `set_protocol_config`):
/// - `min_invoice_amount` must be > 0  
/// - `max_due_date_days` must be in `[1, 730]`  
/// - `grace_period_seconds` must be ≤ 2_592_000 (30 days)
pub fn validate_protocol_config(cfg: &ProtocolConfig) -> Result<(), ContractError> {
    if cfg.min_invoice_amount == 0 {
        return Err(ContractError::InvalidAmount);
    }
    if cfg.max_due_date_days == 0 || cfg.max_due_date_days > 730 {
        return Err(ContractError::InvalidParameter);
    }
    if cfg.grace_period_seconds > 2_592_000 {
        return Err(ContractError::InvalidParameter);
    }
    Ok(())
}

/// Mutate (apply) a [`ProtocolConfig`] to storage and emit an audit event.
///
/// Callers **must** have already called [`require_admin`].
pub fn apply_protocol_config(env: &Env, cfg: &ProtocolConfig) -> Result<(), ContractError> {
    validate_protocol_config(cfg)?;
    env.storage()
        .instance()
        .set(&DataKey::ProtocolConfig, cfg);
    env.events().publish(
        (symbol_short!("proto_cfg"),),
        cfg.clone(),
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// Fee configuration helpers
// ---------------------------------------------------------------------------

/// Read the current [`FeeConfig`] from storage.
///
/// Returns `ContractError::NotInitialized` if the protocol has not been
/// initialized yet.
pub fn get_fee_config(env: &Env) -> Result<FeeConfig, ContractError> {
    env.storage()
        .instance()
        .get::<DataKey, FeeConfig>(&DataKey::FeeConfig)
        .ok_or(ContractError::NotInitialized)
}

/// Validate a [`FeeConfig`] value without writing it.
///
/// Rules (aligned with `set_fee_config`):
/// - `fee_bps` must be ≤ 1000 (max 10 %)
/// - `treasury` must not be the zero address (checked by caller requiring auth)
pub fn validate_fee_config(cfg: &FeeConfig) -> Result<(), ContractError> {
    if cfg.fee_bps > 1000 {
        return Err(ContractError::InvalidFee);
    }
    Ok(())
}

/// Mutate (apply) a [`FeeConfig`] to storage and emit an audit event.
///
/// Callers **must** have already called [`require_admin`].
pub fn apply_fee_config(env: &Env, cfg: &FeeConfig) -> Result<(), ContractError> {
    validate_fee_config(cfg)?;
    env.storage()
        .instance()
        .set(&DataKey::FeeConfig, cfg);
    env.events().publish(
        (symbol_short!("fee_cfg"),),
        cfg.clone(),
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// Public contract entry-points
// ---------------------------------------------------------------------------

#[contract]
pub struct AdminContract;

#[contractimpl]
impl AdminContract {
    // -----------------------------------------------------------------------
    // Initialization
    // -----------------------------------------------------------------------

    /// One-time admin initialization.  Can only be called once.
    pub fn initialize(env: Env, admin: Address) -> Result<(), ContractError> {
        if env
            .storage()
            .instance()
            .has(&DataKey::Admin)
        {
            return Err(ContractError::AlreadyInitialized);
        }
        admin.require_auth();
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.events()
            .publish((symbol_short!("adm_init"),), admin);
        Ok(())
    }

    /// Transfer the admin role to `new_admin`.
    ///
    /// Requires authorization from the *current* admin.
    pub fn transfer_admin(
        env: Env,
        current_admin: Address,
        new_admin: Address,
    ) -> Result<(), ContractError> {
        require_admin(&env, &current_admin)?;
        if current_admin == new_admin {
            return Err(ContractError::OperationNotAllowed);
        }
        env.storage().instance().set(&DataKey::Admin, &new_admin);
        env.events()
            .publish((symbol_short!("adm_trf"),), (current_admin, new_admin));
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Apply operations (mutating)
    // -----------------------------------------------------------------------

    /// Update the protocol configuration (mutating).
    ///
    /// Requires admin authorization.  Parameters are validated before any
    /// storage write.
    pub fn set_protocol_config(
        env: Env,
        admin: Address,
        new_config: ProtocolConfig,
    ) -> Result<(), ContractError> {
        require_admin(&env, &admin)?;
        apply_protocol_config(&env, &new_config)
    }

    /// Update the fee configuration (mutating).
    ///
    /// Requires admin authorization.  Parameters are validated before any
    /// storage write.
    pub fn set_fee_config(
        env: Env,
        admin: Address,
        new_config: FeeConfig,
    ) -> Result<(), ContractError> {
        require_admin(&env, &admin)?;
        apply_fee_config(&env, &new_config)
    }

    // -----------------------------------------------------------------------
    // Dry-run / preview operations (read-only)
    // -----------------------------------------------------------------------

    /// **Dry-run** – Preview the effect of applying `new_config` as the
    /// protocol configuration **without writing to storage**.
    ///
    /// # Security
    /// Admin-gated: the caller must be the current admin and provide
    /// authorization.  This prevents arbitrary parties from probing
    /// parameter-validation logic.
    ///
    /// # Returns
    /// A [`ProtocolConfigDiff`] containing the current on-chain config, the
    /// projected config that *would* result from applying `new_config`, and an
    /// `is_noop` flag that is `true` when the two configs are identical.
    ///
    /// # Errors
    /// Returns the same validation errors as [`set_protocol_config`].  If
    /// `new_config` is invalid the call fails before any read even completes,
    /// letting operators catch bad parameters cheaply.
    pub fn preview_protocol_config(
        env: Env,
        admin: Address,
        new_config: ProtocolConfig,
    ) -> Result<ProtocolConfigDiff, ContractError> {
        // Admin gate: read-only but still protected.
        require_admin(&env, &admin)?;

        // Validate parameters exactly as the apply path does.
        validate_protocol_config(&new_config)?;

        // Read current state (no write occurs).
        let current = get_protocol_config(&env)?;

        let is_noop = current == new_config;

        Ok(ProtocolConfigDiff {
            current,
            projected: new_config,
            is_noop,
        })
    }

    /// **Dry-run** – Preview the effect of applying `new_config` as the fee
    /// configuration **without writing to storage**.
    ///
    /// # Security
    /// Admin-gated: the caller must be the current admin and provide
    /// authorization.
    ///
    /// # Returns
    /// A [`FeeConfigDiff`] containing the current on-chain fee config, the
    /// projected config that *would* result, and an `is_noop` flag.
    ///
    /// # Errors
    /// Returns the same validation errors as [`set_fee_config`].
    pub fn preview_fee_config(
        env: Env,
        admin: Address,
        new_config: FeeConfig,
    ) -> Result<FeeConfigDiff, ContractError> {
        // Admin gate: read-only but still protected.
        require_admin(&env, &admin)?;

        // Validate parameters exactly as the apply path does.
        validate_fee_config(&new_config)?;

        // Read current state (no write occurs).
        let current = get_fee_config(&env)?;

        let is_noop = current == new_config;

        Ok(FeeConfigDiff {
            current,
            projected: new_config,
            is_noop,
        })
    }
}
