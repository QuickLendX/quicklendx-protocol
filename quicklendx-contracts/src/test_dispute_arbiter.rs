//! Negative tests for the `require_dispute_arbiter` guard (Issue #1840).
//!
//! Threat model: every dispute resolution moves escrowed funds. Admin
//! authority already controls protocol configuration (fees, listings,
//! treasury, upgrade). Without a separate arbiter registry, a single
//! compromised admin key quietly authorises every dispute resolution on
//! the platform. This test pins the guard in place: even an authenticated
//! admin cannot resolve disputes without explicit arbiter registration.
//!
//! Tests exercise the entrypoint path: dispute creation, review, and
//! resolution — gaining arbiter status should flip a previously-failing
//! resolution into a passing one.

#![cfg(test)]

use crate::errors::QuickLendXError;
use soroban_sdk::{testutils::Address as _, Address, Env, String};

const REASON: &str = "lorem ipsum dolor sit amet consectetur adipiscing elit";
const EVIDENCE: &str = "evidence placeholder, padded to satisfy minimum length easily trailing padding";
const RESOLUTION: &str = "resolution note, padded out to satisfy the minimum length requirement";

fn setup() -> (Env, Address, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let business = Address::generate(&env);
    let investor = Address::generate(&env);

    crate::admin::AdminStorage::set_admin(&env, &admin);
    (env, admin, business, investor)
}

/// Drive an invoice through create_dispute -> put_under_review so that the
/// only thing left to test is the resolve path.
fn open_dispute_under_review(
    env: &Env,
    admin: &Address,
    business: &Address,
    investor: &Address,
) -> soroban_sdk::BytesN<32> {
    let invoice_id = crate::invoice::Invoice::new(
        env,
        business.clone(),
        1_000_000,
        Address::generate(env),
        env.ledger().timestamp() + 86_400,
        String::from_str(env, "Test invoice for dispute arbiter guard"),
        crate::types::InvoiceCategory::Services,
        soroban_sdk::Vec::new(env),
        None,
        None,
        None,
    )
    .unwrap()
    .id;

    crate::dispute::create_dispute(
        env,
        &invoice_id,
        business,
        &String::from_str(env, REASON),
        &String::from_str(env, EVIDENCE),
    )
    .unwrap();
    crate::dispute::put_dispute_under_review(env, admin, &invoice_id).unwrap();

    // Investor isn't strictly needed for this test but is kept so the setup
    // mirrors real-world usage and will surface any future regression where
    // resolve_dispute starts poking at investor state.
    let _ = investor;
    invoice_id
}

// ─── Negative: admin without arbiter status ───────────────────────────────────

/// Admin-only path (without arbiter registration) must be rejected with a
/// typed error. Before the fix this passes because admin authority alone
/// sufficed; after the fix it fails with `NotArbiter`.
#[test]
fn resolve_dispute_rejected_when_admin_is_not_arbiter() {
    let (env, admin, business, investor) = setup();
    let invoice_id = open_dispute_under_review(&env, &admin, &business, &investor);

    assert!(!crate::arbiter::ArbiterStorage::is_arbiter(&env, &admin));

    let err = crate::dispute::resolve_dispute(
        &env,
        &admin,
        &invoice_id,
        &String::from_str(&env, RESOLUTION),
    )
    .unwrap_err();
    assert_eq!(err, QuickLendXError::NotArbiter);
}

/// Same check for the structured resolution variant.
#[test]
fn resolve_dispute_structured_rejected_when_admin_is_not_arbiter() {
    let (env, admin, business, investor) = setup();
    let invoice_id = open_dispute_under_review(&env, &admin, &business, &investor);

    let err = crate::dispute::resolve_dispute_structured(
        &env,
        &admin,
        &invoice_id,
        crate::types::DisputeResolution::FavorInvestor,
        &String::from_str(&env, RESOLUTION),
    )
    .unwrap_err();
    assert_eq!(err, QuickLendXError::NotArbiter);
}

/// Putting a dispute under review does NOT require arbiter status — Issue #1840
/// explicitly scopes the guard to *resolve*. Any authenticated admin may
/// transition `Disputed → UnderReview`. Only the final `UnderReview → Resolved`
/// step is gated by the arbiter registry.
#[test]
fn put_dispute_under_review_succeeds_for_non_arbiter_admin() {
    let (env, admin, business, investor) = setup();
    let invoice_id = crate::invoice::Invoice::new(
        &env,
        business.clone(),
        1_000_000,
        Address::generate(&env),
        env.ledger().timestamp() + 86_400,
        String::from_str(&env, "Second test invoice"),
        crate::types::InvoiceCategory::Services,
        soroban_sdk::Vec::new(&env),
        None,
        None,
        None,
    )
    .unwrap()
    .id;

    crate::dispute::create_dispute(
        &env,
        &invoice_id,
        &business,
        &String::from_str(&env, REASON),
        &String::from_str(&env, EVIDENCE),
    )
    .unwrap();

    assert!(!crate::arbiter::ArbiterStorage::is_arbiter(&env, &admin));
    crate::dispute::put_dispute_under_review(&env, &admin, &invoice_id).unwrap();
    let _ = investor;
}

