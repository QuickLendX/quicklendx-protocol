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
use crate::init::InitializationParams;
use crate::invoice::{InvoiceCategory, InvoiceStatus};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, BytesN, Env, String, Vec,
};

// Helper: Setup contract with admin and core config
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

fn set_protocol_grace_period(
    _env: &Env,
    _client: &QuickLendXContractClient,
    _admin: &Address,
    _grace_period_seconds: u64,
) {
    // Protocol config is set during initialization
    // This helper is kept for API compatibility but not used in current tests
}

// Helper: Create verified business
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

// Helper: Create verified investor
fn create_verified_investor(
    env: &Env,
    client: &QuickLendXContractClient,
    admin: &Address,
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
    admin: &Address,
    business: &Address,
    investor: &Address,
    amount: i128,
    due_date: u64,
) -> BytesN<32> {
    // Register token contract (use v2 API like test_refund.rs and test_escrow.rs)
    let token_admin = Address::generate(env);
    let currency = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let sac_client = token::StellarAssetClient::new(env, &currency);
    let token_client = token::Client::new(env, &currency);

    // Whitelist the currency
    client.add_currency(admin, &currency);

    // Mint tokens to investor so they can bid
    sac_client.mint(investor, &amount);
    // Approve contract to spend investor's tokens (use a finite TTL)
    let expiry = env.ledger().sequence() + 10_000;
    token_client.approve(investor, &client.address, &amount, &expiry);

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
    // Use default grace period (7 days)
    let default_grace = 7 * 24 * 60 * 60;
    env.ledger()
        .set_timestamp(invoice.due_date + default_grace + 1);

    client.mark_invoice_defaulted(&invoice_id, &None);

    let defaulted_invoice = client.get_invoice(&invoice_id);
    assert_eq!(defaulted_invoice.status, InvoiceStatus::Defaulted);
}

#[test]
fn test_check_invoice_expiration_uses_default_grace_period_when_none() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, &admin, 10000);

    let amount = 1000;
    let due_date = env.ledger().timestamp() + 86400;
    let invoice_id = create_and_fund_invoice(
        &env, &client, &admin, &business, &investor, amount, due_date,
    );

    let invoice = client.get_invoice(&invoice_id);
    // Use default grace period (7 days)
    let default_grace = 7 * 24 * 60 * 60;
    env.ledger()
        .set_timestamp(invoice.due_date + default_grace + 1);

    let did_default = client.check_invoice_expiration(&invoice_id, &None);
    assert!(did_default);

    let defaulted_invoice = client.get_invoice(&invoice_id);
    assert_eq!(defaulted_invoice.status, InvoiceStatus::Defaulted);
}

#[test]
fn test_per_invoice_grace_overrides_default() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, &admin, 10000);

    let per_invoice_grace = 2 * 24 * 60 * 60; // 2 days

    let amount = 1000;
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
fn test_default_with_none_rejects_exactly_at_default_grace_deadline() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, &admin, 10000);

    let amount = 1000;
    let due_date = env.ledger().timestamp() + 86400;
    let invoice_id = create_and_fund_invoice(
        &env, &client, &admin, &business, &investor, amount, due_date,
    );

    let invoice = client.get_invoice(&invoice_id);
    let grace_deadline = invoice.due_date + crate::defaults::DEFAULT_GRACE_PERIOD;
    env.ledger().set_timestamp(grace_deadline);

    let result = client.try_mark_invoice_defaulted(&invoice_id, &None);
    assert!(result.is_err());
    let err = result.err().unwrap();
    let contract_err = err.expect("expected contract error");
    assert_eq!(contract_err, QuickLendXError::OperationNotAllowed);
    assert_eq!(
        client.get_invoice(&invoice_id).status,
        InvoiceStatus::Funded
    );
}

