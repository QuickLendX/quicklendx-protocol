//! Tests for `per_investor_position_cap` — Issue #1858.
//!
//! Threat mitigated: a whale with a high KYC investment limit can otherwise
//! bid the full invoice face value and corner funding, crowding out other
//! investors. The optional per-invoice absolute cap rejects oversized bids
//! with a typed `PerInvestorPositionCapExceeded` error.
//!
//! # Coverage
//!
//! | Scenario                                      | Expected                           |
//! |-----------------------------------------------|------------------------------------|
//! | bid_amount == cap                             | Ok                                 |
//! | bid_amount == cap + 1                         | Err PerInvestorPositionCapExceeded |
//! | uncapped invoice (`None`) full-face bid       | Ok                                 |
//! | set cap > amount                              | Err InvalidAmount                  |
//! | set cap == 0                                  | Err InvalidAmount                  |
//! | non-owner sets cap                            | Err NotBusinessOwner               |

#![cfg(test)]

use crate::errors::QuickLendXError;
use crate::invoice::InvoiceCategory;
use crate::{QuickLendXContract, QuickLendXContractClient};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, BytesN, Env, String, Vec,
};

fn build_env() -> (Env, QuickLendXContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = 1_000_000);
    let id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &id);
    let admin = Address::generate(&env);
    client.set_admin(&admin);
    (env, client, admin)
}

fn setup_verified_invoice(
    env: &Env,
    client: &QuickLendXContractClient,
    admin: &Address,
    invoice_amount: i128,
) -> (BytesN<32>, Address) {
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
        &String::from_str(env, "capped invoice"),
        &InvoiceCategory::Services,
        &Vec::new(env),
        &None,
    );
    client.verify_invoice(&invoice_id);
    (invoice_id, business)
}

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

fn zero_salt(env: &Env) -> BytesN<32> {
    BytesN::from_array(env, &[0u8; 32])
}

/// Negative test: a bid one unit above the configured cap must be rejected
/// with the typed `PerInvestorPositionCapExceeded` error.
#[test]
fn bid_above_per_investor_position_cap_is_rejected() {
    let (env, client, admin) = build_env();
    let invoice_amount = 10_000i128;
    let cap = 3_000i128;
    let (invoice_id, business) =
        setup_verified_invoice(&env, &client, &admin, invoice_amount);
    client.set_per_investor_position_cap(&business, &invoice_id, &Some(cap));
    assert_eq!(client.get_per_investor_position_cap(&invoice_id), Some(cap));

    // Whale KYC limit far above the invoice — without the cap this bid would succeed.
    let investor = setup_verified_investor(&env, &client, 1_000_000);

    let over_cap = cap + 1;
    let result = client.try_place_bid(
        &investor,
        &invoice_id,
        &over_cap,
        &(over_cap + 100),
        &zero_salt(&env),
    );

    assert!(
        result.is_err(),
        "bid_amount == cap + 1 ({over_cap}) must be rejected when cap={cap}"
    );
    assert_eq!(
        result.unwrap_err().expect("expected contract error"),
        QuickLendXError::PerInvestorPositionCapExceeded,
        "rejection must carry PerInvestorPositionCapExceeded"
    );
}

/// Boundary: a bid exactly at the cap is accepted.
#[test]
fn bid_at_per_investor_position_cap_is_accepted() {
    let (env, client, admin) = build_env();
    let invoice_amount = 10_000i128;
    let cap = 3_000i128;
    let (invoice_id, business) =
        setup_verified_invoice(&env, &client, &admin, invoice_amount);
    client.set_per_investor_position_cap(&business, &invoice_id, &Some(cap));
    let investor = setup_verified_investor(&env, &client, 1_000_000);

    let result = client.try_place_bid(
        &investor,
        &invoice_id,
        &cap,
        &(cap + 100),
        &zero_salt(&env),
    );

    assert!(
        result.is_ok(),
        "bid_amount == cap ({cap}) should be accepted; got {:?}",
        result.err()
    );
}

/// Uncapped invoices still allow a full-face-value bid.
#[test]
fn uncapped_invoice_allows_full_face_bid() {
    let (env, client, admin) = build_env();
    let invoice_amount = 5_000i128;
    let (invoice_id, _) = setup_verified_invoice(&env, &client, &admin, invoice_amount);
    assert_eq!(client.get_per_investor_position_cap(&invoice_id), None);
    let investor = setup_verified_investor(&env, &client, 1_000_000);

    let result = client.try_place_bid(
        &investor,
        &invoice_id,
        &invoice_amount,
        &(invoice_amount + 100),
        &zero_salt(&env),
    );

    assert!(
        result.is_ok(),
        "uncapped invoice should accept full face bid; got {:?}",
        result.err()
    );
}

#[test]
fn set_cap_above_amount_is_rejected() {
    let (env, client, admin) = build_env();
    let invoice_amount = 1_000i128;
    let (invoice_id, business) =
        setup_verified_invoice(&env, &client, &admin, invoice_amount);

    let result =
        client.try_set_per_investor_position_cap(&business, &invoice_id, &Some(invoice_amount + 1));
    assert!(result.is_err());
    assert_eq!(
        result.unwrap_err().expect("expected contract error"),
        QuickLendXError::InvalidAmount
    );
}

#[test]
fn set_zero_cap_is_rejected() {
    let (env, client, admin) = build_env();
    let (invoice_id, business) = setup_verified_invoice(&env, &client, &admin, 1_000);

    let result = client.try_set_per_investor_position_cap(&business, &invoice_id, &Some(0));
    assert!(result.is_err());
    assert_eq!(
        result.unwrap_err().expect("expected contract error"),
        QuickLendXError::InvalidAmount
    );
}

#[test]
fn non_owner_cannot_set_cap() {
    let (env, client, admin) = build_env();
    let (invoice_id, _business) = setup_verified_invoice(&env, &client, &admin, 1_000);
    let stranger = Address::generate(&env);

    let result = client.try_set_per_investor_position_cap(&stranger, &invoice_id, &Some(100));
    assert!(result.is_err());
    assert_eq!(
        result.unwrap_err().expect("expected contract error"),
        QuickLendXError::NotBusinessOwner
    );
}
