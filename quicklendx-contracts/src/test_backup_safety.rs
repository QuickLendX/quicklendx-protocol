#![cfg(test)]

/// # Backup/Restore Safety Unit Tests (Issue #819)
///
/// Unit-level tests that exercise `BackupStorage` directly (without going
/// through the contract client) to validate low-level invariants:
///
/// - `validate_backup` rejects every class of corrupt/missing data
/// - `restore_from_backup` follows the validate -> clear -> restore -> archive
///   sequence at the storage layer
/// - `cleanup_old_backups` correctly applies both age and count policies
/// - `generate_backup_id` produces IDs with the correct prefix
/// - `store_backup` rejects duplicate IDs
use crate::{
    backup::{Backup, BackupRetentionPolicy, BackupStatus, BackupStorage},
    errors::QuickLendXError,
    storage::InvoiceStorage,
    types::{DisputeStatus, Invoice, InvoiceCategory, InvoiceRating, InvoiceStatus, PaymentRecord},
};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Bytes, BytesN, Env, IntoVal, String, Vec,
};

// ============================================================================
// Helpers
// ============================================================================

fn setup_env() -> Env {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|li| li.timestamp = 1_000_000);
    env
}

/// Build a minimal valid Invoice for storage tests.
fn make_invoice(env: &Env, idx: u32, amount: i128) -> Invoice {
    use crate::types::{Dispute, DisputeStatus};
    use soroban_sdk::{Address, BytesN, String, Vec};

    let mut id_bytes = [0u8; 32];
    id_bytes[28..32].copy_from_slice(&idx.to_be_bytes());
    let id = BytesN::from_array(env, &id_bytes);

    Invoice {
        id,
        business: Address::generate(env),
        amount,
        currency: Address::generate(env),
        due_date: 9_999_999_999,
        status: InvoiceStatus::Pending,
        description: String::from_str(env, "backup safety test"),
        metadata_customer_name: None,
        metadata_customer_address: None,
        metadata_tax_id: None,
        metadata_notes: None,
        metadata_line_items: Vec::new(env),
        category: InvoiceCategory::Services,
        tags: Vec::new(env),
        funded_amount: 0,
        funded_at: None,
        investor: None,
        settled_at: None,
        average_rating: None,
        total_ratings: 0,
        ratings: Vec::new(env),
        dispute_status: DisputeStatus::None,
        dispute: Dispute {
            created_by: Address::generate(env),
            created_at: 0,
            reason: String::from_str(env, ""),
            evidence: String::from_str(env, ""),
            resolution: String::from_str(env, ""),
            resolved_by: Address::generate(env),
            resolved_at: 0,
            resolution_outcome: DisputeResolution::None,
        },
        total_paid: 0,
        payment_history: Vec::new(env),
        created_at: env.ledger().timestamp(),
    }
}

/// Create and persist a valid backup (metadata + data) and return its ID.
fn create_valid_backup(env: &Env, invoices: Vec<Invoice>) -> BytesN<32> {
    let backup_id = BackupStorage::generate_backup_id(env);
    let count = invoices.len();

    let backup = Backup {
        backup_id: backup_id.clone(),
        timestamp: env.ledger().timestamp(),
        description: String::from_str(env, "unit test backup"),
        invoice_count: count,
        status: BackupStatus::Active,
        format_version: 2,
    };

    BackupStorage::store_backup(env, &backup, Some(&invoices)).unwrap();
    BackupStorage::store_backup_data(env, &backup_id, &invoices);
    BackupStorage::add_to_backup_list(env, &backup_id);

    backup_id
}

// ============================================================================
// generate_backup_id Tests
// ============================================================================

/// Generated backup IDs must have the 0xB4 0xC4 prefix.
#[test]
fn test_generate_backup_id_has_correct_prefix() {
    let env = setup_env();
    let id = BackupStorage::generate_backup_id(&env);
    let bytes = id.to_array();
    assert_eq!(bytes[0], 0xB4, "First byte must be 0xB4");
    assert_eq!(bytes[1], 0xC4, "Second byte must be 0xC4");
}

