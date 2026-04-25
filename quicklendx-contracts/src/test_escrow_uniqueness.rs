//! Escrow uniqueness tests: one escrow per invoice; prevent overwrite/poisoning.
//!
//! ## Security Invariant
//! Each invoice maps to **at most one** escrow record for its entire lifetime.
//! Any attempt to create a second escrow for the same `invoice_id` — regardless
//! of the caller, the escrow's current status, or the amount — must be rejected
//! with [`QuickLendXError::InvoiceAlreadyFunded`].
//!
//! ## Attack Vectors Covered
//! 1. **Double-accept via `accept_bid`** – business calls `accept_bid` twice on
//!    the same invoice (same or different bid).
//! 2. **Direct `create_escrow` bypass** – attacker calls `payments::create_escrow`
//!    directly after a legitimate escrow already exists.
//! 3. **Post-release overwrite** – attempt to create a new escrow after the
//!    original has been released (terminal state).
//! 4. **Post-refund overwrite** – attempt to create a new escrow after the
//!    original has been refunded (terminal state).
//! 5. **Cross-invoice isolation** – funding one invoice must not affect another.
//! 6. **Storage key collision** – two invoices with different IDs must produce
//!    independent escrow records that cannot overwrite each other.
//!
//! Run: `cargo test test_escrow_uniqueness`

use super::*;
use crate::bid::BidStatus;
use crate::errors::QuickLendXError;
use crate::invoice::{InvoiceCategory, InvoiceStatus};
use crate::payments::{create_escrow, EscrowStatus};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, BytesN, Env, String, Vec,
};

// ============================================================================
// Shared helpers
// ============================================================================

fn setup() -> (Env, QuickLendXContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000_000);
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let _ = client.try_initialize_admin(&admin);
    client.set_admin(&admin);
    (env, client, admin)
}

fn setup_token(
    env: &Env,
    business: &Address,
    investor: &Address,
    contract_id: &Address,
) -> Address {
    let token_admin = Address::generate(env);
    let currency = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();
    let sac = token::StellarAssetClient::new(env, &currency);
    let tok = token::Client::new(env, &currency);
    let initial = 100_000i128;
    sac.mint(business, &initial);
    sac.mint(investor, &initial);
    let expiry = env.ledger().sequence() + 10_000;
    tok.approve(business, contract_id, &initial, &expiry);
    tok.approve(investor, contract_id, &initial, &expiry);
    currency
}

fn verified_business(env: &Env, client: &QuickLendXContractClient, admin: &Address) -> Address {
    let b = Address::generate(env);
    client.submit_kyc_application(&b, &String::from_str(env, "KYC"));
    client.verify_business(admin, &b);
    b
}

fn verified_investor(env: &Env, client: &QuickLendXContractClient, limit: i128) -> Address {
    let i = Address::generate(env);
    client.submit_investor_kyc(&i, &String::from_str(env, "KYC"));
    client.verify_investor(&i, &limit);
    i
}

fn verified_invoice(
    env: &Env,
    client: &QuickLendXContractClient,
    business: &Address,
    amount: i128,
    currency: &Address,
) -> BytesN<32> {
    let due_date = env.ledger().timestamp() + 86_400;
    let id = client.store_invoice(
        business,
        &amount,
        currency,
        &due_date,
        &String::from_str(env, "Invoice"),
        &InvoiceCategory::Services,
        &Vec::new(env),
    );
    client.verify_invoice(&id);
    id
}

// ============================================================================
// 1. Double-accept via the public `accept_bid` entry point
// ============================================================================

