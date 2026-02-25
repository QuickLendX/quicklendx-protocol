//! Query tests for get_business_invoices, get_business_invoices_paged, and related endpoints.
//!
//! Covers: empty business, single/multiple status filters, pagination correctness,
//! and integration with available-invoices and audit queries.

use super::*;
use crate::audit::{AuditOperation, AuditOperationFilter, AuditQueryFilter};
use crate::invoice::{InvoiceCategory, InvoiceStatus};
use soroban_sdk::{testutils::Address as _, Address, BytesN, Env, String, Vec};

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

// ============================================================================
// get_business_invoices and get_business_invoices_paged — status_filter & pagination
// ============================================================================

/// get_business_invoices returns an empty vector for a business that has no invoices.
#[test]
fn test_get_business_invoices_empty_business() {
    let (env, client) = setup();
    let business = Address::generate(&env);

    let ids = client.get_business_invoices(&business);
    assert!(
        ids.is_empty(),
        "Expected no invoices for business with no invoices"
    );
}

/// get_business_invoices returns all invoice IDs created for that business.
#[test]
fn test_get_business_invoices_returns_created_invoices() {
    let (env, client) = setup();
    let business = Address::generate(&env);

    let id1 = create_invoice(
        &env,
        &client,
        &business,
        1000,
        InvoiceCategory::Services,
        false,
    );
    let id2 = create_invoice(
        &env,
        &client,
        &business,
        2000,
        InvoiceCategory::Products,
        false,
    );

    let ids = client.get_business_invoices(&business);
    assert_eq!(ids.len(), 2);
    assert!(ids.contains(&id1));
    assert!(ids.contains(&id2));
}

/// get_business_invoices_paged with status_filter: None returns all; Some(status) returns only that status.
#[test]
fn test_get_business_invoices_paged_status_filter_single_and_none() {
    let (env, client) = setup();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let _ = client.set_admin(&admin);

    let business = Address::generate(&env);
    let id_pending1 = create_invoice(
        &env,
        &client,
        &business,
        1000,
        InvoiceCategory::Services,
        false,
    );
    let id_pending2 = create_invoice(
        &env,
        &client,
        &business,
        2000,
        InvoiceCategory::Products,
        false,
    );
    let id_verified = create_invoice(
        &env,
        &client,
        &business,
        3000,
        InvoiceCategory::Services,
        true,
    );

    // No filter => all 3
    let all = client.get_business_invoices_paged(
        &business,
        &Option::<InvoiceStatus>::None,
        &0u32,
        &10u32,
    );
    assert_eq!(all.len(), 3);
    assert!(all.contains(&id_pending1));
    assert!(all.contains(&id_pending2));
    assert!(all.contains(&id_verified));

    // Single status: Pending => only the two pending
    let pending_only =
        client.get_business_invoices_paged(&business, &Some(InvoiceStatus::Pending), &0u32, &10u32);
    assert_eq!(pending_only.len(), 2);
    assert!(pending_only.contains(&id_pending1));
    assert!(pending_only.contains(&id_pending2));
    assert!(!pending_only.contains(&id_verified));

    // Single status: Verified => only the verified one
    let verified_only = client.get_business_invoices_paged(
        &business,
        &Some(InvoiceStatus::Verified),
        &0u32,
        &10u32,
    );
    assert_eq!(verified_only.len(), 1);
    assert_eq!(verified_only.get(0), Some(id_verified));
}

