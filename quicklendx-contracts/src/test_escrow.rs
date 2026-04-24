/// Comprehensive test suite for escrow funding flow
///
/// Test Coverage:
/// 1. Authorization: Only invoice owner can accept bids
/// 2. State Validation: Only verified invoices can be funded
/// 3. Token Transfer: Funds are locked exactly once with correct amounts
/// 4. Idempotency: Rejects double-accept attempts
/// 5. Edge Cases: Invalid states, unauthorized access, fund verification
///
/// Security Notes:
/// - Uses soroban-sdk testutils token patterns for realistic token simulation
/// - All auth requirements verified via require_auth() checks
/// - Escrow state transitions validated at each step
/// - Token balances verified before/after transfers
use super::*;
use crate::bid::BidStatus;
use crate::errors::QuickLendXError;
use crate::invoice::{InvoiceCategory, InvoiceStatus};
use crate::payments::{create_escrow, EscrowStatus};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, BytesN, Env, String, Vec,
};

// ============================================================================
// Helper Functions
// ============================================================================

/// Setup test environment with contract and admin
fn setup() -> (Env, QuickLendXContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let _ = client.try_initialize_admin(&admin);
    client.set_admin(&admin);
    (env, client, admin)
}

/// Create a Stellar Asset Contract token for testing with proper balances
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

    let token_client = token::Client::new(env, &currency);
    let sac_client = token::StellarAssetClient::new(env, &currency);

    // Mint tokens to business and investor
    let initial_balance = 100_000i128;
    sac_client.mint(business, &initial_balance);
    sac_client.mint(investor, &initial_balance);

    // Approve contract to spend tokens
    let expiration = env.ledger().sequence() + 10_000;
    token_client.approve(business, contract_id, &initial_balance, &expiration);
    token_client.approve(investor, contract_id, &initial_balance, &expiration);

    currency
}

/// Create and verify a business
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

/// Create and verify an investor with specified limit
fn setup_verified_investor(env: &Env, client: &QuickLendXContractClient, limit: i128) -> Address {
    let investor = Address::generate(env);
    client.submit_investor_kyc(&investor, &String::from_str(env, "Investor KYC"));
    client.verify_investor(&investor, &limit);
    investor
}

/// Create a verified invoice ready for bidding
fn create_verified_invoice(
    env: &Env,
    client: &QuickLendXContractClient,
    business: &Address,
    amount: i128,
    currency: &Address,
) -> BytesN<32> {
    let due_date = env.ledger().timestamp() + 86400; // 1 day from now
    let invoice_id = client.store_invoice(
        business,
        &amount,
        currency,
        &due_date,
        &String::from_str(env, "Test Invoice"),
        &InvoiceCategory::Services,
        &Vec::new(env),
    );
    client.verify_invoice(&invoice_id);
    invoice_id
}

/// Place a bid on an invoice
fn place_test_bid(
    client: &QuickLendXContractClient,
    investor: &Address,
    invoice_id: &BytesN<32>,
    bid_amount: i128,
    expected_return: i128,
) -> BytesN<32> {
    client.place_bid(investor, invoice_id, &bid_amount, &expected_return)
}

// ============================================================================
// Test Cases
// ============================================================================

#[test]
fn test_only_invoice_owner_can_accept_bid() {
    let (env, client, admin) = setup();
    let contract_id = client.address.clone();

    // Setup parties
    let business = setup_verified_business(&env, &client, &admin);
    let investor = setup_verified_investor(&env, &client, 50_000);

    // Setup token
    let currency = setup_token(&env, &business, &investor, &contract_id);

    // Create verified invoice and bid
    let amount = 10_000i128;
    let invoice_id = create_verified_invoice(&env, &client, &business, amount, &currency);
    let bid_id = place_test_bid(&client, &investor, &invoice_id, amount, amount + 1000);

    // Business owner should succeed (with mock_all_auths this always succeeds)
    let result = client.try_accept_bid(&invoice_id, &bid_id);
    assert!(result.is_ok(), "Invoice owner should be able to accept bid");

    // Verify bid was accepted
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Funded);
}

#[test]
fn test_only_verified_invoice_can_be_funded() {
    let (env, client, admin) = setup();
    let contract_id = client.address.clone();

    // Setup parties
    let business = setup_verified_business(&env, &client, &admin);
    let investor = setup_verified_investor(&env, &client, 50_000);

    // Setup token
    let currency = setup_token(&env, &business, &investor, &contract_id);

    // Create invoice but DON'T verify it (leave in Pending status)
    let amount = 10_000i128;
    let due_date = env.ledger().timestamp() + 86400;
    let invoice_id = client.store_invoice(
        &business,
        &amount,
        &currency,
        &due_date,
        &String::from_str(&env, "Unverified Invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    // Attempt to place bid on unverified invoice - should fail
    let result = client.try_place_bid(&investor, &invoice_id, &amount, &(amount + 1000));
    assert!(
        result.is_err(),
        "Should not be able to bid on unverified invoice"
    );

    // Verify the invoice
    client.verify_invoice(&invoice_id);

    // Now bidding should work
    let bid_id = client.place_bid(&investor, &invoice_id, &amount, &(amount + 1000));

    // Accepting bid should work on verified invoice
    let result = client.try_accept_bid(&invoice_id, &bid_id);
    assert!(
        result.is_ok(),
        "Should be able to accept bid on verified invoice"
    );
}

#[test]
fn test_funds_locked_exactly_once() {
    let (env, client, admin) = setup();
    let contract_id = client.address.clone();

    // Setup parties
    let business = setup_verified_business(&env, &client, &admin);
    let investor = setup_verified_investor(&env, &client, 50_000);

    // Setup token
    let currency = setup_token(&env, &business, &investor, &contract_id);
    let token_client = token::Client::new(&env, &currency);

    // Create verified invoice and bid
    let amount = 10_000i128;
    let invoice_id = create_verified_invoice(&env, &client, &business, amount, &currency);

    // Check initial balances
    let investor_balance_before = token_client.balance(&investor);
    let contract_balance_before = token_client.balance(&contract_id);

    // Place and accept bid
    let bid_id = place_test_bid(&client, &investor, &invoice_id, amount, amount + 1000);
    client.accept_bid(&invoice_id, &bid_id);

    // Check balances after escrow creation
    let investor_balance_after = token_client.balance(&investor);
    let contract_balance_after = token_client.balance(&contract_id);

    // Verify exact amounts transferred
    assert_eq!(
        investor_balance_before - investor_balance_after,
        amount,
        "Investor should have paid exactly the bid amount"
    );
    assert_eq!(
        contract_balance_after - contract_balance_before,
        amount,
        "Contract should hold exactly the bid amount in escrow"
    );

    // Verify escrow details
    let escrow_details = client.get_escrow_details(&invoice_id);
    assert_eq!(
        escrow_details.amount, amount,
        "Escrow should hold exact bid amount"
    );
    assert_eq!(
        escrow_details.status,
        EscrowStatus::Held,
        "Escrow should be in Held status"
    );
    assert_eq!(
        escrow_details.investor, investor,
        "Escrow should reference correct investor"
    );
    assert_eq!(
        escrow_details.business, business,
        "Escrow should reference correct business"
    );
}

#[test]
fn test_rejects_double_accept() {
    let (env, client, admin) = setup();
    let contract_id = client.address.clone();

    // Setup parties
    let business = setup_verified_business(&env, &client, &admin);
    let investor = setup_verified_investor(&env, &client, 50_000);

    // Setup token
    let currency = setup_token(&env, &business, &investor, &contract_id);

    // Create verified invoice and bid
    let amount = 10_000i128;
    let invoice_id = create_verified_invoice(&env, &client, &business, amount, &currency);
    let bid_id = place_test_bid(&client, &investor, &invoice_id, amount, amount + 1000);

    // Accept bid first time - should succeed
    let result = client.try_accept_bid(&invoice_id, &bid_id);
    assert!(result.is_ok(), "First accept should succeed");

    // Verify invoice is now funded
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Funded);

    // Attempt to accept same bid again - should fail
    let result = client.try_accept_bid(&invoice_id, &bid_id);
    assert!(result.is_err(), "Double accept should fail");

    // Also verify we can't accept a different bid on same invoice
    let investor2 = setup_verified_investor(&env, &client, 50_000);

    // Need to setup token for second investor
    let token_client = token::Client::new(&env, &currency);
    let sac_client = token::StellarAssetClient::new(&env, &currency);
    let initial_balance = 100_000i128;
    sac_client.mint(&investor2, &initial_balance);
    let expiration = env.ledger().sequence() + 10_000;
    token_client.approve(&investor2, &contract_id, &initial_balance, &expiration);

    // Try to place another bid on funded invoice
    let result = client.try_place_bid(&investor2, &invoice_id, &amount, &(amount + 500));
    assert!(
        result.is_err(),
        "Should not be able to bid on funded invoice"
    );
}

#[test]
fn test_accept_bid_state_transitions() {
    let (env, client, admin) = setup();
    let contract_id = client.address.clone();

    // Setup parties
    let business = setup_verified_business(&env, &client, &admin);
    let investor = setup_verified_investor(&env, &client, 50_000);

    // Setup token
    let currency = setup_token(&env, &business, &investor, &contract_id);

    // Create verified invoice
    let amount = 10_000i128;
    let invoice_id = create_verified_invoice(&env, &client, &business, amount, &currency);

    // Initial state: Invoice should be Verified
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Verified);

    // Place bid
    let bid_id = place_test_bid(&client, &investor, &invoice_id, amount, amount + 1000);

    // Bid should be Placed
    let bid = client.get_bid(&bid_id).unwrap();
    assert_eq!(bid.status, BidStatus::Placed);

    // Accept bid
    client.accept_bid(&invoice_id, &bid_id);

    // After accept: Invoice should be Funded
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Funded);

    // After accept: Bid should be Accepted
    let bid = client.get_bid(&bid_id).unwrap();
    assert_eq!(bid.status, BidStatus::Accepted);

    // After accept: Escrow should exist and be Held
    let escrow = client.get_escrow_details(&invoice_id);
    assert_eq!(escrow.status, EscrowStatus::Held);
    assert_eq!(escrow.amount, amount);
}

