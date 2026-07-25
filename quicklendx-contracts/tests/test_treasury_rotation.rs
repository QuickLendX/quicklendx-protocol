#![cfg(test)]

extern crate std;

use quicklendx_contracts::{QuickLendXContract, QuickLendXContractClient};
use soroban_sdk::{
    testutils::{Address as _, Events},
    Address, Env,
};

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
fn test_cancel_treasury_rotation_by_admin_succeeds() {
    let (env, client, admin) = setup();
    let new_treasury = Address::generate(&env);

    // Initiate a rotation
    client.set_treasury(&admin, &new_treasury);

    // Verify pending rotation exists
    let pending = client.get_pending_treasury().unwrap();
    assert_eq!(pending.0, new_treasury);

    // Cancel the rotation as admin
    client.cancel_treasury_rotation(&admin);

    // Verify pending rotation is gone
    let pending_after_cancel = client.get_pending_treasury();
    assert!(pending_after_cancel.is_none());

    // Verify event was emitted
    let events = env.events().all();
    assert!(!events.events().is_empty());
}

#[test]
#[should_panic(expected = "Error(Contract, #1858)")]
fn test_cancel_treasury_rotation_fails_if_no_pending_rotation() {
    let (_env, client, admin) = setup();

    // Action: Attempt to cancel a rotation when none is pending.
    // Expectation: Panics with the `NoPendingTreasuryRotation` contract error (1858).
    client.cancel_treasury_rotation(&admin);
}

#[test]
#[should_panic(expected = "Error(Contract, #1103)")]
fn test_cancel_treasury_rotation_fails_for_non_admin() {
    let (env, client, admin) = setup();
    let new_treasury = Address::generate(&env);
    let non_admin = Address::generate(&env);

    // Initiate a rotation as admin
    client.set_treasury(&admin, &new_treasury);

    // Action: Attempt to cancel as a non-admin.
    // Expectation: Panics with `NotAdmin` error (1103).
    client.cancel_treasury_rotation(&non_admin);
}