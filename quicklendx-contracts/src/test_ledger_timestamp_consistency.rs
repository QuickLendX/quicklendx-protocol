#![cfg(test)]
extern crate std;

use crate::{
    invoice::InvoiceCategory,
    QuickLendXContract, QuickLendXContractClient,
};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, BytesN, Env, String, Vec,
};

// ============================================================================
// SETUP & HELPERS
// ============================================================================

fn setup() -> (Env, QuickLendXContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.set_admin(&admin);
    (env, client, admin)
}

fn create_verified_business(
    env: &Env,
    client: &QuickLendXContractClient,
    admin: &Address,
) -> Address {
    let business = Address::generate(env);
    client.submit_kyc_application(&business, &String::from_str(env, "Business KYC"));
    client.verify_business(admin, &business);
    business
}

fn create_verified_investor(
    env: &Env,
    client: &QuickLendXContractClient,
    limit: i128,
) -> Address {
    let investor = Address::generate(env);
    client.submit_investor_kyc(&investor, &String::from_str(env, "Investor KYC"));
    client.verify_investor(&investor, &limit);
    investor
}

fn setup_token(
    env: &Env,
    business: &Address,
    investor: &Address,
    contract_id: &Address,
) -> Address {
    let token_admin = Address::generate(env);
    let currency = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token_client = token::Client::new(env, &currency);
    let sac_client = token::StellarAssetClient::new(env, &currency);

    let initial_balance = 50_000i128;
    sac_client.mint(business, &initial_balance);
    sac_client.mint(investor, &initial_balance);

    let expiration = env.ledger().sequence() + 10_000;
    token_client.approve(business, contract_id, &initial_balance, &expiration);
    token_client.approve(investor, contract_id, &initial_balance, &expiration);

    currency
}

fn create_invoice(
    env: &Env,
    client: &QuickLendXContractClient,
    business: &Address,
    amount: i128,
    currency: &Address,
    due_date: u64,
) -> BytesN<32> {
    client.store_invoice(
        business,
        &amount,
        currency,
        &due_date,
        &String::from_str(env, "Test Invoice"),
        &InvoiceCategory::Services,
        &Vec::new(env),
    )
}

fn create_verified_and_funded_invoice(
    env: &Env,
    client: &QuickLendXContractClient,
    business: &Address,
    investor: &Address,
    amount: i128,
    currency: &Address,
    due_date: u64,
) -> BytesN<32> {
    let invoice_id = create_invoice(env, client, business, amount, currency, due_date);
    client.verify_invoice(&invoice_id);
    let bid_id = client.place_bid(investor, &invoice_id, &amount, &(amount + 1000));
    client.accept_bid(&invoice_id, &bid_id);
    invoice_id
}

// ============================================================================
// MODULE 1: TIMESTAMP SOURCE VERIFICATION
// ============================================================================

#[test]
fn test_created_at_uses_ledger_timestamp() {
    // Verify that Invoice.created_at is captured from env.ledger().timestamp()
    // This ensures the timestamp source is consistent and immutable after creation
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let currency = Address::generate(&env);

    let ts_before = env.ledger().timestamp();
    let due_date = ts_before + 86400;
    let invoice_id = create_invoice(&env, &client, &business, 1000, &currency, due_date);
    let ts_after = env.ledger().timestamp();

    let invoice = client.get_invoice(&invoice_id);
    // created_at must be within the timestamp range captured at creation
    assert!(invoice.created_at >= ts_before && invoice.created_at <= ts_after);
}

#[test]
fn test_created_at_consistent_after_time_advance() {
    // Verify that created_at remains constant even if ledger time advances
    // created_at should be captured at the moment of creation and never change
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let currency = Address::generate(&env);

    let initial_ts = env.ledger().timestamp();
    let due_date = initial_ts + 86400;
    let invoice_id = create_invoice(&env, &client, &business, 1000, &currency, due_date);
    let created_at_1 = client.get_invoice(&invoice_id).created_at;

    // Advance time significantly
    env.ledger().set_timestamp(initial_ts + 10000);

    // Fetch same invoice again - created_at should NOT change
    let created_at_2 = client.get_invoice(&invoice_id).created_at;
    assert_eq!(created_at_1, created_at_2);
}

