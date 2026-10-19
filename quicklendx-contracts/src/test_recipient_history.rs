//! Tests for the fee-recipient (treasury) rotation history — issue #1906.
//!
//! `FeeManager::{initiate,confirm,cancel}_treasury_rotation` implement a
//! two-step, timelocked rotation of the platform's fee-recipient address
//! (see `docs/FEE_RECIPIENT_ROTATION.md`). `TreasuryRotationInitiated` and
//! `TreasuryRotationConfirmed` (`events.rs`) are the "recipient history
//! view": the append-only event trail an off-chain indexer replays to answer
//! "who has held the treasury role, and when."
//!
//! These tests lock in that every rotation outcome is reflected correctly —
//! including the sequence across *multiple* rotations, which is the boundary
//! this issue targets — and pin the current behavior at the points where the
//! emitted history is demonstrably incomplete (the `*_emits_no_history_event`
//! tests). Those are characterization tests, not endorsements: they document
//! a real gap (a cancelled or expired rotation, and the very first rotation
//! when no treasury was previously configured, leave no event trace) so a
//! future change to that behavior shows up as a deliberate, reviewed diff
//! instead of a silent regression.
//!
//! # Wiring note
//! These functions are exercised directly against `FeeManager`, not through
//! `QuickLendXContractClient`: as of this writing they are not wired into
//! any `#[contractimpl]` entrypoint in `lib.rs` (the reachable treasury
//! entrypoints are `configure_treasury` / `set_treasury`, both immediate
//! single-step writes that do not touch this rotation flow at all). Two
//! pre-existing test files for this flow, `test_treasury_rotation.rs` and
//! `test_treasury_rotation_deadline.rs`, call the never-implemented
//! `QuickLendXContractClient::initiate_treasury_rotation` and are not
//! declared as `mod`s anywhere, so they are not part of the compiled test
//! tree. Re-wiring the entrypoints (or deciding not to) is a separate,
//! larger decision left to a maintainer; this file covers the logic as it
//! exists today.

#![cfg(test)]

use super::*;
use crate::errors::QuickLendXError;
use crate::events::{TOPIC_TREASURY_ROTATION_CONFIRMED, TOPIC_TREASURY_ROTATION_INITIATED};
use crate::fees::{FeeManager, MIN_ROTATION_DELAY_SECONDS};
use soroban_sdk::{
    testutils::{Address as _, Events, Ledger},
    xdr, Address, Env, Map, Symbol, TryFromVal, Val,
};

fn setup(env: &Env) -> (Address, Address) {
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    client.initialize_admin(&admin);
    client.initialize_fee_system(&admin);
    (contract_id, admin)
}

fn count_events_with_topic(env: &Env, topic_str: &str) -> usize {
    let topic_sym = Symbol::new(env, topic_str);
    let topic_xdr = xdr::ScVal::try_from_val(env, &topic_sym).expect("topic to ScVal");
    env.events()
        .all()
        .events()
        .iter()
        .filter(|e| match &e.body {
            xdr::ContractEventBody::V0(body) => body.topics.first() == Some(&topic_xdr),
        })
        .count()
}

/// Data maps for every event matching `topic_str`, oldest first.
fn events_with_topic_data(env: &Env, topic_str: &str) -> soroban_sdk::Vec<Map<Symbol, Val>> {
    let topic_sym = Symbol::new(env, topic_str);
    let topic_xdr = xdr::ScVal::try_from_val(env, &topic_sym).expect("topic to ScVal");
    let mut out = soroban_sdk::Vec::new(env);
    for e in env.events().all().events().iter() {
        if let xdr::ContractEventBody::V0(body) = &e.body {
            if body.topics.first() == Some(&topic_xdr) {
                let data_val = Val::try_from_val(env, &body.data).expect("data ScVal to Val");
                out.push_back(
                    Map::<Symbol, Val>::try_from_val(env, &data_val)
                        .expect("event data is not a Map<Symbol, Val>"),
                );
            }
        }
    }
    out
}

fn latest_event_data(env: &Env, topic_str: &str) -> Map<Symbol, Val> {
    let all = events_with_topic_data(env, topic_str);
    all.last()
        .unwrap_or_else(|| panic!("topic {:?} not found in event log", topic_str))
}

