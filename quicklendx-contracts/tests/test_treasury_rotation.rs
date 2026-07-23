#![cfg(test)]

extern crate std;

use quicklendx_contracts::{QuickLendXContract, QuickLendXContractClient, QuickLendXError};
use soroban_sdk::{
    testutils::{Address as _, Events},
    Address, Env, IntoVal,
};

// Helper to setup the test environment.
// This assumes a similar setup to other tests in the project.
fn setup() -> (Env, QuickLendXContractClient, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, QuickLendXContract);
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    // The main `initialize` function takes a complex `InitializationParams` struct.
    // For this test, `initialize_admin` is simpler and sufficient.
    client.initialize_admin(&admin);
    (env, client, admin)
}

#[test]
fn test_cancel_treasury_rotation_by_admin_succeeds() {
    let (env, client, admin) = setup();
    let new_treasury = Address::generate(&env);

    // Initiate a rotation. `set_treasury` requires the admin's address for auth.
    client.set_treasury(&admin, &new_treasury);

    // Verify pending rotation exists. Assumes a getter for the pending treasury.
    // NOTE: `get_pending_treasury` will need to be added to the contract interface.
    let pending = client.get_pending_treasury().unwrap();
    assert_eq!(pending.0, new_treasury);

    // Cancel the rotation as admin
    client.cancel_treasury_rotation(&admin);

    // Verify pending rotation is gone
    let pending_after_cancel = client.get_pending_treasury();
    assert!(pending_after_cancel.is_none());

    // Verify event was emitted
    let events = env.events().all();
    let last_event = events.last().unwrap();

    // This event structure uses the old `publish` format to be consistent
    // with other admin events like `emit_admin_transfer_cancelled`.
    assert_eq!(
        last_event,
        (
            client.address.clone(),
            (soroban_sdk::symbol_short!("tr_rot_cncl"), admin).into_val(&env),
            ().into_val(&env)
        )
    );
}

#[test]
#[should_panic(expected = "Error(Contract, #1858)")] // Use a unique error code from the 185x range for rotations.
fn test_cancel_treasury_rotation_fails_if_no_pending_rotation() {
    let (env, client, admin) = setup();

    // Action: Attempt to cancel a rotation when none is pending.
    // Expectation: Panics with the `NoPendingTreasuryRotation` contract error.
    client.cancel_treasury_rotation(&admin);
}

#[test]
#[should_panic(expected = "Error(Auth, InvalidAction)")]
fn test_cancel_treasury_rotation_fails_for_non_admin() {
    let (env, client, admin) = setup();
    let new_treasury = Address::generate(&env);
    let non_admin = Address::generate(&env);

    // Initiate a rotation as admin
    client.set_treasury(&admin, &new_treasury);

    // Action: Attempt to cancel as a non-admin.
    // Expectation: Panics with an auth error.
    client
        .with_source_account(&non_admin)
        .cancel_treasury_rotation(&non_admin);
}