#[test]
fn test_grace_deadline_calculation_uses_consistent_source() {
    // Verify that grace_deadline is calculated as due_date + grace_period
    // without mixing different time sources (sequence vs timestamp)
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, 50_000);
    let currency = setup_token(&env, &business, &investor, &client.address);
    client.add_currency(&admin, &currency);

    let amount = 10_000i128;
    let ts = env.ledger().timestamp();
    let due_date = ts + 1000;
    let grace_period = 7 * 24 * 60 * 60;

    let invoice_id = create_verified_and_funded_invoice(
        &env, &client, &business, &investor, amount, &currency, due_date,
    );

    let invoice = client.get_invoice(&invoice_id);
    let grace_deadline = invoice.grace_deadline(grace_period);

    // grace_deadline must equal due_date + grace_period (saturating add)
    assert_eq!(grace_deadline, due_date.saturating_add(grace_period));
}

// ============================================================================
// MODULE 2: OFF-BY-ONE & BOUNDARY TESTS
// ============================================================================

#[test]
fn test_overdue_boundary_exact_due_date_not_overdue() {
    // At exactly due_date, invoice should NOT be marked as overdue
    // This tests the < vs <= boundary condition
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, 50_000);
    let currency = setup_token(&env, &business, &investor, &client.address);
    client.add_currency(&admin, &currency);

    let amount = 10_000i128;
    let current_ts = env.ledger().timestamp();
    let due_date = current_ts + 100;

    let invoice_id = create_verified_and_funded_invoice(
        &env, &client, &business, &investor, amount, &currency, due_date,
    );

    // Advance to exact due_date (not past it)
    env.ledger().set_timestamp(due_date);

    let invoice = client.get_invoice(&invoice_id);
    // At due_date, should still be considered "not overdue" (timestamp > due_date is false)
    assert!(!invoice.is_overdue(env.ledger().timestamp()));
}

#[test]
fn test_overdue_boundary_after_due_date_is_overdue() {
    // One second after due_date, invoice should be marked as overdue
    // This verifies the boundary condition is correctly applied
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, 50_000);
    let currency = setup_token(&env, &business, &investor, &client.address);
    client.add_currency(&admin, &currency);

    let amount = 10_000i128;
    let current_ts = env.ledger().timestamp();
    let due_date = current_ts + 100;

    let invoice_id = create_verified_and_funded_invoice(
        &env, &client, &business, &investor, amount, &currency, due_date,
    );

    // Advance one second past due_date
    env.ledger().set_timestamp(due_date + 1);

    let invoice = client.get_invoice(&invoice_id);
    // Past due_date, should be overdue
    assert!(invoice.is_overdue(env.ledger().timestamp()));
}

#[test]
fn test_grace_deadline_boundary_exact_not_defaulted() {
    // At exactly grace_deadline, invoice should NOT be allowed to default
    // Boundary: current_timestamp <= grace_deadline → no default
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, 50_000);
    let currency = setup_token(&env, &business, &investor, &client.address);
    client.add_currency(&admin, &currency);

    let amount = 10_000i128;
    let current_ts = env.ledger().timestamp();
    let due_date = current_ts + 100;
    let grace_period = 1000;

    let invoice_id = create_verified_and_funded_invoice(
        &env, &client, &business, &investor, amount, &currency, due_date,
    );

    let grace_deadline = due_date + grace_period;
    env.ledger().set_timestamp(grace_deadline);

    // At grace_deadline, defaulting should fail
    let result = client.try_mark_invoice_defaulted(&invoice_id, &Some(grace_period));
    assert!(result.is_err(), "Should not allow default at grace_deadline");
}

