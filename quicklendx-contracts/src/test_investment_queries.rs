//! Tests for investment query correctness, empty results, and pagination
//!
//! Test Coverage:
//! 1. by_investor returns only that investor's investments
//! 2. by_status filters correctly
//! 3. by_invoice returns at most one
//! 4. limit/offset pagination respected
//! 5. Empty results handling
//!
//! Security Notes:
//! - All queries return empty results (not errors) for non-existent data
//! - Pagination bounds are safely handled

use super::*;
use crate::investment::InvestmentStatus;
use crate::invoice::InvoiceCategory;
use soroban_sdk::{testutils::Address as _, token, Address, BytesN, Env, String, Vec};

// ============================================================================
// Efficient Test Context - Single setup, reusable components
// ============================================================================

struct TestContext<'a> {
    env: Env,
    client: QuickLendXContractClient<'a>,
    admin: Address,
    currency: Address,
    sac_client: token::StellarAssetClient<'a>,
    token_client: token::Client<'a>,
}

impl<'a> TestContext<'a> {
    fn new(env: Env, client: QuickLendXContractClient<'a>, admin: Address, currency: Address) -> Self {
        let sac_client = token::StellarAssetClient::new(&env, &currency);
        let token_client = token::Client::new(&env, &currency);
        Self { env, client, admin, currency, sac_client, token_client }
    }
}

/// Setup shared test context with single token registration
fn setup_context() -> TestContext<'static> {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    client.set_admin(&admin);

    // Single token for ALL tests - registered once
    let token_admin = Address::generate(&env);
    let currency = env.register_stellar_asset_contract_v2(token_admin).address();

    TestContext::new(env, client, admin, currency)
}

/// Setup verified business - called once per business
fn setup_business(ctx: &TestContext, business: &Address) {
    ctx.client.submit_kyc_application(business, &String::from_str(&ctx.env, "Business KYC"));
    ctx.client.verify_business(&ctx.admin, business);
}

/// Setup verified investor with tokens - called once per investor
fn setup_investor(ctx: &TestContext, investor: &Address, limit: i128) {
    // Mint large amount once
    ctx.sac_client.mint(investor, &(limit * 10));

    // Approve contract once with high limit
    let expiration = ctx.env.ledger().sequence() + 100_000;
    ctx.token_client.approve(investor, &ctx.client.address, &(limit * 10), &expiration);

    // KYC once
    ctx.client.submit_investor_kyc(investor, &String::from_str(&ctx.env, "Investor KYC"));
    ctx.client.verify_investor(investor, &limit);
}

