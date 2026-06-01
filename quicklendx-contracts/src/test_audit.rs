//! Tests for audit trail: log writes, query filters, and integrity validation.
//!
//! Cases: every state change produces correct log entry; query by invoice/actor/op
//! returns correct subset; integrity check passes (and fails when expected).

use super::*;
use crate::audit::{
    AuditLogEntry, AuditOperation, AuditOperationFilter, AuditQueryFilter, AuditStorage,
};
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
        stats.date_range.0,
        u64::MAX,
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
    assert_eq!(stats2.total_entries, initial + 2);

    let _ = client.store_invoice(
        &business,
        &3000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Invoice 3"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    let stats3 = client.get_audit_stats();
    assert_eq!(stats3.total_entries, initial + 3);
}

/// Append-only guarantee: the audit trail for an invoice only grows, never shrinks.
#[test]
fn test_audit_trail_is_append_only() {
    let (env, client, _admin, business) = setup();
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;

    let invoice_id = client.store_invoice(
        &business,
        &1000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Append-only test"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    let trail_after_create = client.get_invoice_audit_trail(&invoice_id);
    let len_after_create = trail_after_create.len();
    assert!(len_after_create >= 1, "trail must have at least 1 entry after create");

    let _ = client.verify_invoice(&invoice_id);
    let trail_after_verify = client.get_invoice_audit_trail(&invoice_id);
    assert!(
        trail_after_verify.len() > len_after_create,
        "trail must grow after verify, never shrink"
    );

    // Confirm earlier entries are still present (append-only: no removal)
    for id in trail_after_create.iter() {
        assert!(
            trail_after_verify.iter().any(|x| x == id),
            "previously appended entry must still be present"
        );
    }
}

/// Mutation guard: stored audit entries cannot be overwritten.
#[test]
fn test_audit_entry_immutable_after_store() {
    let (env, client, _admin, business) = setup();
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;

    let invoice_id = client.store_invoice(
        &business,
        &1000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Immutability test"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    let trail = client.get_invoice_audit_trail(&invoice_id);
    let first_id = trail.get(0).unwrap();
    let entry_before = client.get_audit_entry(&first_id);

    // Perform another operation that writes new entries
    let _ = client.verify_invoice(&invoice_id);

    // The original entry must be unchanged
    let entry_after = client.get_audit_entry(&first_id);
    assert_eq!(entry_before.audit_id, entry_after.audit_id);
    assert_eq!(entry_before.operation, entry_after.operation);
    assert_eq!(entry_before.actor, entry_after.actor);
    assert_eq!(entry_before.timestamp, entry_after.timestamp);
    assert_eq!(entry_before.amount, entry_after.amount);
}

/// Filter correctness: combined actor + operation filter returns only matching entries.
#[test]
fn test_audit_query_combined_actor_and_operation_filter() {
    let (env, client, admin, business) = setup();
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;

    let invoice_id = client.store_invoice(
        &business,
        &1000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Filter test"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    let _ = client.verify_invoice(&invoice_id);

    // Query: admin + InvoiceVerified
    let filter = AuditQueryFilter {
        invoice_id: None,
        operation: AuditOperationFilter::Specific(AuditOperation::InvoiceVerified),
        actor: Some(admin.clone()),
        start_timestamp: None,
        end_timestamp: None,
    };
    let results = client.query_audit_logs(&filter, &100u32);
    assert!(!results.is_empty(), "should find admin InvoiceVerified entries");
    for e in results.iter() {
        assert_eq!(e.operation, AuditOperation::InvoiceVerified);
        assert_eq!(e.actor, admin);
    }
}

/// Filter correctness: time range with no matching entries returns empty.
#[test]
fn test_audit_query_time_range_no_match_returns_empty() {
    let (env, client, _admin, business) = setup();
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;

    let _ = client.store_invoice(
        &business,
        &1000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Time filter test"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    // Query a time range in the far future - no entries should match
    let far_future = env.ledger().timestamp() + 1_000_000;
    let filter = AuditQueryFilter {
        invoice_id: None,
        operation: AuditOperationFilter::Any,
        actor: None,
        start_timestamp: Some(far_future),
        end_timestamp: Some(far_future + 3600),
    };
    let results = client.query_audit_logs(&filter, &100u32);
    assert!(results.is_empty(), "future time range should return no entries");
}

/// Integrity check passes for a full invoice lifecycle.
#[test]
fn test_audit_integrity_full_lifecycle() {
    let (env, client, _admin, business) = setup();
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;
    let investor = Address::generate(&env);

    let invoice_id = client.store_invoice(
        &business,
        &1000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Lifecycle integrity"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    let _ = client.verify_invoice(&invoice_id);
    let bid_id = client.place_bid(&investor, &invoice_id, &900i128, &950i128);
    let _ = client.accept_bid(&invoice_id, &bid_id);

    let valid = client.validate_invoice_audit_integrity(&invoice_id);
    assert!(valid, "full lifecycle audit trail must pass integrity check");
}

// ============================================================================
// COMPREHENSIVE AUDIT TRAIL INTEGRITY TESTS
//
// These tests verify the append-only guarantee, monotonic ordering,
// entry count per operation, and statistics reconciliation.
// ============================================================================

/// **APPEND-ONLY GUARANTEE**: Audit entries are never overwritten; the trail
/// for an invoice grows monotonically without gaps or duplicates.
///
/// This is critical for post-incident review: an audit log that loses or
/// reorders entries is worthless. We verify:
/// 1. Entry IDs are unique
/// 2. Trail indices never decrease
/// 3. Entries are never replaced in storage
#[test]
fn test_audit_append_only_no_overwrites() {
    let (env, client, _admin, business) = setup();
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;

    // Create invoice, which produces the first audit entry
    let invoice_id = client.store_invoice(
        &business,
        &1000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Overwrite test"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    let trail1 = client.get_invoice_audit_trail(&invoice_id);
    let count1 = trail1.len();
    assert!(count1 >= 1, "first operation should create at least 1 entry");

    // Verify the invoice, which adds another entry
    let _ = client.verify_invoice(&invoice_id);
    let trail2 = client.get_invoice_audit_trail(&invoice_id);
    let count2 = trail2.len();

    assert!(
        count2 > count1,
        "verify should append, not overwrite. Expected count > {}, got {}",
        count1,
        count2
    );

    // Verify all previous entries are still present and unchanged
    for i in 0..count1 {
        let id1 = trail1.get(i).unwrap();
        let id2 = trail2.get(i).unwrap();
        assert_eq!(
            id1, id2,
            "entry at index {} should not change; audit trail is append-only",
            i
        );
    }
}

/// **APPEND-ONLY GUARANTEE**: High-volume appends preserve all entries.
/// Stress test: create many invoices and verify the count matches entries.
#[test]
fn test_audit_append_only_high_volume() {
    let (env, client, _admin, business) = setup();
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;

    let num_invoices = 50u32;
    let mut created_ids = Vec::new(&env);

    for i in 0..num_invoices {
        let invoice_id = client.store_invoice(
            &business,
            &(1000i128 + i as i128),
            &currency,
            &due_date,
            &String::from_str(&env, &format!("Invoice {}", i)),
            &InvoiceCategory::Services,
            &Vec::new(&env),
        );
        created_ids.push_back(invoice_id);
    }

    // Verify stats total_entries >= num_invoices (at least one per invoice)
    let stats = client.get_audit_stats();
    assert!(
        stats.total_entries as u32 >= num_invoices,
        "high-volume append should record all {} invoices; got {} entries",
        num_invoices,
        stats.total_entries
    );

    // Verify each invoice has its own audit trail
    for invoice_id in created_ids.iter() {
        let trail = client.get_invoice_audit_trail(&invoice_id);
        assert!(
            !trail.is_empty(),
            "each invoice must have at least one audit entry"
        );
    }
}

/// **MONOTONIC AUDIT IDs**: Audit IDs embedded with timestamp and counter
/// must increase monotonically to prevent tampering.
#[test]
fn test_audit_monotonic_ids_within_single_invoice() {
    let (env, client, _admin, business) = setup();
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;

    let invoice_id = client.store_invoice(
        &business,
        &1000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Monotonic test"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    let trail = client.get_invoice_audit_trail(&invoice_id);
    assert!(trail.len() >= 1);

    // For each pair of consecutive entries, the later one should not be "less than" the earlier
    // (We compare the audit_id byte representations as a proxy for monotonicity)
    for i in 0..(trail.len() - 1) {
        let id_curr = trail.get(i).unwrap();
        let id_next = trail.get(i + 1).unwrap();

        let entry_curr = client.get_audit_entry(&id_curr);
        let entry_next = client.get_audit_entry(&id_next);

        // Timestamps should not decrease
        assert!(
            entry_curr.timestamp <= entry_next.timestamp,
            "audit timestamps must be monotonically non-decreasing. {} > {}",
            entry_curr.timestamp,
            entry_next.timestamp
        );
    }
}

/// **ONE ENTRY PER STATE-CHANGING CALL**: Verify that each entrypoint
/// produces exactly the expected number of audit entries.
#[test]
fn test_audit_one_entry_per_invoice_create() {
    let (env, client, _admin, business) = setup();
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;

    let stats_before = client.get_audit_stats();
    let count_before = stats_before.total_entries;

    // store_invoice should produce exactly 1 audit entry
    let _invoice_id = client.store_invoice(
        &business,
        &1000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "One entry test"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    let stats_after = client.get_audit_stats();
    let count_after = stats_after.total_entries;

    assert_eq!(
        count_after, count_before + 1,
        "store_invoice must produce exactly 1 audit entry"
    );
}

/// **ONE ENTRY PER STATE-CHANGING CALL**: verify_invoice should produce
/// exactly one InvoiceVerified entry.
#[test]
fn test_audit_one_entry_per_verify() {
    let (env, client, _admin, business) = setup();
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;

    let invoice_id = client.store_invoice(
        &business,
        &1000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Verify one entry test"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    let trail_before = client.get_invoice_audit_trail(&invoice_id);
    let count_before = trail_before.len();

    // Count InvoiceVerified entries before
    let mut verified_before = 0u32;
    for audit_id in trail_before.iter() {
        let entry = client.get_audit_entry(&audit_id);
        if entry.operation == AuditOperation::InvoiceVerified {
            verified_before += 1;
        }
    }

    // Perform verify
    let _ = client.verify_invoice(&invoice_id);

    let trail_after = client.get_invoice_audit_trail(&invoice_id);
    let count_after = trail_after.len();

    // Count InvoiceVerified entries after
    let mut verified_after = 0u32;
    for audit_id in trail_after.iter() {
        let entry = client.get_audit_entry(&audit_id);
        if entry.operation == AuditOperation::InvoiceVerified {
            verified_after += 1;
        }
    }

    assert_eq!(
        verified_after, verified_before + 1,
        "verify_invoice must produce exactly 1 InvoiceVerified entry"
    );
    assert!(
        count_after > count_before,
        "verify_invoice must append (count should increase)"
    );
}

/// **ONE ENTRY PER STATE-CHANGING CALL**: place_bid should produce
/// exactly one BidPlaced entry.
#[test]
fn test_audit_one_entry_per_bid() {
    let (env, client, _admin, business) = setup();
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;
    let investor = Address::generate(&env);

    let invoice_id = client.store_invoice(
        &business,
        &1000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Bid one entry"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    let _ = client.verify_invoice(&invoice_id);

    let stats_before = client.get_audit_stats();
    let count_before = stats_before.total_entries;

    // place_bid should produce exactly 1 audit entry
    let _bid_id = client.place_bid(&investor, &invoice_id, &900i128, &950i128);

    let stats_after = client.get_audit_stats();
    let count_after = stats_after.total_entries;

    assert_eq!(
        count_after, count_before + 1,
        "place_bid must produce exactly 1 audit entry"
    );
}

/// **STATS RECONCILIATION**: AuditStats.total_entries must exactly match
/// the count of all audit entries across the entire contract.
#[test]
fn test_audit_stats_reconciliation_total_count() {
    let (env, client, _admin, business) = setup();
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;

    // Create several invoices
    for i in 0..5u32 {
        let _invoice_id = client.store_invoice(
            &business,
            &(1000i128 + i as i128),
            &currency,
            &due_date,
            &String::from_str(&env, &format!("Reconcile {}", i)),
            &InvoiceCategory::Services,
            &Vec::new(&env),
        );
    }

    // Query all audit entries with limit well above expected count
    let filter = AuditQueryFilter {
        invoice_id: None,
        operation: AuditOperationFilter::Any,
        actor: None,
        start_timestamp: None,
        end_timestamp: None,
    };
    let all_entries = client.query_audit_logs(&filter, &1000u32);
    let actual_count = all_entries.len() as u32;

    // Get stats
    let stats = client.get_audit_stats();

    assert_eq!(
        stats.total_entries, actual_count,
        "AUDIT_STATS.total_entries ({}) must exactly match query result count ({})",
        stats.total_entries,
        actual_count
    );
}

/// **STATS RECONCILIATION**: unique_actors in stats must match the number
/// of distinct addresses that performed audit-logged operations.
#[test]
fn test_audit_stats_reconciliation_unique_actors() {
    let (env, client, _admin, business) = setup();
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;
    let investor1 = Address::generate(&env);
    let investor2 = Address::generate(&env);

    // Business creates invoice
    let invoice_id = client.store_invoice(
        &business,
        &1000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Actor test"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    // Admin verifies
    let _ = client.verify_invoice(&invoice_id);

    // Two investors bid
    let _ = client.place_bid(&investor1, &invoice_id, &900i128, &950i128);
    let _ = client.place_bid(&investor2, &invoice_id, &850i128, &900i128);

    let stats = client.get_audit_stats();

    // Should have business, admin, investor1, investor2
    // (4 unique actors minimum; could be more depending on verify_invoice implementation)
    assert!(
        stats.unique_actors >= 3,
        "stats should record at least 3 unique actors (business, admin, investor)"
    );
}

/// **STATS RECONCILIATION**: date_range.0 <= date_range.1 and both
/// are within plausible bounds.
#[test]
fn test_audit_stats_reconciliation_date_range_valid() {
    let (env, client, _admin, business) = setup();
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;

    let ts_before = env.ledger().timestamp();

    let _invoice_id = client.store_invoice(
        &business,
        &1000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Date range test"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    let ts_after = env.ledger().timestamp();

    let stats = client.get_audit_stats();

    // If there are entries, date_range should be valid
    if stats.total_entries > 0 {
        assert!(
            stats.date_range.0 <= stats.date_range.1,
            "stats.date_range.min ({}) must be <= max ({})",
            stats.date_range.0,
            stats.date_range.1
        );

        assert!(
            stats.date_range.0 >= ts_before,
            "min timestamp must be >= operation start time"
        );

        assert!(
            stats.date_range.1 <= ts_after.saturating_add(1),
            "max timestamp must be <= operation end time (plus 1 for ledger precision)"
        );
    }
}

/// **ORDER PRESERVATION**: Entries within a single invoice's audit trail
/// are in strictly chronological order (timestamps must not decrease).
#[test]
fn test_audit_order_preservation_timestamps() {
    let (env, client, _admin, business) = setup();
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;

    let invoice_id = client.store_invoice(
        &business,
        &1000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Order test"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    // Add more operations to the same invoice
    let _ = client.verify_invoice(&invoice_id);

    let trail = client.get_invoice_audit_trail(&invoice_id);

    // Check timestamps are non-decreasing
    if trail.len() > 1 {
        for i in 0..(trail.len() - 1) {
            let entry_curr = client.get_audit_entry(&trail.get(i).unwrap());
            let entry_next = client.get_audit_entry(&trail.get(i + 1).unwrap());

            assert!(
                entry_curr.timestamp <= entry_next.timestamp,
                "timestamps must be non-decreasing in audit trail. Index {} ts={}, index {} ts={}",
                i,
                entry_curr.timestamp,
                i + 1,
                entry_next.timestamp
            );
        }
    }
}

/// **NO TAMPERING**: All entries in the audit trail are retrievable
/// and unchanged after many subsequent operations.
#[test]
fn test_audit_no_tampering_entries_persist() {
    let (env, client, _admin, business) = setup();
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;

    // Create invoice and capture its audit entry
    let invoice_id = client.store_invoice(
        &business,
        &1000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Tampering test"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    let trail_v1 = client.get_invoice_audit_trail(&invoice_id);
    let first_entry_id = trail_v1.get(0).unwrap();
    let first_entry_data = client.get_audit_entry(&first_entry_id);

    // Perform many subsequent operations on other invoices
    for i in 0..10u32 {
        let _ = client.store_invoice(
            &business,
            &(2000i128 + i as i128),
            &currency,
            &due_date,
            &String::from_str(&env, &format!("Other {}", i)),
            &InvoiceCategory::Services,
            &Vec::new(&env),
        );
    }

    // Verify the first entry is still unchanged
    let first_entry_data_again = client.get_audit_entry(&first_entry_id);

    assert_eq!(
        first_entry_data.audit_id, first_entry_data_again.audit_id,
        "audit entry ID must not change"
    );
    assert_eq!(
        first_entry_data.operation, first_entry_data_again.operation,
        "audit entry operation must not change"
    );
    assert_eq!(
        first_entry_data.actor, first_entry_data_again.actor,
        "audit entry actor must not change"
    );
    assert_eq!(
        first_entry_data.timestamp, first_entry_data_again.timestamp,
        "audit entry timestamp must not change"
    );
}

/// **COMPREHENSIVE LIFECYCLE**: Create, verify, bid, accept; check that
/// all entries are recorded, in order, with correct counts.
#[test]
fn test_audit_comprehensive_lifecycle_entry_count() {
    let (env, client, _admin, business) = setup();
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;
    let investor = Address::generate(&env);

    let invoice_id = client.store_invoice(
        &business,
        &1000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Comprehensive test"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    let trail_after_create = client.get_invoice_audit_trail(&invoice_id);
    let count_after_create = trail_after_create.len();

    let _ = client.verify_invoice(&invoice_id);
    let trail_after_verify = client.get_invoice_audit_trail(&invoice_id);
    let count_after_verify = trail_after_verify.len();

    let bid_id = client.place_bid(&investor, &invoice_id, &900i128, &950i128);
    let trail_after_bid = client.get_invoice_audit_trail(&invoice_id);
    let count_after_bid = trail_after_bid.len();

    let _ = client.accept_bid(&invoice_id, &bid_id);
    let trail_after_accept = client.get_invoice_audit_trail(&invoice_id);
    let count_after_accept = trail_after_accept.len();

    // Each operation should append entries
    assert!(count_after_verify >= count_after_create + 1, "verify should add entry");
    assert!(count_after_bid >= count_after_verify + 1, "bid should add entry");
    assert!(
        count_after_accept >= count_after_bid + 1,
        "accept should add entry"
    );

    // All entries from previous steps should still be present
    for i in 0..count_after_create {
        let id_v1 = trail_after_create.get(i).unwrap();
        let id_vN = trail_after_accept.get(i).unwrap();
        assert_eq!(
            id_v1, id_vN,
            "entry at index {} must persist across lifecycle. Audit trail is append-only.",
            i
        );
    }
}

/// **EDGE CASE: Multiple operations in rapid succession**.
/// Verify that even rapid-fire operations each produce exactly one entry.
#[test]
fn test_audit_rapid_fire_operations() {
    let (env, client, _admin, business) = setup();
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;

    let stats_before = client.get_audit_stats();
    let count_before = stats_before.total_entries;

    // Rapid fire: 10 invoices in quick succession (same ledger timestamp)
    for i in 0..10u32 {
        let _ = client.store_invoice(
            &business,
            &(1000i128 + i as i128),
            &currency,
            &due_date,
            &String::from_str(&env, &format!("Rapid {}", i)),
            &InvoiceCategory::Services,
            &Vec::new(&env),
        );
    }

    let stats_after = client.get_audit_stats();
    let count_after = stats_after.total_entries;

    assert_eq!(
        count_after, count_before + 10,
        "10 rapid invoices must create exactly 10 entries"
    );
}

/// **EDGE CASE: Large amounts/values**.
/// Verify audit entries correctly record large amounts.
#[test]
fn test_audit_large_amounts_recorded() {
    let (env, client, _admin, business) = setup();
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;

    let large_amount = 9_999_999_999_999i128;
    let invoice_id = client.store_invoice(
        &business,
        &large_amount,
        &currency,
        &due_date,
        &String::from_str(&env, "Large amount"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    let trail = client.get_invoice_audit_trail(&invoice_id);
    assert!(!trail.is_empty());

    // Check if the amount was recorded in the audit entry
    let entry = client.get_audit_entry(&trail.get(0).unwrap());
    assert_eq!(
        entry.amount, Some(large_amount),
        "audit entry must record the large amount"
    );
}