#[test]
fn test_grace_deadline_boundary_after_allowed_to_default() {
    // One second after grace_deadline, invoice should be allowed to default
    // This verifies the exclusive boundary (> not >=)
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, 50_000);
    let currency = setup_token(&env, &business, &investor, &client.address);
    client.add_currency(&admin, &currency);

    let amount = 10_000i128;
    let current_ts = env.ledger().timestamp();
    let due_date = current_ts + 100;
    let grace_period = 1000;

    let invoice_id = create_verified_and_funded_invoice(
        &env, &client, &business, &investor, amount, &currency, due_date,
    );

    let grace_deadline = due_date + grace_period;
    env.ledger().set_timestamp(grace_deadline + 1);

    // One second after grace_deadline, defaulting should succeed
    let result = client.try_mark_invoice_defaulted(&invoice_id, &Some(grace_period));
    assert!(result.is_ok(), "Should allow default after grace_deadline");
}

#[test]
fn test_timestamp_incremental_advancement_accumulates() {
    // Verify that multiple set_timestamp calls accumulate correctly
    // Tests that time advancement is deterministic and doesn't "reset"
    let (env, _client, _admin) = setup();

    let initial_ts = env.ledger().timestamp();

    env.ledger().set_timestamp(initial_ts + 100);
    let ts1 = env.ledger().timestamp();
    assert_eq!(ts1, initial_ts + 100, "First advance should add 100");

    env.ledger().set_timestamp(initial_ts + 300);
    let ts2 = env.ledger().timestamp();
    assert_eq!(ts2, initial_ts + 300, "Second advance should be absolute, not relative");

    env.ledger().set_timestamp(ts2 + 50);
    let ts3 = env.ledger().timestamp();
    assert_eq!(
        ts3, initial_ts + 350,
        "Relative advance from previous should accumulate"
    );
}

// ============================================================================
// MODULE 3: GRACE PERIOD CONSISTENCY
// ============================================================================

#[test]
fn test_grace_deadline_deterministic() {
    // Verify that grace_deadline(X) always produces the same result for the same inputs
    // Tests that the calculation is pure and consistent
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, 50_000);
    let currency = setup_token(&env, &business, &investor, &client.address);
    client.add_currency(&admin, &currency);

    let amount = 10_000i128;
    let ts = env.ledger().timestamp();
    let due_date = ts + 1000;
    let grace_period = 7 * 24 * 60 * 60;

    let invoice_id = create_verified_and_funded_invoice(
        &env, &client, &business, &investor, amount, &currency, due_date,
    );

    let invoice = client.get_invoice(&invoice_id);
    let deadline_1 = invoice.grace_deadline(grace_period);
    let deadline_2 = invoice.grace_deadline(grace_period);

    // Same invoice, same grace period should always give same deadline
    assert_eq!(deadline_1, deadline_2, "grace_deadline must be deterministic");
}

#[test]
fn test_grace_period_override_per_invoice() {
    // Verify that per-invoice grace period can override the protocol default
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, 50_000);
    let currency = setup_token(&env, &business, &investor, &client.address);
    client.add_currency(&admin, &currency);
    client.initialize_protocol_limits(&admin, &1i128, &365u64, &(7 * 24 * 60 * 60));

    let amount = 10_000i128;
    let current_ts = env.ledger().timestamp();
    let due_date = current_ts + 100;
    let per_invoice_grace = 2 * 24 * 60 * 60; // 2 days override

    let invoice_id = create_verified_and_funded_invoice(
        &env, &client, &business, &investor, amount, &currency, due_date,
    );

    // Move to just past per_invoice_grace deadline
    let deadline = due_date + per_invoice_grace;
    env.ledger().set_timestamp(deadline + 1);

    // Should allow default with per-invoice grace
    let result = client.try_mark_invoice_defaulted(&invoice_id, &Some(per_invoice_grace));
    assert!(result.is_ok(), "Per-invoice grace should override protocol default");
}

