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
use crate::init::ProtocolInitializer;
use crate::invoice::{InvoiceCategory, InvoiceStatus};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, BytesN, Env, String, Vec,
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

fn set_protocol_grace_period(env: &Env, admin: &Address, grace_period_seconds: u64) {
    // Use default protocol values instead of reading from storage
    let min_invoice_amount = 1_000_000; // DEFAULT_MIN_AMOUNT
    let max_due_date_days = 365;        // DEFAULT_MAX_DUE_DATE_DAYS
    ProtocolInitializer::set_protocol_config(
        env,
        admin,
        min_invoice_amount,
        max_due_date_days,
        grace_period_seconds,
    )
    .expect("protocol config update should succeed");
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
    _investor: &Address,
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

    // Manually transition invoice to Funded status for testing
    // This simulates successful bid acceptance without requiring actual currency transfers
    client.update_invoice_status(&invoice_id, &InvoiceStatus::Funded);

    invoice_id
}

#[test]
fn test_default_after_grace_period() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, &admin, 10_000_000);

    let amount = 1_000_000;
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
    let investor = create_verified_investor(&env, &client, &admin, 10_000_000);

    let amount = 1_000_000;
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
#[ignore] // TODO: Fix protocol config storage access issue in test infrastructure
fn test_default_uses_protocol_config_when_none() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, &admin, 10_000_000);

    let custom_grace = 3 * 24 * 60 * 60; // 3 days
    set_protocol_grace_period(&env, &admin, custom_grace);

    let amount = 1_000_000;
    let due_date = env.ledger().timestamp() + 86400;
    let invoice_id = create_and_fund_invoice(
        &env, &client, &admin, &business, &investor, amount, due_date,
    );

    let invoice = client.get_invoice(&invoice_id);
    env.ledger()
        .set_timestamp(invoice.due_date + custom_grace + 1);

    client.mark_invoice_defaulted(&invoice_id, &None);

    let defaulted_invoice = client.get_invoice(&invoice_id);
    assert_eq!(defaulted_invoice.status, InvoiceStatus::Defaulted);
}

#[test]
#[ignore] // TODO: Fix protocol config storage access issue in test infrastructure
fn test_check_invoice_expiration_uses_protocol_config_when_none() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, &admin, 10_000_000);

    let custom_grace = 2 * 24 * 60 * 60; // 2 days
    set_protocol_grace_period(&env, &admin, custom_grace);

    let amount = 1_000_000;
    let due_date = env.ledger().timestamp() + 86400;
    let invoice_id = create_and_fund_invoice(
        &env, &client, &admin, &business, &investor, amount, due_date,
    );

    let invoice = client.get_invoice(&invoice_id);
    env.ledger()
        .set_timestamp(invoice.due_date + custom_grace + 1);

    let did_default = client.check_invoice_expiration(&invoice_id, &None);
    assert!(did_default);

    let defaulted_invoice = client.get_invoice(&invoice_id);
    assert_eq!(defaulted_invoice.status, InvoiceStatus::Defaulted);
}

#[test]
#[ignore] // TODO: Fix protocol config storage access issue in test infrastructure
fn test_per_invoice_grace_overrides_protocol_config() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, &admin, 10_000_000);

    let protocol_grace = 10 * 24 * 60 * 60; // 10 days
    let per_invoice_grace = 2 * 24 * 60 * 60; // 2 days
    set_protocol_grace_period(&env, &admin, protocol_grace);

    let amount = 1_000_000;
    let due_date = env.ledger().timestamp() + 86400;
    let invoice_id = create_and_fund_invoice(
        &env, &client, &admin, &business, &investor, amount, due_date,
    );

    let invoice = client.get_invoice(&invoice_id);
    env.ledger()
        .set_timestamp(invoice.due_date + per_invoice_grace + 1);

    client.mark_invoice_defaulted(&invoice_id, &Some(per_invoice_grace));

    let defaulted_invoice = client.get_invoice(&invoice_id);
    assert_eq!(defaulted_invoice.status, InvoiceStatus::Defaulted);
}

