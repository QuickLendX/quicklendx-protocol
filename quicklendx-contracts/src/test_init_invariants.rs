//! Initialization invariants test suite - Issue #833
//!
//! Verifies the four core invariants of protocol initialization:
//!
//! 1. **One-time init** - `initialize` can only succeed once; any subsequent
//!    call with different parameters returns `OperationNotAllowed`.
//! 2. **Admin/treasury distinct** - admin and treasury must be different
//!    addresses and neither may be the contract address itself.
//! 3. **Fee bps bounds** - `fee_bps` must be in `[0, 1000]`; values outside
//!    that range are rejected with `InvalidFeeBasisPoints`.
//! 4. **Limits configuration bounds** - `min_invoice_amount`, `max_due_date_days`,
//!    and `grace_period_seconds` are validated at init time and on every update.
//!
//! # Security assumptions validated here
//! - No config bypass: invalid params are always rejected before any state write.
//! - Deterministic validation: the same invalid input always produces the same error.
//! - State immutability after init: a failed re-init leaves all stored values unchanged.
//! - Admin/treasury separation: the protocol enforces role separation at the storage level.

#![cfg(test)]

use crate::errors::QuickLendXError;
use crate::init::{InitializationParams, ProtocolInitializer};
use crate::{QuickLendXContract, QuickLendXContractClient};
use soroban_sdk::{testutils::Address as _, Address, Env, Vec};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn setup() -> (Env, QuickLendXContractClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();
    let id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &id);
    (env, client)
}

/// Minimal valid params - all boundary values are within spec.
fn valid_params(env: &Env) -> InitializationParams {
    InitializationParams {
        admin: Address::generate(env),
        treasury: Address::generate(env),
        fee_bps: 200, // 2 %
        min_invoice_amount: 1_000_000,
        max_due_date_days: 365,
        grace_period_seconds: 604_800, // 7 days
        initial_currencies: Vec::new(env),
    }
}

/// Initialize the contract and return the params used.
fn initialized(env: &Env, client: &QuickLendXContractClient) -> InitializationParams {
    let p = valid_params(env);
    client.initialize(&p);
    p
}

// ===========================================================================
// 1. ONE-TIME INITIALIZATION INVARIANT
// ===========================================================================

/// Happy path: first call with valid params must succeed.
#[test]
fn test_init_succeeds_first_call() {
    let (env, client) = setup();
    let p = valid_params(&env);
    assert!(client.try_initialize(&p).is_ok());
    assert!(client.is_initialized());
}

/// Second call with *different* params must fail.
#[test]
fn test_init_second_call_different_params_fails() {
    let (env, client) = setup();
    initialized(&env, &client);

    let p2 = valid_params(&env); // fresh addresses -> different params
    let result = client.try_initialize(&p2);
    assert_eq!(
        result,
        Err(Ok(QuickLendXError::OperationNotAllowed)),
        "Re-init with different params must return OperationNotAllowed"
    );
}

/// Second call with *identical* params must be idempotent (Ok).
#[test]
fn test_init_idempotent_same_params() {
    let (env, client) = setup();
    let p = valid_params(&env);
    client.initialize(&p);
    // Exact same params -> idempotent success
    assert!(
        client.try_initialize(&p).is_ok(),
        "Re-init with identical params must be idempotent"
    );
}

/// `is_initialized` must be false before init and true after.
#[test]
fn test_is_initialized_flag_lifecycle() {
    let (env, client) = setup();
    assert!(!client.is_initialized(), "must be false before init");
    assert!(!ProtocolInitializer::is_initialized(&env));

    initialized(&env, &client);

    assert!(client.is_initialized(), "must be true after init");
    assert!(ProtocolInitializer::is_initialized(&env));
}

/// A failed re-init must not alter any stored values.
#[test]
fn test_failed_reinit_preserves_state() {
    let (env, client) = setup();
    let p = initialized(&env, &client);

    let p2 = valid_params(&env);
    let _ = client.try_initialize(&p2);

    // All original values must be unchanged
    assert_eq!(client.get_current_admin(), Some(p.admin.clone()));
    assert_eq!(client.get_treasury(), Some(p.treasury.clone()));
    assert_eq!(client.get_fee_bps(), p.fee_bps);
    assert_eq!(client.get_min_invoice_amount(), p.min_invoice_amount);
    assert_eq!(client.get_max_due_date_days(), p.max_due_date_days);
    assert_eq!(client.get_grace_period_seconds(), p.grace_period_seconds);
}

