//! Escrow detail-status consistency tests for QuickLendX protocol.
//!
//! Test suite ensures that status field matches detailed escrow record
//! across all terminal states as required by issue #731.
//!
//! ## Test Coverage
//!
//! - **Status Consistency**: Tests that get_escrow_status matches escrow.status
//! - **Detail Completeness**: Tests that get_escrow_details returns complete escrow record
//! - **Terminal State Coverage**: Tests all escrow states (Held, Released, Refunded)
//! - **Edge Cases**: Tests error conditions and missing escrow scenarios
//! - **Integration Tests**: Tests consistency during actual escrow operations
//!
//! Run: `cargo test test_escrow_consistency`

use super::*;
use crate::invoice::{InvoiceCategory, InvoiceStatus};
use crate::payments::{create_escrow, release_escrow, refund_escrow, EscrowStatus};
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Events, Ledger},
    token, Address, BytesN, Env, String, Vec,
};

/// Helper to create a funded invoice with escrow for testing
fn create_funded_invoice_with_escrow_for_tests(
    env: &Env,
    client: &QuickLendXContractClient<'static>,
    business: &Address,
    investor: &Address,
    invoice_amount: i128,
    escrow_amount: i128,
) -> (BytesN<32>, BytesN<32>) {
    // Setup business KYC and verification
    client.submit_kyc_application(business, &String::from_str(env, "Business KYC"));
    let admin = Address::generate(env);
    client.verify_business(&admin, business);
    
    // Setup investor KYC and verification
    client.submit_investor_kyc(investor, &String::from_str(env, "Investor KYC"));
    client.verify_investor(&admin, investor, &(escrow_amount * 2));
    
    // Create and verify invoice
    let currency = Address::generate(env);
    let due_date = env.ledger().timestamp() + 86_400 * 30;
    let invoice_id = client.store_invoice(
        business,
        &invoice_amount,
        &currency,
        &due_date,
        &String::from_str(env, "Test invoice for escrow consistency"),
        &InvoiceCategory::Services,
        &Vec::new(env),
    );
    client.verify_invoice(&invoice_id);
    
    // Create bid and accept to create escrow
    let bid_id = client.place_bid(investor, &invoice_id, &escrow_amount, &(invoice_amount + 100));
    client.accept_bid(&invoice_id, &bid_id);
    
    (invoice_id, bid_id)
}

/// Helper to get escrow status and details for consistency checking
fn get_escrow_status_and_details(
    env: &Env,
    client: &QuickLendXContractClient<'static>,
    invoice_id: &BytesN<32>,
) -> (EscrowStatus, crate::payments::Escrow) {
    let status = client.get_escrow_status(invoice_id).unwrap();
    let details = client.get_escrow_details(invoice_id).unwrap();
    (status, details)
}

#[test]
fn test_escrow_status_consistency_held_state() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000_000);
    
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    
    let business = Address::generate(&env);
    let investor = Address::generate(&env);
    
    // Create funded invoice with escrow
    let (invoice_id, _bid_id) = create_funded_invoice_with_escrow_for_tests(
        &env,
        &client,
        &business,
        &investor,
        1000i128,
        1000i128,
    );
    
    // Test: Status and details should be consistent for Held state
    let (status, details) = get_escrow_status_and_details(&env, &client, &invoice_id);
    
    assert_eq!(status, EscrowStatus::Held);
    assert_eq!(details.status, EscrowStatus::Held);
    assert_eq!(details.invoice_id, invoice_id);
    assert_eq!(details.investor, investor);
    assert_eq!(details.business, business);
    assert_eq!(details.amount, 1000i128);
    assert!(details.created_at > 0);
}

#[test]
fn test_escrow_status_consistency_released_state() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000_000);
    
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    
    let business = Address::generate(&env);
    let investor = Address::generate(&env);
    
    // Create funded invoice with escrow
    let (invoice_id, _bid_id) = create_funded_invoice_with_escrow_for_tests(
        &env,
        &client,
        &business,
        &investor,
        1000i128,
        1000i128,
    );
    
    // Release escrow
    client.release_escrow_funds(&invoice_id);
    
    // Test: Status and details should be consistent for Released state
    let (status, details) = get_escrow_status_and_details(&env, &client, &invoice_id);
    
    assert_eq!(status, EscrowStatus::Released);
    assert_eq!(details.status, EscrowStatus::Released);
    assert_eq!(details.invoice_id, invoice_id);
    assert_eq!(details.investor, investor);
    assert_eq!(details.business, business);
    assert_eq!(details.amount, 1000i128);
    assert!(details.created_at > 0);
}

