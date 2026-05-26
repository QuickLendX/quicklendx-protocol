//! Due Date Validation Boundary Tests
//!
//! Validates strict due date checks near the current ledger timestamp to
//! prevent borderline acceptance inconsistencies:
//! - due_date == current_timestamp is rejected (not in the future)
//! - due_date == current_timestamp + 1 is accepted (minimal future)
//! - due_date in the past is rejected
//! - Overdue check boundary: exactly at due_date vs one second past
//! - Grace deadline boundary: exactly at vs one second past

#![cfg(test)]

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
    client.set_admin(&admin);
    (env, client, admin)
}

fn make_business(env: &Env, client: &QuickLendXContractClient, admin: &Address) -> Address {
    let business = Address::generate(env);
    client.submit_kyc_application(&business, &String::from_str(env, "KYC"));
    client.verify_business(admin, &business);
    business
}

// -- Due date rejection boundaries --------------------------------------------

#[test]
fn due_date_equal_to_current_timestamp_is_rejected() {
    let (env, client, admin) = setup();
    let business = make_business(&env, &client, &admin);
    let currency = Address::generate(&env);
    let now = 1_000_000u64;
    env.ledger().with_mut(|l| l.timestamp = now);

    // due_date == now should fail (not strictly in the future)
    let result = client.try_store_invoice(
        &business,
        &1000i128,
        &currency,
        &now,
        &String::from_str(&env, "Test"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    assert!(result.is_err());
}

#[test]
fn due_date_one_second_after_now_is_accepted() {
    let (env, client, admin) = setup();
    let business = make_business(&env, &client, &admin);
    let currency = Address::generate(&env);
    let now = 1_000_000u64;
    env.ledger().with_mut(|l| l.timestamp = now);

    // due_date == now + 1 should succeed (minimal valid future)
    let result = client.try_store_invoice(
        &business,
        &1000i128,
        &currency,
        &(now + 1),
        &String::from_str(&env, "Test"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    assert!(result.is_ok());
}

#[test]
fn due_date_in_the_past_is_rejected() {
    let (env, client, admin) = setup();
    let business = make_business(&env, &client, &admin);
    let currency = Address::generate(&env);
    let now = 1_000_000u64;
    env.ledger().with_mut(|l| l.timestamp = now);

    let result = client.try_store_invoice(
        &business,
        &1000i128,
        &currency,
        &(now - 100),
        &String::from_str(&env, "Past due"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    assert!(result.is_err());
}

#[test]
fn due_date_zero_is_rejected() {
    let (env, client, admin) = setup();
    let business = make_business(&env, &client, &admin);
    let currency = Address::generate(&env);
    env.ledger().with_mut(|l| l.timestamp = 1_000);

    let result = client.try_store_invoice(
        &business,
        &1000i128,
        &currency,
        &0u64,
        &String::from_str(&env, "Zero due date"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    assert!(result.is_err());
}

// -- Overdue check boundaries (Invoice::is_overdue) ---------------------------

#[test]
fn invoice_not_overdue_at_exact_due_date() {
    let (env, client, admin) = setup();
    let business = make_business(&env, &client, &admin);
    let currency = Address::generate(&env);
    let now = 500_000u64;
    let due = now + 86_400; // 1 day from now
    env.ledger().with_mut(|l| l.timestamp = now);

    let invoice_id = client.store_invoice(
        &business,
        &1000i128,
        &currency,
        &due,
        &String::from_str(&env, "Boundary test"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    // Advance to exactly the due date
    env.ledger().with_mut(|l| l.timestamp = due);
    let invoice = client.get_invoice(&invoice_id);
    // is_overdue uses `current_timestamp > due_date` (exclusive)
    // so at exact due_date, it should NOT be overdue
    assert!(!invoice.is_overdue(due));
}

#[test]
fn invoice_overdue_one_second_after_due_date() {
    let (env, client, admin) = setup();
    let business = make_business(&env, &client, &admin);
    let currency = Address::generate(&env);
    let now = 500_000u64;
    let due = now + 86_400;
    env.ledger().with_mut(|l| l.timestamp = now);

    let invoice_id = client.store_invoice(
        &business,
        &1000i128,
        &currency,
        &due,
        &String::from_str(&env, "Boundary test"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    let invoice = client.get_invoice(&invoice_id);
    // One second past due: should be overdue
    assert!(invoice.is_overdue(due + 1));
}

// -- Grace deadline boundaries ------------------------------------------------

#[test]
fn grace_deadline_uses_saturating_add() {
    let (env, client, admin) = setup();
    let business = make_business(&env, &client, &admin);
    let currency = Address::generate(&env);
    let now = 500_000u64;
    let due = now + 86_400;
    env.ledger().with_mut(|l| l.timestamp = now);

    let invoice_id = client.store_invoice(
        &business,
        &1000i128,
        &currency,
        &due,
        &String::from_str(&env, "Grace test"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    let invoice = client.get_invoice(&invoice_id);

    // With u64::MAX grace period, should saturate to u64::MAX
    let deadline = invoice.grace_deadline(u64::MAX);
    assert_eq!(deadline, u64::MAX);
}

#[test]
fn grace_deadline_calculation_is_correct() {
    let (env, client, admin) = setup();
    let business = make_business(&env, &client, &admin);
    let currency = Address::generate(&env);
    let now = 500_000u64;
    let due = now + 86_400;
    let grace_period = 7 * 86_400u64; // 7 days
    env.ledger().with_mut(|l| l.timestamp = now);

    let invoice_id = client.store_invoice(
        &business,
        &1000i128,
        &currency,
        &due,
        &String::from_str(&env, "Grace calc test"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    let invoice = client.get_invoice(&invoice_id);
    let deadline = invoice.grace_deadline(grace_period);
    assert_eq!(deadline, due + grace_period);
}

#[test]
fn due_date_far_future_accepted() {
    let (env, client, admin) = setup();
    let business = make_business(&env, &client, &admin);
    let currency = Address::generate(&env);
    let now = 1_000u64;
    env.ledger().with_mut(|l| l.timestamp = now);

    // 365 days in the future
    let far_future = now + 365 * 86_400;
    let result = client.try_store_invoice(
        &business,
        &1000i128,
        &currency,
        &far_future,
        &String::from_str(&env, "Far future"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    assert!(result.is_ok());
}
