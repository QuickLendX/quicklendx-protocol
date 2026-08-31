//! Tests for the batch currency whitelist entrypoints:
//! `add_currencies_batch` and `remove_currencies_batch`.
//!
//! Covers empty input, all-new, all-existing, mixed, duplicate-in-input,
//! non-admin rejection, uninitialized admin, paused contract, and round-trip
//! correctness.  These tests run without feature gates so CI always executes them.

use super::*;
use crate::errors::QuickLendXError;
use soroban_sdk::{testutils::Address as _, Address, Env, Vec};

// ---------------------------------------------------------------------------
// Shared setup
// ---------------------------------------------------------------------------

fn setup() -> (Env, QuickLendXContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.set_admin(&admin);
    (env, client, admin)
}

fn make_currency(env: &Env) -> Address {
    Address::generate(env)
}

fn address_vec(env: &Env, items: &[Address]) -> Vec<Address> {
    let mut v = Vec::new(env);
    for item in items {
        v.push_back(item.clone());
    }
    v
}

// ---------------------------------------------------------------------------
// add_currencies_batch
// ---------------------------------------------------------------------------

#[test]
fn test_add_batch_empty() {
    let (env, client, admin) = setup();
    let empty: Vec<Address> = Vec::new(&env);
    let result = client.add_currencies_batch(&admin, &empty);
    assert_eq!(result.len(), 0);
    assert_eq!(client.currency_count(), 0);
}

#[test]
fn test_add_batch_all_new() {
    let (env, client, admin) = setup();
    let c1 = make_currency(&env);
    let c2 = make_currency(&env);
    let c3 = make_currency(&env);
    let batch = address_vec(&env, &[c1.clone(), c2.clone(), c3.clone()]);

    let result = client.add_currencies_batch(&admin, &batch);

    assert_eq!(result.len(), 3);
    assert!(result.get(0).unwrap());
    assert!(result.get(1).unwrap());
    assert!(result.get(2).unwrap());
    assert_eq!(client.currency_count(), 3);
    assert!(client.is_allowed_currency(&c1));
    assert!(client.is_allowed_currency(&c2));
    assert!(client.is_allowed_currency(&c3));
}

#[test]
fn test_add_batch_all_existing() {
    let (env, client, admin) = setup();
    let c1 = make_currency(&env);
    let c2 = make_currency(&env);
    client.add_currency(&admin, &c1);
    client.add_currency(&admin, &c2);

    let batch = address_vec(&env, &[c1.clone(), c2.clone()]);
    let result = client.add_currencies_batch(&admin, &batch);

    assert_eq!(result.len(), 2);
    assert!(!result.get(0).unwrap()); // already present
    assert!(!result.get(1).unwrap()); // already present
    assert_eq!(client.currency_count(), 2); // unchanged
}

#[test]
fn test_add_batch_mixed() {
    let (env, client, admin) = setup();
    let existing = make_currency(&env);
    let new_one = make_currency(&env);
    client.add_currency(&admin, &existing);

    let batch = address_vec(&env, &[existing.clone(), new_one.clone()]);
    let result = client.add_currencies_batch(&admin, &batch);

    assert_eq!(result.len(), 2);
    assert!(!result.get(0).unwrap()); // was already present
    assert!(result.get(1).unwrap()); // newly added
    assert_eq!(client.currency_count(), 2);
}

#[test]
fn test_add_batch_duplicates_in_input() {
    let (env, client, admin) = setup();
    let c = make_currency(&env);
    let batch = address_vec(&env, &[c.clone(), c.clone()]);

    let result = client.add_currencies_batch(&admin, &batch);

    assert_eq!(result.len(), 2);
    assert!(result.get(0).unwrap()); // first occurrence: added
    assert!(!result.get(1).unwrap()); // second occurrence: already in evolving list
    assert_eq!(client.currency_count(), 1); // stored exactly once
}

#[test]
fn test_add_batch_non_admin_rejected() {
    let (env, client, _admin) = setup();
    let impostor = Address::generate(&env);
    let c = make_currency(&env);
    let batch = address_vec(&env, &[c]);

    let err = client
        .try_add_currencies_batch(&impostor, &batch)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, QuickLendXError::NotAdmin);
    assert_eq!(client.currency_count(), 0); // no mutation occurred
}

#[test]
fn test_add_batch_uninitialized_admin() {
    // Fresh contract with no set_admin call
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let any_address = Address::generate(&env);
    let batch = address_vec(&env, &[make_currency(&env)]);

    let err = client
        .try_add_currencies_batch(&any_address, &batch)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, QuickLendXError::OperationNotAllowed);
}

#[test]
fn test_add_batch_paused() {
    let (env, client, admin) = setup();
    client.pause(&admin);
    let batch = address_vec(&env, &[make_currency(&env)]);

    let err = client
        .try_add_currencies_batch(&admin, &batch)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, QuickLendXError::ContractPaused);
}

#[test]
fn test_add_batch_result_length_matches_input() {
    let (env, client, admin) = setup();
    let batch = address_vec(
        &env,
        &[
            make_currency(&env),
            make_currency(&env),
            make_currency(&env),
            make_currency(&env),
        ],
    );
    let result = client.add_currencies_batch(&admin, &batch);
    assert_eq!(result.len(), batch.len());
}

// ---------------------------------------------------------------------------
// remove_currencies_batch
// ---------------------------------------------------------------------------