/// Consecutive backup IDs must be unique.
#[test]
fn test_generate_backup_id_uniqueness() {
    let env = setup_env();
    let id1 = BackupStorage::generate_backup_id(&env);
    let id2 = BackupStorage::generate_backup_id(&env);
    let id3 = BackupStorage::generate_backup_id(&env);
    assert_ne!(id1, id2);
    assert_ne!(id2, id3);
    assert_ne!(id1, id3);
}

/// `is_valid_backup_id` returns true only for IDs with the correct prefix.
#[test]
fn test_is_valid_backup_id_prefix_check() {
    let env = setup_env();
    let valid_id = BackupStorage::generate_backup_id(&env);
    assert!(BackupStorage::is_valid_backup_id(&valid_id));

    let invalid_id = BytesN::from_array(&env, &[0x00u8; 32]);
    assert!(!BackupStorage::is_valid_backup_id(&invalid_id));
}

// ============================================================================
// store_backup Tests
// ============================================================================

/// Storing a backup with a duplicate ID must fail.
#[test]
fn test_store_backup_rejects_duplicate_id() {
    let env = setup_env();
    let mut invoices = Vec::new(&env);
    invoices.push_back(make_invoice(&env, 0, 1_000));

    let backup_id = create_valid_backup(&env, invoices.clone());

    // Try to store again with the same ID
    let backup = Backup {
        backup_id: backup_id.clone(),
        timestamp: env.ledger().timestamp(),
        description: String::from_str(&env, "duplicate"),
        invoice_count: 1,
        status: BackupStatus::Active,
        format_version: 2,
    };
    let result = BackupStorage::store_backup(&env, &backup, Some(&invoices));
    assert_eq!(result, Err(QuickLendXError::OperationNotAllowed));
}

/// Storing a backup with empty description must fail.
#[test]
fn test_store_backup_rejects_empty_description() {
    let env = setup_env();
    let backup_id = BackupStorage::generate_backup_id(&env);
    let invoices: Vec<Invoice> = Vec::new(&env);

    let backup = Backup {
        backup_id: backup_id.clone(),
        timestamp: env.ledger().timestamp(),
        description: String::from_str(&env, ""),
        invoice_count: 0,
        status: BackupStatus::Active,
        format_version: 2,
    };
    let result = BackupStorage::store_backup(&env, &backup, Some(&invoices));
    assert_eq!(result, Err(QuickLendXError::InvalidDescription));
}

/// Storing a backup where invoice_count mismatches the payload must fail.
#[test]
fn test_store_backup_rejects_count_mismatch() {
    let env = setup_env();
    let backup_id = BackupStorage::generate_backup_id(&env);
    let mut invoices = Vec::new(&env);
    invoices.push_back(make_invoice(&env, 0, 1_000));

    let backup = Backup {
        backup_id: backup_id.clone(),
        timestamp: env.ledger().timestamp(),
        description: String::from_str(&env, "mismatch test"),
        invoice_count: 5, // wrong count
        status: BackupStatus::Active,
        format_version: 2,
    };
    let result = BackupStorage::store_backup(&env, &backup, Some(&invoices));
    assert_eq!(result, Err(QuickLendXError::StorageError));
}

// ============================================================================
// validate_backup Tests
// ============================================================================

/// validate_backup succeeds for a well-formed backup.
#[test]
fn test_validate_backup_succeeds_for_valid_backup() {
    let env = setup_env();
    let mut invoices = Vec::new(&env);
    invoices.push_back(make_invoice(&env, 0, 1_000));
    invoices.push_back(make_invoice(&env, 1, 2_000));

    let backup_id = create_valid_backup(&env, invoices);
    assert!(BackupStorage::validate_backup(&env, &backup_id).is_ok());
}

/// validate_backup fails when the backup record does not exist.
#[test]
fn test_validate_backup_fails_when_record_missing() {
    let env = setup_env();
    let id = BackupStorage::generate_backup_id(&env);
    let result = BackupStorage::validate_backup(&env, &id);
    assert_eq!(result, Err(QuickLendXError::StorageKeyNotFound));
}

