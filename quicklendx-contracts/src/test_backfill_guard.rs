//! Negative tests for the `require_no_pending_backfill` guard (Issue #1847).
//!
//! Threat model: `restore_from_backup` performs a destructive wipe + rebuild
//! of invoice state (clear_all followed by per-invoice store_invoice). If a
//! WASM upgrade lands while that sequence is mid-flight, the new contract
//! code comes online reading partially-restored state with no signal that
//! the view is partial — every secondary index rebuilt in step 3 may be
//! missing data the new code's invariants assume.
//!
//! These tests pin the guard: while the `PENDING_BACKFILL_KEY` flag is set
//! by `restore_from_backup`, `schedule_upgrade` must refuse with
//! `BackfillInProgress`. Once the flag clears (end of the restore), the
//! guard releases and the same call succeeds.

#![cfg(test)]

use crate::errors::QuickLendXError;
use soroban_sdk::{testutils::Address as _, Address, BytesN, Env, Wasm};

fn setup() -> (Env, Address, BytesN<32>) {
    let env = Env::default();
    let admin = Address::generate(&env);
    let wasm_hash = env.deployer().upload_contract_wasm(Wasm::from(&[]));
    crate::admin::AdminStorage::set_admin(&env, &admin);
    (env, admin, wasm_hash)
}

/// When no backfill is in flight, the guard is transparent: `schedule_upgrade`
/// succeeds exactly as before. This is the no-regression anchor.
#[test]
fn schedule_upgrade_succeeds_when_no_backfill_pending() {
    let (env, admin, wasm_hash) = setup();
    assert!(!crate::backup::BackupStorage::is_pending_backfill(&env));

    crate::upgrade::UpgradeControl::schedule_upgrade(&env, &admin, &wasm_hash).unwrap();
    assert!(crate::upgrade::UpgradeControl::is_pending_upgrade(&env));

    // Clean up so this test does not leak pause state to later tests.
    crate::upgrade::UpgradeControl::cancel_upgrade(&env, &admin).unwrap();
}

/// While the backfill flag is set, `schedule_upgrade` is refused. The guard
/// returns `BackfillInProgress` — distinct from `OperationNotAllowed` so
/// monitoring can tell "an upgrade is already pending" apart from "a
/// backfill raced with you".
#[test]
fn schedule_upgrade_rejected_when_backfill_pending() {
    let (env, admin, wasm_hash) = setup();

    // Simulate the in-progress flag the same way restore_from_backup sets it:
    // via the typed helper that other paths use.
    env.storage()
        .instance()
        .set(&crate::backup::PENDING_BACKFILL_KEY, &true);

    assert!(crate::backup::BackupStorage::is_pending_backfill(&env));

    let err =
        crate::upgrade::UpgradeControl::schedule_upgrade(&env, &admin, &wasm_hash).unwrap_err();
    assert_eq!(err, QuickLendXError::BackfillInProgress);

    // No upgrade was scheduled (storage has no PENDING_UPGRADE_WASM_KEY).
    assert!(!crate::upgrade::UpgradeControl::is_pending_upgrade(&env));
}

/// The guard releases as soon as the flag clears — proves the opt-out path
/// exists, so an in-flight backfill that aborts (or simply finishes) does
/// not permanently lock the contract out of upgrades.
#[test]
fn schedule_upgrade_releases_after_backfill_flag_cleared() {
    let (env, admin, wasm_hash) = setup();

    env.storage()
        .instance()
        .set(&crate::backup::PENDING_BACKFILL_KEY, &true);
    let err1 =
        crate::upgrade::UpgradeControl::schedule_upgrade(&env, &admin, &wasm_hash).unwrap_err();
    assert_eq!(err1, QuickLendXError::BackfillInProgress);

    env.storage()
        .instance()
        .remove(&crate::backup::PENDING_BACKFILL_KEY);

    crate::upgrade::UpgradeControl::schedule_upgrade(&env, &admin, &wasm_hash).unwrap();
    assert!(crate::upgrade::UpgradeControl::is_pending_upgrade(&env));

    crate::upgrade::UpgradeControl::cancel_upgrade(&env, &admin).unwrap();
}

/// Round-trip: the public `restore_from_backup` entry point must set the
/// flag while running and clear it once done, even when the restore succeeds.
/// This is the positive half of the integration test.
#[test]
fn restore_from_backup_clears_pending_flag_after_success() {
    use crate::backup::{Backup, BackupStatus, BackupStorage};
    use crate::types::{Dispute, Invoice, InvoiceCategory, InvoiceStatus, DisputeStatus};
    use soroban_sdk::{String as SdkString, Vec};

    let env = Env::default();
    env.mock_all_auths();

    let business = Address::generate(&env);
    let mut invoices: Vec<Invoice> = Vec::new(&env);

    let mut id_bytes = [0u8; 32];
    id_bytes[28..32].copy_from_slice(&1u32.to_be_bytes());
    let invoice_id = BytesN::from_array(&env, &id_bytes);
    let invoice = Invoice {
        id: invoice_id,
        business: business.clone(),
        amount: 1_000,
        currency: Address::generate(&env),
        due_date: env.ledger().timestamp() + 86_400,
        status: InvoiceStatus::Pending,
        description: SdkString::from_str(&env, "guard test invoice"),
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
            reason: SdkString::from_str(&env, ""),
            evidence: SdkString::from_str(&env, ""),
            resolution: SdkString::from_str(&env, ""),
            resolved_by: Address::generate(&env),
            resolved_at: 0,
            resolution_outcome: crate::types::DisputeResolution::None,
        },
        total_paid: 0,
        payment_history: Vec::new(&env),
        created_at: env.ledger().timestamp(),
        origination_fee_bps: None,
        late_payment_penalty_bps: None,
        early_payment_discount_bps: None,
    };
    invoices.push_back(invoice);

    let backup_id = BackupStorage::generate_backup_id(&env);
    let backup = Backup {
        backup_id: backup_id.clone(),
        timestamp: env.ledger().timestamp(),
        description: SdkString::from_str(&env, "guard round-trip"),
        invoice_count: invoices.len(),
        status: BackupStatus::Active,
        format_version: 2,
    };
    BackupStorage::store_backup(&env, &backup, Some(&invoices)).unwrap();
    BackupStorage::store_backup_data(&env, &backup_id, &invoices);
    BackupStorage::add_to_backup_list(&env, &backup_id);

    let _restored = BackupStorage::restore_from_backup(&env, &backup_id).unwrap();

    // After completion the flag must be gone, regardless of success or
    // failure — otherwise the contract would be permanently locked out of
    // upgrades.
    assert!(!BackupStorage::is_pending_backfill(&env));
}
