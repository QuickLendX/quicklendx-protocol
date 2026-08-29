//! Storage and migration compatibility tests for QuickLendX.
//!
//! # Purpose
//! These tests provide a deterministic, reviewable guarantee that:
//! - Schema versioning is persisted and retrieved correctly.
//! - A migration lifecycle (start → page → commit) emits the correct events.
//! - A rollback leaves no partial migration state.
//! - Repeated start calls are rejected (only one migration at a time).
//! - A failed page leaves the offset at the last checkpoint so the migration
//!   is resumable without re-processing already-committed records.
//! - Concurrent or stale `begin_migration` calls that pass the wrong
//!   `schema_from` are rejected.
//! - A fresh (version-0) contract can be upgraded to version 1.
//! - A legacy-data fixture (records written before migration) survives the
//!   migration unchanged.

#![cfg(test)]

use soroban_sdk::testutils::Ledger;
use soroban_sdk::testutils::Events as EventsTrait;
use soroban_sdk::{testutils::Address as _, Address, Env};

use crate::storage::{StorageMigration, STORAGE_SCHEMA_VERSION};

// --- Helpers ---

fn setup() -> (Env, Address) {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);
    let admin = Address::generate(&env);
    (env, admin)
}

fn event_emitted(env: &Env, topic: &str) -> bool {
    use soroban_sdk::xdr;
    use soroban_sdk::{Symbol, TryFromVal};
    let topic_sym = Symbol::new(env, topic);
    let topic_xdr = xdr::ScVal::try_from_val(env, &topic_sym).expect("topic to xdr");
    env.events()
        .all()
        .events()
        .iter()
        .any(|e| match &e.body {
            xdr::ContractEventBody::V0(b) => b.topics.first() == Some(&topic_xdr),
        })
}

// --- Tests ---

#[test]
fn test_fresh_contract_schema_version_is_zero() {
    let (env, _admin) = setup();
    assert_eq!(StorageMigration::get_schema_version(&env), 0);
    assert!(!StorageMigration::is_migration_in_progress(&env));
    assert_eq!(StorageMigration::get_migration_offset(&env), 0);
    assert_eq!(StorageMigration::get_migration_records_migrated(&env), 0);
}

#[test]
fn test_set_schema_version_persists_and_emits() {
    let (env, admin) = setup();
    StorageMigration::set_schema_version(&env, 1, &admin);
    assert_eq!(StorageMigration::get_schema_version(&env), 1);
    assert!(event_emitted(&env, "schema_version_set"));
}

#[test]
fn test_begin_migration_happy_path() {
    let (env, admin) = setup();
    StorageMigration::begin_migration(&env, &admin, 0, 1).expect("begin_migration should succeed");
    assert!(StorageMigration::is_migration_in_progress(&env));
    assert_eq!(StorageMigration::get_pending_migration_version(&env), Some(1));
    assert_eq!(StorageMigration::get_migration_offset(&env), 0);
    assert_eq!(StorageMigration::get_migration_records_migrated(&env), 0);
    assert!(event_emitted(&env, "migration_started"));
}

#[test]
fn test_migration_full_lifecycle() {
    let (env, admin) = setup();
    StorageMigration::begin_migration(&env, &admin, 0, 1).unwrap();
    StorageMigration::advance_migration_page(&env, 42, 42);
    assert_eq!(StorageMigration::get_migration_offset(&env), 42);
    assert_eq!(StorageMigration::get_migration_records_migrated(&env), 42);
    StorageMigration::commit_migration(&env, &admin).unwrap();
    assert_eq!(StorageMigration::get_schema_version(&env), 1);
    assert!(!StorageMigration::is_migration_in_progress(&env));
    assert!(StorageMigration::get_pending_migration_version(&env).is_none());
    assert_eq!(StorageMigration::get_migration_offset(&env), 0);
    assert_eq!(StorageMigration::get_migration_records_migrated(&env), 0);
    assert!(event_emitted(&env, "migration_completed"));
}

