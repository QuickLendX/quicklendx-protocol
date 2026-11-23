#![cfg(feature = "legacy-tests")]

use quicklendx_contracts::bid::{BidStatus, BidStorage};
use quicklendx_contracts::invoice::InvoiceCategory;
use quicklendx_contracts::QuickLendXContract;
use quicklendx_contracts::QuickLendXContractClient;
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::{Address, BytesN, Env, String, Vec};

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
    admin: &Address,
    business: &Address,
    amount: i128,
    due_date: u64,
) -> BytesN<32> {
    let token_admin = Address::generate(env);
    let currency = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();
    client.add_currency(&admin, &currency);
    let result = client.try_store_invoice(
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
    let invoice_id = result.expect("store_invoice should succeed").expect("store_invoice contract error");
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

fn set_bid_expiration(env: &Env, contract_id: &Address, bid_id: &BytesN<32>, expiration: u64) {
    env.as_contract(contract_id, || {
        let mut bid = BidStorage::get_bid(env, bid_id).unwrap();
        bid.expiration_timestamp = expiration;
        BidStorage::update_bid(env, &bid);
    });
}

fn set_bid_status(env: &Env, contract_id: &Address, bid_id: &BytesN<32>, status: BidStatus) {
    env.as_contract(contract_id, || {
        let mut bid = BidStorage::get_bid(env, bid_id).unwrap();
        bid.status = status;
        BidStorage::update_bid(env, &bid);
    });
}

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
        1_000_000,
        env.ledger().timestamp() + 86400 * 30,
    );
    let bid_id = create_and_place_bid(&env, &client, &investor, &invoice_id, 1_000_000, 1_010_000);

    set_bid_expiration(&env, &client.address, &bid_id, env.ledger().timestamp());
    env.ledger().set_timestamp(env.ledger().timestamp() + 10);

    let active = env.as_contract(&client.address, || BidStorage::get_active_bid_count(&env, &invoice_id));
    let by_investor = env.as_contract(&client.address, || BidStorage::count_active_bids_by_investor(&env, &investor));
    assert_eq!(active, 0);
    assert_eq!(by_investor, 0);
}

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
        1_000_000,
        env.ledger().timestamp() + 86400 * 30,
    );
    let bid_id = create_and_place_bid(&env, &client, &investor, &invoice_id, 1_000_000, 1_010_000);

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
}

#[test]
fn invariants_accept_boundary_expiry() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, &admin, 100_000_000);
    let other_investor = create_verified_investor(&env, &client, &admin, 100_000_000);
    let invoice_id = create_invoice(
        &env,
        &client,
        &admin,
        &business,
        1_000_000,
        env.ledger().timestamp() + 86400 * 30,
    );

    client.set_bid_expiry_grace_seconds(&86_400u64);
    let bid_id = create_and_place_bid(&env, &client, &investor, &invoice_id, 1_000_000, 1_010_000);
    set_bid_expiration(&env, &client.address, &bid_id, env.ledger().timestamp());
    env.ledger().set_timestamp(env.ledger().timestamp() + 10);

    let within_grace = env.as_contract(&client.address, || {
        BidStorage::assert_bid_invariants(&env, &invoice_id, env.ledger().timestamp())
    });
    assert!(within_grace);

    let expired_id = create_and_place_bid(&env, &client, &other_investor, &invoice_id, 900_000, 909_000);
    set_bid_status(&env, &client.address, &expired_id, BidStatus::Expired);
    set_bid_expiration(&env, &client.address, &expired_id, env.ledger().timestamp());

    let boundary = env.as_contract(&client.address, || {
        BidStorage::assert_bid_invariants(&env, &invoice_id, env.ledger().timestamp())
    });
    assert!(boundary);
}
