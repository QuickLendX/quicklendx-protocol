#![cfg(feature = "legacy-tests")]

use quicklendx_contracts::{types::InvoiceCategory, QuickLendXContract, QuickLendXContractClient};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, BytesN, Bytes, Env, Vec,
};

fn setup() -> (Env, QuickLendXContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|li| li.timestamp = 1_000_000);
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize_admin(&admin);
    client.initialize_protocol_limits(&admin, &1i128, &365u64, &86_400u64);
    (env, client, admin)
}

/// Create a verified business and store an invoice on its behalf.
fn create_invoice(
    env: &Env,
    client: &QuickLendXContractClient,
    admin: &Address,
    business: &Address,
    amount: i128,
    description: &str,
) -> BytesN<32> {
    let currency = Address::generate(env);
    let due_date = env.ledger().timestamp() + 86_400;
    client
        .store_invoice(
            business,
            &amount,
            &currency,
            &due_date,
            &Bytes::from_slice(env, description.as_bytes()),
            &InvoiceCategory::Services,
            &Vec::new(env),
        )
        .expect("store_invoice must succeed for verified business")
}

/// Create a verified business (KYC submitted + approved).
fn verified_business(env: &Env, client: &QuickLendXContractClient, admin: &Address) -> Address {
    let business = Address::generate(env);
    client.submit_kyc_application(&business, &Bytes::from_slice(env, b"kyc-data"));
    client.verify_business(admin, &business).expect("verify_business must succeed");
    business
}

// ============================================================================
// Restore Ordering Tests
// ============================================================================

/// Validation failure must leave storage completely untouched.
/// This tests the "validate before clear" invariant.
#[test]
fn test_restore_ordering_validate_before_clear() {
    let (env, client, admin) = setup();
    let business = verified_business(&env, &client, &admin);

    let invoice_1 = create_invoice(&env, &client, &admin, &business, 1_000, "Invoice A");
    let backup_id = client.create_backup(&admin).expect("backup must succeed");

    let invoice_2 = create_invoice(&env, &client, &admin, &business, 2_000, "Invoice B");
    assert_eq!(client.get_total_invoice_count(), 2);

    // Tamper with the backup metadata so validation fails
    let mut tampered = client.get_backup_details(&backup_id).unwrap();
    tampered.invoice_count = 999;
    env.storage().instance().set(&backup_id, &tampered);

    // Restore must fail and leave storage untouched
    let result = client.try_restore_backup(&admin, &backup_id);
    assert!(result.is_err(), "Tampered backup must fail validation");

    // Both invoices still exist — storage was NOT cleared
    assert_eq!(client.get_total_invoice_count(), 2);
    assert!(client.try_get_invoice(&invoice_1).is_ok());
    assert!(client.try_get_invoice(&invoice_2).is_ok());
}

/// Restore must clear all invoices before writing backup data.
/// This tests the "clear before restore" invariant.
#[test]
fn test_restore_ordering_clear_before_restore() {
    let (env, client, admin) = setup();
    let business = verified_business(&env, &client, &admin);

    let invoice_1 = create_invoice(&env, &client, &admin, &business, 1_000, "Invoice A");
    let backup_id = client.create_backup(&admin).expect("backup must succeed");

    let invoice_2 = create_invoice(&env, &client, &admin, &business, 2_000, "Invoice B");
    let invoice_3 = create_invoice(&env, &client, &admin, &business, 3_000, "Invoice C");
    assert_eq!(client.get_total_invoice_count(), 3);

    client.restore_backup(&admin, &backup_id).expect("restore must succeed");

    // Only the backed-up invoice should exist
    assert_eq!(client.get_total_invoice_count(), 1);
    assert!(client.try_get_invoice(&invoice_1).is_ok());
    assert!(client.try_get_invoice(&invoice_2).is_err());
    assert!(client.try_get_invoice(&invoice_3).is_err());
}

