#![cfg(test)]

use crate::{
    backup::{Backup, BackupStatus, BackupStorage},
    storage::{InvoiceStorage, StorageIntegrityAudit},
    types::{DisputeStatus, Invoice, InvoiceCategory, InvoiceStatus},
    errors::QuickLendXError,
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

fn make_complex_invoice(
    env: &Env,
    idx: u32,
    business: &Address,
    status: InvoiceStatus,
    category: InvoiceCategory,
    tags: Vec<String>,
    customer_name: &str,
    tax_id: &str,
) -> Invoice {
    use crate::types::Dispute;

    let mut id_bytes = [0u8; 32];
    id_bytes[28..32].copy_from_slice(&idx.to_be_bytes());
    let id = BytesN::from_array(env, &id_bytes);

    Invoice {
        id,
        business: business.clone(),
        amount: 1_000,
        currency: Address::generate(env),
        due_date: 9_999_999_999,
        status,
        description: String::from_str(env, "complex invoice"),
        metadata_customer_name: Some(String::from_str(env, customer_name)),
        metadata_customer_address: Some(String::from_str(env, "123 Test St")),
        metadata_tax_id: Some(String::from_str(env, tax_id)),
        metadata_notes: Some(String::from_str(env, "Notes")),
        metadata_line_items: Vec::new(env),
        category,
        tags,
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
        },
        total_paid: 0,
        payment_history: Vec::new(env),
        created_at: env.ledger().timestamp(),
    }
}

