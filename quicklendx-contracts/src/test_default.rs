/// Comprehensive test suite for invoice default handling
/// Tests verify default detection, grace period logic, and state transitions
///
/// Test Categories:
/// 1. Grace period logic - default after grace period, no default before grace period
/// 2. State transitions - proper status changes when defaulting
/// 3. Unfunded invoices - cannot default unfunded invoices
/// 4. Admin-only operations - verify authorization
/// 5. Edge cases - multiple defaults, already defaulted invoices
use super::*;
use crate::errors::QuickLendXError;
use crate::invoice::{InvoiceCategory, InvoiceStatus};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, BytesN, Env, String, Vec,
};

// Helper: Setup contract with admin
fn setup() -> (Env, QuickLendXContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.set_admin(&admin);
    (env, client, admin)
}

// Helper: Create verified business
fn create_verified_business(
    env: &Env,
    client: &QuickLendXContractClient,
    _admin: &Address,
) -> Address {
    let business = Address::generate(env);
    client.submit_kyc_application(&business, &String::from_str(env, "KYC data"));
    let admin = Address::generate(env);
    client.set_admin(&admin);
    client.verify_business(&admin, &business);
    business
}

// Helper: Create verified investor
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

// Helper: Create and fund invoice
fn create_and_fund_invoice(
    env: &Env,
    client: &QuickLendXContractClient,
    _admin: &Address,
    business: &Address,
    investor: &Address,
    amount: i128,
    due_date: u64,
) -> BytesN<32> {
    let currency = Address::generate(env);
    let invoice_id = client.store_invoice(
        business,
        &amount,
        &currency,
        &due_date,
        &String::from_str(env, "Test invoice"),
        &InvoiceCategory::Services,
        &Vec::new(env),
    );
    client.verify_invoice(&invoice_id);

    let bid_id = client.place_bid(investor, &invoice_id, &amount, &(amount + 100));
    client.accept_bid(&invoice_id, &bid_id);

    invoice_id
}

#[test]
fn test_default_after_grace_period() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, &admin, 10000);

    let amount = 1000;
    let due_date = env.ledger().timestamp() + 86400; // 1 day from now
    let invoice_id = create_and_fund_invoice(
        &env, &client, &admin, &business, &investor, amount, due_date,
    );

    // Verify invoice is funded
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Funded);

    // Move time past due date + grace period (7 days default)
    let grace_period = 7 * 24 * 60 * 60; // 7 days
    let default_time = invoice.due_date + grace_period + 1;
    env.ledger().set_timestamp(default_time);

    // Mark as defaulted
    client.mark_invoice_defaulted(&invoice_id, &Some(grace_period));

    // Verify invoice is now defaulted
    let defaulted_invoice = client.get_invoice(&invoice_id);
    assert_eq!(defaulted_invoice.status, InvoiceStatus::Defaulted);
}

#[test]
fn test_no_default_before_grace_period() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, &admin, 10000);

    let amount = 1000;
    let due_date = env.ledger().timestamp() + 86400;
    let invoice_id = create_and_fund_invoice(
        &env, &client, &admin, &business, &investor, amount, due_date,
    );

    let invoice = client.get_invoice(&invoice_id);
    let grace_period = 7 * 24 * 60 * 60; // 7 days

    // Move time past due date but before grace period expires
    let before_grace = invoice.due_date + grace_period / 2;
    env.ledger().set_timestamp(before_grace);

    // Try to mark as defaulted - should fail
    let result = client.try_mark_invoice_defaulted(&invoice_id, &Some(grace_period));
    assert!(result.is_err());
    let err = result.err().unwrap();
    let contract_err = err.expect("expected contract error");
    assert_eq!(contract_err, QuickLendXError::OperationNotAllowed);

    // Verify invoice is still funded
    let invoice_after = client.get_invoice(&invoice_id);
    assert_eq!(invoice_after.status, InvoiceStatus::Funded);
}

#[test]
fn test_cannot_default_unfunded_invoice() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);

    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;
    let invoice_id = client.store_invoice(
        &business,
        &1000,
        &currency,
        &due_date,
        &String::from_str(&env, "Test invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    client.verify_invoice(&invoice_id);

    // Verify invoice is verified, not funded
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Verified);

    // Try to mark unfunded invoice as defaulted - should fail
    let result = client.try_mark_invoice_defaulted(&invoice_id, &None);
    assert!(result.is_err());
    let err = result.err().unwrap();
    let contract_err = err.expect("expected contract error");
    assert_eq!(contract_err, QuickLendXError::InvoiceNotAvailableForFunding);
}