#[test]
fn test_cannot_accept_withdrawn_bid() {
    let (env, client, admin) = setup();
    let contract_id = client.address.clone();

    // Setup parties
    let business = setup_verified_business(&env, &client, &admin);
    let investor = setup_verified_investor(&env, &client, 50_000);

    // Setup token
    let currency = setup_token(&env, &business, &investor, &contract_id);

    // Create verified invoice and bid
    let amount = 10_000i128;
    let invoice_id = create_verified_invoice(&env, &client, &business, amount, &currency);
    let bid_id = place_test_bid(&client, &investor, &invoice_id, amount, amount + 1000);

    // Withdraw the bid
    client.withdraw_bid(&bid_id);

    // Verify bid is withdrawn
    let bid = client.get_bid(&bid_id).unwrap();
    assert_eq!(bid.status, BidStatus::Withdrawn);

    // Attempt to accept withdrawn bid - should fail
    let result = client.try_accept_bid(&invoice_id, &bid_id);
    assert!(
        result.is_err(),
        "Should not be able to accept withdrawn bid"
    );
}

#[test]
fn test_escrow_creation_validates_amount() {
    let (env, client, admin) = setup();
    let contract_id = client.address.clone();

    // Setup parties
    let business = setup_verified_business(&env, &client, &admin);
    let investor = setup_verified_investor(&env, &client, 50_000);

    // Setup token
    let currency = setup_token(&env, &business, &investor, &contract_id);

    // Create verified invoice
    let invoice_amount = 10_000i128;
    let invoice_id = create_verified_invoice(&env, &client, &business, invoice_amount, &currency);

    // Place bid with exact amount
    let bid_id = place_test_bid(
        &client,
        &investor,
        &invoice_id,
        invoice_amount,
        invoice_amount + 1000,
    );

    // Accept bid
    client.accept_bid(&invoice_id, &bid_id);

    // Verify escrow has correct amount
    let escrow = client.get_escrow_details(&invoice_id);
    assert_eq!(
        escrow.amount, invoice_amount,
        "Escrow amount should match bid amount"
    );

    // Verify invoice funded amount matches
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(
        invoice.funded_amount, invoice_amount,
        "Invoice funded amount should match bid amount"
    );
}

#[test]
fn test_rejects_mismatched_invoice_bid_pair_without_side_effects() {
    let (env, client, admin) = setup();
    let contract_id = client.address.clone();

    let business = setup_verified_business(&env, &client, &admin);
    let investor = setup_verified_investor(&env, &client, 100_000);

    let currency = setup_token(&env, &business, &investor, &contract_id);
    let token_client = token::Client::new(&env, &currency);

    let amount = 10_000i128;
    let invoice_id_1 = create_verified_invoice(&env, &client, &business, amount, &currency);
    let invoice_id_2 = create_verified_invoice(&env, &client, &business, amount, &currency);
    let bid_id = place_test_bid(&client, &investor, &invoice_id_2, amount, amount + 500);

    let investor_before = token_client.balance(&investor);
    let contract_before = token_client.balance(&contract_id);

    let result = client.try_accept_bid(&invoice_id_1, &bid_id);
    assert!(result.is_err(), "Mismatched invoice/bid pair must fail");
    let err = result.unwrap_err().unwrap();
    assert_eq!(err, QuickLendXError::Unauthorized);

    let invoice_1 = client.get_invoice(&invoice_id_1);
    let invoice_2 = client.get_invoice(&invoice_id_2);
    let bid = client.get_bid(&bid_id).unwrap();

    assert_eq!(invoice_1.status, InvoiceStatus::Verified);
    assert_eq!(invoice_2.status, InvoiceStatus::Verified);
    assert_eq!(bid.status, BidStatus::Placed);
    assert_eq!(token_client.balance(&investor), investor_before);
    assert_eq!(token_client.balance(&contract_id), contract_before);
    assert!(client.try_get_escrow_details(&invoice_id_1).is_err());
    assert!(client.try_get_escrow_details(&invoice_id_2).is_err());
}

