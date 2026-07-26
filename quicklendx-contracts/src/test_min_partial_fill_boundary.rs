//! Boundary tests for the min-partial-fill (minimum bid) amount — Issue #1891.
//!
//! The "min-partial-fill amount" is the smallest `bid_amount` a `place_bid`
//! call is allowed to carry.  It is computed as:
//!
//! ```text
//! effective_min = max(min_bid_amount, invoice_amount * min_bid_bps / 10_000)
//! ```
//!
//! where `min_bid_amount` and `min_bid_bps` come from the on-chain
//! `ProtocolLimits` (defaults: 10 and 100 bps / 1 % respectively).
//!
//! # Coverage matrix
//!
//! | Scenario                               | bid_amount            | Expected           |
//! |----------------------------------------|-----------------------|--------------------|
//! | absolute floor dominates – at limit    | `min_bid_amount`      | Ok (bid placed)    |
//! | absolute floor dominates – one below   | `min_bid_amount - 1`  | Err InvalidAmount  |
//! | absolute floor dominates – one above   | `min_bid_amount + 1`  | Ok (bid placed)    |
//! | percentage floor dominates – at limit  | `pct_min`             | Ok (bid placed)    |
//! | percentage floor dominates – one below | `pct_min - 1`         | Err InvalidAmount  |
//! | percentage floor dominates – one above | `pct_min + 1`         | Ok (bid placed)    |
//! | validate_bid unit – at limit           | `effective_min`       | Ok                 |
//! | validate_bid unit – one below          | `effective_min - 1`   | Err InvalidAmount  |
//! | validate_bid unit – one above          | `effective_min + 1`   | Ok                 |
//! | admin raises floor – old value         | old floor             | Err InvalidAmount  |
//! | admin raises floor – new value         | new floor             | Ok (bid placed)    |
//!
//! All tests are deterministic and run on every CI matrix entry (no feature
//! gate, no `Date::now`, no random-number calls).

#![cfg(test)]

use crate::errors::QuickLendXError;
use crate::invoice::InvoiceCategory;
use crate::protocol_limits::{
    compute_min_bid_amount, DEFAULT_MIN_BID_AMOUNT, DEFAULT_MIN_BID_BPS,
};
use crate::verification::validate_bid;
use crate::{QuickLendXContract, QuickLendXContractClient};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, BytesN, Env, String, Vec,
};

// ---------------------------------------------------------------------------
// Shared test helpers
// ---------------------------------------------------------------------------

/// Creates an `Env` with a fixed ledger timestamp, registers the contract,
/// initialises an admin, and returns `(env, client, admin)`.
fn build_env() -> (Env, QuickLendXContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    // Fixed timestamp so all due-date calculations are deterministic.
    env.ledger().with_mut(|l| l.timestamp = 1_000_000);
    let id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &id);
    let admin = Address::generate(&env);
    client.set_admin(&admin);
    (env, client, admin)
}

/// Whitelists a fresh currency address, KYC-verifies `business`, and uploads
/// + verifies an invoice for `invoice_amount`. Returns `(invoice_id, business,
/// currency)`.
fn setup_verified_invoice(
    env: &Env,
    client: &QuickLendXContractClient,
    admin: &Address,
    invoice_amount: i128,
) -> (BytesN<32>, Address, Address) {
    let business = Address::generate(env);
    client.submit_kyc_application(&business, &String::from_str(env, "kyc-b"));
    client.verify_business(admin, &business);

    let currency = Address::generate(env);
    client.add_currency(admin, &currency);

    let due_date = env.ledger().timestamp() + 86_400 * 30;
    let invoice_id = client.upload_invoice(
        &business,
        &invoice_amount,
        &currency,
        &due_date,
        &String::from_str(env, "test invoice"),
        &InvoiceCategory::Services,
        &Vec::new(env),
        &None);
    client.verify_invoice(&invoice_id);
    (invoice_id, business, currency)
}

/// KYC-verifies a fresh investor with the given `investment_limit` and returns
/// their address.  (`verify_investor` reads the admin from on-chain storage.)
fn setup_verified_investor(
    env: &Env,
    client: &QuickLendXContractClient,
    investment_limit: i128,
) -> Address {
    let investor = Address::generate(env);
    client.submit_investor_kyc(&investor, &String::from_str(env, "kyc-i"));
    client.verify_investor(&investor, &investment_limit);
    investor
}