fn create_valid_backup(env: &Env, invoices: Vec<Invoice>) -> BytesN<32> {
    let backup_id = BackupStorage::generate_backup_id(env);
    let count = invoices.len();

    let backup = Backup {
        backup_id: backup_id.clone(),
        timestamp: env.ledger().timestamp(),
        description: String::from_str(env, "complex backup"),
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
// Tests
// ============================================================================

#[test]
fn test_restore_rebuilds_all_indexes() {
    let env = setup_env();

    let business_a = Address::generate(&env);
    let business_b = Address::generate(&env);

    let mut tags_1 = Vec::new(&env);
    tags_1.push_back(String::from_str(&env, "urgent"));
    tags_1.push_back(String::from_str(&env, "tech"));

    let mut tags_2 = Vec::new(&env);
    tags_2.push_back(String::from_str(&env, "tech"));

    let inv_1 = make_complex_invoice(&env, 1, &business_a, InvoiceStatus::Pending, InvoiceCategory::Technology, tags_1, "Acme Corp", "TAX123");
    let inv_2 = make_complex_invoice(&env, 2, &business_a, InvoiceStatus::Funded, InvoiceCategory::Services, tags_2, "Acme Corp", "TAX123");
    let inv_3 = make_complex_invoice(&env, 3, &business_b, InvoiceStatus::Pending, InvoiceCategory::Manufacturing, Vec::new(&env), "Beta LLC", "TAX999");

    let mut invoices_to_backup = Vec::new(&env);
    invoices_to_backup.push_back(inv_1.clone());
    invoices_to_backup.push_back(inv_2.clone());
    invoices_to_backup.push_back(inv_3.clone());

    let backup_id = create_valid_backup(&env, invoices_to_backup);

    // Pre-populate state with some completely different data to ensure it's cleared
    let stale_inv = make_complex_invoice(&env, 99, &Address::generate(&env), InvoiceStatus::Verified, InvoiceCategory::Other, Vec::new(&env), "Stale", "TAX000");
    InvoiceStorage::store_invoice(&env, &stale_inv);

    // Perform restore
    let restored_count = BackupStorage::restore_from_backup(&env, &backup_id).unwrap();
    assert_eq!(restored_count, 3);

    // Assert get_by_business
    let bus_a_invoices = InvoiceStorage::get_by_business(&env, &business_a);
    assert_eq!(bus_a_invoices.len(), 2);
    assert!(bus_a_invoices.contains(&inv_1.id));
    assert!(bus_a_invoices.contains(&inv_2.id));

    let bus_b_invoices = InvoiceStorage::get_by_business(&env, &business_b);
    assert_eq!(bus_b_invoices.len(), 1);
    assert!(bus_b_invoices.contains(&inv_3.id));

    // Assert get_by_status
    let pending_invoices = InvoiceStorage::get_by_status(&env, InvoiceStatus::Pending);
    assert_eq!(pending_invoices.len(), 2);
    assert!(pending_invoices.contains(&inv_1.id));
    assert!(pending_invoices.contains(&inv_3.id));

    let funded_invoices = InvoiceStorage::get_by_status(&env, InvoiceStatus::Funded);
    assert_eq!(funded_invoices.len(), 1);
    assert!(funded_invoices.contains(&inv_2.id));

    let verified_invoices = InvoiceStorage::get_by_status(&env, InvoiceStatus::Verified);
    assert_eq!(verified_invoices.len(), 0); // Stale should be gone

    // Assert get_by_customer
    let acme_invoices = InvoiceStorage::get_by_customer(&env, &String::from_str(&env, "Acme Corp"));
    assert_eq!(acme_invoices.len(), 2);
    assert!(acme_invoices.contains(&inv_1.id));
    assert!(acme_invoices.contains(&inv_2.id));

    // Assert get_by_tax_id
    let tax_invoices = InvoiceStorage::get_by_tax_id(&env, &String::from_str(&env, "TAX123"));
    assert_eq!(tax_invoices.len(), 2);
    assert!(tax_invoices.contains(&inv_1.id));

    // Assert category
    let tech_cat = InvoiceStorage::get_by_category(&env, InvoiceCategory::Technology);
    assert_eq!(tech_cat.len(), 1);
    assert!(tech_cat.contains(&inv_1.id));

    let mfg_cat = InvoiceStorage::get_by_category(&env, InvoiceCategory::Manufacturing);
    assert_eq!(mfg_cat.len(), 1);
    assert!(mfg_cat.contains(&inv_3.id));

    // Assert tag
    let urgent_tags = InvoiceStorage::get_by_tag(&env, &String::from_str(&env, "urgent"));
    assert_eq!(urgent_tags.len(), 1);
    assert!(urgent_tags.contains(&inv_1.id));

    let tech_tags = InvoiceStorage::get_by_tag(&env, &String::from_str(&env, "tech"));
    assert_eq!(tech_tags.len(), 2);
    assert!(tech_tags.contains(&inv_1.id));
    assert!(tech_tags.contains(&inv_2.id));

    // Check no orphans
    let audit_result = StorageIntegrityAudit::audit_invoice_integrity(&env);
    assert!(audit_result.is_ok(), "Audit failed: {:?}", audit_result.err().unwrap());
}

#[test]
fn test_restore_idempotent_and_orphan_free() {
    let env = setup_env();

    let business_a = Address::generate(&env);
    let inv_1 = make_complex_invoice(&env, 1, &business_a, InvoiceStatus::Pending, InvoiceCategory::Technology, Vec::new(&env), "Acme Corp", "TAX123");

    let mut invoices_to_backup = Vec::new(&env);
    invoices_to_backup.push_back(inv_1.clone());

    let backup_id_1 = create_valid_backup(&env, invoices_to_backup.clone());

    // Restore first time
    BackupStorage::restore_from_backup(&env, &backup_id_1).unwrap();

    // Verify
    let bus_a_invoices = InvoiceStorage::get_by_business(&env, &business_a);
    assert_eq!(bus_a_invoices.len(), 1);

    // Attempting to restore the same backup fails because it's archived
    let result = BackupStorage::restore_from_backup(&env, &backup_id_1);
    assert_eq!(result, Err(QuickLendXError::OperationNotAllowed));

    // Let's create a NEW backup with the SAME data to prove idempotency of clearing/rebuilding
    let backup_id_2 = create_valid_backup(&env, invoices_to_backup);

    // Restore second time
    BackupStorage::restore_from_backup(&env, &backup_id_2).unwrap();

    // Still exactly 1 invoice
    let bus_a_invoices_after = InvoiceStorage::get_by_business(&env, &business_a);
    assert_eq!(bus_a_invoices_after.len(), 1);

    // No orphans
    let audit_result = StorageIntegrityAudit::audit_invoice_integrity(&env);
    assert!(audit_result.is_ok());
}

#[test]
fn test_unsupported_version_rejected() {
    let env = setup_env();

    let business_a = Address::generate(&env);
    let inv_1 = make_complex_invoice(&env, 1, &business_a, InvoiceStatus::Pending, InvoiceCategory::Technology, Vec::new(&env), "Acme Corp", "TAX123");

    // Populate valid state
    InvoiceStorage::store_invoice(&env, &inv_1);

    let backup_id = BackupStorage::generate_backup_id(&env);

    // Store V3
    use soroban_sdk::IntoVal;
    let mut map = soroban_sdk::Map::<soroban_sdk::Symbol, soroban_sdk::Val>::new(&env);
    map.set(soroban_sdk::Symbol::new(&env, "backup_id"), backup_id.clone().into_val(&env));
    map.set(soroban_sdk::Symbol::new(&env, "timestamp"), env.ledger().timestamp().into_val(&env));
    map.set(soroban_sdk::Symbol::new(&env, "description"), String::from_str(&env, "v3 backup").into_val(&env));
    map.set(soroban_sdk::Symbol::new(&env, "invoice_count"), 1u32.into_val(&env));
    map.set(soroban_sdk::Symbol::new(&env, "status"), BackupStatus::Active.into_val(&env));
    map.set(soroban_sdk::Symbol::new(&env, "format_version"), 3u32.into_val(&env));

    env.storage().instance().set(&backup_id, &map);

    let rest_result = BackupStorage::restore_from_backup(&env, &backup_id);
    assert_eq!(rest_result, Err(QuickLendXError::BackupVersionUnsupported));

    // State is untouched
    let bus_a_invoices = InvoiceStorage::get_by_business(&env, &business_a);
    assert_eq!(bus_a_invoices.len(), 1);
    assert!(bus_a_invoices.contains(&inv_1.id));
}
