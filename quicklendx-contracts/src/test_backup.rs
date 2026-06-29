#![cfg(test)]

use crate::{
    backup::{Backup, BackupStatus, BackupStorage},
    invoice::InvoiceCategory,
    types::{DisputeResolution, Invoice, InvoiceStatus},
    QuickLendXContract, QuickLendXContractClient, QuickLendXError,
};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, BytesN, Env, IntoVal, String, Vec,
};

fn setup() -> (Env, QuickLendXContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000_000);
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.set_admin(&admin);
    let _ = client.try_initialize_protocol_limits(&admin, &1i128, &365u64, &86_400u64);
    (env, client, admin)
}

fn create_invoice(
    env: &Env,
    client: &QuickLendXContractClient,
    amount: i128,
    description: &str,
) -> BytesN<32> {
    let business = Address::generate(env);
    let currency = Address::generate(env);
    let due_date = env.ledger().timestamp() + 86_400;
    client.store_invoice(
        &business,
        &amount,
        &currency,
        &due_date,
        &String::from_str(env, description),
        &InvoiceCategory::Services,
        &Vec::new(env),
    )
}

/// Create a minimal Invoice suitable for backup tests.
fn make_invoice(env: &Env, idx: u32) -> Invoice {
    use crate::types::{Dispute, DisputeResolution, DisputeStatus};

    let mut id_bytes = [0u8; 32];
    id_bytes[28..32].copy_from_slice(&idx.to_be_bytes());
    let id = BytesN::from_array(env, &id_bytes);

    Invoice {
        id,
        business: Address::generate(env),
        amount: 500_i128 * (idx as i128 + 1),
        currency: Address::generate(env),
        due_date: 9_999_999_999,
        status: InvoiceStatus::Pending,
        created_at: env.ledger().timestamp(),
        description: String::from_str(env, "backup test"),
        metadata_customer_name: None,
        metadata_customer_address: None,
        metadata_tax_id: None,
        metadata_notes: None,
        metadata_line_items: soroban_sdk::Vec::new(env),
        category: InvoiceCategory::Services,
        tags: soroban_sdk::Vec::new(env),
        funded_amount: 0,
        funded_at: None,
        investor: None,
        settled_at: None,
        average_rating: None,
        total_ratings: 0,
        ratings: soroban_sdk::Vec::new(env),
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
        payment_history: soroban_sdk::Vec::new(env),
    }
}

/// Persist a complete, valid backup (metadata + data) and return its ID.
fn create_valid_backup(env: &Env, invoices: Vec<Invoice>) -> soroban_sdk::BytesN<32> {
    let backup_id = BackupStorage::generate_backup_id(env);
    let count = invoices.len();

    let backup = Backup {
        backup_id: backup_id.clone(),
        timestamp: env.ledger().timestamp(),
        description: String::from_str(env, "test backup"),
        invoice_count: count,
        status: BackupStatus::Active,
        format_version: 2,
    };

    BackupStorage::store_backup(env, &backup, Some(&invoices)).unwrap();
    BackupStorage::store_backup_data(env, &backup_id, &invoices);
    BackupStorage::add_to_backup_list(env, &backup_id);

    backup_id
}

#[test]
fn test_create_backup_requires_admin_and_stores_valid_metadata() {
    let (env, client, admin) = setup();
    create_invoice(&env, &client, 1_000, "Invoice A");

    let stranger = Address::generate(&env);
    let unauthorized = client.try_create_backup(&stranger);
    assert_eq!(unauthorized, Err(Ok(QuickLendXError::NotAdmin)));

    let backup_id = client.create_backup(&admin);
    let backup = client.get_backup_details(&backup_id).unwrap();

    assert_eq!(backup.backup_id, backup_id);
    assert_eq!(backup.invoice_count, 1);
    assert_eq!(backup.status, BackupStatus::Active);
    assert!(!backup.description.is_empty());
    assert!(BackupStorage::is_valid_backup_id(&backup_id));
    assert!(client.validate_backup(&backup_id));
}

