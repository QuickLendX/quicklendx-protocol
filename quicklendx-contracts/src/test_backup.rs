#![cfg(test)]
extern crate std;

use crate::{
    backup::BackupStatus, invoice::InvoiceCategory, QuickLendXContract, QuickLendXContractClient,
};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, BytesN, Env, String, Vec,
};

fn setup() -> (Env, QuickLendXContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.set_admin(&admin);
    (env, client, admin)
}

fn setup_token(
    env: &Env,
    business: &Address,
    investor: &Address,
    contract_id: &Address,
) -> Address {
    let token_admin = Address::generate(env);
    let currency = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let sac_client = token::StellarAssetClient::new(env, &currency);
    let token_client = token::Client::new(env, &currency);
    let initial = 100_000i128;
    sac_client.mint(business, &initial);
    sac_client.mint(investor, &initial);
    let expiration = env.ledger().sequence() + 10_000;
    token_client.approve(business, contract_id, &initial, &expiration);
    token_client.approve(investor, contract_id, &initial, &expiration);
    currency
}

fn setup_verified_business(
    env: &Env,
    client: &QuickLendXContractClient,
    admin: &Address,
) -> Address {
    let business = Address::generate(env);
    client.submit_kyc_application(&business, &String::from_str(env, "Business KYC"));
    client.verify_business(admin, &business);
    business
}

fn setup_verified_investor(env: &Env, client: &QuickLendXContractClient, limit: i128) -> Address {
    let investor = Address::generate(env);
    client.submit_investor_kyc(&investor, &String::from_str(env, "Investor KYC"));
    client.verify_investor(&investor, &limit);
    investor
}

fn create_funded_invoice(
    env: &Env,
    client: &QuickLendXContractClient,
    admin: &Address,
) -> (BytesN<32>, Address, Address, i128, Address) {
    client.initialize_protocol_limits(admin, &1i128, &100i128, &100u32, &365u64, &86400u64);
    let business = setup_verified_business(env, client, admin);
    let investor = setup_verified_investor(env, client, 50_000);
    let currency = setup_token(env, &business, &investor, &client.address);
    client.add_currency(admin, &currency);
    let amount = 10_000i128;
    let due_date = env.ledger().timestamp() + 86400;
    let invoice_id = client.store_invoice(
        &business,
        &amount,
        &currency,
        &due_date,
        &String::from_str(env, "Test Invoice"),
        &InvoiceCategory::Services,
        &Vec::new(env),
    );
    client.verify_invoice(&invoice_id);
    let bid_id = client.place_bid(&investor, &invoice_id, &amount, &(amount + 1000));
    client.accept_bid(&invoice_id, &bid_id);
    (invoice_id, business, investor, amount, currency)
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

    client.initialize_protocol_limits(&admin, &1i128, &100i128, &100u32, &365u64, &86400u64);
    let business = setup_verified_business(&env, &client, &admin);
    let investor = setup_verified_investor(&env, &client, 50_000);
    let currency = setup_token(&env, &business, &investor, &client.address);
    client.add_currency(&admin, &currency);

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

// ============================================================================
// get_backups and get_backup_details tests (#349)
// ============================================================================

/// get_backups returns IDs in creation order (oldest first); after archive, archived backup is excluded.
#[test]
fn test_get_backups_order_and_after_archive() {
    let (env, client, admin) = setup();

    let _ = create_funded_invoice(&env, &client, &admin);

    let id1 = client.create_backup(&admin);
    env.ledger().set_timestamp(env.ledger().timestamp() + 1);
    let id2 = client.create_backup(&admin);
    env.ledger().set_timestamp(env.ledger().timestamp() + 1);
    let id3 = client.create_backup(&admin);

    let backups = client.get_backups();
    assert_eq!(backups.len(), 3);
    assert_eq!(backups.get(0).unwrap(), id1);
    assert_eq!(backups.get(1).unwrap(), id2);
    assert_eq!(backups.get(2).unwrap(), id3);

    client.archive_backup(&admin, &id2);
    let after_archive = client.get_backups();
    assert_eq!(after_archive.len(), 2);
    assert!(after_archive.contains(&id1));
    assert!(!after_archive.contains(&id2));
    assert!(after_archive.contains(&id3));
}

/// get_backup_details returns Some with correct fields for a valid backup.
#[test]
fn test_get_backup_details_some_with_correct_fields() {
    let (env, client, admin) = setup();

    let (_, _, _, _, _) = create_funded_invoice(&env, &client, &admin);
    let ts_before = env.ledger().timestamp();
    let backup_id = client.create_backup(&admin);
    let ts_after = env.ledger().timestamp();

    let details = client.get_backup_details(&backup_id);
    assert!(details.is_some());
    let b = details.unwrap();
    assert_eq!(b.backup_id, backup_id);
    assert!(b.timestamp >= ts_before && b.timestamp <= ts_after);
    assert_eq!(b.invoice_count, 1);
    assert_eq!(b.status, BackupStatus::Active);
    assert!(!b.description.is_empty());
}

/// get_backup_details returns None for an invalid/unknown backup id.
#[test]
fn test_get_backup_details_none_for_invalid_id() {
    let (env, client, _admin) = setup();

    let invalid_id = BytesN::from_array(&env, &[0u8; 32]);
    let details = client.get_backup_details(&invalid_id);
    assert!(details.is_none());
}

/// Backup ID format: prefix bytes 0xB4, 0xC4 and timestamp embedded in storage.
#[test]
fn test_backup_id_format_and_storage() {
    let (env, client, admin) = setup();

    let _ = create_funded_invoice(&env, &client, &admin);
    let backup_id = client.create_backup(&admin);

    let details = client.get_backup_details(&backup_id).unwrap();
    let arr = backup_id.to_array();
    assert_eq!(arr[0], 0xB4);
    assert_eq!(arr[1], 0xC4);
    assert_eq!(arr[2..10], details.timestamp.to_be_bytes());
}
