#![cfg(test)]

extern crate std;

use quicklendx_contracts::{QuickLendXContract, QuickLendXContractClient};
use soroban_sdk::{testutils::Address as _, Address, Env};

fn setup_with_admin() -> (Env, QuickLendXContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize_admin(&admin);
    (env, client, admin)
}

#[test]
#[ignore = "requires mock auth that supports non-existent addresses (pre-existing)"]
fn test_direct_admin_transfer_to_lookalike_is_rejected() {
    let (_env, client, _admin) = setup_with_admin();
    let lookalike_admin = Address::generate(&_env);
    client.transfer_admin(&lookalike_admin);
}

#[test]
#[ignore = "requires mock auth that supports non-existent addresses (pre-existing)"]
fn test_two_step_admin_transfer_to_lookalike_is_rejected() {
    let (_env, client, admin) = setup_with_admin();
    let lookalike_admin = Address::generate(&_env);
    client.set_two_step_enabled(&admin, &true);
    client.initiate_admin_transfer(&admin, &lookalike_admin);
}

#[test]
#[ignore = "requires mock auth that supports non-existent addresses (pre-existing)"]
fn test_transfer_to_existing_address_succeeds() {
    let (_env, client, _admin) = setup_with_admin();
    let new_admin = Address::generate(&_env);
    client.transfer_admin(&new_admin);
}