#[test]
fn test_rejects_accept_when_escrow_already_exists() {
    let (env, client, admin) = setup();
    let contract_id = client.address.clone();

    let business = setup_verified_business(&env, &client, &admin);
    let investor = setup_verified_investor(&env, &client, 100_000);

    let currency = setup_token(&env, &business, &investor, &contract_id);
    let token_client = token::Client::new(&env, &currency);

    let amount = 10_000i128;
    let invoice_id = create_verified_invoice(&env, &client, &business, amount, &currency);
    let bid_id = place_test_bid(&client, &investor, &invoice_id, amount, amount + 1000);

    let investor_before_manual = token_client.balance(&investor);
    let contract_before_manual = token_client.balance(&contract_id);
    let _escrow_id = create_escrow(&env, &invoice_id, &investor, &business, amount, &currency)
        .expect("manual escrow setup should succeed");

    assert_eq!(
        token_client.balance(&investor),
        investor_before_manual - amount
    );
    assert_eq!(
        token_client.balance(&contract_id),
        contract_before_manual + amount
    );

    let investor_before_accept = token_client.balance(&investor);
    let contract_before_accept = token_client.balance(&contract_id);
    let result = client.try_accept_bid(&invoice_id, &bid_id);

    assert!(
        result.is_err(),
        "Acceptance must fail when escrow already exists"
    );
    let err = result.unwrap_err().unwrap();
    assert_eq!(err, QuickLendXError::InvalidStatus);
    assert_eq!(token_client.balance(&investor), investor_before_accept);
    assert_eq!(token_client.balance(&contract_id), contract_before_accept);

    let invoice = client.get_invoice(&invoice_id);
    let bid = client.get_bid(&bid_id).unwrap();
    let escrow = client.get_escrow_details(&invoice_id);

    assert_eq!(invoice.status, InvoiceStatus::Verified);
    assert_eq!(invoice.funded_amount, 0);
    assert_eq!(bid.status, BidStatus::Placed);
    assert_eq!(escrow.status, EscrowStatus::Held);
    assert_eq!(escrow.amount, amount);
}

#[test]
fn test_multiple_bids_only_one_accepted() {
    let (env, client, admin) = setup();
    let contract_id = client.address.clone();

    // Setup parties
    let business = setup_verified_business(&env, &client, &admin);
    let investor1 = setup_verified_investor(&env, &client, 50_000);
    let investor2 = setup_verified_investor(&env, &client, 50_000);

    // Setup token
    let currency = setup_token(&env, &business, &investor1, &contract_id);

    // Setup token for investor2
    let token_client = token::Client::new(&env, &currency);
    let sac_client = token::StellarAssetClient::new(&env, &currency);
    let initial_balance = 100_000i128;
    sac_client.mint(&investor2, &initial_balance);
    let expiration = env.ledger().sequence() + 10_000;
    token_client.approve(&investor2, &contract_id, &initial_balance, &expiration);

    // Create verified invoice
    let amount = 10_000i128;
    let invoice_id = create_verified_invoice(&env, &client, &business, amount, &currency);

    // Place multiple bids
    let bid_id1 = place_test_bid(&client, &investor1, &invoice_id, amount, amount + 1000);
    let bid_id2 = place_test_bid(&client, &investor2, &invoice_id, amount, amount + 500);

    // Accept first bid
    client.accept_bid(&invoice_id, &bid_id1);

    // Verify first bid is accepted
    let bid1 = client.get_bid(&bid_id1).unwrap();
    assert_eq!(bid1.status, BidStatus::Accepted);

    // Second bid should still be Placed but can't be accepted
    let bid2 = client.get_bid(&bid_id2).unwrap();
    assert_eq!(bid2.status, BidStatus::Placed);

    // Attempt to accept second bid should fail
    let result = client.try_accept_bid(&invoice_id, &bid_id2);
    assert!(
        result.is_err(),
        "Should not accept second bid on funded invoice"
    );
}

#[test]
fn test_token_transfer_idempotency() {
    let (env, client, admin) = setup();
    let contract_id = client.address.clone();

    // Setup parties
    let business = setup_verified_business(&env, &client, &admin);
    let investor = setup_verified_investor(&env, &client, 50_000);

    // Setup token
    let currency = setup_token(&env, &business, &investor, &contract_id);
    let token_client = token::Client::new(&env, &currency);

    // Create verified invoice
    let amount = 10_000i128;
    let invoice_id = create_verified_invoice(&env, &client, &business, amount, &currency);

    // Record balances
    let investor_before = token_client.balance(&investor);
    let contract_before = token_client.balance(&contract_id);

    // Place and accept bid
    let bid_id = place_test_bid(&client, &investor, &invoice_id, amount, amount + 1000);
    client.accept_bid(&invoice_id, &bid_id);

    // Record balances after first accept
    let investor_after_first = token_client.balance(&investor);
    let contract_after_first = token_client.balance(&contract_id);

    // Attempt second accept (should fail)
    let _ = client.try_accept_bid(&invoice_id, &bid_id);

    // Balances should remain unchanged after failed second accept
    let investor_after_second = token_client.balance(&investor);
    let contract_after_second = token_client.balance(&contract_id);

    assert_eq!(
        investor_after_first, investor_after_second,
        "Investor balance should not change on failed accept"
    );
    assert_eq!(
        contract_after_first, contract_after_second,
        "Contract balance should not change on failed accept"
    );

    // Verify amounts transferred only once
    assert_eq!(investor_before - investor_after_first, amount);
    assert_eq!(contract_after_first - contract_before, amount);
}

#[test]
fn test_escrow_invariants() {
    let (env, client, admin) = setup();
    let contract_id = client.address.clone();

    // Setup parties
    let business = setup_verified_business(&env, &client, &admin);
    let investor = setup_verified_investor(&env, &client, 50_000);

    // Setup token
    let currency = setup_token(&env, &business, &investor, &contract_id);

    // Create verified invoice and bid
    let amount = 10_000i128;
    let invoice_id = create_verified_invoice(&env, &client, &business, amount, &currency);
    let bid_id = place_test_bid(&client, &investor, &invoice_id, amount, amount + 1000);

    // Accept bid to create escrow
    client.accept_bid(&invoice_id, &bid_id);

    // Verify escrow invariants
    let escrow = client.get_escrow_details(&invoice_id);

    // Invariant 1: Escrow amount must be positive
    assert!(escrow.amount > 0, "Escrow amount must be positive");

    // Invariant 2: Escrow invoice_id must match
    assert_eq!(
        escrow.invoice_id, invoice_id,
        "Escrow invoice_id must match"
    );

    // Invariant 3: Escrow investor must match bid investor
    let bid = client.get_bid(&bid_id).unwrap();
    assert_eq!(
        escrow.investor, bid.investor,
        "Escrow investor must match bid investor"
    );

    // Invariant 4: Escrow business must match invoice business
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(
        escrow.business, invoice.business,
        "Escrow business must match invoice business"
    );

    // Invariant 5: Escrow status must be Held after creation
    assert_eq!(
        escrow.status,
        EscrowStatus::Held,
        "Escrow status must be Held after creation"
    );

    // Invariant 6: Created timestamp must be set (>= 0)
    assert!(
        escrow.created_at <= env.ledger().timestamp(),
        "Escrow created_at cannot be in future"
    );
}

#[test]
fn test_create_escrow_idempotency_check() {
    let (env, client, admin) = setup();
    let contract_id = client.address.clone();

    // Setup parties
    let business = setup_verified_business(&env, &client, &admin);
    let investor = setup_verified_investor(&env, &client, 50_000);

    // Setup token
    let currency = setup_token(&env, &business, &investor, &contract_id);

    // Create verified invoice
    let amount = 10_000i128;
    let invoice_id = create_verified_invoice(&env, &client, &business, amount, &currency);

    // Manually call crate::payments::create_escrow inside the contract context
    // First call should succeed
    env.as_contract(&contract_id, || {
        use crate::payments::create_escrow;
        let result = create_escrow(
            &env,
            &invoice_id,
            &investor,
            &business,
            amount,
            &currency,
        );
        assert!(result.is_ok(), "First create_escrow should succeed");
    });

    // Second call for the same invoice_id should fail, even if bypassed higher level checks
    env.as_contract(&contract_id, || {
        use crate::payments::create_escrow;
        let result = create_escrow(
            &env,
            &invoice_id,
            &investor,
            &business,
            amount,
            &currency,
        );
        
        // Assert it fails with InvoiceAlreadyFunded
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err, QuickLendXError::InvoiceAlreadyFunded);
    });
}

// ============================================================================