#[test]
fn test_grace_calculation_saturation_safe() {
    // Verify that grace_deadline calculation saturates on overflow, not wraps
    // Tests: u64::MAX - 100 + 1000 should saturate to u64::MAX, not wrap
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, 50_000);
    let currency = setup_token(&env, &business, &investor, &client.address);
    client.add_currency(&admin, &currency);

    let amount = 10_000i128;
    let extreme_due_date = u64::MAX - 100;
    let grace_period = 1000;

    let invoice_id = create_verified_and_funded_invoice(
        &env, &client, &business, &investor, amount, &currency, extreme_due_date,
    );

    let invoice = client.get_invoice(&invoice_id);
    let grace_deadline = invoice.grace_deadline(grace_period);

    // Must saturate to u64::MAX, not wrap around
    assert_eq!(
        grace_deadline, u64::MAX,
        "grace_deadline must saturate to u64::MAX on overflow"
    );
}

#[test]
fn test_multiple_invoices_grace_independent() {
    // Verify that different invoices have independent grace deadline calculations
    // Each invoice's grace_deadline depends only on its own due_date
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, 50_000);
    let currency = setup_token(&env, &business, &investor, &client.address);
    client.add_currency(&admin, &currency);

    let amount = 5_000i128;
    let ts = env.ledger().timestamp();
    let grace_period = 1000;

    let due_date_1 = ts + 500;
    let due_date_2 = ts + 1500;

    let invoice_id_1 = create_verified_and_funded_invoice(
        &env, &client, &business, &investor, amount, &currency, due_date_1,
    );
    let invoice_id_2 = create_verified_and_funded_invoice(
        &env, &client, &business, &investor, amount, &currency, due_date_2,
    );

    let inv1 = client.get_invoice(&invoice_id_1);
    let inv2 = client.get_invoice(&invoice_id_2);

    let deadline_1 = inv1.grace_deadline(grace_period);
    let deadline_2 = inv2.grace_deadline(grace_period);

    // Different due dates should produce different grace deadlines
    assert_eq!(deadline_1, due_date_1 + grace_period);
    assert_eq!(deadline_2, due_date_2 + grace_period);
    assert_ne!(deadline_1, deadline_2, "Different invoices should have different grace deadlines");
}

// ============================================================================
// MODULE 4: TIME ADVANCEMENT CORRECTNESS
// ============================================================================

#[test]
fn test_set_timestamp_advance_by_seconds() {
    // Verify that set_timestamp advances the ledger by exactly the specified amount
    let (env, _client, _admin) = setup();

    let initial_ts = env.ledger().timestamp();
    let target_ts = initial_ts + 86400; // 1 day ahead

    env.ledger().set_timestamp(target_ts);
    let actual_ts = env.ledger().timestamp();

    assert_eq!(actual_ts, target_ts, "set_timestamp should advance by exact amount");
}

#[test]
fn test_set_timestamp_multiple_advances() {
    // Verify that multiple sequential advances work correctly and accumulate
    let (env, _client, _admin) = setup();

    let initial_ts = env.ledger().timestamp();

    env.ledger().set_timestamp(initial_ts + 1000);
    assert_eq!(env.ledger().timestamp(), initial_ts + 1000);

    env.ledger().set_timestamp(initial_ts + 1000 + 2000);
    assert_eq!(env.ledger().timestamp(), initial_ts + 3000);

    env.ledger().set_timestamp(initial_ts + 3000 + 5000);
    assert_eq!(env.ledger().timestamp(), initial_ts + 8000);
}