/// Calling `accept_bid` a second time on the same invoice must fail.
///
/// # Security
/// The `load_accept_bid_context` guard checks `EscrowStorage::get_escrow_by_invoice`
/// before any funds move, so the second call is rejected before the token transfer.
#[test]
fn test_double_accept_bid_rejected() {
    let (env, client, admin) = setup();
    let contract_id = client.address.clone();

    let business = verified_business(&env, &client, &admin);
    let investor = verified_investor(&env, &client, 50_000);
    let currency = setup_token(&env, &business, &investor, &contract_id);
    let tok = token::Client::new(&env, &currency);

    let amount = 10_000i128;
    let invoice_id = verified_invoice(&env, &client, &business, amount, &currency);
    let bid_id = client.place_bid(&investor, &invoice_id, &amount, &(amount + 1_000));

    // First accept succeeds.
    client.accept_bid(&invoice_id, &bid_id);
    assert_eq!(
        client.get_invoice(&invoice_id).status,
        InvoiceStatus::Funded
    );

    let investor_bal = tok.balance(&investor);
    let contract_bal = tok.balance(&contract_id);

    // Second accept on the same invoice must fail.
    let err = client
        .try_accept_bid(&invoice_id, &bid_id)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, QuickLendXError::InvalidStatus);

    // No funds moved on the failed attempt.
    assert_eq!(tok.balance(&investor), investor_bal);
    assert_eq!(tok.balance(&contract_id), contract_bal);

    // Escrow record is unchanged.
    let escrow = client.get_escrow_details(&invoice_id);
    assert_eq!(escrow.status, EscrowStatus::Held);
    assert_eq!(escrow.amount, amount);
    assert_eq!(escrow.investor, investor);
}

/// Accepting a *different* bid on an already-funded invoice must also fail.
///
/// # Security
/// Prevents an attacker from replacing the legitimate investor's escrow by
/// submitting a competing bid after funding.
#[test]
fn test_second_bid_on_funded_invoice_rejected() {
    let (env, client, admin) = setup();
    let contract_id = client.address.clone();

    let business = verified_business(&env, &client, &admin);
    let investor1 = verified_investor(&env, &client, 50_000);
    let investor2 = verified_investor(&env, &client, 50_000);

    let currency = setup_token(&env, &business, &investor1, &contract_id);
    let sac = token::StellarAssetClient::new(&env, &currency);
    let tok = token::Client::new(&env, &currency);
    sac.mint(&investor2, &100_000i128);
    tok.approve(
        &investor2,
        &contract_id,
        &100_000i128,
        &(env.ledger().sequence() + 10_000),
    );

    let amount = 10_000i128;
    let invoice_id = verified_invoice(&env, &client, &business, amount, &currency);
    let bid1 = client.place_bid(&investor1, &invoice_id, &amount, &(amount + 1_000));

    client.accept_bid(&invoice_id, &bid1);

    // investor2 cannot place a bid on a funded invoice.
    let result = client.try_place_bid(&investor2, &invoice_id, &amount, &(amount + 500));
    assert!(result.is_err(), "bidding on funded invoice must fail");

    // Escrow still belongs to investor1.
    let escrow = client.get_escrow_details(&invoice_id);
    assert_eq!(escrow.investor, investor1);
    assert_eq!(escrow.status, EscrowStatus::Held);
}

// ============================================================================
// 2. Direct `create_escrow` bypass (low-level guard)
// ============================================================================

/// `payments::create_escrow` must reject a second call for the same invoice_id
/// even when invoked directly inside the contract context.
///
/// # Security
/// This is the innermost guard. Even if higher-level checks are bypassed, the
/// storage-level duplicate check in `create_escrow` prevents overwrite.
#[test]
fn test_create_escrow_direct_duplicate_rejected() {
    let (env, client, admin) = setup();
    let contract_id = client.address.clone();

    let business = verified_business(&env, &client, &admin);
    let investor = verified_investor(&env, &client, 50_000);
    let currency = setup_token(&env, &business, &investor, &contract_id);

    let amount = 10_000i128;
    let invoice_id = verified_invoice(&env, &client, &business, amount, &currency);

    // First call succeeds.
    env.as_contract(&contract_id, || {
        let result = create_escrow(&env, &invoice_id, &investor, &business, amount, &currency);
        assert!(result.is_ok(), "first create_escrow must succeed");
    });

    // Second call for the same invoice_id must fail.
    env.as_contract(&contract_id, || {
        let result = create_escrow(&env, &invoice_id, &investor, &business, amount, &currency);
        assert!(result.is_err(), "duplicate create_escrow must fail");
        assert_eq!(result.unwrap_err(), QuickLendXError::InvoiceAlreadyFunded);
    });
}

