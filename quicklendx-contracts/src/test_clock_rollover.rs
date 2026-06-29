//! # Clock Rollover Regression Tests — issue #1585
//!
//! Regression suite that locks in contract behaviour at the u64 timestamp
//! boundary: `u64::MAX - 1`, `u64::MAX`, and a simulated rollover back to 0.
//!
//! ## Coverage matrix
//!
//! | Area | Happy-path | Sad-path |
//! |------|-----------|----------|
//! | `Invoice::is_overdue` | not overdue at `u64::MAX` due date | overdue when `due_date` is small and clock is near MAX |
//! | `Invoice::grace_deadline` | saturates to `u64::MAX` from near-MAX `due_date` | — |
//! | `Invoice::grace_deadline` | saturates when `due_date == u64::MAX` and grace > 0 | — |
//! | `Bid::default_expiration` | saturates at `u64::MAX` | — |
//! | `Bid::is_expired` | not expired at exact `u64::MAX` boundary | expired for small expiry near MAX |
//! | invoice creation (contract) | accepted at `u64::MAX - 1` ledger with `u64::MAX` due_date | rejected when `due_date == now == u64::MAX` |
//! | invoice state (contract) | overdue query safe after simulated rollover | — |
//! | bid placement (contract) | expiration saturated when placed near `u64::MAX` | — |
//! | cleanup (contract) | no bid removed at exact saturated boundary | — |
//!
//! ## Design notes
//!
//! * All tests are deterministic — timestamps controlled via
//!   `env.ledger().set_timestamp()` and `env.as_contract()`.
//! * No `std::` symbols beyond `extern crate std` in test modules that need it.
//!   All helpers rely on `soroban_sdk` primitives (#![no_std] discipline).
//! * Test names are assertive: they describe the expected outcome.

#![cfg(test)]

use crate::bid::{Bid, BidStatus};
use crate::invoice::InvoiceCategory;
use crate::{QuickLendXContract, QuickLendXContractClient};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, BytesN, Env, String, Vec,
};

// ── Constants ─────────────────────────────────────────────────────────────────

const SECONDS_PER_DAY: u64 = 86_400;
/// Timestamp immediately before the u64 ceiling.
const NEAR_MAX: u64 = u64::MAX - 1;

// ── Setup helpers ─────────────────────────────────────────────────────────────

fn setup() -> (Env, QuickLendXContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.set_admin(&admin);
    (env, client, admin)
}

fn make_verified_business(
    env: &Env,
    client: &QuickLendXContractClient,
    admin: &Address,
) -> Address {
    let business = Address::generate(env);
    client.submit_kyc_application(&business, &String::from_str(env, "KYC"));
    client.verify_business(admin, &business);
    business
}

fn make_verified_investor(env: &Env, client: &QuickLendXContractClient, limit: i128) -> Address {
    let investor = Address::generate(env);
    client.submit_investor_kyc(&investor, &String::from_str(env, "Investor KYC"));
    client.verify_investor(&investor, &limit);
    investor
}

fn make_token(env: &Env, contract_id: &Address, business: &Address, investor: &Address) -> Address {
    let token_admin = Address::generate(env);
    let currency = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();
    let sac = token::StellarAssetClient::new(env, &currency);
    let tok = token::Client::new(env, &currency);
    sac.mint(business, &100_000i128);
    sac.mint(investor, &100_000i128);
    let exp = env.ledger().sequence() + 100_000;
    tok.approve(business, contract_id, &100_000i128, &exp);
    tok.approve(investor, contract_id, &100_000i128, &exp);
    currency
}

// ── MODULE 1: Invoice::is_overdue at the u64 boundary ────────────────────────

/// Invoice with `due_date == u64::MAX` is NOT overdue when `current_timestamp == u64::MAX`.
/// (`is_overdue` uses strict `>`, equality is not overdue.)
#[test]
fn invoice_not_overdue_when_due_date_equals_u64_max() {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());
    let business = Address::generate(&env);
    let currency = Address::generate(&env);

    // Stamp the ledger at NEAR_MAX so the invoice creation sees a valid future due_date.
    env.ledger().set_timestamp(NEAR_MAX);

    let invoice = env
        .as_contract(&contract_id, || {
            crate::invoice::Invoice::new(
                &env,
                business,
                1_000,
                currency,
                u64::MAX, // due_date == u64::MAX
                String::from_str(&env, "rollover test"),
                InvoiceCategory::Services,
                Vec::new(&env),
            )
        })
        .expect("invoice construction must succeed at NEAR_MAX");

    assert!(
        !invoice.is_overdue(u64::MAX),
        "invoice must not be overdue when current_timestamp == due_date == u64::MAX"
    );
}

