use crate::errors::QuickLendXError;
use crate::protocol_limits::{
    is_active_status,
    DEFAULT_MAX_INVOICES_PER_BUSINESS,
};
use crate::types::InvoiceStatus;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address,
    Env,
};

// =========================================================================
// Unit tests — pure logic, no contract environment
// =========================================================================

fn enforce_limit_logic(active_count: u32, limit: u32) -> Result<(), QuickLendXError> {
    if limit > 0 && active_count >= limit {
        return Err(QuickLendXError::MaxInvoicesPerBusinessExceeded);
    }
    Ok(())
}

#[test]
fn test_business_at_cap_exact_boundary() {
    let limit = 5;

    // Below limit (4): allowed
    assert_eq!(enforce_limit_logic(4, limit), Ok(()));

    // At limit (5): next one is rejected
    assert_eq!(
        enforce_limit_logic(5, limit),
        Err(QuickLendXError::MaxInvoicesPerBusinessExceeded)
    );

    // Above limit (6): safely rejected
    assert_eq!(
        enforce_limit_logic(6, limit),
        Err(QuickLendXError::MaxInvoicesPerBusinessExceeded)
    );
}

#[test]
fn test_business_at_cap_one_above() {
    let limit = 3;

    // At cap: exactly at limit is rejected
    assert_eq!(
        enforce_limit_logic(3, limit),
        Err(QuickLendXError::MaxInvoicesPerBusinessExceeded)
    );

    // One above: also rejected (safety margin)
    assert_eq!(
        enforce_limit_logic(4, limit),
        Err(QuickLendXError::MaxInvoicesPerBusinessExceeded)
    );
}

#[test]
fn test_business_under_cap_allows_invoice() {
    let limit = 10;

    // At N-1: allowed
    assert_eq!(enforce_limit_logic(9, limit), Ok(()));

    // At 0: allowed
    assert_eq!(enforce_limit_logic(0, limit), Ok(()));
}

#[test]
fn test_zero_limit_is_unlimited() {
    // 0 = unlimited per protocol convention
    assert_eq!(enforce_limit_logic(0, 0), Ok(()));
    assert_eq!(enforce_limit_logic(100, 0), Ok(()));
    assert_eq!(enforce_limit_logic(1_000, 0), Ok(()));
    assert_eq!(enforce_limit_logic(u32::MAX, 0), Ok(()));
}

#[test]
fn test_is_active_status_boundaries() {
    // Active statuses count toward the cap
    assert!(is_active_status(&InvoiceStatus::Pending));
    assert!(is_active_status(&InvoiceStatus::Verified));
    assert!(is_active_status(&InvoiceStatus::Funded));

    // Terminal statuses free a slot
    assert!(!is_active_status(&InvoiceStatus::Paid));
    assert!(!is_active_status(&InvoiceStatus::Defaulted));
    assert!(!is_active_status(&InvoiceStatus::Cancelled));
    assert!(!is_active_status(&InvoiceStatus::Refunded));
}

#[test]
fn test_paid_status_frees_cap_slot() {
    let limit = 3;

    // At cap (3 active invoices)
    assert_eq!(
        enforce_limit_logic(3, limit),
        Err(QuickLendXError::MaxInvoicesPerBusinessExceeded)
    );

    // After settlement (1 becomes Paid, count drops to 2)
    assert_eq!(enforce_limit_logic(2, limit), Ok(()));
}

#[test]
fn test_cancelled_status_frees_cap_slot() {
    let limit = 2;

    assert_eq!(
        enforce_limit_logic(2, limit),
        Err(QuickLendXError::MaxInvoicesPerBusinessExceeded)
    );

    // Cancel an invoice: no longer counts toward cap
    assert_eq!(enforce_limit_logic(1, limit), Ok(()));
}

#[test]
fn test_default_cap_constant() {
    assert_eq!(DEFAULT_MAX_INVOICES_PER_BUSINESS, 100u32);
}

// =========================================================================
// Integration tests — full contract entrypoints
// =========================================================================

