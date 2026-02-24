use super::*;
use crate::audit::{AuditOperation, AuditOperationFilter, AuditQueryFilter};
use crate::bid::{Bid, BidStatus, BidStorage};
use crate::investment::{Investment, InvestmentStatus, InvestmentStorage};
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

    let invoice_bids = client.get_bid_history_paged(
        &invoice_id,
        &Option::<BidStatus>::None,
        &0u32,
        &500u32,
    );
    assert_eq!(
        invoice_bids.len(),
        crate::MAX_QUERY_LIMIT,
        "invoice bid history should enforce MAX_QUERY_LIMIT cap"
    );

    let investor_bids = client.get_investor_bids_paged(
        &investor,
        &Option::<BidStatus>::None,
        &0u32,
        &500u32,
    );
    assert_eq!(
        investor_bids.len(),
        crate::MAX_QUERY_LIMIT,
        "investor bid history should enforce MAX_QUERY_LIMIT cap"
    );
}

#[test]
fn test_get_business_invoices_paged_filter_combinations_and_overflow_safety() {
    let (env, client) = setup();
    env.mock_all_auths();

    let business = Address::generate(&env);
    let _pending_1 = create_invoice(
        &env,
        &client,
        &business,
        1_000,
        InvoiceCategory::Services,
        false,
    );
    let verified_1 = create_invoice(
        &env,
        &client,
        &business,
        1_100,
        InvoiceCategory::Services,
        true,
    );
    let _pending_2 = create_invoice(
        &env,
        &client,
        &business,
        1_200,
        InvoiceCategory::Products,
        false,
    );
    let verified_2 = create_invoice(
        &env,
        &client,
        &business,
        1_300,
        InvoiceCategory::Products,
        true,
    );

    let verified = client.get_business_invoices_paged(
        &business,
        &Some(InvoiceStatus::Verified),
        &0u32,
        &10u32,
    );
    assert_eq!(verified.len(), 2);
    assert!(verified.contains(&verified_1));
    assert!(verified.contains(&verified_2));

    let pending = client.get_business_invoices_paged(
        &business,
        &Some(InvoiceStatus::PendingVerification),
        &0u32,
        &10u32,
    );
    assert_eq!(pending.len(), 2);

    let no_results = client.get_business_invoices_paged(
        &business,
        &Some(InvoiceStatus::Rejected),
        &0u32,
        &10u32,
    );
    assert_eq!(no_results.len(), 0);

    let offset_at_len = client.get_business_invoices_paged(
        &business,
        &Some(InvoiceStatus::Verified),
        &2u32,
        &10u32,
    );
    assert_eq!(offset_at_len.len(), 0);

    let limit_zero = client.get_business_invoices_paged(
        &business,
        &Some(InvoiceStatus::Verified),
        &0u32,
        &0u32,
    );
    assert_eq!(limit_zero.len(), 0);

    let overflow_safe = client.get_business_invoices_paged(
        &business,
        &Some(InvoiceStatus::Verified),
        &u32::MAX,
        &10u32,
    );
    assert_eq!(overflow_safe.len(), 0);
}

#[test]
fn test_get_available_invoices_paged_filter_combinations_and_overflow_safety() {
    let (env, client) = setup();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let _ = client.set_admin(&admin);
    let business = Address::generate(&env);

    let _id1 = create_invoice(
        &env,
        &client,
        &business,
        500,
        InvoiceCategory::Services,
        true,
    );
    let id2 = create_invoice(
        &env,
        &client,
        &business,
        1_500,
        InvoiceCategory::Services,
        true,
    );
    let _id3 = create_invoice(
        &env,
        &client,
        &business,
        2_500,
        InvoiceCategory::Products,
        true,
    );
    let id4 = create_invoice(
        &env,
        &client,
        &business,
        2_000,
        InvoiceCategory::Services,
        true,
    );
    let _pending = create_invoice(
        &env,
        &client,
        &business,
        1_800,
        InvoiceCategory::Services,
        false,
    );

    let combined = client.get_available_invoices_paged(
        &Some(1_000i128),
        &Some(2_200i128),
        &Some(InvoiceCategory::Services),
        &0u32,
        &10u32,
    );
    assert_eq!(combined.len(), 2);
    assert!(combined.contains(&id2));
    assert!(combined.contains(&id4));

    let combined_offset_len = client.get_available_invoices_paged(
        &Some(1_000i128),
        &Some(2_200i128),
        &Some(InvoiceCategory::Services),
        &2u32,
        &10u32,
    );
    assert_eq!(combined_offset_len.len(), 0);

    let combined_limit_zero = client.get_available_invoices_paged(
        &Some(1_000i128),
        &Some(2_200i128),
        &Some(InvoiceCategory::Services),
        &0u32,
        &0u32,
    );
    assert_eq!(combined_limit_zero.len(), 0);

    let overflow_safe = client.get_available_invoices_paged(
        &Some(1_000i128),
        &Some(2_200i128),
        &Some(InvoiceCategory::Services),
        &u32::MAX,
        &10u32,
    );
    assert_eq!(overflow_safe.len(), 0);
}