/// validate_backup fails when the payload data is missing.
#[test]
fn test_validate_backup_fails_when_data_missing() {
    let env = setup_env();
    let backup_id = BackupStorage::generate_backup_id(&env);
    let backup = Backup {
        backup_id: backup_id.clone(),
        timestamp: env.ledger().timestamp(),
        description: String::from_str(&env, "no data"),
        invoice_count: 1,
        status: BackupStatus::Active,
        format_version: 2,
    };
    BackupStorage::store_backup(&env, &backup, None).unwrap();
    // Data blob never stored
    let result = BackupStorage::validate_backup(&env, &backup_id);
    assert_eq!(result, Err(QuickLendXError::StorageKeyNotFound));
}

/// validate_backup fails when invoice_count mismatches the payload.
#[test]
fn test_validate_backup_fails_on_count_mismatch() {
    let env = setup_env();
    let backup_id = BackupStorage::generate_backup_id(&env);
    let mut invoices = Vec::new(&env);
    invoices.push_back(make_invoice(&env, 0, 1_000));

    // Claim count = 2, but only 1 invoice in data
    let backup = Backup {
        backup_id: backup_id.clone(),
        timestamp: env.ledger().timestamp(),
        description: String::from_str(&env, "count mismatch"),
        invoice_count: 2,
        status: BackupStatus::Active,
        format_version: 2,
    };
    env.storage().instance().set(&backup_id, &backup);
    BackupStorage::store_backup_data(&env, &backup_id, &invoices);

    let result = BackupStorage::validate_backup(&env, &backup_id);
    assert_eq!(result, Err(QuickLendXError::StorageError));
}

/// validate_backup fails when any invoice has amount <= 0.
#[test]
fn test_validate_backup_fails_on_zero_amount_invoice() {
    let env = setup_env();
    let mut invoices = Vec::new(&env);
    invoices.push_back(make_invoice(&env, 0, 0)); // zero amount

    let backup_id = create_valid_backup(&env, invoices);
    let result = BackupStorage::validate_backup(&env, &backup_id);
    assert_eq!(result, Err(QuickLendXError::StorageError));
}

/// validate_backup fails for an Archived backup.
#[test]
fn test_validate_backup_fails_for_archived_backup() {
    let env = setup_env();
    let mut invoices = Vec::new(&env);
    invoices.push_back(make_invoice(&env, 0, 1_000));

    let backup_id = create_valid_backup(&env, invoices);

    // Archive the backup
    let mut backup = BackupStorage::get_backup(&env, &backup_id).unwrap();
    backup.status = BackupStatus::Archived;
    BackupStorage::update_backup(&env, &backup).unwrap();

    let result = BackupStorage::validate_backup(&env, &backup_id);
    assert_eq!(result, Err(QuickLendXError::OperationNotAllowed));
}

/// validate_backup fails for a Corrupted backup.
#[test]
fn test_validate_backup_fails_for_corrupted_backup() {
    let env = setup_env();
    let mut invoices = Vec::new(&env);
    invoices.push_back(make_invoice(&env, 0, 1_000));

    let backup_id = create_valid_backup(&env, invoices);

    // Mark as Corrupted
    let mut backup = BackupStorage::get_backup(&env, &backup_id).unwrap();
    backup.status = BackupStatus::Corrupted;
    BackupStorage::update_backup(&env, &backup).unwrap();

    let result = BackupStorage::validate_backup(&env, &backup_id);
    assert_eq!(result, Err(QuickLendXError::OperationNotAllowed));
}

// ============================================================================
// restore_from_backup Tests
// ============================================================================

/// Successful restore returns the correct invoice count.
#[test]
fn test_restore_returns_correct_count() {
    let env = setup_env();
    let mut invoices = Vec::new(&env);
    invoices.push_back(make_invoice(&env, 0, 1_000));
    invoices.push_back(make_invoice(&env, 1, 2_000));
    invoices.push_back(make_invoice(&env, 2, 3_000));

    let backup_id = create_valid_backup(&env, invoices);
    let count = BackupStorage::restore_from_backup(&env, &backup_id).unwrap();
    assert_eq!(count, 3);
}

