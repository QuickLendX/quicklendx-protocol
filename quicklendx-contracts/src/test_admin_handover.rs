//! # Admin Handover Validation Tests — issue #1587
//!
//! Regression suite for `AdminStorage::verify_admin_handover`, which returns a
//! typed `OperationNotAllowed` error when the proposed new admin is identical
//! to the current admin.
//!
//! ## Coverage
//!
//! | Scenario | Expected |
//! |----------|----------|
//! | proposed == current | `Err(OperationNotAllowed)` |
//! | proposed != current | `Ok(())` |
//! | called before initialisation | `Err(OperationNotAllowed)` |
//! | two-step flow: initiate uses verify_admin_handover internally | self-target rejected |
//! | transfer_admin still rejects self-target (no regression) | `Err(OperationNotAllowed)` |
//!
//! All tests are deterministic, no std calls.

#![cfg(test)]

use crate::admin::AdminStorage;
use crate::errors::QuickLendXError;
use crate::QuickLendXContract;
use soroban_sdk::{testutils::Address as _, Address, Env};

// ── Helpers ───────────────────────────────────────────────────────────────────

fn setup() -> (Env, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    (env, contract_id)
}

fn setup_with_admin() -> (Env, Address, Address) {
    let (env, contract_id) = setup();
    let admin = Address::generate(&env);
    env.as_contract(&contract_id, || {
        AdminStorage::initialize(&env, &admin).expect("admin initialization must succeed")
    });
    (env, contract_id, admin)
}

fn verify_handover(
    env: &Env,
    contract_id: &Address,
    proposed: &Address,
) -> Result<(), QuickLendXError> {
    env.as_contract(contract_id, || {
        AdminStorage::verify_admin_handover(env, proposed)
    })
}

fn transfer_admin(
    env: &Env,
    contract_id: &Address,
    current: &Address,
    proposed: &Address,
) -> Result<(), QuickLendXError> {
    env.as_contract(contract_id, || {
        AdminStorage::transfer_admin(env, current, proposed)
    })
}

// ── MODULE 1: verify_admin_handover pure validation ───────────────────────────

/// Proposed == current → typed `OperationNotAllowed` error.
#[test]
fn verify_admin_handover_returns_error_when_proposed_equals_current_admin() {
    let (env, contract_id, admin) = setup_with_admin();

    let result = verify_handover(&env, &contract_id, &admin);
    assert_eq!(
        result,
        Err(QuickLendXError::OperationNotAllowed),
        "verify_admin_handover must return OperationNotAllowed when proposed == current"
    );
}

/// Proposed is a fresh address (not current admin) → `Ok(())`.
#[test]
fn verify_admin_handover_returns_ok_when_proposed_differs_from_current_admin() {
    let (env, contract_id, _admin) = setup_with_admin();
    let new_admin = Address::generate(&env);

    let result = verify_handover(&env, &contract_id, &new_admin);
    assert_eq!(
        result,
        Ok(()),
        "verify_admin_handover must return Ok when proposed != current"
    );
}

/// Calling before the admin subsystem is initialized → `Err(OperationNotAllowed)`.
#[test]
fn verify_admin_handover_returns_error_when_not_initialized() {
    let (env, contract_id) = setup();
    let candidate = Address::generate(&env);

    let result = verify_handover(&env, &contract_id, &candidate);
    assert_eq!(
        result,
        Err(QuickLendXError::OperationNotAllowed),
        "verify_admin_handover must return OperationNotAllowed before initialization"
    );
}

/// Proposed is a third address that has nothing to do with admin → `Ok(())`.
/// Guards against future accidental broadening of the rejection predicate.
#[test]
fn verify_admin_handover_permits_any_address_that_is_not_current_admin() {
    let (env, contract_id, _admin) = setup_with_admin();

    // Generate several unrelated addresses; all must be OK.
    for _ in 0..5 {
        let candidate = Address::generate(&env);
        assert_eq!(
            verify_handover(&env, &contract_id, &candidate),
            Ok(()),
            "verify_admin_handover must accept any address that is not the current admin"
        );
    }
}

// ── MODULE 2: verify_admin_handover is idempotent (read-only) ─────────────────

