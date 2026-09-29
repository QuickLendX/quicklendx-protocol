#![cfg(test)]

use crate::{QuickLendXContract, QuickLendXContractClient};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, BytesN, Env, String};

fn setup(env: &Env) -> (QuickLendXContractClient<'static>, Address) {
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    client.initialize_admin(&admin);
    (client, admin)
}

#[test]
fn manual_pause_exposes_reason_and_unpause_clears_it() {
    let env = Env::default();
    let (client, admin) = setup(&env);

    assert_eq!(client.pause_reason(), None);
    client.pause(&admin);
    assert_eq!(
        client.pause_reason(),
        Some(crate::pause::PauseReason::Manual)
    );
    client.unpause(&admin);
    assert_eq!(client.pause_reason(), None);
}

#[test]
fn incident_pause_exposes_reason() {
    let env = Env::default();
    let (client, admin) = setup(&env);

    client.enter_incident_mode(&admin, &String::from_str(&env, "incident"));
    assert_eq!(
        client.pause_reason(),
        Some(crate::pause::PauseReason::Incident)
    );
}

#[test]
fn pending_upgrade_pause_exposes_reason() {
    let env = Env::default();
    let (client, admin) = setup(&env);

    client.schedule_upgrade(&admin, &BytesN::from_array(&env, &[7; 32]));
    assert_eq!(
        client.pause_reason(),
        Some(crate::pause::PauseReason::PendingUpgrade)
    );
}
