//! Symmetric pause / maintenance state-change tests (both directions).
//!
//! # Context
//! Previous coverage was heavily weighted toward the *unpaused → paused*
//! direction.  This module locks in the symmetric edge cases:
//!
//! ## Pause toggle symmetry
//! 1. `unpause_when_already_unpaused_is_a_no_op` — idempotent in the *off*
//!    direction; no spurious `unpaused` event is emitted.
//! 2. `pause_auto_expires_after_max_pause_duration` — advancing the ledger
//!    past `paused_at + MAX_PAUSE_DURATION` automatically clears the flag
//!    without an explicit admin call.
//! 3. `writes_unblocked_after_auto_expiry` — after auto-expiry the contract
//!    accepts mutations again.
//! 4. `non_admin_cannot_unpause` — the unpause path requires admin auth just
//!    like the pause path.
//! 5. `pause_then_unpause_emits_exactly_one_event_each` — full round-trip
//!    emits exactly one `paused` event and one `unpaused` event.
//!
//! ## Maintenance toggle symmetry
//! 6. `disable_maintenance_when_already_disabled_is_a_no_op` — symmetric to
//!    the existing *enable-when-already-enabled* test.
//! 7. `maintenance_disable_then_enable_writes_unblocked_and_blocked` — full
//!    round-trip through both directions: off → on (blocked), on → off
//!    (unblocked), off → on again (blocked again).
//! 8. `non_admin_cannot_disable_maintenance` — the disable path requires admin
//!    auth just like the enable path.
//!
//! All tests use `#[cfg(test)]` with no feature gate so they run on every CI
//! matrix entry regardless of `legacy-tests` or `fuzz-tests` flags.

#![cfg(test)]

use crate::errors::QuickLendXError;
use crate::invoice::InvoiceCategory;
use crate::pause::MAX_PAUSE_DURATION;
use crate::{QuickLendXContract, QuickLendXContractClient};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    xdr, Address, Env, String, Symbol, Vec,
};

// ============================================================================
// Helpers
// ============================================================================

/// Minimal setup: one contract instance, one admin.
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

/// Count events whose first topic matches `topic_name`.
fn count_events_with_topic(env: &Env, topic_name: &str) -> usize {
    let sym = Symbol::new(env, topic_name);
    let sym_xdr = xdr::ScVal::try_from_val(env, &sym).expect("topic to ScVal");
    env.events()
        .all()
        .events()
        .iter()
        .filter(|e| match &e.body {
            xdr::ContractEventBody::V0(body) => body.topics.first() == Some(&sym_xdr),
        })
        .count()
}

// ============================================================================
// Pause toggle — both directions
// ============================================================================

/// Calling `unpause` when the contract is already unpaused must be a no-op:
/// the flag stays `false` and no `unpaused` event is emitted.
#[test]
fn unpause_when_already_unpaused_is_a_no_op() {
    let env = Env::default();
    let (client, admin) = setup(&env);

    assert!(!client.is_paused(), "precondition: starts unpaused");

    // Unpause while already unpaused — must not panic or emit an event.
    client.unpause(&admin);

    assert!(!client.is_paused(), "still unpaused after no-op unpause");
    let event_count = count_events_with_topic(&env, "unpaused");
    assert_eq!(
        event_count, 0,
        "no unpaused event emitted when already unpaused"
    );
}

/// The pause flag is automatically cleared once the ledger timestamp advances
/// past `paused_at + MAX_PAUSE_DURATION`, without any admin action.
#[test]
fn pause_auto_expires_after_max_pause_duration() {
    let env = Env::default();
    let (client, admin) = setup(&env);

    client.pause(&admin);
    assert!(client.is_paused(), "paused after explicit pause");

    // Advance the clock to exactly the expiry boundary — still paused at N.
    let paused_at = env.ledger().timestamp();
    env.ledger().set_timestamp(paused_at + MAX_PAUSE_DURATION);
    assert!(
        client.is_paused(),
        "still paused at exactly the expiry boundary (inclusive)"
    );

    // One second past the boundary — auto-expires.
    env.ledger().set_timestamp(paused_at + MAX_PAUSE_DURATION + 1);
    assert!(
        !client.is_paused(),
        "auto-expires one second past MAX_PAUSE_DURATION"
    );
}