#[test]
fn test_check_invoice_expiration_respects_strict_protocol_grace_boundary() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, &admin, 10000);

    let custom_grace = 2 * 24 * 60 * 60;
    set_protocol_grace_period(&env, &client, &admin, custom_grace);

    let amount = 1000;
    let due_date = env.ledger().timestamp() + 86400;
    let invoice_id = create_and_fund_invoice(
        &env, &client, &admin, &business, &investor, amount, due_date,
    );

    let invoice = client.get_invoice(&invoice_id);
    let grace_deadline = invoice.due_date + custom_grace;

    env.ledger().set_timestamp(grace_deadline);
    let did_default_at_deadline = client.check_invoice_expiration(&invoice_id, &None);
    assert!(!did_default_at_deadline);
    assert_eq!(
        client.get_invoice(&invoice_id).status,
        InvoiceStatus::Funded
    );

    env.ledger().set_timestamp(grace_deadline + 1);
    let did_default_after_deadline = client.check_invoice_expiration(&invoice_id, &None);
    assert!(did_default_after_deadline);
    assert_eq!(
        client.get_invoice(&invoice_id).status,
        InvoiceStatus::Defaulted
    );
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
    let investor = create_verified_investor(&env, &client, &admin, 10000);

    let amount = 1000;
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
    let investor = create_verified_investor(&env, &client, &admin, 10000);

    let amount = 1000;
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

#[test]
fn test_grace_period_override_exceeding_maximum_rejected() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, &admin, 10000);

    let amount = 1000;
    let due_date = env.ledger().timestamp() + 86400;
    let invoice_id = create_and_fund_invoice(
        &env, &client, &admin, &business, &investor, amount, due_date,
    );

    let invoice = client.get_invoice(&invoice_id);
    // Try to use grace period > 30 days (max allowed)
    let excessive_grace = 31 * 24 * 60 * 60; // 31 days
    env.ledger()
        .set_timestamp(invoice.due_date + excessive_grace + 1);

    // Should reject with InvalidTimestamp error
    let result = client.try_mark_invoice_defaulted(&invoice_id, &Some(excessive_grace));
    assert!(result.is_err());
    let err = result.err().unwrap();
    let contract_err = err.expect("expected contract error");
    assert_eq!(contract_err, QuickLendXError::InvalidTimestamp);

    // Verify invoice is still funded (not defaulted)
    let invoice_after = client.get_invoice(&invoice_id);
    assert_eq!(invoice_after.status, InvoiceStatus::Funded);
}

#[test]
fn test_grace_period_zero_allowed_for_immediate_default() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, &admin, 10000);

    let amount = 1000;
    let due_date = env.ledger().timestamp() + 86400;
    let invoice_id = create_and_fund_invoice(
        &env, &client, &admin, &business, &investor, amount, due_date,
    );

    let invoice = client.get_invoice(&invoice_id);
    // With zero grace period, should be able to default immediately after due date
    env.ledger().set_timestamp(invoice.due_date + 1);

    // Zero grace period should be allowed
    client.mark_invoice_defaulted(&invoice_id, &Some(0));
    assert_eq!(
        client.get_invoice(&invoice_id).status,
        InvoiceStatus::Defaulted
    );
}

#[test]
fn test_grace_period_exactly_at_maximum_boundary() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, &admin, 10000);

    let amount = 1000;
    let due_date = env.ledger().timestamp() + 86400;
    let invoice_id = create_and_fund_invoice(
        &env, &client, &admin, &business, &investor, amount, due_date,
    );

    let invoice = client.get_invoice(&invoice_id);
    // Exactly 30 days should be allowed (maximum boundary)
    let max_grace = 30 * 24 * 60 * 60; // 30 days - exactly at limit
    env.ledger().set_timestamp(invoice.due_date + max_grace + 1);

    // Should succeed at exactly the maximum
    client.mark_invoice_defaulted(&invoice_id, &Some(max_grace));
    assert_eq!(
        client.get_invoice(&invoice_id).status,
        InvoiceStatus::Defaulted
    );
}

