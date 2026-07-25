//! Tests for the auto-resolution deadline boundary.
//!
//! # What this locks in
//!
//! `mark_invoice_defaulted` and `check_invoice_expiration` share the same
//! exclusivity rule implemented in `defaults::mark_invoice_defaulted` and
//! `invoice::check_and_handle_expiration`:
//!
//! ```text
//! grace_deadline = invoice.due_date + grace_period   (saturating)
//!
//! current_timestamp <= grace_deadline  →  NOT defaultable  (operation denied / returns false)
//! current_timestamp >  grace_deadline  →  defaultable       (transitions to Defaulted)
//! ```
//!
//! The three cases that must always hold are:
//!
//!  * **deadline − 1**: one second before the grace deadline → NOT defaultable
//!  * **deadline**:     exactly at the grace deadline        → NOT defaultable
//!  * **deadline + 1**: one second after the grace deadline  → defaultable
//!
//! This file runs on every CI matrix entry (no feature gate).

#![cfg(test)]

use super::*;
use crate::defaults::DEFAULT_GRACE_PERIOD;
use crate::errors::QuickLendXError;
use crate::invoice::{InvoiceCategory, InvoiceStatus};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, BytesN, Env, String, Vec,
};

// ---------------------------------------------------------------------------
// Shared test helpers (mirrors the pattern in test_default_grace_boundary.rs)
// ---------------------------------------------------------------------------

fn setup() -> (Env, QuickLendXContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.set_admin(&admin);
    client.initialize_fee_system(&admin);
    (env, client, admin)
}

fn create_verified_business(
    env: &Env,
    client: &QuickLendXContractClient,
    admin: &Address,
) -> Address {
    let business = Address::generate(env);
    client.submit_kyc_application(&business, &String::from_str(env, "KYC data"));
    client.verify_business(admin, &business);
    business
}

fn create_verified_investor(
    env: &Env,
    client: &QuickLendXContractClient,
    limit: i128,
) -> Address {
    let investor = Address::generate(env);
    client.submit_investor_kyc(&investor, &String::from_str(env, "KYC data"));
    client.verify_investor(&investor, &limit);
    investor
}

/// Creates a funded invoice and returns its ID.
/// Mints `amount` tokens to the investor and sets up token approval.
fn create_funded_invoice(
    env: &Env,
    client: &QuickLendXContractClient,
    admin: &Address,
    business: &Address,
    investor: &Address,
    amount: i128,
    due_date: u64,
) -> BytesN<32> {
    let token_admin = Address::generate(env);
    let currency = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let sac_client = token::StellarAssetClient::new(env, &currency);
    let token_client = token::Client::new(env, &currency);

    client.add_currency(admin, &currency);
    sac_client.mint(investor, &amount);

    let expiry = env.ledger().sequence() + 10_000;
    token_client.approve(investor, &client.address, &amount, &expiry);

    let invoice_id = client.store_invoice(
        business,
        &amount,
        &currency,
        &due_date,
        &String::from_str(env, "Test invoice"),
        &InvoiceCategory::Services,
        &Vec::new(env),
    );
    client.verify_invoice(&invoice_id);

    let bid_id = client.place_bid(
        investor,
        &invoice_id,
        &amount,
        &(amount + 100),
        &BytesN::from_array(env, &[0u8; 32]),
    );
    client.accept_bid(&invoice_id, &bid_id);
    invoice_id
}

// ---------------------------------------------------------------------------
// mark_invoice_defaulted — three-point boundary suite
// ---------------------------------------------------------------------------

