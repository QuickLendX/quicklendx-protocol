/// Tests for check_overdue_invoices and check_invoice_expiration
///
/// Coverage targets:
/// - check_overdue_invoices: count accuracy, notification dispatch
/// - check_invoice_expiration: true on default, false when not expired, grace boundary
use super::*;
use crate::defaults::{DEFAULT_GRACE_PERIOD, DEFAULT_OVERDUE_SCAN_BATCH_LIMIT};
use crate::invoice::{InvoiceCategory, InvoiceStatus};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, BytesN, Env, String, Vec,
};

// --- Helpers (mirrors test_default.rs patterns) ---

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
    sac_client.mint(investor, &amount);
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
    env.ledger()
        .set_timestamp(due_date + DEFAULT_GRACE_PERIOD + 1);

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

    env.ledger()
        .set_timestamp(due_date + DEFAULT_GRACE_PERIOD + 1);

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

    env.ledger()
        .set_timestamp(due_date + DEFAULT_GRACE_PERIOD + 1);

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

#[test]
fn test_scan_overdue_invoices_respects_explicit_batch_limit() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, &admin, 100000);
    let base = env.ledger().timestamp();
    let grace = 30 * 24 * 60 * 60;

    for offset in 0..5u64 {
        create_and_fund_invoice(
            &env,
            &client,
            &admin,
            &business,
            &investor,
            1000 + offset as i128,
            base + 86400 + offset,
        );
    }

    env.ledger().set_timestamp(base + 86400 + 10);

    let first = client.scan_overdue_invoices(&Some(grace), &Some(2));
    assert_eq!(first.scanned_count, 2);
    assert_eq!(first.overdue_count, 2);
    assert_eq!(first.total_funded, 5);
    assert_eq!(first.next_cursor, 2);
    assert_eq!(client.get_overdue_scan_cursor(), 2);

    let second = client.scan_overdue_invoices(&Some(grace), &Some(2));
    assert_eq!(second.scanned_count, 2);
    assert_eq!(second.overdue_count, 2);
    assert_eq!(second.total_funded, 5);
    assert_eq!(second.next_cursor, 4);
    assert_eq!(client.get_overdue_scan_cursor(), 4);
}

#[test]
fn test_scan_overdue_invoices_cursor_wraps_after_repeated_calls() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, &admin, 100000);
    let base = env.ledger().timestamp();
    let grace = 30 * 24 * 60 * 60;

    for offset in 0..5u64 {
        create_and_fund_invoice(
            &env,
            &client,
            &admin,
            &business,
            &investor,
            2000 + offset as i128,
            base + 86400 + offset,
        );
    }

    env.ledger().set_timestamp(base + 86400 + 10);

    let mut cursors = Vec::new(&env);
    for _ in 0..5 {
        let result = client.scan_overdue_invoices(&Some(grace), &Some(2));
        cursors.push_back(result.next_cursor);
    }

    assert_eq!(cursors.len(), 5);
    assert_eq!(cursors.get(0).unwrap(), 2);
    assert_eq!(cursors.get(1).unwrap(), 4);
    assert_eq!(cursors.get(2).unwrap(), 1);
    assert_eq!(cursors.get(3).unwrap(), 3);
    assert_eq!(cursors.get(4).unwrap(), 0);
    assert_eq!(client.get_overdue_scan_cursor(), 0);
}

#[test]
fn test_check_overdue_invoices_reports_default_batch_limit_configuration() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, &admin, 100000);
    let base = env.ledger().timestamp();

    for offset in 0..5u64 {
        create_and_fund_invoice(
            &env,
            &client,
            &admin,
            &business,
            &investor,
            3000 + offset as i128,
            base + 86400 + offset,
        );
    }

    env.ledger().set_timestamp(base + 86400 + 10);

    assert_eq!(
        client.get_overdue_scan_batch_limit(),
        DEFAULT_OVERDUE_SCAN_BATCH_LIMIT
    );
    assert_eq!(client.get_overdue_scan_batch_limit_max(), 100);

    let count = client.check_overdue_invoices();
    assert_eq!(count, 5);
    assert_eq!(client.get_overdue_scan_cursor(), 0);
}

#[test]
fn test_scan_overdue_invoices_defaults_only_scanned_expired_invoices() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, &admin, 100000);
    let base = env.ledger().timestamp();
    let grace = 0u64;
    let mut invoice_ids = Vec::new(&env);

    for offset in 0..4u64 {
        let invoice_id = create_and_fund_invoice(
            &env,
            &client,
            &admin,
            &business,
            &investor,
            4000 + offset as i128,
            base + 86400 + offset,
        );
        invoice_ids.push_back(invoice_id);
    }

    env.ledger().set_timestamp(base + 86400 + 10);

    let result = client.scan_overdue_invoices(&Some(grace), &Some(2));
    assert_eq!(result.scanned_count, 2);
    assert_eq!(result.overdue_count, 2);
    assert_eq!(result.total_funded, 4);

    assert_eq!(
        client.get_invoice(&invoice_ids.get(0).unwrap()).status,
        InvoiceStatus::Defaulted
    );
    assert_eq!(
        client.get_invoice(&invoice_ids.get(1).unwrap()).status,
        InvoiceStatus::Defaulted
    );
    assert_eq!(
        client.get_invoice(&invoice_ids.get(2).unwrap()).status,
        InvoiceStatus::Funded
    );
    assert_eq!(
        client.get_invoice(&invoice_ids.get(3).unwrap()).status,
        InvoiceStatus::Funded
    );
}

#[test]
fn test_cleanup_expired_bids_integration() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, &admin, 100_000);
    
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400 * 30; // 30 days away

    let invoice_id = client.store_invoice(
        &business,
        &10_000,
        &currency,
        &due_date,
        &String::from_str(&env, "Test invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    client.verify_invoice(&invoice_id);

    // Place 3 bids at different times
    let _bid1 = client.place_bid(&investor, &invoice_id, &1000, &1100);
    
    env.ledger().set_timestamp(env.ledger().timestamp() + 86400); // +1 day
    let _bid2 = client.place_bid(&investor, &invoice_id, &2000, &2200);

    env.ledger().set_timestamp(env.ledger().timestamp() + 86400 * 5); // +5 days (total 6 days)
    let _bid3 = client.place_bid(&investor, &invoice_id, &3000, &3300);

    // After 2 more days, bid 1 and 2 should be expired (7 days TTL)
    env.ledger().set_timestamp(env.ledger().timestamp() + 86400 * 2); // +2 days (total 8 days)
    
    // Cleanup - should return 2 (bid 1 and 2 expired)
    let cleaned = client.cleanup_expired_bids(&invoice_id);
    assert_eq!(cleaned, 2, "Should clean 2 expired bids");

    // After 6 more days, bid 3 should be expired
    env.ledger().set_timestamp(env.ledger().timestamp() + 86400 * 6);
    let cleaned2 = client.cleanup_expired_bids(&invoice_id);
    assert_eq!(cleaned2, 1, "Should clean last expired bid");

    // Final check
    assert_eq!(client.cleanup_expired_bids(&invoice_id), 0, "No more bids to clean");
}
