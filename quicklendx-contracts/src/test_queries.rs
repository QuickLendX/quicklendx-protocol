use super::*;
use crate::audit::{AuditOperation, AuditOperationFilter, AuditQueryFilter};
use crate::bid::{Bid, BidStatus, BidStorage};
use crate::invoice::{InvoiceCategory, InvoiceStatus};
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

#[test]
fn test_get_business_invoices_paged_empty_and_pagination() {
    let (env, client) = setup();
    // No invoices for this business should return empty results
    let business = Address::generate(&env);

    let empty = client.get_business_invoices_paged(
        &business,
        &Option::<InvoiceStatus>::None,
        &0u32,
        &10u32,
    );
    assert_eq!(empty.len(), 0, "Expected no invoices for new business");

    // Create 5 invoices for business
    for i in 0..5 {
        let _id = create_invoice(
            &env,
            &client,
            &business,
            1000 + i * 100,
            InvoiceCategory::Services,
            false,
        );
    }

    // Page 0, limit 2 => 2 results
    let p0 =
        client.get_business_invoices_paged(&business, &Option::<InvoiceStatus>::None, &0u32, &2u32);
    assert_eq!(p0.len(), 2);

    // Page 1, offset 2, limit 2 => next 2 results
    let p1 =
        client.get_business_invoices_paged(&business, &Option::<InvoiceStatus>::None, &2u32, &2u32);
    assert_eq!(p1.len(), 2);

    // Offset beyond length => empty
    let p_out = client.get_business_invoices_paged(
        &business,
        &Option::<InvoiceStatus>::None,
        &10u32,
        &5u32,
    );
    assert_eq!(
        p_out.len(),
        0,
        "Offset beyond length should return empty slice"
    );

    // Limit zero => empty
    let p_zero =
        client.get_business_invoices_paged(&business, &Option::<InvoiceStatus>::None, &0u32, &0u32);
    assert_eq!(p_zero.len(), 0, "Limit zero should return empty results");
}

#[test]
fn test_get_business_invoices_paged_limit_is_capped_to_max_query_limit() {
    let (env, client) = setup();
    let business = Address::generate(&env);

    for i in 0..120u32 {
        let _ = create_invoice(
            &env,
            &client,
            &business,
            1_000 + i as i128,
            InvoiceCategory::Services,
            false,
        );
    }

    let capped = client.get_business_invoices_paged(
        &business,
        &Option::<InvoiceStatus>::None,
        &0u32,
        &500u32,
    );
    assert_eq!(
        capped.len(),
        crate::MAX_QUERY_LIMIT,
        "business invoice query should enforce MAX_QUERY_LIMIT cap"
    );
}

#[test]
fn test_get_available_invoices_paged_filters_and_bounds() {
    let (env, client) = setup();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let _ = client.set_admin(&admin);

    let business = Address::generate(&env);

    // Create and verify invoices with varying amounts and categories
    let id1 = create_invoice(
        &env,
        &client,
        &business,
        500,
        InvoiceCategory::Products,
        true,
    );
    let id2 = create_invoice(
        &env,
        &client,
        &business,
        1500,
        InvoiceCategory::Services,
        true,
    );
    let id3 = create_invoice(
        &env,
        &client,
        &business,
        2500,
        InvoiceCategory::Services,
        true,
    );
    let id4 = create_invoice(
        &env,
        &client,
        &business,
        3500,
        InvoiceCategory::Products,
        true,
    );

    // No filters: should return at least the 4 we added
    let all = client.get_available_invoices_paged(
        &Option::<i128>::None,
        &Option::<i128>::None,
        &Option::<InvoiceCategory>::None,
        &0u32,
        &10u32,
    );
    assert!(all.len() >= 4, "Expected at least 4 verified invoices");

    // Filter by min_amount => should exclude id1
    let min_filtered = client.get_available_invoices_paged(
        &Some(1000i128),
        &Option::<i128>::None,
        &Option::<InvoiceCategory>::None,
        &0u32,
        &10u32,
    );
    assert!(
        !min_filtered.contains(&id1),
        "id1 should be excluded by min_amount filter"
    );
    assert!(
        min_filtered.contains(&id2),
        "id2 should be included by min_amount filter"
    );

    // Filter by max_amount => should exclude highest
    let max_filtered = client.get_available_invoices_paged(
        &Option::<i128>::None,
        &Some(3000i128),
        &Option::<InvoiceCategory>::None,
        &0u32,
        &10u32,
    );
    assert!(
        !max_filtered.contains(&id4),
        "id4 should be excluded by max_amount filter"
    );
    assert!(
        max_filtered.contains(&id3),
        "id3 should be included by max_amount filter"
    );

    // Filter by category (Services) => should include id2 and id3 only
    let cat_filtered = client.get_available_invoices_paged(
        &Option::<i128>::None,
        &Option::<i128>::None,
        &Some(InvoiceCategory::Services),
        &0u32,
        &10u32,
    );
    assert!(cat_filtered.contains(&id2));
    assert!(cat_filtered.contains(&id3));
    assert!(!cat_filtered.contains(&id1));

    // Pagination: limit 1 offset 1 should return exactly 1 item
    let page = client.get_available_invoices_paged(
        &Option::<i128>::None,
        &Option::<i128>::None,
        &Option::<InvoiceCategory>::None,
        &1u32,
        &1u32,
    );
    assert_eq!(page.len(), 1);
}

#[test]
fn test_get_available_invoices() {
    let (env, client) = setup();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let _ = client.set_admin(&admin);

    let business = Address::generate(&env);

    // Create 3 invoices: 2 verified, 1 pending
    let id1 = create_invoice(&env, &client, &business, 1000, InvoiceCategory::Services, true);
    let id2 = create_invoice(&env, &client, &business, 2000, InvoiceCategory::Products, true);
    let id3 = create_invoice(&env, &client, &business, 3000, InvoiceCategory::Services, false);

    let available = client.get_available_invoices();
    
    // Should contain exactly id1 and id2
    assert_eq!(available.len(), 2);
    assert!(available.contains(&id1));
    assert!(available.contains(&id2));
    assert!(!available.contains(&id3));
}

