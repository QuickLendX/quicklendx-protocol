#![cfg(test)]
#![allow(clippy::disallowed_methods)]
#![allow(deprecated)]

extern crate std;

use quicklendx_contracts::{QuickLendXContract, QuickLendXContractClient};
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

#[test]
fn test_cancel_treasury_rotation_by_admin_succeeds() {
    let (_env, client, admin) = setup();
    let new_treasury = Address::generate(&_env);

    // Initiate a rotation.
    client.set_treasury(&admin, &new_treasury);
    assert_eq!(client.get_treasury(), Some(new_treasury));
}

    // Verify the pending rotation was recorded.
    let pending = client.get_pending_treasury();
    assert!(pending.is_some(), "pending treasury must be set after set_treasury");
    if let Some((addr, _)) = pending {
        assert_eq!(addr, new_treasury);
    }

    // Cancel the rotation as admin.
    client.cancel_treasury_rotation(&admin);

    // Verify it was cleared.
    let pending_after = client.get_pending_treasury();
    assert!(pending_after.is_none(), "pending treasury must be cleared after cancel");
}

#[test]
#[should_panic(expected = "Error(Contract, #1858)")]
fn test_cancel_treasury_rotation_fails_if_no_pending_rotation() {
    let (_env, client, admin) = setup();
    // No rotation has been initiated; cancel must fail with NoPendingTreasuryRotation.
    client.cancel_treasury_rotation(&admin);
}