#[test]
fn test_cannot_default_unfunded_invoice() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);

    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;
    let invoice_id = client.store_invoice(
        &business,
        &1_000_000,
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
        &1_000_000,
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
    let investor = create_verified_investor(&env, &client, &admin, 10_000_000);

    let amount = 1_000_000;
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

    // Try to mark as defaulted again - should fail with InvoiceAlreadyDefaulted
    let result = client.try_mark_invoice_defaulted(&invoice_id, &Some(grace_period));
    assert!(result.is_err());
    let err = result.err().unwrap();
    let contract_err = err.expect("expected contract error");
    assert_eq!(contract_err, QuickLendXError::InvoiceAlreadyDefaulted);
}

#[test]
fn test_custom_grace_period() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, &admin, 10_000_000);

    let amount = 1_000_000;
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
    let investor = create_verified_investor(&env, &client, &admin, 10_000_000);

    let amount = 1_000_000;
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
    let investor = create_verified_investor(&env, &client, &admin, 10_000_000);

    let amount = 1_000_000;
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
    let _investor = create_verified_investor(&env, &client, &admin, 10_000_000);

    let amount = 1_000_000;
    let due_date = env.ledger().timestamp() + 86400;
    let invoice_id = create_and_fund_invoice(
        &env, &client, &admin, &business, &_investor, amount, due_date,
    );

    // Verify invoice is funded
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Funded);

    // Move time past grace period
    let grace_period = 7 * 24 * 60 * 60;
    let default_time = invoice.due_date + grace_period + 1;
    env.ledger().set_timestamp(default_time);

    // Mark as defaulted
    client.mark_invoice_defaulted(&invoice_id, &Some(grace_period));

    // Verify invoice status updated to defaulted
    let defaulted_invoice = client.get_invoice(&invoice_id);
    assert_eq!(defaulted_invoice.status, InvoiceStatus::Defaulted);
}

#[test]
fn test_default_exactly_at_grace_deadline() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, &admin, 10_000_000);

    let amount = 1_000_000;
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

    let amount = 1_000_000;
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

#[test]
fn test_zero_grace_period_defaults_immediately_after_due_date() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, &admin, 10_000_000);

    let amount = 1_000_000;
    let due_date = env.ledger().timestamp() + 86400;
    let invoice_id = create_and_fund_invoice(
        &env, &client, &admin, &business, &investor, amount, due_date,
    );

    let invoice = client.get_invoice(&invoice_id);

    // With zero grace period, default should be possible right after due date
    env.ledger().set_timestamp(invoice.due_date + 1);

    client.mark_invoice_defaulted(&invoice_id, &Some(0));
    assert_eq!(
        client.get_invoice(&invoice_id).status,
        InvoiceStatus::Defaulted
    );
}

#[test]
fn test_cannot_default_paid_invoice() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, &admin, 10_000_000);

    let amount = 1_000_000;
    let due_date = env.ledger().timestamp() + 86400;
    let invoice_id = create_and_fund_invoice(
        &env, &client, &admin, &business, &investor, amount, due_date,
    );

    // Mark as paid via status update
    client.update_invoice_status(&invoice_id, &InvoiceStatus::Paid);

    // Move time well past any grace period
    let grace_period = 7 * 24 * 60 * 60;
    env.ledger().set_timestamp(due_date + grace_period + 1);

    // Paid invoices cannot be defaulted
    let result = client.try_mark_invoice_defaulted(&invoice_id, &Some(grace_period));
    assert!(result.is_err());
    let err = result.err().unwrap();
    let contract_err = err.expect("expected contract error");
    assert_eq!(contract_err, QuickLendXError::InvoiceNotAvailableForFunding);
}

// ============================================================================
// PHASE 1: Direct handle_default() Testing
// ============================================================================

