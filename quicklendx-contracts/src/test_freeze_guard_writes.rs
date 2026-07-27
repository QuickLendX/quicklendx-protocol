#![cfg(test)]

//! Negative tests for `require_no_active_freeze` on all write paths.
//!
//! These tests exercise the defence-in-depth fix: every state-mutating
//! entry-point must reject calls when the target invoice is frozen.
//! Each test **fails today** (before the fix) and passes after.

use crate::errors::QuickLendXError;
use crate::types::{BusinessFreezeReason, InvoiceCategory};
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{token, Address, BytesN, Env, String, Vec};

fn setup_env() -> (Env, crate::QuickLendXContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = crate::QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.set_admin(&admin);
    (env, client, admin)
}

fn setup_business(env: &Env, client: &crate::QuickLendXContractClient, admin: &Address) -> Address {
    let business = Address::generate(env);
    client.submit_kyc_application(&business, &String::from_str(env, "Business KYC"));
    client.verify_business(admin, &business);
    business
}

fn setup_investor(env: &Env, client: &crate::QuickLendXContractClient, limit: i128) -> Address {
    let investor = Address::generate(env);
    client.submit_investor_kyc(&investor, &String::from_str(env, "Investor KYC"));
    client.verify_investor(&investor, &limit);
    investor
}

fn fund_user(
    env: &Env,
    client: &crate::QuickLendXContractClient,
    currency: &Address,
    user: &Address,
    amount: i128,
) {
    let sac = token::StellarAssetClient::new(env, currency);
    let tok = token::Client::new(env, currency);
    sac.mint(user, &amount);
    let expiry = env.ledger().sequence() + 10_000;
    tok.approve(user, &client.address, &amount, &expiry);
}

fn setup_currency(
    env: &Env,
    client: &crate::QuickLendXContractClient,
    admin: &Address,
    business: &Address,
) -> Address {
    let token_admin = Address::generate(env);
    let currency = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let sac = token::StellarAssetClient::new(env, &currency);
    let tok = token::Client::new(env, &currency);
    let initial = 100_000i128;
    sac.mint(business, &initial);
    let expiry = env.ledger().sequence() + 10_000;
    tok.approve(business, &client.address, &initial, &expiry);
    client.add_currency(admin, &currency);
    currency
}

fn setup_invoice(
    env: &Env,
    client: &crate::QuickLendXContractClient,
    admin: &Address,
    business: &Address,
) -> (BytesN<32>, Address) {
    let currency = setup_currency(env, client, admin, business);
    let due = env.ledger().timestamp() + 86_400;
    let invoice_id = client.upload_invoice(
        business,
        &1_000i128,
        &currency,
        &due,
        &String::from_str(env, "test invoice"),
        &InvoiceCategory::Services,
        &Vec::new(env),
        &None);
    (invoice_id, currency)
}

// ============================================================================
// cancel_invoice — frozen invoice must be rejected
// ============================================================================

#[test]
fn test_freeze_blocks_cancel_invoice() {
    let (env, client, admin) = setup_env();
    let business = setup_business(&env, &client, &admin);
    let (invoice_id, _) = setup_invoice(&env, &client, &admin, &business);

    client.freeze_invoice(&admin, &invoice_id, &BusinessFreezeReason::AdminAction);

    let result = client.try_cancel_invoice(&invoice_id);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().unwrap(), QuickLendXError::InvoiceFrozen);
}

#[test]
fn test_unfreeze_allows_cancel_invoice() {
    let (env, client, admin) = setup_env();
    let business = setup_business(&env, &client, &admin);
    let (invoice_id, _) = setup_invoice(&env, &client, &admin, &business);

    client.freeze_invoice(&admin, &invoice_id, &BusinessFreezeReason::AdminAction);
    client.unfreeze_invoice(&admin, &invoice_id);

    let result = client.try_cancel_invoice(&invoice_id);
    assert!(result.is_ok());
}

// ============================================================================
// update_invoice_metadata — frozen invoice must be rejected
// ============================================================================

#[test]
fn test_freeze_blocks_update_invoice_metadata() {
    let (env, client, admin) = setup_env();
    let business = setup_business(&env, &client, &admin);
    let (invoice_id, _) = setup_invoice(&env, &client, &admin, &business);

    client.freeze_invoice(&admin, &invoice_id, &BusinessFreezeReason::AdminAction);

    let metadata = crate::types::InvoiceMetadata {
        customer_name: String::from_str(&env, "Acme Corp"),
        customer_address: String::from_str(&env, "123 Main St"),
        tax_id: String::from_str(&env, "TAX123"),
        line_items: Vec::new(&env),
        notes: String::from_str(&env, "note"),
    };
    let result = client.try_update_invoice_metadata(&invoice_id, &metadata);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().unwrap(), QuickLendXError::InvoiceFrozen);
}

// ============================================================================
// clear_invoice_metadata — frozen invoice must be rejected
// ============================================================================

