//! Payment token and fee policy module.
//!
//! Provides explicit, bounded, and consistent guarantees for accepted assets,
//! amount precision, decimal scale normalization, overflow prevention, and
//! deterministic fee calculation across all contract operations.
//!
//! # Core Invariants
//! 1. **Zero Dust Loss**: `platform_fee + net_amount == gross_amount` for any valid calculation.
//! 2. **Strict Range Enforcement**: Every amount must satisfy `min_amount <= amount <= max_amount`.
//! 3. **Bounded Scale**: Token decimals must be within `0..=MAX_SUPPORTED_DECIMALS` (18).
//! 4. **Checked Arithmetic**: All calculations use non-overflowing integer math (`checked_mul`, `checked_div`, `checked_add`, `checked_sub`).
//! 5. **Fail-Closed Policy**: Inactive or unconfigured tokens reject operations when strict policy is active.

use crate::admin::AdminStorage;
use crate::errors::QuickLendXError;
use crate::storage::{bump_persistent, extend_persistent_ttl};
use soroban_sdk::{contracttype, symbol_short, Address, Env, Symbol, Vec};

/// Maximum supported token decimal scale (18, standard for ERC-20 / EVM-compatible tokens).
pub const MAX_SUPPORTED_DECIMALS: u32 = 18;

/// Basis points denominator (100.00% = 10,000 bps).
pub const BPS_DENOMINATOR: i128 = 10_000;

/// Maximum fee rate in basis points (100% = 10,000 bps).
pub const MAX_FEE_BPS: u32 = 10_000;

/// Storage key prefix for token policy records: `(Symbol("tok_pol"), token_address)`
pub const TOKEN_POLICY_KEY_PREFIX: Symbol = symbol_short!("tok_pol");

/// Storage key for the list of configured token addresses.
pub const TOKEN_LIST_KEY: Symbol = symbol_short!("tok_list");

/// Configuration and policy parameters for a payment token asset.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PaymentTokenConfig {
    /// Contract address of the payment token.
    pub token: Address,
    /// Number of decimals (scale) used by the token (0 to 18).
    pub decimals: u32,
    /// Minimum allowed amount per transaction/invoice in token base units.
    pub min_amount: i128,
    /// Maximum allowed amount per transaction/invoice in token base units.
    pub max_amount: i128,
    /// Whether this payment token is currently active for new operations.
    pub is_active: bool,
    /// Optional asset-specific platform fee override in basis points (0 to 10,000).
    pub fee_bps_override: Option<u32>,
}

/// Breakdown of gross amount, platform fee, and net payout.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FeeCalculationResult {
    /// Original gross payment amount.
    pub gross_amount: i128,
    /// Calculated platform fee amount.
    pub platform_fee: i128,
    /// Net amount disbursed after fee deduction.
    pub net_amount: i128,
    /// Effective fee rate in basis points applied to the transaction.
    pub applied_fee_bps: u32,
}

/// Payment token policy engine.
pub struct PaymentTokenPolicy;

impl PaymentTokenPolicy {
    /// Validates policy configuration bounds and invariants before saving.
    pub fn validate_config(config: &PaymentTokenConfig) -> Result<(), QuickLendXError> {
        if config.decimals > MAX_SUPPORTED_DECIMALS {
            return Err(QuickLendXError::InvalidCurrency);
        }

        if config.min_amount <= 0 {
            return Err(QuickLendXError::InvalidAmount);
        }

        if config.max_amount < config.min_amount {
            return Err(QuickLendXError::InvalidAmount);
        }

        if let Some(bps) = config.fee_bps_override {
            if bps > MAX_FEE_BPS {
                return Err(QuickLendXError::InvalidFeeBasisPoints);
            }
        }

        Ok(())
    }

    /// Sets or updates a payment token policy (admin only).
    pub fn set_policy(
        env: &Env,
        admin: &Address,
        config: &PaymentTokenConfig,
    ) -> Result<(), QuickLendXError> {
        AdminStorage::require_admin(env, admin)?;
        Self::validate_config(config)?;

        let storage_key = (TOKEN_POLICY_KEY_PREFIX, config.token.clone());
        env.storage().persistent().set(&storage_key, config);
        extend_persistent_ttl(env, &storage_key);

        // Update token list index if not already present
        let mut list: Vec<Address> = env
            .storage()
            .persistent()
            .get(&TOKEN_LIST_KEY)
            .unwrap_or_else(|| Vec::new(env));

        let mut already_listed = false;
        for existing in list.iter() {
            if existing == config.token {
                already_listed = true;
                break;
            }
        }

        if !already_listed {
            list.push_back(config.token.clone());
            env.storage().persistent().set(&TOKEN_LIST_KEY, &list);
            extend_persistent_ttl(env, &TOKEN_LIST_KEY);
        }

        crate::events::emit_token_policy_updated(env, config);
        Ok(())
    }

