//! Regression and behaviour tests for the panic-handler harness.
//!
//! [`crate::panic_handler`] provides a [`PanicCaught`] typed contract event
//! and a [`catch_panic`] test helper that intercepts panics and emits that
//! event.  The tests below lock in the following invariants:
//!
//! - **Happy path** — a non-panicking closure returns `Ok` and emits zero
//!   events.
//! - **Sad path** — a panicking closure returns `Err` and emits exactly one
//!   `PanicCaught` event.
//! - **Message fidelity** — the `Err` value matches the string passed to
//!   `panic!`.
//! - **Idempotency boundary** — a second successful call after a panicking
//!   call does not produce an additional event.
//! - **Direct emit** — `emit_panic_caught` publishes exactly one event.

use super::*;
use crate::panic_handler::{catch_panic, emit_panic_caught};
use soroban_sdk::{
    testutils::{Events, Ledger},
    Address, Env, String as SorobanString,
};

fn setup() -> (Env, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    (env, contract_id)
}

fn event_count(env: &Env) -> usize {
    env.events().all().events().len()
}

// ---------------------------------------------------------------------------
// Happy path
// ---------------------------------------------------------------------------

#[test]
fn returns_ok_and_emits_no_event_when_closure_does_not_panic() {
    let (env, contract_id) = setup();
    env.ledger().set_timestamp(1_000_000);

    let result: Result<u32, _> = env.as_contract(&contract_id, || catch_panic(&env, || 42u32));

    assert_eq!(result, Ok(42u32));
    assert_eq!(
        event_count(&env),
        0,
        "no PanicCaught event expected when the closure succeeds"
    );
}

// ---------------------------------------------------------------------------
// Sad paths
// ---------------------------------------------------------------------------

#[test]
fn emits_panic_caught_event_for_deliberate_panic() {
    let (env, contract_id) = setup();
    env.ledger().set_timestamp(1_000_000);

    let result: Result<(), _> = env.as_contract(&contract_id, || {
        catch_panic(&env, || panic!("deliberate test panic"))
    });

    assert!(
        result.is_err(),
        "catch_panic must return Err when the closure panics"
    );
    assert_eq!(
        event_count(&env),
        1,
        "exactly one PanicCaught event expected after a caught panic"
    );
}

#[test]
fn caught_panic_err_message_matches_panic_string() {
    let (env, contract_id) = setup();
    env.ledger().set_timestamp(1_000_000);

    let result: Result<(), _> = env.as_contract(&contract_id, || {
        catch_panic(&env, || panic!("sentinel message"))
    });

    let msg = result.expect_err("must be Err on panic");
    assert_eq!(
        msg, "sentinel message",
        "Err payload must equal the panic! argument"
    );
}

// ---------------------------------------------------------------------------
// Boundary: no additional event after a clean second call
// ---------------------------------------------------------------------------

#[test]
fn no_additional_event_emitted_for_non_panicking_call_after_panicking_call() {
    let (env, contract_id) = setup();
    env.ledger().set_timestamp(1_000_000);

    // First call panics → should yield 1 event.
    let _: Result<(), _> = env.as_contract(&contract_id, || catch_panic(&env, || panic!("first")));
    assert_eq!(event_count(&env), 1);

    // Second call succeeds → event count must remain 1.
    let result: Result<u32, _> = env.as_contract(&contract_id, || catch_panic(&env, || 99u32));
    assert_eq!(result, Ok(99u32));
    assert_eq!(
        event_count(&env),
        0,
        "a successful call must not emit an additional PanicCaught event"
    );
}

// ---------------------------------------------------------------------------
// Direct-emit unit test
// ---------------------------------------------------------------------------

#[test]
fn emit_panic_caught_publishes_exactly_one_event_into_env() {
    let (env, contract_id) = setup();
    env.ledger().set_timestamp(5_000);

    let msg = SorobanString::from_str(&env, "direct emit test");
    env.as_contract(&contract_id, || {
        emit_panic_caught(&env, msg);
    });

    assert_eq!(
        event_count(&env),
        1,
        "emit_panic_caught must publish exactly one event"
    );
}
