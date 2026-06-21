//! Exact-second bid expiry regressions for issue #1321.
//!
//! Bid expiry uses a strict `current_timestamp > expiration_timestamp` rule:
//! the bid remains active at the exact expiration second and becomes expired
//! one ledger second later.

use crate::bid::BidStatus;
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

    let expiration_ledger = env.ledger().sequence() + 100_000;
    tok.approve(business, contract_id, &400_000i128, &expiration_ledger);
    tok.approve(investor, contract_id, &400_000i128, &expiration_ledger);

    currency
}

fn verified_invoice(
    env: &Env,
    client: &QuickLendXContractClient,
    admin: &Address,
) -> (Address, BytesN<32>) {
    let business = Address::generate(env);
    let investor = Address::generate(env);
    let currency = make_token(env, &client.address, &business, &investor);

    client.submit_kyc_application(&business, &String::from_str(env, "business kyc"));
    client.verify_business(admin, &business);
    client.submit_investor_kyc(&investor, &String::from_str(env, "investor kyc"));
    client.verify_investor(&investor, &200_000i128);

    let due_date = env.ledger().timestamp() + 30 * SECONDS_PER_DAY;
    let invoice_id = client.upload_invoice(
        &business,
        &10_000i128,
        &currency,
        &due_date,
        &String::from_str(env, "boundary invoice"),
        &InvoiceCategory::Services,
        &Vec::new(env),
    );
    client.verify_invoice(&invoice_id);

    (investor, invoice_id)
}

fn place_boundary_bid(
    env: &Env,
    client: &QuickLendXContractClient,
    admin: &Address,
) -> (BytesN<32>, BytesN<32>, u64) {
    client.set_bid_ttl_days(&1u64);
    let (investor, invoice_id) = verified_invoice(env, client, admin);
    let bid_id = client.place_bid(&investor, &invoice_id, &5_000i128, &6_000i128);
    let bid = client.get_bid(&bid_id).unwrap();
    (bid_id, invoice_id, bid.expiration_timestamp)
}

#[test]
fn exact_expiration_second_is_not_expired() {
    let (env, client, admin) = setup();
    let (bid_id, invoice_id, expiration_timestamp) = place_boundary_bid(&env, &client, &admin);

    env.ledger().set_timestamp(expiration_timestamp);

    let bid = client.get_bid(&bid_id).unwrap();
    assert!(
        !bid.is_expired(env.ledger().timestamp()),
        "strict expiry keeps the bid active at the exact expiration timestamp"
    );
    assert_eq!(client.cleanup_expired_bids(&invoice_id), 0);
    assert_eq!(client.get_bid(&bid_id).unwrap().status, BidStatus::Placed);
    assert_eq!(client.get_best_bid(&invoice_id).unwrap().bid_id, bid_id);
    assert_eq!(client.get_ranked_bids(&invoice_id).len(), 1);
}

#[test]
fn one_second_after_expiration_is_expired_and_cleaned() {
    let (env, client, admin) = setup();
    let (bid_id, invoice_id, expiration_timestamp) = place_boundary_bid(&env, &client, &admin);

    env.ledger().set_timestamp(expiration_timestamp + 1);

    let bid = client.get_bid(&bid_id).unwrap();
    assert!(
        bid.is_expired(env.ledger().timestamp()),
        "bid must expire one ledger second after the expiration timestamp"
    );
    assert!(
        client.get_best_bid(&invoice_id).is_none(),
        "best-bid reads must not return expired placed bids"
    );
    assert_eq!(client.get_ranked_bids(&invoice_id).len(), 0);

    assert_eq!(
        client.cleanup_expired_bids(&invoice_id),
        0,
        "best-bid/ranking reads refresh expired bids before explicit cleanup"
    );
    assert_eq!(client.get_bid(&bid_id).unwrap().status, BidStatus::Expired);
}

#[test]
fn ttl_updates_are_forward_looking_not_retroactive() {
    let (env, client, admin) = setup();
    let (bid_id, invoice_id, original_expiration) = place_boundary_bid(&env, &client, &admin);

    client.set_bid_ttl_days(&30u64);
    assert_eq!(
        client.get_bid(&bid_id).unwrap().expiration_timestamp,
        original_expiration,
        "existing bids keep their placement-time expiration timestamp"
    );

    env.ledger().set_timestamp(original_expiration);
    assert_eq!(client.cleanup_expired_bids(&invoice_id), 0);

    env.ledger().set_timestamp(original_expiration + 1);
    assert_eq!(client.cleanup_expired_bids(&invoice_id), 1);
    assert_eq!(client.get_bid(&bid_id).unwrap().status, BidStatus::Expired);
}