/// Restore must mark the backup as Archived after success.
/// This tests the "archive after restore" invariant.
#[test]
fn test_restore_ordering_archive_after_restore() {
    let (env, client, admin) = setup();
    let business = verified_business(&env, &client, &admin);

    create_invoice(&env, &client, &admin, &business, 1_000, "Invoice A");
    let backup_id = client.create_backup(&admin).expect("backup must succeed");

    let before = client.get_backup_details(&backup_id).unwrap();
    assert_eq!(before.status, BackupStatus::Active);

    client.restore_backup(&admin, &backup_id).expect("restore must succeed");

    let after = client.get_backup_details(&backup_id).unwrap();
    assert_eq!(after.status, BackupStatus::Archived);
}

/// Restore of a non-existent backup ID must fail without touching storage.
#[test]
fn test_restore_nonexistent_backup_fails_safely() {
    let (env, client, admin) = setup();
    let business = verified_business(&env, &client, &admin);

    let invoice_1 = create_invoice(&env, &client, &admin, &business, 1_000, "Invoice A");
    let fake_id = BytesN::from_array(&env, &[0xB4, 0xC4, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);

    let result = client.try_restore_backup(&admin, &fake_id);
    assert!(result.is_err(), "Non-existent backup must fail");

    // Storage untouched
    assert_eq!(client.get_total_invoice_count(), 1);
    assert!(client.try_get_invoice(&invoice_1).is_ok());
}

// ============================================================================
// Idempotency Tests
// ============================================================================

/// Repeated restore of the same backup must be blocked via archival.
#[test]
fn test_repeated_restore_is_blocked_via_archival() {
    let (env, client, admin) = setup();
    let business = verified_business(&env, &client, &admin);

    create_invoice(&env, &client, &admin, &business, 1_000, "Invoice A");
    let backup_id = client.create_backup(&admin).expect("backup must succeed");

    // First restore succeeds
    client.restore_backup(&admin, &backup_id).expect("first restore must succeed");
    assert_eq!(client.get_total_invoice_count(), 1);

    // Second restore must fail with OperationNotAllowed
    let result = client.try_restore_backup(&admin, &backup_id);
    assert!(result.is_err(), "Second restore must be blocked");
    assert_eq!(
        result.unwrap_err().unwrap(),
        QuickLendXError::OperationNotAllowed,
        "Expected OperationNotAllowed for archived backup"
    );

    // Storage unchanged after failed second restore
    assert_eq!(client.get_total_invoice_count(), 1);
}

/// If backup status is manually reset to Active, a second restore produces
/// the exact same state — proving the clear_all step makes restore idempotent.
#[test]
fn test_restore_produces_identical_state_when_repeated() {
    let (env, client, admin) = setup();
    let business = verified_business(&env, &client, &admin);

    let invoice_1 = create_invoice(&env, &client, &admin, &business, 1_000, "Invoice A");
    let backup_id = client.create_backup(&admin).expect("backup must succeed");

    // First restore
    client.restore_backup(&admin, &backup_id).expect("first restore must succeed");
    assert_eq!(client.get_total_invoice_count(), 1);

    // Manually reset backup status to Active
    let mut backup = client.get_backup_details(&backup_id).unwrap();
    backup.status = BackupStatus::Active;
    env.storage().instance().set(&backup_id, &backup);

    // Second restore — should produce identical state
    client.restore_backup(&admin, &backup_id).expect("second restore must succeed");

    assert_eq!(client.get_total_invoice_count(), 1);
    assert!(client.try_get_invoice(&invoice_1).is_ok());
}

/// Archived backup cannot be restored.
#[test]
fn test_archived_backup_cannot_be_restored() {
    let (env, client, admin) = setup();
    let business = verified_business(&env, &client, &admin);

    create_invoice(&env, &client, &admin, &business, 1_000, "Invoice A");
    let backup_id = client.create_backup(&admin).expect("backup must succeed");

    // Archive the backup manually
    client.archive_backup(&admin, &backup_id).expect("archive must succeed");

    // Restore must fail
    let result = client.try_restore_backup(&admin, &backup_id);
    assert!(result.is_err(), "Archived backup must not be restorable");
    assert_eq!(
        result.unwrap_err().unwrap(),
        QuickLendXError::OperationNotAllowed
    );
}

// ============================================================================
// Index Rebuild Tests
// ============================================================================

/// Restore rebuilds the business invoice index correctly.
#[test]
fn test_restore_rebuilds_business_index() {
    let (env, client, admin) = setup();
    let business_a = verified_business(&env, &client, &admin);
    let business_b = verified_business(&env, &client, &admin);

    let invoice_a1 = create_invoice(&env, &client, &admin, &business_a, 1_000, "A1");
    let invoice_a2 = create_invoice(&env, &client, &admin, &business_a, 2_000, "A2");
    let backup_id = client.create_backup(&admin).expect("backup must succeed");

    // Add invoices for business_b after backup
    create_invoice(&env, &client, &admin, &business_b, 3_000, "B1");
    create_invoice(&env, &client, &admin, &business_b, 4_000, "B2");

    client.restore_backup(&admin, &backup_id).expect("restore must succeed");

    // business_a index rebuilt with 2 invoices
    let a_invoices = client.get_invoices_by_business(&business_a);
    assert_eq!(a_invoices.len(), 2);
    assert!(a_invoices.contains(&invoice_a1));
    assert!(a_invoices.contains(&invoice_a2));

    // business_b index is empty (those invoices were cleared)
    let b_invoices = client.get_invoices_by_business(&business_b);
    assert_eq!(b_invoices.len(), 0);
}

/// Restore rebuilds the status index correctly.
#[test]
fn test_restore_rebuilds_status_index() {
    let (env, client, admin) = setup();
    let business = verified_business(&env, &client, &admin);

    let invoice_1 = create_invoice(&env, &client, &admin, &business, 1_000, "Invoice 1");
    let invoice_2 = create_invoice(&env, &client, &admin, &business, 2_000, "Invoice 2");
    let backup_id = client.create_backup(&admin).expect("backup must succeed");

    // Verify one invoice and add a new one after backup
    client.verify_invoice(&invoice_1);
    create_invoice(&env, &client, &admin, &business, 3_000, "Invoice 3");

    // Before restore: 2 pending, 1 verified
    assert_eq!(client.get_invoice_count_by_status(&InvoiceStatus::Pending), 2);
    assert_eq!(client.get_invoice_count_by_status(&InvoiceStatus::Verified), 1);

    client.restore_backup(&admin, &backup_id).expect("restore must succeed");

    // After restore: 2 pending (from backup), 0 verified
    assert_eq!(client.get_invoice_count_by_status(&InvoiceStatus::Pending), 2);
    assert_eq!(client.get_invoice_count_by_status(&InvoiceStatus::Verified), 0);

    let pending = client.get_invoices_by_status(&InvoiceStatus::Pending);
    assert!(pending.contains(&invoice_1));
    assert!(pending.contains(&invoice_2));
}

/// Restore with multiple invoice statuses rebuilds all status buckets.
#[test]
fn test_restore_rebuilds_multiple_status_buckets() {
    let (env, client, admin) = setup();
    let business = verified_business(&env, &client, &admin);

    let invoice_pending = create_invoice(&env, &client, &admin, &business, 1_000, "Pending");
    let invoice_verified = create_invoice(&env, &client, &admin, &business, 2_000, "Verified");
    client.verify_invoice(&invoice_verified);

    let backup_id = client.create_backup(&admin).expect("backup must succeed");

    // Add more invoices after backup
    create_invoice(&env, &client, &admin, &business, 3_000, "Post-backup");

    client.restore_backup(&admin, &backup_id).expect("restore must succeed");

    // Exactly 1 pending and 1 verified from backup
    assert_eq!(client.get_invoice_count_by_status(&InvoiceStatus::Pending), 1);
    assert_eq!(client.get_invoice_count_by_status(&InvoiceStatus::Verified), 1);
    assert_eq!(client.get_total_invoice_count(), 2);

    let pending = client.get_invoices_by_status(&InvoiceStatus::Pending);
    let verified = client.get_invoices_by_status(&InvoiceStatus::Verified);
    assert!(pending.contains(&invoice_pending));
    assert!(verified.contains(&invoice_verified));
}

/// Restore with an empty backup clears all invoices.
#[test]
fn test_restore_empty_backup_clears_all_invoices() {
    let (env, client, admin) = setup();
    let business = verified_business(&env, &client, &admin);

    // Create backup with no invoices
    let backup_id = client.create_backup(&admin).expect("empty backup must succeed");
    let backup = client.get_backup_details(&backup_id).unwrap();
    assert_eq!(backup.invoice_count, 0);

    // Add invoices after backup
    create_invoice(&env, &client, &admin, &business, 1_000, "Invoice A");
    create_invoice(&env, &client, &admin, &business, 2_000, "Invoice B");
    assert_eq!(client.get_total_invoice_count(), 2);

    client.restore_backup(&admin, &backup_id).expect("restore must succeed");

    // All invoices cleared
    assert_eq!(client.get_total_invoice_count(), 0);
    assert_eq!(client.get_invoices_by_business(&business).len(), 0);
    assert_eq!(client.get_invoice_count_by_status(&InvoiceStatus::Pending), 0);
}

// ============================================================================
// Retention Policy Tests
// ============================================================================

/// Count-based cleanup purges the oldest active backups.
#[test]
fn test_retention_count_based_cleanup() {
    let (env, client, admin) = setup();
    let business = verified_business(&env, &client, &admin);

    create_invoice(&env, &client, &admin, &business, 1_000, "Invoice A");
    client.set_backup_retention_policy(&admin, &2, &0, &true);

    let id1 = client.create_backup(&admin).expect("backup 1 must succeed");
    env.ledger().set_timestamp(env.ledger().timestamp() + 1);
    let id2 = client.create_backup(&admin).expect("backup 2 must succeed");
    env.ledger().set_timestamp(env.ledger().timestamp() + 1);
    let id3 = client.create_backup(&admin).expect("backup 3 must succeed");

    let active = client.get_backups();
    assert_eq!(active.len(), 2);
    assert!(!active.contains(&id1), "Oldest backup must be purged");
    assert!(active.contains(&id2));
    assert!(active.contains(&id3));
    assert!(client.get_backup_details(&id1).is_none());
}

/// Age-based cleanup purges backups older than max_age_seconds.
#[test]
fn test_retention_age_based_cleanup() {
    let (env, client, admin) = setup();
    let business = verified_business(&env, &client, &admin);

    create_invoice(&env, &client, &admin, &business, 1_000, "Invoice A");
    client.set_backup_retention_policy(&admin, &0, &100, &true);

    let old_id = client.create_backup(&admin).expect("old backup must succeed");
    env.ledger().set_timestamp(env.ledger().timestamp() + 150);
    let new_id = client.create_backup(&admin).expect("new backup must succeed");

    let active = client.get_backups();
    assert_eq!(active.len(), 1);
    assert!(!active.contains(&old_id), "Expired backup must be purged");
    assert!(active.contains(&new_id));
}

/// Archived backups survive automatic cleanup.
#[test]
fn test_archived_backups_survive_cleanup() {
    let (env, client, admin) = setup();
    let business = verified_business(&env, &client, &admin);

    create_invoice(&env, &client, &admin, &business, 1_000, "Invoice A");

    let archived_id = client.create_backup(&admin).expect("backup must succeed");
    client.archive_backup(&admin, &archived_id).expect("archive must succeed");

    client.set_backup_retention_policy(&admin, &1, &0, &true);
    env.ledger().set_timestamp(env.ledger().timestamp() + 1);
    let active_1 = client.create_backup(&admin).expect("backup must succeed");
    env.ledger().set_timestamp(env.ledger().timestamp() + 1);
    let active_2 = client.create_backup(&admin).expect("backup must succeed");

    let active = client.get_backups();
    assert_eq!(active.len(), 1);
    assert!(!active.contains(&active_1));
    assert!(active.contains(&active_2));

    // Archived backup still exists
    let archived = client.get_backup_details(&archived_id).unwrap();
    assert_eq!(archived.status, BackupStatus::Archived);
}

/// Manual cleanup returns 0 when auto_cleanup_enabled is false.
#[test]
fn test_manual_cleanup_disabled_returns_zero() {
    let (env, client, admin) = setup();
    let business = verified_business(&env, &client, &admin);

    create_invoice(&env, &client, &admin, &business, 1_000, "Invoice A");
    client.set_backup_retention_policy(&admin, &1, &0, &false);

    client.create_backup(&admin).expect("backup 1 must succeed");
    client.create_backup(&admin).expect("backup 2 must succeed");

    let removed = client.cleanup_backups(&admin).expect("cleanup must succeed");
    assert_eq!(removed, 0);
    assert_eq!(client.get_backups().len(), 2);
}

/// Manual cleanup with auto_cleanup_enabled enforces the policy.
#[test]
fn test_manual_cleanup_enforces_policy_when_enabled() {
    let (env, client, admin) = setup();
    let business = verified_business(&env, &client, &admin);

    create_invoice(&env, &client, &admin, &business, 1_000, "Invoice A");
    // Disable auto-cleanup so backups accumulate
    client.set_backup_retention_policy(&admin, &1, &0, &false);

    let id1 = client.create_backup(&admin).expect("backup 1 must succeed");
    env.ledger().set_timestamp(env.ledger().timestamp() + 1);
    let id2 = client.create_backup(&admin).expect("backup 2 must succeed");
    assert_eq!(client.get_backups().len(), 2);

    // Enable auto-cleanup and trigger manually
    client.set_backup_retention_policy(&admin, &1, &0, &true);
    let removed = client.cleanup_backups(&admin).expect("cleanup must succeed");
    assert_eq!(removed, 1);

    let active = client.get_backups();
    assert_eq!(active.len(), 1);
    assert!(!active.contains(&id1));
    assert!(active.contains(&id2));
}

// ============================================================================
// Admin-Only Access Tests
// ============================================================================

/// Backup creation requires admin authorization.
#[test]
fn test_create_backup_requires_admin() {
    let (env, client, admin) = setup();
    let business = verified_business(&env, &client, &admin);

    create_invoice(&env, &client, &admin, &business, 1_000, "Invoice A");

    let stranger = Address::generate(&env);
    let result = client.try_create_backup(&stranger);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().unwrap(), QuickLendXError::NotAdmin);
}