#[test]
fn test_escrow_status_consistency_refunded_state() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000_000);
    
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    
    let business = Address::generate(&env);
    let investor = Address::generate(&env);
    
    // Create funded invoice with escrow
    let (invoice_id, _bid_id) = create_funded_invoice_with_escrow_for_tests(
        &env,
        &client,
        &business,
        &investor,
        1000i128,
        1000i128,
    );
    
    // Refund escrow
    client.refund_escrow_funds(&invoice_id, investor);
    
    // Test: Status and details should be consistent for Refunded state
    let (status, details) = get_escrow_status_and_details(&env, &client, &invoice_id);
    
    assert_eq!(status, EscrowStatus::Refunded);
    assert_eq!(details.status, EscrowStatus::Refunded);
    assert_eq!(details.invoice_id, invoice_id);
    assert_eq!(details.investor, investor);
    assert_eq!(details.business, business);
    assert_eq!(details.amount, 1000i128);
    assert!(details.created_at > 0);
}

#[test]
fn test_escrow_status_consistency_multiple_operations() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000_000);
    
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    
    let business = Address::generate(&env);
    let investor = Address::generate(&env);
    
    // Create funded invoice with escrow
    let (invoice_id, _bid_id) = create_funded_invoice_with_escrow_for_tests(
        &env,
        &client,
        &business,
        &investor,
        1000i128,
        1000i128,
    );
    
    // Test: Initial state should be Held
    let (status1, details1) = get_escrow_status_and_details(&env, &client, &invoice_id);
    assert_eq!(status1, EscrowStatus::Held);
    assert_eq!(details1.status, EscrowStatus::Held);
    
    // Release escrow
    client.release_escrow_funds(&invoice_id);
    
    // Test: Status should be Released after release
    let (status2, details2) = get_escrow_status_and_details(&env, &client, &invoice_id);
    assert_eq!(status2, EscrowStatus::Released);
    assert_eq!(details2.status, EscrowStatus::Released);
    
    // Verify details are consistent across operations
    assert_eq!(details1.invoice_id, details2.invoice_id);
    assert_eq!(details1.investor, details2.investor);
    assert_eq!(details1.business, details2.business);
    assert_eq!(details1.amount, details2.amount);
    assert_eq!(details1.created_at, details2.created_at);
}

#[test]
fn test_escrow_status_error_missing_escrow() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000_000);
    
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    
    // Test non-existent invoice ID
    let fake_invoice_id = BytesN::from_array(&env, &[1; 32]);
    
    // Both functions should return StorageKeyNotFound error
    let status_result = client.get_escrow_status(&fake_invoice_id);
    assert!(status_result.is_err());
    assert_eq!(status_result.unwrap_err(), QuickLendXError::StorageKeyNotFound);
    
    let details_result = client.get_escrow_details(&fake_invoice_id);
    assert!(details_result.is_err());
    assert_eq!(details_result.unwrap_err(), QuickLendXError::StorageKeyNotFound);
}

#[test]
fn test_escrow_status_consistency_with_real_token_transfers() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000_000);
    
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    
    let business = Address::generate(&env);
    let investor = Address::generate(&env);
    let currency = Address::generate(&env);
    
    // Setup real token with initial balances
    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin);
    let token_client = token::Client::new(&env, &token_contract);
    
    let initial_balance = 10_000i128;
    token_client.mint(&business, &initial_balance);
    token_client.mint(&investor, &initial_balance);
    token_client.mint(&contract_id, &1i128);
    
    let expiration = env.ledger().sequence() + 1000;
    token_client.approve(&business, &contract_id, &initial_balance, &expiration);
    token_client.approve(&investor, &contract_id, &initial_balance, &expiration);
    
    // Create and verify invoice
    let due_date = env.ledger().timestamp() + 86_400 * 30;
    let invoice_id = client.store_invoice(
        &business,
        &1000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Test invoice with real escrow"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    client.verify_invoice(&invoice_id);
    
    // Setup and verify investor
    client.submit_investor_kyc(&investor, &String::from_str(&env, "Investor KYC"));
    let admin = Address::generate(&env);
    client.verify_investor(&admin, &investor, &5000i128);
    
    // Place bid and accept to create real escrow
    let bid_id = client.place_bid(&investor, &invoice_id, &1000i128, &1200i128);
    client.accept_bid(&invoice_id, &bid_id);
    
    // Test: Real escrow should be created and consistent
    let (status, details) = get_escrow_status_and_details(&env, &client, &invoice_id);
    
    assert_eq!(status, EscrowStatus::Held);
    assert_eq!(details.status, EscrowStatus::Held);
    assert_eq!(details.invoice_id, invoice_id);
    assert_eq!(details.investor, investor);
    assert_eq!(details.business, business);
    assert_eq!(details.amount, 1000i128);
    assert!(details.created_at > 0);
    
    // Verify actual token transfers occurred
    assert_eq!(token_client.balance(&investor), initial_balance - 1000i128);
    assert_eq!(token_client.balance(&contract_id), 1000i128);
}