/// Restore clears existing invoices before writing backup data.
#[test]
fn test_restore_clears_existing_invoices() {
    let env = setup_env();

    // Pre-populate storage with an invoice not in the backup
    let stale_invoice = make_invoice(&env, 99, 9_999);
    InvoiceStorage::store_invoice(&env, &stale_invoice);
    assert_eq!(InvoiceStorage::get_total_count(&env), 1);

    // Create backup with a different invoice
    let mut invoices = Vec::new(&env);
    invoices.push_back(make_invoice(&env, 0, 1_000));
    let backup_id = create_valid_backup(&env, invoices);

    BackupStorage::restore_from_backup(&env, &backup_id).unwrap();

    // Stale invoice must be gone
    assert!(InvoiceStorage::get(&env, &stale_invoice.id).is_none());
}

/// Restore rebuilds the status index for all restored invoices.
#[test]
fn test_restore_rebuilds_status_index() {
    let env = setup_env();
    let mut invoices = Vec::new(&env);
    invoices.push_back(make_invoice(&env, 0, 1_000));
    invoices.push_back(make_invoice(&env, 1, 2_000));

    let backup_id = create_valid_backup(&env, invoices);
    BackupStorage::restore_from_backup(&env, &backup_id).unwrap();

    let pending = InvoiceStorage::get_by_status(&env, InvoiceStatus::Pending);
    assert_eq!(pending.len(), 2);
}

/// Restore marks the backup as Archived.
#[test]
fn test_restore_marks_backup_archived() {
    let env = setup_env();
    let mut invoices = Vec::new(&env);
    invoices.push_back(make_invoice(&env, 0, 1_000));

    let backup_id = create_valid_backup(&env, invoices);
    BackupStorage::restore_from_backup(&env, &backup_id).unwrap();

    let backup = BackupStorage::get_backup(&env, &backup_id).unwrap();
    assert_eq!(backup.status, BackupStatus::Archived);
}

/// Restore fails for an already-archived backup (idempotency guard).
#[test]
fn test_restore_fails_for_archived_backup() {
    let env = setup_env();
    let mut invoices = Vec::new(&env);
    invoices.push_back(make_invoice(&env, 0, 1_000));

    let backup_id = create_valid_backup(&env, invoices);
    BackupStorage::restore_from_backup(&env, &backup_id).unwrap();

    // Second restore must fail
    let result = BackupStorage::restore_from_backup(&env, &backup_id);
    assert_eq!(result, Err(QuickLendXError::OperationNotAllowed));
}

/// Restore fails for a non-existent backup ID without touching storage.
#[test]
fn test_restore_fails_for_nonexistent_backup() {
    let env = setup_env();

    // Pre-populate storage
    let invoice = make_invoice(&env, 0, 1_000);
    InvoiceStorage::store_invoice(&env, &invoice);

    let fake_id = BytesN::from_array(
        &env,
        &[
            0xB4, 0xC4, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0,
        ],
    );
    let result = BackupStorage::restore_from_backup(&env, &fake_id);
    assert!(result.is_err());

    // Storage untouched
    assert!(InvoiceStorage::get(&env, &invoice.id).is_some());
}

// ============================================================================
// cleanup_old_backups Tests
// ============================================================================

/// cleanup_old_backups returns 0 when auto_cleanup_enabled is false.
#[test]
fn test_cleanup_returns_zero_when_disabled() {
    let env = setup_env();
    BackupStorage::set_retention_policy(
        &env,
        &BackupRetentionPolicy {
            max_backups: 1,
            max_age_seconds: 0,
            auto_cleanup_enabled: false,
        },
    );

    let mut invoices = Vec::new(&env);
    invoices.push_back(make_invoice(&env, 0, 1_000));
    create_valid_backup(&env, invoices.clone());
    create_valid_backup(&env, invoices);

    let removed = BackupStorage::cleanup_old_backups(&env).unwrap();
    assert_eq!(removed, 0);
    assert_eq!(BackupStorage::get_all_backups(&env).len(), 2);
}