fn get_field<T: TryFromVal<Env, Val>>(env: &Env, map: &Map<Symbol, Val>, field: &str) -> T {
    let key = Symbol::new(env, field);
    let val = map
        .get(key)
        .unwrap_or_else(|| panic!("field '{}' not found in event data", field));
    T::try_from_val(env, &val).unwrap_or_else(|_| panic!("failed to decode field '{}'", field))
}

fn advance_to(env: &Env, timestamp: u64) {
    env.ledger().set_timestamp(timestamp);
}

// ============================================================================
// Initiation
// ============================================================================

#[test]
fn initiate_rotation_stores_pending_request_and_emits_initiated_event() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, admin) = setup(&env);
    let new_treasury = Address::generate(&env);

    env.as_contract(&contract_id, || {
        let request =
            FeeManager::initiate_treasury_rotation(&env, &admin, new_treasury.clone()).unwrap();
        assert_eq!(request.new_address, new_treasury);
        assert_eq!(request.initiated_by, admin);
        assert!(request.confirmation_deadline > request.initiated_at);

        let pending = FeeManager::get_pending_rotation(&env).unwrap();
        assert_eq!(pending.new_address, new_treasury);
    });

    assert_eq!(
        count_events_with_topic(&env, TOPIC_TREASURY_ROTATION_INITIATED),
        1
    );
    let data = latest_event_data(&env, TOPIC_TREASURY_ROTATION_INITIATED);
    let recorded_new: Address = get_field(&env, &data, "new_address");
    assert_eq!(recorded_new, new_treasury);
}

#[test]
fn initiate_rotation_while_pending_returns_already_pending() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, admin) = setup(&env);
    let first = Address::generate(&env);
    let second = Address::generate(&env);

    env.as_contract(&contract_id, || {
        FeeManager::initiate_treasury_rotation(&env, &admin, first).unwrap();
        let err = FeeManager::initiate_treasury_rotation(&env, &admin, second).unwrap_err();
        assert_eq!(err, QuickLendXError::RotationAlreadyPending);
    });
}

#[test]
fn initiate_rotation_to_current_treasury_address_returns_invalid_address() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, admin) = setup(&env);
    let treasury = Address::generate(&env);

    env.as_contract(&contract_id, || {
        FeeManager::configure_treasury(&env, &admin, treasury.clone()).unwrap();
        let err = FeeManager::initiate_treasury_rotation(&env, &admin, treasury).unwrap_err();
        assert_eq!(err, QuickLendXError::InvalidAddress);
    });
}

// ============================================================================
// Confirmation
// ============================================================================

#[test]
fn confirm_rotation_with_no_pending_request_returns_not_found() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, _admin) = setup(&env);
    let someone = Address::generate(&env);

    env.as_contract(&contract_id, || {
        let err = FeeManager::confirm_treasury_rotation(&env, &someone).unwrap_err();
        assert_eq!(err, QuickLendXError::RotationNotFound);
    });
}

#[test]
fn confirm_rotation_by_wrong_address_returns_unauthorized() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, admin) = setup(&env);
    let new_treasury = Address::generate(&env);
    let impostor = Address::generate(&env);

    env.as_contract(&contract_id, || {
        FeeManager::initiate_treasury_rotation(&env, &admin, new_treasury).unwrap();
        let err = FeeManager::confirm_treasury_rotation(&env, &impostor).unwrap_err();
        assert_eq!(err, QuickLendXError::Unauthorized);
        // Rejecting the wrong confirmer must not consume the pending request.
        assert!(FeeManager::get_pending_rotation(&env).is_some());
    });
}

#[test]
fn confirm_rotation_before_min_delay_returns_timelock_not_elapsed() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, admin) = setup(&env);
    let new_treasury = Address::generate(&env);

    let deadline = env.as_contract(&contract_id, || {
        FeeManager::initiate_treasury_rotation(&env, &admin, new_treasury.clone())
            .unwrap()
            .initiated_at
            + MIN_ROTATION_DELAY_SECONDS
    });

    advance_to(&env, deadline - 1);

    env.as_contract(&contract_id, || {
        let err = FeeManager::confirm_treasury_rotation(&env, &new_treasury).unwrap_err();
        assert_eq!(err, QuickLendXError::RotationTimelockNotElapsed);
        assert!(FeeManager::get_pending_rotation(&env).is_some());
    });
}

