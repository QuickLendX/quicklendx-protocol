#![cfg(test)]
//! Tests for pause/unpause (#488) and pause bypass protections (#605).
//!
//! Test Coverage:
//! - When paused, mutating bid/escrow ops fail with `ContractPaused`
//! - Getters and read APIs succeed while paused
//! - Only admin can pause/unpause
//! - Admin can unpause to restore operations
//!
//! Security Notes:
//! - Bid operations: place_bid, cancel_bid, withdraw_bid, accept_bid, cleanup_expired_bids
//! - Escrow operations: accept_bid_and_fund, refund_escrow_funds, release_escrow_funds
//! - All mutating operations MUST check pause state before execution
//! - Read APIs (get_bid, get_escrow_details, etc.) MUST continue functioning

use crate::errors::QuickLendXError;
use crate::invoice::{InvoiceCategory, InvoiceStatus};
use crate::bid::BidStatus;
use crate::{QuickLendXContract, QuickLendXContractClient};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env, String, Vec};

fn setup(env: &Env) -> (QuickLendXContractClient, Address, Address, Address, Address) {
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    client.initialize_admin(&admin);
    let business = Address::generate(env);
    let investor = Address::generate(env);
    let currency = Address::generate(env);
    (client, admin, business, investor, currency)
}

fn verify_investor_for_test(
    env: &Env,
    client: &QuickLendXContractClient,
    investor: &Address,
    limit: i128,
) {
    client.submit_investor_kyc(investor, &String::from_str(env, "Investor KYC"));
    client.verify_investor(investor, &limit);
}