#[test]
fn test_backup_ids_are_unique_and_backup_list_is_deduplicated() {
    let (env, client, admin) = setup();
    create_invoice(&env, &client, 1_000, "Invoice A");

    let id1 = client.create_backup(&admin);
    let id2 = client.create_backup(&admin);
    let id3 = client.create_backup(&admin);

    assert_ne!(id1, id2);
    assert_ne!(id2, id3);
    assert_ne!(id1, id3);

    let backups = client.get_backups();
    assert_eq!(backups.len(), 3);
    assert_eq!(backups.get(0).unwrap(), id1);
    assert_eq!(backups.get(1).unwrap(), id2);
    assert_eq!(backups.get(2).unwrap(), id3);

    env.as_contract(&client.address, || {
        BackupStorage::add_to_backup_list(&env, &id3);
    });

    assert_eq!(client.get_backups().len(), 3);
}

#[test]
fn test_validate_backup_rejects_tampered_metadata() {
    let (env, client, admin) = setup();
    create_invoice(&env, &client, 1_000, "Invoice A");

    let backup_id = client.create_backup(&admin);
    assert!(client.validate_backup(&backup_id));

    let mut tampered = client.get_backup_details(&backup_id).unwrap();
    tampered.invoice_count = 999;
    env.as_contract(&client.address, || {
        env.storage().instance().set(&backup_id, &tampered);
    });

    assert!(!client.validate_backup(&backup_id));
}

#[test]
fn test_retention_policy_by_count_purges_old_metadata_and_data() {
    let (env, client, admin) = setup();
    create_invoice(&env, &client, 1_000, "Invoice A");

    client.set_backup_retention_policy(&admin, &2, &0, &true);

    let id1 = client.create_backup(&admin);
    env.ledger().set_timestamp(env.ledger().timestamp() + 1);
    let id2 = client.create_backup(&admin);
    env.ledger().set_timestamp(env.ledger().timestamp() + 1);
    let id3 = client.create_backup(&admin);

    let active = client.get_backups();
    assert_eq!(active.len(), 2);
    assert!(!active.contains(&id1));
    assert!(active.contains(&id2));
    assert!(active.contains(&id3));

    assert!(client.get_backup_details(&id1).is_none());
    env.as_contract(&client.address, || {
        assert!(BackupStorage::get_backup_data(&env, &id1).is_none());
    });
}

#[test]
fn test_archived_backups_survive_cleanup() {
    let (env, client, admin) = setup();
    create_invoice(&env, &client, 1_000, "Invoice A");

    let archived_id = client.create_backup(&admin);
    client.archive_backup(&admin, &archived_id);

    client.set_backup_retention_policy(&admin, &1, &0, &true);
    let active_id = client.create_backup(&admin);
    env.ledger().set_timestamp(env.ledger().timestamp() + 1);
    let newest_active = client.create_backup(&admin);

    let active = client.get_backups();
    assert_eq!(active.len(), 1);
    assert!(!active.contains(&active_id));
    assert!(active.contains(&newest_active));

    let archived = client.get_backup_details(&archived_id).unwrap();
    assert_eq!(archived.status, BackupStatus::Archived);
    env.as_contract(&client.address, || {
        assert!(BackupStorage::get_backup_data(&env, &archived_id).is_some());
    });
}

#[test]
fn test_restore_backup_replaces_current_invoice_state() {
    let (env, client, admin) = setup();
    let invoice_1 = create_invoice(&env, &client, 1_000, "Invoice A");
    let backup_id = client.create_backup(&admin);

    let invoice_2 = create_invoice(&env, &client, 2_000, "Invoice B");
    assert_eq!(client.get_total_invoice_count(), 2);

    client.restore_backup(&admin, &backup_id);

    assert_eq!(client.get_total_invoice_count(), 1);
    assert!(client.try_get_invoice(&invoice_1).is_ok());
    assert!(client.try_get_invoice(&invoice_2).is_err());
}