/// At deadline − 1: the invoice must NOT be defaultable.
/// `mark_invoice_defaulted` must return `OperationNotAllowed`.
#[test]
fn mark_invoice_defaulted_returns_operation_not_allowed_one_second_before_deadline() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, 10_000);

    let now = env.ledger().timestamp();
    let due_date = now + 86_400; // 1 day from now
    let grace = 3 * 24 * 60 * 60u64; // 3-day grace
    let grace_deadline = due_date + grace;

    let invoice_id =
        create_funded_invoice(&env, &client, &admin, &business, &investor, 1_000, due_date);

    // One second before the grace deadline — must not default.
    env.ledger().set_timestamp(grace_deadline - 1);
    let result = client.try_mark_invoice_defaulted(&invoice_id, &Some(grace));
    assert!(
        matches!(result, Err(Ok(QuickLendXError::OperationNotAllowed))),
        "Expected OperationNotAllowed at deadline-1, got: {:?}",
        result
    );

    // Invoice must remain Funded.
    assert_eq!(
        client.get_invoice(&invoice_id).status,
        InvoiceStatus::Funded,
        "Invoice must stay Funded at deadline-1"
    );
}

/// At deadline: the invoice must NOT be defaultable.
/// The boundary is strictly exclusive (`current > deadline`), so equality
/// must be rejected with `OperationNotAllowed`.
#[test]
fn mark_invoice_defaulted_returns_operation_not_allowed_at_exact_deadline() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, 10_000);

    let now = env.ledger().timestamp();
    let due_date = now + 86_400;
    let grace = 3 * 24 * 60 * 60u64;
    let grace_deadline = due_date + grace;

    let invoice_id =
        create_funded_invoice(&env, &client, &admin, &business, &investor, 1_000, due_date);

    // Exactly at the grace deadline — must still not default.
    env.ledger().set_timestamp(grace_deadline);
    let result = client.try_mark_invoice_defaulted(&invoice_id, &Some(grace));
    assert!(
        matches!(result, Err(Ok(QuickLendXError::OperationNotAllowed))),
        "Expected OperationNotAllowed at exact deadline, got: {:?}",
        result
    );

    assert_eq!(
        client.get_invoice(&invoice_id).status,
        InvoiceStatus::Funded,
        "Invoice must stay Funded at exact deadline"
    );
}

/// At deadline + 1: the invoice MUST be defaultable.
/// One second after the grace deadline the invoice must transition to Defaulted.
#[test]
fn mark_invoice_defaulted_transitions_to_defaulted_one_second_after_deadline() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, 10_000);

    let now = env.ledger().timestamp();
    let due_date = now + 86_400;
    let grace = 3 * 24 * 60 * 60u64;
    let grace_deadline = due_date + grace;

    let invoice_id =
        create_funded_invoice(&env, &client, &admin, &business, &investor, 1_000, due_date);

    // One second past the grace deadline — must default.
    env.ledger().set_timestamp(grace_deadline + 1);
    let result = client.try_mark_invoice_defaulted(&invoice_id, &Some(grace));
    assert!(
        result.is_ok(),
        "mark_invoice_defaulted must succeed at deadline+1, got: {:?}",
        result
    );

    assert_eq!(
        client.get_invoice(&invoice_id).status,
        InvoiceStatus::Defaulted,
        "Invoice must be Defaulted at deadline+1"
    );
}

// ---------------------------------------------------------------------------
// check_invoice_expiration — three-point boundary suite
// ---------------------------------------------------------------------------

/// At deadline − 1: check_invoice_expiration must return false and leave the
/// invoice Funded.
#[test]
fn check_invoice_expiration_returns_false_one_second_before_deadline() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, 10_000);

    let now = env.ledger().timestamp();
    let due_date = now + 86_400;
    let grace = 5 * 24 * 60 * 60u64; // 5-day grace
    let grace_deadline = due_date + grace;

    let invoice_id =
        create_funded_invoice(&env, &client, &admin, &business, &investor, 1_000, due_date);

    env.ledger().set_timestamp(grace_deadline - 1);
    let expired = client.check_invoice_expiration(&invoice_id, &Some(grace));
    assert!(
        !expired,
        "check_invoice_expiration must return false at deadline-1"
    );
    assert_eq!(
        client.get_invoice(&invoice_id).status,
        InvoiceStatus::Funded,
        "Invoice must stay Funded at deadline-1"
    );
}

