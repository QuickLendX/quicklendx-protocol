//! Tests for issue #1830 - `bid_expiry_grace_seconds` gating permissionless
//! bid cleanup (auto-cancellation of stale bids).
//!
//! Validates:
//! - Default grace period is 0, matching the pre-existing immediate-cleanup
//!   behaviour byte-for-byte (no regression for callers that never configure
//!   the new knob).
//! - A bid past its raw TTL is *not* cleaned up while still inside a
//!   configured grace window (`Placed -> Expired` transition is deferred).
//! - The same bid becomes cleanable once the grace window has also elapsed.
//! - Acceptance (`accept_bid`) is unaffected by the grace period: a bid past
//!   its raw TTL is rejected immediately, regardless of grace.
//! - `set_bid_expiry_grace_seconds` enforces `[0, MAX_BID_EXPIRY_GRACE_SECONDS]`
//!   and is admin-only.
//! - `get_bid_expiry_grace_config` reports bounds, default, and `is_custom`.
//! - `reset_bid_grace_to_default` restores the default and clears
//!   `is_custom`.

#![cfg(test)]

use crate::bid::{
    BidStatus, DEFAULT_BID_EXPIRY_GRACE_SECONDS, MAX_BID_EXPIRY_GRACE_SECONDS, MIN_BID_TTL_DAYS,
};
use crate::errors::QuickLendXError;
use crate::invoice::InvoiceCategory;
use crate::{QuickLendXContract, QuickLendXContractClient};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, BytesN, Env, String, Vec,
};

const SECONDS_PER_DAY: u64 = 86_400;

fn setup() -> (Env, QuickLendXContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000_000);
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.set_admin(&admin);
    client.initialize_fee_system(&admin);
    (env, client, admin)
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
    sac.mint(contract_id, &1i128);
    let exp = env.ledger().sequence() + 100_000;
    tok.approve(business, contract_id, &400_000i128, &exp);
    tok.approve(investor, contract_id, &400_000i128, &exp);
    currency
}

fn funded_setup(
    env: &Env,
    client: &QuickLendXContractClient,
    admin: &Address,
    amount: i128,
) -> (Address, Address, BytesN<32>) {
    let business = Address::generate(env);
    let investor = Address::generate(env);
    let contract_id = client.address.clone();
    let currency = make_token(env, &contract_id, &business, &investor);

    client.submit_kyc_application(&business, &String::from_str(env, "KYC"));
    client.verify_business(admin, &business);
    client.submit_investor_kyc(&investor, &String::from_str(env, "KYC"));
    client.verify_investor(&investor, &200_000i128);

    let due_date = env.ledger().timestamp() + 30 * SECONDS_PER_DAY;
    let invoice_id = client.upload_invoice(
        &business,
        &amount,
        &currency,
        &due_date,
        &String::from_str(env, "Test invoice"),
        &InvoiceCategory::Services,
        &Vec::new(env),
    );
    client.verify_invoice(&invoice_id);
    (business, investor, invoice_id)
}

fn place_test_bid(
    env: &Env,
    client: &QuickLendXContractClient,
    investor: &Address,
    invoice_id: &BytesN<32>,
) -> BytesN<32> {
    client.place_bid(
        investor,
        invoice_id,
        &5_000,
        &6_000,
        &BytesN::from_array(env, &[0u8; 32]),
    )
}

#[test]
fn default_grace_period_is_zero() {
    let (env, client, _admin) = setup();
    assert_eq!(client.get_bid_expiry_grace_seconds(), 0);
    assert_eq!(DEFAULT_BID_EXPIRY_GRACE_SECONDS, 0);

    let config = client.get_bid_expiry_grace_config();
    assert_eq!(config.current_seconds, 0);
    assert_eq!(config.default_seconds, 0);
    assert_eq!(config.min_seconds, 0);
    assert_eq!(config.max_seconds, MAX_BID_EXPIRY_GRACE_SECONDS);
    assert!(!config.is_custom);

    let _ = env;
}

/// With the default (zero) grace period, cleanup still fires immediately at
/// raw expiry - this is the "no regression" guarantee for existing callers.
#[test]
fn cleanup_fires_immediately_when_grace_is_default_zero() {
    let (env, client, admin) = setup();
    client.set_bid_ttl_days(&MIN_BID_TTL_DAYS);
    let (_, investor, invoice_id) = funded_setup(&env, &client, &admin, 10_000);
    let bid_id = place_test_bid(&env, &client, &investor, &invoice_id);
    let bid = client.get_bid(&bid_id).unwrap();

    env.ledger().set_timestamp(bid.expiration_timestamp + 1);

    let cleaned = client.cleanup_expired_bids(&invoice_id);
    assert_eq!(
        cleaned, 1,
        "zero grace must behave like pre-existing immediate cleanup"
    );

    let bid_after = client.get_bid(&bid_id).unwrap();
    assert_eq!(bid_after.status, BidStatus::Expired);
}

