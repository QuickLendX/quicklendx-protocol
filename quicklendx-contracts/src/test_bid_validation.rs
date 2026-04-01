//! Bid validation tests for QuickLendX protocol.
//!
//! Test suite validates that bid validation enforces protocol minimums
//! consistently as required by issue #719.
//!
//! ## Test Coverage
//!
//! - **Protocol Minimum Enforcement**: Tests that bids respect both absolute and percentage minimums
//! - **Bid Amount Validation**: Tests various bid amounts against invoice amounts
//! - **Edge Cases**: Tests boundary conditions and error scenarios
//! - **Integration Tests**: Tests bid validation with actual bid placement
//!
//! Run: `cargo test test_bid_validation`

use super::*;
use crate::bid::BidStatus;
use crate::invoice::{InvoiceCategory, InvoiceStatus};
use crate::protocol_limits::{ProtocolLimits, ProtocolLimitsContract};
use crate::verification::validate_bid;
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Events, Ledger},
    Address, BytesN, Env, String, Vec,
};

/// Helper to create a verified invoice for testing
fn create_verified_invoice_for_bid_tests(
    env: &Env,
    client: &QuickLendXContractClient<'static>,
    business: &Address,
    amount: i128,
    currency: &Address,
) -> BytesN<32> {
    // Create and verify invoice
    let due_date = env.ledger().timestamp() + 86_400 * 30; // 30 days
    let invoice_id = client.store_invoice(
        business,
        &amount,
        currency,
        &due_date,
        &String::from_str(env, "Test invoice for bid validation"),
        &InvoiceCategory::Services,
        &Vec::new(env),
    );
    
    // Verify invoice
    client.verify_invoice(&invoice_id);
    invoice_id
}

/// Helper to setup investor for testing
fn setup_investor_for_bid_tests(
    env: &Env,
    client: &QuickLendXContractClient<'static>,
    investor: &Address,
    investment_limit: i128,
) {
    client.submit_investor_kyc(investor, &String::from_str(env, "Investor KYC"));
    client.verify_investor(investor, &investment_limit);
}

/// Helper to get current protocol limits
fn get_protocol_limits_for_test(env: &Env) -> ProtocolLimits {
    ProtocolLimitsContract::get_protocol_limits(env)
}

#[test]
fn test_bid_validation_enforces_absolute_minimum() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000_000);
    
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.set_admin(&admin);
    
    // Setup test addresses
    let business = Address::generate(&env);
    let investor = Address::generate(&env);
    let currency = Address::generate(&env);
    
    // Setup investor with high limit
    setup_investor_for_bid_tests(&env, &client, &investor, 10_000i128);
    
    // Create verified invoice
    let invoice_amount = 1_000i128;
    let invoice_id = create_verified_invoice_for_bid_tests(&env, &client, &business, invoice_amount, &currency);
    
    // Get current protocol limits
    let limits = get_protocol_limits_for_test(&env);
    
    // Test 1: Bid below absolute minimum should fail
    let result = validate_bid(&env, &client.get_invoice(&invoice_id), limits.min_bid_amount - 1, invoice_amount + 100, &investor);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), QuickLendXError::InvalidAmount);
    
    // Test 2: Bid exactly at absolute minimum should pass
    let result = validate_bid(&env, &client.get_invoice(&invoice_id), limits.min_bid_amount, invoice_amount + 100, &investor);
    assert!(result.is_ok());
    
    // Test 3: Bid above absolute minimum should pass
    let result = validate_bid(&env, &client.get_invoice(&invoice_id), limits.min_bid_amount + 100, invoice_amount + 200, &investor);
    assert!(result.is_ok());
}

#[test]
fn test_bid_validation_enforces_percentage_minimum() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000_000);
    
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.set_admin(&admin);
    
    // Setup test addresses
    let business = Address::generate(&env);
    let investor = Address::generate(&env);
    let currency = Address::generate(&env);
    
    // Setup investor with high limit
    setup_investor_for_bid_tests(&env, &client, &investor, 10_000i128);
    
    // Create verified invoice
    let invoice_amount = 5_000i128; // Larger amount to test percentage
    let invoice_id = create_verified_invoice_for_bid_tests(&env, &client, &business, invoice_amount, &currency);
    
    // Get current protocol limits
    let limits = get_protocol_limits_for_test(&env);
    
    // Calculate percentage-based minimum
    let percent_min = invoice_amount
        .saturating_mul(limits.min_bid_bps as i128)
        .saturating_div(10_000);
    
    // Test 1: Bid below percentage minimum should fail
    let result = validate_bid(&env, &client.get_invoice(&invoice_id), percent_min - 1, invoice_amount + 500, &investor);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), QuickLendXError::InvalidAmount);
    
    // Test 2: Bid exactly at percentage minimum should pass
    let result = validate_bid(&env, &client.get_invoice(&invoice_id), percent_min, invoice_amount + 500, &investor);
    assert!(result.is_ok());
    
    // Test 3: Bid above percentage minimum should pass
    let result = validate_bid(&env, &client.get_invoice(&invoice_id), percent_min + 100, invoice_amount + 600, &investor);
    assert!(result.is_ok());
}

