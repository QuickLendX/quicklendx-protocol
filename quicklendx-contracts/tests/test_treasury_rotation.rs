#![cfg(test)]
#![allow(clippy::disallowed_methods)]
#![allow(deprecated)]

extern crate std;

use quicklendx_contracts::{QuickLendXContract, QuickLendXContractClient};
use quicklendx_contracts::errors::QuickLendXError;
use soroban_sdk::{
    testutils::Address as _,
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
fn test_set_treasury_succeeds() {
    let (env, client, admin) = setup();
    let new_treasury = Address::generate(&env);

    client.set_treasury(&admin, &new_treasury);
    assert_eq!(client.get_treasury(), Some(new_treasury));
}

#[test]
fn test_get_pending_treasury_returns_none() {
    let (_env, client, _admin) = setup();

    // This event structure uses the old `publish` format to be consistent
    // with other admin events like `emit_admin_transfer_cancelled`.
    assert_eq!(
        last_event,
        (
            client.address.clone(),
            (soroban_sdk::symbol_short!("tr_rot_cl"), admin).into_val(&env),
            ().into_val(&env)
        )
    );
}

#[test]
fn test_cancel_treasury_rotation_fails_if_no_pending_rotation() {
    let (_env, client, admin) = setup();

    let result = client.try_cancel_treasury_rotation(&admin);
    assert_eq!(result, Err(Ok(QuickLendXError::NoPendingTreasuryRotation)));
}

#[test]
fn test_cancel_treasury_rotation_fails_for_non_admin() {
    let (_env, client, admin) = setup();
    let new_treasury = Address::generate(&_env);
    let non_admin = Address::generate(&_env);

    client.set_treasury(&admin, &new_treasury);

    let result = client.try_cancel_treasury_rotation(&non_admin);
    assert_eq!(result, Err(Ok(QuickLendXError::NotAdmin)));
}
