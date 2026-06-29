#![cfg(test)]

//! # Backup Retention-Policy Enforcement Tests (Issue #1353)
//!
//! Focused unit tests that pin the retention behaviour of
//! [`BackupStorage::cleanup_old_backups`] at the storage layer.
//!
//! ## Retention invariant
//!
//! For a policy `{ max_backups, max_age_seconds, auto_cleanup_enabled }`:
//!
//! 1. When `auto_cleanup_enabled == false`, cleanup is a no-op (returns 0 and
//!    purges nothing) regardless of the count/age of stored backups.
//! 2. When enabled and `max_age_seconds > 0`, every *active* backup whose age
//!    is **strictly greater** than `max_age_seconds` is purged; a backup whose
//!    age is exactly `max_age_seconds` is kept (the boundary is inclusive).
//! 3. When enabled and `max_backups > 0`, after the age pass the oldest active
//!    backups are purged until at most `max_backups` remain; cleanup never
//!    purges below that floor and always keeps the newest.
//! 4. A purge removes the backup record, its data payload, and its list entry,
//!    leaving no orphan keys.
//! 5. `Archived` backups are excluded from both passes and are never purged,
//!    and archiving marks a backup `Archived` without deleting it.

use crate::backup::{Backup, BackupRetentionPolicy, BackupStatus, BackupStorage};
use crate::types::{
    Dispute, DisputeResolution, DisputeStatus, Invoice, InvoiceCategory, InvoiceStatus,
};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, BytesN, Env, String, Vec,
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