/// Backup restore requires admin authorization.
#[test]
fn test_restore_backup_requires_admin() {
    let (env, client, admin) = setup();
    let business = verified_business(&env, &client, &admin);

    create_invoice(&env, &client, &admin, &business, 1_000, "Invoice A");
    let backup_id = client.create_backup(&admin).expect("backup must succeed");

    let stranger = Address::generate(&env);
    let result = client.try_restore_backup(&stranger, &backup_id);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().unwrap(), QuickLendXError::NotAdmin);
}

/// Backup archival requires admin authorization.
#[test]
fn test_archive_backup_requires_admin() {
    let (env, client, admin) = setup();
    let business = verified_business(&env, &client, &admin);

    create_invoice(&env, &client, &admin, &business, 1_000, "Invoice A");
    let backup_id = client.create_backup(&admin).expect("backup must succeed");

    let stranger = Address::generate(&env);
    let result = client.try_archive_backup(&stranger, &backup_id);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().unwrap(), QuickLendXError::NotAdmin);
}

/// Manual cleanup requires admin authorization.
#[test]
fn test_cleanup_backups_requires_admin() {
    let (env, client, _admin) = setup();
    let stranger = Address::generate(&env);

    let result = client.try_cleanup_backups(&stranger);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().unwrap(), QuickLendXError::NotAdmin);
}