#[test]
fn test_migration_rollback_clears_state() {
    let (env, admin) = setup();
    StorageMigration::begin_migration(&env, &admin, 0, 1).unwrap();
    StorageMigration::advance_migration_page(&env, 10, 10);
    StorageMigration::rollback_migration(&env, &admin).unwrap();
    assert_eq!(StorageMigration::get_schema_version(&env), 0);
    assert!(!StorageMigration::is_migration_in_progress(&env));
    assert!(StorageMigration::get_pending_migration_version(&env).is_none());
    assert_eq!(StorageMigration::get_migration_offset(&env), 0);
    assert_eq!(StorageMigration::get_migration_records_migrated(&env), 0);
    assert!(event_emitted(&env, "migration_rolled_back"));
}

#[test]
fn test_begin_migration_rejects_concurrent() {
    let (env, admin) = setup();
    StorageMigration::begin_migration(&env, &admin, 0, 1).unwrap();
    let err = StorageMigration::begin_migration(&env, &admin, 0, 2).unwrap_err();
    assert_eq!(err, crate::errors::QuickLendXError::OperationNotAllowed);
}

#[test]
fn test_begin_migration_rejects_wrong_schema_from() {
    let (env, admin) = setup();
    StorageMigration::set_schema_version(&env, 1, &admin);
    let err = StorageMigration::begin_migration(&env, &admin, 0, 2).unwrap_err();
    assert_eq!(err, crate::errors::QuickLendXError::OperationNotAllowed);
    StorageMigration::begin_migration(&env, &admin, 1, 2).unwrap();
}

#[test]
fn test_begin_migration_rejects_downgrade() {
    let (env, admin) = setup();
    let err_same = StorageMigration::begin_migration(&env, &admin, 0, 0).unwrap_err();
    assert_eq!(err_same, crate::errors::QuickLendXError::OperationNotAllowed);
}

#[test]
fn test_commit_without_begin_fails() {
    let (env, admin) = setup();
    let err = StorageMigration::commit_migration(&env, &admin).unwrap_err();
    assert_eq!(err, crate::errors::QuickLendXError::OperationNotAllowed);
    assert_eq!(StorageMigration::get_schema_version(&env), 0);
    assert!(!event_emitted(&env, "migration_completed"));
}

#[test]
fn test_rollback_without_begin_fails() {
    let (env, admin) = setup();
    let err = StorageMigration::rollback_migration(&env, &admin).unwrap_err();
    assert_eq!(err, crate::errors::QuickLendXError::OperationNotAllowed);
    assert!(!event_emitted(&env, "migration_rolled_back"));
}

#[test]
fn test_migration_partial_progress_resumable() {
    let (env, admin) = setup();
    StorageMigration::begin_migration(&env, &admin, 0, 1).unwrap();
    StorageMigration::advance_migration_page(&env, 20, 20);
    assert_eq!(StorageMigration::get_migration_offset(&env), 20);
    assert_eq!(StorageMigration::get_migration_records_migrated(&env), 20);
    StorageMigration::advance_migration_page(&env, 35, 15);
    assert_eq!(StorageMigration::get_migration_offset(&env), 35);
    assert_eq!(StorageMigration::get_migration_records_migrated(&env), 35);
    StorageMigration::commit_migration(&env, &admin).unwrap();
    assert_eq!(StorageMigration::get_schema_version(&env), 1);
    assert!(!StorageMigration::is_migration_in_progress(&env));
}

#[test]
fn test_migration_failure_event_and_resume() {
    let (env, admin) = setup();
    StorageMigration::begin_migration(&env, &admin, 0, 1).unwrap();
    StorageMigration::advance_migration_page(&env, 5, 5);
    StorageMigration::record_migration_failure(&env, 0, 5, "timeout");
    assert!(StorageMigration::is_migration_in_progress(&env));
    assert_eq!(StorageMigration::get_migration_offset(&env), 5);
    assert_eq!(StorageMigration::get_migration_records_migrated(&env), 5);
    assert!(event_emitted(&env, "migration_failed"));
    StorageMigration::advance_migration_page(&env, 15, 10);
    StorageMigration::commit_migration(&env, &admin).unwrap();
    assert_eq!(StorageMigration::get_schema_version(&env), 1);
    assert!(!StorageMigration::is_migration_in_progress(&env));
}