/// Invoice with `due_date == u64::MAX` is NOT overdue when the clock wraps to 0.
///
/// After a u64 rollover the ledger resets to 0.  Because `0 < u64::MAX`,
/// the strict `>` comparison returns false — the contract is safe here.
#[test]
fn invoice_not_overdue_after_clock_rollover_to_zero_when_due_date_is_u64_max() {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());
    let business = Address::generate(&env);
    let currency = Address::generate(&env);

    env.ledger().set_timestamp(NEAR_MAX);
    let invoice = env
        .as_contract(&contract_id, || {
            crate::invoice::Invoice::new(
                &env,
                business,
                1_000,
                currency,
                u64::MAX,
                String::from_str(&env, "rollover test"),
                InvoiceCategory::Services,
                Vec::new(&env),
            )
        })
        .expect("invoice construction must succeed");

    // Post-rollover: clock resets to 0.  0 < u64::MAX → NOT overdue.
    assert!(
        !invoice.is_overdue(0),
        "after rollover to 0, is_overdue must return false for due_date == u64::MAX"
    );
}

/// Invoice with a small `due_date` IS overdue when the clock reaches `u64::MAX - 1`.
#[test]
fn invoice_is_overdue_at_u64_max_minus_one_when_due_date_is_small() {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());
    let business = Address::generate(&env);
    let currency = Address::generate(&env);

    // Create invoice with a normal due_date that is clearly in the past at NEAR_MAX.
    env.ledger().set_timestamp(1_000_000);
    let invoice = env
        .as_contract(&contract_id, || {
            crate::invoice::Invoice::new(
                &env,
                business,
                1_000,
                currency,
                2_000_000, // due_date well before NEAR_MAX
                String::from_str(&env, "small due date"),
                InvoiceCategory::Services,
                Vec::new(&env),
            )
        })
        .expect("invoice construction must succeed");

    assert!(
        invoice.is_overdue(NEAR_MAX),
        "invoice with small due_date must be overdue when current_timestamp == u64::MAX - 1"
    );
}

// ── MODULE 2: Invoice::grace_deadline saturation ──────────────────────────────

/// `grace_deadline` saturates to `u64::MAX` when `due_date` is near MAX and grace overflows.
#[test]
fn grace_deadline_saturates_at_u64_max_when_due_date_is_near_max() {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());
    let business = Address::generate(&env);
    let currency = Address::generate(&env);

    env.ledger().set_timestamp(NEAR_MAX);
    let invoice = env
        .as_contract(&contract_id, || {
            crate::invoice::Invoice::new(
                &env,
                business,
                1_000,
                currency,
                u64::MAX,
                String::from_str(&env, "grace saturation"),
                InvoiceCategory::Services,
                Vec::new(&env),
            )
        })
        .expect("invoice construction must succeed");

    // Adding any grace period to due_date == u64::MAX must saturate.
    assert_eq!(
        invoice.grace_deadline(1),
        u64::MAX,
        "grace_deadline(1) must equal u64::MAX (already at ceiling)"
    );
    assert_eq!(
        invoice.grace_deadline(SECONDS_PER_DAY),
        u64::MAX,
        "grace_deadline(1 day) must saturate to u64::MAX"
    );
    assert_eq!(
        invoice.grace_deadline(u64::MAX),
        u64::MAX,
        "grace_deadline(u64::MAX) must saturate to u64::MAX"
    );
}

