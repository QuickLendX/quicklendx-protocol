/// Tests for check_overdue_invoices and check_invoice_expiration
///
/// Coverage targets:
/// - check_overdue_invoices: count accuracy, notification dispatch
/// - check_invoice_expiration: true on default, false when not expired, grace boundary
use super::*;
use crate::defaults::DEFAULT_GRACE_PERIOD;
use crate::errors::QuickLendXError;
use crate::init::ProtocolInitializer;
use crate::invoice::{InvoiceCategory, InvoiceStatus};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, BytesN, Env, String, Vec,
};

// --- Helpers (mirrors test_default.rs patterns) ---

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
    let min_invoice_amount = ProtocolInitializer::get_min_invoice_amount(env);
    let max_due_date_days = ProtocolInitializer::get_max_due_date_days(env);
    ProtocolInitializer::set_protocol_config(
        env,
        admin,
        min_invoice_amount,
        max_due_date_days,
        grace_period_seconds,
    )
    .expect("protocol config update should succeed");
}

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

// ============================================================
// check_overdue_invoices tests
// ============================================================

#[test]
fn test_check_overdue_invoices_returns_zero_when_none_overdue() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, &admin, 10000);

    let due_date = env.ledger().timestamp() + 86400 * 30; // 30 days out
    create_and_fund_invoice(&env, &client, &admin, &business, &investor, 1000, due_date);

    let count = client.check_overdue_invoices();
    assert_eq!(count, 0);
}

#[test]
fn test_check_overdue_invoices_counts_single_overdue() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, &admin, 10000);

    let due_date = env.ledger().timestamp() + 86400;
    create_and_fund_invoice(&env, &client, &admin, &business, &investor, 1000, due_date);

    // Move past due date but before grace deadline
    env.ledger().set_timestamp(due_date + 1);

    let count = client.check_overdue_invoices();
    assert_eq!(count, 1);
}

#[test]
fn test_check_overdue_invoices_counts_multiple_overdue() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, &admin, 50000);

    let base = env.ledger().timestamp();
    let due1 = base + 86400;
    let due2 = base + 86400 * 2;
    let due3 = base + 86400 * 60; // far future, not overdue

    create_and_fund_invoice(&env, &client, &admin, &business, &investor, 1000, due1);
    create_and_fund_invoice(&env, &client, &admin, &business, &investor, 2000, due2);
    create_and_fund_invoice(&env, &client, &admin, &business, &investor, 3000, due3);

    // Move past due2 but before due3
    env.ledger().set_timestamp(due2 + 1);

    let count = client.check_overdue_invoices();
    assert_eq!(count, 2);
}

#[test]
fn test_check_overdue_invoices_sends_notifications() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, &admin, 10000);

    let due_date = env.ledger().timestamp() + 86400;
    create_and_fund_invoice(&env, &client, &admin, &business, &investor, 1000, due_date);

    env.ledger().set_timestamp(due_date + 1);

    let biz_notifs_before = client.get_user_notifications(&business);
    let inv_notifs_before = client.get_user_notifications(&investor);

    client.check_overdue_invoices();

    // Business and investor should each receive a PaymentOverdue notification
    let biz_notifs_after = client.get_user_notifications(&business);
    let inv_notifs_after = client.get_user_notifications(&investor);

    assert!(biz_notifs_after.len() > biz_notifs_before.len());
    assert!(inv_notifs_after.len() > inv_notifs_before.len());
}

#[test]
fn test_check_overdue_invoices_defaults_past_grace() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, &admin, 10000);

    let due_date = env.ledger().timestamp() + 86400;
    let invoice_id =
        create_and_fund_invoice(&env, &client, &admin, &business, &investor, 1000, due_date);

    // Move past due_date + default grace period
    env.ledger().set_timestamp(due_date + DEFAULT_GRACE_PERIOD + 1);

    let count = client.check_overdue_invoices();
    assert_eq!(count, 1);

    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Defaulted);
}

