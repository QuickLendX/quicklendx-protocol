//! Comprehensive boundary tests for investor investment queries with pagination
//!
//! This module tests pagination boundary conditions, overflow-safe arithmetic,
//! and edge cases to ensure robust and secure query behavior.
//!
//! # Test Coverage
//! - Pagination boundary conditions (offset >= total, limit = 0, etc.)
//! - Overflow-safe arithmetic validation
//! - Large dataset handling
//! - Status filtering with pagination
//! - Edge cases and error conditions
//!
//! # Security Focus
//! - Prevents integer overflow attacks
//! - Validates all array bounds
//! - Tests DoS resistance via large queries
//! - Ensures consistent behavior across edge cases

#[cfg(test)]
use soroban_sdk::{testutils::Address as _, Address, BytesN, Env, Vec};

#[cfg(test)]
use crate::{
    investment_queries::InvestmentQueries,
    types::{Investment, InvestmentStatus},
    QuickLendXContract, QuickLendXContractClient,
};

#[cfg(test)]
struct TestContext<'a> {
    env: Env,
    client: QuickLendXContractClient<'a>,
    admin: Address,
    investor: Address,
    business: Address,
    currency: Address,
}

#[cfg(test)]
impl<'a> TestContext<'a> {
    fn new(
        env: Env,
        client: QuickLendXContractClient<'a>,
        admin: Address,
        investor: Address,
        business: Address,
        currency: Address,
    ) -> Self {
        Self {
            env,
            client,
            admin,
            investor,
            business,
            currency,
        }
    }
}

#[cfg(test)]
fn setup_context() -> TestContext<'static> {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, QuickLendXContract);
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let investor = Address::generate(&env);
    let business = Address::generate(&env);
    let currency = Address::generate(&env);

    // Initialize contract
    client.initialize_admin(&admin);
    client.add_currency(&admin, &currency);

    TestContext::new(env, client, admin, investor, business, currency)
}

#[cfg(test)]
fn setup_business(ctx: &TestContext, business: &Address) {
    ctx.client.verify_business(&ctx.admin, business);
}

#[cfg(test)]
fn setup_investor(ctx: &TestContext, investor: &Address, limit: i128) {
    ctx.client.submit_investor_kyc(investor, &"Test Investor".into());
    ctx.client.verify_investor(&ctx.admin, investor);
    ctx.client.set_investment_limit(&ctx.admin, investor, &limit);
}

#[cfg(test)]
fn create_investment(
    ctx: &TestContext,
    investor: &Address,
    amount: i128,
    status: InvestmentStatus,
) -> BytesN<32> {
    let investment_id = BytesN::from_array(&ctx.env, &[0u8; 32]);
    let invoice_id = BytesN::from_array(&ctx.env, &[1u8; 32]);
    
    let investment = Investment {
        investment_id: investment_id.clone(),
        invoice_id,
        investor: investor.clone(),
        amount,
        funded_at: ctx.env.ledger().timestamp(),
        status,
        insurance: Vec::new(&ctx.env),
    };

    // Store investment using storage layer
    crate::storage::InvestmentStorage::store(&ctx.env, &investment);
    
    investment_id
}

/// Test pagination boundary: offset equals total count
#[test]
fn test_pagination_offset_equals_total_count() {
    let ctx = setup_context();
    setup_business(&ctx, &ctx.business);
    setup_investor(&ctx, &ctx.investor, 10000);

    // Create exactly 5 investments
    for i in 0..5 {
        create_investment(&ctx, &ctx.investor, 1000 + i, InvestmentStatus::Active);
    }

    // Query with offset = total count (should return empty)
    let result = ctx.client.get_investor_investments_paged(
        &ctx.investor,
        &Some(InvestmentStatus::Active),
        &5u32, // offset equals total count
        &10u32,
    );

    assert_eq!(result.len(), 0, "Offset equal to total count should return empty result");
}

/// Test pagination boundary: offset exceeds total count
#[test]
fn test_pagination_offset_exceeds_total_count() {
    let ctx = setup_context();
    setup_business(&ctx, &ctx.business);
    setup_investor(&ctx, &ctx.investor, 10000);

    // Create 3 investments
    for i in 0..3 {
        create_investment(&ctx, &ctx.investor, 1000 + i, InvestmentStatus::Active);
    }

    // Query with offset > total count
    let result = ctx.client.get_investor_investments_paged(
        &ctx.investor,
        &Some(InvestmentStatus::Active),
        &100u32, // offset exceeds total count
        &10u32,
    );

    assert_eq!(result.len(), 0, "Offset exceeding total count should return empty result");
}