/// At deadline: check_invoice_expiration must return false and leave the
/// invoice Funded — the exclusive boundary (`<=`) keeps this safe.
#[test]
fn check_invoice_expiration_returns_false_at_exact_deadline() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, 10_000);

    let now = env.ledger().timestamp();
    let due_date = now + 86_400;
    let grace = 5 * 24 * 60 * 60u64;
    let grace_deadline = due_date + grace;

    let invoice_id =
        create_funded_invoice(&env, &client, &admin, &business, &investor, 1_000, due_date);

    env.ledger().set_timestamp(grace_deadline);
    let expired = client.check_invoice_expiration(&invoice_id, &Some(grace));
    assert!(
        !expired,
        "check_invoice_expiration must return false at exact deadline"
    );
    assert_eq!(
        client.get_invoice(&invoice_id).status,
        InvoiceStatus::Funded,
        "Invoice must stay Funded at exact deadline"
    );
}

/// At deadline + 1: check_invoice_expiration must return true and transition
/// the invoice to Defaulted.
#[test]
fn check_invoice_expiration_returns_true_and_defaults_one_second_after_deadline() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, 10_000);

    let now = env.ledger().timestamp();
    let due_date = now + 86_400;
    let grace = 5 * 24 * 60 * 60u64;
    let grace_deadline = due_date + grace;

    let invoice_id =
        create_funded_invoice(&env, &client, &admin, &business, &investor, 1_000, due_date);

    env.ledger().set_timestamp(grace_deadline + 1);
    let expired = client.check_invoice_expiration(&invoice_id, &Some(grace));
    assert!(
        expired,
        "check_invoice_expiration must return true at deadline+1"
    );
    assert_eq!(
        client.get_invoice(&invoice_id).status,
        InvoiceStatus::Defaulted,
        "Invoice must be Defaulted at deadline+1"
    );
}

// ---------------------------------------------------------------------------
// Default grace period — same three-point boundary using DEFAULT_GRACE_PERIOD
// ---------------------------------------------------------------------------

/// With the protocol default grace period: exactly at the deadline is safe.
#[test]
fn default_grace_period_boundary_not_defaultable_at_exact_deadline() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, 10_000);

    let now = env.ledger().timestamp();
    let due_date = now + 86_400;
    let grace_deadline = due_date + DEFAULT_GRACE_PERIOD;

    let invoice_id =
        create_funded_invoice(&env, &client, &admin, &business, &investor, 1_000, due_date);

    env.ledger().set_timestamp(grace_deadline);
    let result = client.try_mark_invoice_defaulted(&invoice_id, &None);
    assert!(
        matches!(result, Err(Ok(QuickLendXError::OperationNotAllowed))),
        "Default grace: must not default at exact deadline, got: {:?}",
        result
    );
    assert_eq!(client.get_invoice(&invoice_id).status, InvoiceStatus::Funded);
}

/// With the protocol default grace period: one second after the deadline
/// triggers a successful default.
#[test]
fn default_grace_period_boundary_defaultable_one_second_after_deadline() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, 10_000);

    let now = env.ledger().timestamp();
    let due_date = now + 86_400;
    let grace_deadline = due_date + DEFAULT_GRACE_PERIOD;

    let invoice_id =
        create_funded_invoice(&env, &client, &admin, &business, &investor, 1_000, due_date);

    env.ledger().set_timestamp(grace_deadline + 1);
    let result = client.try_mark_invoice_defaulted(&invoice_id, &None);
    assert!(
        result.is_ok(),
        "Default grace: must default one second after deadline, got: {:?}",
        result
    );
    assert_eq!(
        client.get_invoice(&invoice_id).status,
        InvoiceStatus::Defaulted
    );
}

// ---------------------------------------------------------------------------
// Zero grace period — boundary is the due_date itself
// ---------------------------------------------------------------------------