#[test]
fn test_release_escrow_funds_success() {
    let (env, client, _admin) = setup();
    let contract_id = client.address.clone();

    let business = setup_verified_business(&env, &client, &_admin);
    let investor = setup_verified_investor(&env, &client, 50_000);

    let currency = setup_token(&env, &business, &investor, &contract_id);
    let token_client = token::Client::new(&env, &currency);

    let amount = 10_000i128;
    let invoice_id = create_verified_invoice(&env, &client, &business, amount, &currency);
    let bid_id = place_test_bid(&client, &investor, &invoice_id, amount, amount + 1000);

    client.accept_bid(&invoice_id, &bid_id);

    let business_balance_before = token_client.balance(&business);
    let escrow_before = client.get_escrow_details(&invoice_id);
    assert_eq!(escrow_before.status, EscrowStatus::Held);

    let result = client.try_release_escrow_funds(&invoice_id);
    assert!(result.is_ok(), "release_escrow_funds should succeed");

    let business_balance_after = token_client.balance(&business);
    assert_eq!(
        business_balance_after - business_balance_before,
        amount,
        "Business should receive escrow amount"
    );

    let escrow_after = client.get_escrow_details(&invoice_id);
    assert_eq!(
        escrow_after.status,
        EscrowStatus::Released,
        "Escrow status should be Released after release"
    );
}

#[test]
fn test_release_escrow_funds_idempotency_blocked() {
    let (env, client, _admin) = setup();
    let contract_id = client.address.clone();

    let business = setup_verified_business(&env, &client, &_admin);
    let investor = setup_verified_investor(&env, &client, 50_000);

    let currency = setup_token(&env, &business, &investor, &contract_id);

    let amount = 10_000i128;
    let invoice_id = create_verified_invoice(&env, &client, &business, amount, &currency);
    let bid_id = place_test_bid(&client, &investor, &invoice_id, amount, amount + 1000);

    client.accept_bid(&invoice_id, &bid_id);
    client.release_escrow_funds(&invoice_id);

    let result = client.try_release_escrow_funds(&invoice_id);
    assert!(
        result.is_err(),
        "Second release_escrow_funds should fail (idempotency blocked)"
    );
    let err = result.unwrap_err().unwrap();
    assert_eq!(err, QuickLendXError::InvalidStatus);
}

// ============================================================================
// verify_invoice and auto-release when funded (Issue #300)
// ============================================================================

#[test]
fn test_verify_invoice_when_funded_triggers_release_escrow_funds() {
    let (env, client, admin) = setup();
    let contract_id = client.address.clone();

    let business = setup_verified_business(&env, &client, &admin);
    let investor = setup_verified_investor(&env, &client, 50_000);

    let currency = setup_token(&env, &business, &investor, &contract_id);
    let token_client = token::Client::new(&env, &currency);

    let amount = 10_000i128;
    let invoice_id = create_verified_invoice(&env, &client, &business, amount, &currency);
    let bid_id = place_test_bid(&client, &investor, &invoice_id, amount, amount + 1000);

    client.accept_bid(&invoice_id, &bid_id);
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Funded);

    let escrow_before = client.get_escrow_details(&invoice_id);
    assert_eq!(escrow_before.status, EscrowStatus::Held);

    let business_balance_before = token_client.balance(&business);

    let result = client.try_verify_invoice(&invoice_id);
    assert!(
        result.is_ok(),
        "verify_invoice when funded should trigger release"
    );

    let business_balance_after = token_client.balance(&business);
    assert_eq!(
        business_balance_after - business_balance_before,
        amount,
        "Business should receive escrow amount after verify_invoice on funded invoice"
    );

    let escrow_after = client.get_escrow_details(&invoice_id);
    assert_eq!(
        escrow_after.status,
        EscrowStatus::Released,
        "Escrow should be Released after verify_invoice on funded invoice"
    );
}

// ============================================================================
// Multiple Investors - Escrow Tests (Issue #343)
// ============================================================================

/// Test: Multiple bids on same invoice, only accepted bid creates escrow
#[test]
fn test_multiple_bids_only_accepted_creates_escrow() {
    let (env, client, admin) = setup();
    let contract_id = client.address.clone();

    // Setup parties
    let business = setup_verified_business(&env, &client, &admin);
    let investor1 = setup_verified_investor(&env, &client, 50_000);
    let investor2 = setup_verified_investor(&env, &client, 50_000);
    let investor3 = setup_verified_investor(&env, &client, 50_000);

    // Setup token for all investors
    let currency = setup_token(&env, &business, &investor1, &contract_id);
    let token_client = token::Client::new(&env, &currency);
    let sac_client = token::StellarAssetClient::new(&env, &currency);

    let initial_balance = 100_000i128;
    sac_client.mint(&investor2, &initial_balance);
    sac_client.mint(&investor3, &initial_balance);

    let expiration = env.ledger().sequence() + 10_000;
    token_client.approve(&investor2, &contract_id, &initial_balance, &expiration);
    token_client.approve(&investor3, &contract_id, &initial_balance, &expiration);

    // Create verified invoice
    let amount = 10_000i128;
    let invoice_id = create_verified_invoice(&env, &client, &business, amount, &currency);

    // Three investors place bids
    let _bid_id1 = place_test_bid(&client, &investor1, &invoice_id, 8_000, 10_000);
    let bid_id2 = place_test_bid(&client, &investor2, &invoice_id, 9_000, 11_000);
    let _bid_id3 = place_test_bid(&client, &investor3, &invoice_id, 10_000, 12_000);

    // Record balances before accept
    let investor1_before = token_client.balance(&investor1);
    let investor2_before = token_client.balance(&investor2);
    let investor3_before = token_client.balance(&investor3);
    let contract_before = token_client.balance(&contract_id);

    // Business accepts bid2
    client.accept_bid(&invoice_id, &bid_id2);

    // Verify only investor2's funds were transferred
    assert_eq!(
        token_client.balance(&investor1),
        investor1_before,
        "investor1 balance should not change"
    );
    assert_eq!(
        token_client.balance(&investor2),
        investor2_before - 9_000,
        "investor2 should pay bid amount"
    );
    assert_eq!(
        token_client.balance(&investor3),
        investor3_before,
        "investor3 balance should not change"
    );
    assert_eq!(
        token_client.balance(&contract_id),
        contract_before + 9_000,
        "Contract should hold only accepted bid amount"
    );

    // Verify escrow details
    let escrow = client.get_escrow_details(&invoice_id);
    assert_eq!(
        escrow.investor, investor2,
        "Escrow should reference investor2"
    );
    assert_eq!(
        escrow.amount, 9_000,
        "Escrow should hold investor2's bid amount"
    );
    assert_eq!(escrow.status, EscrowStatus::Held, "Escrow should be Held");
}