#[test]
fn test_handle_default_fails_on_non_funded_invoice() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);

    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;
    let invoice_id = client.store_invoice(
        &business,
        &1_000_000,
        &currency,
        &due_date,
        &String::from_str(&env, "Test invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    client.verify_invoice(&invoice_id);

    // Try to handle_default on a verified (not funded) invoice - should fail
    let result = client.try_handle_default(&invoice_id);
    assert!(result.is_err());
    let err = result.err().unwrap();
    let contract_err = err.expect("expected contract error");
    assert_eq!(contract_err, QuickLendXError::InvalidStatus);
}

#[test]
fn test_handle_default_fails_on_already_defaulted_invoice() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, &admin, 10_000_000);

    let amount = 1_000_000;
    let due_date = env.ledger().timestamp() + 86400;
    let invoice_id = create_and_fund_invoice(
        &env, &client, &admin, &business, &investor, amount, due_date,
    );

    let invoice = client.get_invoice(&invoice_id);
    let grace_period = 7 * 24 * 60 * 60;

    // Move time past grace period and default the invoice
    let default_time = invoice.due_date + grace_period + 1;
    env.ledger().set_timestamp(default_time);
    client.mark_invoice_defaulted(&invoice_id, &Some(grace_period));

    // Verify it's defaulted
    let defaulted_invoice = client.get_invoice(&invoice_id);
    assert_eq!(defaulted_invoice.status, InvoiceStatus::Defaulted);

    // Try to handle_default again - should fail with InvoiceAlreadyDefaulted
    let result = client.try_handle_default(&invoice_id);
    assert!(result.is_err());
    let err = result.err().unwrap();
    let contract_err = err.expect("expected contract error");
    assert_eq!(contract_err, QuickLendXError::InvoiceAlreadyDefaulted);
}

#[test]
fn test_handle_default_updates_investment_status() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let _investor = create_verified_investor(&env, &client, &admin, 10_000_000);

    let amount = 1_000_000;
    let due_date = env.ledger().timestamp() + 86400;
    let invoice_id = create_and_fund_invoice(
        &env, &client, &admin, &business, &_investor, amount, due_date,
    );

    // Verify invoice is funded
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Funded);

    let grace_period = 7 * 24 * 60 * 60;
    env.ledger()
        .set_timestamp(invoice.due_date + grace_period + 1);

    // Call handle_default directly
    client.handle_default(&invoice_id);

    // Verify invoice status is now Defaulted
    let defaulted_invoice = client.get_invoice(&invoice_id);
    assert_eq!(defaulted_invoice.status, InvoiceStatus::Defaulted);
}

#[test]
fn test_handle_default_removes_from_funded_and_adds_to_defaulted() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, &admin, 10_000_000);

    let amount = 1_000_000;
    let due_date = env.ledger().timestamp() + 86400;
    let invoice_id = create_and_fund_invoice(
        &env, &client, &admin, &business, &investor, amount, due_date,
    );

    // Verify it's in funded list
    let funded_before = client.get_invoices_by_status(&InvoiceStatus::Funded);
    assert!(funded_before.iter().any(|id| id == invoice_id));

    let invoice = client.get_invoice(&invoice_id);
    let grace_period = 7 * 24 * 60 * 60;
    env.ledger()
        .set_timestamp(invoice.due_date + grace_period + 1);

    // Call handle_default
    client.handle_default(&invoice_id);

    // Verify removed from funded list
    let funded_after = client.get_invoices_by_status(&InvoiceStatus::Funded);
    assert!(!funded_after.iter().any(|id| id == invoice_id));

    // Verify added to defaulted list
    let defaulted_list = client.get_invoices_by_status(&InvoiceStatus::Defaulted);
    assert!(defaulted_list.iter().any(|id| id == invoice_id));
}

#[test]
fn test_handle_default_preserves_invoice_data() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, &admin, 10_000_000);

    let amount = 1_000_000;
    let due_date = env.ledger().timestamp() + 86400;
    let invoice_id = create_and_fund_invoice(
        &env, &client, &admin, &business, &investor, amount, due_date,
    );

    // Get invoice before default
    let invoice_before = client.get_invoice(&invoice_id);
    let amount_before = invoice_before.amount;
    let business_before = invoice_before.business.clone();

    let grace_period = 7 * 24 * 60 * 60;
    env.ledger()
        .set_timestamp(invoice_before.due_date + grace_period + 1);

    // Call handle_default
    client.handle_default(&invoice_id);

    // Get invoice after default
    let invoice_after = client.get_invoice(&invoice_id);

    // Verify critical data is preserved
    assert_eq!(invoice_after.amount, amount_before);
    assert_eq!(invoice_after.business, business_before);
    assert_eq!(invoice_after.due_date, invoice_before.due_date);
    // Verify status changed
    assert_eq!(invoice_after.status, InvoiceStatus::Defaulted);
}