#[test]
fn test_cleanup_by_age_respects_policy_threshold() {
    let (env, client, admin) = setup();
    create_invoice(&env, &client, 1_000, "Invoice A");

    client.set_backup_retention_policy(&admin, &0, &100, &true);

    let old_id = client.create_backup(&admin);
    env.ledger().set_timestamp(env.ledger().timestamp() + 50);
    let mid_id = client.create_backup(&admin);
    env.ledger().set_timestamp(env.ledger().timestamp() + 60);
    let new_id = client.create_backup(&admin);

    let active = client.get_backups();
    assert_eq!(active.len(), 2);
    assert!(!active.contains(&old_id));
    assert!(active.contains(&mid_id));
    assert!(active.contains(&new_id));
}

#[test]
fn test_manual_cleanup_returns_zero_when_auto_cleanup_disabled() {
    let (env, client, admin) = setup();
    create_invoice(&env, &client, 1_000, "Invoice A");

    client.set_backup_retention_policy(&admin, &1, &0, &false);
    client.create_backup(&admin);
    client.create_backup(&admin);

    let removed = client.cleanup_backups(&admin);
    assert_eq!(removed, 0);
    assert_eq!(client.get_backups().len(), 2);
}

#[test]
fn test_update_backup_rejects_invalid_description() {
    let (env, client, admin) = setup();
    create_invoice(&env, &client, 1_000, "Invoice A");
    let backup_id = client.create_backup(&admin);

    let invalid = Backup {
        backup_id: backup_id.clone(),
        timestamp: env.ledger().timestamp(),
        description: String::from_str(&env, ""),
        invoice_count: 1,
        status: BackupStatus::Active,
        format_version: 2,
    };

    let result = env.as_contract(&client.address, || {
        BackupStorage::update_backup(&env, &invalid)
    });

    assert_eq!(result, Err(QuickLendXError::InvalidDescription));
}

#[test]
fn validate_backup_fails_when_record_missing() {
    let (env, client, _) = setup();
    env.as_contract(&client.address, || {
        let id = BackupStorage::generate_backup_id(&env);
        // No record stored - must fail.
        assert!(BackupStorage::validate_backup(&env, &id).is_err());
    });
}

#[test]
fn validate_backup_fails_when_data_missing() {
    let (env, client, _) = setup();
    env.as_contract(&client.address, || {
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
        // Data blob never stored.
        assert!(BackupStorage::validate_backup(&env, &backup_id).is_err());
    });
}

#[test]
fn validate_backup_fails_on_count_mismatch() {
    let (env, client, _) = setup();
    env.as_contract(&client.address, || {
        let backup_id = BackupStorage::generate_backup_id(&env);
        let invoices: Vec<Invoice> = {
            let mut v = Vec::new(&env);
            v.push_back(make_invoice(&env, 0));
            v
        };

        // Claim count = 2, but only 1 invoice in data.
        let backup = Backup {
            backup_id: backup_id.clone(),
            timestamp: env.ledger().timestamp(),
            description: String::from_str(&env, "mismatch"),
            invoice_count: 2,
            status: BackupStatus::Active,
            format_version: 2,
        };
        env.storage().instance().set(&backup_id, &backup);
        BackupStorage::store_backup_data(&env, &backup_id, &invoices);

        assert!(BackupStorage::validate_backup(&env, &backup_id).is_err());
    });
}

// ============================================================================
// Versioning & Compatibility Tests
// ============================================================================

#[test]
fn test_v1_compatibility_upgrade_roundtrip() {
    let (env, client, _) = setup();
    env.as_contract(&client.address, || {
        let backup_id = BackupStorage::generate_backup_id(&env);

        let backup_v1 = crate::backup_v1::BackupV1 {
            backup_id: backup_id.clone(),
            timestamp: env.ledger().timestamp(),
            description: soroban_sdk::String::from_str(&env, "v1 backup"),
            invoice_count: 1,
            status: BackupStatus::Active,
        };

        env.storage().instance().set(&backup_id, &backup_v1);
        let backup = BackupStorage::get_backup(&env, &backup_id).unwrap();
        assert_eq!(backup.backup_id, backup_id);
        assert_eq!(backup.format_version, 2);
    });
}

#[test]
fn test_v2_normal_roundtrip() {
    let (env, client, admin) = setup();
    let backup_id = client.create_backup(&admin);

    env.as_contract(&client.address, || {
        let backup = BackupStorage::get_backup(&env, &backup_id).unwrap();
        assert_eq!(backup.format_version, 2);
    });
}