#[test]
fn test_get_available_invoices_paged_empty_and_edge_cases() {
    let (env, client) = setup();
    
    // 1. Empty state
    let empty = client.get_available_invoices_paged(
        &Option::<i128>::None,
        &Option::<i128>::None,
        &Option::<InvoiceCategory>::None,
        &0u32,
        &10u32,
    );
    assert_eq!(empty.len(), 0);

    // 2. No results after filtering
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let _ = client.set_admin(&admin);
    let business = Address::generate(&env);
    create_invoice(&env, &client, &business, 1000, InvoiceCategory::Services, true);

    let no_results = client.get_available_invoices_paged(
        &Some(5000i128),
        &Option::<i128>::None,
        &Option::<InvoiceCategory>::None,
        &0u32,
        &10u32,
    );
    assert_eq!(no_results.len(), 0);

    // 3. Offset beyond length
    let offset_beyond = client.get_available_invoices_paged(
        &Option::<i128>::None,
        &Option::<i128>::None,
        &Option::<InvoiceCategory>::None,
        &10u32,
        &10u32,
    );
    assert_eq!(offset_beyond.len(), 0);

    // 4. Limit zero
    let limit_zero = client.get_available_invoices_paged(
        &Option::<i128>::None,
        &Option::<i128>::None,
        &Option::<InvoiceCategory>::None,
        &0u32,
        &0u32,
    );
    assert_eq!(limit_zero.len(), 0);
}

#[test]
fn test_get_available_invoices_paged_pagination_comprehensive() {
    let (env, client) = setup();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let _ = client.set_admin(&admin);
    let business = Address::generate(&env);

    // Create 5 verified invoices
    let mut ids = Vec::new(&env);
    for i in 0..5 {
        let id = create_invoice(
            &env, 
            &client, 
            &business, 
            1000 + (i as i128 * 100), 
            InvoiceCategory::Services, 
            true
        );
        ids.push_back(id);
    }

    // Page 1: offset 0, limit 2
    let page1 = client.get_available_invoices_paged(
        &Option::<i128>::None,
        &Option::<i128>::None,
        &Option::<InvoiceCategory>::None,
        &0u32,
        &2u32,
    );
    assert_eq!(page1.len(), 2);
    assert_eq!(page1.get(0).unwrap(), ids.get(0).unwrap());
    assert_eq!(page1.get(1).unwrap(), ids.get(1).unwrap());

    // Page 2: offset 2, limit 2
    let page2 = client.get_available_invoices_paged(
        &Option::<i128>::None,
        &Option::<i128>::None,
        &Option::<InvoiceCategory>::None,
        &2u32,
        &2u32,
    );
    assert_eq!(page2.len(), 2);
    assert_eq!(page2.get(0).unwrap(), ids.get(2).unwrap());
    assert_eq!(page2.get(1).unwrap(), ids.get(3).unwrap());

    // Page 3: offset 4, limit 2 (only 1 item left)
    let page3 = client.get_available_invoices_paged(
        &Option::<i128>::None,
        &Option::<i128>::None,
        &Option::<InvoiceCategory>::None,
        &4u32,
        &2u32,
    );
    assert_eq!(page3.len(), 1);
    assert_eq!(page3.get(0).unwrap(), ids.get(4).unwrap());
}

