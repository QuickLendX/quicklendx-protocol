//! Tests for currency whitelist functionality:
//! add_currency (admin), remove_currency (admin), is_allowed_currency,
//! get_whitelisted_currencies, store_invoice and place_bid reject non-whitelisted currency.

use super::*;
use crate::invoice::InvoiceCategory;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, BytesN, Env, String, Vec,
};

fn setup() -> (Env, QuickLendXContractClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    (env, client)
}

// ============================================================================
// add_currency (admin)
// ============================================================================

#[test]
fn test_add_currency_success() {
    let (env, client) = setup();
    let admin = Address::generate(&env);
    let _ = client.set_admin(&admin);
    let currency = Address::generate(&env);

    let result = client.try_add_currency(&admin, &currency);
    assert!(result.is_ok(), "add_currency should succeed");
    assert!(client.is_allowed_currency(&currency));
}

#[test]
fn test_add_currency_reject_when_not_admin() {
    let (env, client) = setup();
    let admin = Address::generate(&env);
    let _ = client.set_admin(&admin);
    let non_admin = Address::generate(&env);
    let currency = Address::generate(&env);

    let result = client.try_add_currency(&non_admin, &currency);
    assert!(result.is_err(), "add_currency should fail when not admin");
}

#[test]
fn test_add_currency_idempotent() {
    let (env, client) = setup();
    let admin = Address::generate(&env);
    let _ = client.set_admin(&admin);
    let currency = Address::generate(&env);

    let _ = client.add_currency(&admin, &currency);
    let result = client.try_add_currency(&admin, &currency);
    assert!(result.is_ok(), "second add should succeed (idempotent)");
    assert!(client.is_allowed_currency(&currency));
}

#[test]
fn test_add_currency_multiple() {
    let (env, client) = setup();
    let admin = Address::generate(&env);
    let _ = client.set_admin(&admin);
    let c1 = Address::generate(&env);
    let c2 = Address::generate(&env);
    let c3 = Address::generate(&env);

    let _ = client.add_currency(&admin, &c1);
    let _ = client.add_currency(&admin, &c2);
    let _ = client.add_currency(&admin, &c3);

    let list = client.get_whitelisted_currencies();
    assert!(list.len() >= 3);
    assert!(client.is_allowed_currency(&c1));
    assert!(client.is_allowed_currency(&c2));
    assert!(client.is_allowed_currency(&c3));
}

// ============================================================================
// remove_currency (admin)
// ============================================================================

#[test]
fn test_remove_currency_success() {
    let (env, client) = setup();
    let admin = Address::generate(&env);
    let _ = client.set_admin(&admin);
    let currency = Address::generate(&env);
    let _ = client.add_currency(&admin, &currency);
    assert!(client.is_allowed_currency(&currency));

    let result = client.try_remove_currency(&admin, &currency);
    assert!(result.is_ok());
    assert!(!client.is_allowed_currency(&currency));
}

#[test]
fn test_remove_currency_reject_when_not_admin() {
    let (env, client) = setup();
    let admin = Address::generate(&env);
    let _ = client.set_admin(&admin);
    let non_admin = Address::generate(&env);
    let currency = Address::generate(&env);
    let _ = client.add_currency(&admin, &currency);

    let result = client.try_remove_currency(&non_admin, &currency);
    assert!(result.is_err());
    assert!(client.is_allowed_currency(&currency));
}

#[test]
fn test_remove_currency_non_existent_no_op() {
    let (env, client) = setup();
    let admin = Address::generate(&env);
    let _ = client.set_admin(&admin);
    let currency = Address::generate(&env);

    let result = client.try_remove_currency(&admin, &currency);
    assert!(result.is_ok());
    assert!(!client.is_allowed_currency(&currency));
}

// ============================================================================
// is_allowed_currency
// ============================================================================

#[test]
fn test_is_allowed_currency_true_for_whitelisted() {
    let (env, client) = setup();
    let admin = Address::generate(&env);
    let _ = client.set_admin(&admin);
    let currency = Address::generate(&env);
    let _ = client.add_currency(&admin, &currency);

    assert!(client.is_allowed_currency(&currency));
}