/// Multiple failed re-inits must not corrupt state.
#[test]
fn test_multiple_failed_reinits_preserve_state() {
    let (env, client) = setup();
    let p = initialized(&env, &client);

    for _ in 0..5 {
        let _ = client.try_initialize(&valid_params(&env));
    }

    assert_eq!(client.get_current_admin(), Some(p.admin));
    assert_eq!(client.get_treasury(), Some(p.treasury));
    assert_eq!(client.get_fee_bps(), p.fee_bps);
}

/// Protocol version must be written at init time and remain stable.
#[test]
fn test_version_written_at_init_and_stable() {
    let (env, client) = setup();
    let v_before = client.get_version();
    initialized(&env, &client);
    let v_after = client.get_version();
    assert_eq!(v_before, v_after, "version must not change across init");
    assert_eq!(v_after, crate::init::PROTOCOL_VERSION);
}

// ===========================================================================
// 2. ADMIN / TREASURY DISTINCT INVARIANT
// ===========================================================================

/// admin == treasury must be rejected.
#[test]
fn test_admin_equals_treasury_rejected() {
    let (env, client) = setup();
    let addr = Address::generate(&env);
    let mut p = valid_params(&env);
    p.admin = addr.clone();
    p.treasury = addr;
    assert_eq!(
        client.try_initialize(&p),
        Err(Ok(QuickLendXError::InvalidAddress)),
        "admin == treasury must be InvalidAddress"
    );
}

/// admin == contract address must be rejected.
#[test]
fn test_admin_is_contract_address_rejected() {
    let (env, client) = setup();
    let mut p = valid_params(&env);
    p.admin = client.address.clone();
    assert_eq!(
        client.try_initialize(&p),
        Err(Ok(QuickLendXError::InvalidAddress)),
        "admin == contract address must be InvalidAddress"
    );
}

/// treasury == contract address must be rejected.
#[test]
fn test_treasury_is_contract_address_rejected() {
    let (env, client) = setup();
    let mut p = valid_params(&env);
    p.treasury = client.address.clone();
    assert_eq!(
        client.try_initialize(&p),
        Err(Ok(QuickLendXError::InvalidAddress)),
        "treasury == contract address must be InvalidAddress"
    );
}

/// After init, admin and treasury must be stored as distinct addresses.
#[test]
fn test_stored_admin_and_treasury_are_distinct() {
    let (env, client) = setup();
    let p = initialized(&env, &client);
    let stored_admin = client.get_current_admin().unwrap();
    let stored_treasury = client.get_treasury().unwrap();
    assert_ne!(
        stored_admin, stored_treasury,
        "stored admin and treasury must be distinct"
    );
    assert_eq!(stored_admin, p.admin);
    assert_eq!(stored_treasury, p.treasury);
}

/// `set_treasury` must reject treasury == admin.
#[test]
fn test_set_treasury_same_as_admin_rejected() {
    let (env, client) = setup();
    let p = initialized(&env, &client);
    assert_eq!(
        client.try_set_treasury(&p.admin, &p.admin),
        Err(Ok(QuickLendXError::InvalidAddress)),
        "set_treasury with admin address must be InvalidAddress"
    );
}

/// `set_treasury` with a valid new address must succeed and update storage.
#[test]
fn test_set_treasury_valid_update() {
    let (env, client) = setup();
    let p = initialized(&env, &client);
    let new_treasury = Address::generate(&env);
    assert!(client.try_set_treasury(&p.admin, &new_treasury).is_ok());
    assert_eq!(client.get_treasury(), Some(new_treasury));
}

/// Non-admin must not be able to update treasury.
#[test]
fn test_set_treasury_non_admin_rejected() {
    let (env, client) = setup();
    initialized(&env, &client);
    let stranger = Address::generate(&env);
    let new_treasury = Address::generate(&env);
    assert_eq!(
        client.try_set_treasury(&stranger, &new_treasury),
        Err(Ok(QuickLendXError::NotAdmin)),
    );
}

// ===========================================================================
// 3. FEE BPS BOUNDS INVARIANT
// ===========================================================================

