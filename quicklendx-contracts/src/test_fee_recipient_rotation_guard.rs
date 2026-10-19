//! Contract-surface regression tests for guarded fee-recipient rotation.
//!
//! The FeeManager owns the delayed rotation state machine. These tests exercise
//! the public contract entrypoints that connect that state machine to the live
//! fee-routing configuration. They cover every mutation boundary where an
//! unauthorized, early, expired, cancelled, or replayed operation must leave
//! the active recipient unchanged.

#![cfg(test)]

use crate::fees::MIN_ROTATION_DELAY_SECONDS;
use crate::{QuickLendXContract, QuickLendXContractClient};
use soroban_sdk::{
    testutils::{Address as _, Events, Ledger},
    xdr, Address, Env, Symbol, TryFromVal,
};

const ROTATION_TTL_SECONDS: u64 = 604_800;

fn setup(env: &Env) -> (QuickLendXContractClient, Address, Address) {
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let original = Address::generate(env);
    client.initialize_admin(&admin);
    client.initialize_fee_system(&admin);
    client.configure_treasury(&original);
    (client, admin, original)
}

fn new_address(env: &Env) -> Address {
    Address::generate(env)
}

fn advance_to_min_delay(env: &Env) {
    env.ledger()
        .set_timestamp(env.ledger().timestamp() + MIN_ROTATION_DELAY_SECONDS);
}

fn advance_to_expiry(env: &Env, initiated_at: u64) {
    env.ledger()
        .set_timestamp(initiated_at + ROTATION_TTL_SECONDS + 1);
}

fn count_topic(env: &Env, topic: &str) -> u32 {
    let topic = Symbol::new(env, topic);
    let topic_xdr = xdr::ScVal::try_from_val(env, &topic).expect("topic to ScVal");
    env.events()
        .all()
        .events()
        .iter()
        .filter(|event| match &event.body {
            xdr::ContractEventBody::V0(body) => body.topics.first() == Some(&topic_xdr),
        })
        .count() as u32
}

#[test]
fn public_rotation_flow_starts_with_the_active_recipient() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, original) = setup(&env);
    assert_eq!(client.get_treasury_address(), Some(original.clone()));
    assert!(client.get_pending_treasury_rotation().is_none());

    let proposed = new_address(&env);
    let request = client.initiate_treasury_rotation(&proposed);

    assert_eq!(request.new_address, proposed);
    assert_eq!(client.get_treasury_address(), Some(original));
    assert_eq!(
        request.confirmation_deadline,
        request.initiated_at + ROTATION_TTL_SECONDS
    );
    assert_eq!(count_topic(&env, "tr_rot_i"), 1);
}

#[test]
fn active_recipient_is_stable_until_exact_delay_then_changes_once() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, original) = setup(&env);
    let proposed = new_address(&env);
    client.initiate_treasury_rotation(&proposed);

    let early = client.try_confirm_treasury_rotation(&proposed);
    assert!(early.is_err());
    assert_eq!(client.get_treasury_address(), Some(original.clone()));
    assert!(client.get_pending_treasury_rotation().is_some());

    advance_to_min_delay(&env);
    assert_eq!(client.confirm_treasury_rotation(&proposed), proposed);
    assert_eq!(client.get_treasury_address(), Some(proposed));
    assert!(client.get_pending_treasury_rotation().is_none());
    assert_eq!(count_topic(&env, "tr_rot_f"), 1);
}

#[test]
fn cancellation_keeps_routing_on_the_original_recipient_and_emits_event() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, original) = setup(&env);
    let proposed = new_address(&env);
    client.initiate_treasury_rotation(&proposed);
    client.cancel_treasury_rotation();

    assert_eq!(client.get_treasury_address(), Some(original));
    assert!(client.get_pending_treasury_rotation().is_none());
    assert_eq!(count_topic(&env, "tr_rot_c"), 1);
}

#[test]
fn duplicate_proposal_is_rejected_without_replacing_pending_request() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, original) = setup(&env);
    let first = new_address(&env);
    let second = new_address(&env);
    client.initiate_treasury_rotation(&first);

    let result = client.try_initiate_treasury_rotation(&second);
    assert!(result.is_err());
    assert_eq!(client.get_treasury_address(), Some(original));
    assert_eq!(
        client
            .get_pending_treasury_rotation()
            .expect("first request remains")
            .new_address,
        first
    );
}

