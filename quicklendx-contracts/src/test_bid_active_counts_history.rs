//! Regression tests for expiry-aware active-bid counts and retained expiry
//! history.
//!
//! These tests lock in three behaviours:
//! 1. A `Placed` bid whose raw `expiration_timestamp` has passed is excluded
//!    from every active-bid counter immediately, even while the configurable
//!    grace window still defers its `Placed -> Expired` storage transition.
//! 2. `cleanup_expired_bids` transitions stale bids to `Expired` but retains
//!    them so they remain queryable through `get_bids_by_status` /
//!    `count_bids_by_status` (expiry history is not silently discarded).
//! 3. `assert_bid_invariants` treats a `Placed` bid as valid until it becomes
//!    cleanup-eligible, and treats an `Expired` bid as valid while its deadline
//!    is not in the future.

use super::*;
use crate::bid::{BidStatus, BidStorage};
use crate::invoice::InvoiceCategory;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, BytesN, Env, String, Vec,
};

// ----------------------------------------------------------------------------
// SETUP HELPERS
// ----------------------------------------------------------------------------

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
    _admin: &Address,
    limit: i128,
) -> Address {
    let investor = Address::generate(env);
    client.submit_investor_kyc(&investor, &String::from_str(env, "KYC data"));
    client.verify_investor(&investor, &limit);
    investor
}

fn create_invoice(
    env: &Env,
    client: &QuickLendXContractClient,
    _admin: &Address,
    business: &Address,
    amount: i128,
    due_date: u64,
) -> BytesN<32> {
    let token_admin = Address::generate(env);
    let currency = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();
    let invoice_id = client.store_invoice(
        business,
        &amount,
        &currency,
        &due_date,
        &String::from_str(env, "Test invoice"),
        &InvoiceCategory::Services,
        &Vec::new(env),
        &None,
        &None,
        &None,
    );
    client.verify_invoice(&invoice_id);
    invoice_id
}

fn create_and_place_bid(
    env: &Env,
    client: &QuickLendXContractClient,
    investor: &Address,
    invoice_id: &BytesN<32>,
    bid_amount: i128,
    expected_return: i128,
) -> BytesN<32> {
    client.place_bid(
        investor,
        invoice_id,
        &bid_amount,
        &expected_return,
        &BytesN::from_array(env, &[0u8; 32]),
    )
}

/// Overwrite a bid's expiration timestamp inside the contract storage context.
fn set_bid_expiration(env: &Env, contract_id: &Address, bid_id: &BytesN<32>, expiration: u64) {
    env.as_contract(contract_id, || {
        let mut bid = BidStorage::get_bid(env, bid_id).unwrap();
        bid.expiration_timestamp = expiration;
        BidStorage::update_bid(env, &bid);
    });
}

/// Overwrite a bid's status inside the contract storage context.
fn set_bid_status(env: &Env, contract_id: &Address, bid_id: &BytesN<32>, status: BidStatus) {
    env.as_contract(contract_id, || {
        let mut bid = BidStorage::get_bid(env, bid_id).unwrap();
        bid.status = status;
        BidStorage::update_bid(env, &bid);
    });
}

// ----------------------------------------------------------------------------
// TESTS
// ----------------------------------------------------------------------------

/// A `Placed` bid past its raw expiry is not active, even during the grace
/// window in which it is not yet cleanup-eligible.
#[test]
fn expired_bid_not_active_during_grace() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, &admin, 100_000_000);

    client.set_bid_expiry_grace_seconds(&86_400u64);
    let invoice_id = create_invoice(
        &env,
        &client,
        &admin,
        &business,
        50_000,
        env.ledger().timestamp() + 86400 * 30,
    );
    let bid_id = create_and_place_bid(&env, &client, &investor, &invoice_id, 50_000, 50_500);

    set_bid_expiration(&env, &client.address, &bid_id, env.ledger().timestamp());

    env.ledger().set_timestamp(env.ledger().timestamp() + 10);

    let active = env.as_contract(&client.address, || {
        BidStorage::get_active_bid_count(&env, &invoice_id)
    });
    let by_investor = env.as_contract(&client.address, || {
        BidStorage::count_active_bids_by_investor(&env, &investor)
    });
    assert_eq!(active, 0);
    assert_eq!(by_investor, 0);
}