#[test]
fn test_created_at_captures_correct_time_after_advance() {
    // Verify that created_at captures the timestamp at the moment of creation,
    // not some earlier or later time
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let currency = Address::generate(&env);

    let initial_ts = env.ledger().timestamp();

    // Create first invoice at initial time
    let due_date_1 = initial_ts + 1000;
    let invoice_id_1 = create_invoice(&env, &client, &business, 1000, &currency, due_date_1);
    let created_at_1 = client.get_invoice(&invoice_id_1).created_at;

    // Advance time
    env.ledger().set_timestamp(initial_ts + 500);

    // Create second invoice at advanced time
    let due_date_2 = initial_ts + 500 + 1000;
    let invoice_id_2 = create_invoice(&env, &client, &business, 1000, &currency, due_date_2);
    let created_at_2 = client.get_invoice(&invoice_id_2).created_at;

    // created_at_2 should be later than created_at_1 by approximately 500
    assert!(
        created_at_2 >= created_at_1 + 500,
        "created_at should advance with ledger time"
    );
}

#[test]
fn test_ledger_time_consistent_within_transaction() {
    // Verify that env.ledger().timestamp() returns the same value throughout a single transaction
    // (or at least doesn't mysteriously change)
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let currency = Address::generate(&env);

    let fixed_ts = 1_000_000u64;
    env.ledger().set_timestamp(fixed_ts);

    // Create multiple invoices in sequence without advancing time
    let due_date = fixed_ts + 1000;
    let ids: Vec<_> = (0..3)
        .map(|_| create_invoice(&env, &client, &business, 1000, &currency, due_date))
        .collect();

    // All created_at values should be equal or within same second
    let created_ats: Vec<_> = ids.iter()
        .map(|id| client.get_invoice(id).created_at)
        .collect();

    // All should equal fixed_ts
    for created_at in &created_ats {
        assert!(
            *created_at >= fixed_ts && *created_at <= fixed_ts + 1,
            "created_at should be consistent within transaction"
        );
    }
}

// ============================================================================
// MODULE 5: EDGE CASES & INVARIANTS
// ============================================================================

#[test]
fn test_zero_grace_period_defaults_immediately_after_due() {
    // Verify that zero grace period allows default immediately after due_date
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, 50_000);
    let currency = setup_token(&env, &business, &investor, &client.address);
    client.add_currency(&admin, &currency);

    let amount = 10_000i128;
    let current_ts = env.ledger().timestamp();
    let due_date = current_ts + 100;
    let zero_grace = 0;

    let invoice_id = create_verified_and_funded_invoice(
        &env, &client, &business, &investor, amount, &currency, due_date,
    );

    // At due_date + 1 with zero grace, should allow default
    env.ledger().set_timestamp(due_date + 1);

    let result = client.try_mark_invoice_defaulted(&invoice_id, &Some(zero_grace));
    assert!(
        result.is_ok(),
        "Zero grace period should allow default immediately after due_date"
    );
}

#[test]
fn test_invoice_created_before_due_date_required() {
    // Invariant: created_at must always be before due_date
    // (This is a logic invariant that should always hold)
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let currency = Address::generate(&env);

    let ts = env.ledger().timestamp();
    let due_date = ts + 86400; // 1 day in the future

    let invoice_id = create_invoice(&env, &client, &business, 1000, &currency, due_date);
    let invoice = client.get_invoice(&invoice_id);

    assert!(
        invoice.created_at < invoice.due_date,
        "created_at must be before due_date"
    );
}

#[test]
fn test_grace_deadline_never_before_due_date() {
    // Invariant: grace_deadline is always >= due_date
    // (grace_deadline = due_date + grace_period, so always >= due_date)
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, 50_000);
    let currency = setup_token(&env, &business, &investor, &client.address);
    client.add_currency(&admin, &currency);

    let amount = 10_000i128;
    let ts = env.ledger().timestamp();
    let due_date = ts + 1000;
    let grace_period = 7 * 24 * 60 * 60;

    let invoice_id = create_verified_and_funded_invoice(
        &env, &client, &business, &investor, amount, &currency, due_date,
    );

    let invoice = client.get_invoice(&invoice_id);
    let grace_deadline = invoice.grace_deadline(grace_period);

    assert!(
        grace_deadline >= invoice.due_date,
        "grace_deadline must never be before due_date"
    );
}

