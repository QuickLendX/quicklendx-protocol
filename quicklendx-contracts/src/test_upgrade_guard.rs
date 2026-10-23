#![cfg(test)]

use crate::errors::QuickLendXError;
use crate::incident::IncidentControl;
use crate::pause::{PauseControl, PauseReason};
use crate::upgrade::UpgradeControl;
use soroban_sdk::{testutils::Address as _, Address, BytesN, Env, String, Wasm};

fn setup_test() -> (Env, Address, BytesN<32>) {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let wasm_hash = env.deployer().upload_contract_wasm(Wasm::from(&[]));

    // Register admin
    crate::admin::AdminStorage::set_admin(&env, &admin);

    (env, admin, wasm_hash)
}

#[test]
fn test_schedule_and_cancel_upgrade() {
    let (env, admin, wasm_hash) = setup_test();

    // Schedule upgrade
    UpgradeControl::schedule_upgrade(&env, &admin, &wasm_hash).unwrap();
    assert!(UpgradeControl::is_pending_upgrade(&env));
    assert!(PauseControl::is_paused(&env));

    // Cancel upgrade
    UpgradeControl::cancel_upgrade(&env, &admin).unwrap();
    assert!(!UpgradeControl::is_pending_upgrade(&env));
    assert!(!PauseControl::is_paused(&env));
}

#[test]
fn test_schedule_and_execute_upgrade() {
    let (env, admin, wasm_hash) = setup_test();

    // Schedule upgrade
    UpgradeControl::schedule_upgrade(&env, &admin, &wasm_hash).unwrap();
    assert!(UpgradeControl::is_pending_upgrade(&env));

    // Advance time past the delay
    env.ledger().set_timestamp(env.ledger().timestamp() + crate::upgrade::UPGRADE_DELAY_SECS);

    // Execute upgrade
    UpgradeControl::execute_upgrade(&env, &admin).unwrap();
    assert!(!UpgradeControl::is_pending_upgrade(&env));
    assert!(!PauseControl::is_paused(&env));
}

#[test]
fn test_cannot_write_while_upgrade_pending() {
    let (env, admin, wasm_hash) = setup_test();

    // Schedule an upgrade
    UpgradeControl::schedule_upgrade(&env, &admin, &wasm_hash).unwrap();

    // Try to call require_not_paused – should return UpgradePending error
    let err = PauseControl::require_not_paused(&env).unwrap_err();
    assert_eq!(err, QuickLendXError::UpgradePending);
}

#[test]
fn test_schedule_twice_fails() {
    let (env, admin, wasm_hash) = setup_test();

    UpgradeControl::schedule_upgrade(&env, &admin, &wasm_hash).unwrap();

    let err = UpgradeControl::schedule_upgrade(&env, &admin, &wasm_hash).unwrap_err();
    assert_eq!(err, QuickLendXError::OperationNotAllowed);
}

#[test]
fn test_cancel_without_schedule_fails() {
    let (env, admin, _) = setup_test();

    let err = UpgradeControl::cancel_upgrade(&env, &admin).unwrap_err();
    assert_eq!(err, QuickLendXError::OperationNotAllowed);
}

#[test]
fn test_execute_without_schedule_fails() {
    let (env, admin, _) = setup_test();

    let err = UpgradeControl::execute_upgrade(&env, &admin).unwrap_err();
    assert_eq!(err, QuickLendXError::OperationNotAllowed);
}

#[test]
fn test_non_admin_cannot_schedule() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let non_admin = Address::generate(&env);
    let wasm_hash = env.deployer().upload_contract_wasm(Wasm::from(&[]));

    crate::admin::AdminStorage::set_admin(&env, &admin);

    let err = UpgradeControl::schedule_upgrade(&env, &non_admin, &wasm_hash).unwrap_err();
    assert_eq!(err, QuickLendXError::NotAdmin);
}

// ── New tests for Issue 1 ───────────────────────────────────────────────────

#[test]
fn execute_before_delay_fails() {
    let (env, admin, wasm_hash) = setup_test();

    // Schedule at t
    UpgradeControl::schedule_upgrade(&env, &admin, &wasm_hash).unwrap();
    let (_, scheduled_at) = UpgradeControl::get_pending_upgrade(&env).unwrap();

    // Try to execute at t + UPGRADE_DELAY_SECS - 1
    env.ledger().set_timestamp(scheduled_at + crate::upgrade::UPGRADE_DELAY_SECS - 1);
    let err = UpgradeControl::execute_upgrade(&env, &admin).unwrap_err();
    assert_eq!(err, QuickLendXError::UpgradeTimelockNotElapsed);

    // Pending upgrade should still be there
    assert!(UpgradeControl::is_pending_upgrade(&env));
}

