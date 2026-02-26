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
fn test_get_whitelisted_currencies_empty_by_default() {
    let (env, client, _admin) = setup();
    let currency = Address::generate(&env);

    let list = client.get_whitelisted_currencies();
    assert_eq!(list.len(), 0, "whitelist should start empty");
    assert!(
        !client.is_allowed_currency(&currency),
        "currency should not be allowed before add"
    );
}

#[test]
fn test_get_whitelisted_currencies_after_add_and_remove() {
    let (env, client, admin) = setup();
    let currency_a = Address::generate(&env);
    let currency_b = Address::generate(&env);

    client.add_currency(&admin, &currency_a);
    client.add_currency(&admin, &currency_b);

    let after_add = client.get_whitelisted_currencies();
    assert_eq!(after_add.len(), 2);
    assert!(after_add.contains(&currency_a));
    assert!(after_add.contains(&currency_b));

    client.remove_currency(&admin, &currency_a);
    let after_remove_one = client.get_whitelisted_currencies();
    assert_eq!(after_remove_one.len(), 1);
    assert!(!after_remove_one.contains(&currency_a));
    assert!(after_remove_one.contains(&currency_b));

    client.remove_currency(&admin, &currency_b);
    let after_remove_all = client.get_whitelisted_currencies();
    assert_eq!(after_remove_all.len(), 0);
}

#[test]
fn test_is_allowed_currency_true_false_paths() {
    let (env, client, admin) = setup();
    let allowed = Address::generate(&env);
    let disallowed = Address::generate(&env);

    client.add_currency(&admin, &allowed);
    assert!(client.is_allowed_currency(&allowed));
    assert!(!client.is_allowed_currency(&disallowed));

    client.remove_currency(&admin, &allowed);
    assert!(
        !client.is_allowed_currency(&allowed),
        "removed currency should no longer be allowed"
    );
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

#[test]
fn test_remove_currency_when_missing_is_noop() {
    let (env, client, admin) = setup();
    let currency = Address::generate(&env);

    client.add_currency(&admin, &currency);
    client.remove_currency(&admin, &currency);
    assert_eq!(client.get_whitelisted_currencies().len(), 0);

    let second_remove = client.try_remove_currency(&admin, &currency);
    assert!(
        second_remove.is_ok(),
        "removing an already absent currency should be a no-op"
    );
    assert_eq!(client.get_whitelisted_currencies().len(), 0);
fn test_set_currencies_replaces_whitelist() {
    let (env, client, admin) = setup();
    let currency_a = Address::generate(&env);
    let currency_b = Address::generate(&env);
    client.add_currency(&admin, &currency_a);

    let mut new_list = Vec::new(&env);
    new_list.push_back(currency_b.clone());
    client.set_currencies(&admin, &new_list);

    assert!(!client.is_allowed_currency(&currency_a));
    assert!(client.is_allowed_currency(&currency_b));
    assert_eq!(client.currency_count(), 1);
}

#[test]
fn test_set_currencies_deduplicates() {
    let (env, client, admin) = setup();
    let currency = Address::generate(&env);
    let mut duped = Vec::new(&env);
    duped.push_back(currency.clone());
    duped.push_back(currency.clone());
    client.set_currencies(&admin, &duped);
    assert_eq!(client.currency_count(), 1);
}

#[test]
fn test_non_admin_cannot_set_currencies() {
    let (env, client, admin) = setup();
    let currency = Address::generate(&env);
    let mut list = Vec::new(&env);
    list.push_back(currency.clone());
    let non_admin = Address::generate(&env);
    let res = client.try_set_currencies(&non_admin, &list);
    assert!(res.is_err());
}

#[test]
fn test_clear_currencies_allows_all() {
    let (env, client, admin) = setup();
    let currency = Address::generate(&env);
    client.add_currency(&admin, &currency);
    client.clear_currencies(&admin);
    assert_eq!(client.currency_count(), 0);
    // empty whitelist = all allowed (backward-compat rule)
    let business = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;
    let any_token = Address::generate(&env);
    let invoice_id = client.store_invoice(
        &business,
        &1000i128,
        &any_token,
        &due_date,
        &String::from_str(&env, "Desc"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    let got = client.get_invoice(&invoice_id);
    assert_eq!(got.amount, 1000i128);
}

#[test]
fn test_non_admin_cannot_clear_currencies() {
    let (env, client, admin) = setup();
    let currency = Address::generate(&env);
    client.add_currency(&admin, &currency);
    let non_admin = Address::generate(&env);
    let res = client.try_clear_currencies(&non_admin);
    assert!(res.is_err());
}

#[test]
fn test_currency_count() {
    let (env, client, admin) = setup();
    assert_eq!(client.currency_count(), 0);
    let currency_a = Address::generate(&env);
    let currency_b = Address::generate(&env);
    client.add_currency(&admin, &currency_a);
    assert_eq!(client.currency_count(), 1);
    client.add_currency(&admin, &currency_b);
    assert_eq!(client.currency_count(), 2);
    client.remove_currency(&admin, &currency_a);
    assert_eq!(client.currency_count(), 1);
}

#[test]
fn test_get_whitelisted_currencies_paged() {
    let (env, client, admin) = setup();
    let currency_a = Address::generate(&env);
    let currency_b = Address::generate(&env);
    let currency_c = Address::generate(&env);
    client.add_currency(&admin, &currency_a);
    client.add_currency(&admin, &currency_b);
    client.add_currency(&admin, &currency_c);

    let page1 = client.get_whitelisted_currencies_paged(&0u32, &2u32);
    assert_eq!(page1.len(), 2);

    let page2 = client.get_whitelisted_currencies_paged(&2u32, &2u32);
    assert_eq!(page2.len(), 1);

    // offset beyond length returns empty
    let page3 = client.get_whitelisted_currencies_paged(&10u32, &2u32);
    assert_eq!(page3.len(), 0);
}
