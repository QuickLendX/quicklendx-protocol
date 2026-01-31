//! Tests for multi-currency whitelist: add/remove currency, enforcement in invoice and bid flows.
//!
//! Cases: invoice with non-whitelisted currency fails when whitelist is set; bid on such
//! invoice fails; only admin can add/remove currency.

use super::*;
use crate::invoice::InvoiceCategory;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Env, String, Vec,
};

fn setup() -> (Env, QuickLendXContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let _ = client.initialize_admin(&admin);
    let _ = client.set_admin(&admin);
    (env, client, admin)
}

#[test]
fn test_add_remove_currency_admin_only() {
    let (env, client, admin) = setup();
    let currency = Address::generate(&env);
    client.add_currency(&admin, &currency);
    assert!(client.is_allowed_currency(&currency));
    let list = client.get_whitelisted_currencies();
    assert_eq!(list.len(), 1);
    assert_eq!(list.get(0).unwrap(), currency);

    client.remove_currency(&admin, &currency);
    assert!(!client.is_allowed_currency(&currency));
    let list2 = client.get_whitelisted_currencies();
    assert_eq!(list2.len(), 0);
}

#[test]
fn test_non_admin_cannot_add_currency() {
    let (env, client, admin) = setup();
    let currency = Address::generate(&env);
    client.add_currency(&admin, &currency);
    let non_admin = Address::generate(&env);
    let res = client.try_add_currency(&non_admin, &currency);
    assert!(res.is_err());
}

#[test]
fn test_non_admin_cannot_remove_currency() {
    let (env, client, admin) = setup();
    let currency = Address::generate(&env);
    client.add_currency(&admin, &currency);
    let non_admin = Address::generate(&env);
    let res = client.try_remove_currency(&non_admin, &currency);
    assert!(res.is_err());
}

#[test]
fn test_invoice_with_non_whitelisted_currency_fails_when_whitelist_set() {
    let (env, client, admin) = setup();
    let allowed_currency = Address::generate(&env);
    client.add_currency(&admin, &allowed_currency);
    let disallowed_currency = Address::generate(&env);
    let business = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;
    let res = client.try_store_invoice(
        &business,
        &1000i128,
        &disallowed_currency,
        &due_date,
        &String::from_str(&env, "Desc"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    assert!(res.is_err());
}

#[test]
fn test_invoice_with_whitelisted_currency_succeeds() {
    let (env, client, admin) = setup();
    let currency = Address::generate(&env);
    client.add_currency(&admin, &currency);
    let business = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;
    let invoice_id = client.store_invoice(
        &business,
        &1000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Desc"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    let got = client.get_invoice(&invoice_id);
    assert_eq!(got.amount, 1000i128);
}

#[test]
fn test_bid_on_invoice_with_non_whitelisted_currency_fails_when_whitelist_set() {
    let (env, client, admin) = setup();
    let currency_a = Address::generate(&env);
    let currency_b = Address::generate(&env);
    client.add_currency(&admin, &currency_a);
    let business = Address::generate(&env);
    let investor = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;
    let invoice_id = client.store_invoice(
        &business,
        &1000i128,
        &currency_a,
        &due_date,
        &String::from_str(&env, "Desc"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    client.verify_invoice(&invoice_id);
    client.submit_investor_kyc(&investor, &String::from_str(&env, "KYC"));
    client.verify_investor(&investor, &5000i128);
    client.remove_currency(&admin, &currency_a);
    client.add_currency(&admin, &currency_b);
    let res = client.try_place_bid(&investor, &invoice_id, &1000i128, &1100i128);
    assert!(res.is_err());
}

#[test]
fn test_add_currency_idempotent() {
    let (env, client, admin) = setup();
    let currency = Address::generate(&env);
    client.add_currency(&admin, &currency);
    client.add_currency(&admin, &currency);
    let list = client.get_whitelisted_currencies();
    assert_eq!(list.len(), 1);
}