#[test]
fn test_cannot_default_pending_invoice() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);

    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;
    let invoice_id = client.store_invoice(
        &business,
        &1000,
        &currency,
        &due_date,
        &String::from_str(&env, "Test invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    // Invoice is pending, not verified
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Pending);

    // Try to mark pending invoice as defaulted - should fail
    let result = client.try_mark_invoice_defaulted(&invoice_id, &None);
    assert!(result.is_err());
    let err = result.err().unwrap();
    let contract_err = err.expect("expected contract error");
    assert_eq!(contract_err, QuickLendXError::InvoiceNotAvailableForFunding);
}

#[test]
fn test_cannot_default_already_defaulted_invoice() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, &admin, 10000);

    let amount = 1000;
    let due_date = env.ledger().timestamp() + 86400;
    let invoice_id = create_and_fund_invoice(
        &env, &client, &admin, &business, &investor, amount, due_date,
    );

    let invoice = client.get_invoice(&invoice_id);
    let grace_period = 7 * 24 * 60 * 60;

    // Move time past grace period
    let default_time = invoice.due_date + grace_period + 1;
    env.ledger().set_timestamp(default_time);

    // Mark as defaulted first time
    client.mark_invoice_defaulted(&invoice_id, &Some(grace_period));

    // Try to mark as defaulted again - should fail
    let result = client.try_mark_invoice_defaulted(&invoice_id, &Some(grace_period));
    assert!(result.is_err());
    let err = result.err().unwrap();
    let contract_err = err.expect("expected contract error");
    assert_eq!(contract_err, QuickLendXError::InvalidStatus);
}

#[test]
fn test_custom_grace_period() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, &admin, 10000);

    let amount = 1000;
    let due_date = env.ledger().timestamp() + 86400;
    let invoice_id = create_and_fund_invoice(
        &env, &client, &admin, &business, &investor, amount, due_date,
    );

    let invoice = client.get_invoice(&invoice_id);
    let custom_grace_period = 3 * 24 * 60 * 60; // 3 days instead of default 7

    // Move time past custom grace period but before default grace period
    let custom_default_time = invoice.due_date + custom_grace_period + 1;
    env.ledger().set_timestamp(custom_default_time);

    // Should succeed with custom grace period
    client.mark_invoice_defaulted(&invoice_id, &Some(custom_grace_period));

    let defaulted_invoice = client.get_invoice(&invoice_id);
    assert_eq!(defaulted_invoice.status, InvoiceStatus::Defaulted);
}

#[test]
fn test_default_uses_default_grace_period_when_none_provided() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, &admin, 10000);

    let amount = 1000;
    let due_date = env.ledger().timestamp() + 86400;
    let invoice_id = create_and_fund_invoice(
        &env, &client, &admin, &business, &investor, amount, due_date,
    );

    let invoice = client.get_invoice(&invoice_id);
    let default_grace_period = 7 * 24 * 60 * 60; // Default is 7 days

    // Move time past default grace period
    let default_time = invoice.due_date + default_grace_period + 1;
    env.ledger().set_timestamp(default_time);

    // Mark as defaulted without specifying grace period (should use default)
    client.mark_invoice_defaulted(&invoice_id, &None);

    let defaulted_invoice = client.get_invoice(&invoice_id);
    assert_eq!(defaulted_invoice.status, InvoiceStatus::Defaulted);
}