/// After auto-expiry, mutating entrypoints should succeed again — the auto-clear
/// in `is_paused` is not just a read signal, it lifts the real gate.
#[test]
fn writes_unblocked_after_auto_expiry() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let business = Address::generate(&env);
    let currency = Address::generate(&env);

    client.pause(&admin);

    // Blocked while paused.
    let blocked = client.try_store_invoice(
        &business,
        &1_000i128,
        &currency,
        &(env.ledger().timestamp() + 86_400),
        &String::from_str(&env, "Blocked invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    assert_eq!(
        blocked.unwrap_err().unwrap(),
        QuickLendXError::ContractPaused,
        "store_invoice must be blocked while paused"
    );

    // Advance past the duration — auto-expiry occurs.
    let paused_at = env.ledger().timestamp();
    env.ledger().set_timestamp(paused_at + MAX_PAUSE_DURATION + 1);
    assert!(!client.is_paused(), "auto-expired");

    // Same call now succeeds.
    let invoice_id = client.store_invoice(
        &business,
        &1_000i128,
        &currency,
        &(env.ledger().timestamp() + 86_400),
        &String::from_str(&env, "Unblocked after expiry"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(
        invoice.amount, 1_000i128,
        "invoice stored successfully after auto-expiry"
    );
}

/// `unpause` requires admin auth — a non-admin address must be rejected.
/// Symmetric to the existing `pause` auth test in `test_pause.rs`.
#[test]
fn non_admin_cannot_unpause() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let attacker = Address::generate(&env);

    client.pause(&admin);
    assert!(client.is_paused());

    let result = client.try_unpause(&attacker);
    assert_eq!(
        result.unwrap_err().unwrap(),
        QuickLendXError::NotAdmin,
        "non-admin must receive NotAdmin when attempting unpause"
    );
    assert!(
        client.is_paused(),
        "pause flag must remain set after rejected unpause attempt"
    );
}

/// A full pause → unpause round-trip emits exactly one `paused` event and
/// exactly one `unpaused` event, no duplicates.
#[test]
fn pause_then_unpause_emits_exactly_one_event_each() {
    let env = Env::default();
    let (client, admin) = setup(&env);

    client.pause(&admin);
    client.unpause(&admin);

    let paused_count = count_events_with_topic(&env, "paused");
    let unpaused_count = count_events_with_topic(&env, "unpaused");

    assert_eq!(paused_count, 1, "exactly one 'paused' event in round-trip");
    assert_eq!(
        unpaused_count, 1,
        "exactly one 'unpaused' event in round-trip"
    );
}

// ============================================================================
// Maintenance toggle — both directions
// ============================================================================

/// Calling `set_maintenance_mode(false)` when maintenance is already disabled
/// must be a no-op: flag stays `false`, reason stays `None`.
#[test]
fn disable_maintenance_when_already_disabled_is_a_no_op() {
    let env = Env::default();
    let (client, admin) = setup(&env);

    assert!(
        !client.is_maintenance_mode(),
        "precondition: maintenance is off"
    );
    assert!(
        client.get_maintenance_reason().is_none(),
        "no reason stored initially"
    );

    client.set_maintenance_mode(&admin, &false, &reason(&env, ""));

    assert!(!client.is_maintenance_mode(), "still disabled");
    assert!(
        client.get_maintenance_reason().is_none(),
        "reason still absent"
    );
}

/// Writes are blocked during maintenance, unblocked after disable, and blocked
/// again after re-enable — full toggle cycle in both directions.
#[test]
fn maintenance_disable_then_enable_writes_unblocked_and_blocked() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86_400;

    // ── Direction 1: off → on (writes blocked) ──────────────────────────────
    client.set_maintenance_mode(&admin, &true, &reason(&env, "First window"));
    assert!(client.is_maintenance_mode());

    let blocked = client.try_store_invoice(
        &business,
        &1_000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Blocked"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    assert_eq!(
        blocked.unwrap_err().unwrap(),
        QuickLendXError::MaintenanceModeActive,
        "store_invoice must fail during maintenance"
    );

    // ── Direction 2: on → off (writes unblocked) ─────────────────────────────
    client.set_maintenance_mode(&admin, &false, &reason(&env, ""));
    assert!(!client.is_maintenance_mode());

    let invoice_id = client.store_invoice(
        &business,
        &1_000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Unblocked invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(
        invoice.amount, 1_000i128,
        "invoice stored successfully after maintenance disabled"
    );

    // ── Direction 3: off → on again (writes blocked again) ───────────────────
    client.set_maintenance_mode(&admin, &true, &reason(&env, "Second window"));
    assert!(client.is_maintenance_mode());
    assert_eq!(
        client.get_maintenance_reason().unwrap(),
        reason(&env, "Second window"),
        "reason updated on second enable"
    );

    let blocked_again = client.try_store_invoice(
        &business,
        &1_000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Blocked again"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    assert_eq!(
        blocked_again.unwrap_err().unwrap(),
        QuickLendXError::MaintenanceModeActive,
        "store_invoice must fail after maintenance re-enabled"
    );
}

/// `set_maintenance_mode(false)` requires admin auth — a non-admin address must
/// be rejected and the maintenance flag must remain set.
#[test]
fn non_admin_cannot_disable_maintenance() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let attacker = Address::generate(&env);

    client.set_maintenance_mode(&admin, &true, &reason(&env, "Legitimate maintenance"));
    assert!(client.is_maintenance_mode());

    let result = client.try_set_maintenance_mode(&attacker, &false, &reason(&env, ""));
    assert_eq!(
        result.unwrap_err().unwrap(),
        QuickLendXError::NotAdmin,
        "non-admin must receive NotAdmin when attempting maintenance disable"
    );
    assert!(
        client.is_maintenance_mode(),
        "maintenance flag must remain set after rejected disable attempt"
    );
    assert!(
        client.get_maintenance_reason().is_some(),
        "reason must remain stored after rejected disable attempt"
    );
}