/// The duplicate guard fires even when the second call uses a *different* investor
/// or a *different* amount — the key is the invoice_id, not the caller.
#[test]
fn test_create_escrow_different_investor_same_invoice_rejected() {
    let (env, client, admin) = setup();
    let contract_id = client.address.clone();

    let business = verified_business(&env, &client, &admin);
    let investor1 = verified_investor(&env, &client, 50_000);
    let investor2 = verified_investor(&env, &client, 50_000);
    let currency = setup_token(&env, &business, &investor1, &contract_id);
    let sac = token::StellarAssetClient::new(&env, &currency);
    let tok = token::Client::new(&env, &currency);
    sac.mint(&investor2, &100_000i128);
    tok.approve(
        &investor2,
        &contract_id,
        &100_000i128,
        &(env.ledger().sequence() + 10_000),
    );

    let amount = 10_000i128;
    let invoice_id = verified_invoice(&env, &client, &business, amount, &currency);

    env.as_contract(&contract_id, || {
        create_escrow(&env, &invoice_id, &investor1, &business, amount, &currency)
            .expect("first escrow must succeed");
    });

    env.as_contract(&contract_id, || {
        let result = create_escrow(
            &env,
            &invoice_id,
            &investor2,
            &business,
            amount / 2,
            &currency,
        );
        assert_eq!(
            result.unwrap_err(),
            QuickLendXError::InvoiceAlreadyFunded,
            "different investor must not overwrite existing escrow"
        );
    });

    // Original escrow is intact.
    let escrow = client.get_escrow_details(&invoice_id);
    assert_eq!(escrow.investor, investor1);
    assert_eq!(escrow.amount, amount);
}

// ============================================================================
// 3. Post-release overwrite attempt
// ============================================================================

/// After an escrow is released (terminal state), no new escrow may be created
/// for the same invoice.
///
/// # Security
/// Prevents an attacker from "recycling" a settled invoice to lock fresh funds.
#[test]
fn test_no_new_escrow_after_release() {
    let (env, client, admin) = setup();
    let contract_id = client.address.clone();

    let business = verified_business(&env, &client, &admin);
    let investor = verified_investor(&env, &client, 50_000);
    let currency = setup_token(&env, &business, &investor, &contract_id);

    let amount = 10_000i128;
    let invoice_id = verified_invoice(&env, &client, &business, amount, &currency);
    let bid_id = client.place_bid(&investor, &invoice_id, &amount, &(amount + 1_000));
    client.accept_bid(&invoice_id, &bid_id);
    client.release_escrow_funds(&invoice_id);

    assert_eq!(
        client.get_escrow_details(&invoice_id).status,
        EscrowStatus::Released
    );

    // Attempt to create a new escrow for the same invoice must fail.
    env.as_contract(&contract_id, || {
        let result = create_escrow(&env, &invoice_id, &investor, &business, amount, &currency);
        assert_eq!(
            result.unwrap_err(),
            QuickLendXError::InvoiceAlreadyFunded,
            "must not create escrow after release"
        );
    });
}

// ============================================================================
// 4. Post-refund overwrite attempt
// ============================================================================

/// After an escrow is refunded (terminal state), no new escrow may be created
/// for the same invoice.
///
/// # Security
/// Prevents re-funding a refunded invoice without going through the full
/// invoice lifecycle (which would require a new invoice ID).
#[test]
fn test_no_new_escrow_after_refund() {
    let (env, client, admin) = setup();
    let contract_id = client.address.clone();

    let business = verified_business(&env, &client, &admin);
    let investor = verified_investor(&env, &client, 50_000);
    let currency = setup_token(&env, &business, &investor, &contract_id);

    let amount = 10_000i128;
    let invoice_id = verified_invoice(&env, &client, &business, amount, &currency);
    let bid_id = client.place_bid(&investor, &invoice_id, &amount, &(amount + 1_000));
    client.accept_bid(&invoice_id, &bid_id);
    client.refund_escrow_funds(&invoice_id, &admin);

    assert_eq!(
        client.get_escrow_details(&invoice_id).status,
        EscrowStatus::Refunded
    );

    // Attempt to create a new escrow for the same invoice must fail.
    env.as_contract(&contract_id, || {
        let result = create_escrow(&env, &invoice_id, &investor, &business, amount, &currency);
        assert_eq!(
            result.unwrap_err(),
            QuickLendXError::InvoiceAlreadyFunded,
            "must not create escrow after refund"
        );
    });
}

// ============================================================================
// 5. Cross-invoice isolation
// ============================================================================