#[test]
fn test_get_investor_investments_paged_edge_cases_filters_and_overflow_safety() {
    let (env, client) = setup();
    let contract_id = client.address.clone();
    let investor = Address::generate(&env);

    let empty = client.get_investor_investments_paged(
        &investor,
        &Option::<InvestmentStatus>::None,
        &0u32,
        &10u32,
    );
    assert_eq!(empty.len(), 0);

    env.as_contract(&contract_id, || {
        let investment_id_1 = InvestmentStorage::generate_unique_investment_id(&env);
        let investment_id_2 = InvestmentStorage::generate_unique_investment_id(&env);
        let investment_id_3 = InvestmentStorage::generate_unique_investment_id(&env);

        let investment_1 = Investment {
            investment_id: investment_id_1,
            invoice_id: BytesN::from_array(&env, &[1u8; 32]),
            investor: investor.clone(),
            amount: 1_000,
            funded_at: env.ledger().timestamp(),
            status: InvestmentStatus::Active,
            insurance: Vec::new(&env),
        };
        let investment_2 = Investment {
            investment_id: investment_id_2,
            invoice_id: BytesN::from_array(&env, &[2u8; 32]),
            investor: investor.clone(),
            amount: 2_000,
            funded_at: env.ledger().timestamp(),
            status: InvestmentStatus::Completed,
            insurance: Vec::new(&env),
        };
        let investment_3 = Investment {
            investment_id: investment_id_3,
            invoice_id: BytesN::from_array(&env, &[3u8; 32]),
            investor: investor.clone(),
            amount: 3_000,
            funded_at: env.ledger().timestamp(),
            status: InvestmentStatus::Active,
            insurance: Vec::new(&env),
        };

        InvestmentStorage::store_investment(&env, &investment_1);
        InvestmentStorage::store_investment(&env, &investment_2);
        InvestmentStorage::store_investment(&env, &investment_3);
    });

    let active = client.get_investor_investments_paged(
        &investor,
        &Some(InvestmentStatus::Active),
        &0u32,
        &10u32,
    );
    assert_eq!(active.len(), 2);

    let completed = client.get_investor_investments_paged(
        &investor,
        &Some(InvestmentStatus::Completed),
        &0u32,
        &10u32,
    );
    assert_eq!(completed.len(), 1);

    let offset_at_len = client.get_investor_investments_paged(
        &investor,
        &Some(InvestmentStatus::Active),
        &2u32,
        &10u32,
    );
    assert_eq!(offset_at_len.len(), 0);

    let limit_zero = client.get_investor_investments_paged(
        &investor,
        &Option::<InvestmentStatus>::None,
        &0u32,
        &0u32,
    );
    assert_eq!(limit_zero.len(), 0);

    let overflow_safe = client.get_investor_investments_paged(
        &investor,
        &Option::<InvestmentStatus>::None,
        &u32::MAX,
        &10u32,
    );
    assert_eq!(overflow_safe.len(), 0);
}