/// Negative path: even after the admin legitimately moves the dispute into
/// `UnderReview`, they cannot resolve it without arbiter status. Combines the
/// two halves of the issue ("review is admin-only" + "resolve is arbiter-only")
/// into a single end-to-end regression guard.
#[test]
fn resolve_dispute_rejected_even_through_full_review_path() {
    let (env, admin, business, investor) = setup();
    let invoice_id = crate::invoice::Invoice::new(
        &env,
        business.clone(),
        1_000_000,
        Address::generate(&env),
        env.ledger().timestamp() + 86_400,
        String::from_str(&env, "End-to-end review-then-resolve test"),
        crate::types::InvoiceCategory::Services,
        soroban_sdk::Vec::new(&env),
        None,
        None,
        None,
    )
    .unwrap()
    .id;

    crate::dispute::create_dispute(
        &env,
        &invoice_id,
        &business,
        &String::from_str(&env, REASON),
        &String::from_str(&env, EVIDENCE),
    )
    .unwrap();

    // Review succeeds without arbiter registration.
    crate::dispute::put_dispute_under_review(&env, &admin, &invoice_id).unwrap();

    // Resolve still requires arbiter registration, even after a successful review.
    let err = crate::dispute::resolve_dispute(
        &env,
        &admin,
        &invoice_id,
        &String::from_str(&env, RESOLUTION),
    )
    .unwrap_err();
    assert_eq!(err, QuickLendXError::NotArbiter);
    let _ = investor;
}

// ─── Positive: admin who is also a registered arbiter ─────────────────────────

/// After `register_arbiter` the guard is satisfied and resolution succeeds.
#[test]
fn resolve_dispute_succeeds_after_register_arbiter() {
    let (env, admin, business, investor) = setup();
    let invoice_id = open_dispute_under_review(&env, &admin, &business, &investor);

    crate::arbiter::ArbiterStorage::register_arbiter(&env, &admin, &admin).unwrap();
    assert!(crate::arbiter::ArbiterStorage::is_arbiter(&env, &admin));

    crate::dispute::resolve_dispute(
        &env,
        &admin,
        &invoice_id,
        &String::from_str(&env, RESOLUTION),
    )
    .unwrap();
}

// ─── Arbiter independence from admin authority ───────────────────────────────

/// A non-admin, registered arbiter cannot resolve disputes because the
/// `require_admin` guard still applies. Splits the roles cleanly.
#[test]
fn non_admin_arbiter_still_blocked_by_require_admin() {
    let (env, admin, business, investor) = setup();
    let invoice_id = open_dispute_under_review(&env, &admin, &business, &investor);

    let non_admin_arbiter = Address::generate(&env);
    crate::arbiter::ArbiterStorage::register_arbiter(&env, &admin, &non_admin_arbiter).unwrap();
    assert!(crate::arbiter::ArbiterStorage::is_arbiter(&env, &non_admin_arbiter));

    let err = crate::dispute::resolve_dispute(
        &env,
        &non_admin_arbiter,
        &invoice_id,
        &String::from_str(&env, RESOLUTION),
    )
    .unwrap_err();
    // Either NotAdmin from the admin gate, or NotArbiter if both happen to
    // pass. We only assert "not OK" — the precise code is not the contract
    // surface tested here.
    assert!(
        err == QuickLendXError::NotAdmin || err == QuickLendXError::NotArbiter,
        "non-admin arbiter should be blocked, got {:?}",
        err
    );
}

/// Unregistering an arbiter immediately revokes their authority — the next
/// resolve attempt fails closed.
#[test]
fn unregister_arbiter_revokes_authority_immediately() {
    let (env, admin, business, investor) = setup();
    let invoice_id = open_dispute_under_review(&env, &admin, &business, &investor);

    crate::arbiter::ArbiterStorage::register_arbiter(&env, &admin, &admin).unwrap();
    crate::arbiter::ArbiterStorage::unregister_arbiter(&env, &admin, &admin).unwrap();
    assert!(!crate::arbiter::ArbiterStorage::is_arbiter(&env, &admin));

    let err = crate::dispute::resolve_dispute(
        &env,
        &admin,
        &invoice_id,
        &String::from_str(&env, RESOLUTION),
    )
    .unwrap_err();
    assert_eq!(err, QuickLendXError::NotArbiter);
}

/// Revoking an address that was never registered is a hard error rather
/// than a silent success — surfaces accidental no-op revocations.
#[test]
fn unregister_unknown_arbiter_returns_operation_not_allowed() {
    let (env, admin, _business, _investor) = setup();
    let ghost = Address::generate(&env);
    let err = crate::arbiter::ArbiterStorage::unregister_arbiter(&env, &admin, &ghost).unwrap_err();
    assert_eq!(err, QuickLendXError::OperationNotAllowed);
}

/// Register is idempotent — re-registering an existing arbiter is a no-op
/// rather than a duplicate-key error.
#[test]
fn register_arbiter_is_idempotent() {
    let (env, admin, _business, _investor) = setup();
    let arbiter = Address::generate(&env);
    crate::arbiter::ArbiterStorage::register_arbiter(&env, &admin, &arbiter).unwrap();
    crate::arbiter::ArbiterStorage::register_arbiter(&env, &admin, &arbiter).unwrap();

    assert!(crate::arbiter::ArbiterStorage::is_arbiter(&env, &arbiter));
    let listed = crate::arbiter::ArbiterStorage::list_arbiters(&env);
    assert_eq!(listed.len(), 1);
}