/// cleanup_old_backups removes oldest backups when count exceeds max_backups.
#[test]
fn test_cleanup_count_policy_removes_oldest() {
    let env = setup_env();
    BackupStorage::set_retention_policy(
        &env,
        &BackupRetentionPolicy {
            max_backups: 2,
            max_age_seconds: 0,
            auto_cleanup_enabled: true,
        },
    );

    let mut invoices = Vec::new(&env);
    invoices.push_back(make_invoice(&env, 0, 1_000));

    let id1 = create_valid_backup(&env, invoices.clone());
    env.ledger().set_timestamp(env.ledger().timestamp() + 1);
    let id2 = create_valid_backup(&env, invoices.clone());
    env.ledger().set_timestamp(env.ledger().timestamp() + 1);
    let id3 = create_valid_backup(&env, invoices);

    let removed = BackupStorage::cleanup_old_backups(&env).unwrap();
    assert_eq!(removed, 1);

    let remaining = BackupStorage::get_all_backups(&env);
    assert_eq!(remaining.len(), 2);
    assert!(!remaining.contains(&id1));
    assert!(remaining.contains(&id2));
    assert!(remaining.contains(&id3));
}

/// cleanup_old_backups removes backups older than max_age_seconds.
#[test]
fn test_cleanup_age_policy_removes_expired() {
    let env = setup_env();
    BackupStorage::set_retention_policy(
        &env,
        &BackupRetentionPolicy {
            max_backups: 0,
            max_age_seconds: 100,
            auto_cleanup_enabled: true,
        },
    );

    let mut invoices = Vec::new(&env);
    invoices.push_back(make_invoice(&env, 0, 1_000));

    let old_id = create_valid_backup(&env, invoices.clone());
    env.ledger().set_timestamp(env.ledger().timestamp() + 150);
    let new_id = create_valid_backup(&env, invoices);

    let removed = BackupStorage::cleanup_old_backups(&env).unwrap();
    assert_eq!(removed, 1);

    let remaining = BackupStorage::get_all_backups(&env);
    assert_eq!(remaining.len(), 1);
    assert!(!remaining.contains(&old_id));
    assert!(remaining.contains(&new_id));
}

/// cleanup_old_backups does not remove Archived backups.
#[test]
fn test_cleanup_does_not_remove_archived_backups() {
    let env = setup_env();
    BackupStorage::set_retention_policy(
        &env,
        &BackupRetentionPolicy {
            max_backups: 1,
            max_age_seconds: 0,
            auto_cleanup_enabled: true,
        },
    );

    let mut invoices = Vec::new(&env);
    invoices.push_back(make_invoice(&env, 0, 1_000));

    let archived_id = create_valid_backup(&env, invoices.clone());
    // Archive it
    let mut backup = BackupStorage::get_backup(&env, &archived_id).unwrap();
    backup.status = BackupStatus::Archived;
    BackupStorage::update_backup(&env, &backup).unwrap();

    env.ledger().set_timestamp(env.ledger().timestamp() + 1);
    let active_id = create_valid_backup(&env, invoices.clone());
    env.ledger().set_timestamp(env.ledger().timestamp() + 1);
    let newest_id = create_valid_backup(&env, invoices);

    let removed = BackupStorage::cleanup_old_backups(&env).unwrap();
    assert_eq!(removed, 1); // Only active_id removed

    let remaining = BackupStorage::get_all_backups(&env);
    assert!(!remaining.contains(&active_id));
    assert!(remaining.contains(&newest_id));

    // Archived backup still exists
    let archived = BackupStorage::get_backup(&env, &archived_id).unwrap();
    assert_eq!(archived.status, BackupStatus::Archived);
}