#[test]
fn test_bid_validation_uses_higher_minimum() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000_000);
    
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.set_admin(&admin);
    
    // Setup test addresses
    let business = Address::generate(&env);
    let investor = Address::generate(&env);
    let currency = Address::generate(&env);
    
    // Setup investor with high limit
    setup_investor_for_bid_tests(&env, &client, &investor, 10_000i128);
    
    // Create verified invoice with amount that makes percentage minimum higher than absolute
    let invoice_amount = 2_000i128; // 2% = 40, which is > default min_bid_amount (10)
    let invoice_id = create_verified_invoice_for_bid_tests(&env, &client, &business, invoice_amount, &currency);
    
    // Get current protocol limits
    let limits = get_protocol_limits_for_test(&env);
    
    // Calculate percentage-based minimum
    let percent_min = invoice_amount
        .saturating_mul(limits.min_bid_bps as i128)
        .saturating_div(10_000);
    
    // Test: Bid should use percentage minimum (40) since it's higher than absolute minimum (10)
    let result = validate_bid(&env, &client.get_invoice(&invoice_id), 30, invoice_amount + 200, &investor);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), QuickLendXError::InvalidAmount);
    
    // Test: Bid at percentage minimum should pass
    let result = validate_bid(&env, &client.get_invoice(&invoice_id), percent_min, invoice_amount + 200, &investor);
    assert!(result.is_ok());
}

#[test]
fn test_bid_validation_with_custom_protocol_limits() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000_000);
    
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.set_admin(&admin);
    
    // Setup test addresses
    let business = Address::generate(&env);
    let investor = Address::generate(&env);
    let currency = Address::generate(&env);
    
    // Setup investor with high limit
    setup_investor_for_bid_tests(&env, &client, &investor, 10_000i128);
    
    // Create verified invoice
    let invoice_amount = 1_000i128;
    let invoice_id = create_verified_invoice_for_bid_tests(&env, &client, &business, invoice_amount, &currency);
    
    // Update protocol limits to custom values
    let custom_min_bid_amount = 50i128; // Higher than default
    let custom_min_bid_bps = 500u32; // 5%
    client.update_protocol_limits(
        admin,
        100i128, // min_invoice_amount
        custom_min_bid_amount,
        custom_min_bid_bps,
        365, // max_due_date_days
        604800, // grace_period_seconds (7 days)
        100, // max_invoices_per_business
    );
    
    // Get updated protocol limits
    let limits = get_protocol_limits_for_test(&env);
    assert_eq!(limits.min_bid_amount, custom_min_bid_amount);
    assert_eq!(limits.min_bid_bps, custom_min_bid_bps);
    
    // Test 1: Bid below new absolute minimum should fail
    let result = validate_bid(&env, &client.get_invoice(&invoice_id), custom_min_bid_amount - 1, invoice_amount + 100, &investor);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), QuickLendXError::InvalidAmount);
    
    // Test 2: Bid below new percentage minimum should fail
    let percent_min = invoice_amount
        .saturating_mul(custom_min_bid_bps as i128)
        .saturating_div(10_000);
    let result = validate_bid(&env, &client.get_invoice(&invoice_id), percent_min - 1, invoice_amount + 100, &investor);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), QuickLendXError::InvalidAmount);
    
    // Test 3: Bid at new absolute minimum should pass
    let result = validate_bid(&env, &client.get_invoice(&invoice_id), custom_min_bid_amount, invoice_amount + 100, &investor);
    assert!(result.is_ok());
    
    // Test 4: Bid at new percentage minimum should pass
    let result = validate_bid(&env, &client.get_invoice(&invoice_id), percent_min, invoice_amount + 100, &investor);
    assert!(result.is_ok());
}