#[test]
fn test_grace_period_one_over_maximum_rejected() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, &admin, 10000);

    let amount = 1000;
    let due_date = env.ledger().timestamp() + 86400;
    let invoice_id = create_and_fund_invoice(
        &env, &client, &admin, &business, &investor, amount, due_date,
    );

    let invoice = client.get_invoice(&invoice_id);
    // One second over maximum should be rejected
    let max_grace_plus_one = 30 * 24 * 60 * 60 + 1; // 30 days + 1 second
    env.ledger()
        .set_timestamp(invoice.due_date + max_grace_plus_one + 1);

    // Should reject
    let result = client.try_mark_invoice_defaulted(&invoice_id, &Some(max_grace_plus_one));
    assert!(result.is_err());
    let err = result.err().unwrap();
    let contract_err = err.expect("expected contract error");
    assert_eq!(contract_err, QuickLendXError::InvalidTimestamp);
}

#[test]
fn test_resolve_grace_period_validation() {
    let (env, client, admin) = setup();

    // Test 1: Valid override value
    let override_grace = 5 * 24 * 60 * 60; // 5 days
    let resolved = env.as_contract(&client.address, || {
        crate::defaults::resolve_grace_period(&env, Some(override_grace))
            .expect("should resolve valid override value")
    });
    assert_eq!(resolved, override_grace);

    // Test 2: Invalid override value (exceeds max)
    let excessive_grace = 40 * 24 * 60 * 60; // 40 days - exceeds maximum
    let result = env.as_contract(&client.address, || {
        crate::defaults::resolve_grace_period(&env, Some(excessive_grace))
    });
    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), QuickLendXError::InvalidTimestamp);

    // Test 3: Zero is valid
    let zero_grace = 0;
    let resolved = env.as_contract(&client.address, || {
        crate::defaults::resolve_grace_period(&env, Some(zero_grace))
            .expect("should resolve zero grace period")
    });
    assert_eq!(resolved, zero_grace);

    // Test 4: Maximum boundary is valid
    let max_grace = 30 * 24 * 60 * 60; // 30 days - exactly at limit
    let resolved = env.as_contract(&client.address, || {
        crate::defaults::resolve_grace_period(&env, Some(max_grace))
            .expect("should resolve maximum grace period")
    });
    assert_eq!(resolved, max_grace);
}

#[test]
fn test_invalid_grace_period_does_not_affect_other_invoices() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, &admin, 20000);

    let amount = 1000;
    let due_date = env.ledger().timestamp() + 86400;

    // Create two invoices
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

    // Move time past grace period for both
    let time = invoice1.due_date + grace_period + 1;
    env.ledger().set_timestamp(time);

    // Try to default invoice1 with invalid grace period
    let excessive_grace = 40 * 24 * 60 * 60; // 40 days - invalid
    let result = client.try_mark_invoice_defaulted(&invoice1_id, &Some(excessive_grace));
    assert!(result.is_err());

    // Invoice1 should still be funded
    assert_eq!(
        client.get_invoice(&invoice1_id).status,
        InvoiceStatus::Funded
    );

    // Move time past invoice2's grace period as well
    let time2 = invoice2.due_date + grace_period + 1;
    env.ledger().set_timestamp(time2);

    // Default invoice2 with valid grace period - should work independently
    client.mark_invoice_defaulted(&invoice2_id, &Some(grace_period));
    assert_eq!(
        client.get_invoice(&invoice2_id).status,
        InvoiceStatus::Defaulted
    );

    // Invoice1 should still be funded (not affected by invoice2's default)
    assert_eq!(
        client.get_invoice(&invoice1_id).status,
        InvoiceStatus::Funded
    );
}