/// fee_bps = 0 (minimum) must be accepted.
#[test]
fn test_fee_bps_zero_accepted() {
    let (env, client) = setup();
    let mut p = valid_params(&env);
    p.fee_bps = 0;
    assert!(client.try_initialize(&p).is_ok());
    assert_eq!(client.get_fee_bps(), 0);
}

/// fee_bps = 1000 (maximum, 10 %) must be accepted.
#[test]
fn test_fee_bps_max_accepted() {
    let (env, client) = setup();
    let mut p = valid_params(&env);
    p.fee_bps = 1000;
    assert!(client.try_initialize(&p).is_ok());
    assert_eq!(client.get_fee_bps(), 1000);
}

/// fee_bps = 1001 (one above maximum) must be rejected.
#[test]
fn test_fee_bps_above_max_rejected() {
    let (env, client) = setup();
    let mut p = valid_params(&env);
    p.fee_bps = 1001;
    assert_eq!(
        client.try_initialize(&p),
        Err(Ok(QuickLendXError::InvalidFeeBasisPoints)),
        "fee_bps > 1000 must be InvalidFeeBasisPoints"
    );
}

/// fee_bps = u32::MAX must be rejected.
#[test]
fn test_fee_bps_u32_max_rejected() {
    let (env, client) = setup();
    let mut p = valid_params(&env);
    p.fee_bps = u32::MAX;
    assert_eq!(
        client.try_initialize(&p),
        Err(Ok(QuickLendXError::InvalidFeeBasisPoints)),
    );
}

/// Typical mid-range fee (500 bps = 5 %) must be accepted.
#[test]
fn test_fee_bps_midrange_accepted() {
    let (env, client) = setup();
    let mut p = valid_params(&env);
    p.fee_bps = 500;
    assert!(client.try_initialize(&p).is_ok());
    assert_eq!(client.get_fee_bps(), 500);
}

/// `set_fee_config` must enforce the same 0-1000 bounds.
#[test]
fn test_set_fee_config_bounds_enforced() {
    let (env, client) = setup();
    let p = initialized(&env, &client);

    // Valid updates
    assert!(client.try_set_fee_config(&p.admin, &0u32).is_ok());
    assert!(client.try_set_fee_config(&p.admin, &1000u32).is_ok());

    // Invalid updates
    assert_eq!(
        client.try_set_fee_config(&p.admin, &1001u32),
        Err(Ok(QuickLendXError::InvalidFeeBasisPoints)),
    );
    assert_eq!(
        client.try_set_fee_config(&p.admin, &u32::MAX),
        Err(Ok(QuickLendXError::InvalidFeeBasisPoints)),
    );
}

/// Non-admin must not be able to update fee config.
#[test]
fn test_set_fee_config_non_admin_rejected() {
    let (env, client) = setup();
    initialized(&env, &client);
    let stranger = Address::generate(&env);
    assert_eq!(
        client.try_set_fee_config(&stranger, &100u32),
        Err(Ok(QuickLendXError::NotAdmin)),
    );
}

/// Fee update must be reflected in storage immediately.
#[test]
fn test_set_fee_config_persisted() {
    let (env, client) = setup();
    let p = initialized(&env, &client);
    client.set_fee_config(&p.admin, &750u32);
    assert_eq!(client.get_fee_bps(), 750);
}

// ===========================================================================
// 4. LIMITS CONFIGURATION BOUNDS INVARIANT
// ===========================================================================

// --- min_invoice_amount ---

/// min_invoice_amount = 0 must be rejected.
#[test]
fn test_min_invoice_amount_zero_rejected() {
    let (env, client) = setup();
    let mut p = valid_params(&env);
    p.min_invoice_amount = 0;
    assert_eq!(
        client.try_initialize(&p),
        Err(Ok(QuickLendXError::InvalidAmount)),
    );
}

/// min_invoice_amount < 0 must be rejected.
#[test]
fn test_min_invoice_amount_negative_rejected() {
    let (env, client) = setup();
    let mut p = valid_params(&env);
    p.min_invoice_amount = -1;
    assert_eq!(
        client.try_initialize(&p),
        Err(Ok(QuickLendXError::InvalidAmount)),
    );
}

/// min_invoice_amount = 1 (minimum positive) must be accepted.
#[test]
fn test_min_invoice_amount_one_accepted() {
    let (env, client) = setup();
    let mut p = valid_params(&env);
    p.min_invoice_amount = 1;
    assert!(client.try_initialize(&p).is_ok());
    assert_eq!(client.get_min_invoice_amount(), 1);
}