/// `grace_deadline` saturates when `due_date == u64::MAX - 1` and grace period >= 2.
#[test]
fn grace_deadline_at_u64_max_minus_one_saturates_with_overflow_grace_period() {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());
    let business = Address::generate(&env);
    let currency = Address::generate(&env);

    // Set timestamp to NEAR_MAX - 1 so NEAR_MAX is a valid future due_date.
    env.ledger().set_timestamp(NEAR_MAX - 1);
    let invoice = env
        .as_contract(&contract_id, || {
            crate::invoice::Invoice::new(
                &env,
                business,
                1_000,
                currency,
                NEAR_MAX, // due_date == u64::MAX - 1
                String::from_str(&env, "near max due date"),
                InvoiceCategory::Services,
                Vec::new(&env),
            )
        })
        .expect("invoice construction must succeed");

    // Adding 1 reaches exactly u64::MAX (no saturation needed, exact).
    assert_eq!(
        invoice.grace_deadline(1),
        u64::MAX,
        "grace_deadline(1) at u64::MAX - 1 must yield u64::MAX"
    );
    // Adding 2 would overflow; must saturate.
    assert_eq!(
        invoice.grace_deadline(2),
        u64::MAX,
        "grace_deadline(2) at u64::MAX - 1 must saturate to u64::MAX"
    );
    assert_eq!(
        invoice.grace_deadline(u64::MAX),
        u64::MAX,
        "grace_deadline(u64::MAX) at u64::MAX - 1 must saturate to u64::MAX"
    );
}

// ── MODULE 3: Bid::default_expiration saturation ─────────────────────────────

/// `Bid::default_expiration(u64::MAX)` must saturate, not overflow.
#[test]
fn bid_default_expiration_saturates_when_now_is_u64_max() {
    let expiry = Bid::default_expiration(u64::MAX);
    assert_eq!(
        expiry,
        u64::MAX,
        "default_expiration must return u64::MAX when now == u64::MAX"
    );
}

/// `Bid::default_expiration(u64::MAX - 1)` must saturate to `u64::MAX`
/// because the default TTL (7 days × 86 400 s) exceeds the remaining headroom.
#[test]
fn bid_default_expiration_saturates_when_now_is_u64_max_minus_one() {
    let expiry = Bid::default_expiration(NEAR_MAX);
    assert_eq!(
        expiry,
        u64::MAX,
        "default_expiration must saturate to u64::MAX when now == u64::MAX - 1"
    );
}

// ── MODULE 4: Bid::is_expired pure-logic at the u64 boundary ─────────────────

/// A bid with `expiration_timestamp == u64::MAX` is NOT expired when
/// `current_timestamp == u64::MAX` (strict `>` semantics).
#[test]
fn bid_not_expired_when_expiration_and_clock_are_both_u64_max() {
    let env = Env::default();
    let bid = Bid {
        bid_id: BytesN::from_array(&env, &[0u8; 32]),
        invoice_id: BytesN::from_array(&env, &[1u8; 32]),
        investor: Address::generate(&env),
        bid_amount: 1_000,
        expected_return: 1_100,
        timestamp: NEAR_MAX,
        status: BidStatus::Placed,
        expiration_timestamp: u64::MAX,
    };

    assert!(
        !bid.is_expired(u64::MAX),
        "bid must not be expired when current_timestamp == expiration_timestamp == u64::MAX"
    );
}

/// A bid with `expiration_timestamp == u64::MAX` is NOT expired at `u64::MAX - 1`.
#[test]
fn bid_not_expired_at_u64_max_minus_one_when_expiration_is_u64_max() {
    let env = Env::default();
    let bid = Bid {
        bid_id: BytesN::from_array(&env, &[0u8; 32]),
        invoice_id: BytesN::from_array(&env, &[1u8; 32]),
        investor: Address::generate(&env),
        bid_amount: 1_000,
        expected_return: 1_100,
        timestamp: 0,
        status: BidStatus::Placed,
        expiration_timestamp: u64::MAX,
    };

    assert!(
        !bid.is_expired(NEAR_MAX),
        "bid must not be expired at u64::MAX - 1 when expiration_timestamp == u64::MAX"
    );
}