use crate::invoice::InvoiceCategory;
use crate::{QuickLendXContract, QuickLendXContractClient};
use soroban_sdk::{token, BytesN, String, Vec};

/// Register a test token and mint balances for test participants.
fn create_test_token(
    env: &Env,
    contract_id: &Address,
    business: &Address,
    balance: i128,
) -> Address {
    let token_admin = Address::generate(env);
    let currency = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let sac_client = token::StellarAssetClient::new(env, &currency);
    let token_client = token::Client::new(env, &currency);

    sac_client.mint(business, &balance);
    sac_client.mint(contract_id, &1i128);

    let expiration = env.ledger().sequence() + 10_000;
    token_client.approve(business, contract_id, &balance, &expiration);

    currency
}

/// Helper: create a verified business and a test token.
fn setup_environment(
    env: &Env,
    client: &QuickLendXContractClient,
    admin: &Address,
    business: &Address,
) -> Address {
    client.set_admin(admin);

    client.submit_kyc_application(business, &String::from_str(env, "business-kyc"));
    client.verify_business(admin, business);

    let contract_id = client.address.clone();
    create_test_token(env, &contract_id, business, 1_000_000)
}

/// Helper: create an invoice for a business using upload_invoice.
fn create_invoice_at(
    env: &Env,
    client: &QuickLendXContractClient,
    business: &Address,
    currency: &Address,
) -> BytesN<32> {
    let due_date = env.ledger().timestamp() + 86_400;
    client.upload_invoice(
        business,
        &1_000i128,
        currency,
        &due_date,
        &String::from_str(env, "Test invoice"),
        &InvoiceCategory::Services,
        &Vec::new(env),
        &None,
    )
}

// ── Integration: at cap → new invoice rejected ──────────────────────────

#[test]
fn test_store_invoice_respects_cap_at_boundary() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let business = Address::generate(&env);

    let currency = setup_environment(&env, &client, &admin, &business);

    // Set a small limit so we don't hit resource budget
    let small_cap = 5u32;
    client.update_limits_max_invoices(
        &admin,
        &10i128,
        &365u64,
        &604800u64,
        &small_cap,
    );

    // Fill invoices up to the cap.
    for _ in 0..small_cap {
        create_invoice_at(&env, &client, &business, &currency);
    }

    // Business is at the cap — next invoice should fail.
    let due_date = env.ledger().timestamp() + 86_400;
    let result = client.try_upload_invoice(
        &business,
        &1_000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "One-over-cap invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
        &None,
    );
    assert_eq!(result, Err(Ok(QuickLendXError::MaxInvoicesPerBusinessExceeded)));
}

#[test]
fn test_store_invoice_one_above_cap_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let business = Address::generate(&env);

    let currency = setup_environment(&env, &client, &admin, &business);

    // Set a small limit for testing
    let small_limit = 3u32;
    client.update_limits_max_invoices(
        &admin,
        &10i128,
        &365u64,
        &604800u64,
        &small_limit,
    );

    // Fill to cap
    for _ in 0..small_limit {
        create_invoice_at(&env, &client, &business, &currency);
    }

    // One more should fail
    let due_date = env.ledger().timestamp() + 86_400;
    let result = client.try_upload_invoice(
        &business,
        &1_000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Over-cap"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
        &None,
    );
    assert_eq!(result, Err(Ok(QuickLendXError::MaxInvoicesPerBusinessExceeded)));
}

// ── Integration: zero limit = unlimited ─────────────────────────────────

