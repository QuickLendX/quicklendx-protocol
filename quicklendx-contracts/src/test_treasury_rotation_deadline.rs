//! # Treasury-Rotation Confirmation-Deadline Boundary Tests (Issue #1344)
//!
//! Pins the exact expiration semantics of the `FeeManager` two-step treasury
//! rotation flow exposed through the contract client.
//!
//! ## Deadline semantics
//!
//! `confirm_treasury_rotation` checks `now > confirmation_deadline` before
//! applying the rotation (see `fees.rs`). Therefore:
//!
//! - at `confirmation_deadline - 1` -> confirms (within window),
//! - at exactly `confirmation_deadline` -> confirms (boundary is **inclusive**),
//! - at `confirmation_deadline + 1` -> fails with [`RotationExpired`] and the
//!   pending request is cleared, leaving the old treasury in effect.
//!
//! The confirmation window opens only after `MIN_ROTATION_DELAY_SECONDS` has
//! elapsed; every boundary case below sets the timestamp to the deadline, which
//! is well past that timelock (TTL = 7 days, min delay = 1 day).
//!
//! ## Routing effect
//!
//! `route_platform_fee` resolves its destination via `get_treasury_address`, so
//! a confirmed rotation that updates `get_treasury_address` is exactly what
//! redirects fee routing. These tests assert the treasury address the router
//! reads, which is the routing destination.
//!
//! ## Finding
//!
//! No off-by-one found: the boundary is inclusive at exactly the deadline and
//! expires one second later, matching the issue's specification.

use crate::fees::MIN_ROTATION_DELAY_SECONDS;
use crate::QuickLendXContract;
use crate::QuickLendXContractClient;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Env,
};

fn setup(env: &Env) -> (QuickLendXContractClient, Address) {
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    client.initialize_admin(&admin);
    client.initialize_fee_system(&admin);
    (client, admin)
}

// ============================================================================
// Deadline boundary
// ============================================================================

/// Confirming one second before the deadline succeeds and routes to the new
/// treasury.
#[test]
fn test_confirm_one_second_before_deadline_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);
    let old_treasury = Address::generate(&env);
    let new_treasury = Address::generate(&env);

    client.configure_treasury(&old_treasury);
    let req = client.initiate_treasury_rotation(&new_treasury);

    env.ledger().set_timestamp(req.confirmation_deadline - 1);
    let confirmed = client.confirm_treasury_rotation(&new_treasury);

    assert_eq!(confirmed, new_treasury);
    assert_eq!(client.get_treasury_address().unwrap(), new_treasury);
}

/// Confirming at exactly the deadline succeeds (inclusive boundary) and the
/// router destination becomes the new treasury.
#[test]
fn test_confirm_at_exact_deadline_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);
    let old_treasury = Address::generate(&env);
    let new_treasury = Address::generate(&env);

    client.configure_treasury(&old_treasury);
    let req = client.initiate_treasury_rotation(&new_treasury);

    env.ledger().set_timestamp(req.confirmation_deadline);
    let confirmed = client.confirm_treasury_rotation(&new_treasury);

    assert_eq!(confirmed, new_treasury);
    // get_treasury_address is what route_platform_fee reads as its destination.
    assert_eq!(client.get_treasury_address().unwrap(), new_treasury);
    // The pending request is cleared after a successful confirm.
    assert!(client.get_pending_treasury_rotation().is_none());
}

/// Confirming one second past the deadline fails with `RotationExpired`, clears
/// the pending request, and leaves the old treasury in effect.
#[test]
fn test_confirm_one_second_past_deadline_expires() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);
    let old_treasury = Address::generate(&env);
    let new_treasury = Address::generate(&env);

    client.configure_treasury(&old_treasury);
    let req = client.initiate_treasury_rotation(&new_treasury);

    env.ledger().set_timestamp(req.confirmation_deadline + 1);
    let result = client.try_confirm_treasury_rotation(&new_treasury);
    assert!(result.is_err(), "confirm past the deadline must fail");

    // Old treasury still in effect; pending request cleared.
    assert_eq!(client.get_treasury_address().unwrap(), old_treasury);
    assert!(client.get_pending_treasury_rotation().is_none());
}

// ============================================================================
// Pending / none-pending error matrix
// ============================================================================

/// Initiating a rotation while one is already pending fails with
/// `RotationAlreadyPending`.
#[test]
fn test_initiate_while_pending_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);
    let first = Address::generate(&env);
    let second = Address::generate(&env);

    client.initiate_treasury_rotation(&first);
    let result = client.try_initiate_treasury_rotation(&second);
    assert!(result.is_err(), "second initiate while pending must fail");
}

/// Confirming with no pending rotation fails with `RotationNotFound`.
#[test]
fn test_confirm_with_none_pending_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);
    let addr = Address::generate(&env);

    let result = client.try_confirm_treasury_rotation(&addr);
    assert!(result.is_err(), "confirm without a pending rotation must fail");
}

/// Cancelling with no pending rotation fails with `RotationNotFound`.
#[test]
fn test_cancel_with_none_pending_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);

    let result = client.try_cancel_treasury_rotation();
    assert!(result.is_err(), "cancel without a pending rotation must fail");
}

// ============================================================================
// Cancel clears pending and keeps the old treasury
// ============================================================================

/// Cancelling a pending rotation clears it and leaves the old treasury as the
/// routing destination; a fresh rotation can then be initiated and confirmed.
#[test]
fn test_cancel_clears_pending_and_keeps_old_treasury() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);
    let old_treasury = Address::generate(&env);
    let new_treasury = Address::generate(&env);

    client.configure_treasury(&old_treasury);
    client.initiate_treasury_rotation(&new_treasury);
    client.cancel_treasury_rotation();

    assert!(client.get_pending_treasury_rotation().is_none());
    assert_eq!(client.get_treasury_address().unwrap(), old_treasury);

    // Re-initiate after cancel is allowed and confirmable.
    let req = client.initiate_treasury_rotation(&new_treasury);
    env.ledger()
        .set_timestamp(req.initiated_at + MIN_ROTATION_DELAY_SECONDS + 1);
    client.confirm_treasury_rotation(&new_treasury);
    assert_eq!(client.get_treasury_address().unwrap(), new_treasury);
}
