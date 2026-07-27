#![cfg(test)]

use crate::errors::QuickLendXError;
use crate::pause::PauseControl;
use crate::upgrade::UpgradeControl;
use soroban_sdk::{testutils::Address as _, Address, BytesN, Env, Wasm};

fn setup_test() -> (Env, Address, BytesN<32>) {
    let env = Env::default();
    let admin = Address::generate(&env);
    let wasm_hash = env.deployer().upload_contract_wasm(Wasm::from(&[]));

    // Register admin
    crate::admin::AdminStorage::set_admin(&env, &admin);

    (env, admin, wasm_hash)
}

#[test]
fn test_schedule_and_cancel_upgrade() {
    let (env, admin, wasm_hash) = setup_test();

    // Schedule upgrade
    UpgradeControl::schedule_upgrade(&env, &admin, &wasm_hash).unwrap();
    assert!(UpgradeControl::is_pending_upgrade(&env));
    assert!(PauseControl::is_paused(&env));

    // Cancel upgrade
    UpgradeControl::cancel_upgrade(&env, &admin).unwrap();
    assert!(!UpgradeControl::is_pending_upgrade(&env));
    assert!(!PauseControl::is_paused(&env));
}

#[test]
fn test_schedule_and_execute_upgrade() {
    let (env, admin, wasm_hash) = setup_test();

    // Schedule upgrade
    UpgradeControl::schedule_upgrade(&env, &admin, &wasm_hash).unwrap();
    assert!(UpgradeControl::is_pending_upgrade(&env));

    // Execute upgrade
    UpgradeControl::execute_upgrade(&env, &admin).unwrap();
    assert!(!UpgradeControl::is_pending_upgrade(&env));
    assert!(!PauseControl::is_paused(&env));
}

#[test]
fn test_cannot_write_while_upgrade_pending() {
    let (env, admin, wasm_hash) = setup_test();

    // Schedule an upgrade
    UpgradeControl::schedule_upgrade(&env, &admin, &wasm_hash).unwrap();

    // Try to call require_not_paused – should return UpgradePending error
    let err = PauseControl::require_not_paused(&env).unwrap_err();
    assert_eq!(err, QuickLendXError::UpgradePending);
}

#[test]
fn test_schedule_twice_fails() {
    let (env, admin, wasm_hash) = setup_test();

    UpgradeControl::schedule_upgrade(&env, &admin, &wasm_hash).unwrap();

    let err = UpgradeControl::schedule_upgrade(&env, &admin, &wasm_hash).unwrap_err();
    assert_eq!(err, QuickLendXError::OperationNotAllowed);
}

#[test]
fn test_cancel_without_schedule_fails() {
    let (env, admin, _) = setup_test();

    let err = UpgradeControl::cancel_upgrade(&env, &admin).unwrap_err();
    assert_eq!(err, QuickLendXError::OperationNotAllowed);
}

#[test]
fn test_execute_without_schedule_fails() {
    let (env, admin, _) = setup_test();

    let err = UpgradeControl::execute_upgrade(&env, &admin).unwrap_err();
    assert_eq!(err, QuickLendXError::OperationNotAllowed);
}

#[test]
fn test_non_admin_cannot_schedule() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let non_admin = Address::generate(&env);
    let wasm_hash = env.deployer().upload_contract_wasm(Wasm::from(&[]));

    crate::admin::AdminStorage::set_admin(&env, &admin);

    let err = UpgradeControl::schedule_upgrade(&env, &non_admin, &wasm_hash).unwrap_err();
    // Either NotAdmin from admin check or auth failure
    assert!(matches!(
        err,
        QuickLendXError::NotAdmin | QuickLendXError::OperationNotAllowed
    ));
}
