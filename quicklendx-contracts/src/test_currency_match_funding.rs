//! Regression tests for `accept_bid_and_fund` currency matching and whitelist enforcement.
//!
//! These tests verify that the funded escrow always uses the invoice currency
//! and that currency whitelist enforcement happens before funding.

use super::*;
use crate::invoice::InvoiceCategory;
use soroban_sdk::{testutils::Address as _, token, Address, Env, String, Vec};

#[test]
fn test_accept_bid_and_fund_uses_invoice_currency_for_escrow_and_release() {
    let (env, client, admin) = crate::test::setup_env();
    let contract_id = client.address.clone();

    let business = crate::test::setup_verified_business(&env, &client, &admin);
    let investor = crate::test::setup_verified_investor(&env, &client, 50_000);

    let currency = crate::test::setup_token(&env, &business, &investor, &contract_id);
    client.add_currency(&admin, &currency);

    let amount = 5_000i128;
    let due_date = env.ledger().timestamp() + 86_400;
    let invoice_id = client.store_invoice(
        &business,
        &amount,
        &currency,
        &due_date,
        &String::from_str(&env, "Invoice currency match test"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    client.verify_invoice(&invoice_id);

    let bid_id = client.place_bid(&investor, &invoice_id, &amount, &(amount + 500));

    let investor_balance_before = token::Client::new(&env, &currency).balance(&investor);
    let contract_balance_before = token::Client::new(&env, &currency).balance(&contract_id);

    client.accept_bid_and_fund(&invoice_id, &bid_id);

    let escrow = client.get_escrow_details(&invoice_id).expect("Escrow should exist after funding");
    assert_eq!(escrow.currency, currency, "Escrow currency must match invoice currency");
    assert_eq!(escrow.amount, amount, "Escrow amount should equal accepted bid amount");
    assert_eq!(escrow.business, business, "Escrow business should match invoice business");
    assert_eq!(escrow.status, payments::EscrowStatus::Held);

    assert_eq!(
        token::Client::new(&env, &currency).balance(&investor),
        investor_balance_before - amount,
        "Investor balance should decrease by funded amount"
    );
    assert_eq!(
        token::Client::new(&env, &currency).balance(&contract_id),
        contract_balance_before + amount,
        "Contract balance should increase by funded amount"
    );

    client.release_escrow_funds(&invoice_id).expect("Release should succeed");

    assert_eq!(
        client.get_escrow_status(&invoice_id).expect("Escrow status query must succeed"),
        payments::EscrowStatus::Released,
        "Released escrow should reflect final release state"
    );
    assert_eq!(
        token::Client::new(&env, &currency).balance(&business),
        amount,
        "Business should receive the released funds in the same invoice currency"
    );
}

#[test]
fn test_place_bid_rejected_when_invoice_currency_removed_from_whitelist() {
    let (env, client, admin) = crate::test::setup_env();
    let contract_id = client.address.clone();

    let business = crate::test::setup_verified_business(&env, &client, &admin);
    let investor = crate::test::setup_verified_investor(&env, &client, 50_000);

    let currency = crate::test::setup_token(&env, &business, &investor, &contract_id);
    client.add_currency(&admin, &currency);

    let amount = 5_000i128;
    let invoice_id = client.store_invoice(
        &business,
        &amount,
        &currency,
        &(env.ledger().timestamp() + 86_400),
        &String::from_str(&env, "Whitelist removal regression test"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    client.verify_invoice(&invoice_id);

    client.remove_currency(&admin, &currency);

    let result = client.try_place_bid(&investor, &invoice_id, &amount, &(amount + 500));
    assert!(result.is_err(), "Bid placement must be rejected when invoice currency is no longer whitelisted");
    assert!(client.try_get_escrow_details(&invoice_id).is_err(), "No escrow should exist after a rejected bid placement");
}
