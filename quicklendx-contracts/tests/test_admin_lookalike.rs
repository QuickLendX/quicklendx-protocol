#![cfg(test)]

extern crate std;

use quicklendx_contracts::{QuickLendXContract, QuickLendXContractClient};
use quicklendx_contracts::errors::QuickLendXError;
use soroban_sdk::{testutils::Address as _, Address, Env};

fn setup() -> (Env, QuickLendXContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.set_admin(&admin);
    (env, client, admin)
}

/// Attempting a two-step admin transfer to an address that has never been used
/// (no on-ledger entry) must fail.
#[test]
fn test_two_step_admin_transfer_to_lookalike_is_rejected() {
    let (env, client, admin) = setup();
    let lookalike_admin = Address::generate(&env);
    client.set_two_step_enabled(&admin, &true);
    let result = client.try_initiate_admin_transfer(&admin, &lookalike_admin);
    assert!(
        result.is_err(),
        "admin transfer to a lookalike (non-existent) address must be rejected"
    );
}

/// Transferring admin to an address that already exists must succeed.
#[test]
fn test_transfer_to_existing_address_succeeds() {
    let (env, client, _admin) = setup();
    let new_admin = Address::generate(&env);
    client.submit_investor_kyc(&new_admin, &soroban_sdk::String::from_str(&env, "kyc"));
    let result = client.try_transfer_admin(&new_admin);
    assert!(
        result.is_ok(),
        "admin transfer to an existing address must succeed; got {:?}",
        result.err()
    );
}