/// cleanup_old_backups enforces both age and count limits together.
#[test]
fn test_cleanup_enforces_both_age_and_count_limits() {
    let env = setup_env();
    BackupStorage::set_retention_policy(
        &env,
        &BackupRetentionPolicy {
            max_backups: 3,
            max_age_seconds: 200,
            auto_cleanup_enabled: true,
        },
    );

    let mut invoices = Vec::new(&env);
    invoices.push_back(make_invoice(&env, 0, 1_000));

    // Create 5 backups with varying ages
    let id1 = create_valid_backup(&env, invoices.clone()); // t=0
    env.ledger().set_timestamp(env.ledger().timestamp() + 50);
    let id2 = create_valid_backup(&env, invoices.clone()); // t=50
    env.ledger().set_timestamp(env.ledger().timestamp() + 50);
    let id3 = create_valid_backup(&env, invoices.clone()); // t=100
    env.ledger().set_timestamp(env.ledger().timestamp() + 100);
    let id4 = create_valid_backup(&env, invoices.clone()); // t=200
    env.ledger().set_timestamp(env.ledger().timestamp() + 50);
    let id5 = create_valid_backup(&env, invoices); // t=250

    // Current time = 250
    // id1 age = 250 (exceeds 200) -> removed by age
    // id2 age = 200 (at limit) -> kept
    // id3 age = 150 (within limit) -> kept
    // id4 age = 50 (within limit) -> kept
    // id5 age = 0 (within limit) -> kept
    // After age cleanup: 4 backups remain (id2, id3, id4, id5)
    // Count limit = 3, so oldest (id2) removed
    // Final: id3, id4, id5 remain

    let removed = BackupStorage::cleanup_old_backups(&env).unwrap();
    assert_eq!(removed, 2); // id1 (age) + id2 (count)

    let remaining = BackupStorage::get_all_backups(&env);
    assert_eq!(remaining.len(), 3);
    assert!(!remaining.contains(&id1)); // Removed by age
    assert!(!remaining.contains(&id2)); // Removed by count
    assert!(remaining.contains(&id3));
    assert!(remaining.contains(&id4));
    assert!(remaining.contains(&id5));

    // Verify remaining backups are within age limit
    let current_time = env.ledger().timestamp();
    for backup_id in remaining.iter() {
        let backup = BackupStorage::get_backup(&env, backup_id).unwrap();
        let age = current_time.saturating_sub(backup.timestamp);
        assert!(age <= 200, "Backup age {} exceeds max_age_seconds 200", age);
    }
}

// ============================================================================
// add_to_backup_list / remove_from_backup_list Tests
// ============================================================================

/// add_to_backup_list is idempotent (no duplicates).
#[test]
fn test_add_to_backup_list_is_idempotent() {
    let env = setup_env();
    let id = BackupStorage::generate_backup_id(&env);

    BackupStorage::add_to_backup_list(&env, &id);
    BackupStorage::add_to_backup_list(&env, &id);
    BackupStorage::add_to_backup_list(&env, &id);

    let list = BackupStorage::get_all_backups(&env);
    assert_eq!(list.len(), 1);
}

/// remove_from_backup_list removes the correct entry.
#[test]
fn test_remove_from_backup_list() {
    let env = setup_env();
    let id1 = BackupStorage::generate_backup_id(&env);
    let id2 = BackupStorage::generate_backup_id(&env);

    BackupStorage::add_to_backup_list(&env, &id1);
    BackupStorage::add_to_backup_list(&env, &id2);
    assert_eq!(BackupStorage::get_all_backups(&env).len(), 2);

    BackupStorage::remove_from_backup_list(&env, &id1);
    let list = BackupStorage::get_all_backups(&env);
    assert_eq!(list.len(), 1);
    assert!(!list.contains(&id1));
    assert!(list.contains(&id2));
}

// ============================================================================
// purge_backup Tests
// ============================================================================

/// purge_backup removes metadata, payload, and list entry.
#[test]
fn test_purge_backup_removes_all_traces() {
    let env = setup_env();
    let mut invoices = Vec::new(&env);
    invoices.push_back(make_invoice(&env, 0, 1_000));

    let backup_id = create_valid_backup(&env, invoices);

    BackupStorage::purge_backup(&env, &backup_id);

    assert!(BackupStorage::get_backup(&env, &backup_id).is_none());
    assert!(BackupStorage::get_backup_data(&env, &backup_id).is_none());
    assert!(!BackupStorage::get_all_backups(&env).contains(&backup_id));
}