#[test]
fn test_bid_paged_queries_edge_cases_filters_and_overflow_safety() {
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

    let empty_invoice = client.get_bid_history_paged(
        &invoice_id,
        &Option::<BidStatus>::None,
        &0u32,
        &10u32,
    );
    assert_eq!(empty_invoice.len(), 0);

    let empty_investor = client.get_investor_bids_paged(
        &investor,
        &Option::<BidStatus>::None,
        &0u32,
        &10u32,
    );
    assert_eq!(empty_investor.len(), 0);

    env.as_contract(&contract_id, || {
        let bid_id_1 = BidStorage::generate_unique_bid_id(&env);
        let bid_id_2 = BidStorage::generate_unique_bid_id(&env);
        let bid_id_3 = BidStorage::generate_unique_bid_id(&env);
        let now = env.ledger().timestamp();

        let bid_1 = Bid {
            bid_id: bid_id_1.clone(),
            invoice_id: invoice_id.clone(),
            investor: investor.clone(),
            bid_amount: 1_000,
            expected_return: 1_100,
            timestamp: now,
            status: BidStatus::Placed,
            expiration_timestamp: now.saturating_add(86_400),
        };
        let bid_2 = Bid {
            bid_id: bid_id_2.clone(),
            invoice_id: invoice_id.clone(),
            investor: investor.clone(),
            bid_amount: 1_200,
            expected_return: 1_350,
            timestamp: now,
            status: BidStatus::Accepted,
            expiration_timestamp: now.saturating_add(86_400),
        };
        let bid_3 = Bid {
            bid_id: bid_id_3.clone(),
            invoice_id: invoice_id.clone(),
            investor: investor.clone(),
            bid_amount: 1_300,
            expected_return: 1_450,
            timestamp: now,
            status: BidStatus::Cancelled,
            expiration_timestamp: now.saturating_add(86_400),
        };

        BidStorage::store_bid(&env, &bid_1);
        BidStorage::store_bid(&env, &bid_2);
        BidStorage::store_bid(&env, &bid_3);
        BidStorage::add_bid_to_invoice(&env, &invoice_id, &bid_id_1);
        BidStorage::add_bid_to_invoice(&env, &invoice_id, &bid_id_2);
        BidStorage::add_bid_to_invoice(&env, &invoice_id, &bid_id_3);
    });

    let accepted_invoice = client.get_bid_history_paged(
        &invoice_id,
        &Some(BidStatus::Accepted),
        &0u32,
        &10u32,
    );
    assert_eq!(accepted_invoice.len(), 1);
    assert_eq!(accepted_invoice.get(0).unwrap().status, BidStatus::Accepted);

    let accepted_investor = client.get_investor_bids_paged(
        &investor,
        &Some(BidStatus::Accepted),
        &0u32,
        &10u32,
    );
    assert_eq!(accepted_investor.len(), 1);
    assert_eq!(accepted_investor.get(0).unwrap().status, BidStatus::Accepted);

    let invoice_offset_at_len = client.get_bid_history_paged(
        &invoice_id,
        &Option::<BidStatus>::None,
        &3u32,
        &10u32,
    );
    assert_eq!(invoice_offset_at_len.len(), 0);

    let investor_offset_at_len = client.get_investor_bids_paged(
        &investor,
        &Option::<BidStatus>::None,
        &3u32,
        &10u32,
    );
    assert_eq!(investor_offset_at_len.len(), 0);

    let invoice_limit_zero = client.get_bid_history_paged(
        &invoice_id,
        &Option::<BidStatus>::None,
        &0u32,
        &0u32,
    );
    assert_eq!(invoice_limit_zero.len(), 0);

    let investor_limit_zero = client.get_investor_bids_paged(
        &investor,
        &Option::<BidStatus>::None,
        &0u32,
        &0u32,
    );
    assert_eq!(investor_limit_zero.len(), 0);

    let invoice_overflow = client.get_bid_history_paged(
        &invoice_id,
        &Option::<BidStatus>::None,
        &u32::MAX,
        &10u32,
    );
    assert_eq!(invoice_overflow.len(), 0);

    let investor_overflow = client.get_investor_bids_paged(
        &investor,
        &Option::<BidStatus>::None,
        &u32::MAX,
        &10u32,
    );
    assert_eq!(investor_overflow.len(), 0);
}
