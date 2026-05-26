#![cfg(test)]

use soroban_sdk::{testutils::Address as _, Address, Env};

use crate::{admin::AdminStorage, errors::QuickLendXError};

#[test]
fn standalone_admin_security_flow() {
    let env = Env::default();
    env.mock_all_auths();

    let admin_1 = Address::generate(&env);
    let admin_2 = Address::generate(&env);
    let attacker = Address::generate(&env);

    assert_eq!(AdminStorage::initialize(&env, &admin_1), Ok(()));
    assert_eq!(
        AdminStorage::initialize(&env, &admin_2),
        Err(QuickLendXError::OperationNotAllowed)
    );

    assert_eq!(
        AdminStorage::transfer_admin(&env, &attacker, &admin_2),
        Err(QuickLendXError::NotAdmin)
    );

    assert_eq!(
        AdminStorage::transfer_admin(&env, &admin_1, &admin_2),
        Ok(())
    );
    assert_eq!(AdminStorage::get_admin(&env), Some(admin_2));
}

#[test]
fn standalone_two_step_flow() {
    let env = Env::default();
    env.mock_all_auths();

    let admin_1 = Address::generate(&env);
    let admin_2 = Address::generate(&env);

    AdminStorage::initialize(&env, &admin_1).unwrap();
    AdminStorage::set_two_step_enabled(&env, &admin_1, true).unwrap();

    AdminStorage::transfer_admin(&env, &admin_1, &admin_2).unwrap();
    assert_eq!(AdminStorage::get_pending_admin(&env), Some(admin_2.clone()));
    assert!(AdminStorage::is_transfer_locked(&env));

    AdminStorage::accept_admin_transfer(&env, &admin_2).unwrap();
    assert_eq!(AdminStorage::get_admin(&env), Some(admin_2));
    assert_eq!(AdminStorage::get_pending_admin(&env), None);
    assert!(!AdminStorage::is_transfer_locked(&env));
}
