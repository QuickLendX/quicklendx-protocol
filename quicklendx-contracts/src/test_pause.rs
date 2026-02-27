#![cfg(test)]
//! Tests for pause/unpause (#488): when paused, mutating ops fail with ContractPaused;
//! getters succeed; only admin can pause/unpause; admin can unpause.

use crate::errors::QuickLendXError;
use crate::invoice::InvoiceCategory;
use crate::{QuickLendXContract, QuickLendXContractClient};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env, String, Vec};

fn setup(
    env: &Env,
) -> (
    QuickLendXContractClient,
    Address,
    Address,
    Address,
    Address,
) {
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    client.initialize_admin(&admin);
    let business = Address::generate(env);
    let investor = Address::generate(env);
    let currency = Address::generate(env);
    (client, admin, business, investor, currency)
}

fn verify_investor_for_test(
    env: &Env,
    client: &QuickLendXContractClient,
    investor: &Address,
    limit: i128,
) {
    client.submit_investor_kyc(investor, &String::from_str(env, "Investor KYC"));
    client.verify_investor(investor, &limit);
}

#[test]
fn test_when_paused_store_invoice_fails_with_contract_paused() {
    let env = Env::default();
    let (client, admin, business, _investor, currency) = setup(&env);
    let due_date = env.ledger().timestamp() + 86400;

    client.pause(&admin);
    assert!(client.is_paused());

    let result = client.try_store_invoice(
        &business,
        &1000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Test"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    let err = result.err().expect("expected contract error");
    let contract_error = err.expect("expected contract invoke error");
    assert_eq!(contract_error, QuickLendXError::ContractPaused);
}

#[test]
fn test_when_paused_place_bid_fails_with_contract_paused() {
    let env = Env::default();
    let (client, admin, business, investor, currency) = setup(&env);
    let due_date = env.ledger().timestamp() + 86400;

    let invoice_id = client.store_invoice(
        &business,
        &1000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    client.verify_invoice(&invoice_id);
    verify_investor_for_test(&env, &client, &investor, 10_000);

    client.pause(&admin);
    assert!(client.is_paused());

    let result = client.try_place_bid(&investor, &invoice_id, &1000i128, &1100i128);
    let err = result.err().expect("expected contract error");
    let contract_error = err.expect("expected contract invoke error");
    assert_eq!(contract_error, QuickLendXError::ContractPaused);
}

#[test]
fn test_when_paused_accept_bid_fails_with_contract_paused() {
    let env = Env::default();
    let (client, admin, business, investor, currency) = setup(&env);
    let due_date = env.ledger().timestamp() + 86400;

    let invoice_id = client.store_invoice(
        &business,
        &1000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    client.verify_invoice(&invoice_id);
    verify_investor_for_test(&env, &client, &investor, 10_000);
    let bid_id = client.place_bid(&investor, &invoice_id, &1000i128, &1100i128);

    client.pause(&admin);
    assert!(client.is_paused());

    let result = client.try_accept_bid(&invoice_id, &bid_id);
    let err = result.err().expect("expected contract error");
    let contract_error = err.expect("expected contract invoke error");
    assert_eq!(contract_error, QuickLendXError::ContractPaused);
}

#[test]
fn test_getters_succeed_when_paused() {
    let env = Env::default();
    let (client, admin, business, _investor, currency) = setup(&env);
    let due_date = env.ledger().timestamp() + 86400;

    let invoice_id = client.store_invoice(
        &business,
        &1000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    client.pause(&admin);
    assert!(client.is_paused());

    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.amount, 1000);
    assert_eq!(invoice.business, business);

    let list = client.get_business_invoices(&business);
    assert!(!list.is_empty());
    assert_eq!(client.get_current_admin(), Some(admin));
    assert!(client.get_whitelisted_currencies().len() >= 0);
}

#[test]
fn test_admin_can_unpause() {
    let env = Env::default();
    let (client, admin, business, _investor, currency) = setup(&env);
    let due_date = env.ledger().timestamp() + 86400;

    client.pause(&admin);
    assert!(client.is_paused());

    client.unpause(&admin);
    assert!(!client.is_paused());

    let invoice_id = client.store_invoice(
        &business,
        &500i128,
        &currency,
        &due_date,
        &String::from_str(&env, "After unpause"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.amount, 500);
}

#[test]
fn test_non_admin_cannot_pause() {
    let env = Env::default();
    let (client, _admin, non_admin, _investor, _currency) = setup(&env);
    env.mock_all_auths();

    let result = client.try_pause(&non_admin);
    let err = result.err().expect("expected contract error");
    let contract_error = err.expect("expected contract invoke error");
    assert_eq!(contract_error, QuickLendXError::NotAdmin);
}

#[test]
fn test_non_admin_cannot_unpause() {
    let env = Env::default();
    let (client, admin, non_admin, _investor, _currency) = setup(&env);
    client.pause(&admin);
    assert!(client.is_paused());

    let result = client.try_unpause(&non_admin);
    let err = result.err().expect("expected contract error");
    let contract_error = err.expect("expected contract invoke error");
    assert_eq!(contract_error, QuickLendXError::NotAdmin);
    assert!(client.is_paused());
}

#[test]
fn test_pause_blocks_cancel_invoice() {
    let env = Env::default();
    let (client, admin, business, _investor, currency) = setup(&env);
    let due_date = env.ledger().timestamp() + 86400;

    let invoice_id = client.store_invoice(
        &business,
        &1000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "To cancel"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    client.pause(&admin);

    let result = client.try_cancel_invoice(&invoice_id);
    let err = result.err().expect("expected contract error");
    let contract_error = err.expect("expected contract invoke error");
    assert_eq!(contract_error, QuickLendXError::ContractPaused);
}

#[test]
fn test_pause_blocks_withdraw_bid() {
    let env = Env::default();
    let (client, admin, business, investor, currency) = setup(&env);
    let due_date = env.ledger().timestamp() + 86400;

    let invoice_id = client.store_invoice(
        &business,
        &1000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    client.verify_invoice(&invoice_id);
    verify_investor_for_test(&env, &client, &investor, 10_000);
    let bid_id = client.place_bid(&investor, &invoice_id, &1000i128, &1100i128);

    client.pause(&admin);
    let result = client.try_withdraw_bid(&bid_id);
    let err = result.err().expect("expected contract error");
    let contract_error = err.expect("expected contract invoke error");
    assert_eq!(contract_error, QuickLendXError::ContractPaused);
}

#[test]
fn test_verify_invoice_fails_when_paused() {
    let env = Env::default();
    let (client, admin, business, _investor, currency) = setup(&env);
    let due_date = env.ledger().timestamp() + 86400;

    let invoice_id = client.store_invoice(
        &business,
        &1000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    client.pause(&admin);

    let result = client.try_verify_invoice(&invoice_id);
    let err = result.err().expect("expected contract error");
    let contract_error = err.expect("expected contract invoke error");
    assert_eq!(contract_error, QuickLendXError::ContractPaused);
}

#[test]
fn test_upload_invoice_fails_when_paused() {
    let env = Env::default();
    let (client, admin, business, _investor, currency) = setup(&env);
    env.mock_all_auths();
    client.submit_kyc_application(&business, &String::from_str(&env, "KYC"));
    client.verify_business(&admin, &business);
    let due_date = env.ledger().timestamp() + 86400;

    client.pause(&admin);

    let result = client.try_upload_invoice(
        &business,
        &1000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Upload"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    let err = result.err().expect("expected contract error");
    let contract_error = err.expect("expected contract invoke error");
    assert_eq!(contract_error, QuickLendXError::ContractPaused);
}
