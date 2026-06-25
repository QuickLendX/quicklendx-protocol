//! Exact-second bid expiry boundary regressions.
//!
//! This module pins the strict `Bid::is_expired` semantics (`current_timestamp >
//! expiration_timestamp`) across acceptance, cleanup, and best-bid selection.
//!
//! The tests cover:
//! - exact boundary: timestamp == expiration_timestamp is still valid
//! - one second later: timestamp == expiration_timestamp + 1 is expired
//! - TTL config changes are forward-looking and do not retroactively alter
//!   existing bid expirations
//! - expired bids are excluded from `get_best_bid` immediately at the boundary
//! - multiple bids that straddle the boundary retain the correct winner

#![cfg(test)]

use crate::bid::{BidStatus, MAX_BID_TTL_DAYS, MIN_BID_TTL_DAYS};
use crate::errors::QuickLendXError;
use crate::invoice::InvoiceCategory;
use crate::{QuickLendXContract, QuickLendXContractClient};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token,
    Address,
    BytesN,
    Env,
    String,
    Vec,
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

#[test]
fn test_bid_exact_expiration_timestamp_is_not_expired() {
    let (env, client, admin) = setup();
    client.set_bid_ttl_days(&MIN_BID_TTL_DAYS);
    let (_, investor, invoice_id) = funded_setup(&env, &client, &admin, 10_000);
    let bid_id = client.place_bid(&investor, &invoice_id, &5_000, &6_000);
    let bid = client.get_bid(&bid_id).unwrap();

    env.ledger().set_timestamp(bid.expiration_timestamp);
    let now = env.ledger().timestamp();

    assert!(!bid.is_expired(now), "bid must not be expired at exact boundary");

    let cleaned = client.cleanup_expired_bids(&invoice_id);
    assert_eq!(cleaned, 0, "cleanup must not remove a bid at exact expiration");

    let bid_after = client.get_bid(&bid_id).unwrap();
    assert_eq!(bid_after.status, BidStatus::Placed);
    assert_eq!(client.get_best_bid(&invoice_id).unwrap().bid_id, bid_id);
}

#[test]
fn test_bid_one_second_past_expiration_timestamp_is_expired() {
    let (env, client, admin) = setup();
    client.set_bid_ttl_days(&MIN_BID_TTL_DAYS);
    let (_, investor, invoice_id) = funded_setup(&env, &client, &admin, 10_000);
    let bid_id = client.place_bid(&investor, &invoice_id, &5_000, &6_000);
    let bid = client.get_bid(&bid_id).unwrap();

    env.ledger().set_timestamp(bid.expiration_timestamp + 1);
    let now = env.ledger().timestamp();

    assert!(bid.is_expired(now), "bid must be expired one second past boundary");

    let cleaned = client.cleanup_expired_bids(&invoice_id);
    assert_eq!(cleaned, 1, "cleanup must remove a bid once it is expired");

    let bid_after = client.get_bid(&bid_id).unwrap();
    assert_eq!(bid_after.status, BidStatus::Expired);
    assert!(client.get_best_bid(&invoice_id).is_none());
}

#[test]
fn test_bid_ttl_change_is_forward_only_and_best_bid_honors_boundary() {
    let (env, client, admin) = setup();

    // Place an existing bid with a long TTL.
    client.set_bid_ttl_days(&MAX_BID_TTL_DAYS);
    let (_, investor, invoice_id) = funded_setup(&env, &client, &admin, 10_000);
    let bid_long_id = client.place_bid(&investor, &invoice_id, &5_000, &6_000);
    let bid_long = client.get_bid(&bid_long_id).unwrap();
    let long_expiration = bid_long.expiration_timestamp;

    // Change TTL; the existing bid must keep its original expiration.
    client.set_bid_ttl_days(&MIN_BID_TTL_DAYS);
    let bid_long_after = client.get_bid(&bid_long_id).unwrap();
    assert_eq!(bid_long_after.expiration_timestamp, long_expiration);

    // Place a new, shorter-lived bid after the TTL change.
    let bid_short_id = client.place_bid(&investor, &invoice_id, &5_000, &5_500);
    let bid_short = client.get_bid(&bid_short_id).unwrap();
    assert!(bid_short.expiration_timestamp < long_expiration);

    // At the exact short-bid expiration, it must still be eligible.
    env.ledger().set_timestamp(bid_short.expiration_timestamp);
    assert!(!bid_short.is_expired(env.ledger().timestamp()));
    let best_at_boundary = client.get_best_bid(&invoice_id).unwrap();
    assert_eq!(best_at_boundary.bid_id, bid_short_id);

    // One second later, the short bid must be expired and excluded.
    env.ledger().set_timestamp(bid_short.expiration_timestamp + 1);
    assert!(bid_short.is_expired(env.ledger().timestamp()));

    let cleaned = client.cleanup_expired_bids(&invoice_id);
    assert_eq!(cleaned, 1, "cleanup must remove only the newly expired bid");

    let best_after = client.get_best_bid(&invoice_id).unwrap();
    assert_eq!(best_after.bid_id, bid_long_id);
    let bid_long_after_cleanup = client.get_bid(&bid_long_id).unwrap();
    assert_eq!(bid_long_after_cleanup.status, BidStatus::Placed);
}