#[test]
fn confirm_rotation_exactly_at_min_delay_boundary_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, admin) = setup(&env);
    let new_treasury = Address::generate(&env);

    let deadline = env.as_contract(&contract_id, || {
        FeeManager::configure_treasury(&env, &admin, Address::generate(&env)).unwrap();
        FeeManager::initiate_treasury_rotation(&env, &admin, new_treasury.clone())
            .unwrap()
            .initiated_at
            + MIN_ROTATION_DELAY_SECONDS
    });

    advance_to(&env, deadline);

    env.as_contract(&contract_id, || {
        // Exactly at the delay boundary: `now < deadline` is false, so this
        // must succeed (the guard is strictly-less-than, not less-or-equal).
        FeeManager::confirm_treasury_rotation(&env, &new_treasury).unwrap();
    });
}

#[test]
fn confirm_rotation_after_min_delay_updates_treasury_and_emits_confirmed_event() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, admin) = setup(&env);
    let first_treasury = Address::generate(&env);
    let second_treasury = Address::generate(&env);

    let ready_at = env.as_contract(&contract_id, || {
        FeeManager::configure_treasury(&env, &admin, first_treasury.clone()).unwrap();
        FeeManager::initiate_treasury_rotation(&env, &admin, second_treasury.clone())
            .unwrap()
            .initiated_at
            + MIN_ROTATION_DELAY_SECONDS
    });
    advance_to(&env, ready_at);

    env.as_contract(&contract_id, || {
        let confirmed =
            FeeManager::confirm_treasury_rotation(&env, &second_treasury).unwrap();
        assert_eq!(confirmed, second_treasury);
        assert_eq!(
            FeeManager::get_treasury_address(&env),
            Some(second_treasury.clone())
        );
        assert!(FeeManager::get_pending_rotation(&env).is_none());
    });

    assert_eq!(
        count_events_with_topic(&env, TOPIC_TREASURY_ROTATION_CONFIRMED),
        1
    );
    let data = latest_event_data(&env, TOPIC_TREASURY_ROTATION_CONFIRMED);
    let old: Address = get_field(&env, &data, "old_address");
    let new: Address = get_field(&env, &data, "new_address");
    assert_eq!(old, first_treasury);
    assert_eq!(new, second_treasury);
}

#[test]
fn confirm_rotation_exactly_at_ttl_deadline_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, admin) = setup(&env);
    let new_treasury = Address::generate(&env);

    let confirmation_deadline = env.as_contract(&contract_id, || {
        FeeManager::configure_treasury(&env, &admin, Address::generate(&env)).unwrap();
        FeeManager::initiate_treasury_rotation(&env, &admin, new_treasury.clone())
            .unwrap()
            .confirmation_deadline
    });

    advance_to(&env, confirmation_deadline);

    env.as_contract(&contract_id, || {
        // `now > confirmation_deadline` is the expiry guard, so exactly at
        // the deadline must still succeed.
        FeeManager::confirm_treasury_rotation(&env, &new_treasury).unwrap();
    });
}

#[test]
fn confirm_rotation_after_ttl_deadline_returns_expired_and_clears_pending() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, admin) = setup(&env);
    let new_treasury = Address::generate(&env);

    let confirmation_deadline = env.as_contract(&contract_id, || {
        FeeManager::initiate_treasury_rotation(&env, &admin, new_treasury.clone())
            .unwrap()
            .confirmation_deadline
    });

    advance_to(&env, confirmation_deadline + 1);

    env.as_contract(&contract_id, || {
        let err = FeeManager::confirm_treasury_rotation(&env, &new_treasury).unwrap_err();
        assert_eq!(err, QuickLendXError::RotationExpired);
        // The expired request must not linger — a fresh rotation can start.
        assert!(FeeManager::get_pending_rotation(&env).is_none());
        assert!(FeeManager::get_treasury_address(&env).is_none());
    });
}