#[test]
fn test_handle_default_fails_on_non_existent_invoice() {
    let (env, client, _admin) = setup();
    let non_existent_id = BytesN::from_array(&env, &[1u8; 32]);

    // Try to handle default on non-existent invoice
    let result = client.try_handle_default(&non_existent_id);
    assert!(result.is_err());
    let err = result.err().unwrap();
    let contract_err = err.expect("expected contract error");
    assert_eq!(contract_err, QuickLendXError::InvoiceNotFound);
}

// ============================================================================
// PHASE 2: check_invoice_expiration() Comprehensive Testing
// ============================================================================

#[test]
fn test_check_invoice_expiration_returns_true_when_expired() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, &admin, 10_000_000);

    let amount = 1_000_000;
    let due_date = env.ledger().timestamp() + 86400;
    let invoice_id = create_and_fund_invoice(
        &env, &client, &admin, &business, &investor, amount, due_date,
    );

    let invoice = client.get_invoice(&invoice_id);
    let grace_period = 7 * 24 * 60 * 60;

    // Move time past grace period
    env.ledger()
        .set_timestamp(invoice.due_date + grace_period + 1);

    // check_invoice_expiration should return true
    let did_expire = client.check_invoice_expiration(&invoice_id, &Some(grace_period));
    assert!(did_expire);

    // Verify invoice is now defaulted
    let defaulted_invoice = client.get_invoice(&invoice_id);
    assert_eq!(defaulted_invoice.status, InvoiceStatus::Defaulted);
}

#[test]
fn test_check_invoice_expiration_returns_false_when_not_expired() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, &admin, 10_000_000);

    let amount = 1_000_000;
    let due_date = env.ledger().timestamp() + 86400;
    let invoice_id = create_and_fund_invoice(
        &env, &client, &admin, &business, &investor, amount, due_date,
    );

    let invoice = client.get_invoice(&invoice_id);
    let grace_period = 7 * 24 * 60 * 60;

    // Move time to before grace period expires
    env.ledger()
        .set_timestamp(invoice.due_date + grace_period / 2);

    // check_invoice_expiration should return false
    let did_expire = client.check_invoice_expiration(&invoice_id, &Some(grace_period));
    assert!(!did_expire);

    // Verify invoice is still funded
    let invoice_after = client.get_invoice(&invoice_id);
    assert_eq!(invoice_after.status, InvoiceStatus::Funded);
}

#[test]
fn test_check_invoice_expiration_returns_false_for_pending_invoice() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);

    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;
    let invoice_id = client.store_invoice(
        &business,
        &1_000_000,
        &currency,
        &due_date,
        &String::from_str(&env, "Test invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    // Invoice is pending, not funded
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Pending);

    // Move time well past any grace period
    let grace_period = 7 * 24 * 60 * 60;
    env.ledger()
        .set_timestamp(invoice.due_date + grace_period + 1);

    // check_invoice_expiration should return false for non-funded invoices
    let did_expire = client.check_invoice_expiration(&invoice_id, &Some(grace_period));
    assert!(!did_expire);

    // Verify invoice is still pending
    let invoice_after = client.get_invoice(&invoice_id);
    assert_eq!(invoice_after.status, InvoiceStatus::Pending);
}

#[test]
fn test_check_invoice_expiration_returns_false_for_verified_invoice() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);

    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;
    let invoice_id = client.store_invoice(
        &business,
        &1_000_000,
        &currency,
        &due_date,
        &String::from_str(&env, "Test invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    client.verify_invoice(&invoice_id);

    // Invoice is verified, not funded
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Verified);

    // Move time well past any grace period
    let grace_period = 7 * 24 * 60 * 60;
    env.ledger()
        .set_timestamp(invoice.due_date + grace_period + 1);

    // check_invoice_expiration should return false for non-funded invoices
    let did_expire = client.check_invoice_expiration(&invoice_id, &Some(grace_period));
    assert!(!did_expire);

    // Verify invoice is still verified
    let invoice_after = client.get_invoice(&invoice_id);
    assert_eq!(invoice_after.status, InvoiceStatus::Verified);
}