#[test]
fn test_check_overdue_invoices_no_funded_invoices() {
    let (_env, client, _admin) = setup();

    // No invoices at all
    let count = client.check_overdue_invoices();
    assert_eq!(count, 0);
}

#[test]
fn test_check_overdue_invoices_grace_custom_period() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, &admin, 10000);

    let due_date = env.ledger().timestamp() + 86400;
    let invoice_id =
        create_and_fund_invoice(&env, &client, &admin, &business, &investor, 1000, due_date);

    let custom_grace = 2 * 24 * 60 * 60; // 2 days

    // Past due but before custom grace
    env.ledger().set_timestamp(due_date + custom_grace - 1);
    let count = client.check_overdue_invoices_grace(&custom_grace);
    assert_eq!(count, 1); // overdue
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Funded); // not yet defaulted

    // Past custom grace
    env.ledger().set_timestamp(due_date + custom_grace + 1);
    let count = client.check_overdue_invoices_grace(&custom_grace);
    assert_eq!(count, 1);
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Defaulted);
}

#[test]
fn test_check_overdue_invoices_skips_non_funded() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);

    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;

    // Create a verified-only (not funded) invoice
    client.store_invoice(
        &business,
        &1000,
        &currency,
        &due_date,
        &String::from_str(&env, "Unfunded invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    env.ledger().set_timestamp(due_date + DEFAULT_GRACE_PERIOD + 1);

    let count = client.check_overdue_invoices();
    assert_eq!(count, 0);
}

// ============================================================
// check_invoice_expiration tests
// ============================================================

#[test]
fn test_check_invoice_expiration_returns_true_when_defaulted() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, &admin, 10000);

    let due_date = env.ledger().timestamp() + 86400;
    let invoice_id =
        create_and_fund_invoice(&env, &client, &admin, &business, &investor, 1000, due_date);

    let grace = 3 * 24 * 60 * 60; // 3 days
    env.ledger().set_timestamp(due_date + grace + 1);

    let result = client.check_invoice_expiration(&invoice_id, &Some(grace));
    assert!(result);

    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Defaulted);
}

#[test]
fn test_check_invoice_expiration_returns_false_when_not_expired() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, &admin, 10000);

    let due_date = env.ledger().timestamp() + 86400;
    let invoice_id =
        create_and_fund_invoice(&env, &client, &admin, &business, &investor, 1000, due_date);

    let grace = 7 * 24 * 60 * 60;

    // Before grace deadline
    env.ledger().set_timestamp(due_date + grace - 100);

    let result = client.check_invoice_expiration(&invoice_id, &Some(grace));
    assert!(!result);

    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Funded);
}

#[test]
fn test_check_invoice_expiration_grace_boundary_exact() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, &admin, 10000);

    let due_date = env.ledger().timestamp() + 86400;
    let invoice_id =
        create_and_fund_invoice(&env, &client, &admin, &business, &investor, 1000, due_date);

    let grace = 5 * 24 * 60 * 60; // 5 days
    let deadline = due_date + grace;

    // Exactly at the grace deadline — should NOT default (current <= deadline)
    env.ledger().set_timestamp(deadline);
    let result = client.check_invoice_expiration(&invoice_id, &Some(grace));
    assert!(!result);
    assert_eq!(
        client.get_invoice(&invoice_id).status,
        InvoiceStatus::Funded
    );

    // One second past the deadline — should default
    env.ledger().set_timestamp(deadline + 1);
    let result = client.check_invoice_expiration(&invoice_id, &Some(grace));
    assert!(result);
    assert_eq!(
        client.get_invoice(&invoice_id).status,
        InvoiceStatus::Defaulted
    );
}

