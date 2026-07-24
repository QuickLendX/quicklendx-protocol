#![cfg(test)]
#![allow(clippy::disallowed_methods)]
#![allow(deprecated)]

extern crate std;

use quicklendx_contracts::{QuickLendXContract, QuickLendXContractClient};
use soroban_sdk::{
    testutils::Address as _,
    Address, Env,
};

fn setup() -> (Env, QuickLendXContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, QuickLendXContract);
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize_admin(&admin);
    (env, client, admin)
}

#[test]
fn test_cancel_treasury_rotation_by_admin_succeeds() {
    let (_env, client, admin) = setup();
    let new_treasury = Address::generate(&_env);

    // Initiate a rotation.
    client.set_treasury(&admin, &new_treasury);

    // Verify pending rotation exists.
    let pending = client.get_pending_treasury().unwrap();
    assert_eq!(pending.0, new_treasury);

    // Cancel the rotation as admin
    client.cancel_treasury_rotation(&admin);

    // Verify pending rotation is gone
    let pending_after_cancel = client.get_pending_treasury();
    assert!(pending_after_cancel.is_none());
}

#[test]
#[should_panic(expected = "Error(Contract, #1858)")]
fn test_cancel_treasury_rotation_fails_if_no_pending_rotation() {
    let (_env, client, admin) = setup();

    // Attempt to cancel a rotation when none is pending.
    // Expects panic with NoPendingTreasuryRotation (error code 1858).
    client.cancel_treasury_rotation(&admin);
}

#[test]
#[should_panic(expected = "Error(Auth, InvalidAction)")]
fn test_cancel_treasury_rotation_fails_for_non_admin() {
    let (_env, client, admin) = setup();
    let new_treasury = Address::generate(&_env);
    let non_admin = Address::generate(&_env);

    // Initiate a rotation as admin
    client.set_treasury(&admin, &new_treasury);

    // Attempt to cancel as a non-admin.
    // Expects panic with auth error since non_admin hasn't signed.
    let _ = client
        .try_cancel_treasury_rotation(&non_admin)
        .unwrap_err();
}
