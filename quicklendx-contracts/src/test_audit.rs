//! Tests for audit trail: log writes, query filters, and integrity validation.
//!
//! Cases: every state change produces correct log entry; query by invoice/actor/op
//! returns correct subset; integrity check passes (and fails when expected).

use super::*;
use crate::audit::{AuditLogEntry, AuditOperation, AuditOperationFilter, AuditQueryFilter, AuditStorage};
use crate::invoice::InvoiceCategory;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, BytesN, Env, String, Vec,
};

fn setup() -> (Env, QuickLendXContractClient<'static>, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let _ = client.initialize_admin(&admin);
    let business = Address::generate(&env);
    
    (env, client, admin, business)
}

#[test]
fn test_audit_invoice_created_and_trail() {
    let (env, client, _admin, business) = setup();
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;
    let invoice_id = client.store_invoice(
        &business,
        &1000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Desc"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    let trail = client.get_invoice_audit_trail(&invoice_id);
    assert!(
        !trail.is_empty(),
        "store_invoice should produce at least one audit entry"
    );
    let entry = client.get_audit_entry(&trail.get(0).unwrap());
    assert_eq!(entry.operation, AuditOperation::InvoiceCreated);
    assert_eq!(entry.actor, business);
    assert_eq!(entry.invoice_id, invoice_id);
}

#[test]
fn test_audit_verify_produces_entry() {
    let (env, client, admin, business) = setup();
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;
    let invoice_id = client.store_invoice(
        &business,
        &1000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Desc"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    let _ = client.verify_invoice(&invoice_id);
    let trail = client.get_invoice_audit_trail(&invoice_id);
    let has_verified = trail
        .iter()
        .any(|id| client.get_audit_entry(&id).operation == AuditOperation::InvoiceVerified);
    assert!(
        has_verified,
        "verify_invoice should produce InvoiceVerified audit entry"
    );
}

#[test]
fn test_audit_query_by_invoice() {
    let (env, client, _admin, business) = setup();
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;
    let inv1 = client.store_invoice(
        &business,
        &1000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "A"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    let inv2 = client.store_invoice(
        &business,
        &2000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "B"),
        &InvoiceCategory::Products,
        &Vec::new(&env),
    );
    let filter = AuditQueryFilter {
        invoice_id: Some(inv1.clone()),
        operation: AuditOperationFilter::Any,
        actor: None,
        start_timestamp: None,
        end_timestamp: None,
    };
    let results = client.query_audit_logs(&filter, &100u32);
    assert!(!results.is_empty());
    for e in results.iter() {
        assert_eq!(
            e.invoice_id, inv1,
            "query by invoice should return only that invoice"
        );
    }
    let trail2 = client.get_invoice_audit_trail(&inv2);
    assert!(!trail2.is_empty());
}

#[test]
fn test_audit_query_by_operation() {
    let (env, client, _admin, business) = setup();
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;
    let _ = client.store_invoice(
        &business,
        &1000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "X"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    let ids = client.get_audit_entries_by_operation(&AuditOperation::InvoiceCreated);
    assert!(
        !ids.is_empty(),
        "should have at least one InvoiceCreated entry"
    );
}

#[test]
fn test_audit_query_by_actor() {
    let (env, client, admin, business) = setup();
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;
    let invoice_id = client.store_invoice(
        &business,
        &1000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "X"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    let _ = client.verify_invoice(&invoice_id);
    let admin_entries = client.get_audit_entries_by_actor(&admin);
    assert!(
        !admin_entries.is_empty(),
        "admin should have at least one audit entry (verify)"
    );
}

#[test]
fn test_audit_query_time_range() {
    let (env, client, _admin, business) = setup();
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;
    let _ = client.store_invoice(
        &business,
        &1000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "X"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    let now = env.ledger().timestamp();
    let filter = AuditQueryFilter {
        invoice_id: None,
        operation: AuditOperationFilter::Any,
        actor: None,
        start_timestamp: Some(now.saturating_sub(3600)),
        end_timestamp: Some(now.saturating_add(3600)),
    };
    let results = client.query_audit_logs(&filter, &10u32);
    assert!(
        !results.is_empty(),
        "recent entries should match time range"
    );
}

#[test]
fn test_audit_query_limit_is_capped_to_max_query_limit() {
    let (env, client, _admin, business) = setup();
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;

    for i in 0..130u32 {
        let _ = client.store_invoice(
            &business,
            &(1_000i128 + i as i128),
            &currency,
            &due_date,
            &String::from_str(&env, "Cap"),
            &InvoiceCategory::Services,
            &Vec::new(&env),
        );
    }

    let filter = AuditQueryFilter {
        invoice_id: None,
        operation: AuditOperationFilter::Specific(AuditOperation::InvoiceCreated),
        actor: Some(business),
        start_timestamp: None,
        end_timestamp: None,
    };

    let results = client.query_audit_logs(&filter, &500u32);
    assert_eq!(
        results.len(),
        crate::MAX_QUERY_LIMIT,
        "audit query should enforce MAX_QUERY_LIMIT cap"
    );
}

#[test]
fn test_audit_integrity_valid() {
    let (env, client, _admin, business) = setup();
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;
    let invoice_id = client.store_invoice(
        &business,
        &1000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "X"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    let valid = client.validate_invoice_audit_integrity(&invoice_id);
    assert!(valid, "valid trail should pass integrity check");
}

#[test]
fn test_audit_integrity_no_invoice() {
    let (env, client, _admin, _business) = setup();
    let fake_id = BytesN::from_array(&env, &[0u8; 32]);
    let valid = client.validate_invoice_audit_integrity(&fake_id);
    assert!(valid, "non-existent invoice has empty trail and passes");
}

#[test]
fn test_audit_stats() {
    let (env, client, _admin, business) = setup();
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;
    let _ = client.store_invoice(
        &business,
        &1000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "X"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    let stats = client.get_audit_stats();
    assert!(stats.total_entries >= 1);
}

#[test]
fn test_audit_stats_empty_state() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let _ = client.initialize_admin(&admin);
    
    let stats = client.get_audit_stats();
    assert_eq!(stats.total_entries, 0, "empty state should have 0 entries");
    assert_eq!(stats.unique_actors, 0, "empty state should have 0 actors");
    assert_eq!(
        stats.date_range.0, u64::MAX,
        "empty state min timestamp should be MAX"
    );
    assert_eq!(
        stats.date_range.1, 0,
        "empty state max timestamp should be 0"
    );
}

#[test]
fn test_audit_stats_total_entries_after_invoice_create() {
    let (env, client, _admin, business) = setup();
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;

    let stats_before = client.get_audit_stats();
    let initial_count = stats_before.total_entries;

    let _ = client.store_invoice(
        &business,
        &1000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Invoice 1"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    let stats_after = client.get_audit_stats();
    assert_eq!(
        stats_after.total_entries,
        initial_count + 1,
        "creating invoice should add 1 audit entry"
    );
}

#[test]
fn test_audit_stats_total_entries_after_verify() {
    let (env, client, _admin, business) = setup();
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;

    let invoice_id = client.store_invoice(
        &business,
        &1000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    let stats_before = client.get_audit_stats();
    let count_before = stats_before.total_entries;

    let _ = client.verify_invoice(&invoice_id);

    let stats_after = client.get_audit_stats();
    assert_eq!(
        stats_after.total_entries,
        count_before + 2,
        "verifying invoice should add 2 audit entries"
    );
}

#[test]
fn test_audit_stats_total_entries_after_bid() {
    let (env, client, _admin, business) = setup();
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;
    let investor = Address::generate(&env);

    let invoice_id = client.store_invoice(
        &business,
        &1000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    let _ = client.verify_invoice(&invoice_id);

    let stats_before = client.get_audit_stats();
    let count_before = stats_before.total_entries;

    let _ = client.place_bid(&investor, &invoice_id, &900i128, &950i128);

    let stats_after = client.get_audit_stats();
    assert_eq!(
        stats_after.total_entries,
        count_before + 1,
        "placing bid should add 1 audit entry"
    );
}

#[test]
fn test_audit_stats_total_entries_after_escrow() {
    let (env, client, _admin, business) = setup();
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;
    let investor = Address::generate(&env);

    let invoice_id = client.store_invoice(
        &business,
        &1000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    let _ = client.verify_invoice(&invoice_id);
    let bid_id = client.place_bid(&investor, &invoice_id, &900i128, &950i128);

    let stats_before = client.get_audit_stats();
    let count_before = stats_before.total_entries;

    let _ = client.accept_bid(&invoice_id, &bid_id);

    let stats_after = client.get_audit_stats();
    assert!(
        stats_after.total_entries > count_before,
        "accepting bid should add audit entries (bid accepted + escrow created)"
    );
}

#[test]
fn test_audit_stats_multiple_operations() {
    let (env, client, _admin, business) = setup();
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;

    let stats_before = client.get_audit_stats();
    let initial_count = stats_before.total_entries;

    // Create multiple invoices (each creates 1 entry)
    let _ = client.store_invoice(
        &business,
        &1000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Invoice 1"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    let _ = client.store_invoice(
        &business,
        &2000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Invoice 2"),
        &InvoiceCategory::Products,
        &Vec::new(&env),
    );
    let invoice_id3 = client.store_invoice(
        &business,
        &3000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Invoice 3"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    // Verify one (adds 2 entries)
    let _ = client.verify_invoice(&invoice_id3);

    let stats = client.get_audit_stats();
    assert_eq!(
        stats.total_entries,
        initial_count + 5,
        "3 creates (3 entries) + 1 verify (2 entries) = 5 entries"
    );
}

#[test]
fn test_audit_stats_unique_actors_single() {
    let (env, client, _admin, business) = setup();
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;

    let _ = client.store_invoice(
        &business,
        &1000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    let stats = client.get_audit_stats();
    assert_eq!(
        stats.unique_actors, 1,
        "single business should result in 1 unique actor"
    );
}

#[test]
fn test_audit_stats_unique_actors_multiple() {
    let (env, client, _admin, business) = setup();
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;
    let investor = Address::generate(&env);

    let invoice_id = client.store_invoice(
        &business,
        &1000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    let _ = client.verify_invoice(&invoice_id);
    let _ = client.place_bid(&investor, &invoice_id, &900i128, &950i128);

    let stats = client.get_audit_stats();
    assert_eq!(
        stats.unique_actors, 3,
        "business + admin + investor = 3 unique actors"
    );
}

#[test]
fn test_audit_stats_unique_actors_duplicate_operations() {
    let (env, client, _admin, business) = setup();
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;

    // Same business creates multiple invoices
    let _ = client.store_invoice(
        &business,
        &1000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Invoice 1"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    let _ = client.store_invoice(
        &business,
        &2000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Invoice 2"),
        &InvoiceCategory::Products,
        &Vec::new(&env),
    );

    let stats = client.get_audit_stats();
    assert_eq!(
        stats.unique_actors, 1,
        "same actor multiple times should count as 1 unique actor"
    );
}

#[test]
fn test_audit_stats_date_range_single_entry() {
    let (env, client, _admin, business) = setup();
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;

    let timestamp_before = env.ledger().timestamp();

    let _ = client.store_invoice(
        &business,
        &1000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    let stats = client.get_audit_stats();
    assert!(
        stats.date_range.0 >= timestamp_before,
        "min timestamp should be >= operation time"
    );
    assert!(
        stats.date_range.1 >= timestamp_before,
        "max timestamp should be >= operation time"
    );
    assert_eq!(
        stats.date_range.0, stats.date_range.1,
        "single entry should have same min and max timestamp"
    );
}

#[test]
fn test_audit_stats_date_range_multiple_entries() {
    let (env, client, _admin, business) = setup();
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;

    let _ = client.store_invoice(
        &business,
        &1000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Invoice 1"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    // Advance time
    env.ledger().with_mut(|li| {
        li.timestamp = li.timestamp.saturating_add(3600);
    });

    let new_due_date = env.ledger().timestamp() + 86400;
    let _ = client.store_invoice(
        &business,
        &2000i128,
        &currency,
        &new_due_date,
        &String::from_str(&env, "Invoice 2"),
        &InvoiceCategory::Products,
        &Vec::new(&env),
    );

    let stats = client.get_audit_stats();
    assert!(
        stats.date_range.1 > stats.date_range.0,
        "max timestamp should be greater than min with time progression"
    );
    assert_eq!(
        stats.date_range.1 - stats.date_range.0,
        3600,
        "date range should reflect 1 hour difference"
    );
}

#[test]
fn test_audit_stats_comprehensive_workflow() {
    let (env, client, _admin, business) = setup();
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;
    let investor1 = Address::generate(&env);
    let investor2 = Address::generate(&env);

    // Create invoice
    let invoice_id = client.store_invoice(
        &business,
        &1000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    // Verify
    let _ = client.verify_invoice(&invoice_id);

    // Place bids
    let _ = client.place_bid(&investor1, &invoice_id, &900i128, &950i128);
    let bid_id2 = client.place_bid(&investor2, &invoice_id, &850i128, &900i128);

    // Accept bid (creates escrow)
    let _ = client.accept_bid(&invoice_id, &bid_id2);

    let stats = client.get_audit_stats();

    // Should have: 1 create + 1 verify + 2 bids + 1 accept + 1 escrow = 6 entries
    assert!(
        stats.total_entries >= 6,
        "comprehensive workflow should produce at least 6 audit entries"
    );

    // Should have: business + admin + investor1 + investor2 = 4 unique actors
    assert_eq!(
        stats.unique_actors, 4,
        "should have 4 unique actors in workflow"
    );

    // Date range should be valid
    assert!(
        stats.date_range.1 >= stats.date_range.0,
        "max timestamp should be >= min timestamp"
    );
}

#[test]
fn test_audit_stats_after_bid_withdrawal() {
    let (env, client, _admin, business) = setup();
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;
    let investor = Address::generate(&env);

    let invoice_id = client.store_invoice(
        &business,
        &1000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    let _ = client.verify_invoice(&invoice_id);
    let bid_id = client.place_bid(&investor, &invoice_id, &900i128, &950i128);

    let stats_before = client.get_audit_stats();
    let count_before = stats_before.total_entries;

    let _ = client.withdraw_bid(&bid_id);

    let stats_after = client.get_audit_stats();
    assert_eq!(
        stats_after.total_entries,
        count_before + 1,
        "withdrawing bid should add 1 audit entry"
    );
}

#[test]
fn test_audit_stats_incremental_updates() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let _ = client.initialize_admin(&admin);
    let business = Address::generate(&env);
    
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;

    let stats0 = client.get_audit_stats();
    let initial = stats0.total_entries;

    let _ = client.store_invoice(
        &business,
        &1000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Invoice 1"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    let stats1 = client.get_audit_stats();
    assert_eq!(stats1.total_entries, initial + 1); // 1 entry per invoice

    let _ = client.store_invoice(
        &business,
        &2000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Invoice 2"),
        &InvoiceCategory::Products,
        &Vec::new(&env),
    );
    let stats2 = client.get_audit_stats();
    assert_eq!(stats2.total_entries, initial + 2); // 2 invoices

    let invoice_id3 = client.store_invoice(
        &business,
        &3000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Invoice 3"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    let stats3 = client.get_audit_stats();
    assert_eq!(stats3.total_entries, initial + 3); // 3 invoices

    let _ = client.verify_invoice(&invoice_id3);
    let stats4 = client.get_audit_stats();
    assert_eq!(stats4.total_entries, initial + 5); // 3 + 2 verify
}

#[test]
fn test_audit_stats_operations_count_structure() {
    let (env, client, _admin, business) = setup();
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;

    let _ = client.store_invoice(
        &business,
        &1000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    let stats = client.get_audit_stats();
    // operations_count is currently empty in implementation, but structure should exist
    assert!(
        stats.operations_count.len() == 0,
        "operations_count is currently not populated but should be valid Vec"
    );
}

#[test]
fn test_audit_stats_consistency_across_calls() {
    let (env, client, _admin, business) = setup();
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;

    let _ = client.store_invoice(
        &business,
        &1000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    let stats1 = client.get_audit_stats();
    let stats2 = client.get_audit_stats();

    assert_eq!(
        stats1.total_entries, stats2.total_entries,
        "consecutive calls should return same total"
    );
    assert_eq!(
        stats1.unique_actors, stats2.unique_actors,
        "consecutive calls should return same unique actors"
    );
    assert_eq!(
        stats1.date_range, stats2.date_range,
        "consecutive calls should return same date range"
    );
}

#[test]
#[should_panic]
fn test_audit_get_entry_not_found() {
    let (env, client, _admin, _business) = setup();
    let fake_id = BytesN::from_array(&env, &[0u8; 32]);
    let _ = client.get_audit_entry(&fake_id);
}

#[test]
fn test_query_audit_logs_operation_actor_time_combinations_and_limits() {
    let (env, client, admin, business) = setup();
    let business2 = Address::generate(&env);
    let currency = Address::generate(&env);

    let t0 = env.ledger().timestamp();
    let due_date = t0 + 86400;

    let inv1 = client.store_invoice(
        &business,
        &1000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "inv1"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    env.ledger().set_timestamp(t0 + 10);
    let _ = client.verify_invoice(&inv1);

    env.ledger().set_timestamp(t0 + 20);
    let _inv2 = client.store_invoice(
        &business,
        &2000i128,
        &currency,
        &(t0 + 20 + 86400),
        &String::from_str(&env, "inv2"),
        &InvoiceCategory::Products,
        &Vec::new(&env),
    );

    env.ledger().set_timestamp(t0 + 30);
    let _inv3 = client.store_invoice(
        &business2,
        &3000i128,
        &currency,
        &(t0 + 30 + 86400),
        &String::from_str(&env, "inv3"),
        &InvoiceCategory::Products,
        &Vec::new(&env),
    );

    // operation only (non-empty)
    let op_only = AuditQueryFilter {
        invoice_id: None,
        operation: AuditOperationFilter::Specific(AuditOperation::InvoiceCreated),
        actor: None,
        start_timestamp: None,
        end_timestamp: None,
    };
    let op_only_results = client.query_audit_logs(&op_only, &100u32);
    assert_eq!(op_only_results.len(), 3);
    for e in op_only_results.iter() {
        assert_eq!(e.operation, AuditOperation::InvoiceCreated);
    }

    // actor only (non-empty)
    let actor_only = AuditQueryFilter {
        invoice_id: None,
        operation: AuditOperationFilter::Any,
        actor: Some(business.clone()),
        start_timestamp: None,
        end_timestamp: None,
    };
    let actor_only_results = client.query_audit_logs(&actor_only, &100u32);
    assert_eq!(actor_only_results.len(), 2);
    for e in actor_only_results.iter() {
        assert_eq!(e.actor, business);
    }

    // time range only (non-empty)
    let time_only = AuditQueryFilter {
        invoice_id: None,
        operation: AuditOperationFilter::Any,
        actor: None,
        start_timestamp: Some(t0 + 5),
        end_timestamp: Some(t0 + 15),
    };
    let time_only_results = client.query_audit_logs(&time_only, &100u32);
    assert!(
        !time_only_results.is_empty(),
        "time-only filter should return entries in-range"
    );
    assert!(
        time_only_results
            .iter()
            .any(|e| e.operation == AuditOperation::InvoiceVerified),
        "time-only results should include verification entry"
    );

    // combination: operation + actor (non-empty)
    let op_actor = AuditQueryFilter {
        invoice_id: None,
        operation: AuditOperationFilter::Specific(AuditOperation::InvoiceCreated),
        actor: Some(business.clone()),
        start_timestamp: None,
        end_timestamp: None,
    };
    let op_actor_results = client.query_audit_logs(&op_actor, &100u32);
    assert_eq!(op_actor_results.len(), 2);

    // combination: operation + actor + time (non-empty)
    let op_actor_time = AuditQueryFilter {
        invoice_id: None,
        operation: AuditOperationFilter::Specific(AuditOperation::InvoiceVerified),
        actor: Some(admin.clone()),
        start_timestamp: Some(t0 + 5),
        end_timestamp: Some(t0 + 15),
    };
    let op_actor_time_results = client.query_audit_logs(&op_actor_time, &100u32);
    assert_eq!(op_actor_time_results.len(), 1);

    // combination: operation + actor (empty)
    let empty_op_actor = AuditQueryFilter {
        invoice_id: None,
        operation: AuditOperationFilter::Specific(AuditOperation::InvoiceVerified),
        actor: Some(business.clone()),
        start_timestamp: None,
        end_timestamp: None,
    };
    assert_eq!(client.query_audit_logs(&empty_op_actor, &100u32).len(), 0);

    // time range only (empty)
    let empty_time = AuditQueryFilter {
        invoice_id: None,
        operation: AuditOperationFilter::Any,
        actor: None,
        start_timestamp: Some(t0 + 100),
        end_timestamp: Some(t0 + 200),
    };
    assert_eq!(client.query_audit_logs(&empty_time, &100u32).len(), 0);

    // limit edges: 0, 1, 100
    assert_eq!(client.query_audit_logs(&op_actor, &0u32).len(), 0);
    assert_eq!(client.query_audit_logs(&op_actor, &1u32).len(), 1);
    assert_eq!(client.query_audit_logs(&op_actor, &100u32).len(), 2);
}

#[test]
fn test_get_audit_entries_by_operation_each_type_empty_and_non_empty() {
    let (env, client, admin, business) = setup();
    let investor = Address::generate(&env);
    let contract_id = client.address.clone();

    // Empty cases before any entry is stored
    assert_eq!(
        client
            .get_audit_entries_by_operation(&AuditOperation::InvoiceCreated)
            .len(),
        0
    );
    assert_eq!(
        client
            .get_audit_entries_by_operation(&AuditOperation::SettlementCompleted)
            .len(),
        0
    );

    let operations = [
        AuditOperation::InvoiceCreated,
        AuditOperation::InvoiceUploaded,
        AuditOperation::InvoiceVerified,
        AuditOperation::InvoiceFunded,
        AuditOperation::InvoicePaid,
        AuditOperation::InvoiceDefaulted,
        AuditOperation::InvoiceStatusChanged,
        AuditOperation::InvoiceRated,
        AuditOperation::BidPlaced,
        AuditOperation::BidAccepted,
        AuditOperation::BidWithdrawn,
        AuditOperation::EscrowCreated,
        AuditOperation::EscrowReleased,
        AuditOperation::EscrowRefunded,
        AuditOperation::PaymentProcessed,
        AuditOperation::SettlementCompleted,
    ];

    for (idx, operation) in operations.iter().enumerate() {
        let mut id_bytes = [0u8; 32];
        id_bytes[0] = (idx as u8).saturating_add(1);
        let invoice_id = BytesN::from_array(&env, &id_bytes);

        let actor = match idx % 3 {
            0 => business.clone(),
            1 => investor.clone(),
            _ => admin.clone(),
        };

        env.as_contract(&contract_id, || {
            let entry = AuditLogEntry::new(
                &env,
                invoice_id,
                operation.clone(),
                actor,
                None,
                None,
                None,
                None,
            );
            AuditStorage::store_audit_entry(&env, &entry);
        });
    }

    // Add one extra InvoiceCreated entry to cover multiple entries for one operation.
    let mut extra_id_bytes = [0u8; 32];
    extra_id_bytes[0] = 250;
    let extra_invoice_id = BytesN::from_array(&env, &extra_id_bytes);
    env.as_contract(&contract_id, || {
        let entry = AuditLogEntry::new(
            &env,
            extra_invoice_id,
            AuditOperation::InvoiceCreated,
            business.clone(),
            None,
            None,
            None,
            None,
        );
        AuditStorage::store_audit_entry(&env, &entry);
    });

    for operation in operations.iter() {
        let ids = client.get_audit_entries_by_operation(operation);
        let expected_len = if *operation == AuditOperation::InvoiceCreated {
            2
        } else {
            1
        };
        assert_eq!(ids.len(), expected_len, "unexpected operation index size");
        for id in ids.iter() {
            let entry = client.get_audit_entry(&id);
            assert_eq!(entry.operation, *operation);
        }
    }
}

#[test]
fn test_get_audit_entries_by_actor_business_investor_admin_empty_and_multiple() {
    let (env, client, admin, business) = setup();
    let investor = Address::generate(&env);
    let contract_id = client.address.clone();

    let add_entry = |env: &Env, contract_id: &Address, invoice_seed: u8, operation: AuditOperation, actor: Address| {
        let mut id_bytes = [0u8; 32];
        id_bytes[0] = invoice_seed;
        let invoice_id = BytesN::from_array(env, &id_bytes);
        env.as_contract(contract_id, || {
            let entry = AuditLogEntry::new(
                env,
                invoice_id,
                operation,
                actor,
                None,
                None,
                None,
                None,
            );
            AuditStorage::store_audit_entry(env, &entry);
        });
    };

    // Multiple for business and investor, single for admin.
    add_entry(
        &env,
        &contract_id,
        1,
        AuditOperation::InvoiceCreated,
        business.clone(),
    );
    add_entry(
        &env,
        &contract_id,
        2,
        AuditOperation::InvoiceUploaded,
        business.clone(),
    );
    add_entry(
        &env,
        &contract_id,
        3,
        AuditOperation::BidPlaced,
        investor.clone(),
    );
    add_entry(
        &env,
        &contract_id,
        4,
        AuditOperation::InvoiceFunded,
        investor.clone(),
    );
    add_entry(
        &env,
        &contract_id,
        5,
        AuditOperation::InvoiceVerified,
        admin.clone(),
    );

    let business_ids = client.get_audit_entries_by_actor(&business);
    assert_eq!(business_ids.len(), 2);
    for id in business_ids.iter() {
        let entry = client.get_audit_entry(&id);
        assert_eq!(entry.actor, business);
    }

    let investor_ids = client.get_audit_entries_by_actor(&investor);
    assert_eq!(investor_ids.len(), 2);
    for id in investor_ids.iter() {
        let entry = client.get_audit_entry(&id);
        assert_eq!(entry.actor, investor);
    }

    let admin_ids = client.get_audit_entries_by_actor(&admin);
    assert_eq!(admin_ids.len(), 1);
    let admin_entry = client.get_audit_entry(&admin_ids.get(0).unwrap());
    assert_eq!(admin_entry.actor, admin);

    // Empty case
    let unknown = Address::generate(&env);
    assert_eq!(client.get_audit_entries_by_actor(&unknown).len(), 0);
}