// ============================================================================
// Metadata Validation Tests
// ============================================================================

/// Tampered invoice_count is rejected during validation.
#[test]
fn test_validate_backup_rejects_tampered_count() {
    let (env, client, admin) = setup();
    let business = verified_business(&env, &client, &admin);

    create_invoice(&env, &client, &admin, &business, 1_000, "Invoice A");
    let backup_id = client.create_backup(&admin).expect("backup must succeed");
    assert!(client.validate_backup(&backup_id));

    let mut tampered = client.get_backup_details(&backup_id).unwrap();
    tampered.invoice_count = 999;
    env.storage().instance().set(&backup_id, &tampered);

    assert!(!client.validate_backup(&backup_id));
}

/// Backup with missing payload data is rejected.
#[test]
fn test_validate_backup_rejects_missing_payload() {
    let (env, client, admin) = setup();
    let business = verified_business(&env, &client, &admin);

    create_invoice(&env, &client, &admin, &business, 1_000, "Invoice A");
    let backup_id = client.create_backup(&admin).expect("backup must succeed");

    // Remove the payload data key
    let data_key = (soroban_sdk::symbol_short!("bkup_data"), backup_id.clone());
    env.storage().instance().remove(&data_key);

    assert!(!client.validate_backup(&backup_id));
}

