//! Comprehensive tests for MAX_QUERY_LIMIT enforcement across all paginated endpoints.
//! 
//! This module validates that all query endpoints properly enforce the configured
//! maximum query limit to prevent resource abuse and ensure predictable performance.

use super::*;
use crate::bid::{Bid, BidStatus};
use crate::invoice::{InvoiceCategory, InvoiceStatus};
use crate::investment::InvestmentStatus;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, BytesN, Env, String, Vec,
};

// Helper: basic setup returning env and client
fn setup() -> (Env, QuickLendXContractClient<'static>) {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    (env, client)
}

// Helper: create and optionally verify an invoice
fn create_invoice(
    env: &Env,
    client: &QuickLendXContractClient,
    business: &Address,
    amount: i128,
    category: InvoiceCategory,
    verify: bool,
) -> BytesN<32> {
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;
    let invoice_id = client.store_invoice(
        business,
        &amount,
        &currency,
        &due_date,
        &String::from_str(env, "Invoice"),
        &category,
        &Vec::new(env),
    );
    if verify {
        // set admin and verify
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let _ = client.set_admin(&admin);
        let _ = client.try_verify_invoice(&invoice_id);
    }
    invoice_id
}

// Helper: create a bid for an invoice
fn create_bid(
    env: &Env,
    client: &QuickLendXContractClient,
    invoice_id: &BytesN<32>,
    investor: &Address,
    amount: i128,
) -> BytesN<32> {
    env.mock_all_auths();
    client.place_bid(investor, invoice_id, &amount, &1000i128)
}

/// Test that limit=0 returns empty results for all paginated endpoints
#[test]
fn test_limit_zero_returns_empty_all_endpoints() {
    let (env, client) = setup();
    env.mock_all_auths();
    
    let business = Address::generate(&env);
    let investor = Address::generate(&env);
    let admin = Address::generate(&env);
    let _ = client.set_admin(&admin);
    
    // Create some test data
    let invoice_id = create_invoice(&env, &client, &business, 1000, InvoiceCategory::Services, true);
    
    // Test all paginated endpoints with limit=0
    let business_invoices = client.get_business_invoices_paged(
        &business,
        &Option::<InvoiceStatus>::None,
        &0u32,
        &0u32,
    );
    assert_eq!(business_invoices.len(), 0, "Business invoices with limit=0 should return empty");
    
    let available_invoices = client.get_available_invoices_paged(
        &Option::<i128>::None,
        &Option::<i128>::None,
        &Option::<InvoiceCategory>::None,
        &0u32,
        &0u32,
    );
    assert_eq!(available_invoices.len(), 0, "Available invoices with limit=0 should return empty");
    
    let investor_investments = client.get_investor_investments_paged(
        &investor,
        &Option::<InvestmentStatus>::None,
        &0u32,
        &0u32,
    );
    assert_eq!(investor_investments.len(), 0, "Investor investments with limit=0 should return empty");
    
    let bid_history = client.get_bid_history_paged(
        &invoice_id,
        &Option::<BidStatus>::None,
        &0u32,
        &0u32,
    );
    assert_eq!(bid_history.len(), 0, "Bid history with limit=0 should return empty");
    
    let investor_bids = client.get_investor_bids_paged(
        &investor,
        &Option::<BidStatus>::None,
        &0u32,
        &0u32,
    );
    assert_eq!(investor_bids.len(), 0, "Investor bids with limit=0 should return empty");
    
    let currencies = client.get_whitelisted_currencies_paged(&0u32, &0u32);
    assert_eq!(currencies.len(), 0, "Whitelisted currencies with limit=0 should return empty");
}

/// Test that limit > MAX_QUERY_LIMIT is capped to MAX_QUERY_LIMIT
#[test]
fn test_limit_exceeds_max_query_limit_is_capped() {
    let (env, client) = setup();
    env.mock_all_auths();
    
    let business = Address::generate(&env);
    let admin = Address::generate(&env);
    let _ = client.set_admin(&admin);
    
    // Create more invoices than MAX_QUERY_LIMIT (100) but not too many to avoid resource limits
    for i in 0..110u32 {
        let _ = create_invoice(
            &env,
            &client,
            &business,
            1000 + i as i128,
            InvoiceCategory::Services,
            true,
        );
    }
    
    // Test business invoices with limit > MAX_QUERY_LIMIT
    let business_invoices = client.get_business_invoices_paged(
        &business,
        &Option::<InvoiceStatus>::None,
        &0u32,
        &500u32, // Much larger than MAX_QUERY_LIMIT
    );
    assert_eq!(
        business_invoices.len(),
        crate::MAX_QUERY_LIMIT,
        "Business invoices should be capped at MAX_QUERY_LIMIT"
    );
    
    // Test available invoices with limit > MAX_QUERY_LIMIT
    let available_invoices = client.get_available_invoices_paged(
        &Option::<i128>::None,
        &Option::<i128>::None,
        &Option::<InvoiceCategory>::None,
        &0u32,
        &1000u32, // Much larger than MAX_QUERY_LIMIT
    );
    assert_eq!(
        available_invoices.len(),
        crate::MAX_QUERY_LIMIT,
        "Available invoices should be capped at MAX_QUERY_LIMIT"
    );
}