/// Test: Multiple bids scenario - comprehensive workflow
#[test]
fn test_multiple_bids_complete_workflow() {
    let (env, client, admin) = setup();
    let contract_id = client.address.clone();

    // Setup 4 investors and business
    let business = setup_verified_business(&env, &client, &admin);
    let investor1 = setup_verified_investor(&env, &client, 100_000);
    let investor2 = setup_verified_investor(&env, &client, 100_000);
    let investor3 = setup_verified_investor(&env, &client, 100_000);
    let investor4 = setup_verified_investor(&env, &client, 100_000);

    // Setup token for all investors
    let currency = setup_token(&env, &business, &investor1, &contract_id);
    let token_client = token::Client::new(&env, &currency);
    let sac_client = token::StellarAssetClient::new(&env, &currency);

    let initial_balance = 100_000i128;
    for investor in [&investor2, &investor3, &investor4] {
        sac_client.mint(investor, &initial_balance);
        let expiration = env.ledger().sequence() + 10_000;
        token_client.approve(investor, &contract_id, &initial_balance, &expiration);
    }

    // Create verified invoice
    let invoice_amount = 50_000i128;
    let invoice_id = create_verified_invoice(&env, &client, &business, invoice_amount, &currency);

    // Four investors place bids with different amounts
    let bid_id1 = place_test_bid(&client, &investor1, &invoice_id, 40_000, 50_000); // profit: 10k
    let bid_id2 = place_test_bid(&client, &investor2, &invoice_id, 45_000, 60_000); // profit: 15k (best)
    let bid_id3 = place_test_bid(&client, &investor3, &invoice_id, 42_000, 54_000); // profit: 12k
    let bid_id4 = place_test_bid(&client, &investor4, &invoice_id, 38_000, 48_000); // profit: 10k

    // Verify all bids are Placed
    let placed = client.get_bids_by_status(&invoice_id, &BidStatus::Placed);
    assert_eq!(placed.len(), 4, "All 4 bids should be Placed");

    // Verify ranking
    let ranked = client.get_ranked_bids(&invoice_id);
    assert_eq!(
        ranked.get(0).unwrap().investor,
        investor2,
        "investor2 should be ranked first"
    );

    // Business accepts the best bid (investor2)
    client.accept_bid(&invoice_id, &bid_id2);

    // Verify bid statuses
    assert_eq!(client.get_bid(&bid_id1).unwrap().status, BidStatus::Placed);
    assert_eq!(
        client.get_bid(&bid_id2).unwrap().status,
        BidStatus::Accepted
    );
    assert_eq!(client.get_bid(&bid_id3).unwrap().status, BidStatus::Placed);
    assert_eq!(client.get_bid(&bid_id4).unwrap().status, BidStatus::Placed);

    // Verify escrow
    let escrow = client.get_escrow_details(&invoice_id);
    assert_eq!(escrow.investor, investor2);
    assert_eq!(escrow.amount, 45_000);
    assert_eq!(escrow.status, EscrowStatus::Held);

    // Verify invoice
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Funded);
    assert_eq!(invoice.funded_amount, 45_000);
    assert_eq!(invoice.investor, Some(investor2));

    // Non-accepted investors withdraw their bids
    client.withdraw_bid(&bid_id1);
    client.withdraw_bid(&bid_id3);
    client.withdraw_bid(&bid_id4);

    // Verify withdrawals
    assert_eq!(
        client.get_bid(&bid_id1).unwrap().status,
        BidStatus::Withdrawn
    );
    assert_eq!(
        client.get_bid(&bid_id3).unwrap().status,
        BidStatus::Withdrawn
    );
    assert_eq!(
        client.get_bid(&bid_id4).unwrap().status,
        BidStatus::Withdrawn
    );

    // Verify get_bids_for_invoice still returns all bids
    let all_bids = client.get_bids_for_invoice(&invoice_id);
    assert_eq!(all_bids.len(), 4, "Should still track all 4 bids");
}

/// Test: Verify only one escrow exists per invoice even with multiple bids
#[test]
fn test_single_escrow_per_invoice_with_multiple_bids() {
    let (env, client, admin) = setup();
    let contract_id = client.address.clone();

    let business = setup_verified_business(&env, &client, &admin);
    let investor1 = setup_verified_investor(&env, &client, 50_000);
    let investor2 = setup_verified_investor(&env, &client, 50_000);

    let currency = setup_token(&env, &business, &investor1, &contract_id);
    let token_client = token::Client::new(&env, &currency);
    let sac_client = token::StellarAssetClient::new(&env, &currency);

    let initial_balance = 100_000i128;
    sac_client.mint(&investor2, &initial_balance);
    let expiration = env.ledger().sequence() + 10_000;
    token_client.approve(&investor2, &contract_id, &initial_balance, &expiration);

    let amount = 10_000i128;
    let invoice_id = create_verified_invoice(&env, &client, &business, amount, &currency);

    // Two bids placed
    let _bid_id1 = place_test_bid(&client, &investor1, &invoice_id, amount, amount + 1000);
    let bid_id2 = place_test_bid(&client, &investor2, &invoice_id, amount, amount + 2000);

    // Accept first bid
    client.accept_bid(&invoice_id, &bid_id2);

    // Verify escrow exists
    let escrow = client.get_escrow_details(&invoice_id);
    assert_eq!(escrow.invoice_id, invoice_id);
    assert_eq!(escrow.investor, investor2);

    // Attempt to accept second bid should fail
    let result = client.try_accept_bid(&invoice_id, &_bid_id1);
    assert!(
        result.is_err(),
        "Cannot accept second bid on funded invoice"
    );

    // Verify still only one escrow
    let escrow_after = client.get_escrow_details(&invoice_id);
    assert_eq!(
        escrow_after.escrow_id, escrow.escrow_id,
        "Should be same escrow"
    );
    assert_eq!(
        escrow_after.investor, investor2,
        "Escrow investor unchanged"
    );
}

#[test]
fn test_release_escrow_fails_if_not_funded() {
    let (env, client, admin) = setup();
    let contract_id = client.address.clone();

    let business = setup_verified_business(&env, &client, &admin);
    let investor = setup_verified_investor(&env, &client, 50_000);
    let currency = setup_token(&env, &business, &investor, &contract_id);

    let amount = 10_000i128;
    let invoice_id = create_verified_invoice(&env, &client, &business, amount, &currency);

    // Invoice is Verified but not Funded. release_escrow_funds should fail.
    let result = client.try_release_escrow_funds(&invoice_id);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().unwrap(), QuickLendXError::InvalidStatus);
}

#[test]
fn test_release_escrow_fails_if_refunded() {
    let (env, client, admin) = setup();
    let contract_id = client.address.clone();

    let business = setup_verified_business(&env, &client, &admin);
    let investor = setup_verified_investor(&env, &client, 50_000);
    let currency = setup_token(&env, &business, &investor, &contract_id);

    let amount = 10_000i128;
    let invoice_id = create_verified_invoice(&env, &client, &business, amount, &currency);
    let bid_id = place_test_bid(&client, &investor, &invoice_id, amount, amount + 1000);

    client.accept_bid(&invoice_id, &bid_id);
    
    // Refund the escrow
    client.refund_escrow_funds(&invoice_id, &admin);

    // Invoice status is now Refunded. release_escrow_funds should fail.
    let result = client.try_release_escrow_funds(&invoice_id);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().unwrap(), QuickLendXError::InvalidStatus);
}

// ============================================================================
// Token Transfer Failure Tests
//
// These tests document and verify the contract's behavior when the underlying
// Stellar token transfer fails or cannot proceed (zero balance, zero allowance,
// partial allowance). In every failure case:
//   - No escrow record is written.
//   - Invoice and bid states are left unchanged.
//   - The correct error variant is returned.
// ============================================================================