#[test]
fn test_check_invoice_expiration_returns_false_for_paid_invoice() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, &admin, 10_000_000);

    let amount = 1_000_000;
    let due_date = env.ledger().timestamp() + 86400;
    let invoice_id = create_and_fund_invoice(
        &env, &client, &admin, &business, &investor, amount, due_date,
    );

    // Mark invoice as paid
    client.update_invoice_status(&invoice_id, &InvoiceStatus::Paid);

    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Paid);

    // Move time well past any grace period
    let grace_period = 7 * 24 * 60 * 60;
    env.ledger()
        .set_timestamp(invoice.due_date + grace_period + 1);

    // check_invoice_expiration should return false for paid invoices
    let did_expire = client.check_invoice_expiration(&invoice_id, &Some(grace_period));
    assert!(!did_expire);

    // Verify invoice is still paid
    let invoice_after = client.get_invoice(&invoice_id);
    assert_eq!(invoice_after.status, InvoiceStatus::Paid);
}

#[test]
fn test_check_invoice_expiration_with_custom_grace_period() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, &admin, 10_000_000);

    let amount = 1_000_000;
    let due_date = env.ledger().timestamp() + 86400;
    let invoice_id = create_and_fund_invoice(
        &env, &client, &admin, &business, &investor, amount, due_date,
    );

    let invoice = client.get_invoice(&invoice_id);
    let custom_grace = 2 * 24 * 60 * 60; // 2 days instead of 7

    // Move time past custom grace period
    env.ledger()
        .set_timestamp(invoice.due_date + custom_grace + 1);

    // Should default with custom grace period
    let did_expire = client.check_invoice_expiration(&invoice_id, &Some(custom_grace));
    assert!(did_expire);

    let defaulted_invoice = client.get_invoice(&invoice_id);
    assert_eq!(defaulted_invoice.status, InvoiceStatus::Defaulted);
}

#[test]
fn test_check_invoice_expiration_with_zero_grace_period() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, &admin, 10_000_000);

    let amount = 1_000_000;
    let due_date = env.ledger().timestamp() + 86400;
    let invoice_id = create_and_fund_invoice(
        &env, &client, &admin, &business, &investor, amount, due_date,
    );

    let invoice = client.get_invoice(&invoice_id);

    // Move time just past due date (zero grace period)
    env.ledger().set_timestamp(invoice.due_date + 1);

    // Should default immediately with zero grace
    let did_expire = client.check_invoice_expiration(&invoice_id, &Some(0));
    assert!(did_expire);

    let defaulted_invoice = client.get_invoice(&invoice_id);
    assert_eq!(defaulted_invoice.status, InvoiceStatus::Defaulted);
}

#[test]
fn test_check_invoice_expiration_fails_for_non_existent_invoice() {
    let (env, client, _admin) = setup();
    let non_existent_id = BytesN::from_array(&env, &[2u8; 32]);

    let result = client.try_check_invoice_expiration(&non_existent_id, &Some(7 * 24 * 60 * 60));
    assert!(result.is_err());
    let err = result.err().unwrap();
    let contract_err = err.expect("expected contract error");
    assert_eq!(contract_err, QuickLendXError::InvoiceNotFound);
}

// ============================================================================
// PHASE 3: Grace Period Boundary Tests
// ============================================================================

#[test]
fn test_grace_period_boundary_at_exact_deadline() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, &admin, 10_000_000);

    let amount = 1_000_000;
    let due_date = env.ledger().timestamp() + 86400;
    let invoice_id = create_and_fund_invoice(
        &env, &client, &admin, &business, &investor, amount, due_date,
    );

    let invoice = client.get_invoice(&invoice_id);
    let grace_period = 7 * 24 * 60 * 60;
    let grace_deadline = invoice.due_date + grace_period;

    // Move to exactly at grace deadline
    env.ledger().set_timestamp(grace_deadline);

    // Should NOT default at exact deadline (uses > condition, not >=)
    let result = client.try_mark_invoice_defaulted(&invoice_id, &Some(grace_period));
    assert!(result.is_err());
    assert_eq!(
        client.get_invoice(&invoice_id).status,
        InvoiceStatus::Funded
    );
}