#[test]
fn test_concurrent_creation_timestamps_ordered() {
    // Verify that invoices created at different timestamps have ordered created_at values
    // Tests that we use set_timestamp correctly between creations
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let currency = Address::generate(&env);

    let initial_ts = env.ledger().timestamp();

    // Create first invoice at initial time
    let invoice_id_1 = create_invoice(&env, &client, &business, 1000, &currency, initial_ts + 1000);
    let created_at_1 = client.get_invoice(&invoice_id_1).created_at;

    // Advance time and create second invoice
    env.ledger().set_timestamp(initial_ts + 100);
    let invoice_id_2 = create_invoice(&env, &client, &business, 1000, &currency, initial_ts + 100 + 1000);
    let created_at_2 = client.get_invoice(&invoice_id_2).created_at;

    // Advance time again and create third invoice
    env.ledger().set_timestamp(initial_ts + 200);
    let invoice_id_3 = create_invoice(&env, &client, &business, 1000, &currency, initial_ts + 200 + 1000);
    let created_at_3 = client.get_invoice(&invoice_id_3).created_at;

    // Verify ordering: created_at_1 < created_at_2 < created_at_3
    assert!(
        created_at_1 <= created_at_2,
        "Second invoice should have later or equal created_at"
    );
    assert!(
        created_at_2 <= created_at_3,
        "Third invoice should have later or equal created_at"
    );
}

// ============================================================================
// MODULE 6: INTEGRATION & REAL-WORLD FLOWS
// ============================================================================

#[test]
fn test_real_world_invoice_lifecycle_with_time_advances() {
    // Comprehensive integration test simulating a realistic invoice workflow
    // with multiple time advances and state transitions
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, 100_000);
    let currency = setup_token(&env, &business, &investor, &client.address);
    client.add_currency(&admin, &currency);
    client.initialize_protocol_limits(&admin, &1i128, &365u64, &(7 * 24 * 60 * 60));

    let amount = 10_000i128;
    let ts_creation = env.ledger().timestamp();

    // Day 0: Create invoice (1 day due date)
    let due_date = ts_creation + 86400;
    let invoice_id = create_invoice(&env, &client, &business, amount, &currency, due_date);
    let created_at = client.get_invoice(&invoice_id).created_at;
    assert_eq!(created_at, ts_creation);

    // Day 0: Verify and fund invoice
    client.verify_invoice(&invoice_id);
    let bid_id = client.place_bid(&investor, &invoice_id, &amount, &(amount + 1000));
    client.accept_bid(&invoice_id, &bid_id);
    let funded_at = client.get_invoice(&invoice_id).funded_at;
    assert!(funded_at.is_some());

    // Day 1: Invoice due (but not overdue, grace period active)
    env.ledger().set_timestamp(due_date);
    let invoice = client.get_invoice(&invoice_id);
    assert!(!invoice.is_overdue(env.ledger().timestamp()));

    // Day 3: Still in grace period (7 days default)
    env.ledger().set_timestamp(due_date + 2 * 86400);
    let invoice = client.get_invoice(&invoice_id);
    let grace_deadline = invoice.grace_deadline(7 * 24 * 60 * 60);
    let current_ts = env.ledger().timestamp();
    assert!(current_ts <= grace_deadline, "Should still be in grace period");

    // Day 8: Grace period expired, default allowed
    env.ledger().set_timestamp(grace_deadline + 1);
    let result = client.try_mark_invoice_defaulted(&invoice_id, &None);
    assert!(result.is_ok(), "Should allow default after grace period");

    // Verify final state
    let final_invoice = client.get_invoice(&invoice_id);
    assert_eq!(final_invoice.status, crate::invoice::InvoiceStatus::Defaulted);
}