/// Calling verify_admin_handover multiple times changes no state.
#[test]
fn verify_admin_handover_does_not_mutate_admin_state() {
    let (env, contract_id, admin) = setup_with_admin();
    let candidate = Address::generate(&env);

    // Call multiple times; admin must remain unchanged.
    let _ = verify_handover(&env, &contract_id, &candidate);
    let _ = verify_handover(&env, &contract_id, &candidate);
    let _ = verify_handover(&env, &contract_id, &admin); // self-target (should err)

    let current = env.as_contract(&contract_id, || AdminStorage::get_admin(&env));
    assert_eq!(
        current,
        Some(admin.clone()),
        "verify_admin_handover must not mutate admin storage"
    );

    // Transfer lock must still be clear.
    let locked = env.as_contract(&contract_id, || AdminStorage::is_transfer_locked(&env));
    assert!(
        !locked,
        "verify_admin_handover must not set the transfer lock"
    );

    // No pending admin should have been set.
    let pending = env.as_contract(&contract_id, || AdminStorage::get_pending_admin(&env));
    assert_eq!(
        pending, None,
        "verify_admin_handover must not create a pending admin"
    );
}

// ── MODULE 3: regression — transfer_admin still rejects self-target ───────────

/// `transfer_admin` must still reject self-transfer independently.
/// Ensures `verify_admin_handover` does not replace the existing guard.
#[test]
fn transfer_admin_still_rejects_self_transfer_after_handover_helper_added() {
    let (env, contract_id, admin) = setup_with_admin();

    let result = transfer_admin(&env, &contract_id, &admin, &admin);
    assert_eq!(
        result,
        Err(QuickLendXError::OperationNotAllowed),
        "transfer_admin must still return OperationNotAllowed on self-transfer"
    );

    // Admin must be unchanged after the rejected attempt.
    let current = env.as_contract(&contract_id, || AdminStorage::get_admin(&env));
    assert_eq!(current, Some(admin), "admin must not change after rejected self-transfer");
}

/// `transfer_admin` with a new address still succeeds (no regression).
#[test]
fn transfer_admin_succeeds_after_valid_handover_verification() {
    let (env, contract_id, admin) = setup_with_admin();
    let new_admin = Address::generate(&env);

    // Pre-validate — must pass.
    assert_eq!(verify_handover(&env, &contract_id, &new_admin), Ok(()));

    // Actual transfer must succeed.
    let result = transfer_admin(&env, &contract_id, &admin, &new_admin);
    assert_eq!(
        result,
        Ok(()),
        "transfer_admin must succeed when proposed != current admin"
    );

    let current = env.as_contract(&contract_id, || AdminStorage::get_admin(&env));
    assert_eq!(current, Some(new_admin));
}

// ── MODULE 4: two-step flow regression ───────────────────────────────────────

/// Initiating a two-step transfer with self as target still returns an error
/// (initiate_admin_transfer uses the same self-target guard).
#[test]
fn initiate_admin_transfer_still_rejects_self_target() {
    let (env, contract_id, admin) = setup_with_admin();

    let result = env.as_contract(&contract_id, || {
        AdminStorage::set_two_step_enabled(&env, &admin, true)?;
        AdminStorage::initiate_admin_transfer(&env, &admin, &admin)
    });
    assert_eq!(
        result,
        Err(QuickLendXError::OperationNotAllowed),
        "initiate_admin_transfer must return OperationNotAllowed for self-target"
    );
}

/// Full two-step transfer flow still works with a valid (non-self) candidate
/// even after the handover helper is present.
#[test]
fn two_step_transfer_succeeds_for_valid_candidate() {
    let (env, contract_id, admin) = setup_with_admin();
    let new_admin = Address::generate(&env);

    // verify_admin_handover accepts the candidate.
    assert_eq!(verify_handover(&env, &contract_id, &new_admin), Ok(()));

    // Enable two-step and initiate.
    env.as_contract(&contract_id, || {
        AdminStorage::set_two_step_enabled(&env, &admin, true)
            .expect("enable two-step must succeed");
        AdminStorage::initiate_admin_transfer(&env, &admin, &new_admin)
            .expect("initiate transfer must succeed");
    });

    // Accept.
    env.as_contract(&contract_id, || {
        AdminStorage::accept_admin_transfer(&env, &new_admin)
            .expect("accept transfer must succeed");
    });

    let current = env.as_contract(&contract_id, || AdminStorage::get_admin(&env));
    assert_eq!(
        current,
        Some(new_admin),
        "admin must be updated after successful two-step transfer"
    );
}