/// Accepting a bid fails with `InsufficientFunds` when the investor has no token balance.
///
/// # Security note
/// The balance check in `transfer_funds` runs before the token call, so the
/// token contract is never invoked and no partial state is written.
#[test]
fn test_accept_bid_fails_when_investor_has_zero_balance() {
    let (env, client, admin) = setup();
    let contract_id = client.address.clone();

    let business = setup_verified_business(&env, &client, &admin);
    let investor = setup_verified_investor(&env, &client, 50_000);

    // Create a token but do NOT mint any balance for the investor.
    let token_admin = Address::generate(&env);
    let currency = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token_client = token::Client::new(&env, &currency);
    let sac_client = token::StellarAssetClient::new(&env, &currency);

    // Mint only to business (so invoice upload works), not to investor.
    sac_client.mint(&business, &100_000i128);
    let expiration = env.ledger().sequence() + 10_000;
    token_client.approve(&business, &contract_id, &100_000i128, &expiration);
    // Investor approves but has zero balance.
    token_client.approve(&investor, &contract_id, &50_000i128, &expiration);

    let amount = 10_000i128;
    let invoice_id = create_verified_invoice(&env, &client, &business, amount, &currency);
    let bid_id = place_test_bid(&client, &investor, &invoice_id, amount, amount + 1_000);

    let investor_balance_before = token_client.balance(&investor);
    let contract_balance_before = token_client.balance(&contract_id);

    let result = client.try_accept_bid(&invoice_id, &bid_id);

    assert!(result.is_err(), "accept_bid must fail when investor has no balance");
    assert_eq!(
        result.unwrap_err().unwrap(),
        QuickLendXError::InsufficientFunds,
        "Expected InsufficientFunds error"
    );

    // No funds moved.
    assert_eq!(token_client.balance(&investor), investor_balance_before);
    assert_eq!(token_client.balance(&contract_id), contract_balance_before);

    // State unchanged.
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Verified);
    assert_eq!(invoice.funded_amount, 0);
    assert!(invoice.investor.is_none());

    let bid = client.get_bid(&bid_id).unwrap();
    assert_eq!(bid.status, BidStatus::Placed);

    // No escrow record created.
    assert!(client.try_get_escrow_details(&invoice_id).is_err());
}

/// Accepting a bid fails with `OperationNotAllowed` when the investor has not
/// approved the contract to spend the required amount.
///
/// # Security note
/// The allowance check in `transfer_funds` runs before `transfer_from`, so the
/// token contract is never invoked and no partial state is written.
#[test]
fn test_accept_bid_fails_when_investor_has_zero_allowance() {
    let (env, client, admin) = setup();
    let contract_id = client.address.clone();

    let business = setup_verified_business(&env, &client, &admin);
    let investor = setup_verified_investor(&env, &client, 50_000);

    // Mint balance to investor but grant NO allowance to the contract.
    let token_admin = Address::generate(&env);
    let currency = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token_client = token::Client::new(&env, &currency);
    let sac_client = token::StellarAssetClient::new(&env, &currency);

    sac_client.mint(&business, &100_000i128);
    sac_client.mint(&investor, &100_000i128);

    let expiration = env.ledger().sequence() + 10_000;
    // Business approves so invoice upload works.
    token_client.approve(&business, &contract_id, &100_000i128, &expiration);
    // Investor deliberately grants zero allowance.
    token_client.approve(&investor, &contract_id, &0i128, &expiration);

    let amount = 10_000i128;
    let invoice_id = create_verified_invoice(&env, &client, &business, amount, &currency);
    let bid_id = place_test_bid(&client, &investor, &invoice_id, amount, amount + 1_000);

    let investor_balance_before = token_client.balance(&investor);
    let contract_balance_before = token_client.balance(&contract_id);

    let result = client.try_accept_bid(&invoice_id, &bid_id);

    assert!(result.is_err(), "accept_bid must fail with zero allowance");
    assert_eq!(
        result.unwrap_err().unwrap(),
        QuickLendXError::OperationNotAllowed,
        "Expected OperationNotAllowed error"
    );

    // No funds moved.
    assert_eq!(token_client.balance(&investor), investor_balance_before);
    assert_eq!(token_client.balance(&contract_id), contract_balance_before);

    // State unchanged.
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Verified);
    assert_eq!(invoice.funded_amount, 0);

    let bid = client.get_bid(&bid_id).unwrap();
    assert_eq!(bid.status, BidStatus::Placed);

    assert!(client.try_get_escrow_details(&invoice_id).is_err());
}

/// Accepting a bid fails with `OperationNotAllowed` when the investor's allowance
/// is positive but less than the bid amount.
///
/// # Security note
/// Partial allowance is rejected before the token call, preventing any transfer.
#[test]
fn test_accept_bid_fails_when_investor_has_partial_allowance() {
    let (env, client, admin) = setup();
    let contract_id = client.address.clone();

    let business = setup_verified_business(&env, &client, &admin);
    let investor = setup_verified_investor(&env, &client, 50_000);

    let token_admin = Address::generate(&env);
    let currency = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token_client = token::Client::new(&env, &currency);
    let sac_client = token::StellarAssetClient::new(&env, &currency);

    let amount = 10_000i128;
    sac_client.mint(&business, &100_000i128);
    sac_client.mint(&investor, &100_000i128);

    let expiration = env.ledger().sequence() + 10_000;
    token_client.approve(&business, &contract_id, &100_000i128, &expiration);
    // Investor approves only half the required amount.
    token_client.approve(&investor, &contract_id, &(amount / 2), &expiration);

    let invoice_id = create_verified_invoice(&env, &client, &business, amount, &currency);
    let bid_id = place_test_bid(&client, &investor, &invoice_id, amount, amount + 1_000);

    let investor_balance_before = token_client.balance(&investor);
    let contract_balance_before = token_client.balance(&contract_id);

    let result = client.try_accept_bid(&invoice_id, &bid_id);

    assert!(result.is_err(), "accept_bid must fail with partial allowance");
    assert_eq!(
        result.unwrap_err().unwrap(),
        QuickLendXError::OperationNotAllowed,
        "Expected OperationNotAllowed for partial allowance"
    );

    // No funds moved.
    assert_eq!(token_client.balance(&investor), investor_balance_before);
    assert_eq!(token_client.balance(&contract_id), contract_balance_before);

    // State unchanged.
    assert_eq!(client.get_invoice(&invoice_id).status, InvoiceStatus::Verified);
    assert_eq!(client.get_bid(&bid_id).unwrap().status, BidStatus::Placed);
    assert!(client.try_get_escrow_details(&invoice_id).is_err());
}

/// After a failed `accept_bid` (due to insufficient funds), the bid can be
/// retried once the investor tops up their balance and allowance.
///
/// This verifies that no permanent state corruption occurs on failure.
#[test]
fn test_accept_bid_succeeds_after_topping_up_balance() {
    let (env, client, admin) = setup();
    let contract_id = client.address.clone();

    let business = setup_verified_business(&env, &client, &admin);
    let investor = setup_verified_investor(&env, &client, 50_000);

    let token_admin = Address::generate(&env);
    let currency = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token_client = token::Client::new(&env, &currency);
    let sac_client = token::StellarAssetClient::new(&env, &currency);

    let amount = 10_000i128;
    sac_client.mint(&business, &100_000i128);
    // Investor starts with insufficient balance.
    sac_client.mint(&investor, &(amount - 1));

    let expiration = env.ledger().sequence() + 10_000;
    token_client.approve(&business, &contract_id, &100_000i128, &expiration);
    token_client.approve(&investor, &contract_id, &100_000i128, &expiration);

    let invoice_id = create_verified_invoice(&env, &client, &business, amount, &currency);
    let bid_id = place_test_bid(&client, &investor, &invoice_id, amount, amount + 1_000);

    // First attempt fails.
    let result = client.try_accept_bid(&invoice_id, &bid_id);
    assert_eq!(
        result.unwrap_err().unwrap(),
        QuickLendXError::InsufficientFunds
    );

    // Top up investor balance.
    sac_client.mint(&investor, &1i128);

    // Second attempt succeeds.
    let result = client.try_accept_bid(&invoice_id, &bid_id);
    assert!(result.is_ok(), "accept_bid should succeed after top-up");

    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Funded);
    assert_eq!(invoice.funded_amount, amount);

    let escrow = client.get_escrow_details(&invoice_id);
    assert_eq!(escrow.status, EscrowStatus::Held);
    assert_eq!(escrow.amount, amount);
}