/// Funding invoice A must not affect invoice B's escrow state.
///
/// # Security
/// Verifies that the storage key `(symbol_short!("escrow"), invoice_id)` is
/// truly per-invoice and that writes to one slot do not bleed into another.
#[test]
fn test_escrow_isolation_between_invoices() {
    let (env, client, admin) = setup();
    let contract_id = client.address.clone();

    let business = verified_business(&env, &client, &admin);
    let investor = verified_investor(&env, &client, 100_000);
    let currency = setup_token(&env, &business, &investor, &contract_id);

    let amount = 5_000i128;
    let invoice_a = verified_invoice(&env, &client, &business, amount, &currency);
    let invoice_b = verified_invoice(&env, &client, &business, amount, &currency);

    let bid_a = client.place_bid(&investor, &invoice_a, &amount, &(amount + 500));
    let bid_b = client.place_bid(&investor, &invoice_b, &amount, &(amount + 500));

    // Fund only invoice A.
    client.accept_bid(&invoice_a, &bid_a);

    // Invoice B must still be Verified with no escrow.
    assert_eq!(
        client.get_invoice(&invoice_b).status,
        InvoiceStatus::Verified
    );
    assert!(
        client.try_get_escrow_details(&invoice_b).is_err(),
        "invoice B must have no escrow after funding invoice A"
    );

    // Fund invoice B independently.
    client.accept_bid(&invoice_b, &bid_b);

    // Both escrows are independent.
    let escrow_a = client.get_escrow_details(&invoice_a);
    let escrow_b = client.get_escrow_details(&invoice_b);

    assert_ne!(
        escrow_a.escrow_id, escrow_b.escrow_id,
        "escrow IDs must differ"
    );
    assert_eq!(escrow_a.invoice_id, invoice_a);
    assert_eq!(escrow_b.invoice_id, invoice_b);
    assert_eq!(escrow_a.status, EscrowStatus::Held);
    assert_eq!(escrow_b.status, EscrowStatus::Held);
}

/// Releasing invoice A's escrow must not change invoice B's escrow.
#[test]
fn test_release_one_escrow_does_not_affect_other() {
    let (env, client, admin) = setup();
    let contract_id = client.address.clone();

    let business = verified_business(&env, &client, &admin);
    let investor = verified_investor(&env, &client, 100_000);
    let currency = setup_token(&env, &business, &investor, &contract_id);

    let amount = 5_000i128;
    let invoice_a = verified_invoice(&env, &client, &business, amount, &currency);
    let invoice_b = verified_invoice(&env, &client, &business, amount, &currency);

    let bid_a = client.place_bid(&investor, &invoice_a, &amount, &(amount + 500));
    let bid_b = client.place_bid(&investor, &invoice_b, &amount, &(amount + 500));

    client.accept_bid(&invoice_a, &bid_a);
    client.accept_bid(&invoice_b, &bid_b);

    // Release only invoice A.
    client.release_escrow_funds(&invoice_a);

    assert_eq!(
        client.get_escrow_details(&invoice_a).status,
        EscrowStatus::Released
    );
    // Invoice B's escrow must remain Held.
    assert_eq!(
        client.get_escrow_details(&invoice_b).status,
        EscrowStatus::Held,
        "releasing invoice A must not affect invoice B"
    );
}

// ============================================================================
// 6. Storage key collision resistance
// ============================================================================

/// Two invoices with distinct IDs must produce distinct escrow storage entries.
/// Verifying that the escrow lookup key is the full 32-byte invoice_id, not a
/// truncated or hashed prefix.
#[test]
fn test_distinct_invoice_ids_produce_distinct_escrow_keys() {
    let (env, client, admin) = setup();
    let contract_id = client.address.clone();

    let business = verified_business(&env, &client, &admin);
    let investor = verified_investor(&env, &client, 100_000);
    let currency = setup_token(&env, &business, &investor, &contract_id);

    let amount = 3_000i128;

    // Create three invoices and fund all of them.
    let inv_a = verified_invoice(&env, &client, &business, amount, &currency);
    let inv_b = verified_invoice(&env, &client, &business, amount, &currency);
    let inv_c = verified_invoice(&env, &client, &business, amount, &currency);

    let bid_a = client.place_bid(&investor, &inv_a, &amount, &(amount + 100));
    let bid_b = client.place_bid(&investor, &inv_b, &amount, &(amount + 100));
    let bid_c = client.place_bid(&investor, &inv_c, &amount, &(amount + 100));

    client.accept_bid(&inv_a, &bid_a);
    client.accept_bid(&inv_b, &bid_b);
    client.accept_bid(&inv_c, &bid_c);

    let ea = client.get_escrow_details(&inv_a);
    let eb = client.get_escrow_details(&inv_b);
    let ec = client.get_escrow_details(&inv_c);

    // Each escrow references its own invoice.
    assert_eq!(ea.invoice_id, inv_a);
    assert_eq!(eb.invoice_id, inv_b);
    assert_eq!(ec.invoice_id, inv_c);

    // All escrow IDs must be unique.
    assert_ne!(ea.escrow_id, eb.escrow_id, "escrow IDs A/B must differ");
    assert_ne!(ea.escrow_id, ec.escrow_id, "escrow IDs A/C must differ");
    assert_ne!(eb.escrow_id, ec.escrow_id, "escrow IDs B/C must differ");

    // All escrows are Held.
    assert_eq!(ea.status, EscrowStatus::Held);
    assert_eq!(eb.status, EscrowStatus::Held);
    assert_eq!(ec.status, EscrowStatus::Held);
}