/// After a clock rollover the ledger resets to 0.  A bid with
/// `expiration_timestamp == u64::MAX` is NOT expired (0 < u64::MAX).
/// Documents the safe rollover invariant.
#[test]
fn bid_not_expired_after_clock_rollover_when_expiration_is_u64_max() {
    let env = Env::default();
    let bid = Bid {
        bid_id: BytesN::from_array(&env, &[0u8; 32]),
        invoice_id: BytesN::from_array(&env, &[1u8; 32]),
        investor: Address::generate(&env),
        bid_amount: 1_000,
        expected_return: 1_100,
        timestamp: 0,
        status: BidStatus::Placed,
        expiration_timestamp: u64::MAX,
    };

    // Post-rollover: clock is back at 0.  0 < u64::MAX → NOT expired.
    assert!(
        !bid.is_expired(0),
        "bid must not be expired when clock has rolled over to 0 and expiration is u64::MAX"
    );
}

/// A bid with a small `expiration_timestamp` IS expired when the clock is `u64::MAX - 1`.
#[test]
fn bid_is_expired_at_u64_max_minus_one_when_expiration_is_small() {
    let env = Env::default();
    let bid = Bid {
        bid_id: BytesN::from_array(&env, &[0u8; 32]),
        invoice_id: BytesN::from_array(&env, &[1u8; 32]),
        investor: Address::generate(&env),
        bid_amount: 1_000,
        expected_return: 1_100,
        timestamp: 0,
        status: BidStatus::Placed,
        expiration_timestamp: 1_000_000u64,
    };

    assert!(
        bid.is_expired(NEAR_MAX),
        "bid with small expiration_timestamp must be expired when clock == u64::MAX - 1"
    );
}

// ── MODULE 5: Contract-level invoice creation at the timestamp boundary ───────