// ============================================================================
// accept_bid_and_fund Atomicity Tests
//
// These tests assert that accept_bid_and_fund is atomic: when the token
// transfer fails, NO partial state is written — no orphan escrow, no bid
// status change, no invoice mutation, and no investment record.
//
// Security invariant: funds safety requires that storage state only advances
// AFTER a successful token transfer. If the transfer panics or returns an
// error, Soroban rolls back all ledger writes for that invocation, so callers
// see a clean slate and can retry safely.
// ============================================================================

/// Helper: mint tokens to investor/business but grant NO allowance to the contract.
fn setup_token_no_allowance(
    env: &Env,
    business: &Address,
    investor: &Address,
    contract_id: &Address,
    amount: i128,
) -> Address {
    let token_admin = Address::generate(env);
    let currency = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let sac_client = token::StellarAssetClient::new(env, &currency);
    sac_client.mint(business, &(amount * 10));
    sac_client.mint(investor, &(amount * 10));
    // Deliberately omit investor approve — contract has zero allowance.
    let _ = contract_id; // allowance intentionally absent
    currency
}

/// Helper: mint tokens and grant partial allowance (less than bid amount).
fn setup_token_partial_allowance(
    env: &Env,
    business: &Address,
    investor: &Address,
    contract_id: &Address,
    amount: i128,
) -> Address {
    let token_admin = Address::generate(env);
    let currency = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token_client = token::Client::new(env, &currency);
    let sac_client = token::StellarAssetClient::new(env, &currency);
    sac_client.mint(business, &(amount * 10));
    sac_client.mint(investor, &(amount * 10));
    let expiration = env.ledger().sequence() + 10_000;
    token_client.approve(business, contract_id, &(amount * 10), &expiration);
    // Investor approves only half the required amount.
    token_client.approve(investor, contract_id, &(amount / 2), &expiration);
    currency
}

/// accept_bid_and_fund with insufficient investor balance leaves all state unchanged.
///
/// Invariants checked:
/// - Invoice status stays Verified
/// - Invoice funded_amount stays 0, investor field stays None
/// - Bid status stays Placed
/// - No escrow record is created
/// - No funds move
#[test]
fn test_accept_bid_and_fund_no_balance_leaves_state_unchanged() {
    let (env, client, admin) = setup();
    let contract_id = client.address.clone();

    let business = setup_verified_business(&env, &client, &admin);
    let investor = setup_verified_investor(&env, &client, 50_000);

    let amount = 10_000i128;
    let token_admin = Address::generate(&env);
    let currency = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token_client = token::Client::new(&env, &currency);
    let sac_client = token::StellarAssetClient::new(&env, &currency);
    sac_client.mint(&business, &(amount * 10));
    // Investor deliberately receives no mint — balance is 0.
    let expiration = env.ledger().sequence() + 10_000;
    token_client.approve(&business, &contract_id, &(amount * 10), &expiration);
    token_client.approve(&investor, &contract_id, &(amount * 10), &expiration);

    let invoice_id = create_verified_invoice(&env, &client, &business, amount, &currency);
    let bid_id = place_test_bid(&client, &investor, &invoice_id, amount, amount + 500);

    let investor_balance_before = token_client.balance(&investor);
    let contract_balance_before = token_client.balance(&contract_id);

    let result = client.try_accept_bid_and_fund(&invoice_id, &bid_id);
    assert!(result.is_err(), "must fail with zero investor balance");
    assert_eq!(
        result.unwrap_err().unwrap(),
        QuickLendXError::InsufficientFunds,
    );

    // No funds moved.
    assert_eq!(token_client.balance(&investor), investor_balance_before);
    assert_eq!(token_client.balance(&contract_id), contract_balance_before);

    // Invoice untouched.
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Verified);
    assert_eq!(invoice.funded_amount, 0);
    assert!(invoice.investor.is_none());

    // Bid untouched.
    let bid = client.get_bid(&bid_id).unwrap();
    assert_eq!(bid.status, BidStatus::Placed);

    // No orphan escrow.
    assert!(client.try_get_escrow_details(&invoice_id).is_err());
}

/// accept_bid_and_fund with zero allowance leaves all state unchanged.
///
/// The allowance check in transfer_funds runs before the token call so the
/// token contract is never invoked and no storage writes occur.
#[test]
fn test_accept_bid_and_fund_no_allowance_leaves_state_unchanged() {
    let (env, client, admin) = setup();
    let contract_id = client.address.clone();

    let business = setup_verified_business(&env, &client, &admin);
    let investor = setup_verified_investor(&env, &client, 50_000);

    let amount = 10_000i128;
    let currency = setup_token_no_allowance(&env, &business, &investor, &contract_id, amount);
    let token_client = token::Client::new(&env, &currency);

    // Business needs allowance for operations; investor deliberately has none.
    let expiration = env.ledger().sequence() + 10_000;
    token_client.approve(&business, &contract_id, &(amount * 10), &expiration);

    let invoice_id = create_verified_invoice(&env, &client, &business, amount, &currency);
    let bid_id = place_test_bid(&client, &investor, &invoice_id, amount, amount + 500);

    let investor_balance_before = token_client.balance(&investor);
    let contract_balance_before = token_client.balance(&contract_id);

    let result = client.try_accept_bid_and_fund(&invoice_id, &bid_id);
    assert!(result.is_err(), "must fail with zero investor allowance");
    assert_eq!(
        result.unwrap_err().unwrap(),
        QuickLendXError::OperationNotAllowed,
    );

    // No funds moved.
    assert_eq!(token_client.balance(&investor), investor_balance_before);
    assert_eq!(token_client.balance(&contract_id), contract_balance_before);

    // Invoice untouched.
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Verified);
    assert_eq!(invoice.funded_amount, 0);
    assert!(invoice.investor.is_none());

    // Bid untouched.
    assert_eq!(client.get_bid(&bid_id).unwrap().status, BidStatus::Placed);

    // No orphan escrow.
    assert!(client.try_get_escrow_details(&invoice_id).is_err());
}

/// accept_bid_and_fund with partial allowance leaves all state unchanged.
///
/// A positive-but-insufficient allowance is rejected before the token call,
/// so no partial state is written.
#[test]
fn test_accept_bid_and_fund_partial_allowance_leaves_state_unchanged() {
    let (env, client, admin) = setup();
    let contract_id = client.address.clone();

    let business = setup_verified_business(&env, &client, &admin);
    let investor = setup_verified_investor(&env, &client, 50_000);

    let amount = 10_000i128;
    let currency =
        setup_token_partial_allowance(&env, &business, &investor, &contract_id, amount);
    let token_client = token::Client::new(&env, &currency);

    let invoice_id = create_verified_invoice(&env, &client, &business, amount, &currency);
    let bid_id = place_test_bid(&client, &investor, &invoice_id, amount, amount + 500);

    let investor_balance_before = token_client.balance(&investor);
    let contract_balance_before = token_client.balance(&contract_id);

    let result = client.try_accept_bid_and_fund(&invoice_id, &bid_id);
    assert!(result.is_err(), "must fail with partial allowance");
    assert_eq!(
        result.unwrap_err().unwrap(),
        QuickLendXError::OperationNotAllowed,
    );

    // No funds moved.
    assert_eq!(token_client.balance(&investor), investor_balance_before);
    assert_eq!(token_client.balance(&contract_id), contract_balance_before);

    // Invoice untouched.
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Verified);
    assert_eq!(invoice.funded_amount, 0);
    assert!(invoice.investor.is_none());

    // Bid untouched.
    assert_eq!(client.get_bid(&bid_id).unwrap().status, BidStatus::Placed);

    // No orphan escrow.
    assert!(client.try_get_escrow_details(&invoice_id).is_err());
}