// ============================================================================
// Cancellation
// ============================================================================

#[test]
fn cancel_rotation_with_no_pending_request_returns_not_found() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, admin) = setup(&env);

    env.as_contract(&contract_id, || {
        let err = FeeManager::cancel_treasury_rotation(&env, &admin).unwrap_err();
        assert_eq!(err, QuickLendXError::RotationNotFound);
    });
}

#[test]
fn cancel_rotation_clears_pending_request_without_changing_treasury() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, admin) = setup(&env);
    let original = Address::generate(&env);
    let proposed = Address::generate(&env);

    env.as_contract(&contract_id, || {
        FeeManager::configure_treasury(&env, &admin, original.clone()).unwrap();
        FeeManager::initiate_treasury_rotation(&env, &admin, proposed).unwrap();

        FeeManager::cancel_treasury_rotation(&env, &admin).unwrap();

        assert!(FeeManager::get_pending_rotation(&env).is_none());
        assert_eq!(FeeManager::get_treasury_address(&env), Some(original));
    });
}

#[test]
fn cancel_rotation_allows_a_new_rotation_to_be_initiated() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, admin) = setup(&env);
    let abandoned = Address::generate(&env);
    let replacement = Address::generate(&env);

    env.as_contract(&contract_id, || {
        FeeManager::initiate_treasury_rotation(&env, &admin, abandoned).unwrap();
        FeeManager::cancel_treasury_rotation(&env, &admin).unwrap();

        // The slot freed by cancellation must accept a brand-new proposal.
        let request =
            FeeManager::initiate_treasury_rotation(&env, &admin, replacement.clone()).unwrap();
        assert_eq!(request.new_address, replacement);
    });
}

// ============================================================================
// "Covers every rotation" — the completeness boundary this issue targets
// ============================================================================

/// The recipient history view must reflect *every* rotation a treasury goes
/// through, in order — not just the first. This chains two full rotations
/// (`a -> b -> c`) and checks the event log carries both, correctly ordered
/// and with the right `old_address`/`new_address` on each.
#[test]
fn sequential_rotations_are_each_reflected_in_the_history_view() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, admin) = setup(&env);
    let addr_a = Address::generate(&env);
    let addr_b = Address::generate(&env);
    let addr_c = Address::generate(&env);

    // Rotation 1: a -> b
    let ready_1 = env.as_contract(&contract_id, || {
        FeeManager::configure_treasury(&env, &admin, addr_a.clone()).unwrap();
        FeeManager::initiate_treasury_rotation(&env, &admin, addr_b.clone())
            .unwrap()
            .initiated_at
            + MIN_ROTATION_DELAY_SECONDS
    });
    advance_to(&env, ready_1);
    env.as_contract(&contract_id, || {
        FeeManager::confirm_treasury_rotation(&env, &addr_b).unwrap();
        assert_eq!(FeeManager::get_treasury_address(&env), Some(addr_b.clone()));
    });

    // Rotation 2: b -> c
    let ready_2 = env.as_contract(&contract_id, || {
        FeeManager::initiate_treasury_rotation(&env, &admin, addr_c.clone())
            .unwrap()
            .initiated_at
            + MIN_ROTATION_DELAY_SECONDS
    });
    advance_to(&env, ready_2);
    env.as_contract(&contract_id, || {
        FeeManager::confirm_treasury_rotation(&env, &addr_c).unwrap();
        assert_eq!(FeeManager::get_treasury_address(&env), Some(addr_c.clone()));
    });

    assert_eq!(
        count_events_with_topic(&env, TOPIC_TREASURY_ROTATION_INITIATED),
        2,
        "both initiations must appear in the history view"
    );
    assert_eq!(
        count_events_with_topic(&env, TOPIC_TREASURY_ROTATION_CONFIRMED),
        2,
        "both confirmations must appear in the history view"
    );

    let confirmed = events_with_topic_data(&env, TOPIC_TREASURY_ROTATION_CONFIRMED);
    assert_eq!(confirmed.len(), 2);

    let first_old: Address = get_field(&env, &confirmed.get(0).unwrap(), "old_address");
    let first_new: Address = get_field(&env, &confirmed.get(0).unwrap(), "new_address");
    let second_old: Address = get_field(&env, &confirmed.get(1).unwrap(), "old_address");
    let second_new: Address = get_field(&env, &confirmed.get(1).unwrap(), "new_address");

    assert_eq!(first_old, addr_a);
    assert_eq!(first_new, addr_b);
    assert_eq!(second_old, addr_b);
    assert_eq!(second_new, addr_c);
    // The chain must be unbroken: rotation 2's prior recipient is exactly
    // rotation 1's new recipient.
    assert_eq!(first_new, second_old);
}

