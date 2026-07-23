#![cfg(test)]

use crate::errors::QuickLendXError;
use crate::maintenance::MAX_REASON_LEN;
use crate::{QuickLendXContract, QuickLendXContractClient};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env, String};

fn setup(env: &Env) -> (QuickLendXContractClient<'static>, Address) {
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    client.initialize_admin(&admin);
    (client, admin)
}

fn reason_of_len(env: &Env, len: usize) -> String {
    let s = "a".repeat(len);
    String::from_str(env, &s)
}

#[test]
fn test_enter_incident_mode_empty_reason_succeeds() {
    let env = Env::default();
    let (client, admin) = setup(&env);

    // Empty reason (0 bytes)
    let empty_reason = reason_of_len(&env, 0);
    let snapshot = client.enter_incident_mode(&admin, &empty_reason);

    assert!(snapshot.is_paused);
    assert!(snapshot.is_maintenance);
    assert_eq!(snapshot.reason, empty_reason);

    // Clean up
    client.exit_incident_mode(&admin);
}

#[test]
fn test_enter_incident_mode_at_cap_reason_succeeds() {
    let env = Env::default();
    let (client, admin) = setup(&env);

    // Exactly at cap (256 bytes)
    let cap_reason = reason_of_len(&env, MAX_REASON_LEN as usize);
    let snapshot = client.enter_incident_mode(&admin, &cap_reason);

    assert!(snapshot.is_paused);
    assert!(snapshot.is_maintenance);
    assert_eq!(snapshot.reason, cap_reason);

    // Clean up
    client.exit_incident_mode(&admin);
}

#[test]
fn test_enter_incident_mode_over_cap_reason_fails() {
    let env = Env::default();
    let (client, admin) = setup(&env);

    // Over cap (257 bytes)
    let over_cap_reason = reason_of_len(&env, (MAX_REASON_LEN + 1) as usize);
    let result = client.try_enter_incident_mode(&admin, &over_cap_reason);

    assert_eq!(
        result.unwrap_err().unwrap(),
        QuickLendXError::InvalidDescription
    );
    assert!(!client.is_paused());
    assert!(!client.is_maintenance_mode());
}