/// Returns an all-zeros `BytesN<32>` salt.  Each test gets its own `Env` so
/// there is no cross-test collision on the idempotency key.
fn zero_salt(env: &Env) -> BytesN<32> {
    BytesN::from_array(env, &[0u8; 32])
}

// ---------------------------------------------------------------------------
// Section 1 — absolute floor dominates
//
// DEFAULT_MIN_BID_AMOUNT = 10, DEFAULT_MIN_BID_BPS = 100 (1 %).
// invoice_amount = 500 → pct_min = 500 * 100 / 10_000 = 5 < 10
//                      → effective_min = 10  (absolute floor wins)
// ---------------------------------------------------------------------------

/// Placing a bid exactly at the absolute minimum floor is accepted.
#[test]
fn bid_at_absolute_floor_is_accepted() {
    let (env, client, admin) = build_env();
    let invoice_amount = 500i128;
    let (invoice_id, _, _) = setup_verified_invoice(&env, &client, &admin, invoice_amount);
    let investor = setup_verified_investor(&env, &client, 10_000);

    let effective_min = DEFAULT_MIN_BID_AMOUNT; // 10
    let result = client.try_place_bid(
        &investor,
        &invoice_id,
        &effective_min,
        &(invoice_amount + 1),
        &zero_salt(&env),
    );
    assert!(
        result.is_ok(),
        "bid_amount == effective_min ({effective_min}) should be accepted; got {:?}",
        result.err()
    );
}

/// Placing a bid one unit below the absolute minimum floor is rejected.
#[test]
fn bid_one_below_absolute_floor_is_rejected() {
    let (env, client, admin) = build_env();
    let invoice_amount = 500i128;
    let (invoice_id, _, _) = setup_verified_invoice(&env, &client, &admin, invoice_amount);
    let investor = setup_verified_investor(&env, &client, 10_000);

    let one_below = DEFAULT_MIN_BID_AMOUNT - 1; // 9
    let result = client.try_place_bid(
        &investor,
        &invoice_id,
        &one_below,
        &(invoice_amount + 1),
        &zero_salt(&env),
    );
    assert!(
        result.is_err(),
        "bid_amount == effective_min - 1 ({one_below}) should be rejected"
    );
    assert_eq!(
        result.unwrap_err().expect("expected contract error"),
        QuickLendXError::InvalidAmount,
        "rejection must carry InvalidAmount"
    );
}

/// Placing a bid one unit above the absolute minimum floor is accepted.
#[test]
fn bid_one_above_absolute_floor_is_accepted() {
    let (env, client, admin) = build_env();
    let invoice_amount = 500i128;
    let (invoice_id, _, _) = setup_verified_invoice(&env, &client, &admin, invoice_amount);
    let investor = setup_verified_investor(&env, &client, 10_000);

    let one_above = DEFAULT_MIN_BID_AMOUNT + 1; // 11
    let result = client.try_place_bid(
        &investor,
        &invoice_id,
        &one_above,
        &(invoice_amount + 1),
        &zero_salt(&env),
    );
    assert!(
        result.is_ok(),
        "bid_amount == effective_min + 1 ({one_above}) should be accepted; got {:?}",
        result.err()
    );
}

// ---------------------------------------------------------------------------
// Section 2 — percentage floor dominates
//
// invoice_amount = 5_000 → pct_min = 5_000 * 100 / 10_000 = 50 > 10
//                        → effective_min = 50  (percentage floor wins)
// ---------------------------------------------------------------------------

/// Placing a bid exactly at the percentage-based minimum floor is accepted.
#[test]
fn bid_at_percentage_floor_is_accepted() {
    let (env, client, admin) = build_env();
    let invoice_amount = 5_000i128;
    let (invoice_id, _, _) = setup_verified_invoice(&env, &client, &admin, invoice_amount);
    let investor = setup_verified_investor(&env, &client, 100_000);

    // pct_min = 5_000 * 100 / 10_000 = 50
    let pct_min = invoice_amount
        .saturating_mul(DEFAULT_MIN_BID_BPS as i128)
        .saturating_div(10_000);
    let result = client.try_place_bid(
        &investor,
        &invoice_id,
        &pct_min,
        &(invoice_amount + 1),
        &zero_salt(&env),
    );
    assert!(
        result.is_ok(),
        "bid_amount == pct_min ({pct_min}) should be accepted; got {:?}",
        result.err()
    );
}