/// Test large offset scenarios (offset >= total items)
#[test]
fn test_large_offset_returns_empty() {
    let (env, client) = setup();
    env.mock_all_auths();
    
    let business = Address::generate(&env);
    let admin = Address::generate(&env);
    let _ = client.set_admin(&admin);
    
    // Create 10 invoices
    for i in 0..10u32 {
        let _ = create_invoice(
            &env,
            &client,
            &business,
            1000 + i as i128,
            InvoiceCategory::Services,
            true,
        );
    }
    
    // Test offset beyond available items
    let business_invoices = client.get_business_invoices_paged(
        &business,
        &Option::<InvoiceStatus>::None,
        &100u32, // Offset much larger than available items
        &10u32,
    );
    assert_eq!(business_invoices.len(), 0, "Large offset should return empty results");
    
    let available_invoices = client.get_available_invoices_paged(
        &Option::<i128>::None,
        &Option::<i128>::None,
        &Option::<InvoiceCategory>::None,
        &50u32, // Offset larger than available items
        &10u32,
    );
    assert_eq!(available_invoices.len(), 0, "Large offset should return empty results");
}

/// Test overflow protection in offset + limit calculations
#[test]
fn test_overflow_protection() {
    let (env, client) = setup();
    env.mock_all_auths();
    
    let business = Address::generate(&env);
    let admin = Address::generate(&env);
    let _ = client.set_admin(&admin);
    
    // Create a few invoices
    for i in 0..5u32 {
        let _ = create_invoice(
            &env,
            &client,
            &business,
            1000 + i as i128,
            InvoiceCategory::Services,
            false,
        );
    }
    
    // Test with offset that could cause overflow
    let large_offset = u32::MAX - 50;
    let business_invoices = client.get_business_invoices_paged(
        &business,
        &Option::<InvoiceStatus>::None,
        &large_offset,
        &100u32,
    );
    assert_eq!(business_invoices.len(), 0, "Overflow protection should return empty results");
    
    let available_invoices = client.get_available_invoices_paged(
        &Option::<i128>::None,
        &Option::<i128>::None,
        &Option::<InvoiceCategory>::None,
        &large_offset,
        &100u32,
    );
    assert_eq!(available_invoices.len(), 0, "Overflow protection should return empty results");
}

/// Test MAX_QUERY_LIMIT enforcement for bid-related endpoints
#[test]
fn test_bid_endpoints_enforce_max_query_limit() {
    let (env, client) = setup();
    env.mock_all_auths();
    
    let business = Address::generate(&env);
    let investor = Address::generate(&env);
    let admin = Address::generate(&env);
    let _ = client.set_admin(&admin);
    
    // Create an invoice
    let invoice_id = create_invoice(&env, &client, &business, 10000, InvoiceCategory::Services, true);
    
    // Test bid history with large limit (even with no bids)
    let bid_history = client.get_bid_history_paged(
        &invoice_id,
        &Option::<BidStatus>::None,
        &0u32,
        &500u32,
    );
    assert!(
        bid_history.len() <= crate::MAX_QUERY_LIMIT,
        "Bid history should not exceed MAX_QUERY_LIMIT"
    );
    
    // Test investor bids with large limit
    let investor_bids = client.get_investor_bids_paged(
        &investor,
        &Option::<BidStatus>::None,
        &0u32,
        &500u32,
    );
    assert!(
        investor_bids.len() <= crate::MAX_QUERY_LIMIT,
        "Investor bids should not exceed MAX_QUERY_LIMIT"
    );
}