// ============================================================================
// Versioning & Compatibility Tests
// ============================================================================

/// Test that V1 backups are correctly upgraded to V2 when read.
#[test]
fn test_v1_compatibility_upgrade_roundtrip() {
    let env = setup_env();
    let backup_id = BackupStorage::generate_backup_id(&env);

    let backup_v1 = crate::backup_v1::BackupV1 {
        backup_id: backup_id.clone(),
        timestamp: env.ledger().timestamp(),
        description: soroban_sdk::String::from_str(&env, "v1 backup"),
        invoice_count: 1,
        status: BackupStatus::Active,
    };

    env.storage().instance().set(&backup_id, &backup_v1);

    // Get backup and verify it was upgraded to v2 on the fly
    let backup = BackupStorage::get_backup(&env, &backup_id).unwrap();
    assert_eq!(backup.backup_id, backup_id);
    assert_eq!(backup.format_version, 2);
    assert_eq!(
        backup.description,
        soroban_sdk::String::from_str(&env, "v1 backup")
    );
}

/// Test that V2/current backups are stored and retrieved normally.
#[test]
fn test_v2_normal_roundtrip() {
    let env = setup_env();
    let mut invoices = Vec::new(&env);
    invoices.push_back(make_invoice(&env, 0, 1_000));

    let backup_id = create_valid_backup(&env, invoices);

    let backup = BackupStorage::get_backup(&env, &backup_id).unwrap();
    assert_eq!(backup.format_version, 2);
    assert_eq!(backup.invoice_count, 1);
}

/// Test that V1 backups are upgraded correctly and can be restored.
#[test]
fn test_v1_upgrade_restore_path() {
    let env = setup_env();

    // Pre-populate storage with stale invoice
    let stale = make_invoice(&env, 99, 9_999);
    InvoiceStorage::store_invoice(&env, &stale);

    let mut invoices = Vec::new(&env);
    invoices.push_back(make_invoice(&env, 0, 1_000));

    let backup_id = BackupStorage::generate_backup_id(&env);
    let backup_v1 = crate::backup_v1::BackupV1 {
        backup_id: backup_id.clone(),
        timestamp: env.ledger().timestamp(),
        description: soroban_sdk::String::from_str(&env, "v1 backup"),
        invoice_count: 1,
        status: BackupStatus::Active,
    };

    env.storage().instance().set(&backup_id, &backup_v1);
    BackupStorage::store_backup_data(&env, &backup_id, &invoices);
    BackupStorage::add_to_backup_list(&env, &backup_id);

    // Restore and verify
    let count = BackupStorage::restore_from_backup(&env, &backup_id).unwrap();
    assert_eq!(count, 1);

    // Stale invoice should be gone and new invoice registered
    assert!(InvoiceStorage::get(&env, &stale.id).is_none());
    assert!(InvoiceStorage::get(&env, &invoices.get(0).unwrap().id).is_some());

    // Backup status should be Archived
    let backup = BackupStorage::get_backup(&env, &backup_id).unwrap();
    assert_eq!(backup.status, BackupStatus::Archived);
}

/// Test that V3 and newer future versions are rejected before restore/validation execution.
#[test]
fn test_v3_rejection_and_unsupported_error() {
    let env = setup_env();
    let backup_id = BackupStorage::generate_backup_id(&env);

    // Store a backup with version = 3 directly using Map to simulate future struct
    let mut map = soroban_sdk::Map::<soroban_sdk::Symbol, soroban_sdk::Val>::new(&env);
    map.set(
        soroban_sdk::Symbol::new(&env, "backup_id"),
        backup_id.clone().into_val(&env),
    );
    map.set(
        soroban_sdk::Symbol::new(&env, "timestamp"),
        env.ledger().timestamp().into_val(&env),
    );
    map.set(
        soroban_sdk::Symbol::new(&env, "description"),
        soroban_sdk::String::from_str(&env, "v3 backup").into_val(&env),
    );
    map.set(
        soroban_sdk::Symbol::new(&env, "invoice_count"),
        1u32.into_val(&env),
    );
    map.set(
        soroban_sdk::Symbol::new(&env, "status"),
        BackupStatus::Active.into_val(&env),
    );
    map.set(
        soroban_sdk::Symbol::new(&env, "format_version"),
        3u32.into_val(&env),
    );

    env.storage().instance().set(&backup_id, &map);

    // Verify validate fails with BackupVersionUnsupported
    let val_result = BackupStorage::validate_backup(&env, &backup_id);
    assert_eq!(val_result, Err(QuickLendXError::BackupVersionUnsupported));

    // Verify restore fails with BackupVersionUnsupported
    let rest_result = BackupStorage::restore_from_backup(&env, &backup_id);
    assert_eq!(rest_result, Err(QuickLendXError::BackupVersionUnsupported));
}