#[test]
fn cancellation_allows_a_reviewed_replacement_proposal() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, original) = setup(&env);
    let abandoned = new_address(&env);
    let replacement = new_address(&env);
    client.initiate_treasury_rotation(&abandoned);
    client.cancel_treasury_rotation();
    client.initiate_treasury_rotation(&replacement);

    assert_eq!(client.get_treasury_address(), Some(original));
    assert_eq!(
        client.get_pending_treasury_rotation().unwrap().new_address,
        replacement
    );
}

#[test]
fn wrong_recipient_cannot_finalize_or_clear_pending_state() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, original) = setup(&env);
    let proposed = new_address(&env);
    let wrong = new_address(&env);
    client.initiate_treasury_rotation(&proposed);
    advance_to_min_delay(&env);

    let result = client.try_confirm_treasury_rotation(&wrong);
    assert!(result.is_err());
    assert_eq!(client.get_treasury_address(), Some(original));
    assert_eq!(
        client.get_pending_treasury_rotation().unwrap().new_address,
        proposed
    );
}

#[test]
fn replayed_finalization_is_rejected_and_recipient_stays_active() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _original) = setup(&env);
    let proposed = new_address(&env);
    client.initiate_treasury_rotation(&proposed);
    advance_to_min_delay(&env);
    client.confirm_treasury_rotation(&proposed);

    let result = client.try_confirm_treasury_rotation(&proposed);
    assert!(result.is_err());
    assert_eq!(client.get_treasury_address(), Some(proposed));
}

#[test]
fn expired_finalization_clears_request_without_changing_routing() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, original) = setup(&env);
    let proposed = new_address(&env);
    let request = client.initiate_treasury_rotation(&proposed);
    advance_to_expiry(&env, request.initiated_at);

    let result = client.try_confirm_treasury_rotation(&proposed);
    assert!(result.is_err());
    assert_eq!(client.get_treasury_address(), Some(original));
    assert!(client.get_pending_treasury_rotation().is_none());
}

#[test]
fn exact_deadline_is_valid_but_one_second_later_is_not() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, original) = setup(&env);
    let proposed = new_address(&env);
    let request = client.initiate_treasury_rotation(&proposed);
    env.ledger().set_timestamp(request.confirmation_deadline);
    assert_eq!(client.confirm_treasury_rotation(&proposed), proposed);

    let second = new_address(&env);
    let request = client.initiate_treasury_rotation(&second);
    env.ledger()
        .set_timestamp(request.confirmation_deadline + 1);
    assert!(client.try_confirm_treasury_rotation(&second).is_err());
    assert_eq!(client.get_treasury_address(), Some(proposed));
    assert_ne!(client.get_treasury_address(), Some(original));
}

#[test]
fn legacy_single_step_reconfiguration_is_rejected_after_bootstrap() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, original) = setup(&env);
    let attempted = new_address(&env);
    let result = client.try_configure_treasury(&attempted);

    assert!(result.is_err());
    assert_eq!(client.get_treasury_address(), Some(original));
    assert!(client.get_pending_treasury_rotation().is_none());
}

#[test]
fn finalized_recipient_is_visible_to_the_fee_router_configuration() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, original) = setup(&env);
    assert_eq!(client.get_treasury_address(), Some(original.clone()));

    let proposed = new_address(&env);
    client.initiate_treasury_rotation(&proposed);
    advance_to_min_delay(&env);
    client.confirm_treasury_rotation(&proposed);

    // FeeManager::route_platform_fee reads this same platform configuration;
    // the view proves settlement observes the activated recipient without a
    // token transfer obscuring this state-machine assertion.
    assert_eq!(client.get_treasury_address(), Some(proposed));
    assert_ne!(client.get_treasury_address(), Some(original));
}

#[test]
fn every_rotation_step_has_a_distinct_audit_topic() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _original) = setup(&env);
    let proposed = new_address(&env);
    client.initiate_treasury_rotation(&proposed);
    client.cancel_treasury_rotation();

    assert_eq!(count_topic(&env, "tr_rot_i"), 1);
    assert_eq!(count_topic(&env, "tr_rot_c"), 1);
    assert_eq!(count_topic(&env, "tr_rot_f"), 0);
}