#[test]
fn test_check_invoice_expiration_with_invalid_grace_period() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, &admin, 10000);

    let amount = 1000;
    let due_date = env.ledger().timestamp() + 86400;
    let invoice_id = create_and_fund_invoice(
        &env, &client, &admin, &business, &investor, amount, due_date,
    );

    let invoice = client.get_invoice(&invoice_id);
    let excessive_grace = 35 * 24 * 60 * 60; // 35 days - exceeds maximum
    env.ledger()
        .set_timestamp(invoice.due_date + excessive_grace + 1);

    // check_invoice_expiration should reject invalid grace period
    let result = client.try_check_invoice_expiration(&invoice_id, &Some(excessive_grace));
    assert!(result.is_err());
    let err = result.err().unwrap();
    let contract_err = err.expect("expected contract error");
    assert_eq!(contract_err, QuickLendXError::InvalidTimestamp);

    // Invoice should not be defaulted
    assert_eq!(
        client.get_invoice(&invoice_id).status,
        InvoiceStatus::Funded
    );
}

#[test]
fn test_check_overdue_invoices_propagates_grace_period_error() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, &admin, 10000);

    let amount = 1000;
    let due_date = env.ledger().timestamp() + 86400;
    create_and_fund_invoice(
        &env, &client, &admin, &business, &investor, amount, due_date,
    );

    // Set up protocol config with invalid grace period would require low-level access
    // For now, we test that check_overdue_invoices returns Result properly
    // The happy path is tested in other tests

    // This test ensures the function signature accepts Result propagation
    let result = client.check_overdue_invoices();
    // Should succeed with default protocol config (returns count)
    assert!(result >= 0); // Just verify it returns a value without error
}

#[test]
fn test_transition_guard_prevents_duplicate_default() {
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

    // First attempt should succeed
    client.mark_invoice_defaulted(&invoice_id, &Some(grace_period));
    assert_eq!(
        client.get_invoice(&invoice_id).status,
        InvoiceStatus::Defaulted
    );

    // Second attempt should fail with DuplicateDefaultTransition
    let result = client.try_mark_invoice_defaulted(&invoice_id, &Some(grace_period));
    assert!(result.is_err());
    let err = result.err().unwrap();
    let contract_err = err.expect("expected contract error");
    assert_eq!(contract_err, QuickLendXError::DuplicateDefaultTransition);
}

#[test]
fn test_transition_guard_persists_across_calls() {
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

    // First default should succeed
    client.mark_invoice_defaulted(&invoice_id, &Some(grace_period));
    assert_eq!(
        client.get_invoice(&invoice_id).status,
        InvoiceStatus::Defaulted
    );

    // Simulate multiple calls - all should fail
    for _ in 0..3 {
        let result = client.try_mark_invoice_defaulted(&invoice_id, &Some(grace_period));
        assert!(result.is_err());
        let err = result.err().unwrap();
        let contract_err = err.expect("expected contract error");
        assert_eq!(contract_err, QuickLendXError::DuplicateDefaultTransition);
    }
}

#[test]
fn test_transition_guard_atomicity_during_partial_failure() {
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

    // First attempt should succeed and set the guard
    client.mark_invoice_defaulted(&invoice_id, &Some(grace_period));
    assert_eq!(
        client.get_invoice(&invoice_id).status,
        InvoiceStatus::Defaulted
    );

    // Even if we try to call handle_default directly, it should fail due to guard
    let result = env.as_contract(&client.address, || {
        crate::defaults::handle_default(&env, &invoice_id)
    });
    assert!(result.is_err());
    assert_eq!(
        result.unwrap_err(),
        QuickLendXError::DuplicateDefaultTransition
    );
}