#[test]
fn test_when_paused_store_invoice_fails_with_contract_paused() {
    let env = Env::default();
    let (client, admin, business, _investor, currency) = setup(&env);
    let due_date = env.ledger().timestamp() + 86400;

    client.pause(&admin);
    assert!(client.is_paused());

    let result = client.try_store_invoice(
        &business,
        &1000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Test"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    let err = result.err().expect("expected contract error");
    let contract_error = err.expect("expected contract invoke error");
    assert_eq!(contract_error, QuickLendXError::ContractPaused);
}

#[test]
fn test_when_paused_place_bid_fails_with_contract_paused() {
    let env = Env::default();
    let (client, admin, business, investor, currency) = setup(&env);
    let due_date = env.ledger().timestamp() + 86400;

    let invoice_id = client.store_invoice(
        &business,
        &1000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    client.verify_invoice(&invoice_id);
    verify_investor_for_test(&env, &client, &investor, 10_000);

    client.pause(&admin);
    assert!(client.is_paused());

    let result = client.try_place_bid(&investor, &invoice_id, &1000i128, &1100i128);
    let err = result.err().expect("expected contract error");
    let contract_error = err.expect("expected contract invoke error");
    assert_eq!(contract_error, QuickLendXError::ContractPaused);
}

#[test]
fn test_when_paused_accept_bid_fails_with_contract_paused() {
    let env = Env::default();
    let (client, admin, business, investor, currency) = setup(&env);
    let due_date = env.ledger().timestamp() + 86400;

    let invoice_id = client.store_invoice(
        &business,
        &1000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    client.verify_invoice(&invoice_id);
    verify_investor_for_test(&env, &client, &investor, 10_000);
    let bid_id = client.place_bid(&investor, &invoice_id, &1000i128, &1100i128);

    client.pause(&admin);
    assert!(client.is_paused());

    let result = client.try_accept_bid(&invoice_id, &bid_id);
    let err = result.err().expect("expected contract error");
    let contract_error = err.expect("expected contract invoke error");
    assert_eq!(contract_error, QuickLendXError::ContractPaused);
}

#[test]
fn test_getters_succeed_when_paused() {
    let env = Env::default();
    let (client, admin, business, _investor, currency) = setup(&env);
    let due_date = env.ledger().timestamp() + 86400;

    let invoice_id = client.store_invoice(
        &business,
        &1000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    client.pause(&admin);
    assert!(client.is_paused());

    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.amount, 1000);
    assert_eq!(invoice.business, business);

    let list = client.get_business_invoices(&business);
    assert!(!list.is_empty());
    assert_eq!(client.get_current_admin(), Some(admin));
    assert!(client.get_whitelisted_currencies().len() >= 0);
}

#[test]
fn test_admin_can_unpause() {
    let env = Env::default();
    let (client, admin, business, _investor, currency) = setup(&env);
    let due_date = env.ledger().timestamp() + 86400;

    client.pause(&admin);
    assert!(client.is_paused());

    client.unpause(&admin);
    assert!(!client.is_paused());

    let invoice_id = client.store_invoice(
        &business,
        &500i128,
        &currency,
        &due_date,
        &String::from_str(&env, "After unpause"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.amount, 500);
}

#[test]
fn test_non_admin_cannot_pause() {
    let env = Env::default();
    let (client, _admin, non_admin, _investor, _currency) = setup(&env);
    env.mock_all_auths();

    let result = client.try_pause(&non_admin);
    let err = result.err().expect("expected contract error");
    let contract_error = err.expect("expected contract invoke error");
    assert_eq!(contract_error, QuickLendXError::NotAdmin);
}

#[test]
fn test_non_admin_cannot_unpause() {
    let env = Env::default();
    let (client, admin, non_admin, _investor, _currency) = setup(&env);
    client.pause(&admin);
    assert!(client.is_paused());

    let result = client.try_unpause(&non_admin);
    let err = result.err().expect("expected contract error");
    let contract_error = err.expect("expected contract invoke error");
    assert_eq!(contract_error, QuickLendXError::NotAdmin);
    assert!(client.is_paused());
}

#[test]
fn test_pause_blocks_cancel_invoice() {
    let env = Env::default();
    let (client, admin, business, _investor, currency) = setup(&env);
    let due_date = env.ledger().timestamp() + 86400;

    let invoice_id = client.store_invoice(
        &business,
        &1000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "To cancel"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    client.pause(&admin);

    let result = client.try_cancel_invoice(&invoice_id);
    let err = result.err().expect("expected contract error");
    let contract_error = err.expect("expected contract invoke error");
    assert_eq!(contract_error, QuickLendXError::ContractPaused);
}

#[test]
fn test_pause_blocks_withdraw_bid() {
    let env = Env::default();
    let (client, admin, business, investor, currency) = setup(&env);
    let due_date = env.ledger().timestamp() + 86400;

    let invoice_id = client.store_invoice(
        &business,
        &1000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    client.verify_invoice(&invoice_id);
    verify_investor_for_test(&env, &client, &investor, 10_000);
    let bid_id = client.place_bid(&investor, &invoice_id, &1000i128, &1100i128);

    client.pause(&admin);
    let result = client.try_withdraw_bid(&bid_id);
    let err = result.err().expect("expected contract error");
    let contract_error = err.expect("expected contract invoke error");
    assert_eq!(contract_error, QuickLendXError::ContractPaused);
}

#[test]
fn test_verify_invoice_fails_when_paused() {
    let env = Env::default();
    let (client, admin, business, _investor, currency) = setup(&env);
    let due_date = env.ledger().timestamp() + 86400;

    let invoice_id = client.store_invoice(
        &business,
        &1000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    client.pause(&admin);

    let result = client.try_verify_invoice(&invoice_id);
    let err = result.err().expect("expected contract error");
    let contract_error = err.expect("expected contract invoke error");
    assert_eq!(contract_error, QuickLendXError::ContractPaused);
}

#[test]
fn test_upload_invoice_fails_when_paused() {
    let env = Env::default();
    let (client, admin, business, _investor, currency) = setup(&env);
    env.mock_all_auths();
    client.submit_kyc_application(&business, &String::from_str(&env, "KYC"));
    client.verify_business(&admin, &business);
    let due_date = env.ledger().timestamp() + 86400;

    client.pause(&admin);

    let result = client.try_upload_invoice(
        &business,
        &1000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Upload"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    let err = result.err().expect("expected contract error");
    let contract_error = err.expect("expected contract invoke error");
    assert_eq!(contract_error, QuickLendXError::ContractPaused);
}

// ============================================================================
// Pause Bypass Tests for Bid Operations (#605)
// ============================================================================

#[test]
fn test_pause_blocks_cancel_bid() {
    let env = Env::default();
    let (client, admin, business, investor, currency) = setup(&env);
    let due_date = env.ledger().timestamp() + 86400;

    let invoice_id = client.store_invoice(
        &business,
        &1000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    client.verify_invoice(&invoice_id);
    verify_investor_for_test(&env, &client, &investor, 10_000);
    let bid_id = client.place_bid(&investor, &invoice_id, &1000i128, &1100i128);

    // Pause the contract
    client.pause(&admin);
    assert!(client.is_paused());

    // Attempt to cancel bid - should fail
    let result = client.try_cancel_bid(&bid_id);
    let err = result.err().expect("expected contract error");
    let contract_error = err.expect("expected contract invoke error");
    assert_eq!(contract_error, QuickLendXError::ContractPaused);

    // Verify bid status unchanged
    let bid = client.get_bid(&bid_id).unwrap();
    assert_eq!(bid.status, BidStatus::Placed);
}

#[test]
fn test_pause_blocks_cleanup_expired_bids() {
    let env = Env::default();
    let (client, admin, business, investor, currency) = setup(&env);
    let due_date = env.ledger().timestamp() + 86400;

    let invoice_id = client.store_invoice(
        &business,
        &1000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    client.verify_invoice(&invoice_id);
    verify_investor_for_test(&env, &client, &investor, 10_000);
    client.place_bid(&investor, &invoice_id, &1000i128, &1100i128);

    // Pause the contract
    client.pause(&admin);
    assert!(client.is_paused());

    // Attempt to cleanup expired bids - should fail
    let result = client.try_cleanup_expired_bids(&invoice_id);
    let err = result.err().expect("expected contract error");
    let contract_error = err.expect("expected contract invoke error");
    assert_eq!(contract_error, QuickLendXError::ContractPaused);
}

#[test]
fn test_pause_blocks_accept_bid_and_fund() {
    let env = Env::default();
    let (client, admin, business, investor, currency) = setup(&env);
    let due_date = env.ledger().timestamp() + 86400;

    let invoice_id = client.store_invoice(
        &business,
        &1000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    client.verify_invoice(&invoice_id);
    verify_investor_for_test(&env, &client, &investor, 10_000);
    let bid_id = client.place_bid(&investor, &invoice_id, &1000i128, &1100i128);

    // Pause before accepting bid
    client.pause(&admin);
    assert!(client.is_paused());

    // Attempt to accept bid and fund - should fail
    let result = client.try_accept_bid_and_fund(&invoice_id, &bid_id);
    let err = result.err().expect("expected contract error");
    let contract_error = err.expect("expected contract invoke error");
    assert_eq!(contract_error, QuickLendXError::ContractPaused);

    // Verify invoice status unchanged
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Verified);
}

#[test]
fn test_pause_blocks_refund_escrow_funds() {
    let env = Env::default();
    let (client, admin, business, investor, _currency) = setup(&env);
    env.mock_all_auths();

    // Setup verified business
    client.submit_kyc_application(&business, &String::from_str(&env, "Business KYC"));
    client.verify_business(&admin, &business);

    // Setup verified investor
    client.submit_investor_kyc(&investor, &String::from_str(&env, "Investor KYC"));
    client.verify_investor(&investor, &10_000i128);

    // Setup token
    let token_admin = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(token_admin.clone());
    let currency = sac.address();
    let token_client = soroban_sdk::token::Client::new(&env, &currency);
    let sac_client = soroban_sdk::token::StellarAssetClient::new(&env, &currency);
    sac_client.mint(&investor, &100_000i128);
    sac_client.mint(&business, &100_000i128);
    let expiration = env.ledger().sequence() + 10_000;
    token_client.approve(&investor, &client.address, &100_000i128, &expiration);
    token_client.approve(&business, &client.address, &100_000i128, &expiration);

    let due_date = env.ledger().timestamp() + 86400;
    let invoice_id = client.store_invoice(
        &business,
        &1000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    client.verify_invoice(&invoice_id);
    let bid_id = client.place_bid(&investor, &invoice_id, &1000i128, &1100i128);
    client.accept_bid(&invoice_id, &bid_id);

    // Verify invoice is funded
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Funded);

    // Pause the contract
    client.pause(&admin);
    assert!(client.is_paused());

    // Attempt to refund escrow funds - should fail
    let result = client.try_refund_escrow_funds(&invoice_id, &business);
    let err = result.err().expect("expected contract error");
    let contract_error = err.expect("expected contract invoke error");
    assert_eq!(contract_error, QuickLendXError::ContractPaused);
}

#[test]
fn test_pause_blocks_release_escrow_funds() {
    let env = Env::default();
    let (client, admin, business, investor, _currency) = setup(&env);
    env.mock_all_auths();

    // Setup verified business
    client.submit_kyc_application(&business, &String::from_str(&env, "Business KYC"));
    client.verify_business(&admin, &business);

    // Setup verified investor
    client.submit_investor_kyc(&investor, &String::from_str(&env, "Investor KYC"));
    client.verify_investor(&investor, &10_000i128);

    // Setup token
    let token_admin = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(token_admin.clone());
    let currency = sac.address();
    let token_client = soroban_sdk::token::Client::new(&env, &currency);
    let sac_client = soroban_sdk::token::StellarAssetClient::new(&env, &currency);
    sac_client.mint(&investor, &100_000i128);
    sac_client.mint(&business, &100_000i128);
    let expiration = env.ledger().sequence() + 10_000;
    token_client.approve(&investor, &client.address, &100_000i128, &expiration);
    token_client.approve(&business, &client.address, &100_000i128, &expiration);

    let due_date = env.ledger().timestamp() + 86400;
    let invoice_id = client.store_invoice(
        &business,
        &1000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    client.verify_invoice(&invoice_id);
    let bid_id = client.place_bid(&investor, &invoice_id, &1000i128, &1100i128);
    client.accept_bid(&invoice_id, &bid_id);

    // Verify invoice is funded but escrow not released
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Funded);

    // Pause the contract
    client.pause(&admin);
    assert!(client.is_paused());

    // Attempt to release escrow funds - should fail
    let result = client.try_release_escrow_funds(&invoice_id);
    let err = result.err().expect("expected contract error");
    let contract_error = err.expect("expected contract invoke error");
    assert_eq!(contract_error, QuickLendXError::ContractPaused);
}

// ============================================================================
// Read API Tests - Verify getters work while paused
// ============================================================================

#[test]
fn test_get_bid_works_when_paused() {
    let env = Env::default();
    let (client, admin, business, investor, currency) = setup(&env);
    let due_date = env.ledger().timestamp() + 86400;

    let invoice_id = client.store_invoice(
        &business,
        &1000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    client.verify_invoice(&invoice_id);
    verify_investor_for_test(&env, &client, &investor, 10_000);
    let bid_id = client.place_bid(&investor, &invoice_id, &1000i128, &1100i128);

    // Pause the contract
    client.pause(&admin);
    assert!(client.is_paused());

    // Get bid should succeed
    let bid = client.get_bid(&bid_id);
    assert!(bid.is_some());
    assert_eq!(bid.unwrap().bid_id, bid_id);
}

#[test]
fn test_get_bids_for_invoice_works_when_paused() {
    let env = Env::default();
    let (client, admin, business, investor, currency) = setup(&env);
    let due_date = env.ledger().timestamp() + 86400;

    let invoice_id = client.store_invoice(
        &business,
        &1000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    client.verify_invoice(&invoice_id);
    verify_investor_for_test(&env, &client, &investor, 10_000);
    client.place_bid(&investor, &invoice_id, &1000i128, &1100i128);

    // Pause the contract
    client.pause(&admin);
    assert!(client.is_paused());

    // Get bids for invoice should succeed
    let bids = client.get_bids_for_invoice(&invoice_id);
    assert!(!bids.is_empty());
}

#[test]
fn test_get_bids_by_status_works_when_paused() {
    let env = Env::default();
    let (client, admin, business, investor, currency) = setup(&env);
    let due_date = env.ledger().timestamp() + 86400;

    let invoice_id = client.store_invoice(
        &business,
        &1000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    client.verify_invoice(&invoice_id);
    verify_investor_for_test(&env, &client, &investor, 10_000);
    client.place_bid(&investor, &invoice_id, &1000i128, &1100i128);

    // Pause the contract
    client.pause(&admin);
    assert!(client.is_paused());

    // Get bids by status should succeed
    let bids = client.get_bids_by_status(&invoice_id, &BidStatus::Placed);
    assert!(!bids.is_empty());
}

#[test]
fn test_get_all_bids_by_investor_works_when_paused() {
    let env = Env::default();
    let (client, admin, business, investor, currency) = setup(&env);
    let due_date = env.ledger().timestamp() + 86400;

    let invoice_id = client.store_invoice(
        &business,
        &1000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    client.verify_invoice(&invoice_id);
    verify_investor_for_test(&env, &client, &investor, 10_000);
    client.place_bid(&investor, &invoice_id, &1000i128, &1100i128);

    // Pause the contract
    client.pause(&admin);
    assert!(client.is_paused());

    // Get all bids by investor should succeed
    let bids = client.get_all_bids_by_investor(&investor);
    assert!(!bids.is_empty());
}

#[test]
fn test_get_escrow_details_works_when_paused() {
    let env = Env::default();
    let (client, admin, business, investor, _currency) = setup(&env);
    env.mock_all_auths();

    // Setup verified business
    client.submit_kyc_application(&business, &String::from_str(&env, "Business KYC"));
    client.verify_business(&admin, &business);

    // Setup verified investor
    client.submit_investor_kyc(&investor, &String::from_str(&env, "Investor KYC"));
    client.verify_investor(&investor, &10_000i128);

    // Setup token
    let token_admin = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(token_admin.clone());
    let currency = sac.address();
    let token_client = soroban_sdk::token::Client::new(&env, &currency);
    let sac_client = soroban_sdk::token::StellarAssetClient::new(&env, &currency);
    sac_client.mint(&investor, &100_000i128);
    sac_client.mint(&business, &100_000i128);
    let expiration = env.ledger().sequence() + 10_000;
    token_client.approve(&investor, &client.address, &100_000i128, &expiration);
    token_client.approve(&business, &client.address, &100_000i128, &expiration);

    let due_date = env.ledger().timestamp() + 86400;
    let invoice_id = client.store_invoice(
        &business,
        &1000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    client.verify_invoice(&invoice_id);
    let bid_id = client.place_bid(&investor, &invoice_id, &1000i128, &1100i128);
    client.accept_bid(&invoice_id, &bid_id);

    // Pause the contract
    client.pause(&admin);
    assert!(client.is_paused());

    // Get escrow details should succeed
    let escrow = client.get_escrow_details(&invoice_id);
    assert_eq!(escrow.amount, 1000);
}

#[test]
fn test_get_ranked_bids_works_when_paused() {
    let env = Env::default();
    let (client, admin, business, investor, currency) = setup(&env);
    let due_date = env.ledger().timestamp() + 86400;

    let invoice_id = client.store_invoice(
        &business,
        &1000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    client.verify_invoice(&invoice_id);
    verify_investor_for_test(&env, &client, &investor, 10_000);
    client.place_bid(&investor, &invoice_id, &1000i128, &1100i128);

    // Pause the contract
    client.pause(&admin);
    assert!(client.is_paused());

    // Get ranked bids should succeed
    let bids = client.get_ranked_bids(&invoice_id);
    assert!(!bids.is_empty());
}

#[test]
fn test_get_best_bid_works_when_paused() {
    let env = Env::default();
    let (client, admin, business, investor, currency) = setup(&env);
    let due_date = env.ledger().timestamp() + 86400;

    let invoice_id = client.store_invoice(
        &business,
        &1000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    client.verify_invoice(&invoice_id);
    verify_investor_for_test(&env, &client, &investor, 10_000);
    client.place_bid(&investor, &invoice_id, &1000i128, &1100i128);

    // Pause the contract
    client.pause(&admin);
    assert!(client.is_paused());

    // Get best bid should succeed
    let best_bid = client.get_best_bid(&invoice_id);
    assert!(best_bid.is_some());
}

#[test]
fn test_is_paused_read_works_when_paused() {
    let env = Env::default();
    let (client, admin, _business, _investor, _currency) = setup(&env);

    client.pause(&admin);
    assert!(client.is_paused());

    // is_paused getter should work
    let paused = client.is_paused();
    assert!(paused);
}