/// Large min_invoice_amount must be accepted.
#[test]
fn test_min_invoice_amount_large_accepted() {
    let (env, client) = setup();
    let mut p = valid_params(&env);
    p.min_invoice_amount = i128::MAX / 2;
    assert!(client.try_initialize(&p).is_ok());
}

// --- max_due_date_days ---

/// max_due_date_days = 0 must be rejected.
#[test]
fn test_max_due_date_days_zero_rejected() {
    let (env, client) = setup();
    let mut p = valid_params(&env);
    p.max_due_date_days = 0;
    assert_eq!(
        client.try_initialize(&p),
        Err(Ok(QuickLendXError::InvoiceDueDateInvalid)),
    );
}

/// max_due_date_days = 731 (one above maximum) must be rejected.
#[test]
fn test_max_due_date_days_above_max_rejected() {
    let (env, client) = setup();
    let mut p = valid_params(&env);
    p.max_due_date_days = 731;
    assert_eq!(
        client.try_initialize(&p),
        Err(Ok(QuickLendXError::InvoiceDueDateInvalid)),
    );
}

/// max_due_date_days = 730 (maximum) must be accepted.
#[test]
fn test_max_due_date_days_max_accepted() {
    let (env, client) = setup();
    let mut p = valid_params(&env);
    p.max_due_date_days = 730;
    assert!(client.try_initialize(&p).is_ok());
    assert_eq!(client.get_max_due_date_days(), 730);
}

/// max_due_date_days = 1 (minimum) must be accepted.
#[test]
fn test_max_due_date_days_one_accepted() {
    let (env, client) = setup();
    let mut p = valid_params(&env);
    p.max_due_date_days = 1;
    assert!(client.try_initialize(&p).is_ok());
    assert_eq!(client.get_max_due_date_days(), 1);
}

// --- grace_period_seconds ---

/// grace_period_seconds = 0 must be accepted (no grace period).
#[test]
fn test_grace_period_zero_accepted() {
    let (env, client) = setup();
    let mut p = valid_params(&env);
    p.grace_period_seconds = 0;
    assert!(client.try_initialize(&p).is_ok());
    assert_eq!(client.get_grace_period_seconds(), 0);
}

/// grace_period_seconds = 2_592_000 (30 days, maximum) must be accepted.
#[test]
fn test_grace_period_max_accepted() {
    let (env, client) = setup();
    let mut p = valid_params(&env);
    p.grace_period_seconds = 2_592_000;
    assert!(client.try_initialize(&p).is_ok());
    assert_eq!(client.get_grace_period_seconds(), 2_592_000);
}

/// grace_period_seconds = 2_592_001 (one above maximum) must be rejected.
#[test]
fn test_grace_period_above_max_rejected() {
    let (env, client) = setup();
    let mut p = valid_params(&env);
    p.grace_period_seconds = 2_592_001;
    assert_eq!(
        client.try_initialize(&p),
        Err(Ok(QuickLendXError::InvalidTimestamp)),
    );
}

/// `set_protocol_config` must enforce the same bounds as init.
#[test]
fn test_set_protocol_config_bounds_enforced() {
    let (env, client) = setup();
    let p = initialized(&env, &client);

    // Invalid min_invoice_amount
    assert_eq!(
        client.try_set_protocol_config(&p.admin, &0i128, &365u64, &604_800u64),
        Err(Ok(QuickLendXError::InvalidAmount)),
    );

    // Invalid max_due_date_days = 0
    assert_eq!(
        client.try_set_protocol_config(&p.admin, &1_000_000i128, &0u64, &604_800u64),
        Err(Ok(QuickLendXError::InvoiceDueDateInvalid)),
    );

    // Invalid max_due_date_days > 730
    assert_eq!(
        client.try_set_protocol_config(&p.admin, &1_000_000i128, &731u64, &604_800u64),
        Err(Ok(QuickLendXError::InvoiceDueDateInvalid)),
    );

    // Invalid grace_period_seconds > 30 days
    assert_eq!(
        client.try_set_protocol_config(&p.admin, &1_000_000i128, &365u64, &2_592_001u64),
        Err(Ok(QuickLendXError::InvalidTimestamp)),
    );
}