#[test]
fn test_bid_validation_edge_cases() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000_000);
    
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.set_admin(&admin);
    
    // Setup test addresses
    let business = Address::generate(&env);
    let investor = Address::generate(&env);
    let currency = Address::generate(&env);
    
    // Setup investor with high limit
    setup_investor_for_bid_tests(&env, &client, &investor, 10_000i128);
    
    // Create verified invoice
    let invoice_amount = 1_000i128;
    let invoice_id = create_verified_invoice_for_bid_tests(&env, &client, &business, invoice_amount, &currency);
    
    // Test 1: Zero bid amount should fail
    let result = validate_bid(&env, &client.get_invoice(&invoice_id), 0, invoice_amount + 100, &investor);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), QuickLendXError::InvalidAmount);
    
    // Test 2: Negative bid amount should fail
    let result = validate_bid(&env, &client.get_invoice(&invoice_id), -100, invoice_amount + 100, &investor);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), QuickLendXError::InvalidAmount);
    
    // Test 3: Bid amount exceeding invoice amount should fail
    let result = validate_bid(&env, &client.get_invoice(&invoice_id), invoice_amount + 1, invoice_amount + 100, &investor);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), QuickLendXError::InvoiceAmountInvalid);
    
    // Test 4: Expected return less than or equal to bid amount should fail
    let result = validate_bid(&env, &client.get_invoice(&invoice_id), invoice_amount, invoice_amount, &investor);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), QuickLendXError::InvalidAmount);
    
    // Test 5: Valid bid should pass
    let result = validate_bid(&env, &client.get_invoice(&invoice_id), 100, invoice_amount + 100, &investor);
    assert!(result.is_ok());
}

#[test]
fn test_bid_validation_integration_with_place_bid() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000_000);
    
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.set_admin(&admin);
    
    // Setup test addresses
    let business = Address::generate(&env);
    let investor = Address::generate(&env);
    let currency = Address::generate(&env);
    
    // Setup investor with high limit
    setup_investor_for_bid_tests(&env, &client, &investor, 10_000i128);
    
    // Create verified invoice
    let invoice_amount = 1_000i128;
    let invoice_id = create_verified_invoice_for_bid_tests(&env, &client, &business, invoice_amount, &currency);
    
    // Get current protocol limits
    let limits = get_protocol_limits_for_test(&env);
    let percent_min = invoice_amount
        .saturating_mul(limits.min_bid_bps as i128)
        .saturating_div(10_000);
    let effective_min = if percent_min > limits.min_bid_amount {
        percent_min
    } else {
        limits.min_bid_amount
    };
    
    // Test 1: Place bid below minimum should fail
    let result = client.place_bid(&investor, &invoice_id, &(effective_min - 1), &(invoice_amount + 100));
    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), QuickLendXError::InvalidAmount);
    
    // Test 2: Place bid at minimum should succeed
    let bid_id = client.place_bid(&investor, &invoice_id, &effective_min, &(invoice_amount + 100));
    assert!(bid_id != BytesN::from_array(&env, &[0; 32]));
    
    // Verify bid was created and has correct amount
    let bid = client.get_bid(&bid_id);
    assert_eq!(bid.bid_amount, effective_min);
    assert_eq!(bid.status, BidStatus::Placed);
    
    // Test 3: Place second bid from same investor should fail (active bid protection)
    let result = client.place_bid(&investor, &invoice_id, &(effective_min + 100), &(invoice_amount + 200));
    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), QuickLendXError::OperationNotAllowed);
}

#[test]
fn test_bid_validation_with_invoice_status_checks() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000_000);
    
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.set_admin(&admin);
    
    // Setup test addresses
    let business = Address::generate(&env);
    let investor = Address::generate(&env);
    let currency = Address::generate(&env);
    
    // Setup investor with high limit
    setup_investor_for_bid_tests(&env, &client, &investor, 10_000i128);
    
    // Create invoice but don't verify it
    let invoice_amount = 1_000i128;
    let due_date = env.ledger().timestamp() + 86_400 * 30;
    let invoice_id = client.store_invoice(
        business,
        &invoice_amount,
        &currency,
        &due_date,
        &String::from_str(env, "Unverified invoice"),
        &InvoiceCategory::Services,
        &Vec::new(env),
    );
    
    // Get current protocol limits
    let limits = get_protocol_limits_for_test(&env);
    let valid_bid_amount = limits.min_bid_amount + 100;
    
    // Test 1: Bid on unverified invoice should fail
    let result = validate_bid(&env, &client.get_invoice(&invoice_id), valid_bid_amount, invoice_amount + 100, &investor);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), QuickLendXError::InvalidStatus);
    
    // Test 2: Bid on expired invoice should fail
    let mut invoice = client.get_invoice(&invoice_id);
    invoice.status = InvoiceStatus::Verified;
    invoice.due_date = env.ledger().timestamp() - 1000; // Past due date
    let result = validate_bid(&env, &invoice, valid_bid_amount, invoice_amount + 100, &investor);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), QuickLendXError::InvalidStatus);
    
    // Test 3: Business bidding on own invoice should fail
    invoice.status = InvoiceStatus::Verified;
    invoice.due_date = env.ledger().timestamp() + 1000; // Future due date
    let result = validate_bid(&env, &invoice, valid_bid_amount, invoice_amount + 100, &business);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), QuickLendXError::Unauthorized);
}