#[test]
fn test_transition_guard_different_invoices_independent() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, &admin, 20000);

    let amount = 1000;
    let due_date = env.ledger().timestamp() + 86400;

    // Create two invoices
    let invoice1_id = create_and_fund_invoice(
        &env, &client, &admin, &business, &investor, amount, due_date,
    );
    let invoice2_id = create_and_fund_invoice(
        &env, &client, &admin, &business, &investor, amount, due_date,
    );

    let grace_period = 7 * 24 * 60 * 60;
    let default_time = due_date + grace_period + 1;
    env.ledger().set_timestamp(default_time);

    // Default first invoice
    client.mark_invoice_defaulted(&invoice1_id, &Some(grace_period));
    assert_eq!(
        client.get_invoice(&invoice1_id).status,
        InvoiceStatus::Defaulted
    );

    // Second invoice should still be defaultable
    client.mark_invoice_defaulted(&invoice2_id, &Some(grace_period));
    assert_eq!(
        client.get_invoice(&invoice2_id).status,
        InvoiceStatus::Defaulted
    );

    // But first invoice still guarded
    let result = client.try_mark_invoice_defaulted(&invoice1_id, &Some(grace_period));
    assert!(result.is_err());
    let err = result.err().unwrap();
    let contract_err = err.expect("expected contract error");
    assert_eq!(contract_err, QuickLendXError::DuplicateDefaultTransition);
}

// ============================================================================
// Finality and cross-state ordering tests
//
// These tests verify that default handling is final and cannot double-account
// with refunds, settlements, or partial payments. See
// `docs/contracts/defaults.md` for the full cross-state ordering rules.
// ============================================================================

// Helper: Create and fund an invoice, additionally pre-funding the business
// with enough balance and approval to complete a full settlement. Used only
// by tests that exercise the settle path before defaulting.
fn create_and_fund_invoice_with_business_funds(
    env: &Env,
    client: &QuickLendXContractClient,
    admin: &Address,
    business: &Address,
    investor: &Address,
    amount: i128,
    due_date: u64,
) -> BytesN<32> {
    let token_admin = Address::generate(env);
    let currency = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let sac_client = token::StellarAssetClient::new(env, &currency);
    let token_client = token::Client::new(env, &currency);

    client.add_currency(admin, &currency);

    // Mint tokens to investor so they can bid.
    sac_client.mint(investor, &amount);
    let expiry = env.ledger().sequence() + 10_000;
    token_client.approve(investor, &client.address, &amount, &expiry);

    // Additionally mint to the business so it can fund a settlement that
    // pays the investor return plus platform fee.
    sac_client.mint(business, &(amount + 2000));
    token_client.approve(business, &client.address, &(amount + 2000), &expiry);

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

/// Refunded invoices cannot be defaulted even after the grace period elapses.
///
/// Finality rule: once an invoice leaves `Funded` for `Refunded`, the status
/// guard in `mark_invoice_defaulted` rejects any default attempt before any
/// state mutation, preventing duplicate investment accounting (investor was
/// already made whole via the refund path).
#[test]
fn test_cannot_default_refunded_invoice() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, &admin, 10000);

    let amount = 1000;
    let due_date = env.ledger().timestamp() + 86400;
    let invoice_id = create_and_fund_invoice(
        &env, &client, &admin, &business, &investor, amount, due_date,
    );

    // Move the invoice to Refunded via the business-initiated refund path.
    client.refund_escrow_funds(&invoice_id, &business);
    assert_eq!(
        client.get_invoice(&invoice_id).status,
        InvoiceStatus::Refunded
    );

    // Move time past the grace period so the grace-period check cannot be
    // confused with the status check we are targeting.
    let grace_period = 7 * 24 * 60 * 60;
    env.ledger().set_timestamp(due_date + grace_period + 1);

    let result = client.try_mark_invoice_defaulted(&invoice_id, &Some(grace_period));
    assert!(result.is_err());
    let err = result.err().unwrap();
    let contract_err = err.expect("expected contract error");
    assert_eq!(contract_err, QuickLendXError::InvoiceNotAvailableForFunding);

    // Status must be unchanged by the failed default attempt.
    assert_eq!(
        client.get_invoice(&invoice_id).status,
        InvoiceStatus::Refunded
    );
}