/// Valid `set_protocol_config` must update all three fields atomically.
#[test]
fn test_set_protocol_config_valid_update_atomic() {
    let (env, client) = setup();
    let p = initialized(&env, &client);

    client.set_protocol_config(&p.admin, &500_000i128, &180u64, &86_400u64);

    assert_eq!(client.get_min_invoice_amount(), 500_000);
    assert_eq!(client.get_max_due_date_days(), 180);
    assert_eq!(client.get_grace_period_seconds(), 86_400);
}

/// Non-admin must not be able to update protocol config.
#[test]
fn test_set_protocol_config_non_admin_rejected() {
    let (env, client) = setup();
    initialized(&env, &client);
    let stranger = Address::generate(&env);
    assert_eq!(
        client.try_set_protocol_config(&stranger, &1_000_000i128, &365u64, &604_800u64),
        Err(Ok(QuickLendXError::NotAdmin)),
    );
}

// ===========================================================================
// 5. CURRENCY WHITELIST INVARIANTS
// ===========================================================================

/// Duplicate currencies in initial list must be rejected.
#[test]
fn test_init_duplicate_currencies_rejected() {
    let (env, client) = setup();
    let currency = Address::generate(&env);
    let mut p = valid_params(&env);
    p.initial_currencies = Vec::from_array(&env, [currency.clone(), currency]);
    assert_eq!(
        client.try_initialize(&p),
        Err(Ok(QuickLendXError::InvalidCurrency)),
    );
}

/// Currency equal to admin address must be rejected.
#[test]
fn test_init_currency_equals_admin_rejected() {
    let (env, client) = setup();
    let mut p = valid_params(&env);
    p.initial_currencies = Vec::from_array(&env, [p.admin.clone()]);
    assert_eq!(
        client.try_initialize(&p),
        Err(Ok(QuickLendXError::InvalidCurrency)),
    );
}

/// Currency equal to treasury address must be rejected.
#[test]
fn test_init_currency_equals_treasury_rejected() {
    let (env, client) = setup();
    let mut p = valid_params(&env);
    p.initial_currencies = Vec::from_array(&env, [p.treasury.clone()]);
    assert_eq!(
        client.try_initialize(&p),
        Err(Ok(QuickLendXError::InvalidCurrency)),
    );
}

/// Currency equal to contract address must be rejected.
#[test]
fn test_init_currency_equals_contract_rejected() {
    let (env, client) = setup();
    let mut p = valid_params(&env);
    p.initial_currencies = Vec::from_array(&env, [client.address.clone()]);
    assert_eq!(
        client.try_initialize(&p),
        Err(Ok(QuickLendXError::InvalidCurrency)),
    );
}

/// Valid distinct currencies must be accepted.
#[test]
fn test_init_valid_currencies_accepted() {
    let (env, client) = setup();
    let c1 = Address::generate(&env);
    let c2 = Address::generate(&env);
    let mut p = valid_params(&env);
    p.initial_currencies = Vec::from_array(&env, [c1, c2]);
    assert!(client.try_initialize(&p).is_ok());
}

// ===========================================================================
// 6. AUTHORIZATION INVARIANT
// ===========================================================================

/// Initialization without mocked auth must panic (require_auth enforced).
#[test]
fn test_init_requires_admin_auth() {
    let env = Env::default();
    // No mock_all_auths - auth is enforced
    let id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &id);
    let p = valid_params(&env);

    // Should fail without authorization (no mock_all_auths)
    let result = client.try_initialize(&p);
    assert!(result.is_err(), "init without auth must fail");
}

// ===========================================================================
// 7. QUERY DEFAULTS BEFORE INIT
// ===========================================================================

/// Before init, query functions must return safe defaults (not panic).
#[test]
fn test_query_defaults_before_init() {
    let (_env, client) = setup();
    assert!(!client.is_initialized());
    assert_eq!(client.get_treasury(), None);
    // fee_bps defaults to 200 (DEFAULT_FEE_BPS)
    assert_eq!(client.get_fee_bps(), 200);
    // min_invoice_amount defaults to 10 in test cfg
    assert_eq!(client.get_min_invoice_amount(), 10);
    assert_eq!(client.get_max_due_date_days(), 365);
    assert_eq!(client.get_grace_period_seconds(), 604_800);
}