#[test]
fn test_is_allowed_currency_false_for_non_whitelisted() {
    let (env, client) = setup();
    let currency = Address::generate(&env);

    assert!(!client.is_allowed_currency(&currency));
}

#[test]
fn test_is_allowed_currency_false_when_empty() {
    let (env, client) = setup();
    let admin = Address::generate(&env);
    let _ = client.set_admin(&admin);
    let currency = Address::generate(&env);

    assert!(!client.is_allowed_currency(&currency));
}

// ============================================================================
// get_whitelisted_currencies
// ============================================================================

#[test]
fn test_get_whitelisted_currencies_empty_initially() {
    let (env, client) = setup();
    let list = client.get_whitelisted_currencies();
    assert_eq!(list.len(), 0);
}

#[test]
fn test_get_whitelisted_currencies_returns_added() {
    let (env, client) = setup();
    let admin = Address::generate(&env);
    let _ = client.set_admin(&admin);
    let c1 = Address::generate(&env);
    let c2 = Address::generate(&env);
    let _ = client.add_currency(&admin, &c1);
    let _ = client.add_currency(&admin, &c2);

    let list = client.get_whitelisted_currencies();
    assert!(list.len() >= 2);
    assert!(list.iter().any(|x| x == c1));
    assert!(list.iter().any(|x| x == c2));
}

#[test]
fn test_get_whitelisted_currencies_reflects_removals() {
    let (env, client) = setup();
    let admin = Address::generate(&env);
    let _ = client.set_admin(&admin);
    let c1 = Address::generate(&env);
    let c2 = Address::generate(&env);
    let _ = client.add_currency(&admin, &c1);
    let _ = client.add_currency(&admin, &c2);
    let _ = client.remove_currency(&admin, &c1);

    let list = client.get_whitelisted_currencies();
    assert!(!list.iter().any(|x| x == c1));
    assert!(list.iter().any(|x| x == c2));
}

// ============================================================================
// store_invoice rejects non-whitelisted currency
// ============================================================================

#[test]
fn test_store_invoice_rejects_non_whitelisted_currency() {
    let (env, client) = setup();
    let admin = Address::generate(&env);
    let _ = client.set_admin(&admin);
    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    // Do NOT add currency to whitelist

    let result = client.try_store_invoice(
        &business,
        &1000,
        &currency,
        &(env.ledger().timestamp() + 86400),
        &String::from_str(&env, "Test"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    assert!(result.is_err(), "store_invoice must reject non-whitelisted currency");
}

// ============================================================================
// place_bid rejects invoice with non-whitelisted currency
// ============================================================================

#[test]
fn test_place_bid_rejects_invoice_with_non_whitelisted_currency() {
    let (env, client) = setup();
    let admin = Address::generate(&env);
    let _ = client.set_admin(&admin);
    let business = Address::generate(&env);
    let investor = Address::generate(&env);
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;

    // Add currency, create and verify invoice, then remove currency before placing bid
    let _ = client.add_currency(&admin, &currency);
    client.submit_kyc_application(&business, &String::from_str(&env, "KYC"));
    client.verify_business(&admin, &business);
    client.submit_investor_kyc(&investor, &String::from_str(&env, "KYC"));
    client.verify_investor(&investor, &100_000);

    let invoice_id = client.store_invoice(
        &business,
        &10_000,
        &currency,
        &due_date,
        &String::from_str(&env, "Invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    client.verify_invoice(&invoice_id);

    // Remove currency from whitelist - invoice was stored with it, but now it's not allowed
    let _ = client.remove_currency(&admin, &currency);

    // place_bid should reject because invoice.currency is no longer whitelisted
    let result = client.try_place_bid(&investor, &invoice_id, &5_000, &6_000);
    assert!(
        result.is_err(),
        "place_bid must reject invoice with non-whitelisted currency"
    );
}