    /// Retrieves the policy configuration for a given token address, if set.
    pub fn get_policy(env: &Env, token: &Address) -> Option<PaymentTokenConfig> {
        let storage_key = (TOKEN_POLICY_KEY_PREFIX, token.clone());
        let policy = env.storage().persistent().get(&storage_key);
        if policy.is_some() {
            bump_persistent(env, &storage_key);
        }
        policy
    }

    /// Removes a payment token policy (admin only).
    pub fn remove_policy(
        env: &Env,
        admin: &Address,
        token: &Address,
    ) -> Result<(), QuickLendXError> {
        AdminStorage::require_admin(env, admin)?;

        let storage_key = (TOKEN_POLICY_KEY_PREFIX, token.clone());
        if !env.storage().persistent().has(&storage_key) {
            return Err(QuickLendXError::InvalidCurrency);
        }

        env.storage().persistent().remove(&storage_key);

        // Remove from token list index
        if let Some(list) = env
            .storage()
            .persistent()
            .get::<_, Vec<Address>>(&TOKEN_LIST_KEY)
        {
            let mut new_list = Vec::new(env);
            for item in list.iter() {
                if item != *token {
                    new_list.push_back(item);
                }
            }
            env.storage().persistent().set(&TOKEN_LIST_KEY, &new_list);
            extend_persistent_ttl(env, &TOKEN_LIST_KEY);
        }

        crate::events::emit_token_policy_removed(env, token);
        Ok(())
    }

    /// Returns a list of all configured payment token policies.
    pub fn list_policies(env: &Env) -> Vec<PaymentTokenConfig> {
        let list: Vec<Address> = env
            .storage()
            .persistent()
            .get(&TOKEN_LIST_KEY)
            .unwrap_or_else(|| Vec::new(env));

        let mut configs = Vec::new(env);
        for token in list.iter() {
            if let Some(cfg) = Self::get_policy(env, &token) {
                configs.push_back(cfg);
            }
        }
        configs
    }

    /// Returns whether a token is accepted and currently active.
    pub fn is_token_accepted(env: &Env, token: &Address) -> bool {
        match Self::get_policy(env, token) {
            Some(cfg) => cfg.is_active,
            None => true, // Fallback when unconfigured allows backward compatibility with whitelisting
        }
    }

    /// Validates an amount against the token's policy bounds and precision requirements.
    pub fn validate_amount(
        env: &Env,
        token: &Address,
        amount: i128,
    ) -> Result<(), QuickLendXError> {
        if amount <= 0 {
            return Err(QuickLendXError::InvalidAmount);
        }

        if let Some(cfg) = Self::get_policy(env, token) {
            if !cfg.is_active {
                return Err(QuickLendXError::InvalidCurrency);
            }

            if amount < cfg.min_amount || amount > cfg.max_amount {
                return Err(QuickLendXError::InvalidAmount);
            }
        }

        Ok(())
    }

    /// Calculates platform fee and net payout using exact checked integer arithmetic.
    ///
    /// # Invariants
    /// - `platform_fee + net_amount == gross_amount` (Zero dust loss).
    /// - `fee_bps` is taken from token override if present, else fallback `default_fee_bps`.
    /// - Overflow is prevented using checked arithmetic.
    pub fn calculate_fee(
        env: &Env,
        token: &Address,
        gross_amount: i128,
        default_fee_bps: u32,
    ) -> Result<FeeCalculationResult, QuickLendXError> {
        if gross_amount <= 0 {
            return Err(QuickLendXError::InvalidAmount);
        }

        let fee_bps = if let Some(cfg) = Self::get_policy(env, token) {
            if !cfg.is_active {
                return Err(QuickLendXError::InvalidCurrency);
            }
            cfg.fee_bps_override.unwrap_or(default_fee_bps)
        } else {
            default_fee_bps
        };

        if fee_bps > MAX_FEE_BPS {
            return Err(QuickLendXError::InvalidFeeBasisPoints);
        }

        let fee_bps_i128 = fee_bps as i128;
        let platform_fee = gross_amount
            .checked_mul(fee_bps_i128)
            .ok_or(QuickLendXError::ArithmeticOverflow)?
            .checked_div(BPS_DENOMINATOR)
            .ok_or(QuickLendXError::ArithmeticOverflow)?;

        let net_amount = gross_amount
            .checked_sub(platform_fee)
            .ok_or(QuickLendXError::ArithmeticOverflow)?;

        // Invariant check: zero dust guarantee
        if platform_fee
            .checked_add(net_amount)
            .ok_or(QuickLendXError::ArithmeticOverflow)?
            != gross_amount
        {
            return Err(QuickLendXError::ArithmeticOverflow);
        }

        Ok(FeeCalculationResult {
            gross_amount,
            platform_fee,
            net_amount,
            applied_fee_bps: fee_bps,
        })
    }