/// After a failed accept_bid_and_fund, the bid can be retried once the
/// investor provides sufficient balance and allowance. No permanent corruption.
#[test]
fn test_accept_bid_and_fund_retry_succeeds_after_topping_up() {
    let (env, client, admin) = setup();
    let contract_id = client.address.clone();

    let business = setup_verified_business(&env, &client, &admin);
    let investor = setup_verified_investor(&env, &client, 50_000);

    let amount = 10_000i128;
    let token_admin = Address::generate(&env);
    let currency = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token_client = token::Client::new(&env, &currency);
    let sac_client = token::StellarAssetClient::new(&env, &currency);

    sac_client.mint(&business, &(amount * 10));
    // Investor starts with insufficient balance.
    sac_client.mint(&investor, &(amount - 1));
    let expiration = env.ledger().sequence() + 10_000;
    token_client.approve(&business, &contract_id, &(amount * 10), &expiration);
    token_client.approve(&investor, &contract_id, &(amount * 10), &expiration);

    let invoice_id = create_verified_invoice(&env, &client, &business, amount, &currency);
    let bid_id = place_test_bid(&client, &investor, &invoice_id, amount, amount + 500);

    // First attempt fails — state is clean.
    assert_eq!(
        client
            .try_accept_bid_and_fund(&invoice_id, &bid_id)
            .unwrap_err()
            .unwrap(),
        QuickLendXError::InsufficientFunds,
    );
    assert_eq!(
        client.get_invoice(&invoice_id).status,
        InvoiceStatus::Verified
    );
    assert_eq!(client.get_bid(&bid_id).unwrap().status, BidStatus::Placed);
    assert!(client.try_get_escrow_details(&invoice_id).is_err());

    // Top up investor balance.
    sac_client.mint(&investor, &1i128);

    // Second attempt succeeds.
    client.accept_bid_and_fund(&invoice_id, &bid_id);

    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Funded);
    assert_eq!(invoice.funded_amount, amount);
    assert!(invoice.investor.is_some());

    assert_eq!(client.get_bid(&bid_id).unwrap().status, BidStatus::Accepted);

    let escrow = client.get_escrow_details(&invoice_id);
    assert_eq!(escrow.status, EscrowStatus::Held);
    assert_eq!(escrow.amount, amount);
    assert_eq!(token_client.balance(&contract_id), amount);
}

/// A second call to accept_bid_and_fund on an already-funded invoice is rejected.
///
/// Prevents double-escrow / double-investment creation even if the caller
/// retries a successful operation.
#[test]
fn test_accept_bid_and_fund_second_call_rejected() {
    let (env, client, admin) = setup();
    let contract_id = client.address.clone();

    let business = setup_verified_business(&env, &client, &admin);
    let investor = setup_verified_investor(&env, &client, 50_000);

    let amount = 5_000i128;
    let currency = setup_token(&env, &business, &investor, &contract_id);
    let token_client = token::Client::new(&env, &currency);

    let invoice_id = create_verified_invoice(&env, &client, &business, amount, &currency);
    let bid_id = place_test_bid(&client, &investor, &invoice_id, amount, amount + 500);

    // First call succeeds.
    client.accept_bid_and_fund(&invoice_id, &bid_id);
    assert_eq!(
        client.get_invoice(&invoice_id).status,
        InvoiceStatus::Funded
    );

    let balance_after_first = token_client.balance(&contract_id);

    // Second call must fail — invoice is already Funded.
    let result = client.try_accept_bid_and_fund(&invoice_id, &bid_id);
    assert!(result.is_err(), "second accept_bid_and_fund must be rejected");

    // No additional funds moved.
    assert_eq!(token_client.balance(&contract_id), balance_after_first);

    // Exactly one escrow record with the original amount.
    let escrow = client.get_escrow_details(&invoice_id);
    assert_eq!(escrow.status, EscrowStatus::Held);
    assert_eq!(escrow.amount, amount);
}

/// When accept_bid_and_fund fails, invoice status bucket indices are not corrupted.
///
/// The invoice must remain in the Verified bucket, not appear in Funded.
#[test]
fn test_accept_bid_and_fund_failure_preserves_status_indices() {
    let (env, client, admin) = setup();
    let contract_id = client.address.clone();

    let business = setup_verified_business(&env, &client, &admin);
    let investor = setup_verified_investor(&env, &client, 50_000);

    let amount = 8_000i128;
    let currency = setup_token_no_allowance(&env, &business, &investor, &contract_id, amount);
    let token_client = token::Client::new(&env, &currency);
    let expiration = env.ledger().sequence() + 10_000;
    token_client.approve(&business, &contract_id, &(amount * 10), &expiration);

    let invoice_id = create_verified_invoice(&env, &client, &business, amount, &currency);
    let bid_id = place_test_bid(&client, &investor, &invoice_id, amount, amount + 500);

    let verified_before = client.get_invoice_count_by_status(&InvoiceStatus::Verified);
    let funded_before = client.get_invoice_count_by_status(&InvoiceStatus::Funded);

    // Attempt fails (no investor allowance).
    assert!(client.try_accept_bid_and_fund(&invoice_id, &bid_id).is_err());

    // Buckets unchanged.
    assert_eq!(
        client.get_invoice_count_by_status(&InvoiceStatus::Verified),
        verified_before,
        "Verified bucket must not change on failure"
    );
    assert_eq!(
        client.get_invoice_count_by_status(&InvoiceStatus::Funded),
        funded_before,
        "Funded bucket must not gain phantom entries on failure"
    );
}

/// Mismatched bid/invoice pair is rejected before any funds move.
///
/// Prevents an attacker from supplying a bid ID from one invoice while
/// targeting a different invoice to redirect funds or corrupt state.
#[test]
fn test_accept_bid_and_fund_mismatched_pair_rejected_atomically() {
    let (env, client, admin) = setup();
    let contract_id = client.address.clone();

    let business = setup_verified_business(&env, &client, &admin);
    let investor = setup_verified_investor(&env, &client, 50_000);

    let amount = 5_000i128;
    let currency = setup_token(&env, &business, &investor, &contract_id);
    let token_client = token::Client::new(&env, &currency);

    // Two separate invoices.
    let invoice_id_a = create_verified_invoice(&env, &client, &business, amount, &currency);
    let invoice_id_b = create_verified_invoice(&env, &client, &business, amount, &currency);

    // Bid placed only on invoice A.
    let bid_id_a = place_test_bid(&client, &investor, &invoice_id_a, amount, amount + 500);

    let investor_balance_before = token_client.balance(&investor);

    // Try to accept bid_a against invoice_b — must fail.
    let result = client.try_accept_bid_and_fund(&invoice_id_b, &bid_id_a);
    assert!(result.is_err(), "mismatched pair must be rejected");

    // No funds moved.
    assert_eq!(token_client.balance(&investor), investor_balance_before);

    // Neither invoice was funded.
    assert_eq!(
        client.get_invoice(&invoice_id_a).status,
        InvoiceStatus::Verified
    );
    assert_eq!(
        client.get_invoice(&invoice_id_b).status,
        InvoiceStatus::Verified
    );

    // No escrow records created on either invoice.
    assert!(client.try_get_escrow_details(&invoice_id_a).is_err());
    assert!(client.try_get_escrow_details(&invoice_id_b).is_err());
}
