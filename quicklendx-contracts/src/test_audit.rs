//! Tests for audit trail: log writes, query filters, and integrity validation.
//!
//! Cases: every state change produces correct log entry; query by invoice/actor/op
//! returns correct subset; integrity check passes (and fails when expected).

use super::*;
use crate::audit::{AuditOperation, AuditOperationFilter, AuditQueryFilter};
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