#[test]
fn test_multiple_invoices_lifecycle_with_sequential_creations() {
    // Test creating multiple invoices at different times and verifying each
    // has correct timestamp and grace deadline progression
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, 500_000);
    let currency = setup_token(&env, &business, &investor, &client.address);
    client.add_currency(&admin, &currency);

    let amount = 5_000i128;
    let grace_period = 3 * 24 * 60 * 60; // 3 days
    let mut invoice_ids = Vec::new(&env);
    let mut created_ats = Vec::new(&env);

    // Create 5 invoices at 1-day intervals
    for i in 0..5 {
        let ts = env.ledger().timestamp();
        let due_date = ts + 86400; // Each due in 1 day from creation

        let invoice_id = create_verified_and_funded_invoice(
            &env, &client, &business, &investor, amount, &currency, due_date,
        );
        let created_at = client.get_invoice(&invoice_id).created_at;

        invoice_ids.push_back(invoice_id);
        created_ats.push_back(created_at);

        // Advance to next day
        if i < 4 {
            env.ledger().set_timestamp(ts + 86400);
        }
    }

    // Verify ordering: created_at values should be monotonically increasing
    for i in 0..4 {
        let current = created_ats.get(i).unwrap();
        let next = created_ats.get(i + 1).unwrap();
        assert!(
            current <= next,
            "created_at values should be ordered (or equal if same second)"
        );
    }

    // Verify each invoice's grace deadline is consistent
    for invoice_id in invoice_ids.iter() {
        let invoice = client.get_invoice(invoice_id);
        let grace_deadline = invoice.grace_deadline(grace_period);
        assert!(grace_deadline >= invoice.due_date);
        assert_eq!(grace_deadline, invoice.due_date.saturating_add(grace_period));
    }
}

#[test]
fn test_boundary_stress_test_multiple_threshold_crossings() {
    // Stress test crossing multiple time boundaries in sequence
    // Simulates: creation -> past due -> grace period expires
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, 50_000);
    let currency = setup_token(&env, &business, &investor, &client.address);
    client.add_currency(&admin, &currency);

    let amount = 10_000i128;
    let ts_start = env.ledger().timestamp();
    let due_date = ts_start + 1000;
    let grace_period = 2000;
    let grace_deadline = due_date + grace_period;

    let invoice_id = create_verified_and_funded_invoice(
        &env, &client, &business, &investor, amount, &currency, due_date,
    );

    // Test 1: Before due_date
    env.ledger().set_timestamp(due_date - 1);
    let invoice = client.get_invoice(&invoice_id);
    assert!(!invoice.is_overdue(env.ledger().timestamp()));
    assert!(
        client.try_mark_invoice_defaulted(&invoice_id, &Some(grace_period)).is_err()
    );

    // Test 2: At due_date
    env.ledger().set_timestamp(due_date);
    let invoice = client.get_invoice(&invoice_id);
    assert!(!invoice.is_overdue(env.ledger().timestamp()));
    assert!(
        client.try_mark_invoice_defaulted(&invoice_id, &Some(grace_period)).is_err()
    );

    // Test 3: After due_date but before grace_deadline
    env.ledger().set_timestamp(due_date + 500);
    let invoice = client.get_invoice(&invoice_id);
    assert!(invoice.is_overdue(env.ledger().timestamp()));
    assert!(
        client.try_mark_invoice_defaulted(&invoice_id, &Some(grace_period)).is_err()
    );

    // Test 4: At grace_deadline
    env.ledger().set_timestamp(grace_deadline);
    let invoice = client.get_invoice(&invoice_id);
    assert!(invoice.is_overdue(env.ledger().timestamp()));
    assert!(
        client.try_mark_invoice_defaulted(&invoice_id, &Some(grace_period)).is_err()
    );

    // Test 5: After grace_deadline - now default is allowed
    env.ledger().set_timestamp(grace_deadline + 1);
    let result = client.try_mark_invoice_defaulted(&invoice_id, &Some(grace_period));
    assert!(result.is_ok());

    let final_invoice = client.get_invoice(&invoice_id);
    assert_eq!(final_invoice.status, crate::invoice::InvoiceStatus::Defaulted);
}
