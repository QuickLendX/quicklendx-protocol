//! #2940 — FeeManager admin-mutating APIs must reject non-admin signers.
#![cfg(test)]

use crate::admin::AdminStorage;
use crate::errors::QuickLendXError;
use crate::fees::{FeeManager, FeeType, RevenueConfig};
use crate::QuickLendXContract;
use soroban_sdk::{testutils::Address as _, Address, Env};

fn setup(env: &Env) -> (Address, Address, Address) {
    let contract_id = env.register(QuickLendXContract, ());
    let admin = Address::generate(env);
    let attacker = Address::generate(env);

    env.mock_all_auths();
    env.as_contract(&contract_id, || {
        AdminStorage::initialize(env, &admin).expect("admin init");
        FeeManager::initialize(env, &admin).expect("fee init");
    });

    (contract_id, admin, attacker)
}

#[test]
fn non_admin_cannot_initiate_rotation() {
    let env = Env::default();
    let (contract_id, _admin, attacker) = setup(&env);
    let new_treasury = Address::generate(&env);

    env.as_contract(&contract_id, || {
        let err =
            FeeManager::initiate_treasury_rotation(&env, &attacker, new_treasury).unwrap_err();
        assert_eq!(err, QuickLendXError::NotAdmin);
        assert!(FeeManager::get_pending_rotation(&env).is_none());
    });
}

#[test]
fn non_admin_cannot_cancel_rotation() {
    let env = Env::default();
    let (contract_id, admin, attacker) = setup(&env);
    let new_treasury = Address::generate(&env);

    env.as_contract(&contract_id, || {
        FeeManager::initiate_treasury_rotation(&env, &admin, new_treasury)
            .expect("admin can initiate");
        assert!(FeeManager::get_pending_rotation(&env).is_some());

        let err = FeeManager::cancel_treasury_rotation(&env, &attacker).unwrap_err();
        assert_eq!(err, QuickLendXError::NotAdmin);
        assert!(FeeManager::get_pending_rotation(&env).is_some());
    });
}

#[test]
fn non_admin_cannot_update_fee_structure() {
    let env = Env::default();
    let (contract_id, _admin, attacker) = setup(&env);

    env.as_contract(&contract_id, || {
        let before = FeeManager::get_fee_structure(&env, &FeeType::Platform)
            .expect("platform fee structure exists");

        let err = FeeManager::update_fee_structure(
            &env,
            &attacker,
            FeeType::Platform,
            1000,
            0,
            1_000_000,
            true,
        )
        .unwrap_err();
        assert_eq!(err, QuickLendXError::NotAdmin);

        let after = FeeManager::get_fee_structure(&env, &FeeType::Platform)
            .expect("platform fee structure still exists");
        assert_eq!(before.base_fee_bps, after.base_fee_bps);
        assert_eq!(before.min_fee, after.min_fee);
        assert_eq!(before.max_fee, after.max_fee);
        assert_eq!(before.is_active, after.is_active);
    });
}

#[test]
fn non_admin_cannot_configure_or_distribute_revenue() {
    let env = Env::default();
    let (contract_id, _admin, attacker) = setup(&env);
    let treasury = Address::generate(&env);

    env.as_contract(&contract_id, || {
        let cfg = RevenueConfig {
            treasury_address: treasury,
            treasury_share_bps: 5000,
            developer_share_bps: 0,
            platform_share_bps: 5000,
            auto_distribution: false,
            min_distribution_amount: 0,
        };

        let err_cfg =
            FeeManager::configure_revenue_distribution(&env, &attacker, cfg).unwrap_err();
        assert_eq!(err_cfg, QuickLendXError::NotAdmin);

        let period = env.ledger().timestamp() / 2_592_000;
        let err_dist = FeeManager::distribute_revenue(&env, &attacker, period).unwrap_err();
        assert_eq!(err_dist, QuickLendXError::NotAdmin);
    });
}

#[test]
fn admin_paths_unchanged() {
    let env = Env::default();
    let (contract_id, admin, _attacker) = setup(&env);
    let new_treasury = Address::generate(&env);
    let treasury = Address::generate(&env);

    env.as_contract(&contract_id, || {
        let req = FeeManager::initiate_treasury_rotation(&env, &admin, new_treasury.clone())
            .expect("admin initiate");
        assert_eq!(req.new_address, new_treasury);
        assert!(FeeManager::get_pending_rotation(&env).is_some());

        FeeManager::cancel_treasury_rotation(&env, &admin).expect("admin cancel");
        assert!(FeeManager::get_pending_rotation(&env).is_none());

        let updated = FeeManager::update_fee_structure(
            &env,
            &admin,
            FeeType::Platform,
            250,
            100,
            1_000_000,
            true,
        )
        .expect("admin update fee structure");
        assert_eq!(updated.base_fee_bps, 250);

        let cfg = RevenueConfig {
            treasury_address: treasury,
            treasury_share_bps: 5000,
            developer_share_bps: 0,
            platform_share_bps: 5000,
            auto_distribution: false,
            min_distribution_amount: 100,
        };
        FeeManager::configure_revenue_distribution(&env, &admin, cfg)
            .expect("admin configure revenue");
    });
}