/// Placing a bid one unit below the percentage-based minimum floor is rejected.
#[test]
fn bid_one_below_percentage_floor_is_rejected() {
    let (env, client, admin) = build_env();
    let invoice_amount = 5_000i128;
    let (invoice_id, _, _) = setup_verified_invoice(&env, &client, &admin, invoice_amount);
    let investor = setup_verified_investor(&env, &client, 100_000);

    let pct_min = invoice_amount
        .saturating_mul(DEFAULT_MIN_BID_BPS as i128)
        .saturating_div(10_000); // 50
    let one_below = pct_min - 1; // 49
    let result = client.try_place_bid(
        &investor,
        &invoice_id,
        &one_below,
        &(invoice_amount + 1),
        &zero_salt(&env),
    );
    assert!(
        result.is_err(),
        "bid_amount == pct_min - 1 ({one_below}) should be rejected"
    );
    assert_eq!(
        result.unwrap_err().expect("expected contract error"),
        QuickLendXError::InvalidAmount,
        "rejection must carry InvalidAmount"
    );
}

/// Placing a bid one unit above the percentage-based minimum floor is accepted.
#[test]
fn bid_one_above_percentage_floor_is_accepted() {
    let (env, client, admin) = build_env();
    let invoice_amount = 5_000i128;
    let (invoice_id, _, _) = setup_verified_invoice(&env, &client, &admin, invoice_amount);
    let investor = setup_verified_investor(&env, &client, 100_000);

    let pct_min = invoice_amount
        .saturating_mul(DEFAULT_MIN_BID_BPS as i128)
        .saturating_div(10_000); // 50
    let one_above = pct_min + 1; // 51
    let result = client.try_place_bid(
        &investor,
        &invoice_id,
        &one_above,
        &(invoice_amount + 1),
        &zero_salt(&env),
    );
    assert!(
        result.is_ok(),
        "bid_amount == pct_min + 1 ({one_above}) should be accepted; got {:?}",
        result.err()
    );
}

// ---------------------------------------------------------------------------
// Section 3 — pure-logic unit tests via `validate_bid` and
//             `compute_min_bid_amount` (no token transfers needed)
// ---------------------------------------------------------------------------

/// `compute_min_bid_amount` returns the absolute floor when pct_min < absolute.
#[test]
fn compute_min_bid_amount_returns_absolute_floor_when_dominant() {
    use crate::protocol_limits::ProtocolLimits;
    // Build limits directly — no contract storage needed.
    let limits = ProtocolLimits {
        min_invoice_amount: 10,
        min_bid_amount: DEFAULT_MIN_BID_AMOUNT, // 10
        min_bid_bps: DEFAULT_MIN_BID_BPS,       // 100 (1%)
        max_due_date_days: 365,
        grace_period_seconds: 604_800,
        max_invoices_per_business: 100,
        min_investor_tier: crate::verification::InvestorTier::Basic,
    };
    // 500 * 1 % = 5 < 10 → absolute floor wins
    let result = compute_min_bid_amount(500, &limits);
    assert_eq!(
        result,
        DEFAULT_MIN_BID_AMOUNT,
        "absolute floor must dominate when pct_min < min_bid_amount"
    );
}

/// `compute_min_bid_amount` returns the percentage floor when it is larger.
#[test]
fn compute_min_bid_amount_returns_percentage_floor_when_dominant() {
    use crate::protocol_limits::ProtocolLimits;
    let limits = ProtocolLimits {
        min_invoice_amount: 10,
        min_bid_amount: DEFAULT_MIN_BID_AMOUNT, // 10
        min_bid_bps: DEFAULT_MIN_BID_BPS,       // 100 (1%)
        max_due_date_days: 365,
        grace_period_seconds: 604_800,
        max_invoices_per_business: 100,
        min_investor_tier: crate::verification::InvestorTier::Basic,
    };
    // 5_000 * 1 % = 50 > 10 → percentage floor wins
    let result = compute_min_bid_amount(5_000, &limits);
    let expected = 5_000i128
        .saturating_mul(DEFAULT_MIN_BID_BPS as i128)
        .saturating_div(10_000);
    assert_eq!(
        result, expected,
        "percentage floor must dominate when pct_min > min_bid_amount"
    );
}