/// After init, all query functions must return the stored values.
#[test]
fn test_query_values_after_init() {
    let (env, client) = setup();
    let p = initialized(&env, &client);

    assert_eq!(client.get_current_admin(), Some(p.admin.clone()));
    assert_eq!(client.get_treasury(), Some(p.treasury.clone()));
    assert_eq!(client.get_fee_bps(), p.fee_bps);
    assert_eq!(client.get_min_invoice_amount(), p.min_invoice_amount);
    assert_eq!(client.get_max_due_date_days(), p.max_due_date_days);
    assert_eq!(client.get_grace_period_seconds(), p.grace_period_seconds);
}

// ===========================================================================
// 8. PROTOCOL CONFIG STORED CORRECTLY
// ===========================================================================

/// ProtocolConfig must be None before init.
#[test]
fn test_protocol_config_none_before_init() {
    let (env, _client) = setup();
    assert!(ProtocolInitializer::get_protocol_config(&env).is_none());
}

/// ProtocolConfig must be Some after init with correct values.
#[test]
fn test_protocol_config_some_after_init() {
    let (env, _client) = setup();
    let p = valid_params(&env);
    let client = {
        let id = env.register(QuickLendXContract, ());
        QuickLendXContractClient::new(&env, &id)
    };
    client.initialize(&p);

    let cfg = ProtocolInitializer::get_protocol_config(&env).expect("config must exist");
    assert_eq!(cfg.min_invoice_amount, p.min_invoice_amount);
    assert_eq!(cfg.max_due_date_days, p.max_due_date_days);
    assert_eq!(cfg.grace_period_seconds, p.grace_period_seconds);
    assert_eq!(cfg.updated_by, p.admin);
}

// ===========================================================================
// 9. BOUNDARY COMBINATION - ALL LIMITS AT EXTREMES
// ===========================================================================

/// All parameters at their minimum valid values must succeed.
#[test]
fn test_all_params_at_minimum_boundary() {
    let (env, client) = setup();
    let mut p = valid_params(&env);
    p.fee_bps = 0;
    p.min_invoice_amount = 1;
    p.max_due_date_days = 1;
    p.grace_period_seconds = 0;
    assert!(client.try_initialize(&p).is_ok());
}

/// All parameters at their maximum valid values must succeed.
#[test]
fn test_all_params_at_maximum_boundary() {
    let (env, client) = setup();
    let mut p = valid_params(&env);
    p.fee_bps = 1000;
    p.min_invoice_amount = 1_000_000_000_000;
    p.max_due_date_days = 730;
    p.grace_period_seconds = 2_592_000;
    assert!(client.try_initialize(&p).is_ok());
}

// ===========================================================================
// 10. ADMIN TRANSFER AFTER INIT
// ===========================================================================

/// After admin transfer, new admin can update config; old admin cannot.
#[test]
fn test_admin_transfer_revokes_old_admin_config_access() {
    let (env, client) = setup();
    let p = initialized(&env, &client);
    let new_admin = Address::generate(&env);

    client.transfer_admin(&new_admin);

    // New admin can update fee
    assert!(client.try_set_fee_config(&new_admin, &300u32).is_ok());

    // Old admin is rejected
    assert_eq!(
        client.try_set_fee_config(&p.admin, &400u32),
        Err(Ok(QuickLendXError::NotAdmin)),
    );
}

// ===========================================================================
// 11. DETERMINISTIC VALIDATION - same invalid input -> same error
// ===========================================================================

/// Validation is deterministic: calling with the same invalid params always
/// returns the same error code regardless of call order.
#[test]
fn test_validation_is_deterministic() {
    for _ in 0..3 {
        let (env, client) = setup();
        let mut p = valid_params(&env);
        p.fee_bps = 9999;
        assert_eq!(
            client.try_initialize(&p),
            Err(Ok(QuickLendXError::InvalidFeeBasisPoints)),
        );
    }
}

/// Validation order: fee_bps is checked before min_invoice_amount.
/// When both are invalid, the fee error is returned.
#[test]
fn test_validation_order_fee_before_amount() {
    let (env, client) = setup();
    let mut p = valid_params(&env);
    p.fee_bps = 9999;
    p.min_invoice_amount = -1;
    // fee_bps is validated first
    assert_eq!(
        client.try_initialize(&p),
        Err(Ok(QuickLendXError::InvalidFeeBasisPoints)),
    );
}