#[test]
fn test_check_invoice_expiration_returns_false_for_non_funded() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);

    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;
    let invoice_id = client.store_invoice(
        &business,
        &1000,
        &currency,
        &due_date,
        &String::from_str(&env, "Unfunded"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    client.verify_invoice(&invoice_id);

    env.ledger().set_timestamp(due_date + DEFAULT_GRACE_PERIOD + 1);

    // Verified but not funded — check_and_handle_expiration returns false
    let result = client.check_invoice_expiration(&invoice_id, &None);
    assert!(!result);
}

#[test]
fn test_check_invoice_expiration_not_found() {
    let (env, client, _admin) = setup();

    let fake_id = BytesN::from_array(&env, &[0u8; 32]);
    let result = client.try_check_invoice_expiration(&fake_id, &None);
    assert!(result.is_err());
}

#[test]
fn test_check_invoice_expiration_zero_grace() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, &admin, 10000);

    let due_date = env.ledger().timestamp() + 86400;
    let invoice_id =
        create_and_fund_invoice(&env, &client, &admin, &business, &investor, 1000, due_date);

    // Zero grace: defaults immediately after due_date
    env.ledger().set_timestamp(due_date + 1);
    let result = client.check_invoice_expiration(&invoice_id, &Some(0));
    assert!(result);
    assert_eq!(
        client.get_invoice(&invoice_id).status,
        InvoiceStatus::Defaulted
    );
}

#[test]
fn test_check_invoice_expiration_uses_protocol_grace_when_none() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, &admin, 10000);

    let custom_grace = 2 * 24 * 60 * 60; // 2 days
    set_protocol_grace_period(&env, &admin, custom_grace);

    let due_date = env.ledger().timestamp() + 86400;
    let invoice_id =
        create_and_fund_invoice(&env, &client, &admin, &business, &investor, 1000, due_date);

    // Before protocol grace
    env.ledger().set_timestamp(due_date + custom_grace - 1);
    let result = client.check_invoice_expiration(&invoice_id, &None);
    assert!(!result);
    assert_eq!(
        client.get_invoice(&invoice_id).status,
        InvoiceStatus::Funded
    );

    // Past protocol grace
    env.ledger().set_timestamp(due_date + custom_grace + 1);
    let result = client.check_invoice_expiration(&invoice_id, &None);
    assert!(result);
    assert_eq!(
        client.get_invoice(&invoice_id).status,
        InvoiceStatus::Defaulted
    );
}

#[test]
fn test_check_invoice_expiration_already_defaulted_returns_false() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, &admin, 10000);

    let due_date = env.ledger().timestamp() + 86400;
    let invoice_id =
        create_and_fund_invoice(&env, &client, &admin, &business, &investor, 1000, due_date);

    let grace = 3 * 24 * 60 * 60;
    env.ledger().set_timestamp(due_date + grace + 1);

    // First call defaults the invoice
    let first = client.check_invoice_expiration(&invoice_id, &Some(grace));
    assert!(first);

    // Second call on already-defaulted invoice should return false
    let second = client.check_invoice_expiration(&invoice_id, &Some(grace));
    assert!(!second);
}

#[test]
fn test_check_overdue_invoices_mixed_overdue_and_current() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, &admin, 50000);

    let base = env.ledger().timestamp();
    let overdue_due = base + 86400;
    let current_due = base + 86400 * 90;

    let overdue_id = create_and_fund_invoice(
        &env,
        &client,
        &admin,
        &business,
        &investor,
        1000,
        overdue_due,
    );
    let current_id = create_and_fund_invoice(
        &env,
        &client,
        &admin,
        &business,
        &investor,
        2000,
        current_due,
    );

    env.ledger().set_timestamp(overdue_due + 1);

    let count = client.check_overdue_invoices();
    assert_eq!(count, 1);

    // Overdue invoice still funded (within grace), current invoice untouched
    assert_eq!(
        client.get_invoice(&overdue_id).status,
        InvoiceStatus::Funded
    );
    assert_eq!(
        client.get_invoice(&current_id).status,
        InvoiceStatus::Funded
    );
}