/// Test that truncated / malformed payloads fail safely.
#[test]
fn test_malformed_truncated_payload_handling() {
    let env = setup_env();
    let backup_id = BackupStorage::generate_backup_id(&env);

    // Set a malformed non-struct Val (like a single u32)
    env.storage().instance().set(&backup_id, &42u32);

    let val_result = BackupStorage::validate_backup(&env, &backup_id);
    assert_eq!(val_result, Err(QuickLendXError::StorageError));

    let rest_result = BackupStorage::restore_from_backup(&env, &backup_id);
    assert_eq!(rest_result, Err(QuickLendXError::StorageError));
}

/// Test mixed-version restore handling.
#[test]
fn test_mixed_version_restore_handling() {
    let env = setup_env();

    // Pre-populate storage
    let stale = make_invoice(&env, 99, 9_999);
    InvoiceStorage::store_invoice(&env, &stale);

    // 1. Create a V3 backup (unsupported)
    let v3_id = BackupStorage::generate_backup_id(&env);
    let mut map = soroban_sdk::Map::<soroban_sdk::Symbol, soroban_sdk::Val>::new(&env);
    map.set(
        soroban_sdk::Symbol::new(&env, "backup_id"),
        v3_id.clone().into_val(&env),
    );
    map.set(
        soroban_sdk::Symbol::new(&env, "timestamp"),
        env.ledger().timestamp().into_val(&env),
    );
    map.set(
        soroban_sdk::Symbol::new(&env, "description"),
        soroban_sdk::String::from_str(&env, "v3").into_val(&env),
    );
    map.set(
        soroban_sdk::Symbol::new(&env, "invoice_count"),
        1u32.into_val(&env),
    );
    map.set(
        soroban_sdk::Symbol::new(&env, "status"),
        BackupStatus::Active.into_val(&env),
    );
    map.set(
        soroban_sdk::Symbol::new(&env, "format_version"),
        3u32.into_val(&env),
    );
    env.storage().instance().set(&v3_id, &map);

    // Attempting to restore V3 should fail and leave storage untouched
    let rest_v3 = BackupStorage::restore_from_backup(&env, &v3_id);
    assert_eq!(rest_v3, Err(QuickLendXError::BackupVersionUnsupported));
    assert!(InvoiceStorage::get(&env, &stale.id).is_some());

    // 2. Create a V1 backup (supported)
    let v1_id = BackupStorage::generate_backup_id(&env);
    let mut invoices = Vec::new(&env);
    invoices.push_back(make_invoice(&env, 0, 1_000));
    let backup_v1 = crate::backup_v1::BackupV1 {
        backup_id: v1_id.clone(),
        timestamp: env.ledger().timestamp(),
        description: soroban_sdk::String::from_str(&env, "v1"),
        invoice_count: 1,
        status: BackupStatus::Active,
    };
    env.storage().instance().set(&v1_id, &backup_v1);
    BackupStorage::store_backup_data(&env, &v1_id, &invoices);

    // Restore V1 should succeed
    let rest_v1 = BackupStorage::restore_from_backup(&env, &v1_id).unwrap();
    assert_eq!(rest_v1, 1);
    assert!(InvoiceStorage::get(&env, &stale.id).is_none());
    assert!(InvoiceStorage::get(&env, &invoices.get(0).unwrap().id).is_some());
}
