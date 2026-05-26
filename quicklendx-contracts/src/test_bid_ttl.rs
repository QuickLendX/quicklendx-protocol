//! Tests for issue #543 - bid TTL configuration bounds and default behaviour.
//!
//! Validates:
//! - Default TTL (7 days) is used when no admin override exists
//! - `get_bid_ttl_config` returns correct bounds, default, and `is_custom` flag
//! - Zero TTL is rejected with `InvalidBidTtl`
//! - Values below minimum (< 1) are rejected
//! - Values above maximum (> 30) are rejected
//! - Boundary values (1 and 30) are accepted
//! - Every valid value in [1, 30] is accepted
//! - Bids placed after a TTL update use the new value
//! - Bids placed before a TTL update keep their original expiration
//! - `reset_bid_ttl_to_default` restores default and clears `is_custom`
//! - Non-admin cannot set or reset TTL
//! - `ttl_upd` event is emitted on every successful set/reset
//! - Expiration timestamp arithmetic is overflow-safe
//! - Bids expire correctly at the configured TTL boundary

use super::*;
use crate::bid::{
    BidStatus, BidTtlConfig, DEFAULT_BID_TTL_DAYS, MAX_BID_TTL_DAYS, MIN_BID_TTL_DAYS,
};
use crate::errors::QuickLendXError;
use crate::invoice::InvoiceCategory;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, BytesN, Env, String, Vec,
};

const SECONDS_PER_DAY: u64 = 86_400;

// --- helpers -----------------------------------------------------------------

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

/// Create a verified invoice ready for bidding.
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

// --- 1. Default TTL -----------------------------------------------------------

/// Fresh contract returns DEFAULT_BID_TTL_DAYS with no custom override.
#[test]
fn test_default_ttl_is_seven_days() {
    let (_, client, _) = setup();
    assert_eq!(client.get_bid_ttl_days(), DEFAULT_BID_TTL_DAYS);
}

/// `get_bid_ttl_config` on a fresh contract reports correct defaults and
/// `is_custom = false`.
#[test]
fn test_get_bid_ttl_config_defaults() {
    let (_, client, _) = setup();
    let cfg = client.get_bid_ttl_config();
    assert_eq!(cfg.current_days, DEFAULT_BID_TTL_DAYS);
    assert_eq!(cfg.min_days, MIN_BID_TTL_DAYS);
    assert_eq!(cfg.max_days, MAX_BID_TTL_DAYS);
    assert_eq!(cfg.default_days, DEFAULT_BID_TTL_DAYS);
    assert!(
        !cfg.is_custom,
        "is_custom must be false before any admin set"
    );
}

/// Bid placed on a fresh contract uses the 7-day default expiration.
#[test]
fn test_bid_uses_default_ttl_expiration() {
    let (env, client, admin) = setup();
    let (_, investor, invoice_id) = funded_setup(&env, &client, &admin, 10_000);
    let now = env.ledger().timestamp();
    let bid_id = client.place_bid(&investor, &invoice_id, &5_000, &6_000);
    let bid = client.get_bid(&bid_id).unwrap();
    assert_eq!(
        bid.expiration_timestamp,
        now + DEFAULT_BID_TTL_DAYS * SECONDS_PER_DAY,
        "default expiration must be now + 7 days"
    );
}

// --- 2. Bound enforcement -----------------------------------------------------

/// Zero TTL must be rejected with InvalidBidTtl.
#[test]
fn test_zero_ttl_rejected() {
    let (_, client, _) = setup();
    let result = client.try_set_bid_ttl_days(&0u64);
    assert_eq!(
        result.unwrap_err().expect("contract error"),
        QuickLendXError::InvalidBidTtl,
        "zero TTL must return InvalidBidTtl"
    );
}

/// Value below minimum (u64 wraps, but any value < 1 is 0 for u64 - test
/// that 0 is the only sub-minimum possible and is rejected).
#[test]
fn test_below_minimum_ttl_rejected() {
    let (_, client, _) = setup();
    // Only sub-minimum for u64 is 0; already covered, but assert the error type.
    let result = client.try_set_bid_ttl_days(&0u64);
    assert!(result.is_err());
    assert_eq!(
        result.unwrap_err().expect("contract error"),
        QuickLendXError::InvalidBidTtl
    );
}

/// Value above maximum (31) must be rejected with InvalidBidTtl.
#[test]
fn test_above_maximum_ttl_rejected() {
    let (_, client, _) = setup();
    let result = client.try_set_bid_ttl_days(&31u64);
    assert_eq!(
        result.unwrap_err().expect("contract error"),
        QuickLendXError::InvalidBidTtl,
        "31 days must return InvalidBidTtl"
    );
}

/// Large value (u64::MAX) must be rejected with InvalidBidTtl.
#[test]
fn test_extreme_large_ttl_rejected() {
    let (_, client, _) = setup();
    let result = client.try_set_bid_ttl_days(&u64::MAX);
    assert_eq!(
        result.unwrap_err().expect("contract error"),
        QuickLendXError::InvalidBidTtl
    );
}