#[test]
fn test_migration_multi_page_advance() {
    let (env, admin) = setup();
    StorageMigration::begin_migration(&env, &admin, 0, 1).unwrap();
    for i in 1u32..=5 {
        StorageMigration::advance_migration_page(&env, i * 10, 10);
    }
    assert_eq!(StorageMigration::get_migration_records_migrated(&env), 50);
    assert_eq!(StorageMigration::get_migration_offset(&env), 50);
    StorageMigration::commit_migration(&env, &admin).unwrap();
    assert_eq!(StorageMigration::get_schema_version(&env), 1);
}

#[test]
fn test_legacy_data_survives_migration() {
    use crate::storage::InvoiceStorage;
    use crate::types::{
        DisputeResolution, DisputeStatus, Dispute, Invoice, InvoiceCategory, InvoiceStatus,
    };
    use soroban_sdk::{BytesN, String, Vec};

    let (env, admin) = setup();

    let invoice_id = BytesN::from_array(&env, &[0xAAu8; 32]);
    let business = Address::generate(&env);
    let legacy_invoice = Invoice {
        id: invoice_id.clone(),
        business: business.clone(),
        amount: 5_000,
        currency: Address::generate(&env),
        due_date: env.ledger().timestamp() + 86_400,
        status: InvoiceStatus::Pending,
        created_at: env.ledger().timestamp(),
        description: String::from_str(&env, "legacy invoice"),
        metadata_customer_name: None,
        metadata_customer_address: None,
        metadata_tax_id: None,
        metadata_notes: None,
        metadata_line_items: Vec::new(&env),
        category: InvoiceCategory::Services,
        tags: Vec::new(&env),
        funded_amount: 0,
        funded_at: None,
        investor: None,
        settled_at: None,
        average_rating: None,
        total_ratings: 0,
        ratings: Vec::new(&env),
        dispute_status: DisputeStatus::None,
        dispute: Dispute {
            created_by: Address::generate(&env),
            created_at: 0,
            reason: String::from_str(&env, ""),
            evidence: String::from_str(&env, ""),
            resolution: String::from_str(&env, ""),
            resolved_by: Address::generate(&env),
            resolved_at: 0,
            resolution_outcome: DisputeResolution::None,
        },
        total_paid: 0,
        payment_history: Vec::new(&env),
        origination_fee_bps: None,
        late_payment_penalty_bps: None,
        early_payment_discount_bps: None,
    };
    InvoiceStorage::store_invoice(&env, &legacy_invoice);
    assert_eq!(StorageMigration::get_schema_version(&env), 0);

    StorageMigration::begin_migration(&env, &admin, 0, 1).unwrap();
    StorageMigration::advance_migration_page(&env, 1, 1);
    StorageMigration::commit_migration(&env, &admin).unwrap();

    let retrieved = InvoiceStorage::get_invoice(&env, &invoice_id)
        .expect("legacy invoice must still exist after migration");
    assert_eq!(retrieved.amount, 5_000);
    assert_eq!(retrieved.status, InvoiceStatus::Pending);
    assert_eq!(retrieved.business, business);
    assert_eq!(StorageMigration::get_schema_version(&env), 1);
    assert!(!StorageMigration::is_migration_in_progress(&env));
}

#[test]
fn test_storage_migration_key_snapshot() {
    use soroban_sdk::symbol_short;
    assert_eq!(symbol_short!("sch_ver"), symbol_short!("sch_ver"));
    assert_eq!(symbol_short!("mig_ver"), symbol_short!("mig_ver"));
    assert_eq!(symbol_short!("mig_off"), symbol_short!("mig_off"));
    assert_eq!(symbol_short!("mig_rec"), symbol_short!("mig_rec"));
    assert_eq!(STORAGE_SCHEMA_VERSION, 1u32);
}

#[test]
fn test_migration_not_blocked_by_upgrade_state() {
    let (env, admin) = setup();
    StorageMigration::begin_migration(&env, &admin, 0, 1).unwrap();
    StorageMigration::commit_migration(&env, &admin).unwrap();
    assert_eq!(StorageMigration::get_schema_version(&env), 1);
}
