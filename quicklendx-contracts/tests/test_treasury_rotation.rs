#![cfg(test)]

extern crate std;

use quicklendx_contracts::admin;
use quicklendx_contracts::errors::QuickLendXError;
use quicklendx_contracts::fees::FeeManager;
use quicklendx_contracts::storage;
use quicklendx_contracts::{QuickLendXContract, QuickLendXContractClient};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env};

fn setup() -> (Env, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin_addr = Address::generate(&env);
    client.set_admin(&admin_addr);
    client.initialize_fee_system(&admin_addr);
    (env, contract_id, admin_addr)
}

fn seed_pending_treasury(env: &Env, treasury: &Address, execute_at: u64) {
    env.storage()
        .instance()
        .set(&storage::PENDING_TREASURY_KEY, treasury);
    env.storage()
        .instance()
        .set(&storage::PENDING_TREASURY_TS_KEY, &execute_at);
}

/// Unique to integration tests: `admin::cancel_treasury_rotation` clears the
/// `storage` pending-treasury slot (in-crate `src/test_treasury_rotation.rs`
/// only exercises `FeeManager`'s `ROTATION_KEY` flow).
#[test]
fn test_admin_cancel_treasury_rotation_clears_pending_storage() {
    let (env, contract_id, admin_addr) = setup();
    let new_treasury = Address::generate(&env);

    env.as_contract(&contract_id, || {
        seed_pending_treasury(&env, &new_treasury, 1_000);
        assert!(storage::get_pending_treasury(&env).is_some());

        admin::cancel_treasury_rotation(&env, &admin_addr).expect("admin cancel should succeed");
        assert!(storage::get_pending_treasury(&env).is_none());
    });
}

/// Unique to integration tests: `admin::cancel_treasury_rotation` returns
/// `NoPendingTreasuryRotation` (#1858) when no pending treasury is stored.
#[test]
fn test_admin_cancel_treasury_rotation_fails_if_no_pending_rotation() {
    let (env, contract_id, admin_addr) = setup();

    env.as_contract(&contract_id, || {
        let err = admin::cancel_treasury_rotation(&env, &admin_addr).unwrap_err();
        assert_eq!(err, QuickLendXError::NoPendingTreasuryRotation);
    });
}

/// Unique to integration tests: non-admin callers are rejected with `NotAdmin`
/// on both `admin::cancel_treasury_rotation` and `FeeManager::cancel_treasury_rotation`
/// (in-crate `src/test_treasury_rotation.rs` only tests admin-authorized cancellation).
#[test]
fn test_cancel_treasury_rotation_fails_for_non_admin() {
    let (env, contract_id, admin_addr) = setup();
    let non_admin = Address::generate(&env);
    let new_treasury = Address::generate(&env);

    env.as_contract(&contract_id, || {
        seed_pending_treasury(&env, &new_treasury, 1_000);
        let admin_err = admin::cancel_treasury_rotation(&env, &non_admin).unwrap_err();
        assert_eq!(admin_err, QuickLendXError::NotAdmin);
    });

    env.as_contract(&contract_id, || {
        FeeManager::initiate_treasury_rotation(&env, &admin_addr, new_treasury.clone())
            .expect("admin initiate should succeed");
    });

    env.as_contract(&contract_id, || {
        let fee_err = FeeManager::cancel_treasury_rotation(&env, &non_admin).unwrap_err();
        assert_eq!(fee_err, QuickLendXError::NotAdmin);
    });
}
