#![cfg(test)]

use crate::errors::QuickLendXError;
use crate::types::{BusinessFreezeReason, FreezeInfo};
use soroban_sdk::testutils::{Address as _, Ledger};
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
        &crate::invoice::InvoiceCategory::Services,
        &Vec::new(env),
    );
    (invoice_id, currency)
}

// ============================================================================
// Happy path — each freeze variant can be stored and retrieved
// ============================================================================

#[test]
fn test_freeze_admin_action() {
    let (env, client, admin) = setup_env();
    let business = setup_business(&env, &client, &admin);
    let (invoice_id, _) = setup_invoice(&env, &client, &admin, &business);

    client.freeze_invoice(&admin, &invoice_id, &BusinessFreezeReason::AdminAction);

    let info = client.get_invoice_freeze_info(&invoice_id).unwrap();
    assert_eq!(info.reason, BusinessFreezeReason::AdminAction);
    assert_eq!(info.frozen_by, admin);
}

#[test]
fn test_freeze_kyc_rejected() {
    let (env, client, admin) = setup_env();
    let business = setup_business(&env, &client, &admin);
    let (invoice_id, _) = setup_invoice(&env, &client, &admin, &business);

    client.freeze_invoice(&admin, &invoice_id, &BusinessFreezeReason::KYCRejected);

    let info = client.get_invoice_freeze_info(&invoice_id).unwrap();
    assert_eq!(info.reason, BusinessFreezeReason::KYCRejected);
}

#[test]
fn test_freeze_compliance_violation() {
    let (env, client, admin) = setup_env();
    let business = setup_business(&env, &client, &admin);
    let (invoice_id, _) = setup_invoice(&env, &client, &admin, &business);

    client.freeze_invoice(
        &admin,
        &invoice_id,
        &BusinessFreezeReason::ComplianceViolation,
    );

    let info = client.get_invoice_freeze_info(&invoice_id).unwrap();
    assert_eq!(info.reason, BusinessFreezeReason::ComplianceViolation);
}

#[test]
fn test_freeze_suspicious_activity() {
    let (env, client, admin) = setup_env();
    let business = setup_business(&env, &client, &admin);
    let (invoice_id, _) = setup_invoice(&env, &client, &admin, &business);

    client.freeze_invoice(
        &admin,
        &invoice_id,
        &BusinessFreezeReason::SuspiciousActivity,
    );

    let info = client.get_invoice_freeze_info(&invoice_id).unwrap();
    assert_eq!(info.reason, BusinessFreezeReason::SuspiciousActivity);
}

#[test]
fn test_freeze_legal_hold() {
    let (env, client, admin) = setup_env();
    let business = setup_business(&env, &client, &admin);
    let (invoice_id, _) = setup_invoice(&env, &client, &admin, &business);

    client.freeze_invoice(&admin, &invoice_id, &BusinessFreezeReason::LegalHold);

    let info = client.get_invoice_freeze_info(&invoice_id).unwrap();
    assert_eq!(info.reason, BusinessFreezeReason::LegalHold);
}

// ============================================================================
// Freeze blocks operations — integration paths
// ============================================================================

#[test]
fn test_freeze_blocks_place_bid() {
    let (env, client, admin) = setup_env();
    let business = setup_business(&env, &client, &admin);
    let (invoice_id, _) = setup_invoice(&env, &client, &admin, &business);
    let investor = setup_investor(&env, &client, 10_000);

    client.verify_invoice(&invoice_id);
    client.freeze_invoice(&admin, &invoice_id, &BusinessFreezeReason::AdminAction);

    let result = client.try_place_bid(
        &investor,
        &invoice_id,
        &900i128,
        &950i128,
        &BytesN::from_array(&env, &[0u8; 32]),
    );
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().unwrap(), QuickLendXError::InvoiceFrozen);
}

#[test]
fn test_freeze_blocks_accept_bid() {
    let (env, client, admin) = setup_env();
    let business = setup_business(&env, &client, &admin);
    let (invoice_id, currency) = setup_invoice(&env, &client, &admin, &business);

    client.verify_invoice(&invoice_id);

    let investor = setup_investor(&env, &client, 10_000);
    fund_user(&env, &client, &currency, &investor, 10_000);
    let bid_id = client.place_bid(
        &investor,
        &invoice_id,
        &900i128,
        &950i128,
        &BytesN::from_array(&env, &[0u8; 32]),
    );

    client.freeze_invoice(&admin, &invoice_id, &BusinessFreezeReason::AdminAction);

    let result = client.try_accept_bid(&invoice_id, &bid_id);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().unwrap(), QuickLendXError::InvoiceFrozen);
}

#[test]
fn test_freeze_blocks_partial_payment() {
    let (env, client, admin) = setup_env();
    let business = setup_business(&env, &client, &admin);
    let (invoice_id, currency) = setup_invoice(&env, &client, &admin, &business);

    client.verify_invoice(&invoice_id);

    let investor = setup_investor(&env, &client, 10_000);
    fund_user(&env, &client, &currency, &investor, 10_000);
    let bid_id = client.place_bid(
        &investor,
        &invoice_id,
        &900i128,
        &950i128,
        &BytesN::from_array(&env, &[0u8; 32]),
    );
    client.accept_bid(&invoice_id, &bid_id);

    client.freeze_invoice(&admin, &invoice_id, &BusinessFreezeReason::AdminAction);

    let result = client.try_process_partial_payment(
        &invoice_id,
        &400i128,
        &String::from_str(&env, "tx-freeze-block"),
    );
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().unwrap(), QuickLendXError::InvoiceFrozen);
}

