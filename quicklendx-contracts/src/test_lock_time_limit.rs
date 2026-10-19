//! Test for invoice lock time limit guard (issue #2103)
//!
//! This test verifies that actions on expired locks are rejected with
//! InvoiceLockExpired error, providing defense-in-depth against
//! indefinite invoice freezing.

use soroban_sdk::{Address, BytesN, Env};

use crate::testutils::{create_test_invoice, create_verified_business, setup};
use crate::types::BusinessFreezeReason;

#[test]
fn test_expired_lock_rejects_actions() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let (invoice_id, _) = create_test_invoice(&env, &client, &business, 100_000);

    // Freeze the invoice
    client.freeze_invoice(&admin, &invoice_id, &BusinessFreezeReason::AdminAction);
    assert!(client.get_invoice_freeze_info(&invoice_id).is_some());

    // Advance time beyond the lock time limit (30 days + 1 second)
    let current_time = env.ledger().timestamp();
    let thirty_days_seconds = 2_592_000; // LOCK_TIME_LIMIT_SECONDS
    env.ledger()
        .set_timestamp(current_time + thirty_days_seconds + 1);

    // Attempt to place a bid - should fail with InvoiceLockExpired
    let investor = Address::generate(&env);
    let result = client.try_place_bid(
        &investor,
        &invoice_id,
        &10_000,
        &11_000,
        &BytesN::from_array(&env, &[0u8; 32]),
    );

    assert!(result.is_err());
    assert_eq!(
        result.unwrap_err().unwrap(),
        crate::errors::QuickLendXError::InvoiceLockExpired
    );
}

#[test]
fn test_fresh_lock_allows_actions() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let (invoice_id, _) = create_test_invoice(&env, &client, &business, 100_000);

    // Freeze the invoice
    client.freeze_invoice(&admin, &invoice_id, &BusinessFreezeReason::AdminAction);
    assert!(client.get_invoice_freeze_info(&invoice_id).is_some());

    // Keep time within the lock time limit (29 days)
    let current_time = env.ledger().timestamp();
    let twenty_nine_days_seconds = 2_505_600; // 29 days
    env.ledger()
        .set_timestamp(current_time + twenty_nine_days_seconds);

    // Attempt to place a bid - should fail with InvoiceFrozen (not expired)
    let investor = Address::generate(&env);
    let result = client.try_place_bid(
        &investor,
        &invoice_id,
        &10_000,
        &11_000,
        &BytesN::from_array(&env, &[0u8; 32]),
    );

    assert!(result.is_err());
    assert_eq!(
        result.unwrap_err().unwrap(),
        crate::errors::QuickLendXError::InvoiceFrozen
    );
}