#[test]
fn test_freeze_blocks_clear_invoice_metadata() {
    let (env, client, admin) = setup_env();
    let business = setup_business(&env, &client, &admin);
    let (invoice_id, _) = setup_invoice(&env, &client, &admin, &business);

    client.freeze_invoice(&admin, &invoice_id, &BusinessFreezeReason::AdminAction);

    let result = client.try_clear_invoice_metadata(&invoice_id);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().unwrap(), QuickLendXError::InvoiceFrozen);
}

// ============================================================================
// update_invoice_category — frozen invoice must be rejected
// ============================================================================

#[test]
fn test_freeze_blocks_update_invoice_category() {
    let (env, client, admin) = setup_env();
    let business = setup_business(&env, &client, &admin);
    let (invoice_id, _) = setup_invoice(&env, &client, &admin, &business);

    client.freeze_invoice(&admin, &invoice_id, &BusinessFreezeReason::AdminAction);

    let result = client.try_update_invoice_category(&invoice_id, &InvoiceCategory::Goods);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().unwrap(), QuickLendXError::InvoiceFrozen);
}

// ============================================================================
// add_invoice_tag — frozen invoice must be rejected
// ============================================================================

#[test]
fn test_freeze_blocks_add_invoice_tag() {
    let (env, client, admin) = setup_env();
    let business = setup_business(&env, &client, &admin);
    let (invoice_id, _) = setup_invoice(&env, &client, &admin, &business);

    client.freeze_invoice(&admin, &invoice_id, &BusinessFreezeReason::AdminAction);

    let result = client.try_add_invoice_tag(&invoice_id, &String::from_str(&env, "urgent"));
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().unwrap(), QuickLendXError::InvoiceFrozen);
}

// ============================================================================
// remove_invoice_tag — frozen invoice must be rejected
// ============================================================================

#[test]
fn test_freeze_blocks_remove_invoice_tag() {
    let (env, client, admin) = setup_env();
    let business = setup_business(&env, &client, &admin);
    let (invoice_id, _) = setup_invoice(&env, &client, &admin, &business);

    // First add a tag so there's something to remove
    client.add_invoice_tag(&invoice_id, &String::from_str(&env, "test-tag"));

    client.freeze_invoice(&admin, &invoice_id, &BusinessFreezeReason::AdminAction);

    let result = client.try_remove_invoice_tag(&invoice_id, &String::from_str(&env, "test-tag"));
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().unwrap(), QuickLendXError::InvoiceFrozen);
}

// ============================================================================
// create_dispute — frozen invoice must be rejected
// ============================================================================

#[test]
fn test_freeze_blocks_create_dispute() {
    let (env, client, admin) = setup_env();
    let business = setup_business(&env, &client, &admin);
    let (invoice_id, _) = setup_invoice(&env, &client, &admin, &business);

    client.freeze_invoice(&admin, &invoice_id, &BusinessFreezeReason::AdminAction);

    let result = client.try_create_dispute(
        &invoice_id,
        &business,
        &String::from_str(&env, "quality issue"),
        &String::from_str(&env, "evidence"),
    );
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().unwrap(), QuickLendXError::InvoiceFrozen);
}

// ============================================================================
// add_invoice_rating — frozen invoice must be rejected
// ============================================================================

#[test]
fn test_freeze_blocks_add_invoice_rating() {
    let (env, client, admin) = setup_env();
    let business = setup_business(&env, &client, &admin);
    let (invoice_id, _) = setup_invoice(&env, &client, &admin, &business);
    let investor = setup_investor(&env, &client, 10_000);

    client.freeze_invoice(&admin, &invoice_id, &BusinessFreezeReason::AdminAction);

    let result = client.try_add_invoice_rating(
        &invoice_id,
        &5u32,
        &String::from_str(&env, "great service"),
        &investor,
    );
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().unwrap(), QuickLendXError::InvoiceFrozen);
}

// ============================================================================
// verify_invoice — frozen invoice must be rejected
// ============================================================================

#[test]
fn test_freeze_blocks_verify_invoice() {
    let (env, client, admin) = setup_env();
    let business = setup_business(&env, &client, &admin);
    let (invoice_id, _) = setup_invoice(&env, &client, &admin, &business);

    client.freeze_invoice(&admin, &invoice_id, &BusinessFreezeReason::AdminAction);

    let result = client.try_verify_invoice(&invoice_id);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().unwrap(), QuickLendXError::InvoiceFrozen);
}

// ============================================================================
// update_invoice_status — frozen invoice must be rejected
// ============================================================================

#[test]
fn test_freeze_blocks_update_invoice_status() {
    let (env, client, admin) = setup_env();
    let business = setup_business(&env, &client, &admin);
    let (invoice_id, _) = setup_invoice(&env, &client, &admin, &business);

    client.freeze_invoice(&admin, &invoice_id, &BusinessFreezeReason::AdminAction);

    let result =
        client.try_update_invoice_status(&invoice_id, &crate::types::InvoiceStatus::Verified);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().unwrap(), QuickLendXError::InvoiceFrozen);
}
