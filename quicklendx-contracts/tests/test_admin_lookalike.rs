#![cfg(test)]

extern crate std;

use quicklendx_contracts::{QuickLendXContract, QuickLendXContractClient};
use soroban_sdk::{testutils::Address as _, Address, Env};

/// Sets up the test environment with an initialized contract and an admin address
/// that is guaranteed to have a ledger entry.
fn setup() -> (Env, QuickLendXContractClient, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, QuickLendXContract);
    let client = QuickLendXContractClient::new(&env, &contract_id);

    // Generate an admin address and initialize the contract with it.
    // This ensures the admin address has a ledger entry and `exists()` will return true.
    let admin = Address::generate(&env);
    client.initialize_admin(&admin);

    (env, client, admin)
}

#[test]
#[should_panic(expected = "Error(Contract, #1201)")]
fn test_direct_admin_transfer_to_lookalike_is_rejected() {
    let (env, client, admin) = setup();

    // A "lookalike" address is syntactically valid but has no on-ledger entry.
    let lookalike_admin = Address::generate(&env);

    // Pre-condition check: The lookalike address should not exist yet.
    // Note: This assert is for clarity; the contract's `exists()` check is the real guard.
    assert!(!lookalike_admin.exists());

    // Action: Attempt a direct admin transfer to the non-existent address.
    // Expectation: The call panics with `QuickLendXError::InvalidAddress` (1201).
    client.transfer_admin(&admin, &lookalike_admin);
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

#[test]
fn test_transfer_to_existing_address_succeeds() {
    let (env, client, admin) = setup();

    // Create a new valid admin address that is guaranteed to exist.
    let new_admin = Address::generate(&env);
    client.initialize_protocol_limits(&new_admin, 1, 1, 1); // Using any auth'd function makes it exist.

    // Action: Transfer admin to the new, existing address.
    client.transfer_admin(&admin, &new_admin);

    // Assert: The admin was successfully updated.
    assert_eq!(client.get_current_admin(), Some(new_admin));
}