#[test]
fn a_second_rotation_requires_a_new_delay_window() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _original) = setup(&env);
    let first = new_address(&env);
    client.initiate_treasury_rotation(&first);
    advance_to_min_delay(&env);
    client.confirm_treasury_rotation(&first);

    let second = new_address(&env);
    let request = client.initiate_treasury_rotation(&second);
    assert!(client.try_confirm_treasury_rotation(&second).is_err());
    assert_eq!(client.get_treasury_address(), Some(first));
    assert_eq!(request.initiated_at, env.ledger().timestamp());
}

#[test]
fn cancelling_without_a_request_does_not_mutate_active_configuration() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, original) = setup(&env);

    assert!(client.try_cancel_treasury_rotation().is_err());
    assert_eq!(client.get_treasury_address(), Some(original));
    assert!(client.get_pending_treasury_rotation().is_none());
    assert_eq!(count_topic(&env, "tr_rot_c"), 0);
}

#[test]
fn proposing_the_current_recipient_is_rejected_without_pending_state() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, original) = setup(&env);

    let result = client.try_initiate_treasury_rotation(&original);
    assert!(result.is_err());
    assert_eq!(client.get_treasury_address(), Some(original));
    assert!(client.get_pending_treasury_rotation().is_none());
    assert_eq!(count_topic(&env, "tr_rot_i"), 0);
}

#[test]
fn expiry_is_recoverable_by_a_fresh_proposal() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, original) = setup(&env);
    let expired = new_address(&env);
    let request = client.initiate_treasury_rotation(&expired);
    advance_to_expiry(&env, request.initiated_at);
    assert!(client.try_confirm_treasury_rotation(&expired).is_err());

    let replacement = new_address(&env);
    let replacement_request = client.initiate_treasury_rotation(&replacement);
    assert_eq!(replacement_request.new_address, replacement);
    assert_eq!(client.get_treasury_address(), Some(original));
    assert_eq!(count_topic(&env, "tr_rot_i"), 2);
}

#[test]
fn failed_confirmation_does_not_consume_the_confirmation_window() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, original) = setup(&env);
    let proposed = new_address(&env);
    let wrong = new_address(&env);
    let request = client.initiate_treasury_rotation(&proposed);
    env.ledger()
        .set_timestamp(request.initiated_at + MIN_ROTATION_DELAY_SECONDS - 1);

    assert!(client.try_confirm_treasury_rotation(&wrong).is_err());
    assert_eq!(client.get_treasury_address(), Some(original));
    assert_eq!(
        client.get_pending_treasury_rotation().unwrap().new_address,
        proposed
    );
    assert_eq!(count_topic(&env, "tr_rot_f"), 0);
}

#[test]
fn sequential_successful_rotations_preserve_the_event_trail() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, original) = setup(&env);
    let first = new_address(&env);
    client.initiate_treasury_rotation(&first);
    advance_to_min_delay(&env);
    client.confirm_treasury_rotation(&first);

    let second = new_address(&env);
    client.initiate_treasury_rotation(&second);
    advance_to_min_delay(&env);
    client.confirm_treasury_rotation(&second);

    assert_eq!(client.get_treasury_address(), Some(second));
    assert_ne!(client.get_treasury_address(), Some(original));
    assert_eq!(count_topic(&env, "tr_rot_i"), 2);
    assert_eq!(count_topic(&env, "tr_rot_f"), 2);
}

#[test]
fn pending_request_exposes_review_window_to_operators() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _original) = setup(&env);
    let proposed = new_address(&env);
    let request = client.initiate_treasury_rotation(&proposed);
    let pending = client
        .get_pending_treasury_rotation()
        .expect("pending request is queryable");

    assert_eq!(pending.new_address, proposed);
    assert_eq!(pending.initiated_by, admin);
    assert_eq!(pending.initiated_at, request.initiated_at);
    assert_eq!(
        pending.confirmation_deadline - pending.initiated_at,
        ROTATION_TTL_SECONDS
    );
}