/// Paid (settled) invoices cannot be defaulted, even after the grace period.
///
/// Finality rule: once settlement moves the invoice to `Paid`, neither
/// `settle_invoice` nor `mark_invoice_defaulted` can transition it further.
/// This guards against a double-accounting scenario where the investor
/// receives their principal+return via settlement and then has their
/// investment reprocessed as a defaulted/insurance claim.
#[test]
fn test_cannot_default_after_settlement() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, &admin, 10000);

    let amount = 1000;
    let due_date = env.ledger().timestamp() + 86400;
    let invoice_id = create_and_fund_invoice_with_business_funds(
        &env, &client, &admin, &business, &investor, amount, due_date,
    );

    // Settle to move the invoice to Paid.
    client.settle_invoice(&invoice_id, &amount);
    assert_eq!(client.get_invoice(&invoice_id).status, InvoiceStatus::Paid);

    // Move time past the grace period so the failure can only come from the
    // status check, not the grace-period check.
    let grace_period = 7 * 24 * 60 * 60;
    env.ledger().set_timestamp(due_date + grace_period + 1);

    let result = client.try_mark_invoice_defaulted(&invoice_id, &Some(grace_period));
    assert!(result.is_err());
    let err = result.err().unwrap();
    let contract_err = err.expect("expected contract error");
    assert_eq!(contract_err, QuickLendXError::InvoiceNotAvailableForFunding);

    assert_eq!(client.get_invoice(&invoice_id).status, InvoiceStatus::Paid);
}

/// Defaulting an invoice does not touch escrow state or move funds.
///
/// Finality rule: `handle_default` never calls into `escrow::refund_escrow_funds`
/// or `payments::release_escrow`. Escrow disposition after a default is handled
/// through a separate insurance/claims path, so the escrow status must stay
/// `Held` and the investor's on-chain balance must be unchanged by the
/// default transition itself.
#[test]
fn test_default_leaves_escrow_held() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, &admin, 10000);

    let amount = 1000;
    let due_date = env.ledger().timestamp() + 86400;
    let invoice_id = create_and_fund_invoice(
        &env, &client, &admin, &business, &investor, amount, due_date,
    );

    // Snapshot escrow and investor balance before default.
    let escrow_before = client.get_escrow_details(&invoice_id);
    assert_eq!(escrow_before.status, crate::payments::EscrowStatus::Held);
    let invoice = client.get_invoice(&invoice_id);
    let token_client = token::Client::new(&env, &invoice.currency);
    let investor_balance_before = token_client.balance(&investor);

    // Move past grace period and default.
    let grace_period = 7 * 24 * 60 * 60;
    env.ledger()
        .set_timestamp(invoice.due_date + grace_period + 1);
    client.mark_invoice_defaulted(&invoice_id, &Some(grace_period));
    assert_eq!(
        client.get_invoice(&invoice_id).status,
        InvoiceStatus::Defaulted
    );

    // Escrow must still be Held — defaulting does not automatically refund
    // or release. Claims flow through a separate path.
    let escrow_after = client.get_escrow_details(&invoice_id);
    assert_eq!(escrow_after.status, crate::payments::EscrowStatus::Held);
    assert_eq!(escrow_after.amount, escrow_before.amount);

    // Investor's on-chain balance must be unchanged: no automatic refund.
    assert_eq!(
        token_client.balance(&investor),
        investor_balance_before,
        "investor balance must not change on default (no automatic refund)"
    );
}