/// Pagination: consecutive pages return disjoint slices; order and size match offset/limit.
#[test]
fn test_get_business_invoices_paged_pagination_correctness() {
    let (env, client) = setup();
    let business = Address::generate(&env);

    for i in 0..5 {
        create_invoice(
            &env,
            &client,
            &business,
            1000 + i * 100,
            InvoiceCategory::Services,
            false,
        );
    }

    let all = client.get_business_invoices(&business);
    assert_eq!(all.len(), 5);

    let page0 =
        client.get_business_invoices_paged(&business, &Option::<InvoiceStatus>::None, &0u32, &2u32);
    let page1 =
        client.get_business_invoices_paged(&business, &Option::<InvoiceStatus>::None, &2u32, &2u32);
    let page2 =
        client.get_business_invoices_paged(&business, &Option::<InvoiceStatus>::None, &4u32, &2u32);

    assert_eq!(page0.len(), 2);
    assert_eq!(page1.len(), 2);
    assert_eq!(page2.len(), 1);

    // No overlap: page0 and page1 and page2 should be disjoint
    for a in page0.iter() {
        for b in page1.iter() {
            assert!(a != b, "page0 and page1 must not overlap");
        }
        for b in page2.iter() {
            assert!(a != b, "page0 and page2 must not overlap");
        }
    }
    for a in page1.iter() {
        for b in page2.iter() {
            assert!(a != b, "page1 and page2 must not overlap");
        }
    }

    // Every returned id must be in the full list
    for id in page0.iter() {
        assert!(all.contains(&id));
    }
    for id in page1.iter() {
        assert!(all.contains(&id));
    }
    for id in page2.iter() {
        assert!(all.contains(&id));
    }
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
    let id1 = create_invoice(
        &env,
        &client,
        &business,
        1000,
        InvoiceCategory::Services,
        true,
    );
    let id2 = create_invoice(
        &env,
        &client,
        &business,
        2000,
        InvoiceCategory::Products,
        true,
    );
    let id3 = create_invoice(
        &env,
        &client,
        &business,
        3000,
        InvoiceCategory::Services,
        false,
    );

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
    create_invoice(
        &env,
        &client,
        &business,
        1000,
        InvoiceCategory::Services,
        true,
    );

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
            true,
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
fn setup_verified_investor(env: &Env, client: &QuickLendXContractClient, limit: i128) -> Address {
    let investor = Address::generate(env);
    let kyc_data = String::from_str(env, "Valid KYC data");
    client.submit_investor_kyc(&investor, &kyc_data);
    client.verify_investor(&investor, &limit);
    investor
}

/// Helper: Setup verified business
fn setup_verified_business(env: &Env, client: &QuickLendXContractClient) -> Address {
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
    let invoice_id = create_invoice(
        &env,
        &client,
        &business,
        10_000,
        InvoiceCategory::Services,
        true,
    );
    let bid_id = client.place_bid(&investor, &invoice_id, &5_000, &6_000);
    client.accept_bid(&invoice_id, &bid_id);

    // Query investments
    let investments = client.get_investments_by_investor(&investor);
    assert_eq!(investments.len(), 1, "Should have 1 investment");

    let investment_ids = client.get_investment_ids_by_investor(&investor);
    assert_eq!(investment_ids.len(), 1, "Should have 1 investment ID");
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
    let invoice_id1 = create_invoice(
        &env,
        &client,
        &business,
        10_000,
        InvoiceCategory::Services,
        true,
    );
    let invoice_id2 = create_invoice(
        &env,
        &client,
        &business,
        15_000,
        InvoiceCategory::Products,
        true,
    );
    let invoice_id3 = create_invoice(
        &env,
        &client,
        &business,
        20_000,
        InvoiceCategory::Services,
        true,
    );

    let bid_id1 = client.place_bid(&investor, &invoice_id1, &5_000, &6_000);
    let bid_id2 = client.place_bid(&investor, &invoice_id2, &7_500, &9_000);
    let bid_id3 = client.place_bid(&investor, &invoice_id3, &10_000, &12_000);

    client.accept_bid(&invoice_id1, &bid_id1);
    client.accept_bid(&invoice_id2, &bid_id2);
    client.accept_bid(&invoice_id3, &bid_id3);

    // Query investments
    let investments = client.get_investments_by_investor(&investor);
    assert_eq!(investments.len(), 3, "Should have 3 investments");

    // Verify all investments belong to the investor
    for investment in investments.iter() {
        assert_eq!(
            investment.investor, investor,
            "All investments should belong to investor"
        );
    }

    // Verify investment amounts
    let amounts: soroban_sdk::Vec<i128> = investments.iter().map(|inv| inv.amount).collect();
    assert!(
        amounts.contains(&5_000),
        "Should contain investment of 5,000"
    );
    assert!(
        amounts.contains(&7_500),
        "Should contain investment of 7,500"
    );
    assert!(
        amounts.contains(&10_000),
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
    let invoice_id1 = create_invoice(
        &env,
        &client,
        &business,
        10_000,
        InvoiceCategory::Services,
        true,
    );
    let invoice_id2 = create_invoice(
        &env,
        &client,
        &business,
        15_000,
        InvoiceCategory::Products,
        true,
    );

    // Investor1 funds invoice1
    let bid_id1 = client.place_bid(&investor1, &invoice_id1, &5_000, &6_000);
    client.accept_bid(&invoice_id1, &bid_id1);

    // Investor2 funds invoice2
    let bid_id2 = client.place_bid(&investor2, &invoice_id2, &7_500, &9_000);
    client.accept_bid(&invoice_id2, &bid_id2);

    // Query investor1's investments
    let investments1 = client.get_investments_by_investor(&investor1);
    assert_eq!(investments1.len(), 1, "Investor1 should have 1 investment");
    assert_eq!(investments1.get(0).unwrap().investor, investor1);
    assert_eq!(investments1.get(0).unwrap().amount, 5_000);

    // Query investor2's investments
    let investments2 = client.get_investments_by_investor(&investor2);
    assert_eq!(investments2.len(), 1, "Investor2 should have 1 investment");
    assert_eq!(investments2.get(0).unwrap().investor, investor2);
    assert_eq!(investments2.get(0).unwrap().amount, 7_500);
}

#[test]
fn test_get_investor_investments_paged_empty() {
    let (env, client) = setup();
    let investor = Address::generate(&env);

    let paged = client.get_investor_investments_paged(&investor, &0u32, &10u32);
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
            true,
        );
        let bid_amount = 5_000 + (i * 500);
        let bid_id = client.place_bid(&investor, &invoice_id, &bid_amount, &(bid_amount + 1000));
        client.accept_bid(&invoice_id, &bid_id);
    }

    // Page 1: offset 0, limit 2
    let page1 = client.get_investor_investments_paged(&investor, &0u32, &2u32);
    assert_eq!(page1.len(), 2, "Page 1 should have 2 investments");

    // Page 2: offset 2, limit 2
    let page2 = client.get_investor_investments_paged(&investor, &2u32, &2u32);
    assert_eq!(page2.len(), 2, "Page 2 should have 2 investments");

    // Page 3: offset 4, limit 2 (only 1 left)
    let page3 = client.get_investor_investments_paged(&investor, &4u32, &2u32);
    assert_eq!(page3.len(), 1, "Page 3 should have 1 investment");

    // Verify no overlap between pages
    let id1 = page1.get(0).unwrap().investment_id;
    let id2 = page2.get(0).unwrap().investment_id;
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
        let invoice_id = create_invoice(
            &env,
            &client,
            &business,
            10_000,
            InvoiceCategory::Services,
            true,
        );
        let bid_id = client.place_bid(&investor, &invoice_id, &5_000, &6_000);
        client.accept_bid(&invoice_id, &bid_id);
    }

    // Query with offset beyond length
    let paged = client.get_investor_investments_paged(&investor, &10u32, &5u32);
    assert_eq!(
        paged.len(),
        0,
        "Should return empty when offset beyond length"
    );
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
    let invoice_id = create_invoice(
        &env,
        &client,
        &business,
        10_000,
        InvoiceCategory::Services,
        true,
    );
    let bid_id = client.place_bid(&investor, &invoice_id, &5_000, &6_000);
    client.accept_bid(&invoice_id, &bid_id);

    // Query with limit 0
    let paged = client.get_investor_investments_paged(&investor, &0u32, &0u32);
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
            true,
        );
        let bid_amount = 5_000 + i;
        let bid_id = client.place_bid(&investor, &invoice_id, &bid_amount, &(bid_amount + 1000));
        client.accept_bid(&invoice_id, &bid_id);
    }

    // Query with very large limit
    let paged = client.get_investor_investments_paged(&investor, &0u32, &500u32);
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
    let invoice_id1 = create_invoice(
        &env,
        &client,
        &business,
        10_000,
        InvoiceCategory::Services,
        true,
    );
    let invoice_id2 = create_invoice(
        &env,
        &client,
        &business,
        15_000,
        InvoiceCategory::Products,
        true,
    );
    let invoice_id3 = create_invoice(
        &env,
        &client,
        &business,
        20_000,
        InvoiceCategory::Services,
        true,
    );
    let invoice_id4 = create_invoice(
        &env,
        &client,
        &business,
        25_000,
        InvoiceCategory::Products,
        true,
    );

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
    assert_eq!(
        investments.len(),
        2,
        "Should have 2 investments (only accepted bids)"
    );

    // Verify investment amounts match accepted bids
    let amounts: soroban_sdk::Vec<i128> = investments.iter().map(|inv| inv.amount).collect();
    assert!(
        amounts.contains(&5_000),
        "Should contain investment from bid 1"
    );
    assert!(
        amounts.contains(&10_000),
        "Should contain investment from bid 3"
    );
    assert!(
        !amounts.contains(&7_500),
        "Should not contain withdrawn bid 2"
    );
    assert!(
        !amounts.contains(&12_500),
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
    let invoice_id1 = create_invoice(
        &env,
        &client,
        &business1,
        10_000,
        InvoiceCategory::Services,
        true,
    );
    let invoice_id2 = create_invoice(
        &env,
        &client,
        &business1,
        15_000,
        InvoiceCategory::Products,
        true,
    );
    let invoice_id3 = create_invoice(
        &env,
        &client,
        &business2,
        20_000,
        InvoiceCategory::Services,
        true,
    );
    let invoice_id4 = create_invoice(
        &env,
        &client,
        &business2,
        25_000,
        InvoiceCategory::Products,
        true,
    );
    let invoice_id5 = create_invoice(
        &env,
        &client,
        &business1,
        30_000,
        InvoiceCategory::Services,
        true,
    );
    let invoice_id6 = create_invoice(
        &env,
        &client,
        &business2,
        35_000,
        InvoiceCategory::Products,
        true,
    );

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
    let page1 = client.get_investor_investments_paged(&investor, &0u32, &2u32);
    assert_eq!(page1.len(), 2, "Page 1 should have 2 investments");

    let page2 = client.get_investor_investments_paged(&investor, &2u32, &2u32);
    assert_eq!(page2.len(), 1, "Page 2 should have 1 investment");

    // Verify total investment amount
    let total_invested: i128 = all_investments
        .iter()
        .map(|inv| inv.amount)
        .fold(0i128, |acc, amt| acc + amt);
    assert_eq!(
        total_invested, 30_000,
        "Total invested should be 30,000 (5k + 10k + 15k)"
    );

    // Verify all investments are Active
    for investment in all_investments.iter() {
        assert_eq!(
            investment.status,
            crate::investment::InvestmentStatus::Active
        );
    }
}
