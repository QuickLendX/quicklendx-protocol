#![cfg(test)]

use crate::errors::QuickLendXError;
use crate::types::{InvoiceCategory, RatingsSnapshot, RATINGS_SNAPSHOT_SCHEMA_VERSION};
use crate::{QuickLendXContract, QuickLendXContractClient};
use soroban_sdk::{testutils::Address as _, Address, BytesN, Env, String, Vec};

#[test]
fn test_ratings_snapshot_lifecycle() {
    let env = Env::default();
    env.mock_all_auths();
    
    let contract_id = env.register_contract(None, QuickLendXContract);
    let client = QuickLendXContractClient::new(&env, &contract_id);
    
    let admin = Address::generate(&env);
    client.initialize(&crate::init::InitializationParams {
        admin: admin.clone(),
        treasury: admin.clone(),
        fee_bps: 100,
        min_invoice_amount: 100,
        max_due_date_days: 90,
        grace_period_seconds: 86400,
        initial_currencies: soroban_sdk::Vec::new(&env),
        corridors: soroban_sdk::Vec::new(&env),
        backfill_max_batch_size: 100,
    });
    
    let business = Address::generate(&env);
    let investor = Address::generate(&env);
    let currency = Address::generate(&env);
    
    client.add_currency(&admin, &currency);
    client.submit_investor_kyc(&investor);
    client.verify_investor(&admin, &investor);
    
    let due_date = env.ledger().timestamp() + 86400 * 30;
    
    let invoice_id = client.store_invoice(
        &business,
        &1000,
        &currency,
        &due_date,
        &String::from_str(&env, "Test Invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    
    client.verify_invoice(&admin, &invoice_id);
    client.accept_bid_and_fund(&investor, &invoice_id, &1000, &1050, &0, &false, &admin);
    
    // Add multiple ratings
    client.add_invoice_rating(&invoice_id, &5, &String::from_str(&env, "Great!"), &investor);
    
    let investor2 = Address::generate(&env);
    client.submit_investor_kyc(&investor2);
    client.verify_investor(&admin, &investor2);
    // Since only funders can rate, we need to bypass it or use admin override. Let's just override it.
    client.rating_override(&admin, &invoice_id, &3);
    
    // Test snapshot
    env.ledger().with_mut(|l| {
        l.sequence = 12345;
    });
    
    let snapshot = client.ratings_snapshot(&invoice_id);
    
    assert_eq!(snapshot.schema_version, RATINGS_SNAPSHOT_SCHEMA_VERSION);
    assert_eq!(snapshot.invoice_id, invoice_id);
    assert_eq!(snapshot.average_rating, Some(3)); 
    assert_eq!(snapshot.total_ratings, 1);
    assert_eq!(snapshot.highest_rating, Some(5));
    assert_eq!(snapshot.lowest_rating, Some(5));
    assert_eq!(snapshot.ledger_sequence, 12345);
}

/// Same invoice state, called repeatedly, must yield an identical snapshot:
/// deterministic input to output with no hidden state (e.g. randomness or
/// wall-clock reads) leaking into the result.
#[test]
fn test_ratings_snapshot_is_deterministic_for_unchanged_state() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, QuickLendXContract);
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    client.initialize(&crate::init::InitializationParams {
        admin: admin.clone(),
        treasury: admin.clone(),
        fee_bps: 100,
        min_invoice_amount: 100,
        max_due_date_days: 90,
        grace_period_seconds: 86400,
    });

    let business = Address::generate(&env);
    let investor = Address::generate(&env);
    let currency = Address::generate(&env);

    client.add_currency(&admin, &currency);
    client.submit_investor_kyc(&investor);
    client.verify_investor(&admin, &investor);

    let due_date = env.ledger().timestamp() + 86400 * 30;

    let invoice_id = client.store_invoice(
        &business,
        &1000,
        &currency,
        &due_date,
        &String::from_str(&env, "Deterministic Snapshot Invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    client.verify_invoice(&admin, &invoice_id);
    client.accept_bid_and_fund(&investor, &invoice_id, &1000, &1050, &0, &false, &admin);
    client.add_invoice_rating(&invoice_id, &4, &String::from_str(&env, "Solid"), &investor);

    env.ledger().with_mut(|l| {
        l.sequence = 777;
    });

    // Calling the entrypoint multiple times with no state change in between
    // must return byte-for-byte equal snapshots.
    let first: RatingsSnapshot = client.ratings_snapshot(&invoice_id);
    let second: RatingsSnapshot = client.ratings_snapshot(&invoice_id);
    let third: RatingsSnapshot = client.ratings_snapshot(&invoice_id);

    assert_eq!(first, second);
    assert_eq!(second, third);

    // Pin down every field explicitly so a future regression that changes
    // only one field (rather than making the whole struct diverge) is
    // still caught.
    assert_eq!(first.schema_version, RATINGS_SNAPSHOT_SCHEMA_VERSION);
    assert_eq!(first.invoice_id, invoice_id);
    assert_eq!(first.average_rating, Some(4));
    assert_eq!(first.total_ratings, 1);
    assert_eq!(first.highest_rating, Some(4));
    assert_eq!(first.lowest_rating, Some(4));
    assert_eq!(first.ledger_sequence, 777);
}

/// A snapshot request for an invoice that was never stored must fail with
/// `InvoiceNotFound` rather than panicking or returning a default/empty
/// snapshot — the explicit sad path for the deterministic-snapshot contract.
#[test]
fn test_ratings_snapshot_fails_for_nonexistent_invoice() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, QuickLendXContract);
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    client.initialize(&crate::init::InitializationParams {
        admin: admin.clone(),
        treasury: admin.clone(),
        fee_bps: 100,
        min_invoice_amount: 100,
        max_due_date_days: 90,
        grace_period_seconds: 86400,
    });

    let missing_invoice_id = BytesN::from_array(&env, &[42u8; 32]);
    let result = client.try_ratings_snapshot(&missing_invoice_id);

    assert!(result.is_err());
    assert_eq!(
        result.unwrap_err().expect("expected contract error"),
        QuickLendXError::InvoiceNotFound
    );
}
