#![cfg(feature = "legacy-tests")]

use quicklendx_contracts::{types::InvoiceCategory, QuickLendXContract, QuickLendXContractClient};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, BytesN, Env, String, Vec,
};

fn setup() -> (Env, QuickLendXContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.set_admin(&admin);
    client.initialize_protocol_limits(&admin, &1i128, &365u64, &86_400u64);
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

#[test]
fn backup_creation_requires_admin_and_validates_metadata() {
    let (env, client, admin) = setup();
    create_invoice(&env, &client, 1_000, "Invoice A");

    let stranger = Address::generate(&env);
    assert!(client.try_create_backup(&stranger).is_err());

    let backup_id = client.create_backup(&admin);
    let backup = client.get_backup_details(&backup_id).unwrap();

    assert_eq!(backup.invoice_count, 1);
    assert!(client.validate_backup(&backup_id));
}

#[test]
fn backup_ids_are_unique_and_list_is_deduplicated() {
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
}

#[test]
fn cleanup_by_count_purges_old_backup_metadata_and_payload() {
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
    assert!(!client.validate_backup(&id1));
}

#[test]
fn archived_backups_survive_automatic_cleanup() {
    let (env, client, admin) = setup();
    create_invoice(&env, &client, 1_000, "Invoice A");

    let archived_id = client.create_backup(&admin);
    client.archive_backup(&admin, &archived_id);

    client.set_backup_retention_policy(&admin, &1, &0, &true);
    client.create_backup(&admin);
    env.ledger().set_timestamp(env.ledger().timestamp() + 1);
    let newest_active = client.create_backup(&admin);

    let active = client.get_backups();
    assert_eq!(active.len(), 1);
    assert!(active.contains(&newest_active));
    assert!(client.get_backup_details(&archived_id).is_some());
    assert!(client.validate_backup(&archived_id));
}

#[test]
fn restore_backup_replaces_current_invoice_state() {
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
