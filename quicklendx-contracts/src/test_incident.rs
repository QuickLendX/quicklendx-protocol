//! Tests for coordinated incident mode (`enter_incident_mode` / `exit_incident_mode`).

#![cfg(test)]

use crate::errors::QuickLendXError;
use crate::incident::IncidentSnapshot;
use crate::invoice::InvoiceCategory;
use crate::maintenance::{MaintenanceControl, MAX_REASON_LEN};
use crate::pause::PauseControl;
use crate::{QuickLendXContract, QuickLendXContractClient};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env, String, Vec};

fn setup(env: &Env) -> (QuickLendXContractClient<'static>, Address) {
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    client.initialize_admin(&admin);
    (client, admin)
}

fn reason(env: &Env, msg: &str) -> String {
    String::from_str(env, msg)
}

fn reason_of_len(env: &Env, len: usize) -> String {
    let s = "x".repeat(len);
    String::from_str(env, &s)
}

#[test]
fn test_enter_incident_mode_sets_pause_and_maintenance() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let msg = "Oracle feed anomaly — freezing writes";

    let snapshot = client.enter_incident_mode(&admin, &reason(&env, msg));

    assert!(snapshot.is_paused);
    assert!(snapshot.is_maintenance);
    assert_eq!(snapshot.reason, reason(&env, msg));
    assert_eq!(snapshot.timestamp, env.ledger().timestamp());
    assert!(client.is_paused());
    assert!(MaintenanceControl::is_maintenance_mode(&env));
    assert_eq!(
        MaintenanceControl::get_maintenance_reason(&env).unwrap(),
        reason(&env, msg)
    );
}

#[test]
fn test_enter_incident_mode_rejects_non_admin() {
    let env = Env::default();
    let (client, _admin) = setup(&env);
    let attacker = Address::generate(&env);

    let result = client.try_enter_incident_mode(&attacker, &reason(&env, "attack"));
    assert_eq!(result.unwrap_err().unwrap(), QuickLendXError::NotAdmin);
    assert!(!client.is_paused());
    assert!(!MaintenanceControl::is_maintenance_mode(&env));
}

#[test]
fn test_enter_incident_mode_rejects_oversized_reason() {
    let env = Env::default();
    let (client, admin) = setup(&env);

    let oversized = reason_of_len(&env, (MAX_REASON_LEN + 1) as usize);

    let result = client.try_enter_incident_mode(&admin, &oversized);
    assert_eq!(result.unwrap_err().unwrap(), QuickLendXError::InvalidDescription);
    assert!(!client.is_paused());
    assert!(!MaintenanceControl::is_maintenance_mode(&env));
}

#[test]
fn test_enter_incident_mode_idempotent_reentry_updates_reason() {
    let env = Env::default();
    let (client, admin) = setup(&env);

    client.enter_incident_mode(&admin, &reason(&env, "First incident"));
    let snapshot = client.enter_incident_mode(&admin, &reason(&env, "Escalated incident"));

    assert!(snapshot.is_paused);
    assert!(snapshot.is_maintenance);
    assert_eq!(snapshot.reason, reason(&env, "Escalated incident"));
}

#[test]
fn test_exit_incident_mode_clears_both_flags() {
    let env = Env::default();
    let (client, admin) = setup(&env);

    client.enter_incident_mode(&admin, &reason(&env, "Investigating"));
    let snapshot = client.exit_incident_mode(&admin);

    assert!(!snapshot.is_paused);
    assert!(!snapshot.is_maintenance);
    assert_eq!(snapshot.reason, reason(&env, ""));
    assert!(!client.is_paused());
    assert!(!MaintenanceControl::is_maintenance_mode(&env));
    assert!(MaintenanceControl::get_maintenance_reason(&env).is_none());
}

#[test]
fn test_exit_incident_mode_idempotent_when_not_active() {
    let env = Env::default();
    let (client, admin) = setup(&env);

    assert!(!client.is_paused());
    let snapshot = client.exit_incident_mode(&admin);

    assert_eq!(
        snapshot,
        IncidentSnapshot {
            is_paused: false,
            is_maintenance: false,
            reason: reason(&env, ""),
            timestamp: env.ledger().timestamp(),
        }
    );
}

#[test]
fn test_exit_incident_mode_rejects_non_admin() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let attacker = Address::generate(&env);

    client.enter_incident_mode(&admin, &reason(&env, "Incident"));
    let result = client.try_exit_incident_mode(&attacker);
    assert_eq!(result.unwrap_err().unwrap(), QuickLendXError::NotAdmin);
    assert!(client.is_paused());
    assert!(MaintenanceControl::is_maintenance_mode(&env));
}

#[test]
fn test_incident_mode_blocks_store_invoice() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86_400;

    client.enter_incident_mode(&admin, &reason(&env, "Security review"));

    let result = client.try_store_invoice(
        &business,
        &1_000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Blocked"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    assert_eq!(result.unwrap_err().unwrap(), QuickLendXError::ContractPaused);
}

#[test]
fn test_unpause_alone_leaves_maintenance_active() {
    let env = Env::default();
    let (client, admin) = setup(&env);

    client.enter_incident_mode(&admin, &reason(&env, "Drift test"));
    client.unpause(&admin);

    assert!(!PauseControl::is_paused(&env));
    assert!(MaintenanceControl::is_maintenance_mode(&env));
}

#[test]
fn test_reenter_realigns_drifted_flags() {
    let env = Env::default();
    let (client, admin) = setup(&env);

    client.pause(&admin);
    assert!(client.is_paused());
    assert!(!MaintenanceControl::is_maintenance_mode(&env));

    let snapshot = client.enter_incident_mode(&admin, &reason(&env, "Realign"));
    assert!(snapshot.is_paused);
    assert!(snapshot.is_maintenance);
}
