//! Tests for `get_health_status` operational aggregation (#1310).

#![cfg(test)]

use crate::invoice::InvoiceCategory;
use crate::monitor::derive_writes_allowed;
use crate::{QuickLendXContract, QuickLendXContractClient};
use soroban_sdk::{testutils::Address as _, Address, Env, String, Vec};

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

#[test]
fn test_health_status_writes_allowed_when_all_clear() {
    let env = Env::default();
    let (client, _admin) = setup(&env);

    let status = client.get_health_status();

    assert!(!status.is_paused);
    assert!(!status.is_maintenance);
    assert!(status.maintenance_reason.is_none());
    assert!(!status.backpressure_active);
    assert_eq!(status.index_lag_seconds, 0);
    assert!(!status.data_is_stale);
    assert!(status.writes_allowed);
}

#[test]
fn test_health_status_paused_but_not_maintenance() {
    let env = Env::default();
    let (client, admin) = setup(&env);

    client.pause(&admin);
    let status = client.get_health_status();

    assert!(status.is_paused);
    assert!(!status.is_maintenance);
    assert!(status.maintenance_reason.is_none());
    assert!(!status.backpressure_active);
    assert!(!status.writes_allowed);
}

#[test]
fn test_health_status_maintenance_with_reason() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let msg = "Scheduled upgrade";

    client.set_maintenance_mode(&admin, &true, &reason(&env, msg));
    let status = client.get_health_status();

    assert!(!status.is_paused);
    assert!(status.is_maintenance);
    assert_eq!(status.maintenance_reason, Some(reason(&env, msg)));
    assert!(status.backpressure_active);
    assert!(!status.writes_allowed);
}

#[test]
fn test_health_status_backpressure_active_blocks_writes() {
    let env = Env::default();
    let (client, admin) = setup(&env);

    client.set_maintenance_mode(&admin, &true, &reason(&env, "Load shedding active"));
    let status = client.get_health_status();

    assert!(status.backpressure_active);
    assert!(!status.writes_allowed);
}

#[test]
fn test_health_status_available_while_paused() {
    let env = Env::default();
    let (client, admin) = setup(&env);

    client.pause(&admin);
    let status = client.get_health_status();

    assert!(status.is_paused);
    assert!(client.is_paused());
}

#[test]
fn test_health_status_recovery_after_unpause_and_maintenance_lift() {
    let env = Env::default();
    let (client, admin) = setup(&env);

    client.pause(&admin);
    client.set_maintenance_mode(&admin, &true, &reason(&env, "Upgrade"));
    assert!(!client.get_health_status().writes_allowed);

    client.unpause(&admin);
    assert!(!client.get_health_status().writes_allowed);

    client.set_maintenance_mode(&admin, &false, &reason(&env, ""));
    let status = client.get_health_status();
    assert!(status.writes_allowed);
    assert!(!status.is_paused);
    assert!(!status.is_maintenance);
    assert!(!status.backpressure_active);
}

#[test]
fn test_health_status_freshness_at_current_ledger() {
    let env = Env::default();
    let (client, _admin) = setup(&env);

    let status = client.get_health_status();
    assert_eq!(status.index_lag_seconds, 0);
    assert!(!status.data_is_stale);
}

#[test]
fn test_health_status_store_invoice_blocked_when_paused() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86_400;

    client.pause(&admin);
    assert!(!client.get_health_status().writes_allowed);

    let result = client.try_store_invoice(
        &business,
        &1_000i128,
        &currency,
        &due_date,
        &reason(&env, "Invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    assert!(result.is_err());
}

#[test]
fn test_derive_writes_allowed_branch_coverage() {
    assert!(derive_writes_allowed(false, false, false));
    assert!(!derive_writes_allowed(true, false, false));
    assert!(!derive_writes_allowed(false, true, false));
    assert!(!derive_writes_allowed(false, false, true));
    assert!(!derive_writes_allowed(true, true, false));
    assert!(!derive_writes_allowed(true, false, true));
    assert!(!derive_writes_allowed(false, true, true));
    assert!(!derive_writes_allowed(true, true, true));
}
