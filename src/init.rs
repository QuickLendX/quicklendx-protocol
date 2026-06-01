/// Protocol initialization module for QuickLendX.
///
/// Handles first-time setup of all protocol parameters with atomic validation.
use soroban_sdk::{symbol_short, Address, Env};

use crate::admin::{get_admin, require_admin};
use crate::errors::ContractError;
use crate::storage_types::{DataKey, FeeConfig, ProtocolConfig};

/// Initialize the protocol with its first configuration.
///
/// This function is intended to be called exactly **once** immediately after
/// admin initialization.  Subsequent updates must go through
/// [`set_protocol_config`](crate::admin::AdminContract::set_protocol_config) and
/// [`set_fee_config`](crate::admin::AdminContract::set_fee_config).
///
/// # Security
/// - Requires admin authorization.
/// - Both configs are validated **before** any storage write (atomic safety).
/// - A separate `Initialized` flag prevents re-initialization.
///
/// # Errors
/// Returns [`ContractError::AlreadyInitialized`] if called more than once.
pub fn initialize_protocol(
    env: &Env,
    admin: &Address,
    protocol_cfg: ProtocolConfig,
    fee_cfg: FeeConfig,
) -> Result<(), ContractError> {
    // Ensure admin is set and caller is authorized.
    require_admin(env, admin)?;

    // Prevent re-initialization.
    if env.storage().instance().has(&DataKey::Initialized) {
        return Err(ContractError::AlreadyInitialized);
    }

    // Validate both configs before writing anything.
    validate_protocol_config_init(&protocol_cfg)?;
    validate_fee_config_init(&fee_cfg, admin)?;

    // Atomic write — only reached if all validation passes.
    env.storage()
        .instance()
        .set(&DataKey::ProtocolConfig, &protocol_cfg);
    env.storage()
        .instance()
        .set(&DataKey::FeeConfig, &fee_cfg);
    env.storage()
        .instance()
        .set(&DataKey::Initialized, &true);

    env.events()
        .publish((symbol_short!("proto_in"),), (protocol_cfg, fee_cfg));

    Ok(())
}

// ---------------------------------------------------------------------------
// Init-time validation helpers (stricter than update-time)
// ---------------------------------------------------------------------------

fn validate_protocol_config_init(cfg: &ProtocolConfig) -> Result<(), ContractError> {
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

fn validate_fee_config_init(cfg: &FeeConfig, admin: &Address) -> Result<(), ContractError> {
    if cfg.fee_bps > 1000 {
        return Err(ContractError::InvalidFee);
    }
    // Treasury must not equal the admin address.
    if cfg.treasury == *admin {
        return Err(ContractError::InvalidParameter);
    }
    Ok(())
}