/// Non-existent backup ID returns false from validate_backup.
#[test]
fn test_validate_backup_returns_false_for_nonexistent_id() {
    let (env, client, _admin) = setup();
    let fake_id = BytesN::from_array(&env, &[0xB4, 0xC4, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    assert!(!client.validate_backup(&fake_id));
}

// ============================================================================
// Payload Integrity Tests
// ============================================================================

/// Backup metadata invoice_count matches the actual payload length.
#[test]
fn test_backup_metadata_count_matches_payload() {
    let (env, client, admin) = setup();
    let business = verified_business(&env, &client, &admin);

    create_invoice(&env, &client, &admin, &business, 1_000, "Invoice 1");
    create_invoice(&env, &client, &admin, &business, 2_000, "Invoice 2");
    create_invoice(&env, &client, &admin, &business, 3_000, "Invoice 3");

    let backup_id = client.create_backup(&admin).expect("backup must succeed");
    let backup = client.get_backup_details(&backup_id).unwrap();

    assert_eq!(backup.invoice_count, 3);
    assert!(client.validate_backup(&backup_id));
}

/// Backup IDs are unique across multiple creations.
#[test]
fn test_backup_ids_are_unique() {
    let (env, client, admin) = setup();
    let business = verified_business(&env, &client, &admin);

    create_invoice(&env, &client, &admin, &business, 1_000, "Invoice A");

    let id1 = client.create_backup(&admin).expect("backup 1 must succeed");
    let id2 = client.create_backup(&admin).expect("backup 2 must succeed");
    let id3 = client.create_backup(&admin).expect("backup 3 must succeed");

    assert_ne!(id1, id2);
    assert_ne!(id2, id3);
    assert_ne!(id1, id3);
}

/// Backup list deduplication prevents duplicate entries.
#[test]
fn test_backup_list_deduplication() {
    let (env, client, admin) = setup();
    let business = verified_business(&env, &client, &admin);

    create_invoice(&env, &client, &admin, &business, 1_000, "Invoice A");

    let id1 = client.create_backup(&admin).expect("backup must succeed");
    let id2 = client.create_backup(&admin).expect("backup must succeed");

    // Manually add id2 again (simulating a bug)
    BackupStorage::add_to_backup_list(&env, &id2);

    // List should still have 2 entries (deduplication)
    let backups = client.get_backups();
    assert_eq!(backups.len(), 2);
    assert_eq!(backups.get(0).unwrap(), id1);
    assert_eq!(backups.get(1).unwrap(), id2);
}

/// Backup creation stores the correct status (Active).
#[test]
fn test_backup_creation_stores_active_status() {
    let (env, client, admin) = setup();
    let business = verified_business(&env, &client, &admin);

    create_invoice(&env, &client, &admin, &business, 1_000, "Invoice A");
    let backup_id = client.create_backup(&admin).expect("backup must succeed");

    let backup = client.get_backup_details(&backup_id).unwrap();
    assert_eq!(backup.status, BackupStatus::Active);
    assert!(BackupStorage::is_valid_backup_id(&backup_id));
}