/// Build a minimal valid Invoice for backup payloads.
fn make_invoice(env: &Env, idx: u32, amount: i128) -> Invoice {
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
        description: String::from_str(env, "retention test"),
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

/// Create and persist a valid active backup (metadata + data + list entry)
/// stamped at the current ledger time, and return its ID.
fn create_backup(env: &Env) -> BytesN<32> {
    let backup_id = BackupStorage::generate_backup_id(env);
    let mut invoices = Vec::new(env);
    invoices.push_back(make_invoice(env, 0, 1_000));

    let backup = Backup {
        backup_id: backup_id.clone(),
        timestamp: env.ledger().timestamp(),
        description: String::from_str(env, "retention enforcement backup"),
        invoice_count: invoices.len(),
        status: BackupStatus::Active,
        format_version: 2,
    };

    BackupStorage::store_backup(env, &backup, Some(&invoices)).unwrap();
    BackupStorage::store_backup_data(env, &backup_id, &invoices);
    BackupStorage::add_to_backup_list(env, &backup_id);
    backup_id
}

fn advance(env: &Env, seconds: u64) {
    env.ledger().set_timestamp(env.ledger().timestamp() + seconds);
}

fn set_policy(env: &Env, max_backups: u32, max_age_seconds: u64, auto_cleanup_enabled: bool) {
    BackupStorage::set_retention_policy(
        env,
        &BackupRetentionPolicy {
            max_backups,
            max_age_seconds,
            auto_cleanup_enabled,
        },
    );
}

/// Assert that a purged backup leaves no orphan record, payload, or list entry.
fn assert_no_orphan(env: &Env, backup_id: &BytesN<32>) {
    assert!(
        BackupStorage::get_backup(env, backup_id).is_none(),
        "purged backup record should be gone"
    );
    assert!(
        BackupStorage::get_backup_data(env, backup_id).is_none(),
        "purged backup payload should be gone"
    );
    assert!(
        !BackupStorage::get_all_backups(env).contains(backup_id),
        "purged backup should not remain in the list"
    );
}

// ============================================================================
// auto_cleanup_enabled gating
// ============================================================================

/// With `auto_cleanup_enabled == false`, cleanup purges nothing even when both
/// the count and age limits are exceeded.
#[test]
fn test_disabled_cleanup_is_a_noop() {
    let env = setup_env();
    set_policy(&env, 1, 10, false);

    let id1 = create_backup(&env);
    advance(&env, 1_000); // well past max_age
    let id2 = create_backup(&env);

    let removed = BackupStorage::cleanup_old_backups(&env).unwrap();
    assert_eq!(removed, 0);

    let remaining = BackupStorage::get_all_backups(&env);
    assert_eq!(remaining.len(), 2);
    assert!(remaining.contains(&id1));
    assert!(remaining.contains(&id2));
}

// ============================================================================
// Count-limit enforcement
// ============================================================================

/// Count cleanup purges the oldest backups down to `max_backups`, keeping the
/// newest, and leaves no orphan data behind.
#[test]
fn test_count_limit_purges_oldest_keeps_newest() {
    let env = setup_env();
    set_policy(&env, 2, 0, true);

    let id1 = create_backup(&env);
    advance(&env, 1);
    let id2 = create_backup(&env);
    advance(&env, 1);
    let id3 = create_backup(&env);
    advance(&env, 1);
    let id4 = create_backup(&env);

    let removed = BackupStorage::cleanup_old_backups(&env).unwrap();
    assert_eq!(removed, 2);

    let remaining = BackupStorage::get_all_backups(&env);
    assert_eq!(remaining.len(), 2);
    assert!(remaining.contains(&id3));
    assert!(remaining.contains(&id4));

    // Oldest two purged with no orphan keys.
    assert_no_orphan(&env, &id1);
    assert_no_orphan(&env, &id2);
}

/// Exactly `max_backups` backups: count cleanup must not purge below the floor.
#[test]
fn test_count_limit_at_exact_capacity_keeps_all() {
    let env = setup_env();
    set_policy(&env, 3, 0, true);

    let id1 = create_backup(&env);
    advance(&env, 1);
    let id2 = create_backup(&env);
    advance(&env, 1);
    let id3 = create_backup(&env);

    let removed = BackupStorage::cleanup_old_backups(&env).unwrap();
    assert_eq!(removed, 0);

    let remaining = BackupStorage::get_all_backups(&env);
    assert_eq!(remaining.len(), 3);
    assert!(remaining.contains(&id1));
    assert!(remaining.contains(&id2));
    assert!(remaining.contains(&id3));
}

// ============================================================================
// Age-limit enforcement (boundary at exactly max_age_seconds)
// ============================================================================

/// A backup whose age is exactly `max_age_seconds` is kept (inclusive
/// boundary); one strictly older is purged.
#[test]
fn test_age_limit_boundary_is_inclusive() {
    let env = setup_env();
    set_policy(&env, 0, 100, true);

    // older_id will be 101s old (purged); boundary_id exactly 100s (kept).
    let older_id = create_backup(&env);
    advance(&env, 1);
    let boundary_id = create_backup(&env);
    advance(&env, 100);

    // Now: older_id age = 101 > 100 -> purged; boundary_id age = 100 == 100 -> kept.
    let removed = BackupStorage::cleanup_old_backups(&env).unwrap();
    assert_eq!(removed, 1);

    let remaining = BackupStorage::get_all_backups(&env);
    assert_eq!(remaining.len(), 1);
    assert!(remaining.contains(&boundary_id));
    assert_no_orphan(&env, &older_id);
}

// ============================================================================
// Empty / no-op edge cases
// ============================================================================

/// Cleanup with zero stored backups is a safe no-op.
#[test]
fn test_cleanup_with_zero_backups() {
    let env = setup_env();
    set_policy(&env, 2, 100, true);

    let removed = BackupStorage::cleanup_old_backups(&env).unwrap();
    assert_eq!(removed, 0);
    assert_eq!(BackupStorage::get_all_backups(&env).len(), 0);
}

// ============================================================================
// Archive lifecycle
// ============================================================================

/// `Archived` backups are excluded from cleanup and are never purged, while
/// archiving marks the record `Archived` without deleting it.
#[test]
fn test_archive_marks_without_deleting_and_is_excluded() {
    let env = setup_env();
    set_policy(&env, 1, 0, true);

    // Oldest backup, then archive it.
    let archived_id = create_backup(&env);
    let mut backup = BackupStorage::get_backup(&env, &archived_id).unwrap();
    backup.status = BackupStatus::Archived;
    BackupStorage::update_backup(&env, &backup).unwrap();

    // archive marks Archived without deleting record or payload.
    let after = BackupStorage::get_backup(&env, &archived_id).unwrap();
    assert_eq!(after.status, BackupStatus::Archived);
    assert!(BackupStorage::get_backup_data(&env, &archived_id).is_some());

    advance(&env, 1);
    let active_old = create_backup(&env);
    advance(&env, 1);
    let active_new = create_backup(&env);

    // Two active backups with max_backups = 1 -> the oldest active is purged.
    // The archived backup is invisible to cleanup and survives.
    let removed = BackupStorage::cleanup_old_backups(&env).unwrap();
    assert_eq!(removed, 1);

    let remaining = BackupStorage::get_all_backups(&env);
    assert!(remaining.contains(&archived_id));
    assert!(remaining.contains(&active_new));
    assert!(!remaining.contains(&active_old));
    assert_no_orphan(&env, &active_old);

    // Archived backup remains fully intact.
    assert_eq!(
        BackupStorage::get_backup(&env, &archived_id).unwrap().status,
        BackupStatus::Archived
    );
}