// ============================================================================
// 7. Invariant: funded_amount and investor fields are set atomically with escrow
// ============================================================================

/// After a successful `accept_bid`, the invoice's `funded_amount` and `investor`
/// fields must match the escrow record exactly.
///
/// # Security
/// Ensures no partial-write state where an escrow exists but the invoice is not
/// marked funded (or vice-versa), which could be exploited to double-fund.
#[test]
fn test_invoice_and_escrow_fields_are_consistent_after_accept() {
    let (env, client, admin) = setup();
    let contract_id = client.address.clone();

    let business = verified_business(&env, &client, &admin);
    let investor = verified_investor(&env, &client, 50_000);
    let currency = setup_token(&env, &business, &investor, &contract_id);

    let amount = 7_500i128;
    let invoice_id = verified_invoice(&env, &client, &business, amount, &currency);
    let bid_id = client.place_bid(&investor, &invoice_id, &amount, &(amount + 750));
    client.accept_bid(&invoice_id, &bid_id);

    let invoice = client.get_invoice(&invoice_id);
    let escrow = client.get_escrow_details(&invoice_id);

    assert_eq!(invoice.status, InvoiceStatus::Funded);
    assert_eq!(invoice.funded_amount, escrow.amount);
    assert_eq!(invoice.investor, Some(escrow.investor.clone()));
    assert_eq!(escrow.invoice_id, invoice_id);
    assert_eq!(escrow.business, invoice.business);
    assert_eq!(escrow.status, EscrowStatus::Held);
}

/// A failed `accept_bid` must leave the invoice in its pre-call state with no
/// escrow record written.
#[test]
fn test_failed_accept_leaves_no_escrow_and_no_state_change() {
    let (env, client, admin) = setup();
    let contract_id = client.address.clone();

    let business = verified_business(&env, &client, &admin);
    let investor = verified_investor(&env, &client, 50_000);

    // Investor has balance but zero allowance → transfer will fail.
    let token_admin = Address::generate(&env);
    let currency = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();
    let sac = token::StellarAssetClient::new(&env, &currency);
    let tok = token::Client::new(&env, &currency);
    sac.mint(&business, &100_000i128);
    sac.mint(&investor, &100_000i128);
    let expiry = env.ledger().sequence() + 10_000;
    tok.approve(&business, &contract_id, &100_000i128, &expiry);
    tok.approve(&investor, &contract_id, &0i128, &expiry); // zero allowance

    let amount = 5_000i128;
    let invoice_id = verified_invoice(&env, &client, &business, amount, &currency);
    let bid_id = client.place_bid(&investor, &invoice_id, &amount, &(amount + 500));

    let result = client.try_accept_bid(&invoice_id, &bid_id);
    assert_eq!(
        result.unwrap_err().unwrap(),
        QuickLendXError::OperationNotAllowed
    );

    // Invoice unchanged.
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Verified);
    assert_eq!(invoice.funded_amount, 0);
    assert!(invoice.investor.is_none());

    // Bid unchanged.
    assert_eq!(client.get_bid(&bid_id).unwrap().status, BidStatus::Placed);

    // No escrow record.
    assert!(
        client.try_get_escrow_details(&invoice_id).is_err(),
        "no escrow must exist after failed accept"
    );
}