/// Zero grace period: at the due_date the invoice is NOT yet defaultable.
#[test]
fn zero_grace_period_not_defaultable_at_due_date() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, 10_000);

    let now = env.ledger().timestamp();
    let due_date = now + 86_400;

    let invoice_id =
        create_funded_invoice(&env, &client, &admin, &business, &investor, 1_000, due_date);

    // With zero grace, grace_deadline == due_date.  Exactly at due_date: not defaultable.
    env.ledger().set_timestamp(due_date);
    let result = client.try_mark_invoice_defaulted(&invoice_id, &Some(0));
    assert!(
        matches!(result, Err(Ok(QuickLendXError::OperationNotAllowed))),
        "Zero grace: must not default at exact due_date, got: {:?}",
        result
    );
    assert_eq!(client.get_invoice(&invoice_id).status, InvoiceStatus::Funded);
}

/// Zero grace period: one second after the due_date the invoice IS defaultable.
#[test]
fn zero_grace_period_defaultable_one_second_after_due_date() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, 10_000);

    let now = env.ledger().timestamp();
    let due_date = now + 86_400;

    let invoice_id =
        create_funded_invoice(&env, &client, &admin, &business, &investor, 1_000, due_date);

    env.ledger().set_timestamp(due_date + 1);
    let result = client.try_mark_invoice_defaulted(&invoice_id, &Some(0));
    assert!(
        result.is_ok(),
        "Zero grace: must default one second after due_date, got: {:?}",
        result
    );
    assert_eq!(
        client.get_invoice(&invoice_id).status,
        InvoiceStatus::Defaulted
    );
}

// ---------------------------------------------------------------------------
// Idempotency — double-default must fail with InvoiceAlreadyDefaulted
// ---------------------------------------------------------------------------

/// Calling mark_invoice_defaulted twice on the same invoice must fail on the
/// second call with InvoiceAlreadyDefaulted, not silently succeed or panic.
#[test]
fn mark_invoice_defaulted_is_idempotent_after_transition() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, 10_000);

    let now = env.ledger().timestamp();
    let due_date = now + 86_400;
    let grace = 2 * 24 * 60 * 60u64;
    let grace_deadline = due_date + grace;

    let invoice_id =
        create_funded_invoice(&env, &client, &admin, &business, &investor, 1_000, due_date);

    env.ledger().set_timestamp(grace_deadline + 1);

    // First call — must succeed.
    client.mark_invoice_defaulted(&invoice_id, &Some(grace));
    assert_eq!(
        client.get_invoice(&invoice_id).status,
        InvoiceStatus::Defaulted
    );

    // Second call — must be rejected.
    let second = client.try_mark_invoice_defaulted(&invoice_id, &Some(grace));
    assert!(
        matches!(
            second,
            Err(Ok(QuickLendXError::InvoiceAlreadyDefaulted))
                | Err(Ok(QuickLendXError::DuplicateDefaultTransition))
        ),
        "Second default attempt must fail with AlreadyDefaulted or DuplicateTransition, got: {:?}",
        second
    );
}

// ---------------------------------------------------------------------------
// Non-funded invoice — must never be auto-resolved regardless of time
// ---------------------------------------------------------------------------

/// A verified-but-not-funded invoice must never be auto-resolved, even long
/// after any deadline would have elapsed.
#[test]
fn non_funded_invoice_not_defaultable_after_deadline() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);

    let currency = Address::generate(&env);
    let now = env.ledger().timestamp();
    let due_date = now + 86_400;
    let grace = DEFAULT_GRACE_PERIOD;

    let invoice_id = client.store_invoice(
        &business,
        &1_000,
        &currency,
        &due_date,
        &String::from_str(&env, "Not funded"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    client.verify_invoice(&invoice_id);

    // Jump well past any deadline.
    env.ledger().set_timestamp(due_date + grace + 86_400);

    let result = client.try_mark_invoice_defaulted(&invoice_id, &Some(grace));
    assert!(
        result.is_err(),
        "Non-funded invoice must not be defaultable, got: {:?}",
        result
    );
    // Status must not be Defaulted.
    let status = client.get_invoice(&invoice_id).status;
    assert_ne!(
        status,
        InvoiceStatus::Defaulted,
        "Non-funded invoice status must never reach Defaulted"
    );
}