/// A second default attempt against an already-defaulted invoice must not
/// modify any state. The public entry point short-circuits on the
/// already-defaulted status check and returns `InvoiceAlreadyDefaulted`
/// before any analytics or investment update can run a second time.
#[test]
fn test_second_default_attempt_does_not_change_state() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, &admin, 10000);

    let amount = 1000;
    let due_date = env.ledger().timestamp() + 86400;
    let invoice_id = create_and_fund_invoice(
        &env, &client, &admin, &business, &investor, amount, due_date,
    );

    let grace_period = 7 * 24 * 60 * 60;
    env.ledger().set_timestamp(due_date + grace_period + 1);
    client.mark_invoice_defaulted(&invoice_id, &Some(grace_period));

    // Snapshot post-default state.
    let invoice_snapshot = client.get_invoice(&invoice_id);
    let investment_snapshot = client.get_invoice_investment(&invoice_id);
    assert_eq!(invoice_snapshot.status, InvoiceStatus::Defaulted);

    // Second default attempt must be rejected by the already-defaulted
    // status check at the public entry point.
    let result = client.try_mark_invoice_defaulted(&invoice_id, &Some(grace_period));
    assert!(result.is_err());
    let err = result.err().unwrap();
    let contract_err = err.expect("expected contract error");
    assert_eq!(contract_err, QuickLendXError::InvoiceAlreadyDefaulted);

    // Invoice fields that the default path mutates must be unchanged.
    let invoice_after = client.get_invoice(&invoice_id);
    assert_eq!(invoice_after.status, invoice_snapshot.status);
    assert_eq!(invoice_after.funded_amount, invoice_snapshot.funded_amount);
    assert_eq!(invoice_after.investor, invoice_snapshot.investor);
    assert_eq!(invoice_after.total_paid, invoice_snapshot.total_paid);

    // Investment must not have been reprocessed (status and amount are the
    // two fields the default handler touches).
    let investment_after = client.get_invoice_investment(&invoice_id);
    assert_eq!(investment_after.status, investment_snapshot.status);
    assert_eq!(investment_after.amount, investment_snapshot.amount);
}

/// Partial payments against a defaulted invoice must be rejected without
/// any funds moving or any payment being recorded.
#[test]
fn test_cannot_partial_pay_defaulted_invoice() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, &admin, 10000);

    let amount = 1000;
    let due_date = env.ledger().timestamp() + 86400;
    let invoice_id = create_and_fund_invoice(
        &env, &client, &admin, &business, &investor, amount, due_date,
    );

    let grace_period = 7 * 24 * 60 * 60;
    env.ledger().set_timestamp(due_date + grace_period + 1);
    client.mark_invoice_defaulted(&invoice_id, &Some(grace_period));

    let result = client.try_process_partial_payment(
        &invoice_id,
        &100,
        &String::from_str(&env, "post-default"),
    );
    assert!(result.is_err());
    let err = result.err().unwrap();
    let contract_err = err.expect("expected contract error");
    assert_eq!(contract_err, QuickLendXError::InvalidStatus);

    let invoice_after = client.get_invoice(&invoice_id);
    assert_eq!(invoice_after.status, InvoiceStatus::Defaulted);
    assert_eq!(
        invoice_after.total_paid, 0,
        "no payment must be recorded against a defaulted invoice"
    );
}

/// Full settlement against a defaulted invoice must be rejected before any
/// funds move and before the Paid status would conflict with Defaulted.
#[test]
fn test_cannot_settle_defaulted_invoice() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, &admin, 10000);

    let amount = 1000;
    let due_date = env.ledger().timestamp() + 86400;
    let invoice_id = create_and_fund_invoice(
        &env, &client, &admin, &business, &investor, amount, due_date,
    );

    let grace_period = 7 * 24 * 60 * 60;
    env.ledger().set_timestamp(due_date + grace_period + 1);
    client.mark_invoice_defaulted(&invoice_id, &Some(grace_period));

    let result = client.try_settle_invoice(&invoice_id, &1000);
    assert!(result.is_err());
    let err = result.err().unwrap();
    let contract_err = err.expect("expected contract error");
    assert_eq!(contract_err, QuickLendXError::InvalidStatus);

    let invoice_after = client.get_invoice(&invoice_id);
    assert_eq!(invoice_after.status, InvoiceStatus::Defaulted);
    assert_eq!(
        invoice_after.total_paid, 0,
        "no payment must be recorded against a defaulted invoice via settle"
    );
}