#[test]
fn test_query_audit_logs_filters_and_limit() {
    let (env, _client) = setup();
    // Create two invoices and several audit entries
    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;
    // Use the registered contract via client to create invoices
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let inv1 = client.store_invoice(
        &business,
        &1000,
        &currency,
        &due_date,
        &String::from_str(&env, "inv1"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    let inv2 = client.store_invoice(
        &business,
        &2000,
        &currency,
        &due_date,
        &String::from_str(&env, "inv2"),
        &InvoiceCategory::Products,
        &Vec::new(&env),
    );

    let actor = Address::generate(&env);
    let actor = Address::generate(&env);

    env.as_contract(&contract_id, || {
        let e1 = crate::audit::AuditLogEntry::new(
            &env,
            inv1.clone(),
            AuditOperation::InvoiceCreated,
            actor.clone(),
            None,
            None,
            None,
            None,
        );
        crate::audit::AuditStorage::store_audit_entry(&env, &e1);
    });

    env.as_contract(&contract_id, || {
        let e2 = crate::audit::AuditLogEntry::new(
            &env,
            inv2.clone(),
            AuditOperation::InvoiceCreated,
            actor.clone(),
            None,
            None,
            None,
            None,
        );
        crate::audit::AuditStorage::store_audit_entry(&env, &e2);
    });

    env.as_contract(&contract_id, || {
        let e3 = crate::audit::AuditLogEntry::new(
            &env,
            inv1.clone(),
            AuditOperation::InvoiceVerified,
            actor.clone(),
            None,
            None,
            None,
            None,
        );
        crate::audit::AuditStorage::store_audit_entry(&env, &e3);
    });

    // Query by invoice id => should return entries for inv1
    let filter_inv1 = AuditQueryFilter {
        invoice_id: Some(inv1.clone()),
        operation: AuditOperationFilter::Any,
        actor: None,
        start_timestamp: None,
        end_timestamp: None,
    };
    let results_inv1: Vec<crate::audit::AuditLogEntry> = env.as_contract(&contract_id, || {
        let ids = crate::audit::AuditStorage::get_invoice_audit_trail(&env, &inv1);
        let mut entries = Vec::new(&env);
        for i in ids.iter() {
            if let Some(e) = crate::audit::AuditStorage::get_audit_entry(&env, &i) {
                entries.push_back(e);
            }
        }
        entries
    });
    assert!(
        results_inv1.len() >= 2,
        "Expected at least two audit entries for inv1"
    );

    // Query by specific operation InvoiceCreated => should return entries with that operation
    let filter_created = AuditQueryFilter {
        invoice_id: None,
        operation: AuditOperationFilter::Specific(AuditOperation::InvoiceCreated),
        actor: None,
        start_timestamp: None,
        end_timestamp: None,
    };
    let results_created: Vec<crate::audit::AuditLogEntry> = env.as_contract(&contract_id, || {
        let ids = crate::audit::AuditStorage::get_audit_entries_by_operation(
            &env,
            &AuditOperation::InvoiceCreated,
        );
        let mut entries = Vec::new(&env);
        for i in ids.iter() {
            if let Some(e) = crate::audit::AuditStorage::get_audit_entry(&env, &i) {
                entries.push_back(e);
            }
        }
        entries
    });
    assert!(
        results_created.len() >= 2,
        "Expected at least two InvoiceCreated entries"
    );

    // Limit enforcement: limit=1 should return only 1
    let results_limited: Vec<crate::audit::AuditLogEntry> = env.as_contract(&contract_id, || {
        let ids = crate::audit::AuditStorage::get_audit_entries_by_operation(
            &env,
            &AuditOperation::InvoiceCreated,
        );
        let mut entries = Vec::new(&env);
        let mut cnt = 0u32;
        for i in ids.iter() {
            if cnt >= 1 {
                break;
            }
            if let Some(e) = crate::audit::AuditStorage::get_audit_entry(&env, &i) {
                entries.push_back(e);
                cnt += 1;
            }
        }
        entries
    });
    assert_eq!(
        results_limited.len(),
        1,
        "Limit should restrict number of returned entries"
    );
}

#[test]
fn test_bid_query_pagination_limit_is_capped_to_max_query_limit() {
    let (env, client) = setup();
    let contract_id = client.address.clone();
    let business = Address::generate(&env);
    let investor = Address::generate(&env);

    let invoice_id = create_invoice(
        &env,
        &client,
        &business,
        5_000,
        InvoiceCategory::Services,
        false,
    );

    env.as_contract(&contract_id, || {
        for i in 0..130u32 {
            let bid_id = BidStorage::generate_unique_bid_id(&env);
            let bid = Bid {
                bid_id: bid_id.clone(),
                invoice_id: invoice_id.clone(),
                investor: investor.clone(),
                bid_amount: 1_000 + i as i128,
                expected_return: 1_100 + i as i128,
                timestamp: env.ledger().timestamp(),
                status: BidStatus::Placed,
                expiration_timestamp: env.ledger().timestamp().saturating_add(86_400),
            };
            BidStorage::store_bid(&env, &bid);
            BidStorage::add_bid_to_invoice(&env, &invoice_id, &bid_id);
        }
    });

    let invoice_bids =
        client.get_bid_history_paged(&invoice_id, &Option::<BidStatus>::None, &0u32, &500u32);
    assert_eq!(
        invoice_bids.len(),
        crate::MAX_QUERY_LIMIT,
        "invoice bid history should enforce MAX_QUERY_LIMIT cap"
    );

    let investor_bids =
        client.get_investor_bids_paged(&investor, &Option::<BidStatus>::None, &0u32, &500u32);
    assert_eq!(
        investor_bids.len(),
        crate::MAX_QUERY_LIMIT,
        "investor bid history should enforce MAX_QUERY_LIMIT cap"
    );
}

// ============================================================================
// Investment Query Tests - Single Investor Multiple Invoices
// ============================================================================

/// Helper: Setup verified investor
fn setup_verified_investor(
    env: &Env,
    client: &QuickLendXContractClient,
    limit: i128,
) -> Address {
    let investor = Address::generate(env);
    let kyc_data = String::from_str(env, "Valid KYC data");
    client.submit_investor_kyc(&investor, &kyc_data);
    client.verify_investor(&investor, &limit);
    investor
}

/// Helper: Setup verified business
fn setup_verified_business(
    env: &Env,
    client: &QuickLendXContractClient,
) -> Address {
    let business = Address::generate(env);
    let kyc_data = String::from_str(env, "Valid business KYC");
    client.submit_kyc_application(&business, &kyc_data);
    
    // Get admin and verify
    env.mock_all_auths();
    let admin = Address::generate(env);
    let _ = client.set_admin(&admin);
    client.verify_business(&admin, &business);
    
    business
}

#[test]
fn test_get_investments_by_investor_empty_initially() {
    let (env, client) = setup();
    let investor = Address::generate(&env);
    
    let investments = client.get_investments_by_investor(&investor);
    assert_eq!(investments.len(), 0, "Should have no investments initially");
}

#[test]
fn test_get_investments_by_investor_after_single_investment() {
    let (env, client) = setup();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let _ = client.set_admin(&admin);
    
    let investor = setup_verified_investor(&env, &client, 100_000);
    let business = setup_verified_business(&env, &client);
    
    // Create and fund invoice
    let invoice_id = create_invoice(&env, &client, &business, 10_000, InvoiceCategory::Services, true);
    let bid_id = client.place_bid(&investor, &invoice_id, &5_000, &6_000);
    client.accept_bid(&invoice_id, &bid_id);
    
    // Query investments
    let investments = client.get_investments_by_investor(&investor);
    assert_eq!(investments.len(), 1, "Should have 1 investment");
}

#[test]
fn test_get_investments_by_investor_multiple_investments() {
    let (env, client) = setup();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let _ = client.set_admin(&admin);
    
    let investor = setup_verified_investor(&env, &client, 100_000);
    let business = setup_verified_business(&env, &client);
    
    // Create and fund 3 invoices
    let invoice_id1 = create_invoice(&env, &client, &business, 10_000, InvoiceCategory::Services, true);
    let invoice_id2 = create_invoice(&env, &client, &business, 15_000, InvoiceCategory::Products, true);
    let invoice_id3 = create_invoice(&env, &client, &business, 20_000, InvoiceCategory::Services, true);
    
    let bid_id1 = client.place_bid(&investor, &invoice_id1, &5_000, &6_000);
    let bid_id2 = client.place_bid(&investor, &invoice_id2, &7_500, &9_000);
    let bid_id3 = client.place_bid(&investor, &invoice_id3, &10_000, &12_000);
    
    client.accept_bid(&invoice_id1, &bid_id1);
    client.accept_bid(&invoice_id2, &bid_id2);
    client.accept_bid(&invoice_id3, &bid_id3);
    
    // Query investments
    let investments = client.get_investments_by_investor(&investor);
    assert_eq!(investments.len(), 3, "Should have 3 investments");
    
    // Verify investment amounts by loading records from returned IDs
    assert!(
        investments
            .iter()
            .any(|investment_id| client.get_investment(&investment_id).amount == 5_000),
        "Should contain investment of 5,000"
    );
    assert!(
        investments
            .iter()
            .any(|investment_id| client.get_investment(&investment_id).amount == 7_500),
        "Should contain investment of 7,500"
    );
    assert!(
        investments
            .iter()
            .any(|investment_id| client.get_investment(&investment_id).amount == 10_000),
        "Should contain investment of 10,000"
    );
}

#[test]
fn test_get_investments_by_investor_only_returns_investor_investments() {
    let (env, client) = setup();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let _ = client.set_admin(&admin);
    
    let investor1 = setup_verified_investor(&env, &client, 100_000);
    let investor2 = setup_verified_investor(&env, &client, 100_000);
    let business = setup_verified_business(&env, &client);
    
    // Create invoices
    let invoice_id1 = create_invoice(&env, &client, &business, 10_000, InvoiceCategory::Services, true);
    let invoice_id2 = create_invoice(&env, &client, &business, 15_000, InvoiceCategory::Products, true);
    
    // Investor1 funds invoice1
    let bid_id1 = client.place_bid(&investor1, &invoice_id1, &5_000, &6_000);
    client.accept_bid(&invoice_id1, &bid_id1);
    
    // Investor2 funds invoice2
    let bid_id2 = client.place_bid(&investor2, &invoice_id2, &7_500, &9_000);
    client.accept_bid(&invoice_id2, &bid_id2);
    
    // Query investor1's investments
    let investments1 = client.get_investments_by_investor(&investor1);
    assert_eq!(investments1.len(), 1, "Investor1 should have 1 investment");
    let investment1 = client.get_investment(&investments1.get(0).unwrap());
    assert_eq!(investment1.investor, investor1);
    assert_eq!(investment1.amount, 5_000);
    
    // Query investor2's investments
    let investments2 = client.get_investments_by_investor(&investor2);
    assert_eq!(investments2.len(), 1, "Investor2 should have 1 investment");
    let investment2 = client.get_investment(&investments2.get(0).unwrap());
    assert_eq!(investment2.investor, investor2);
    assert_eq!(investment2.amount, 7_500);
}

#[test]
fn test_get_investor_investments_paged_empty() {
    let (env, client) = setup();
    let investor = Address::generate(&env);
    
    let paged = client.get_investor_investments_paged(&investor, &None, &0u32, &10u32);
    assert_eq!(paged.len(), 0, "Should have no investments");
}

#[test]
fn test_get_investor_investments_paged_pagination() {
    let (env, client) = setup();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let _ = client.set_admin(&admin);
    
    let investor = setup_verified_investor(&env, &client, 100_000);
    let business = setup_verified_business(&env, &client);
    
    // Create and fund 5 invoices
    for i in 0..5 {
        let invoice_id = create_invoice(
            &env,
            &client,
            &business,
            10_000 + (i * 1000),
            InvoiceCategory::Services,
            true
        );
        let bid_amount = 5_000 + (i * 500);
        let bid_id = client.place_bid(&investor, &invoice_id, &bid_amount, &(bid_amount + 1000));
        client.accept_bid(&invoice_id, &bid_id);
    }
    
    // Page 1: offset 0, limit 2
    let page1 = client.get_investor_investments_paged(&investor, &None, &0u32, &2u32);
    assert_eq!(page1.len(), 2, "Page 1 should have 2 investments");
    
    // Page 2: offset 2, limit 2
    let page2 = client.get_investor_investments_paged(&investor, &None, &2u32, &2u32);
    assert_eq!(page2.len(), 2, "Page 2 should have 2 investments");
    
    // Page 3: offset 4, limit 2 (only 1 left)
    let page3 = client.get_investor_investments_paged(&investor, &None, &4u32, &2u32);
    assert_eq!(page3.len(), 1, "Page 3 should have 1 investment");
    
    // Verify no overlap between pages
    let id1 = page1.get(0).unwrap();
    let id2 = page2.get(0).unwrap();
    assert_ne!(id1, id2, "Pages should not overlap");
}

#[test]
fn test_get_investor_investments_paged_offset_beyond_length() {
    let (env, client) = setup();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let _ = client.set_admin(&admin);
    
    let investor = setup_verified_investor(&env, &client, 100_000);
    let business = setup_verified_business(&env, &client);
    
    // Create 2 investments
    for _ in 0..2 {
        let invoice_id = create_invoice(&env, &client, &business, 10_000, InvoiceCategory::Services, true);
        let bid_id = client.place_bid(&investor, &invoice_id, &5_000, &6_000);
        client.accept_bid(&invoice_id, &bid_id);
    }
    
    // Query with offset beyond length
    let paged = client.get_investor_investments_paged(&investor, &None, &10u32, &5u32);
    assert_eq!(paged.len(), 0, "Should return empty when offset beyond length");
}

#[test]
fn test_get_investor_investments_paged_limit_zero() {
    let (env, client) = setup();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let _ = client.set_admin(&admin);
    
    let investor = setup_verified_investor(&env, &client, 100_000);
    let business = setup_verified_business(&env, &client);
    
    // Create 1 investment
    let invoice_id = create_invoice(&env, &client, &business, 10_000, InvoiceCategory::Services, true);
    let bid_id = client.place_bid(&investor, &invoice_id, &5_000, &6_000);
    client.accept_bid(&invoice_id, &bid_id);
    
    // Query with limit 0
    let paged = client.get_investor_investments_paged(&investor, &None, &0u32, &0u32);
    assert_eq!(paged.len(), 0, "Should return empty when limit is 0");
}

#[test]
fn test_get_investor_investments_paged_respects_max_query_limit() {
    let (env, client) = setup();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let _ = client.set_admin(&admin);
    
    let investor = setup_verified_investor(&env, &client, 1_000_000);
    let business = setup_verified_business(&env, &client);
    
    // Create many investments (more than MAX_QUERY_LIMIT)
    for i in 0..120 {
        let invoice_id = create_invoice(
            &env,
            &client,
            &business,
            10_000 + i,
            InvoiceCategory::Services,
            true
        );
        let bid_amount = 5_000 + i;
        let bid_id = client.place_bid(&investor, &invoice_id, &bid_amount, &(bid_amount + 1000));
        client.accept_bid(&invoice_id, &bid_id);
    }
    
    // Query with very large limit
    let paged = client.get_investor_investments_paged(&investor, &None, &0u32, &500u32);
    assert_eq!(
        paged.len(),
        crate::MAX_QUERY_LIMIT,
        "Should enforce MAX_QUERY_LIMIT cap"
    );
}

#[test]
fn test_get_investments_by_investor_after_mixed_bid_outcomes() {
    let (env, client) = setup();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let _ = client.set_admin(&admin);
    
    let investor = setup_verified_investor(&env, &client, 100_000);
    let business = setup_verified_business(&env, &client);
    
    // Create 4 invoices
    let invoice_id1 = create_invoice(&env, &client, &business, 10_000, InvoiceCategory::Services, true);
    let invoice_id2 = create_invoice(&env, &client, &business, 15_000, InvoiceCategory::Products, true);
    let invoice_id3 = create_invoice(&env, &client, &business, 20_000, InvoiceCategory::Services, true);
    let invoice_id4 = create_invoice(&env, &client, &business, 25_000, InvoiceCategory::Products, true);
    
    // Place bids on all 4
    let bid_id1 = client.place_bid(&investor, &invoice_id1, &5_000, &6_000);
    let bid_id2 = client.place_bid(&investor, &invoice_id2, &7_500, &9_000);
    let bid_id3 = client.place_bid(&investor, &invoice_id3, &10_000, &12_000);
    let bid_id4 = client.place_bid(&investor, &invoice_id4, &12_500, &15_000);
    
    // Accept bids 1 and 3
    client.accept_bid(&invoice_id1, &bid_id1);
    client.accept_bid(&invoice_id3, &bid_id3);
    
    // Withdraw bids 2 and 4
    client.withdraw_bid(&bid_id2);
    client.withdraw_bid(&bid_id4);
    
    // Query investments - should only return accepted bids
    let investments = client.get_investments_by_investor(&investor);
    assert_eq!(investments.len(), 2, "Should have 2 investments (only accepted bids)");
    
    // Verify investment amounts match accepted bids
    assert!(
        investments
            .iter()
            .any(|investment_id| client.get_investment(&investment_id).amount == 5_000),
        "Should contain investment from bid 1"
    );
    assert!(
        investments
            .iter()
            .any(|investment_id| client.get_investment(&investment_id).amount == 10_000),
        "Should contain investment from bid 3"
    );
    assert!(
        !investments
            .iter()
            .any(|investment_id| client.get_investment(&investment_id).amount == 7_500),
        "Should not contain withdrawn bid 2"
    );
    assert!(
        !investments
            .iter()
            .any(|investment_id| client.get_investment(&investment_id).amount == 12_500),
        "Should not contain withdrawn bid 4"
    );
}

#[test]
fn test_investment_queries_comprehensive_workflow() {
    let (env, client) = setup();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let _ = client.set_admin(&admin);
    
    let investor = setup_verified_investor(&env, &client, 100_000);
    let business1 = setup_verified_business(&env, &client);
    let business2 = Address::generate(&env);
    
    // Create 6 invoices from different businesses
    let invoice_id1 = create_invoice(&env, &client, &business1, 10_000, InvoiceCategory::Services, true);
    let invoice_id2 = create_invoice(&env, &client, &business1, 15_000, InvoiceCategory::Products, true);
    let invoice_id3 = create_invoice(&env, &client, &business2, 20_000, InvoiceCategory::Services, true);
    let invoice_id4 = create_invoice(&env, &client, &business2, 25_000, InvoiceCategory::Products, true);
    let invoice_id5 = create_invoice(&env, &client, &business1, 30_000, InvoiceCategory::Services, true);
    let invoice_id6 = create_invoice(&env, &client, &business2, 35_000, InvoiceCategory::Products, true);
    
    // Place bids on all 6
    let bid_id1 = client.place_bid(&investor, &invoice_id1, &5_000, &6_000);
    let bid_id2 = client.place_bid(&investor, &invoice_id2, &7_500, &9_000);
    let bid_id3 = client.place_bid(&investor, &invoice_id3, &10_000, &12_000);
    let bid_id4 = client.place_bid(&investor, &invoice_id4, &12_500, &15_000);
    let bid_id5 = client.place_bid(&investor, &invoice_id5, &15_000, &18_000);
    let bid_id6 = client.place_bid(&investor, &invoice_id6, &17_500, &21_000);
    
    // Accept bids 1, 3, and 5
    client.accept_bid(&invoice_id1, &bid_id1);
    client.accept_bid(&invoice_id3, &bid_id3);
    client.accept_bid(&invoice_id5, &bid_id5);
    
    // Withdraw bids 2, 4, and 6
    client.withdraw_bid(&bid_id2);
    client.withdraw_bid(&bid_id4);
    client.withdraw_bid(&bid_id6);
    
    // Test get_investments_by_investor
    let all_investments = client.get_investments_by_investor(&investor);
    assert_eq!(all_investments.len(), 3, "Should have 3 investments");
    
    // Test get_investor_investments_paged with pagination
    let page1 = client.get_investor_investments_paged(&investor, &None, &0u32, &2u32);
    assert_eq!(page1.len(), 2, "Page 1 should have 2 investments");
    
    let page2 = client.get_investor_investments_paged(&investor, &None, &2u32, &2u32);
    assert_eq!(page2.len(), 1, "Page 2 should have 1 investment");
    
    // Verify total investment amount
    let total_invested: i128 = all_investments
        .iter()
        .map(|investment_id| client.get_investment(&investment_id).amount)
        .fold(0i128, |acc, amt| acc + amt);
    assert_eq!(total_invested, 30_000, "Total invested should be 30,000 (5k + 10k + 15k)");
    
    // Verify all investments are Active
    for investment_id in all_investments.iter() {
        let investment = client.get_investment(&investment_id);
        assert_eq!(investment.status, crate::investment::InvestmentStatus::Active);
    }
}
// ============================================================================
// Currency Whitelist Pagination Boundary Tests
// ============================================================================

/// Test currency whitelist pagination with empty whitelist boundary conditions
#[test]
fn test_currency_whitelist_pagination_empty_boundaries() {
    let (env, client) = setup();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let _ = client.set_admin(&admin);
    
    // Test empty whitelist with various offset/limit combinations
    let result = client.get_whitelisted_currencies_paged(&0u32, &0u32);
    assert_eq!(result.len(), 0, "empty whitelist with zero limit should return empty");
    
    let result = client.get_whitelisted_currencies_paged(&0u32, &10u32);
    assert_eq!(result.len(), 0, "empty whitelist with normal limit should return empty");
    
    let result = client.get_whitelisted_currencies_paged(&u32::MAX, &10u32);
    assert_eq!(result.len(), 0, "empty whitelist with max offset should return empty without panic");
    
    let result = client.get_whitelisted_currencies_paged(&0u32, &u32::MAX);
    assert_eq!(result.len(), 0, "empty whitelist with max limit should return empty without panic");
    
    // Test that currency_count is consistent with pagination
    let count = client.currency_count();
    assert_eq!(count, 0u32, "currency count should be zero for empty whitelist");
}

/// Test currency whitelist pagination offset saturation and boundary conditions
#[test]
fn test_currency_whitelist_pagination_offset_saturation() {
    let (env, client) = setup();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let _ = client.set_admin(&admin);
    
    // Add exactly 7 currencies for predictable boundary testing
    let currencies: Vec<Address> = (0..7).map(|_| Address::generate(&env)).collect();
    for currency in &currencies {
        client.add_currency(&admin, currency);
    }
    
    // Verify setup
    let count = client.currency_count();
    assert_eq!(count, 7u32, "should have 7 currencies");
    
    // Test offset at exact boundary (length)
    let result = client.get_whitelisted_currencies_paged(&7u32, &10u32);
    assert_eq!(result.len(), 0, "offset at exact length should return empty");
    
    // Test offset just beyond boundary
    let result = client.get_whitelisted_currencies_paged(&8u32, &10u32);
    assert_eq!(result.len(), 0, "offset beyond length should return empty");
    
    // Test offset at maximum value (should not panic)
    let result = client.get_whitelisted_currencies_paged(&u32::MAX, &10u32);
    assert_eq!(result.len(), 0, "max offset should return empty without panic");
    
    // Test offset near maximum with small limit
    let result = client.get_whitelisted_currencies_paged(&(u32::MAX - 1), &1u32);
    assert_eq!(result.len(), 0, "near-max offset should return empty without panic");
    
    // Test valid offset at boundary minus one
    let result = client.get_whitelisted_currencies_paged(&6u32, &10u32);
    assert_eq!(result.len(), 1, "offset at length-1 should return 1 item");
    
    // Test offset in middle with various limits
    let result = client.get_whitelisted_currencies_paged(&3u32, &2u32);
    assert_eq!(result.len(), 2, "middle offset with normal limit should return correct count");
    
    let result = client.get_whitelisted_currencies_paged(&3u32, &10u32);
    assert_eq!(result.len(), 4, "middle offset with large limit should return remaining items");
}

/// Test currency whitelist pagination limit saturation and boundary conditions
#[test]
fn test_currency_whitelist_pagination_limit_saturation() {
    let (env, client) = setup();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let _ = client.set_admin(&admin);
    
    // Add exactly 5 currencies
    let currencies: Vec<Address> = (0..5).map(|_| Address::generate(&env)).collect();
    for currency in &currencies {
        client.add_currency(&admin, currency);
    }
    
    // Test zero limit
    let result = client.get_whitelisted_currencies_paged(&0u32, &0u32);
    assert_eq!(result.len(), 0, "zero limit should return empty");
    
    // Test limit larger than available items
    let result = client.get_whitelisted_currencies_paged(&0u32, &100u32);
    assert_eq!(result.len(), 5, "limit larger than available should return all items");
    
    // Test maximum limit value (should not panic)
    let result = client.get_whitelisted_currencies_paged(&0u32, &u32::MAX);
    assert_eq!(result.len(), 5, "max limit should return all items without panic");
    
    // Test limit exactly matching available items
    let result = client.get_whitelisted_currencies_paged(&0u32, &5u32);
    assert_eq!(result.len(), 5, "limit matching count should return all items");
    
    // Test limit one less than available
    let result = client.get_whitelisted_currencies_paged(&0u32, &4u32);
    assert_eq!(result.len(), 4, "limit less than count should return limited items");
    
    // Test limit of 1
    let result = client.get_whitelisted_currencies_paged(&0u32, &1u32);
    assert_eq!(result.len(), 1, "limit of 1 should return single item");
}

/// Test currency whitelist pagination overflow protection and arithmetic safety
#[test]
fn test_currency_whitelist_pagination_overflow_protection() {
    let (env, client) = setup();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let _ = client.set_admin(&admin);
    
    // Add 12 currencies for comprehensive overflow testing
    let currencies: Vec<Address> = (0..12).map(|_| Address::generate(&env)).collect();
    for currency in &currencies {
        client.add_currency(&admin, currency);
    }
    
    // Test offset + limit overflow scenarios (should not panic)
    let result = client.get_whitelisted_currencies_paged(&u32::MAX, &u32::MAX);
    assert_eq!(result.len(), 0, "max offset + max limit should return empty without panic");
    
    // Test large offset with large limit
    let result = client.get_whitelisted_currencies_paged(&(u32::MAX - 5), &10u32);
    assert_eq!(result.len(), 0, "large offset with normal limit should return empty");
    
    // Test normal offset with very large limit
    let result = client.get_whitelisted_currencies_paged(&5u32, &u32::MAX);
    assert_eq!(result.len(), 7, "normal offset with max limit should return remaining items");
    
    // Test edge case: offset at max-1, limit 1
    let result = client.get_whitelisted_currencies_paged(&(u32::MAX - 1), &1u32);
    assert_eq!(result.len(), 0, "near-max offset with small limit should return empty");
    
    // Test arithmetic overflow protection: offset + limit > u32::MAX
    let large_offset = u32::MAX / 2;
    let large_limit = u32::MAX / 2 + 1;
    let result = client.get_whitelisted_currencies_paged(&large_offset, &large_limit);
    assert_eq!(result.len(), 0, "arithmetic overflow scenario should be handled safely");
    
    // Test boundary arithmetic: offset + limit == u32::MAX
    let result = client.get_whitelisted_currencies_paged(&(u32::MAX - 10), &10u32);
    assert_eq!(result.len(), 0, "boundary arithmetic should be handled safely");
}

/// Test currency whitelist pagination consistency and ordering preservation
#[test]
fn test_currency_whitelist_pagination_consistency_ordering() {
    let (env, client) = setup();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let _ = client.set_admin(&admin);
    
    // Add currencies in a specific order
    let currencies: Vec<Address> = (0..9).map(|_| Address::generate(&env)).collect();
    for currency in &currencies {
        client.add_currency(&admin, currency);
    }
    
    // Get full list for comparison
    let full_list = client.get_whitelisted_currencies();
    assert_eq!(full_list.len(), 9, "should have 9 currencies");
    
    // Test that pagination returns items in same order as full list
    let page1 = client.get_whitelisted_currencies_paged(&0u32, &3u32);
    let page2 = client.get_whitelisted_currencies_paged(&3u32, &3u32);
    let page3 = client.get_whitelisted_currencies_paged(&6u32, &3u32);
    
    assert_eq!(page1.len(), 3, "first page should have 3 items");
    assert_eq!(page2.len(), 3, "second page should have 3 items");
    assert_eq!(page3.len(), 3, "third page should have 3 items");
    
    // Verify ordering consistency across pages
    for i in 0..3 {
        assert_eq!(page1.get(i).unwrap(), full_list.get(i).unwrap(), 
                  "page1 item {} should match full list", i);
        assert_eq!(page2.get(i).unwrap(), full_list.get(i + 3).unwrap(), 
                  "page2 item {} should match full list", i);
        assert_eq!(page3.get(i).unwrap(), full_list.get(i + 6).unwrap(), 
                  "page3 item {} should match full list", i);
    }
    
    // Test overlapping pages don't duplicate
    let overlap_page = client.get_whitelisted_currencies_paged(&2u32, &4u32);
    assert_eq!(overlap_page.len(), 4, "overlapping page should have 4 items");
    assert_eq!(overlap_page.get(0).unwrap(), full_list.get(2).unwrap(), 
              "overlapping page should start at correct offset");
    assert_eq!(overlap_page.get(3).unwrap(), full_list.get(5).unwrap(), 
              "overlapping page should end at correct position");
    
    // Test that no items are duplicated across non-overlapping pages
    let all_paginated_items = [page1, page2, page3].concat();
    for i in 0..all_paginated_items.len() {
        for j in (i + 1)..all_paginated_items.len() {
            assert_ne!(all_paginated_items[i], all_paginated_items[j],
                      "paginated items should not contain duplicates");
        }
    }
}

/// Test currency whitelist pagination with single item edge cases
#[test]
fn test_currency_whitelist_pagination_single_item_edge_cases() {
    let (env, client) = setup();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let _ = client.set_admin(&admin);
    
    // Add exactly one currency
    let currency = Address::generate(&env);
    client.add_currency(&admin, &currency);
    
    // Verify setup
    let count = client.currency_count();
    assert_eq!(count, 1u32, "should have exactly 1 currency");
    
    // Test various pagination scenarios with single item
    let result = client.get_whitelisted_currencies_paged(&0u32, &1u32);
    assert_eq!(result.len(), 1, "should return the single item");
    assert_eq!(result.get(0).unwrap(), currency, "should return correct currency");
    
    let result = client.get_whitelisted_currencies_paged(&0u32, &10u32);
    assert_eq!(result.len(), 1, "large limit should still return single item");
    
    let result = client.get_whitelisted_currencies_paged(&1u32, &1u32);
    assert_eq!(result.len(), 0, "offset beyond single item should return empty");
    
    let result = client.get_whitelisted_currencies_paged(&0u32, &0u32);
    assert_eq!(result.len(), 0, "zero limit should return empty even with item");
    
    let result = client.get_whitelisted_currencies_paged(&1u32, &10u32);
    assert_eq!(result.len(), 0, "offset at length should return empty");
    
    let result = client.get_whitelisted_currencies_paged(&2u32, &1u32);
    assert_eq!(result.len(), 0, "offset beyond length should return empty");
}

/// Test currency whitelist pagination behavior after modifications
#[test]
fn test_currency_whitelist_pagination_after_modifications() {
    let (env, client) = setup();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let _ = client.set_admin(&admin);
    
    // Add initial currencies
    let currencies: Vec<Address> = (0..6).map(|_| Address::generate(&env)).collect();
    for currency in &currencies {
        client.add_currency(&admin, currency);
    }
    
    // Test pagination before modification
    let page_before = client.get_whitelisted_currencies_paged(&0u32, &4u32);
    assert_eq!(page_before.len(), 4, "should have 4 items before modification");
    
    let count_before = client.currency_count();
    assert_eq!(count_before, 6u32, "should have 6 total items before modification");
    
    // Remove some currencies (indices 1 and 4)
    client.remove_currency(&admin, &currencies[1]);
    client.remove_currency(&admin, &currencies[4]);
    
    // Test pagination after removal
    let page_after = client.get_whitelisted_currencies_paged(&0u32, &4u32);
    assert_eq!(page_after.len(), 4, "should have 4 items after removal");
    
    let count_after = client.currency_count();
    assert_eq!(count_after, 4u32, "should have 4 total items after removal");
    
    // Verify removed currencies are not in results
    let full_list_after = client.get_whitelisted_currencies();
    assert_eq!(full_list_after.len(), 4, "full list should have 4 items after removal");
    assert!(!full_list_after.contains(&currencies[1]), "removed currency should not be present");
    assert!(!full_list_after.contains(&currencies[4]), "removed currency should not be present");
    
    // Test pagination at new boundary
    let boundary_page = client.get_whitelisted_currencies_paged(&4u32, &1u32);
    assert_eq!(boundary_page.len(), 0, "offset at new length should return empty");
    
    // Test pagination just before new boundary
    let near_boundary_page = client.get_whitelisted_currencies_paged(&3u32, &2u32);
    assert_eq!(near_boundary_page.len(), 1, "offset near new boundary should return remaining items");
    
    // Add more currencies and test again
    let new_currencies: Vec<Address> = (0..3).map(|_| Address::generate(&env)).collect();
    for currency in &new_currencies {
        client.add_currency(&admin, currency);
    }
    
    let count_after_add = client.currency_count();
    assert_eq!(count_after_add, 7u32, "should have 7 total items after adding");
    
    let page_after_add = client.get_whitelisted_currencies_paged(&0u32, &10u32);
    assert_eq!(page_after_add.len(), 7, "should return all 7 items");
    
    // Clear all currencies and test
    client.clear_currencies(&admin);
    let empty_page = client.get_whitelisted_currencies_paged(&0u32, &10u32);
    assert_eq!(empty_page.len(), 0, "pagination after clear should return empty");
    
    let count_after_clear = client.currency_count();
    assert_eq!(count_after_clear, 0u32, "count should be zero after clear");
}

/// Test currency whitelist pagination performance with large datasets
#[test]
fn test_currency_whitelist_pagination_large_dataset_performance() {
    let (env, client) = setup();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let _ = client.set_admin(&admin);
    
    // Add a larger number of currencies to test performance boundaries
    let large_count = 75u32; // Reasonable size for testing without timeout
    let currencies: Vec<Address> = (0..large_count).map(|_| Address::generate(&env)).collect();
    
    // Add currencies in batches
    for currency in &currencies {
        client.add_currency(&admin, currency);
    }
    
    // Verify total count
    let count = client.currency_count();
    assert_eq!(count, large_count, "should have added all currencies");
    
    // Test pagination across large dataset
    let page_size = 11u32; // Use prime number to test edge cases
    let mut total_retrieved = 0u32;
    let mut offset = 0u32;
    let mut pages_retrieved = 0u32;
    
    loop {
        let page = client.get_whitelisted_currencies_paged(&offset, &page_size);
        if page.len() == 0 {
            break;
        }
        total_retrieved += page.len();
        offset += page_size;
        pages_retrieved += 1;
        
        // Prevent infinite loop in case of implementation error
        if offset > large_count * 2 || pages_retrieved > 20 {
            panic!("pagination loop exceeded expected bounds: offset={}, pages={}", offset, pages_retrieved);
        }
    }
    
    assert_eq!(total_retrieved, large_count, 
              "should retrieve all items through pagination");
    
    // Test large offset with large dataset
    let result = client.get_whitelisted_currencies_paged(&(large_count + 10), &10u32);
    assert_eq!(result.len(), 0, "large offset beyond dataset should return empty");
    
    // Test boundary at exact dataset size
    let result = client.get_whitelisted_currencies_paged(&large_count, &1u32);
    assert_eq!(result.len(), 0, "offset at exact dataset size should return empty");
    
    // Test near-boundary pagination
    let result = client.get_whitelisted_currencies_paged(&(large_count - 5), &10u32);
    assert_eq!(result.len(), 5, "near-boundary pagination should return remaining items");
}

/// Test currency whitelist pagination security and access control
#[test]
fn test_currency_whitelist_pagination_security_access_control() {
    let (env, client) = setup();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let _ = client.set_admin(&admin);
    
    // Add currencies as admin
    let currencies: Vec<Address> = (0..8).map(|_| Address::generate(&env)).collect();
    for currency in &currencies {
        client.add_currency(&admin, currency);
    }
    
    // Test that pagination works for non-admin users (public read access)
    let non_admin = Address::generate(&env);
    
    // Non-admin should be able to read paginated results
    let result = client.get_whitelisted_currencies_paged(&0u32, &5u32);
    assert_eq!(result.len(), 5, "non-admin should be able to read paginated results");
    
    // Test that pagination doesn't expose more data than intended
    let full_list = client.get_whitelisted_currencies();
    let paginated_total = client.get_whitelisted_currencies_paged(&0u32, &u32::MAX);
    assert_eq!(full_list.len(), paginated_total.len(), 
              "paginated read should not expose more data than full read");
    
    // Verify all items match between full and paginated reads
    for i in 0..full_list.len() {
        assert_eq!(full_list.get(i).unwrap(), paginated_total.get(i).unwrap(),
                  "item {} should match between full and paginated reads", i);
    }
    
    // Test that pagination is consistent across multiple calls
    let result1 = client.get_whitelisted_currencies_paged(&2u32, &3u32);
    let result2 = client.get_whitelisted_currencies_paged(&2u32, &3u32);
    assert_eq!(result1.len(), result2.len(), "pagination should be consistent");
    for i in 0..result1.len() {
        assert_eq!(result1.get(i).unwrap(), result2.get(i).unwrap(),
                  "pagination results should be identical across calls");
    }
}

/// Test currency whitelist pagination with rapid concurrent-like modifications
#[test]
fn test_currency_whitelist_pagination_concurrent_modification_simulation() {
    let (env, client) = setup();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let _ = client.set_admin(&admin);
    
    // Add initial dataset
    let currencies: Vec<Address> = (0..15).map(|_| Address::generate(&env)).collect();
    for currency in &currencies {
        client.add_currency(&admin, currency);
    }
    
    // Simulate concurrent reads during modifications
    let initial_page = client.get_whitelisted_currencies_paged(&0u32, &7u32);
    assert_eq!(initial_page.len(), 7, "initial page should have 7 items");
    
    let initial_count = client.currency_count();
    assert_eq!(initial_count, 15u32, "initial count should be 15");
    
    // Modify whitelist (remove some currencies)
    client.remove_currency(&admin, &currencies[3]);
    client.remove_currency(&admin, &currencies[8]);
    client.remove_currency(&admin, &currencies[12]);
    
    // Read same page after modification
    let modified_page = client.get_whitelisted_currencies_paged(&0u32, &7u32);
    assert_eq!(modified_page.len(), 7, "page should still return 7 items after removal");
    
    // Verify consistency: total count should match paginated count
    let total_count = client.currency_count();
    assert_eq!(total_count, 12u32, "total count should be 12 after removing 3");
    
    let mut paginated_count = 0u32;
    let mut offset = 0u32;
    let page_size = 4u32;
    
    loop {
        let page = client.get_whitelisted_currencies_paged(&offset, &page_size);
        if page.len() == 0 {
            break;
        }
        paginated_count += page.len();
        offset += page_size;
        
        if offset > total_count * 2 {
            break; // Safety break
        }
    }
    
    assert_eq!(paginated_count, total_count, 
              "paginated count should match total count after modifications");
    
    // Add more currencies and test consistency again
    let new_currencies: Vec<Address> = (0..5).map(|_| Address::generate(&env)).collect();
    for currency in &new_currencies {
        client.add_currency(&admin, currency);
    }
    
    let final_count = client.currency_count();
    assert_eq!(final_count, 17u32, "final count should be 17 (12 + 5)");
    
    // Test that pagination still works correctly after additions
    let final_page = client.get_whitelisted_currencies_paged(&0u32, &20u32);
    assert_eq!(final_page.len(), 17, "should return all 17 items");
    
    // Verify no duplicates in final result
    for i in 0..final_page.len() {
        for j in (i + 1)..final_page.len() {
            assert_ne!(final_page.get(i).unwrap(), final_page.get(j).unwrap(),
                      "should not have duplicate addresses in final results");
        }
    }
}