// ============================================================================
// Sad path — unfreeze restores operations
// ============================================================================

#[test]
fn test_unfreeze_restores_bid() {
    let (env, client, admin) = setup_env();
    let business = setup_business(&env, &client, &admin);
    let (invoice_id, _) = setup_invoice(&env, &client, &admin, &business);
    let investor = setup_investor(&env, &client, 10_000);

    client.verify_invoice(&invoice_id);

    client.freeze_invoice(&admin, &invoice_id, &BusinessFreezeReason::AdminAction);
    assert!(client.get_invoice_freeze_info(&invoice_id).is_some());

    client.unfreeze_invoice(&admin, &invoice_id);
    assert!(client.get_invoice_freeze_info(&invoice_id).is_none());

    let result = client.try_place_bid(
        &investor,
        &invoice_id,
        &900i128,
        &950i128,
        &BytesN::from_array(&env, &[0u8; 32]),
    );
    assert!(result.is_ok());
}

#[test]
fn test_unfreeze_restores_payment() {
    let (env, client, admin) = setup_env();
    let business = setup_business(&env, &client, &admin);
    let (invoice_id, currency) = setup_invoice(&env, &client, &admin, &business);

    client.verify_invoice(&invoice_id);

    let investor = setup_investor(&env, &client, 10_000);
    fund_user(&env, &client, &currency, &investor, 10_000);
    let bid_id = client.place_bid(
        &investor,
        &invoice_id,
        &900i128,
        &950i128,
        &BytesN::from_array(&env, &[0u8; 32]),
    );
    client.accept_bid(&invoice_id, &bid_id);

    client.freeze_invoice(&admin, &invoice_id, &BusinessFreezeReason::AdminAction);
    client.unfreeze_invoice(&admin, &invoice_id);

    client.process_partial_payment(
        &invoice_id,
        &400i128,
        &String::from_str(&env, "tx-after-unfreeze"),
    );
}

// ============================================================================
// Sad path — non-admin cannot freeze
// ============================================================================

#[test]
fn test_freeze_requires_admin() {
    let (env, client, admin) = setup_env();
    let business = setup_business(&env, &client, &admin);
    let (invoice_id, _) = setup_invoice(&env, &client, &admin, &business);

    let non_admin = Address::generate(&env);
    let result =
        client.try_freeze_invoice(&non_admin, &invoice_id, &BusinessFreezeReason::AdminAction);
    assert!(result.is_err());
}

// ============================================================================
// Sad path — freeze on non-existent invoice
// ============================================================================

#[test]
fn test_freeze_nonexistent_invoice() {
    let (env, client, admin) = setup_env();
    let fake_id = BytesN::from_array(&env, &[0u8; 32]);

    let result = client.try_freeze_invoice(&admin, &fake_id, &BusinessFreezeReason::AdminAction);
    assert!(result.is_err());
    assert_eq!(
        result.unwrap_err().unwrap(),
        QuickLendXError::InvoiceNotFound
    );
}

// ============================================================================
// Query — get_invoice_freeze_info returns None for non-frozen invoices
// ============================================================================

#[test]
fn test_get_freeze_info_none_when_not_frozen() {
    let (env, client, admin) = setup_env();
    let business = setup_business(&env, &client, &admin);
    let (invoice_id, _) = setup_invoice(&env, &client, &admin, &business);

    let info = client.get_invoice_freeze_info(&invoice_id);
    assert!(info.is_none());
}

#[test]
fn test_get_freeze_info_none_after_unfreeze() {
    let (env, client, admin) = setup_env();
    let business = setup_business(&env, &client, &admin);
    let (invoice_id, _) = setup_invoice(&env, &client, &admin, &business);

    client.freeze_invoice(&admin, &invoice_id, &BusinessFreezeReason::LegalHold);
    assert!(client.get_invoice_freeze_info(&invoice_id).is_some());

    client.unfreeze_invoice(&admin, &invoice_id);
    assert!(client.get_invoice_freeze_info(&invoice_id).is_none());
}

// ============================================================================
// Freeze info contains correct metadata
// ============================================================================

#[test]
fn test_freeze_info_includes_timestamp() {
    let (env, client, admin) = setup_env();
    let business = setup_business(&env, &client, &admin);
    let (invoice_id, _) = setup_invoice(&env, &client, &admin, &business);
    let now = env.ledger().timestamp();

    client.freeze_invoice(
        &admin,
        &invoice_id,
        &BusinessFreezeReason::SuspiciousActivity,
    );

    let info = client.get_invoice_freeze_info(&invoice_id).unwrap();
    assert_eq!(info.frozen_by, admin);
    assert!(info.frozen_at >= now);
    assert_eq!(info.reason, BusinessFreezeReason::SuspiciousActivity);
}