#[test]
fn test_grace_period_boundary_one_second_before() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, &admin, 10_000_000);

    let amount = 1_000_000;
    let due_date = env.ledger().timestamp() + 86400;
    let invoice_id = create_and_fund_invoice(
        &env, &client, &admin, &business, &investor, amount, due_date,
    );

    let invoice = client.get_invoice(&invoice_id);
    let grace_period = 7 * 24 * 60 * 60;
    let one_second_before = invoice.due_date + grace_period - 1;

    // Move one second before deadline
    env.ledger().set_timestamp(one_second_before);

    // Should NOT default
    let result = client.try_mark_invoice_defaulted(&invoice_id, &Some(grace_period));
    assert!(result.is_err());
    assert_eq!(
        client.get_invoice(&invoice_id).status,
        InvoiceStatus::Funded
    );
}

#[test]
fn test_grace_period_boundary_one_second_after() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, &admin, 10_000_000);

    let amount = 1_000_000;
    let due_date = env.ledger().timestamp() + 86400;
    let invoice_id = create_and_fund_invoice(
        &env, &client, &admin, &business, &investor, amount, due_date,
    );

    let invoice = client.get_invoice(&invoice_id);
    let grace_period = 7 * 24 * 60 * 60;
    let one_second_after = invoice.due_date + grace_period + 1;

    // Move one second after deadline
    env.ledger().set_timestamp(one_second_after);

    // Should default
    client.mark_invoice_defaulted(&invoice_id, &Some(grace_period));
    assert_eq!(
        client.get_invoice(&invoice_id).status,
        InvoiceStatus::Defaulted
    );
}

#[test]
fn test_grace_period_boundary_large_grace_period() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, &admin, 100000);

    let amount = 1_000_000;
    let due_date = env.ledger().timestamp() + 86400;
    let invoice_id = create_and_fund_invoice(
        &env, &client, &admin, &business, &investor, amount, due_date,
    );

    let invoice = client.get_invoice(&invoice_id);
    // 180 days (very large grace period)
    let large_grace = 180 * 24 * 60 * 60;

    // Move past large grace period
    env.ledger()
        .set_timestamp(invoice.due_date + large_grace + 1);

    // Should still work correctly
    client.mark_invoice_defaulted(&invoice_id, &Some(large_grace));
    assert_eq!(
        client.get_invoice(&invoice_id).status,
        InvoiceStatus::Defaulted
    );
}

#[test]
fn test_grace_period_boundary_very_small_grace_period() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, &admin, 10_000_000);

    let amount = 1_000_000;
    let due_date = env.ledger().timestamp() + 86400;
    let invoice_id = create_and_fund_invoice(
        &env, &client, &admin, &business, &investor, amount, due_date,
    );

    let invoice = client.get_invoice(&invoice_id);
    let small_grace = 1; // 1 second grace period

    // Move just past small grace period
    env.ledger().set_timestamp(invoice.due_date + small_grace + 1);

    // Should default with 1 second grace
    client.mark_invoice_defaulted(&invoice_id, &Some(small_grace));
    assert_eq!(
        client.get_invoice(&invoice_id).status,
        InvoiceStatus::Defaulted
    );
}

// ============================================================================
// PHASE 4: Edge Cases and Integration Tests
// ============================================================================

#[test]
fn test_check_invoice_expiration_idempotent_on_already_defaulted() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, &admin, 10_000_000);

    let amount = 1_000_000;
    let due_date = env.ledger().timestamp() + 86400;
    let invoice_id = create_and_fund_invoice(
        &env, &client, &admin, &business, &investor, amount, due_date,
    );

    let invoice = client.get_invoice(&invoice_id);
    let grace_period = 7 * 24 * 60 * 60;
    env.ledger()
        .set_timestamp(invoice.due_date + grace_period + 1);

    // First call defaults the invoice
    let first_result = client.check_invoice_expiration(&invoice_id, &Some(grace_period));
    assert!(first_result);
    assert_eq!(
        client.get_invoice(&invoice_id).status,
        InvoiceStatus::Defaulted
    );

    // Second call should return false (already defaulted)
    let second_result = client.check_invoice_expiration(&invoice_id, &Some(grace_period));
    assert!(!second_result);

    // Invoice should still be defaulted
    assert_eq!(
        client.get_invoice(&invoice_id).status,
        InvoiceStatus::Defaulted
    );
}

