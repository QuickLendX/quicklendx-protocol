#![cfg(test)]

extern crate std;

use quicklendx_contracts::admin::AdminStorage;
use quicklendx_contracts::errors::QuickLendXError;
use quicklendx_contracts::{QuickLendXContract, QuickLendXContractClient};
use soroban_sdk::{testutils::Address as _, Address, Env};

fn setup_with_admin() -> (Env, Address, QuickLendXContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.set_admin(&admin);
    (env, contract_id, client, admin)
}

/// Unique to integration tests: `AdminStorage::require_existing_transfer_destination`
/// is active under `#[cfg(not(test))]`, so direct transfers to a non-existent
/// lookalike address are rejected with `InvalidAddress`.
#[test]
fn test_direct_admin_transfer_to_lookalike_is_rejected() {
    let (env, contract_id, _client, admin) = setup_with_admin();
    let lookalike_admin = Address::generate(&env);

    env.as_contract(&contract_id, || {
        let res = AdminStorage::transfer_admin(&env, &admin, &lookalike_admin);
        assert_eq!(res, Err(QuickLendXError::InvalidAddress));
    });
}

/// Unique to integration tests: initiating a two-step admin transfer to a
/// non-existent lookalike address is rejected with `InvalidAddress`.
#[test]
fn test_two_step_admin_transfer_to_lookalike_is_rejected() {
    let (env, contract_id, _client, admin) = setup_with_admin();
    let lookalike_admin = Address::generate(&env);

    env.as_contract(&contract_id, || {
        AdminStorage::set_two_step_enabled(&env, &admin, true).unwrap();
    });
    env.as_contract(&contract_id, || {
        let res = AdminStorage::initiate_admin_transfer(&env, &admin, &lookalike_admin);
        assert_eq!(res, Err(QuickLendXError::InvalidAddress));
    });
}

/// Unique to integration tests: transferring admin to an address that exists
/// on-ledger (e.g., a deployed contract address) succeeds under `#[cfg(not(test))]`.
#[test]
fn test_transfer_to_existing_address_succeeds() {
    let (env, contract_id, client, admin) = setup_with_admin();
    let new_admin = env.register(QuickLendXContract, ());

    env.as_contract(&contract_id, || {
        AdminStorage::transfer_admin(&env, &admin, &new_admin).unwrap();
    });
    assert_eq!(client.get_admin(), new_admin);
}