/// Storing an invoice when the ledger is at `u64::MAX - 1` with
/// `due_date == u64::MAX` is accepted (strictly in the future).
#[test]
fn store_invoice_accepted_when_ledger_timestamp_is_u64_max_minus_one() {
    let (env, client, admin) = setup();
    let business = make_verified_business(&env, &client, &admin);
    let currency = Address::generate(&env);

    env.ledger().set_timestamp(NEAR_MAX);

    let result = client.try_store_invoice(
        &business,
        &1_000i128,
        &currency,
        &u64::MAX, // strictly after NEAR_MAX
        &String::from_str(&env, "boundary invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    assert!(
        result.is_ok(),
        "store_invoice must succeed when ledger == u64::MAX - 1 and due_date == u64::MAX"
    );
}

/// Storing an invoice with `due_date == ledger_timestamp == u64::MAX` is rejected
/// (due_date is NOT strictly in the future).
#[test]
fn store_invoice_rejected_when_due_date_equals_ledger_at_u64_max() {
    let (env, client, admin) = setup();
    let business = make_verified_business(&env, &client, &admin);
    let currency = Address::generate(&env);

    env.ledger().set_timestamp(u64::MAX);

    let result = client.try_store_invoice(
        &business,
        &1_000i128,
        &currency,
        &u64::MAX, // due_date == now == u64::MAX → not future
        &String::from_str(&env, "max timestamp invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    assert!(
        result.is_err(),
        "store_invoice must reject due_date == u64::MAX when ledger timestamp is also u64::MAX"
    );
}

/// An invoice created near `u64::MAX` with `due_date == u64::MAX` is NOT overdue
/// when the clock rolls over to 0 (0 < u64::MAX).
#[test]
fn invoice_created_near_u64_max_is_not_overdue_after_clock_rollover_to_zero() {
    let (env, client, admin) = setup();
    let business = make_verified_business(&env, &client, &admin);
    let currency = Address::generate(&env);

    // Create the invoice with ledger at NEAR_MAX.
    env.ledger().set_timestamp(NEAR_MAX);
    let invoice_id = client.store_invoice(
        &business,
        &1_000i128,
        &currency,
        &u64::MAX,
        &String::from_str(&env, "pre-rollover invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    // Simulate rollover: clock resets to 0.
    env.ledger().set_timestamp(0);

    let invoice = client.get_invoice(&invoice_id);
    // 0 < u64::MAX → NOT overdue.
    assert!(
        !invoice.is_overdue(0),
        "invoice with due_date == u64::MAX must NOT be overdue when clock rolls over to 0"
    );
}

/// `grace_deadline` of an invoice stored at `u64::MAX - 1` saturates correctly
/// regardless of the added grace period.
#[test]
fn grace_deadline_of_invoice_at_near_max_saturates_for_any_grace_period() {
    let (env, client, admin) = setup();
    let business = make_verified_business(&env, &client, &admin);
    let currency = Address::generate(&env);

    env.ledger().set_timestamp(NEAR_MAX);
    let invoice_id = client.store_invoice(
        &business,
        &1_000i128,
        &currency,
        &u64::MAX,
        &String::from_str(&env, "grace saturation invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    let invoice = client.get_invoice(&invoice_id);

    assert_eq!(
        invoice.grace_deadline(1),
        u64::MAX,
        "grace_deadline(1) must equal u64::MAX (due_date is already u64::MAX)"
    );
    assert_eq!(
        invoice.grace_deadline(u64::MAX),
        u64::MAX,
        "grace_deadline(u64::MAX) must saturate to u64::MAX"
    );
}

// ── MODULE 6: Bid expiry via contract at the timestamp boundary ───────────────

/// A bid placed when the ledger is at `u64::MAX - 1` has its
/// `expiration_timestamp` saturated to `u64::MAX` (not wrapped).
#[test]
fn bid_placed_at_u64_max_minus_one_has_expiration_saturated_to_u64_max() {
    let (env, client, admin) = setup();
    client.initialize_fee_system(&admin);

    env.ledger().set_timestamp(NEAR_MAX);

    let business = make_verified_business(&env, &client, &admin);
    let investor = make_verified_investor(&env, &client, 200_000);
    let contract_id = client.address.clone();
    let currency = make_token(&env, &contract_id, &business, &investor);

    let invoice_id = client.upload_invoice(
        &business,
        &10_000i128,
        &currency,
        &u64::MAX,
        &String::from_str(&env, "bid boundary invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    client.verify_invoice(&invoice_id);

    let bid_id = client.place_bid(
        &investor,
        &invoice_id,
        &5_000,
        &6_000,
        &BytesN::from_array(&env, &[0u8; 32]),
    );
    let bid = client.get_bid(&bid_id).unwrap();

    assert_eq!(
        bid.expiration_timestamp,
        u64::MAX,
        "bid expiration must saturate to u64::MAX when placed at u64::MAX - 1"
    );
    // At u64::MAX the bid is still within its window (strict > semantics).
    assert!(
        !bid.is_expired(u64::MAX),
        "bid must not be expired at its own saturated expiration timestamp"
    );
}

/// Cleanup does NOT remove a bid whose expiration saturated to `u64::MAX`
/// when the clock is also at `u64::MAX` (exact boundary, strict > is false).
#[test]
fn cleanup_does_not_remove_bid_whose_expiration_saturated_to_u64_max() {
    let (env, client, admin) = setup();
    client.initialize_fee_system(&admin);

    env.ledger().set_timestamp(NEAR_MAX);

    let business = make_verified_business(&env, &client, &admin);
    let investor = make_verified_investor(&env, &client, 200_000);
    let contract_id = client.address.clone();
    let currency = make_token(&env, &contract_id, &business, &investor);

    let invoice_id = client.upload_invoice(
        &business,
        &10_000i128,
        &currency,
        &u64::MAX,
        &String::from_str(&env, "cleanup boundary invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    client.verify_invoice(&invoice_id);

    let bid_id = client.place_bid(
        &investor,
        &invoice_id,
        &5_000,
        &6_000,
        &BytesN::from_array(&env, &[0u8; 32]),
    );
    let bid = client.get_bid(&bid_id).unwrap();
    assert_eq!(bid.expiration_timestamp, u64::MAX);

    // Advance to u64::MAX; bid is at its exact boundary.
    env.ledger().set_timestamp(u64::MAX);
    let removed = client.cleanup_expired_bids(&invoice_id);
    assert_eq!(
        removed, 0,
        "cleanup must not remove a bid at expiration_timestamp == clock == u64::MAX"
    );

    let bid_after = client.get_bid(&bid_id).unwrap();
    assert_eq!(
        bid_after.status,
        BidStatus::Placed,
        "bid status must remain Placed after cleanup at the exact u64::MAX boundary"
    );
}