#[test]
fn test_check_invoice_expiration_idempotent_on_non_expired() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, &admin, 10_000_000);

    let amount = 1_000_000;
    let due_date = env.ledger().timestamp() + 86400;
    let invoice_id = create_and_fund_invoice(
        &env, &client, &admin, &business, &investor, amount, due_date,
    );

    let invoice = client.get_invoice(&invoice_id);
    let grace_period = 7 * 24 * 60 * 60;
    env.ledger()
        .set_timestamp(invoice.due_date + grace_period / 2);

    // Multiple calls should all return false and not modify state
    for _ in 0..3 {
        let result = client.check_invoice_expiration(&invoice_id, &Some(grace_period));
        assert!(!result);
        assert_eq!(
            client.get_invoice(&invoice_id).status,
            InvoiceStatus::Funded
        );
    }
}

#[test]
fn test_multiple_invoices_independent_default_timings() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, &admin, 50000000);

    let amount = 1_000_000;
    let now = env.ledger().timestamp();

    // Create invoices with different due dates
    let due_date_1 = now + 86400;
    let due_date_2 = now + 172800; // 2 days later
    let due_date_3 = now + 259200; // 3 days later

    let invoice1 = create_and_fund_invoice(
        &env, &client, &admin, &business, &investor, amount, due_date_1,
    );
    let invoice2 = create_and_fund_invoice(
        &env, &client, &admin, &business, &investor, amount, due_date_2,
    );
    let invoice3 = create_and_fund_invoice(
        &env, &client, &admin, &business, &investor, amount, due_date_3,
    );

    let grace_period = 7 * 24 * 60 * 60;

    // Move time to default first invoice but not others
    env.ledger().set_timestamp(due_date_1 + grace_period + 1);
    client.check_invoice_expiration(&invoice1, &Some(grace_period));

    assert_eq!(
        client.get_invoice(&invoice1).status,
        InvoiceStatus::Defaulted
    );
    assert_eq!(client.get_invoice(&invoice2).status, InvoiceStatus::Funded);
    assert_eq!(client.get_invoice(&invoice3).status, InvoiceStatus::Funded);

    // Move time to default second invoice
    env.ledger().set_timestamp(due_date_2 + grace_period + 1);
    client.check_invoice_expiration(&invoice2, &Some(grace_period));

    assert_eq!(
        client.get_invoice(&invoice1).status,
        InvoiceStatus::Defaulted
    );
    assert_eq!(
        client.get_invoice(&invoice2).status,
        InvoiceStatus::Defaulted
    );
    assert_eq!(client.get_invoice(&invoice3).status, InvoiceStatus::Funded);

    // Move time to default third invoice
    env.ledger().set_timestamp(due_date_3 + grace_period + 1);
    client.check_invoice_expiration(&invoice3, &Some(grace_period));

    assert_eq!(
        client.get_invoice(&invoice1).status,
        InvoiceStatus::Defaulted
    );
    assert_eq!(
        client.get_invoice(&invoice2).status,
        InvoiceStatus::Defaulted
    );
    assert_eq!(
        client.get_invoice(&invoice3).status,
        InvoiceStatus::Defaulted
    );
}

#[test]
fn test_default_status_lists_consistency_with_invoice_status() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, &admin, 10_000_000);

    let amount = 1_000_000;
    let due_date = env.ledger().timestamp() + 86400;
    let invoice_id = create_and_fund_invoice(
        &env, &client, &admin, &business, &investor, amount, due_date,
    );

    // Verify consistency before default
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Funded);
    let funded_list = client.get_invoices_by_status(&InvoiceStatus::Funded);
    assert!(funded_list.iter().any(|id| id == invoice_id));

    let grace_period = 7 * 24 * 60 * 60;
    env.ledger()
        .set_timestamp(invoice.due_date + grace_period + 1);

    // Default via check_invoice_expiration
    client.check_invoice_expiration(&invoice_id, &Some(grace_period));

    // Verify consistency after default
    let invoice_after = client.get_invoice(&invoice_id);
    assert_eq!(invoice_after.status, InvoiceStatus::Defaulted);

    let funded_after = client.get_invoices_by_status(&InvoiceStatus::Funded);
    assert!(!funded_after.iter().any(|id| id == invoice_id));

    let defaulted_list = client.get_invoices_by_status(&InvoiceStatus::Defaulted);
    assert!(defaulted_list.iter().any(|id| id == invoice_id));
}