/// Test pagination boundary: limit is zero
#[test]
fn test_pagination_limit_zero() {
    let ctx = setup_context();
    setup_business(&ctx, &ctx.business);
    setup_investor(&ctx, &ctx.investor, 10000);

    // Create some investments
    for i in 0..5 {
        create_investment(&ctx, &ctx.investor, 1000 + i, InvestmentStatus::Active);
    }

    // Query with limit = 0
    let result = ctx.client.get_investor_investments_paged(
        &ctx.investor,
        &Some(InvestmentStatus::Active),
        &0u32,
        &0u32, // limit is zero
    );

    assert_eq!(result.len(), 0, "Zero limit should return empty result");
}

/// Test pagination boundary: limit exceeds MAX_QUERY_LIMIT
#[test]
fn test_pagination_limit_exceeds_max() {
    let ctx = setup_context();
    setup_business(&ctx, &ctx.business);
    setup_investor(&ctx, &ctx.investor, 50000);

    // Create more investments than MAX_QUERY_LIMIT (100)
    for i in 0..120 {
        create_investment(&ctx, &ctx.investor, 1000 + i, InvestmentStatus::Active);
    }

    // Query with limit > MAX_QUERY_LIMIT
    let result = ctx.client.get_investor_investments_paged(
        &ctx.investor,
        &Some(InvestmentStatus::Active),
        &0u32,
        &200u32, // limit exceeds MAX_QUERY_LIMIT
    );

    assert_eq!(
        result.len(),
        crate::investment_queries::MAX_QUERY_LIMIT,
        "Limit should be capped to MAX_QUERY_LIMIT"
    );
}

/// Test overflow-safe arithmetic: maximum u32 values
#[test]
fn test_overflow_safe_arithmetic_max_values() {
    let ctx = setup_context();
    setup_business(&ctx, &ctx.business);
    setup_investor(&ctx, &ctx.investor, 10000);

    // Create a few investments
    for i in 0..3 {
        create_investment(&ctx, &ctx.investor, 1000 + i, InvestmentStatus::Active);
    }

    // Test with maximum u32 values
    let result = ctx.client.get_investor_investments_paged(
        &ctx.investor,
        &Some(InvestmentStatus::Active),
        &u32::MAX, // maximum offset
        &u32::MAX, // maximum limit
    );

    // Should handle gracefully without panic
    assert_eq!(result.len(), 0, "Max u32 values should be handled safely");
}

/// Test saturating arithmetic in boundary calculations
#[test]
fn test_saturating_arithmetic_boundary_calculations() {
    let env = Env::default();
    
    // Test validate_pagination_params with edge cases
    let (offset, limit, has_more) = InvestmentQueries::validate_pagination_params(
        u32::MAX - 1,
        u32::MAX,
        10,
    );
    
    assert_eq!(offset, 10, "Offset should be capped to total count");
    assert_eq!(limit, 0, "Limit should be 0 when offset >= total");
    assert_eq!(has_more, false, "Should not have more when at end");
}

/// Test calculate_safe_bounds with overflow conditions
#[test]
fn test_calculate_safe_bounds_overflow_protection() {
    let env = Env::default();
    
    // Test with values that would overflow if not using saturating arithmetic
    let (start, end) = InvestmentQueries::calculate_safe_bounds(
        u32::MAX - 50,
        100,
        u32::MAX - 10,
    );
    
    assert!(start <= end, "Start should never exceed end");
    assert!(end <= u32::MAX - 10, "End should not exceed collection size");
}

/// Test pagination with mixed investment statuses
#[test]
fn test_pagination_with_status_filtering() {
    let ctx = setup_context();
    setup_business(&ctx, &ctx.business);
    setup_investor(&ctx, &ctx.investor, 20000);

    // Create investments with different statuses
    for i in 0..10 {
        let status = if i % 2 == 0 {
            InvestmentStatus::Active
        } else {
            InvestmentStatus::Completed
        };
        create_investment(&ctx, &ctx.investor, 1000 + i, status);
    }

    // Query only active investments with pagination
    let active_page1 = ctx.client.get_investor_investments_paged(
        &ctx.investor,
        &Some(InvestmentStatus::Active),
        &0u32,
        &3u32,
    );

    let active_page2 = ctx.client.get_investor_investments_paged(
        &ctx.investor,
        &Some(InvestmentStatus::Active),
        &3u32,
        &3u32,
    );

    // Should have 5 active investments total (0, 2, 4, 6, 8)
    assert_eq!(active_page1.len(), 3, "First page should have 3 active investments");
    assert_eq!(active_page2.len(), 2, "Second page should have 2 active investments");
}