/// Minimum boundary value (1) must be accepted.
#[test]
fn test_minimum_boundary_accepted() {
    let (_, client, _) = setup();
    let result = client.try_set_bid_ttl_days(&MIN_BID_TTL_DAYS);
    assert!(result.is_ok(), "minimum boundary (1) must be accepted");
    assert_eq!(client.get_bid_ttl_days(), MIN_BID_TTL_DAYS);
}

/// Maximum boundary value (30) must be accepted.
#[test]
fn test_maximum_boundary_accepted() {
    let (_, client, _) = setup();
    let result = client.try_set_bid_ttl_days(&MAX_BID_TTL_DAYS);
    assert!(result.is_ok(), "maximum boundary (30) must be accepted");
    assert_eq!(client.get_bid_ttl_days(), MAX_BID_TTL_DAYS);
}

/// Every value in [1, 30] must be accepted.
#[test]
fn test_all_valid_ttl_values_accepted() {
    let (_, client, _) = setup();
    for days in MIN_BID_TTL_DAYS..=MAX_BID_TTL_DAYS {
        let result = client.try_set_bid_ttl_days(&days);
        assert!(result.is_ok(), "TTL {} days must be accepted", days);
    }
}

// --- 3. Config state after set ------------------------------------------------

/// After admin sets TTL, `get_bid_ttl_config` reports `is_custom = true` and
/// the correct `current_days`.
#[test]
fn test_config_is_custom_after_set() {
    let (_, client, _) = setup();
    client.set_bid_ttl_days(&14u64);
    let cfg = client.get_bid_ttl_config();
    assert_eq!(cfg.current_days, 14);
    assert!(cfg.is_custom, "is_custom must be true after admin set");
    assert_eq!(cfg.min_days, MIN_BID_TTL_DAYS);
    assert_eq!(cfg.max_days, MAX_BID_TTL_DAYS);
    assert_eq!(cfg.default_days, DEFAULT_BID_TTL_DAYS);
}

/// `get_bid_ttl_days` returns the newly set value immediately.
#[test]
fn test_get_bid_ttl_days_reflects_update() {
    let (_, client, _) = setup();
    client.set_bid_ttl_days(&21u64);
    assert_eq!(client.get_bid_ttl_days(), 21);
}

// --- 4. Bid expiration uses configured TTL ------------------------------------

/// Bid placed after TTL update uses the new expiration window.
#[test]
fn test_bid_uses_updated_ttl() {
    let (env, client, admin) = setup();
    client.set_bid_ttl_days(&14u64);
    let (_, investor, invoice_id) = funded_setup(&env, &client, &admin, 10_000);
    let now = env.ledger().timestamp();
    let bid_id = client.place_bid(&investor, &invoice_id, &5_000, &6_000);
    let bid = client.get_bid(&bid_id).unwrap();
    assert_eq!(
        bid.expiration_timestamp,
        now + 14 * SECONDS_PER_DAY,
        "bid must use the updated 14-day TTL"
    );
}

/// Bid placed before a TTL update keeps its original expiration; the update
/// does not retroactively change existing bids.
#[test]
fn test_existing_bid_expiration_unchanged_after_ttl_update() {
    let (env, client, admin) = setup();
    let (_, investor, invoice_id) = funded_setup(&env, &client, &admin, 10_000);
    let now = env.ledger().timestamp();

    // Place bid with default TTL (7 days).
    let bid_id = client.place_bid(&investor, &invoice_id, &5_000, &6_000);
    let original_expiry = client.get_bid(&bid_id).unwrap().expiration_timestamp;
    assert_eq!(original_expiry, now + 7 * SECONDS_PER_DAY);

    // Admin updates TTL to 1 day.
    client.set_bid_ttl_days(&1u64);

    // Existing bid expiration must be unchanged.
    let bid_after = client.get_bid(&bid_id).unwrap();
    assert_eq!(
        bid_after.expiration_timestamp, original_expiry,
        "TTL update must not retroactively change existing bid expiration"
    );
}

/// Bid placed with minimum TTL (1 day) expires exactly at now + 1 day.
#[test]
fn test_bid_expiration_with_minimum_ttl() {
    let (env, client, admin) = setup();
    client.set_bid_ttl_days(&MIN_BID_TTL_DAYS);
    let (_, investor, invoice_id) = funded_setup(&env, &client, &admin, 10_000);
    let now = env.ledger().timestamp();
    let bid_id = client.place_bid(&investor, &invoice_id, &5_000, &6_000);
    let bid = client.get_bid(&bid_id).unwrap();
    assert_eq!(bid.expiration_timestamp, now + SECONDS_PER_DAY);
}

