#![cfg(test)]

extern crate std;

use quicklendx_contracts::{QuickLendXContract, QuickLendXContractClient};
use quicklendx_contracts::errors::QuickLendXError;
use soroban_sdk::{testutils::Address as _, Address, Env};

/// Sets up the test environment with an initialized contract and an admin address
/// that is guaranteed to have a ledger entry.
fn setup() -> (Env, QuickLendXContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    client.initialize_admin(&admin);

    (env, client, admin)
}

#[test]
fn test_transfer_to_same_address_is_rejected() {
    let (env, client, admin) = setup();

    let result = client.try_transfer_admin(&admin);
    assert_eq!(result, Err(Ok(QuickLendXError::OperationNotAllowed)));
}

#[test]
#[should_panic(expected = "Error(Contract, #1201)")]
fn test_two_step_admin_transfer_to_lookalike_is_rejected() {
    let (env, client, admin) = setup();

    // A "lookalike" address is syntactically valid but has no on-ledger entry.
    let lookalike_admin = Address::generate(&env);

    // Pre-condition check: The lookalike address should not exist yet.
    assert!(!lookalike_admin.exists());

    // Enable two-step transfers to test the other protected path.
    client.set_two_step_enabled(&admin, &true);

    // Action: Attempt to initiate a two-step admin transfer to the non-existent address.
    // Expectation: The call panics with `QuickLendXError::InvalidAddress` (1201).
    client.initiate_admin_transfer(&admin, &lookalike_admin);
}