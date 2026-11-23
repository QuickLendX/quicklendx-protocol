use quicklendx_contracts::{
    backup::{BackupStatus, BackupStorage},
    errors::QuickLendXError,
    invoice::InvoiceCategory,
    QuickLendXContract, QuickLendXContractClient,
};
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::{Address, BytesN, Env, String, Vec};

fn setup() -> (Env, QuickLendXContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000_000);
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.set_admin(&admin);
    (env, client, admin)
}

fn create_invoice(env: &Env, client: &QuickLendXContractClient, amount: i128) {
    let business = Address::generate(env);
    let currency = Address::generate(env);
    let due_date = env.ledger().timestamp() + 86_400;
    client.store_invoice(
        &business,
        &amount,
        &currency,
        &due_date,
        &String::from_str(env, "backup validation fixture"),
        &InvoiceCategory::Services,
        &Vec::new(env),
        &None,
    );
}

#[test]
fn retention_policy_prunes_oldest_active_backup_and_preserves_newest() {
    let (env, client, admin) = setup();
    create_invoice(&env, &client, 1_000);
    client.set_backup_retention_policy(&admin, &2, &0, &true);

    let first = client.create_backup(&admin);
    env.ledger().set_timestamp(env.ledger().timestamp() + 1);
    let second = client.create_backup(&admin);
    env.ledger().set_timestamp(env.ledger().timestamp() + 1);
    let third = client.create_backup(&admin);

    assert!(client.get_backup_details(&first).is_none());
    assert_eq!(client.get_backups().len(), 2);
    assert!(client.get_backups().contains(&second));
    assert!(client.get_backups().contains(&third));
    assert!(client.validate_backup(&second));
    assert!(client.validate_backup(&third));
}

#[test]
fn backup_validation_rejects_tampered_metadata_without_mutating_invoices() {
    let (env, client, admin) = setup();
    create_invoice(&env, &client, 1_000);
    let backup_id = client.create_backup(&admin);
    assert!(client.validate_backup(&backup_id));

    env.as_contract(&client.address, || {
        let mut backup = BackupStorage::get_backup(&env, &backup_id).unwrap();
        backup.invoice_count = 999;
        BackupStorage::update_backup(&env, &backup).unwrap();
        assert!(BackupStorage::validate_backup(&env, &backup_id).is_err());
    });
    assert!(!client.validate_backup(&backup_id));
    assert_eq!(client.get_total_invoice_count(), 1);

    let restore = client.try_restore_backup(&admin, &backup_id);
    assert_eq!(restore, Err(Ok(QuickLendXError::StorageError)));
    assert_eq!(client.get_total_invoice_count(), 1);
}

#[test]
fn archived_backups_are_excluded_from_active_retention_cleanup() {
    let (env, client, admin) = setup();
    create_invoice(&env, &client, 1_000);
    let archived_id = client.create_backup(&admin);
    client.archive_backup(&admin, &archived_id);

    client.set_backup_retention_policy(&admin, &1, &0, &true);
    env.ledger().set_timestamp(env.ledger().timestamp() + 1);
    let old_active = client.create_backup(&admin);
    env.ledger().set_timestamp(env.ledger().timestamp() + 1);
    let newest_active = client.create_backup(&admin);

    assert!(client.get_backup_details(&old_active).is_none());
    assert!(client.get_backups().contains(&newest_active));
    assert_eq!(
        client.get_backup_details(&archived_id).unwrap().status,
        BackupStatus::Archived
    );
}

#[test]
fn nonexistent_backup_validation_returns_false() {
    let (env, client, _admin) = setup();
    let missing = BytesN::from_array(&env, &[0; 32]);
    assert!(!client.validate_backup(&missing));
}