/// Test currency whitelist pagination with MAX_QUERY_LIMIT enforcement
#[test]
fn test_currency_pagination_enforces_max_query_limit() {
    let (env, client) = setup();
    env.mock_all_auths();
    
    let admin = Address::generate(&env);
    let _ = client.set_admin(&admin);
    
    // Add more currencies than MAX_QUERY_LIMIT
    let mut currencies = Vec::new(&env);
    for _i in 0..150u32 {
        currencies.push_back(Address::generate(&env));
    }
    let _ = client.set_currencies(&admin, &currencies);
    
    // Test with large limit
    let paged_currencies = client.get_whitelisted_currencies_paged(&0u32, &500u32);
    assert_eq!(
        paged_currencies.len(),
        crate::MAX_QUERY_LIMIT,
        "Currency pagination should be capped at MAX_QUERY_LIMIT"
    );
}

/// Test pagination consistency across multiple pages
#[test]
fn test_pagination_consistency() {
    let (env, client) = setup();
    env.mock_all_auths();
    
    let business = Address::generate(&env);
    let admin = Address::generate(&env);
    let _ = client.set_admin(&admin);
    
    // Create exactly 250 invoices (more than 2 * MAX_QUERY_LIMIT)
    for i in 0..250u32 {
        let _ = create_invoice(
            &env,
            &client,
            &business,
            1000 + i as i128,
            InvoiceCategory::Services,
            true,
        );
    }
    
    // Get first page
    let page1 = client.get_business_invoices_paged(
        &business,
        &Option::<InvoiceStatus>::None,
        &0u32,
        &crate::MAX_QUERY_LIMIT,
    );
    assert_eq!(page1.len(), crate::MAX_QUERY_LIMIT, "First page should be full");
    
    // Get second page
    let page2 = client.get_business_invoices_paged(
        &business,
        &Option::<InvoiceStatus>::None,
        &crate::MAX_QUERY_LIMIT,
        &crate::MAX_QUERY_LIMIT,
    );
    assert_eq!(page2.len(), crate::MAX_QUERY_LIMIT, "Second page should be full");
    
    // Get third page (partial)
    let page3 = client.get_business_invoices_paged(
        &business,
        &Option::<InvoiceStatus>::None,
        &(2 * crate::MAX_QUERY_LIMIT),
        &crate::MAX_QUERY_LIMIT,
    );
    assert_eq!(page3.len(), 50, "Third page should have remaining items");
    
    // Verify no overlap between pages
    for item1 in page1.iter() {
        assert!(!page2.contains(&item1), "Pages should not overlap");
        assert!(!page3.contains(&item1), "Pages should not overlap");
    }
    
    for item2 in page2.iter() {
        assert!(!page3.contains(&item2), "Pages should not overlap");
    }
}

/// Test edge case: exactly MAX_QUERY_LIMIT items
#[test]
fn test_exactly_max_query_limit_items() {
    let (env, client) = setup();
    env.mock_all_auths();
    
    let business = Address::generate(&env);
    let admin = Address::generate(&env);
    let _ = client.set_admin(&admin);
    
    // Create exactly MAX_QUERY_LIMIT invoices
    for i in 0..crate::MAX_QUERY_LIMIT {
        let _ = create_invoice(
            &env,
            &client,
            &business,
            1000 + i as i128,
            InvoiceCategory::Services,
            true,
        );
    }
    
    // Request all items
    let all_invoices = client.get_business_invoices_paged(
        &business,
        &Option::<InvoiceStatus>::None,
        &0u32,
        &crate::MAX_QUERY_LIMIT,
    );
    assert_eq!(
        all_invoices.len(),
        crate::MAX_QUERY_LIMIT,
        "Should return exactly MAX_QUERY_LIMIT items"
    );
    
    // Request with larger limit
    let capped_invoices = client.get_business_invoices_paged(
        &business,
        &Option::<InvoiceStatus>::None,
        &0u32,
        &(crate::MAX_QUERY_LIMIT * 2),
    );
    assert_eq!(
        capped_invoices.len(),
        crate::MAX_QUERY_LIMIT,
        "Should still return exactly MAX_QUERY_LIMIT items"
    );
}

/// Test validation with extreme offset values
#[test]
fn test_extreme_offset_validation() {
    let (env, client) = setup();
    
    let business = Address::generate(&env);
    
    // Test with u32::MAX offset
    let result = client.get_business_invoices_paged(
        &business,
        &Option::<InvoiceStatus>::None,
        &u32::MAX,
        &10u32,
    );
    assert_eq!(result.len(), 0, "u32::MAX offset should return empty results");
    
    // Test with offset that would overflow when added to MAX_QUERY_LIMIT
    let dangerous_offset = u32::MAX - 50;
    let result2 = client.get_business_invoices_paged(
        &business,
        &Option::<InvoiceStatus>::None,
        &dangerous_offset,
        &100u32,
    );
    assert_eq!(result2.len(), 0, "Dangerous offset should return empty results");
}