// ============================================================================
// Characterization tests: where the history view is currently incomplete
//
// These pin real, verified gaps rather than desired behavior. They exist so
// that closing (or intentionally keeping) either gap is a reviewed decision,
// not a silent side effect of an unrelated change. See the module doc above.
// ============================================================================

#[test]
fn confirm_first_ever_rotation_emits_no_confirmed_event() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, admin) = setup(&env);
    let new_treasury = Address::generate(&env);

    // No `configure_treasury` call: the platform fee config has
    // `treasury_address: None` at this point, so `confirm_treasury_rotation`
    // takes the `if let Some(old) = old_treasury` branch to `None` and never
    // calls `emit_treasury_rotation_confirmed`.
    let ready_at = env.as_contract(&contract_id, || {
        assert!(FeeManager::get_treasury_address(&env).is_none());
        FeeManager::initiate_treasury_rotation(&env, &admin, new_treasury.clone())
            .unwrap()
            .initiated_at
            + MIN_ROTATION_DELAY_SECONDS
    });
    advance_to(&env, ready_at);

    env.as_contract(&contract_id, || {
        FeeManager::confirm_treasury_rotation(&env, &new_treasury).unwrap();
        // The treasury address change is real and durable...
        assert_eq!(
            FeeManager::get_treasury_address(&env),
            Some(new_treasury.clone())
        );
    });

    // ...but it is invisible to anything that only watches the Confirmed
    // topic for the recipient history.
    assert_eq!(
        count_events_with_topic(&env, TOPIC_TREASURY_ROTATION_CONFIRMED),
        0
    );
    // The Initiated event from step one is still there, though.
    assert_eq!(
        count_events_with_topic(&env, TOPIC_TREASURY_ROTATION_INITIATED),
        1
    );
}

#[test]
fn cancel_rotation_emits_no_history_event() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, admin) = setup(&env);
    let proposed = Address::generate(&env);

    env.as_contract(&contract_id, || {
        FeeManager::initiate_treasury_rotation(&env, &admin, proposed).unwrap();
    });
    let events_before_cancel = env.events().all().events().len();

    env.as_contract(&contract_id, || {
        FeeManager::cancel_treasury_rotation(&env, &admin).unwrap();
    });

    // `cancel_treasury_rotation` neither uses `TOPIC_TREASURY_ROTATION_CANCELLED`
    // nor publishes anything else — the event count is unchanged by the call.
    // A reader who only replays TreasuryRotation* events cannot distinguish
    // "this proposed rotation was withdrawn" from "it is still pending".
    assert_eq!(env.events().all().events().len(), events_before_cancel);
}

#[test]
fn expired_rotation_emits_no_history_event_marking_the_expiry() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, admin) = setup(&env);
    let proposed = Address::generate(&env);

    let confirmation_deadline = env.as_contract(&contract_id, || {
        FeeManager::initiate_treasury_rotation(&env, &admin, proposed.clone())
            .unwrap()
            .confirmation_deadline
    });
    let events_before_expiry = env.events().all().events().len();

    advance_to(&env, confirmation_deadline + 1);
    env.as_contract(&contract_id, || {
        let err = FeeManager::confirm_treasury_rotation(&env, &proposed).unwrap_err();
        assert_eq!(err, QuickLendXError::RotationExpired);
    });

    // Same gap as cancellation: an expired proposal disappears from the
    // pending view with no corresponding entry in the event history.
    assert_eq!(env.events().all().events().len(), events_before_expiry);
}