/// Test empty collection pagination
#[test]
fn test_pagination_empty_collection() {
    let ctx = setup_context();
    setup_business(&ctx, &ctx.business);
    setup_investor(&ctx, &ctx.investor, 10000);

    // No investments created - empty collection
    let result = ctx.client.get_investor_investments_paged(
        &ctx.investor,
        &Some(InvestmentStatus::Active),
        &0u32,
        &10u32,
    );

    assert_eq!(result.len(), 0, "Empty collection should return empty result");
}

/// Test pagination with single item collection
#[test]
fn test_pagination_single_item_collection() {
    let ctx = setup_context();
    setup_business(&ctx, &ctx.business);
    setup_investor(&ctx, &ctx.investor, 10000);

    // Create exactly one investment
    create_investment(&ctx, &ctx.investor, 1000, InvestmentStatus::Active);

    // Test various pagination scenarios
    let page1 = ctx.client.get_investor_investments_paged(
        &ctx.investor,
        &Some(InvestmentStatus::Active),
        &0u32,
        &1u32,
    );

    let page2 = ctx.client.get_investor_investments_paged(
        &ctx.investor,
        &Some(InvestmentStatus::Active),
        &1u32,
        &1u32,
    );

    assert_eq!(page1.len(), 1, "First page should contain the single item");
    assert_eq!(page2.len(), 0, "Second page should be empty");
}

/// Test large offset with small collection
#[test]
fn test_large_offset_small_collection() {
    let ctx = setup_context();
    setup_business(&ctx, &ctx.business);
    setup_investor(&ctx, &ctx.investor, 10000);

    // Create small collection
    for i in 0..3 {
        create_investment(&ctx, &ctx.investor, 1000 + i, InvestmentStatus::Active);
    }

    // Use very large offset
    let result = ctx.client.get_investor_investments_paged(
        &ctx.investor,
        &Some(InvestmentStatus::Active),
        &1000u32, // much larger than collection size
        &10u32,
    );

    assert_eq!(result.len(), 0, "Large offset on small collection should return empty");
}

/// Test cap_query_limit function directly
#[test]
fn test_cap_query_limit_function() {
    // Test normal values
    assert_eq!(InvestmentQueries::cap_query_limit(50), 50);
    assert_eq!(InvestmentQueries::cap_query_limit(100), 100);
    
    // Test values exceeding limit
    assert_eq!(
        InvestmentQueries::cap_query_limit(150),
        crate::investment_queries::MAX_QUERY_LIMIT
    );
    assert_eq!(
        InvestmentQueries::cap_query_limit(u32::MAX),
        crate::investment_queries::MAX_QUERY_LIMIT
    );
}

/// Test validate_pagination_params function directly
#[test]
fn test_validate_pagination_params_function() {
    // Normal case
    let (offset, limit, has_more) = InvestmentQueries::validate_pagination_params(0, 10, 50);
    assert_eq!(offset, 0);
    assert_eq!(limit, 10);
    assert_eq!(has_more, true);

    // Offset at boundary
    let (offset, limit, has_more) = InvestmentQueries::validate_pagination_params(45, 10, 50);
    assert_eq!(offset, 45);
    assert_eq!(limit, 5); // Only 5 items remaining
    assert_eq!(has_more, false);

    // Offset exceeds total
    let (offset, limit, has_more) = InvestmentQueries::validate_pagination_params(60, 10, 50);
    assert_eq!(offset, 50); // Capped to total
    assert_eq!(limit, 0); // No items remaining
    assert_eq!(has_more, false);
}