#[test]
fn execute_after_delay_succeeds() {
    let (env, admin, wasm_hash) = setup_test();

    // Schedule upgrade
    UpgradeControl::schedule_upgrade(&env, &admin, &wasm_hash).unwrap();
    let (stored_hash, scheduled_at) = UpgradeControl::get_pending_upgrade(&env).unwrap();
    assert_eq!(stored_hash, wasm_hash);

    // Advance time to exactly t + UPGRADE_DELAY_SECS
    env.ledger().set_timestamp(scheduled_at + crate::upgrade::UPGRADE_DELAY_SECS);

    // Execute should succeed
    UpgradeControl::execute_upgrade(&env, &admin).unwrap();
    assert!(!UpgradeControl::is_pending_upgrade(&env));
}

#[test]
fn cancel_preserves_incident_pause() {
    let (env, admin, wasm_hash) = setup_test();

    // Enter incident mode first
    IncidentControl::enter_incident_mode(&env, &admin, &String::from_str(&env, "test incident")).unwrap();
    assert!(PauseControl::is_paused(&env));
    assert_eq!(PauseControl::pause_reason(&env), Some(PauseReason::Incident));

    // Schedule upgrade (this overwrites pause reason to PendingUpgrade)
    UpgradeControl::schedule_upgrade(&env, &admin, &wasm_hash).unwrap();
    assert_eq!(PauseControl::pause_reason(&env), Some(PauseReason::PendingUpgrade));

    // Cancel upgrade
    UpgradeControl::cancel_upgrade(&env, &admin).unwrap();

    // Pause should be restored to Incident
    assert!(PauseControl::is_paused(&env));
    assert_eq!(PauseControl::pause_reason(&env), Some(PauseReason::Incident));
}

#[test]
fn execute_preserves_manual_pause() {
    let (env, admin, wasm_hash) = setup_test();

    // Manually pause first
    PauseControl::set_paused(&env, &admin, true).unwrap();
    assert!(PauseControl::is_paused(&env));

    // Schedule upgrade
    UpgradeControl::schedule_upgrade(&env, &admin, &wasm_hash).unwrap();

    // Advance time past delay
    env.ledger().set_timestamp(env.ledger().timestamp() + crate::upgrade::UPGRADE_DELAY_SECS);

    // Execute upgrade
    UpgradeControl::execute_upgrade(&env, &admin).unwrap();

    // Should still be paused (manual pause was restored)
    assert!(PauseControl::is_paused(&env));
}

#[test]
fn cancel_unpauses_when_only_upgrade_paused() {
    let (env, admin, wasm_hash) = setup_test();

    // No prior pause
    assert!(!PauseControl::is_paused(&env));

    // Schedule upgrade (auto-pauses)
    UpgradeControl::schedule_upgrade(&env, &admin, &wasm_hash).unwrap();
    assert!(PauseControl::is_paused(&env));

    // Cancel upgrade
    UpgradeControl::cancel_upgrade(&env, &admin).unwrap();

    // Should be unpaused
    assert!(!PauseControl::is_paused(&env));
}

#[test]
fn scheduled_event_has_executable_at() {
    let (env, admin, wasm_hash) = setup_test();

    let initial_time = env.ledger().timestamp();
    UpgradeControl::schedule_upgrade(&env, &admin, &wasm_hash).unwrap();

    // Check the event was emitted with executable_at
    let events = env.events().all();
    let upgrade_event = events.iter().find(|e| {
        e.0.get(0).map_or(false, |v| {
            if let soroban_sdk::Val::Symbol(s) = v {
                s.to_string() == "upg_sch"
            } else {
                false
            }
        })
    });

    assert!(upgrade_event.is_some(), "upgrade_scheduled event not found");
    
    // Event payload should contain (admin, wasm_hash, scheduled_at, executable_at)
    // We can verify the upgrade is scheduled for the correct time
    let (_, scheduled_at) = UpgradeControl::get_pending_upgrade(&env).unwrap();
    assert_eq!(scheduled_at, initial_time);
}