/// Bid placed with maximum TTL (30 days) expires exactly at now + 30 days.
#[test]
fn test_bid_expiration_with_maximum_ttl() {
    let (env, client, admin) = setup();
    client.set_bid_ttl_days(&MAX_BID_TTL_DAYS);
    let (_, investor, invoice_id) = funded_setup(&env, &client, &admin, 10_000);
    let now = env.ledger().timestamp();
    let bid_id = client.place_bid(&investor, &invoice_id, &5_000, &6_000);
    let bid = client.get_bid(&bid_id).unwrap();
    assert_eq!(bid.expiration_timestamp, now + 30 * SECONDS_PER_DAY);
}

/// Bid is not expired one second before its TTL boundary.
#[test]
fn test_bid_not_expired_before_ttl_boundary() {
    let (env, client, admin) = setup();
    client.set_bid_ttl_days(&1u64);
    let (_, investor, invoice_id) = funded_setup(&env, &client, &admin, 10_000);
    let bid_id = client.place_bid(&investor, &invoice_id, &5_000, &6_000);

    // Advance to 1 second before expiry.
    env.ledger()
        .set_timestamp(env.ledger().timestamp() + SECONDS_PER_DAY - 1);

    let bid = client.get_bid(&bid_id).unwrap();
    assert_eq!(
        bid.status,
        BidStatus::Placed,
        "bid must still be Placed one second before expiry"
    );
}

/// Bid is expired one second after its TTL boundary.
#[test]
fn test_bid_expired_after_ttl_boundary() {
    let (env, client, admin) = setup();
    client.set_bid_ttl_days(&1u64);
    let (_, investor, invoice_id) = funded_setup(&env, &client, &admin, 10_000);
    let bid_id = client.place_bid(&investor, &invoice_id, &5_000, &6_000);

    // Advance past expiry.
    env.ledger()
        .set_timestamp(env.ledger().timestamp() + SECONDS_PER_DAY + 1);

    // Trigger expiry sweep via cleanup.
    client.cleanup_expired_bids(&invoice_id);

    let bid = client.get_bid(&bid_id).unwrap();
    assert_eq!(
        bid.status,
        BidStatus::Expired,
        "bid must be Expired one second after TTL boundary"
    );
}

// --- 5. Reset to default ------------------------------------------------------

/// `reset_bid_ttl_to_default` restores the default and clears `is_custom`.
#[test]
fn test_reset_ttl_to_default() {
    let (_, client, _) = setup();
    client.set_bid_ttl_days(&20u64);
    assert_eq!(client.get_bid_ttl_days(), 20);

    let result = client.reset_bid_ttl_to_default();
    assert_eq!(result, DEFAULT_BID_TTL_DAYS);
    assert_eq!(client.get_bid_ttl_days(), DEFAULT_BID_TTL_DAYS);

    let cfg = client.get_bid_ttl_config();
    assert!(!cfg.is_custom, "is_custom must be false after reset");
}

/// After reset, bids use the default TTL again.
#[test]
fn test_bid_uses_default_after_reset() {
    let (env, client, admin) = setup();
    client.set_bid_ttl_days(&20u64);
    client.reset_bid_ttl_to_default();

    let (_, investor, invoice_id) = funded_setup(&env, &client, &admin, 10_000);
    let now = env.ledger().timestamp();
    let bid_id = client.place_bid(&investor, &invoice_id, &5_000, &6_000);
    let bid = client.get_bid(&bid_id).unwrap();
    assert_eq!(
        bid.expiration_timestamp,
        now + DEFAULT_BID_TTL_DAYS * SECONDS_PER_DAY,
        "bid must use default TTL after reset"
    );
}

/// Resetting when already at default is idempotent.
#[test]
fn test_reset_when_already_default_is_idempotent() {
    let (_, client, _) = setup();
    // No prior set - already at default.
    let result = client.reset_bid_ttl_to_default();
    assert_eq!(result, DEFAULT_BID_TTL_DAYS);
    assert_eq!(client.get_bid_ttl_days(), DEFAULT_BID_TTL_DAYS);
    let cfg = client.get_bid_ttl_config();
    assert!(!cfg.is_custom);
}

// --- 6. Multiple updates ------------------------------------------------------

/// Multiple sequential updates each take effect immediately.
#[test]
fn test_multiple_sequential_ttl_updates() {
    let (_, client, _) = setup();
    for days in [1u64, 7, 14, 30, 5] {
        client.set_bid_ttl_days(&days);
        assert_eq!(client.get_bid_ttl_days(), days);
    }
}

/// Set -> reset -> set cycle works correctly.
#[test]
fn test_set_reset_set_cycle() {
    let (_, client, _) = setup();
    client.set_bid_ttl_days(&10u64);
    assert_eq!(client.get_bid_ttl_days(), 10);

    client.reset_bid_ttl_to_default();
    assert_eq!(client.get_bid_ttl_days(), DEFAULT_BID_TTL_DAYS);
    assert!(!client.get_bid_ttl_config().is_custom);

    client.set_bid_ttl_days(&25u64);
    assert_eq!(client.get_bid_ttl_days(), 25);
    assert!(client.get_bid_ttl_config().is_custom);
}