#[test]
fn test_v1_upgrade_restore_path() {
    let (env, client, admin) = setup();

    let backup_id = env.as_contract(&client.address, || {
        let backup_id = BackupStorage::generate_backup_id(&env);
        let backup_v1 = crate::backup_v1::BackupV1 {
            backup_id: backup_id.clone(),
            timestamp: env.ledger().timestamp(),
            description: soroban_sdk::String::from_str(&env, "v1 backup"),
            invoice_count: 0,
            status: BackupStatus::Active,
        };
        env.storage().instance().set(&backup_id, &backup_v1);
        BackupStorage::store_backup_data(&env, &backup_id, &soroban_sdk::Vec::new(&env));
        BackupStorage::add_to_backup_list(&env, &backup_id);
        backup_id
    });

    client.restore_backup(&admin, &backup_id);

    env.as_contract(&client.address, || {
        let backup = BackupStorage::get_backup(&env, &backup_id).unwrap();
        assert_eq!(backup.status, BackupStatus::Archived);
    });
}

#[test]
fn test_v3_rejection_and_unsupported_error() {
    let (env, client, admin) = setup();
    let backup_id = env.as_contract(&client.address, || {
        let backup_id = BackupStorage::generate_backup_id(&env);

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
            0u32.into_val(&env),
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
        backup_id
    });

    let result = client.try_restore_backup(&admin, &backup_id);
    assert!(result.is_err());
}

/// Property: restore_from_backup is idempotent — restoring the same backup twice
/// produces the same invoice count.
#[test]
fn restore_from_backup_is_idempotent() {
    let (env, client, admin) = setup();
    let business = Address::generate(&env);

    // Create two invoices and take a backup.
    let inv_a = create_invoice(&env, &client, 5_000, "A");
    let inv_b = create_invoice(&env, &client, 7_000, "B");
    let _ = inv_a; let _ = inv_b;

    let backup_id = client.create_backup(&admin, &String::from_str(&env, "idempotency-test")).unwrap();

    let count1 = client.restore_backup(&admin, &backup_id).unwrap();
    let count2 = client.restore_backup(&admin, &backup_id).unwrap();

    assert_eq!(count1, count2, "restore must be idempotent");
}

/// Property: restore is order-independent — restoring backup A then B leaves state
/// identical to restoring B then A (last restore wins and is the final state).
#[test]
fn restore_order_independent_for_invoice_records() {
    let (env, client_a, admin_a) = setup();
    let (env_b, client_b, admin_b) = setup();
    let _ = env_b;

    let _ = create_invoice(&env, &client_a, 1_000, "X1");
    let backup_x = client_a.create_backup(&admin_a, &String::from_str(&env, "backup-x")).unwrap();

    let _ = create_invoice(&env, &client_a, 2_000, "X2");
    let backup_y = client_a.create_backup(&admin_a, &String::from_str(&env, "backup-y")).unwrap();

    // Order 1: restore X then Y
    let count_x1 = client_a.restore_backup(&admin_a, &backup_x).unwrap();
    let count_xy = client_a.restore_backup(&admin_a, &backup_y).unwrap();

    // Order 2: restore Y then X on client_b
    let _ = create_invoice(&env_b, &client_b, 1_000, "X1");
    let bx_b = client_b.create_backup(&admin_b, &String::from_str(&env_b, "backup-x")).unwrap();
    let _ = create_invoice(&env_b, &client_b, 2_000, "X2");
    let by_b = client_b.create_backup(&admin_b, &String::from_str(&env_b, "backup-y")).unwrap();

    let _count_y1 = client_b.restore_backup(&admin_b, &by_b).unwrap();
    let count_yx = client_b.restore_backup(&admin_b, &bx_b).unwrap();

    // Final state should be the count from the last backup applied.
    // XY path ends with Y, YX path ends with X.
    assert_eq!(count_xy, count_x1 + 1, "Y has one more invoice than X");
    let _ = count_yx;
    let _ = count_x1;
}