#[test]
fn test_zero_limit_allows_many_invoices() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let business = Address::generate(&env);

    let currency = setup_environment(&env, &client, &admin, &business);

    // Set limit to 0 = unlimited
    client.update_limits_max_invoices(
        &admin,
        &10i128,
        &365u64,
        &604800u64,
        &0u32,
    );

    // Create many invoices — should all succeed
    for i in 0..10u32 {
        let due_date = env.ledger().timestamp() + 86_400;
        let invoice_id = client.upload_invoice(
            &business,
            &(1_000i128 * (i as i128 + 1)),
            &currency,
            &due_date,
            &String::from_str(&env, "Unlimited cap invoice"),
            &InvoiceCategory::Services,
            &Vec::new(&env),
            &None,
        );
        // Read invoice back to confirm it was stored
        let fetched = client.get_invoice(&invoice_id);
        assert_eq!(
            fetched.amount,
            1_000i128 * (i as i128 + 1),
            "Invoice {} amount mismatch",
            i + 1
        );
    }
}

// ── Integration: after settlement, cap slot is freed ────────────────────

#[test]
fn test_after_settlement_frees_cap_slot() {
    let env = Env::default();
    env.budget().reset_unlimited();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let business = Address::generate(&env);
    let investor = Address::generate(&env);

    // Set a small cap
    let cap = 2u32;
    client.set_admin(&admin);

    client.submit_kyc_application(&business, &String::from_str(&env, "business-kyc"));
    client.verify_business(&admin, &business);

    client.submit_investor_kyc(&investor, &String::from_str(&env, "investor-kyc"));
    client.verify_investor(&investor, &10_000_000i128);

    let token_admin = Address::generate(&env);
    let currency = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let sac_client = token::StellarAssetClient::new(&env, &currency);
    let token_client = token::Client::new(&env, &currency);

    sac_client.mint(&business, &1_000_000i128);
    sac_client.mint(&investor, &1_000_000i128);
    sac_client.mint(&contract_id, &1i128);

    let expiration = env.ledger().sequence() + 10_000;
    token_client.approve(&business, &contract_id, &1_000_000i128, &expiration);
    token_client.approve(&investor, &contract_id, &1_000_000i128, &expiration);

    client.update_limits_max_invoices(
        &admin,
        &10i128,
        &365u64,
        &604800u64,
        &cap,
    );

    // Create invoices up to cap
    let inv1 = create_invoice_at(&env, &client, &business, &currency);
    let inv2 = create_invoice_at(&env, &client, &business, &currency);

    // A third one should fail — at cap
    let due_date = env.ledger().timestamp() + 86_400;
    let over_cap = client.try_upload_invoice(
        &business,
        &1_000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Over-cap"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
        &None,
    );
    assert_eq!(over_cap, Err(Ok(QuickLendXError::MaxInvoicesPerBusinessExceeded)));

    // Fund the first invoice
    client.verify_invoice(&inv1);
    let bid_amount = 500i128;
    let expected_return = 550i128;
    let bid_id = client.place_bid(
        &investor,
        &inv1,
        &bid_amount,
        &expected_return,
        &BytesN::from_array(&env, &[0u8; 32]),
    );
    client.accept_bid(&inv1, &bid_id);

    // Settle with full payment covering the invoice amount
    env.ledger().set_timestamp(env.ledger().timestamp() + 172_800);
    client.settle_invoice(&inv1, &1_000i128);

    // Now the invoice is Paid — a new invoice should succeed
    let new_invoice = create_invoice_at(&env, &client, &business, &currency);
    let fetched = client.get_invoice(&new_invoice);
    assert_eq!(fetched.amount, 1_000i128);
}

// ── Integration: count_active_invoices via on-chain query ───────────────

#[test]
fn test_onchain_active_invoice_count() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let business = Address::generate(&env);

    let currency = setup_environment(&env, &client, &admin, &business);

    // No invoices yet
    let before = client.get_business_invoices(&business);
    assert_eq!(before.len(), 0u32);

    // After creating three invoices
    create_invoice_at(&env, &client, &business, &currency);
    create_invoice_at(&env, &client, &business, &currency);
    create_invoice_at(&env, &client, &business, &currency);

    let invoices = client.get_business_invoices(&business);
    assert_eq!(invoices.len(), 3u32);

    // Verify each invoice has an active status
    for invoice_id in invoices.iter() {
        let invoice = client.get_invoice(&invoice_id);
        assert!(is_active_status(&invoice.status));
    }
}