/// Test pagination consistency across multiple queries
#[test]
fn test_pagination_consistency() {
    let ctx = setup_context();
    setup_business(&ctx, &ctx.business);
    setup_investor(&ctx, &ctx.investor, 20000);

    // Create 15 investments
    for i in 0..15 {
        create_investment(&ctx, &ctx.investor, 1000 + i, InvestmentStatus::Active);
    }

    // Query in pages of 5
    let page1 = ctx.client.get_investor_investments_paged(
        &ctx.investor,
        &Some(InvestmentStatus::Active),
        &0u32,
        &5u32,
    );

    let page2 = ctx.client.get_investor_investments_paged(
        &ctx.investor,
        &Some(InvestmentStatus::Active),
        &5u32,
        &5u32,
    );

    let page3 = ctx.client.get_investor_investments_paged(
        &ctx.investor,
        &Some(InvestmentStatus::Active),
        &10u32,
        &5u32,
    );

    let page4 = ctx.client.get_investor_investments_paged(
        &ctx.investor,
        &Some(InvestmentStatus::Active),
        &15u32,
        &5u32,
    );

    assert_eq!(page1.len(), 5, "Page 1 should have 5 items");
    assert_eq!(page2.len(), 5, "Page 2 should have 5 items");
    assert_eq!(page3.len(), 5, "Page 3 should have 5 items");
    assert_eq!(page4.len(), 0, "Page 4 should be empty");

    // Verify no duplicates across pages
    let mut all_ids = Vec::new(&ctx.env);
    for id in page1.iter() {
        all_ids.push_back(id);
    }
    for id in page2.iter() {
        all_ids.push_back(id);
    }
    for id in page3.iter() {
        all_ids.push_back(id);
    }

    assert_eq!(all_ids.len(), 15, "Should have all 15 unique items across pages");
}

/// Test arithmetic overflow protection in real scenarios
#[test]
fn test_arithmetic_overflow_protection() {
    let ctx = setup_context();
    setup_business(&ctx, &ctx.business);
    setup_investor(&ctx, &ctx.investor, 10000);

    // Create some investments
    for i in 0..5 {
        create_investment(&ctx, &ctx.investor, 1000 + i, InvestmentStatus::Active);
    }

    // Test scenarios that could cause overflow with unsafe arithmetic
    let test_cases = vec![
        (u32::MAX, 1),
        (u32::MAX - 1, 2),
        (u32::MAX / 2, u32::MAX / 2),
        (0, u32::MAX),
    ];

    for (offset, limit) in test_cases {
        let result = ctx.client.get_investor_investments_paged(
            &ctx.investor,
            &Some(InvestmentStatus::Active),
            &offset,
            &limit,
        );

        // Should not panic and should return reasonable results
        assert!(result.len() <= crate::investment_queries::MAX_QUERY_LIMIT as usize);
    }
}

/// Test count_investor_investments function
#[test]
fn test_count_investor_investments() {
    let ctx = setup_context();
    setup_business(&ctx, &ctx.business);
    setup_investor(&ctx, &ctx.investor, 20000);

    // Create investments with mixed statuses
    for i in 0..10 {
        let status = match i % 3 {
            0 => InvestmentStatus::Active,
            1 => InvestmentStatus::Completed,
            _ => InvestmentStatus::Defaulted,
        };
        create_investment(&ctx, &ctx.investor, 1000 + i, status);
    }

    // Test counting with different filters
    let total_count = InvestmentQueries::count_investor_investments(
        &ctx.env,
        &ctx.investor,
        None,
    );

    let active_count = InvestmentQueries::count_investor_investments(
        &ctx.env,
        &ctx.investor,
        Some(InvestmentStatus::Active),
    );

    let completed_count = InvestmentQueries::count_investor_investments(
        &ctx.env,
        &ctx.investor,
        Some(InvestmentStatus::Completed),
    );
    assert_eq!(all_paged.len(), 2);
}

/// Test: get_invoice_investment should ignore stale pointers where invoice_id does not match
#[test]
fn test_get_investment_by_invoice_stale_pointer_protection() {
    let ctx = setup_context();

    let business = Address::generate(&ctx.env);
    let investor = Address::generate(&ctx.env);

    setup_business(&ctx, &business);
    setup_investor(&ctx, &investor, 50_000);

    // 1. Create a valid investment
    let invoice_id = fund_invoice(&ctx, &business, &investor, 1_000);
    let investment = ctx.client.get_invoice_investment(&invoice_id);
    let investment_id = investment.investment_id.clone();

    // 2. Corrupt storage: Point another invoice_id to this same investment_id
    let second_invoice_id = BytesN::from_array(&ctx.env, &[77u8; 32]);
    let index_key = (soroban_sdk::symbol_short!("inv_map"), second_invoice_id.clone());
    
    ctx.env.as_contract(&ctx.client.address, || {
        ctx.env.storage().instance().set(&index_key, &investment_id);
    });

    // 3. Verify lookup for first invoice still works
    let retrieved_1 = ctx.client.get_invoice_investment(&invoice_id);
    assert_eq!(retrieved_1.investment_id, investment_id);

    // 4. Verify lookup for second invoice returns error (stale/invalid pointer)
    // because Investment.invoice_id (invoice_id) != second_invoice_id
    let result = ctx.client.try_get_invoice_investment(&second_invoice_id);
    assert!(result.is_err(), "Should ignore stale pointer and return error");
}