/// Lightweight invoice funding - reuses existing token and verified parties
fn fund_invoice(ctx: &TestContext, business: &Address, investor: &Address, amount: i128) -> BytesN<32> {
    let due_date = ctx.env.ledger().timestamp() + 86_400;

    let invoice_id = ctx.client.store_invoice(
        business,
        &amount,
        &ctx.currency,
        &due_date,
        &String::from_str(&ctx.env, "Invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&ctx.env),
    );

    ctx.client.verify_invoice(&invoice_id);

    let bid_id = ctx.client.place_bid(investor, &invoice_id, &amount, &(amount + 100));
    ctx.client.accept_bid(&invoice_id, &bid_id);

    invoice_id
}

// ============================================================================
// Test Cases
// ============================================================================

/// Test: by_investor returns ONLY that investor's investments
#[test]
fn test_get_investments_by_investor_correctness() {
    let ctx = setup_context();

    let business = Address::generate(&ctx.env);
    let investor_a = Address::generate(&ctx.env);
    let investor_b = Address::generate(&ctx.env);

    // Setup once per party
    setup_business(&ctx, &business);
    setup_investor(&ctx, &investor_a, 50_000);
    setup_investor(&ctx, &investor_b, 50_000);

    // Create investments
    let invoice_1 = fund_invoice(&ctx, &business, &investor_a, 1_000);
    let invoice_2 = fund_invoice(&ctx, &business, &investor_b, 2_000);

    // Query investor_a
    let investments_a = ctx.client.get_investments_by_investor(&investor_a);
    assert_eq!(investments_a.len(), 1, "investor_a should have exactly 1 investment");

    let investment_a = ctx.client.get_invoice_investment(&invoice_1);
    assert!(investments_a.contains(&investment_a.investment_id));

    // Query investor_b
    let investments_b = ctx.client.get_investments_by_investor(&investor_b);
    assert_eq!(investments_b.len(), 1, "investor_b should have exactly 1 investment");

    // Verify isolation
    let investment_b = ctx.client.get_invoice_investment(&invoice_2);
    assert!(!investments_a.contains(&investment_b.investment_id), "investor_a should NOT have investor_b's investment");
}

/// Test: by_investor returns empty Vec for investor with no investments
#[test]
fn test_get_investments_by_investor_empty() {
    let ctx = setup_context();

    let new_investor = Address::generate(&ctx.env);

    let investments = ctx.client.get_investments_by_investor(&new_investor);
    assert_eq!(investments.len(), 0, "New investor should have 0 investments");
}

/// Test: by_status filter returns only matching investments
#[test]
fn test_get_investor_investments_paged_status_filter() {
    let ctx = setup_context();

    let business = Address::generate(&ctx.env);
    let investor_a = Address::generate(&ctx.env);
    let investor_b = Address::generate(&ctx.env);

    // Setup once
    setup_business(&ctx, &business);
    setup_investor(&ctx, &investor_a, 100_000);
    setup_investor(&ctx, &investor_b, 100_000);

    // Create investments
    let invoice_1 = fund_invoice(&ctx, &business, &investor_a, 1_000);
    let invoice_2 = fund_invoice(&ctx, &business, &investor_a, 2_000);
    let _invoice_3 = fund_invoice(&ctx, &business, &investor_b, 3_000);

    // Query Active
    let active = ctx.client.get_investor_investments_paged(
        &investor_a,
        &Some(InvestmentStatus::Active),
        &0u32,
        &10u32,
    );
    assert_eq!(active.len(), 2, "investor_a should have 2 Active investments");

    // Verify correct investments
    let inv_1 = ctx.client.get_invoice_investment(&invoice_1);
    let inv_2 = ctx.client.get_invoice_investment(&invoice_2);
    assert!(active.contains(&inv_1.investment_id));
    assert!(active.contains(&inv_2.investment_id));

    // Query Completed (none)
    let completed = ctx.client.get_investor_investments_paged(
        &investor_a,
        &Some(InvestmentStatus::Completed),
        &0u32,
        &10u32,
    );
    assert_eq!(completed.len(), 0);

    // Query no filter
    let all = ctx.client.get_investor_investments_paged(
        &investor_a,
        &Option::<InvestmentStatus>::None,
        &0u32,
        &10u32,
    );
    assert_eq!(all.len(), 2);

    // Verify investor isolation
    let investor_b_all = ctx.client.get_investor_investments_paged(
        &investor_b,
        &Option::<InvestmentStatus>::None,
        &0u32,
        &10u32,
    );
    assert_eq!(investor_b_all.len(), 1);
}

/// Test: by_invoice returns exactly one investment
#[test]
fn test_get_investment_by_invoice_at_most_one() {
    let ctx = setup_context();

    let business = Address::generate(&ctx.env);
    let investor = Address::generate(&ctx.env);

    setup_business(&ctx, &business);
    setup_investor(&ctx, &investor, 50_000);

    let invoice_id = fund_invoice(&ctx, &business, &investor, 1_000);

    let investment = ctx.client.get_invoice_investment(&invoice_id);
    assert_eq!(investment.invoice_id, invoice_id);
    assert_eq!(investment.investor, investor);
}

/// Test: by_invoice returns error for invoice without investment
#[test]
fn test_get_investment_by_invoice_not_found() {
    let ctx = setup_context();

    let business = Address::generate(&ctx.env);
    setup_business(&ctx, &business);

    // Create unfunded invoice
    let due_date = ctx.env.ledger().timestamp() + 86_400;
    let invoice_id = ctx.client.store_invoice(
        &business,
        &1_000,
        &ctx.currency,
        &due_date,
        &String::from_str(&ctx.env, "Unfunded"),
        &InvoiceCategory::Services,
        &Vec::new(&ctx.env),
    );
    ctx.client.verify_invoice(&invoice_id);

    // Should return error
    let result = ctx.client.try_get_invoice_investment(&invoice_id);
    assert!(result.is_err());

    // Non-existent invoice
    let fake_id = BytesN::from_array(&ctx.env, &[99u8; 32]);
    let result = ctx.client.try_get_invoice_investment(&fake_id);
    assert!(result.is_err());
}

/// Test: pagination limit is respected
#[test]
fn test_get_investor_investments_paged_limit() {
    let ctx = setup_context();

    let business = Address::generate(&ctx.env);
    let investor = Address::generate(&ctx.env);

    setup_business(&ctx, &business);
    setup_investor(&ctx, &investor, 100_000);

    // Create 5 investments
    for i in 0..5 {
        fund_invoice(&ctx, &business, &investor, 1_000 + i * 100);
    }

    // Verify total
    let all = ctx.client.get_investments_by_investor(&investor);
    assert_eq!(all.len(), 5);

    // limit=2
    let limited = ctx.client.get_investor_investments_paged(
        &investor,
        &Option::<InvestmentStatus>::None,
        &0u32,
        &2u32,
    );
    assert_eq!(limited.len(), 2);

    // limit > total
    let over = ctx.client.get_investor_investments_paged(
        &investor,
        &Option::<InvestmentStatus>::None,
        &0u32,
        &10u32,
    );
    assert_eq!(over.len(), 5);

    // limit=0
    let zero = ctx.client.get_investor_investments_paged(
        &investor,
        &Option::<InvestmentStatus>::None,
        &0u32,
        &0u32,
    );
    assert_eq!(zero.len(), 0);
}

/// Test: pagination offset is respected
#[test]
fn test_get_investor_investments_paged_offset() {
    let ctx = setup_context();

    let business = Address::generate(&ctx.env);
    let investor = Address::generate(&ctx.env);

    setup_business(&ctx, &business);
    setup_investor(&ctx, &investor, 100_000);

    // Create 5 investments
    for i in 0..5 {
        fund_invoice(&ctx, &business, &investor, 1_000 + i * 100);
    }

    // offset=0
    let page_0 = ctx.client.get_investor_investments_paged(
        &investor,
        &Option::<InvestmentStatus>::None,
        &0u32,
        &10u32,
    );
    assert_eq!(page_0.len(), 5);

    // offset=2
    let page_2 = ctx.client.get_investor_investments_paged(
        &investor,
        &Option::<InvestmentStatus>::None,
        &2u32,
        &10u32,
    );
    assert_eq!(page_2.len(), 3);

    // Verify no overlap
    let first_two: Vec<BytesN<32>> = {
        let mut v = Vec::new(&ctx.env);
        v.push_back(page_0.get(0).unwrap());
        v.push_back(page_0.get(1).unwrap());
        v
    };
    for id in page_2.iter() {
        assert!(!first_two.contains(&id), "Offset results should not overlap");
    }

    // offset beyond
    let beyond = ctx.client.get_investor_investments_paged(
        &investor,
        &Option::<InvestmentStatus>::None,
        &10u32,
        &10u32,
    );
    assert_eq!(beyond.len(), 0);
}

/// Test: filter + pagination combined
#[test]
fn test_get_investor_investments_paged_filter_and_pagination() {
    let ctx = setup_context();

    let business = Address::generate(&ctx.env);
    let investor = Address::generate(&ctx.env);

    setup_business(&ctx, &business);
    setup_investor(&ctx, &investor, 100_000);

    // Create 5 investments
    for i in 0..5 {
        fund_invoice(&ctx, &business, &investor, 1_000 + i * 100);
    }

    // Active + offset=1 + limit=2
    let paged = ctx.client.get_investor_investments_paged(
        &investor,
        &Some(InvestmentStatus::Active),
        &1u32,
        &2u32,
    );
    assert_eq!(paged.len(), 2);

    // Active + limit=3
    let limited = ctx.client.get_investor_investments_paged(
        &investor,
        &Some(InvestmentStatus::Active),
        &0u32,
        &3u32,
    );
    assert_eq!(limited.len(), 3);

    // Active + offset beyond
    let beyond = ctx.client.get_investor_investments_paged(
        &investor,
        &Some(InvestmentStatus::Active),
        &10u32,
        &10u32,
    );
    assert_eq!(beyond.len(), 0);

    // No filter + pagination
    let all_paged = ctx.client.get_investor_investments_paged(
        &investor,
        &Option::<InvestmentStatus>::None,
        &2u32,
        &2u32,
    );
    assert_eq!(all_paged.len(), 2);
}