/// `validate_bid` passes when `bid_amount == effective_min`.
#[test]
fn validate_bid_passes_at_effective_minimum() {
    let (env, client, admin) = build_env();
    let invoice_amount = 500i128;
    let (invoice_id, _, _) = setup_verified_invoice(&env, &client, &admin, invoice_amount);
    let investor = setup_verified_investor(&env, &client, 10_000);

    // effective_min = DEFAULT_MIN_BID_AMOUNT = 10 (absolute floor dominates at 500)
    let effective_min = DEFAULT_MIN_BID_AMOUNT;
    let invoice = client.get_invoice(&invoice_id);

    // validate_bid reads contract storage, so we call it inside the contract context.
    let result = env.as_contract(&client.address, || {
        validate_bid(&env, &invoice, effective_min, invoice_amount + 1, &investor)
    });
    assert!(
        result.is_ok(),
        "validate_bid must accept bid_amount == effective_min ({effective_min})"
    );
}

/// `validate_bid` rejects when `bid_amount == effective_min - 1`.
#[test]
fn validate_bid_rejects_one_below_effective_minimum() {
    let (env, client, admin) = build_env();
    let invoice_amount = 500i128;
    let (invoice_id, _, _) = setup_verified_invoice(&env, &client, &admin, invoice_amount);
    let investor = setup_verified_investor(&env, &client, 10_000);

    let one_below = DEFAULT_MIN_BID_AMOUNT - 1; // 9
    let invoice = client.get_invoice(&invoice_id);

    let result = env.as_contract(&client.address, || {
        validate_bid(&env, &invoice, one_below, invoice_amount + 1, &investor)
    });
    assert!(
        result.is_err(),
        "validate_bid must reject bid_amount == effective_min - 1 ({one_below})"
    );
    assert_eq!(
        result.unwrap_err(),
        QuickLendXError::InvalidAmount,
        "error must be InvalidAmount"
    );
}

/// `validate_bid` passes when `bid_amount == effective_min + 1`.
#[test]
fn validate_bid_passes_one_above_effective_minimum() {
    let (env, client, admin) = build_env();
    let invoice_amount = 500i128;
    let (invoice_id, _, _) = setup_verified_invoice(&env, &client, &admin, invoice_amount);
    let investor = setup_verified_investor(&env, &client, 10_000);

    let one_above = DEFAULT_MIN_BID_AMOUNT + 1; // 11
    let invoice = client.get_invoice(&invoice_id);

    let result = env.as_contract(&client.address, || {
        validate_bid(&env, &invoice, one_above, invoice_amount + 1, &investor)
    });
    assert!(
        result.is_ok(),
        "validate_bid must accept bid_amount == effective_min + 1 ({one_above})"
    );
}

// ---------------------------------------------------------------------------
// Section 4 — boundary shifts when admin updates `min_bid_amount`
//
// Verifies the contract enforces the *live* on-chain limit, not a
// compile-time constant.
// ---------------------------------------------------------------------------

/// After admin raises `min_bid_amount` to 100, the old passing value (10) is
/// rejected.
#[test]
fn bid_at_old_floor_rejected_after_admin_raises_min_bid() {
    let (env, client, admin) = build_env();
    let invoice_amount = 500i128; // pct_min = 5; initial absolute floor = 10
    let (invoice_id, _, _) = setup_verified_invoice(&env, &client, &admin, invoice_amount);
    let investor = setup_verified_investor(&env, &client, 10_000);

    // Raise min_bid_amount to 100.
    let update_result = client.try_update_minimum_bid(&admin, &100i128);
    assert!(update_result.is_ok(), "update_minimum_bid must succeed");

    // The old floor value (10) is now below the new minimum → must fail.
    let result = client.try_place_bid(
        &investor,
        &invoice_id,
        &10i128,
        &(invoice_amount + 1),
        &zero_salt(&env),
    );
    assert!(
        result.is_err(),
        "after raising min_bid_amount to 100, bidding 10 must be rejected"
    );
    assert_eq!(
        result.unwrap_err().expect("expected contract error"),
        QuickLendXError::InvalidAmount,
    );
}

/// After admin raises `min_bid_amount` to 100, bidding exactly 100 is accepted.
#[test]
fn bid_at_new_floor_accepted_after_admin_raises_min_bid() {
    let (env, client, admin) = build_env();
    let invoice_amount = 500i128;
    let (invoice_id, _, _) = setup_verified_invoice(&env, &client, &admin, invoice_amount);
    let investor = setup_verified_investor(&env, &client, 10_000);

    client
        .try_update_minimum_bid(&admin, &100i128)
        .expect("update_minimum_bid must succeed");

    let result = client.try_place_bid(
        &investor,
        &invoice_id,
        &100i128,
        &(invoice_amount + 1),
        &zero_salt(&env),
    );
    assert!(
        result.is_ok(),
        "after raising min_bid_amount to 100, bidding 100 must be accepted; got {:?}",
        result.err()
    );
}
