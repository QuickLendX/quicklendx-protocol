#![cfg(test)]
extern crate std;

use crate::{
    backup::BackupStatus,
    errors::QuickLendXError,
    invoice::InvoiceCategory,
    test::{
        create_funded_invoice, setup_env, setup_token, setup_verified_business,
        setup_verified_investor,
    },
    QuickLendXContract, QuickLendXContractClient,
};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, BytesN, Env, String, Vec,
};

fn setup() -> (Env, QuickLendXContractClient<'static>, Address) {
    let (env, client, admin, _) = setup_env();
    (env, client, admin)
}

#[test]
fn test_create_and_validate_backup() {
    let (env, client, admin) = setup();

    // Create an invoice so the state isn't totally empty
    let (_, _, _, _, _) = create_funded_invoice(&env, &client, &admin);

    // Only admin can create backup
    let stranger = Address::generate(&env);
    assert!(client.try_create_backup(&stranger).is_err());

    // Admin creates backup
    let backup_id = client.create_backup(&admin);

    // Validate the backup
    let is_valid = client.validate_backup(&backup_id);
    assert_eq!(is_valid, true);

    let backups = client.get_backups();
    assert_eq!(backups.len(), 1);
    assert_eq!(&backups.get(0).unwrap(), &backup_id);
}

#[test]
fn test_restore_backup() {
    let (env, client, admin) = setup();

    let business = setup_verified_business(&env, &client, &admin);
    let investor = setup_verified_investor(&env, &client, 50_000);
    let currency = setup_token(&env, &business, &investor, &client.address);

    let amount1 = 1_000i128;
    let due_date = env.ledger().timestamp() + 86400;

    // Create an invoice #1
    let invoice_id_1 = client.store_invoice(
        &business,
        &amount1,
        &currency,
        &due_date,
        &String::from_str(&env, "Invoice 1"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    client.verify_invoice(&invoice_id_1);

    // Initial state: 1 verified invoice
    assert_eq!(client.get_total_invoice_count(), 1);
    assert_eq!(client.get_invoice(&invoice_id_1).amount, 1_000i128);

    // Create a backup
    let backup_id = client.create_backup(&admin);

    // Change the state: Add a second invoice
    let amount2 = 2_000i128;
    let invoice_id_2 = client.store_invoice(
        &business,
        &amount2,
        &currency,
        &due_date,
        &String::from_str(&env, "Invoice 2"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    client.verify_invoice(&invoice_id_2);

    // State after change: 2 verified invoices
    assert_eq!(client.get_total_invoice_count(), 2);
    assert!(client.try_get_invoice(&invoice_id_2).is_ok());

    // Stranger cannot restore
    let stranger = Address::generate(&env);
    assert!(client.try_restore_backup(&stranger, &backup_id).is_err());

    // Admin restores backup
    client.restore_backup(&admin, &backup_id);

    // State should be reverted: only invoice #1 exists
    assert_eq!(client.get_total_invoice_count(), 1);
    assert!(client.try_get_invoice(&invoice_id_1).is_ok());
    assert!(client.try_get_invoice(&invoice_id_2).is_err());
}

#[test]
fn test_archive_backup() {
    let (env, client, admin) = setup();

    let backup_id = client.create_backup(&admin);
    assert_eq!(client.get_backups().len(), 1);

    // Archive backup
    client.archive_backup(&admin, &backup_id);

    // The backup should no longer be active (in the active list)
    assert_eq!(client.get_backups().len(), 0);
}

#[test]
fn test_backup_limit_cleanup() {
    let (env, client, admin) = setup();

    // Create 7 backups
    let mut backup_ids = Vec::new(&env);
    for _ in 0..7 {
        let id = client.create_backup(&admin);
        backup_ids.push_back(id);

        // Manipulate timestamp slightly to ensure sequential order in creation timestamp
        env.ledger().set_timestamp(env.ledger().timestamp() + 1);
    }

    let active_backups = client.get_backups();

    // Verify only the last 5 backups are kept
    assert_eq!(active_backups.len(), 5);

    // The first 2 should have been purged from the active list
    for i in 0..2 {
        let old_id = backup_ids.get(i).unwrap();
        assert!(!active_backups.contains(&old_id));
    }
}
