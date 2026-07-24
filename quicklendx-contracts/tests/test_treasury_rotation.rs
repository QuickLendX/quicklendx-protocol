#![cfg(test)]

extern crate std;

use quicklendx_contracts::errors::QuickLendXError;
use quicklendx_contracts::{QuickLendXContract, QuickLendXContractClient};
use soroban_sdk::{
    testutils::{Address as _, Events},
    Address, Env, FromVal, IntoVal,
};

// Helper to setup the test environment.
// This assumes a similar setup to other tests in the project.
fn setup() -> (Env, QuickLendXContractClient<'static>, Address) {
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
    let last_event = events.events().last().unwrap();

    use soroban_sdk::xdr;
    if let xdr::ContractEventBody::V0(body) = &last_event.body {
        let topic_sym = soroban_sdk::Symbol::from_val(&env, body.topics.first().unwrap());
        assert_eq!(topic_sym, soroban_sdk::symbol_short!("tr_rot_cn"));
    } else {
        panic!("unexpected event body");
    }
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
#[should_panic(expected = "Error(Contract, #1103)")]
fn test_cancel_treasury_rotation_fails_for_non_admin() {
    let (env, client, admin) = setup();
    let new_treasury = Address::generate(&env);
    let non_admin = Address::generate(&env);

    // Initiate a rotation as admin
    client.set_treasury(&admin, &new_treasury);

    // Action: Attempt to cancel as a non-admin.
    // Expectation: Panics with NotAdmin (1103) error since mock_all_auths is on but they aren't admin.
    client.cancel_treasury_rotation(&non_admin);
}