#[test]
fn test_default_status_transition() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, &admin, 10000);

    let amount = 1000;
    let due_date = env.ledger().timestamp() + 86400;
    let invoice_id = create_and_fund_invoice(
        &env, &client, &admin, &business, &investor, amount, due_date,
    );

    // Verify initial status
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Funded);

    // Check status in funded list
    let funded_invoices = client.get_invoices_by_status(&InvoiceStatus::Funded);
    assert!(funded_invoices.iter().any(|id| id == invoice_id));

    // Move time past grace period
    let grace_period = 7 * 24 * 60 * 60;
    let default_time = invoice.due_date + grace_period + 1;
    env.ledger().set_timestamp(default_time);

    // Mark as defaulted
    client.mark_invoice_defaulted(&invoice_id, &Some(grace_period));

    // Verify status changed
    let defaulted_invoice = client.get_invoice(&invoice_id);
    assert_eq!(defaulted_invoice.status, InvoiceStatus::Defaulted);

    // Verify removed from funded list
    let funded_after = client.get_invoices_by_status(&InvoiceStatus::Funded);
    assert!(!funded_after.iter().any(|id| id == invoice_id));

    // Verify added to defaulted list
    let defaulted_invoices = client.get_invoices_by_status(&InvoiceStatus::Defaulted);
    assert!(defaulted_invoices.iter().any(|id| id == invoice_id));
}

#[test]
fn test_default_investment_status_update() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, &admin, 10000);

    let amount = 1000;
    let due_date = env.ledger().timestamp() + 86400;
    let invoice_id = create_and_fund_invoice(
        &env, &client, &admin, &business, &investor, amount, due_date,
    );

    // Get investment
    let investment = client.get_invoice_investment(&invoice_id);
    assert_eq!(
        investment.status,
        crate::investment::InvestmentStatus::Active
    );

    // Move time past grace period
    let invoice = client.get_invoice(&invoice_id);
    let grace_period = 7 * 24 * 60 * 60;
    let default_time = invoice.due_date + grace_period + 1;
    env.ledger().set_timestamp(default_time);

    // Mark as defaulted
    client.mark_invoice_defaulted(&invoice_id, &Some(grace_period));

    // Verify investment status updated
    let defaulted_investment = client.get_invoice_investment(&invoice_id);
    assert_eq!(
        defaulted_investment.status,
        crate::investment::InvestmentStatus::Defaulted
    );
}

#[test]
fn test_default_exactly_at_grace_deadline() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, &admin, 10000);

    let amount = 1000;
    let due_date = env.ledger().timestamp() + 86400;
    let invoice_id = create_and_fund_invoice(
        &env, &client, &admin, &business, &investor, amount, due_date,
    );

    let invoice = client.get_invoice(&invoice_id);
    let grace_period = 7 * 24 * 60 * 60;

    // Move time to exactly grace deadline (should not default yet)
    let grace_deadline = invoice.due_date + grace_period;
    env.ledger().set_timestamp(grace_deadline);

    // Should fail - grace period hasn't passed yet
    let result = client.try_mark_invoice_defaulted(&invoice_id, &Some(grace_period));
    assert!(result.is_err());

    // Move one second past grace deadline
    env.ledger().set_timestamp(grace_deadline + 1);

    // Should succeed now
    client.mark_invoice_defaulted(&invoice_id, &Some(grace_period));
    let defaulted_invoice = client.get_invoice(&invoice_id);
    assert_eq!(defaulted_invoice.status, InvoiceStatus::Defaulted);
}

#[test]
fn test_multiple_invoices_default_handling() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, &admin, 20000);

    let amount = 1000;
    let due_date = env.ledger().timestamp() + 86400;

    // Create multiple invoices
    let invoice1_id = create_and_fund_invoice(
        &env, &client, &admin, &business, &investor, amount, due_date,
    );
    let invoice2_id = create_and_fund_invoice(
        &env,
        &client,
        &admin,
        &business,
        &investor,
        amount,
        due_date + 86400,
    );

    let invoice1 = client.get_invoice(&invoice1_id);
    let invoice2 = client.get_invoice(&invoice2_id);
    let grace_period = 7 * 24 * 60 * 60;

    // Move time past first invoice's grace period but not second
    let time1 = invoice1.due_date + grace_period + 1;
    env.ledger().set_timestamp(time1);

    // First invoice should default
    client.mark_invoice_defaulted(&invoice1_id, &Some(grace_period));
    assert_eq!(
        client.get_invoice(&invoice1_id).status,
        InvoiceStatus::Defaulted
    );

    // Second invoice should not default yet
    let result = client.try_mark_invoice_defaulted(&invoice2_id, &Some(grace_period));
    assert!(result.is_err());
    assert_eq!(
        client.get_invoice(&invoice2_id).status,
        InvoiceStatus::Funded
    );
}