#[test]
fn test_escrow_status_consistency_state_transitions() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000_000);
    
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    
    let business = Address::generate(&env);
    let investor = Address::generate(&env);
    
    // Create funded invoice with escrow
    let (invoice_id, _bid_id) = create_funded_invoice_with_escrow_for_tests(
        &env,
        &client,
        &business,
        &investor,
        1000i128,
        1000i128,
    );
    
    // Test state transitions and consistency at each step
    let mut previous_details: Option<crate::payments::Escrow> = None;
    
    // Initial: Held state
    let (status1, details1) = get_escrow_status_and_details(&env, &client, &invoice_id);
    assert_eq!(status1, EscrowStatus::Held);
    assert_eq!(details1.status, EscrowStatus::Held);
    previous_details = Some(details1.clone());
    
    // After release: Released state
    client.release_escrow_funds(&invoice_id);
    let (status2, details2) = get_escrow_status_and_details(&env, &client, &invoice_id);
    assert_eq!(status2, EscrowStatus::Released);
    assert_eq!(details2.status, EscrowStatus::Released);
    
    // Verify only status changed, other fields remained same
    if let Some(prev) = previous_details {
        assert_eq!(prev.invoice_id, details2.invoice_id);
        assert_eq!(prev.investor, details2.investor);
        assert_eq!(prev.business, details2.business);
        assert_eq!(prev.amount, details2.amount);
        assert_eq!(prev.created_at, details2.created_at);
    }
    
    // Test that Released cannot be released again (idempotency)
    let result = client.release_escrow_funds(&invoice_id);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), QuickLendXError::InvalidStatus);
}

#[test]
fn test_escrow_status_consistency_edge_cases() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000_000);
    
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    
    let business = Address::generate(&env);
    let investor = Address::generate(&env);
    
    // Create funded invoice with escrow
    let (invoice_id, _bid_id) = create_funded_invoice_with_escrow_for_tests(
        &env,
        &client,
        &business,
        &investor,
        1000i128,
        1000i128,
    );
    
    // Test 1: Details should match status immediately after creation
    let (status, details) = get_escrow_status_and_details(&env, &client, &invoice_id);
    assert_eq!(status, EscrowStatus::Held);
    assert_eq!(details.status, EscrowStatus::Held);
    
    // Test 2: Timestamp should be reasonable (not zero, not in future)
    let current_time = env.ledger().timestamp();
    assert!(details.created_at > 0);
    assert!(details.created_at <= current_time);
    
    // Test 3: All required fields should be present
    assert!(!details.invoice_id.is_zero());
    assert!(!details.investor.is_zero());
    assert!(!details.business.is_zero());
    assert!(details.amount > 0);
    assert!(!details.currency.is_zero());
    
    // Test 4: Fields should be consistent with each other
    // (This would require additional validation logic in a real implementation)
    // For now, we test that the basic consistency holds
}

#[test]
fn test_escrow_status_consistency_with_multiple_invoices() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000_000);
    
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    
    let business = Address::generate(&env);
    let investor = Address::generate(&env);
    
    // Create two invoices with escrows
    let (invoice_id1, _bid_id1) = create_funded_invoice_with_escrow_for_tests(
        &env,
        &client,
        &business,
        &investor,
        1000i128,
        1000i128,
    );
    
    let (invoice_id2, _bid_id2) = create_funded_invoice_with_escrow_for_tests(
        &env,
        &client,
        &business,
        &investor,
        2000i128,
        2000i128,
    );
    
    // Release first escrow
    client.release_escrow_funds(&invoice_id1);
    
    // Test: Each escrow should have consistent status/details independently
    let (status1, details1) = get_escrow_status_and_details(&env, &client, &invoice_id1);
    let (status2, details2) = get_escrow_status_and_details(&env, &client, &invoice_id2);
    
    // First escrow should be Released
    assert_eq!(status1, EscrowStatus::Released);
    assert_eq!(details1.status, EscrowStatus::Released);
    
    // Second escrow should still be Held
    assert_eq!(status2, EscrowStatus::Held);
    assert_eq!(details2.status, EscrowStatus::Held);
    
    // Verify escrows are independent
    assert_ne!(details1.invoice_id, details2.invoice_id);
    assert_ne!(details1.amount, details2.amount);
}