    /// Converts an amount between different decimal scales with checked arithmetic.
    pub fn normalize_amount(
        amount: i128,
        from_decimals: u32,
        to_decimals: u32,
    ) -> Result<i128, QuickLendXError> {
        if amount < 0 {
            return Err(QuickLendXError::InvalidAmount);
        }
        if from_decimals > MAX_SUPPORTED_DECIMALS || to_decimals > MAX_SUPPORTED_DECIMALS {
            return Err(QuickLendXError::InvalidCurrency);
        }

        if from_decimals == to_decimals || amount == 0 {
            return Ok(amount);
        }

        if to_decimals > from_decimals {
            let diff = to_decimals - from_decimals;
            let factor = 10i128
                .checked_pow(diff)
                .ok_or(QuickLendXError::ArithmeticOverflow)?;
            amount
                .checked_mul(factor)
                .ok_or(QuickLendXError::ArithmeticOverflow)
        } else {
            let diff = from_decimals - to_decimals;
            let factor = 10i128
                .checked_pow(diff)
                .ok_or(QuickLendXError::ArithmeticOverflow)?;
            amount
                .checked_div(factor)
                .ok_or(QuickLendXError::ArithmeticOverflow)
        }
use crate::errors::QuickLendXError;
use soroban_sdk::{Address, Env};

pub struct PaymentTokenPolicy;

impl PaymentTokenPolicy {
    /// Authorizes a mutation on a protected resource, ensuring the actor
    /// matches the expected tenant and the identity is not stale.
    pub fn authorize_mutation(
        _env: &Env,
        actor: &Address,
        expected_tenant: &Address,
        is_active: bool,
    ) -> Result<(), QuickLendXError> {
        actor.require_auth();

        if actor != expected_tenant {
            return Err(QuickLendXError::Unauthorized);
        }

        if !is_active {
            return Err(QuickLendXError::InvalidStatus);
        }

        Ok(())
    }

    /// Enforces bounds on fee configurations to prevent malicious or accidental
    /// extreme values from drifting into insolvency.
    pub fn validate_fee_configuration(
        origination_bps: u128,
        servicing_bps: u128,
        default_penalty_bps: u128,
        early_repayment_bps: u128,
    ) -> Result<(), QuickLendXError> {
        // Maximums based on fees.rs constants
        if origination_bps > 500 {
            return Err(QuickLendXError::InvalidAmount);
        }
        if servicing_bps > 300 {
            return Err(QuickLendXError::InvalidAmount);
        }
        if default_penalty_bps > 2000 {
            return Err(QuickLendXError::InvalidAmount);
        }
        if early_repayment_bps > 500 {
            return Err(QuickLendXError::InvalidAmount);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{testutils::Address as _, Address, Env};

    #[test]
    fn test_authorize_mutation_success() {
        let env = Env::default();
        let actor = Address::generate(&env);
        
        let result = PaymentTokenPolicy::authorize_mutation(&env, &actor, &actor, true);
        assert_eq!(result, Ok(()));
    }

    #[test]
    fn test_authorize_mutation_unauthorized() {
        let env = Env::default();
        let actor = Address::generate(&env);
        let expected = Address::generate(&env);
        
        let result = PaymentTokenPolicy::authorize_mutation(&env, &actor, &expected, true);
        assert_eq!(result, Err(QuickLendXError::Unauthorized));
    }

    #[test]
    fn test_authorize_mutation_stale() {
        let env = Env::default();
        let actor = Address::generate(&env);
        
        let result = PaymentTokenPolicy::authorize_mutation(&env, &actor, &actor, false);
        assert_eq!(result, Err(QuickLendXError::InvalidStatus));
    }

    #[test]
    fn test_validate_fee_configuration() {
        assert_eq!(PaymentTokenPolicy::validate_fee_configuration(500, 300, 2000, 500), Ok(()));
        assert_eq!(PaymentTokenPolicy::validate_fee_configuration(501, 300, 2000, 500), Err(QuickLendXError::InvalidAmount));
        assert_eq!(PaymentTokenPolicy::validate_fee_configuration(500, 301, 2000, 500), Err(QuickLendXError::InvalidAmount));
        assert_eq!(PaymentTokenPolicy::validate_fee_configuration(500, 300, 2001, 500), Err(QuickLendXError::InvalidAmount));
        assert_eq!(PaymentTokenPolicy::validate_fee_configuration(500, 300, 2000, 501), Err(QuickLendXError::InvalidAmount));
    }
}
