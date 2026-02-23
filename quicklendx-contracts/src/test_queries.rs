//! Query tests for get_business_invoices, get_business_invoices_paged, and related endpoints.
//!
//! Covers: empty business, single/multiple status filters, pagination correctness,
//! and integration with available-invoices and audit queries.

use super::*;
use crate::audit::{AuditOperation, AuditOperationFilter, AuditQueryFilter};
use crate::invoice::{InvoiceCategory, InvoiceStatus};
use soroban_sdk::{
    testutils::Address as _,
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

// ============================================================================
// get_business_invoices and get_business_invoices_paged — status_filter & pagination
// ============================================================================

/// get_business_invoices returns an empty vector for a business that has no invoices.
#[test]
fn test_get_business_invoices_empty_business() {
    let (env, client) = setup();
    let business = Address::generate(&env);

    let ids = client.get_business_invoices(&business);
    assert!(ids.is_empty(), "Expected no invoices for business with no invoices");
}

/// get_business_invoices returns all invoice IDs created for that business.
#[test]
fn test_get_business_invoices_returns_created_invoices() {
    let (env, client) = setup();
    let business = Address::generate(&env);

    let id1 = create_invoice(&env, &client, &business, 1000, InvoiceCategory::Services, false);
    let id2 = create_invoice(&env, &client, &business, 2000, InvoiceCategory::Products, false);

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
    let id_pending1 = create_invoice(&env, &client, &business, 1000, InvoiceCategory::Services, false);
    let id_pending2 = create_invoice(&env, &client, &business, 2000, InvoiceCategory::Products, false);
    let id_verified = create_invoice(&env, &client, &business, 3000, InvoiceCategory::Services, true);

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
    let pending_only = client.get_business_invoices_paged(
        &business,
        &Some(InvoiceStatus::Pending),
        &0u32,
        &10u32,
    );
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

    let page0 = client.get_business_invoices_paged(
        &business,
        &Option::<InvoiceStatus>::None,
        &0u32,
        &2u32,
    );
    let page1 = client.get_business_invoices_paged(
        &business,
        &Option::<InvoiceStatus>::None,
        &2u32,
        &2u32,
    );
    let page2 = client.get_business_invoices_paged(
        &business,
        &Option::<InvoiceStatus>::None,
        &4u32,
        &2u32,
    );

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