#[test]
fn test_remove_batch_empty() {
    let (env, client, admin) = setup();
    let c = make_currency(&env);
    client.add_currency(&admin, &c);

    let empty: Vec<Address> = Vec::new(&env);
    let result = client.remove_currencies_batch(&admin, &empty);

    assert_eq!(result.len(), 0);
    assert_eq!(client.currency_count(), 1); // unchanged
}

#[test]
fn test_remove_batch_all_present() {
    let (env, client, admin) = setup();
    let c1 = make_currency(&env);
    let c2 = make_currency(&env);
    client.add_currency(&admin, &c1);
    client.add_currency(&admin, &c2);

    let batch = address_vec(&env, &[c1.clone(), c2.clone()]);
    let result = client.remove_currencies_batch(&admin, &batch);

    assert_eq!(result.len(), 2);
    assert!(result.get(0).unwrap()); // was present
    assert!(result.get(1).unwrap()); // was present
    assert_eq!(client.currency_count(), 0);
    assert!(!client.is_allowed_currency(&c1));
    assert!(!client.is_allowed_currency(&c2));
}

#[test]
fn test_remove_batch_all_absent() {
    let (env, client, admin) = setup();
    let c1 = make_currency(&env);
    let c2 = make_currency(&env);
    // do NOT add them to the whitelist

    let batch = address_vec(&env, &[c1, c2]);
    let result = client.remove_currencies_batch(&admin, &batch);

    assert_eq!(result.len(), 2);
    assert!(!result.get(0).unwrap()); // was not present
    assert!(!result.get(1).unwrap()); // was not present
    assert_eq!(client.currency_count(), 0); // unchanged
}

#[test]
fn test_remove_batch_mixed() {
    let (env, client, admin) = setup();
    let present = make_currency(&env);
    let absent = make_currency(&env);
    client.add_currency(&admin, &present);

    let batch = address_vec(&env, &[present.clone(), absent.clone()]);
    let result = client.remove_currencies_batch(&admin, &batch);

    assert_eq!(result.len(), 2);
    assert!(result.get(0).unwrap()); // was present, removed
    assert!(!result.get(1).unwrap()); // was not present
    assert_eq!(client.currency_count(), 0);
}

#[test]
fn test_remove_batch_duplicates_in_input() {
    let (env, client, admin) = setup();
    let c = make_currency(&env);
    client.add_currency(&admin, &c);

    let batch = address_vec(&env, &[c.clone(), c.clone()]);
    let result = client.remove_currencies_batch(&admin, &batch);

    assert_eq!(result.len(), 2);
    assert!(result.get(0).unwrap()); // was present
    assert!(result.get(1).unwrap()); // also marked as was present (checked against original)
    assert_eq!(client.currency_count(), 0); // removed exactly once
}

#[test]
fn test_remove_batch_non_admin_rejected() {
    let (env, client, admin) = setup();
    let c = make_currency(&env);
    client.add_currency(&admin, &c);
    let impostor = Address::generate(&env);

    let batch = address_vec(&env, &[c]);
    let err = client
        .try_remove_currencies_batch(&impostor, &batch)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, QuickLendXError::NotAdmin);
    assert_eq!(client.currency_count(), 1); // no mutation occurred
}

#[test]
fn test_remove_batch_uninitialized_admin() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let any_address = Address::generate(&env);
    let batch = address_vec(&env, &[make_currency(&env)]);

    let err = client
        .try_remove_currencies_batch(&any_address, &batch)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, QuickLendXError::NotAdmin);
}

#[test]
fn test_remove_batch_paused() {
    let (env, client, admin) = setup();
    let c = make_currency(&env);
    client.add_currency(&admin, &c);
    client.pause(&admin);

    let batch = address_vec(&env, &[c]);
    let err = client
        .try_remove_currencies_batch(&admin, &batch)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, QuickLendXError::ContractPaused);
}

#[test]
fn test_remove_batch_result_length_matches_input() {
    let (env, client, admin) = setup();
    let batch = address_vec(
        &env,
        &[
            make_currency(&env),
            make_currency(&env),
            make_currency(&env),
        ],
    );
    let result = client.remove_currencies_batch(&admin, &batch);
    assert_eq!(result.len(), batch.len());
}

// ---------------------------------------------------------------------------
// Round-trip / integration
// ---------------------------------------------------------------------------

#[test]
fn test_roundtrip_add_then_remove_batch() {
    let (env, client, admin) = setup();
    let pre_existing = make_currency(&env);
    client.add_currency(&admin, &pre_existing);

    let c1 = make_currency(&env);
    let c2 = make_currency(&env);
    let batch = address_vec(&env, &[c1.clone(), c2.clone()]);

    client.add_currencies_batch(&admin, &batch);
    assert_eq!(client.currency_count(), 3);

    client.remove_currencies_batch(&admin, &batch);
    assert_eq!(client.currency_count(), 1);
    assert!(client.is_allowed_currency(&pre_existing));
    assert!(!client.is_allowed_currency(&c1));
    assert!(!client.is_allowed_currency(&c2));
}

#[test]
fn test_add_batch_does_not_affect_other_currencies() {
    let (env, client, admin) = setup();
    let existing = make_currency(&env);
    client.add_currency(&admin, &existing);

    let new_one = make_currency(&env);
    let batch = address_vec(&env, core::slice::from_ref(&new_one));
    client.add_currencies_batch(&admin, &batch);

    assert_eq!(client.currency_count(), 2);
    assert!(client.is_allowed_currency(&existing)); // untouched
    assert!(client.is_allowed_currency(&new_one));
}