/// Every active-bid counter agrees on the same live/stale bid set.
#[test]
fn counters_agree() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, &admin, 100_000_000);
    let invoice_id = create_invoice(
        &env,
        &client,
        &admin,
        &business,
        500_000,
        env.ledger().timestamp() + 86400 * 30,
    );

    let live1 = create_and_place_bid(&env, &client, &investor, &invoice_id, 25_000, 25_250);
    let live2 = create_and_place_bid(&env, &client, &investor, &invoice_id, 30_000, 30_300);
    let live3 = create_and_place_bid(&env, &client, &investor, &invoice_id, 35_000, 35_350);

    let stale1 = create_and_place_bid(&env, &client, &investor, &invoice_id, 40_000, 40_400);
    let stale2 = create_and_place_bid(&env, &client, &investor, &invoice_id, 45_000, 45_450);

    let now = env.ledger().timestamp();
    for bid_id in [live1.clone(), live2.clone(), live3.clone()] {
        set_bid_expiration(&env, &client.address, &bid_id, now + 100_000);
    }
    for bid_id in [stale1.clone(), stale2.clone()] {
        set_bid_expiration(&env, &client.address, &bid_id, now);
    }

    let expired_bid = create_and_place_bid(&env, &client, &investor, &invoice_id, 50_000, 50_500);
    set_bid_expiration(
        &env,
        &client.address,
        &expired_bid,
        now.saturating_sub(1),
    );
    set_bid_status(&env, &client.address, &expired_bid, BidStatus::Expired);

    env.ledger().set_timestamp(now + 10);

    let by_investor = env.as_contract(&client.address, || {
        BidStorage::count_active_bids_by_investor(&env, &investor)
    });
    let placed_by_investor = env.as_contract(&client.address, || {
        BidStorage::count_active_placed_bids_for_investor(&env, &investor)
    });
    let active = env.as_contract(&client.address, || {
        BidStorage::get_active_bid_count(&env, &invoice_id)
    });

    assert_eq!(by_investor, 3);
    assert_eq!(placed_by_investor, 3);
    assert_eq!(active, 3);
}

/// Expired bids remain queryable after cleanup (expiry history retained).
#[test]
fn expired_bids_queryable() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, &admin, 100_000_000);
    let invoice_id = create_invoice(
        &env,
        &client,
        &admin,
        &business,
        50_000,
        env.ledger().timestamp() + 86400 * 30,
    );
    let bid_id = create_and_place_bid(&env, &client, &investor, &invoice_id, 50_000, 50_500);

    set_bid_expiration(&env, &client.address, &bid_id, env.ledger().timestamp());
    env.ledger().set_timestamp(env.ledger().timestamp() + 1);

    env.as_contract(&client.address, || {
        BidStorage::cleanup_expired_bids(&env, &invoice_id);
    });

    let expired = env.as_contract(&client.address, || {
        BidStorage::get_bids_by_status(&env, &invoice_id, BidStatus::Expired)
    });
    assert_eq!(expired.len(), 1);
    assert_eq!(expired.get(0).unwrap().bid_id, bid_id);

    let expired_count = env.as_contract(&client.address, || {
        BidStorage::count_bids_by_status(&env, &invoice_id).3
    });
    assert_eq!(expired_count, 1);
}

/// A `Placed` bid still inside its grace window, and an `Expired` bid whose
/// deadline is exactly now, are both valid states.
#[test]
fn invariants_accept_boundary_expiry() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, &admin, 100_000_000);
    let invoice_id = create_invoice(
        &env,
        &client,
        &admin,
        &business,
        50_000,
        env.ledger().timestamp() + 86400 * 30,
    );

    client.set_bid_expiry_grace_seconds(&86_400u64);
    let bid_id = create_and_place_bid(&env, &client, &investor, &invoice_id, 50_000, 50_500);
    set_bid_expiration(&env, &client.address, &bid_id, env.ledger().timestamp());
    env.ledger().set_timestamp(env.ledger().timestamp() + 10);

    let within_grace = env.as_contract(&client.address, || {
        BidStorage::assert_bid_invariants(&env, &invoice_id, env.ledger().timestamp())
    });
    assert!(within_grace);

    let expired_id = create_and_place_bid(&env, &client, &investor, &invoice_id, 60_000, 60_600);
    set_bid_status(&env, &client.address, &expired_id, BidStatus::Expired);
    set_bid_expiration(
        &env,
        &client.address,
        &expired_id,
        env.ledger().timestamp(),
    );

    let boundary = env.as_contract(&client.address, || {
        BidStorage::assert_bid_invariants(&env, &invoice_id, env.ledger().timestamp())
    });
    assert!(boundary);
}

/// An `Expired` bid whose deadline is still in the future is a real violation.
#[test]
fn invariants_reject_real_violation() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, &admin, 100_000_000);
    let invoice_id = create_invoice(
        &env,
        &client,
        &admin,
        &business,
        50_000,
        env.ledger().timestamp() + 86400 * 30,
    );
    let bid_id = create_and_place_bid(&env, &client, &investor, &invoice_id, 50_000, 50_500);

    set_bid_status(&env, &client.address, &bid_id, BidStatus::Expired);
    set_bid_expiration(
        &env,
        &client.address,
        &bid_id,
        env.ledger().timestamp() + 100,
    );

    let valid = env.as_contract(&client.address, || {
        BidStorage::assert_bid_invariants(&env, &invoice_id, env.ledger().timestamp())
    });
    assert!(!valid);
}
