//! Regression tests for platform fee ownership (issue #2939).
//!
//! `plt_fee` must have exactly one owner: [`FeeManager`]. These tests pin the
//! security properties that matter:
//!
//! * Only the protocol admin can change the fee.
//! * Changing the fee never clears or overwrites the configured treasury.
//! * Settlement reads the fee through the same configuration the admin writes.

#![cfg(test)]

use crate::admin::AdminStorage;
use crate::errors::QuickLendXError;
use crate::fees::FeeManager;
use crate::payments::allocate_repayment;
use crate::profits::{PlatformFee, BPS_DENOMINATOR};
use crate::QuickLendXContract;
use soroban_sdk::{testutils::Address as _, Address, Env};

/// Deploy the placeholder contract and initialize the admin + fee modules.
///
/// Each authorization-sensitive call runs in its own `as_contract` frame so a
/// single test does not call `require_auth` twice for the same address inside
/// one frame.
fn setup() -> (Env, Address, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(QuickLendXContract, ());
    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);

    env.as_contract(&contract_id, || AdminStorage::initialize(&env, &admin))
        .expect("admin initialization");
    env.as_contract(&contract_id, || FeeManager::initialize(&env, &admin))
        .expect("fee manager initialization");
    env.as_contract(&contract_id, || {
        FeeManager::configure_treasury(&env, &admin, treasury.clone())
    })
    .expect("treasury configuration");

    (env, contract_id, admin, treasury)
}

#[test]
fn non_admin_cannot_set_platform_fee() {
    let (env, contract_id, _admin, treasury) = setup();
    let attacker = Address::generate(&env);

    // The attacker signs only for themselves and is not the protocol admin.
    let result = env.as_contract(&contract_id, || {
        PlatformFee::set_config(&env, &attacker, 1000)
    });

    assert_eq!(result, Err(QuickLendXError::NotAdmin));

    let (fee_bps, stored_treasury) = env.as_contract(&contract_id, || {
        let config = PlatformFee::get_config(&env);
        (config.fee_bps, config.treasury_address)
    });

    assert_eq!(fee_bps, 200, "fee must remain at the default");
    assert_eq!(
        stored_treasury,
        Some(treasury.clone()),
        "treasury must not be erased by a rejected fee change"
    );
    assert_eq!(
        env.as_contract(&contract_id, || FeeManager::get_treasury_address(&env)),
        Some(treasury)
    );
}

#[test]
fn fee_update_preserves_treasury() {
    let (env, contract_id, admin, treasury) = setup();

    env.as_contract(&contract_id, || PlatformFee::set_config(&env, &admin, 500))
        .expect("admin fee update");

    let config = env.as_contract(&contract_id, || PlatformFee::get_config(&env));
    assert_eq!(config.fee_bps, 500);
    assert_eq!(config.treasury_address, Some(treasury.clone()));
    assert_eq!(
        env.as_contract(&contract_id, || FeeManager::get_treasury_address(&env)),
        Some(treasury)
    );
}

#[test]
fn settlement_uses_updated_fee() {
    let (env, contract_id, admin, _treasury) = setup();

    env.as_contract(&contract_id, || PlatformFee::set_config(&env, &admin, 500))
        .expect("admin fee update");

    // Mirrors `repay_escrow`: the fee input to `allocate_repayment` is read
    // from `PlatformFee::get_config`.
    let fee_bps = env.as_contract(&contract_id, || PlatformFee::get_config(&env).fee_bps);
    assert_eq!(fee_bps, 500);

    let allocation = allocate_repayment(1_000, 2_000, fee_bps as i128, 0, BPS_DENOMINATOR)
        .expect("repayment allocation");
    assert_eq!(allocation.gross_profit, 1_000);
    assert_eq!(allocation.platform_fee, 50); // 5% of 1_000 profit
}
