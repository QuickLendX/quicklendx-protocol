// NOTE: This suite has been updated for the caller-cannot-choose-grace API.
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
    token, Address, BytesN, Env, String, Vec,
};

// Helper: Setup contract with admin and core config
pub fn setup() -> (Env, QuickLendXContractClient<'static>, Address) {
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
    _admin: &Address,
) -> Address {
    let business = Address::generate(env);
    client.submit_kyc_application(&business, &String::from_str(env, "KYC data"));
    client.verify_business(_admin, &business);
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
        &None,
    );
    client.verify_invoice(&invoice_id);

    let bid_id = client.place_bid(
        investor,
        &invoice_id,
        &amount,
        &(amount + 100),
        &BytesN::from_array(&env, &[0u8; 32]),
    );
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
    client.mark_invoice_defaulted(&invoice_id);

    let defaulted_invoice = client.get_invoice(&invoice_id);
    assert_eq!(defaulted_invoice.status, InvoiceStatus::Defaulted);
}d
    assert_eq!(client.get_business_default_history(&business), 0);
    client.mark_invoice_defaulted(&invoice_id);

    // Verify invoice is now defaulted
    let defaulted_invoice = client.get_invoice(&invoice_id);
    assert_eq!(defaulted_invoice.status, InvoiceStatus::Defaulted);

    // Verify business default history counter incremented
    assert_eq!(client.get_business_default_history(&business), 1);
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
    let result = client.try_mark_invoice_defaulted(&invoice_id);
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

    client.mark_invoice_defaulted(&invoice_id);

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

    let did_default = client.check_invoice_expiration(&invoice_id);
    assert!(did_default);

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
        &None,
    );
    client.verify_invoice(&invoice_id);

    // Verify invoice is verified, not funded
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Verified);

    // Try to mark unfunded invoice as defaulted - should fail
    let result = client.try_mark_invoice_defaulted(&invoice_id);
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
        &None,
    );

    // Invoice is pending, not verified
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Pending);

    // Try to mark pending invoice as defaulted - should fail
    let result = client.try_mark_invoice_defaulted(&invoice_id);
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
    client.mark_invoice_defaulted(&invoice_id);

    // Try to mark as defaulted again - should fail with DuplicateDefaultTransition (guard fires first)
    let result = client.try_mark_invoice_defaulted(&invoice_id);
    assert!(result.is_err());
    let err = result.err().unwrap();
    let contract_err = err.expect("expected contract error");
    assert_eq!(contract_err, QuickLendXError::DuplicateDefaultTransition);
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
    client.mark_invoice_defaulted(&invoice_id);

    let defaulted_invoice = client.get_invoice(&invoice_id);
    assert_eq!(defaulted_invoice.status, InvoiceStatus::Defaulted);
}