/// A bid past its raw TTL is not cleaned up while still within a configured
/// grace window: the `Placed -> Expired` transition is deferred.
#[test]
fn bid_is_not_cleaned_up_within_grace_window() {
    let (env, client, admin) = setup();
    client.set_bid_ttl_days(&MIN_BID_TTL_DAYS);
    let grace_seconds: u64 = 1_000;
    client.set_bid_expiry_grace_seconds(&grace_seconds);

    let (_, investor, invoice_id) = funded_setup(&env, &client, &admin, 10_000);
    let bid_id = place_test_bid(&env, &client, &investor, &invoice_id);
    let bid = client.get_bid(&bid_id).unwrap();

    // Past raw expiry, but still inside the grace window.
    env.ledger().set_timestamp(bid.expiration_timestamp + 1);

    let cleaned = client.cleanup_expired_bids(&invoice_id);
    assert_eq!(
        cleaned, 0,
        "bid must not be cleaned up while inside grace window"
    );

    let bid_after = client.get_bid(&bid_id).unwrap();
    assert_eq!(bid_after.status, BidStatus::Placed);
}

/// Once the grace window has also elapsed on top of the raw TTL, cleanup
/// transitions the bid to `Expired` as usual.
#[test]
fn bid_is_cleaned_up_after_grace_window_elapses() {
    let (env, client, admin) = setup();
    client.set_bid_ttl_days(&MIN_BID_TTL_DAYS);
    let grace_seconds: u64 = 1_000;
    client.set_bid_expiry_grace_seconds(&grace_seconds);

    let (_, investor, invoice_id) = funded_setup(&env, &client, &admin, 10_000);
    let bid_id = place_test_bid(&env, &client, &investor, &invoice_id);
    let bid = client.get_bid(&bid_id).unwrap();

    // Still within grace: no-op.
    env.ledger().set_timestamp(bid.expiration_timestamp + 1);
    assert_eq!(client.cleanup_expired_bids(&invoice_id), 0);

    // Now past expiration + grace: cleanup must act.
    env.ledger()
        .set_timestamp(bid.expiration_timestamp + grace_seconds);
    let cleaned = client.cleanup_expired_bids(&invoice_id);
    assert_eq!(
        cleaned, 1,
        "bid must be cleaned up once grace window elapses"
    );

    let bid_after = client.get_bid(&bid_id).unwrap();
    assert_eq!(bid_after.status, BidStatus::Expired);
}

/// The grace period never resurrects acceptability: a bid past its raw TTL
/// is rejected by `accept_bid` immediately, even while a grace period would
/// otherwise keep it un-cleaned-up in storage.
#[test]
fn accept_bid_still_rejects_bid_past_raw_ttl_during_grace_window() {
    let (env, client, admin) = setup();
    client.set_bid_ttl_days(&MIN_BID_TTL_DAYS);
    client.set_bid_expiry_grace_seconds(&10_000u64);

    let (_business, investor, invoice_id) = funded_setup(&env, &client, &admin, 10_000);
    let bid_id = place_test_bid(&env, &client, &investor, &invoice_id);
    let bid = client.get_bid(&bid_id).unwrap();

    // Past raw expiry, well within the grace window.
    env.ledger().set_timestamp(bid.expiration_timestamp + 1);

    let bid_still_placed = client.get_bid(&bid_id).unwrap();
    assert_eq!(bid_still_placed.status, BidStatus::Placed);

    let result = client.try_accept_bid(&invoice_id, &bid_id);
    assert!(
        result.is_err(),
        "accept_bid must reject a bid past raw TTL regardless of grace period"
    );
}

#[test]
fn set_bid_expiry_grace_seconds_rejects_value_above_maximum() {
    let (_env, client, _admin) = setup();
    let result = client.try_set_bid_expiry_grace_seconds(&(MAX_BID_EXPIRY_GRACE_SECONDS + 1));
    assert!(result.is_err());
    let err = result
        .err()
        .unwrap()
        .expect("expected contract error, got host error");
    assert_eq!(err, QuickLendXError::InvalidTimestamp);
}

#[test]
fn set_bid_expiry_grace_seconds_accepts_boundary_values() {
    let (_env, client, _admin) = setup();
    assert_eq!(client.set_bid_expiry_grace_seconds(&0u64), 0);
    assert_eq!(
        client.set_bid_expiry_grace_seconds(&MAX_BID_EXPIRY_GRACE_SECONDS),
        MAX_BID_EXPIRY_GRACE_SECONDS
    );
}

#[test]
fn get_bid_expiry_grace_config_reports_custom_after_set_and_clears_after_reset() {
    let (_env, client, admin) = setup();

    client.set_bid_expiry_grace_seconds(&5_000u64);
    let config = client.get_bid_expiry_grace_config();
    assert_eq!(config.current_seconds, 5_000);
    assert!(config.is_custom);

    client.reset_bid_grace_to_default(&admin);
    let config_after_reset = client.get_bid_expiry_grace_config();
    assert_eq!(
        config_after_reset.current_seconds,
        DEFAULT_BID_EXPIRY_GRACE_SECONDS
    );
    assert!(!config_after_reset.is_custom);
}

/// Calling `set_bid_expiry_grace_seconds` before any admin has been
/// configured for the contract must fail with `NotAdmin`.
#[test]
fn non_admin_cannot_set_bid_expiry_grace_seconds() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let result = client.try_set_bid_expiry_grace_seconds(&1_000u64);
    assert!(result.is_err());
    let err = result
        .err()
        .unwrap()
        .expect("expected contract error, got host error");
    assert_eq!(err, QuickLendXError::NotAdmin);
}
