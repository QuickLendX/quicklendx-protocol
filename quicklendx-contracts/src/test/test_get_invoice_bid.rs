/// Comprehensive tests for get_invoice and get_bid single entity retrieval
///
/// Test Coverage Goals:
/// - get_invoice: Ok with correct data, Err InvoiceNotFound for nonexistent IDs
/// - get_bid: Some with correct data, None for nonexistent IDs
/// - Minimum 95% coverage for single entity getters
/// - Edge cases and error conditions
///
/// This test module covers:
/// - Basic retrieval of invoices and bids
/// - Error handling for missing entities
/// - Data integrity validation
/// - Multiple entity scenarios
use super::*;
use crate::bid::{Bid, BidStatus};
use crate::invoice::{Invoice, InvoiceCategory, InvoiceStatus};
use soroban_sdk::{testutils::Address as _, token, vec, Address, BytesN, Env, String, Vec};

// ============================================================================
// HELPER FUNCTIONS
// ============================================================================

/// Helper to set up contract and admin
fn setup_contract() -> (Env, QuickLendXContractClient<'static>) {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    env.mock_all_auths();
    (env, client)
}

/// Helper to create a verified business
fn create_verified_business(env: &Env, client: &QuickLendXContractClient) -> Address {
    let admin = Address::generate(env);
    let business = Address::generate(env);
    let kyc_data = String::from_str(env, "Business KYC data");

    client.set_admin(&admin);
    client.submit_kyc_application(&business, &kyc_data);
    client.verify_business(&admin, &business);

    business
}

/// Helper to create a verified investor
fn create_verified_investor(env: &Env, client: &QuickLendXContractClient, limit: i128) -> Address {
    let investor = Address::generate(env);
    let kyc_data = String::from_str(env, "Investor KYC data");

    client.submit_investor_kyc(&investor, &kyc_data);
    client.verify_investor(&investor, &limit);

    investor
}

/// Helper to create an invoice and verify it
fn create_and_verify_invoice(
    env: &Env,
    client: &QuickLendXContractClient,
    business: &Address,
    amount: i128,
    category: InvoiceCategory,
) -> BytesN<32> {
    let currency = Address::generate(env);
    let due_date = env.ledger().timestamp() + 86400; // 1 day from now
    let description = String::from_str(env, "Test invoice");

    let invoice_id = client.store_invoice(
        business,
        &amount,
        &currency,
        &due_date,
        &description,
        &category,
        &Vec::new(env),
    );

    // Verify the invoice
    let _ = client.try_verify_invoice(&invoice_id);

    invoice_id
}

/// Helper to place a bid on an invoice
fn place_bid(
    env: &Env,
    client: &QuickLendXContractClient,
    investor: &Address,
    invoice_id: &BytesN<32>,
    bid_amount: i128,
    expected_return: i128,
) -> BytesN<32> {
    client.place_bid(investor, invoice_id, &bid_amount, &expected_return)
}

// ============================================================================
// GET_INVOICE TESTS
// ============================================================================

/// Test 1: Get invoice with valid data - Ok case
#[test]
fn test_get_invoice_ok_with_correct_data() {
    let (env, client) = setup_contract();
    let business = create_verified_business(&env, &client);

    // Create invoice
    let currency = Address::generate(&env);
    let amount = 5000i128;
    let due_date = env.ledger().timestamp() + 86400;
    let description = String::from_str(&env, "Test invoice for retrieval");

    let invoice_id = client.store_invoice(
        &business,
        &amount,
        &currency,
        &due_date,
        &description,
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    // Test get_invoice - should return Ok with correct data
    let result = client.try_get_invoice(&invoice_id);
    assert!(result.is_ok(), "get_invoice should succeed for valid invoice ID");

    let invoice = result.unwrap();
    assert_eq!(invoice.id, invoice_id, "Invoice ID should match");
    assert_eq!(invoice.business, business, "Business should match");
    assert_eq!(invoice.amount, amount, "Amount should match");
    assert_eq!(invoice.currency, currency, "Currency should match");
    assert_eq!(invoice.due_date, due_date, "Due date should match");
    assert_eq!(
        invoice.description, description,
        "Description should match"
    );
    assert_eq!(
        invoice.status,
        InvoiceStatus::Pending,
        "Initial status should be Pending"
    );
    assert_eq!(
        invoice.funded_amount, 0,
        "Initial funded amount should be 0"
    );
    assert!(
        invoice.investor.is_none(),
        "Initial investor should be None"
    );
}

/// Test 2: Get invoice with all categories - validates data integrity
#[test]
fn test_get_invoice_ok_all_categories() {
    let (env, client) = setup_contract();
    let business = create_verified_business(&env, &client);

    let categories = vec![
        InvoiceCategory::Services,
        InvoiceCategory::Products,
        InvoiceCategory::Consulting,
        InvoiceCategory::Manufacturing,
        InvoiceCategory::Technology,
        InvoiceCategory::Healthcare,
        InvoiceCategory::Other,
    ];

    for category in categories.iter() {
        let invoice_id = create_and_verify_invoice(&env, &client, &business, 1000, *category);

        let result = client.try_get_invoice(&invoice_id);
        assert!(result.is_ok(), "get_invoice should succeed");

        let invoice = result.unwrap();
        assert_eq!(
            invoice.category, *category,
            "Category should match stored value"
        );
    }
}

/// Test 3: Get invoice after status change - validates data consistency
#[test]
fn test_get_invoice_ok_after_status_transitions() {
    let (env, client) = setup_contract();
    let business = create_verified_business(&env, &client);
    let investor = create_verified_investor(&env, &client, 100_000);

    let invoice_id = create_and_verify_invoice(&env, &client, &business, 5000, InvoiceCategory::Services);

    // Check after verification
    let invoice = client.try_get_invoice(&invoice_id).unwrap();
    assert_eq!(
        invoice.status,
        InvoiceStatus::Verified,
        "Status should be Verified after verify_invoice"
    );

    // Fund the invoice
    client.fund_invoice(&investor, &invoice_id, &5000);

    // Check after funding
    let invoice = client.try_get_invoice(&invoice_id).unwrap();
    assert_eq!(
        invoice.status,
        InvoiceStatus::Funded,
        "Status should be Funded after funding"
    );
    assert_eq!(
        invoice.funded_amount, 5000,
        "Funded amount should be updated"
    );
    assert_eq!(
        invoice.investor,
        Some(investor),
        "Investor should be stored"
    );
}

/// Test 4: Get invoice with nonexistent ID - Err InvoiceNotFound case
#[test]
fn test_get_invoice_err_nonexistent_invoice() {
    let (env, client) = setup_contract();

    // Generate a random BytesN<32> that doesn't exist
    let nonexistent_id = BytesN::from_array(&env, &[1u8; 32]);

    let result = client.try_get_invoice(&nonexistent_id);
    assert!(
        result.is_err(),
        "get_invoice should fail for nonexistent invoice"
    );

    let err = result.unwrap_err().unwrap();
    assert_eq!(
        err,
        QuickLendXError::InvoiceNotFound,
        "Error should be InvoiceNotFound"
    );
}

/// Test 5: Get invoice with multiple random IDs - Err case
#[test]
fn test_get_invoice_err_multiple_random_bytesn32() {
    let (env, client) = setup_contract();

    // Test with multiple different random IDs
    for i in 0..5u8 {
        let mut random_bytes = [0u8; 32];
        random_bytes[0] = i;
        let random_id = BytesN::from_array(&env, &random_bytes);

        let result = client.try_get_invoice(&random_id);
        assert!(
            result.is_err(),
            "get_invoice should fail for random ID {}",
            i
        );
        let err = result.unwrap_err().unwrap();
        assert_eq!(err, QuickLendXError::InvoiceNotFound);
    }
}

/// Test 6: Create multiple invoices and retrieve each one
#[test]
fn test_get_invoice_ok_multiple_invoices() {
    let (env, client) = setup_contract();
    let business = create_verified_business(&env, &client);
    let mut invoice_ids = Vec::new(&env);

    // Create 5 invoices
    for i in 0..5 {
        let amount = 1000 + (i as i128) * 100;
        let invoice_id = create_and_verify_invoice(&env, &client, &business, amount, InvoiceCategory::Services);
        invoice_ids.push_back(invoice_id.clone());

        // Verify each can be retrieved
        let invoice = client.try_get_invoice(&invoice_id).unwrap();
        assert_eq!(invoice.amount, amount, "Amount should match for invoice {}", i);
    }

    // Verify we can retrieve all of them
    for (i, invoice_id) in invoice_ids.iter().enumerate() {
        let result = client.try_get_invoice(&invoice_id);
        assert!(result.is_ok(), "Should be able to retrieve invoice {}", i);
    }
}

/// Test 7: Get invoice with tags validation
#[test]
fn test_get_invoice_ok_with_tags() {
    let (env, client) = setup_contract();
    let business = create_verified_business(&env, &client);

    let currency = Address::generate(&env);
    let amount = 3000i128;
    let due_date = env.ledger().timestamp() + 86400;
    let description = String::from_str(&env, "Invoice with tags");

    let mut tags = Vec::new(&env);
    tags.push_back(String::from_str(&env, "urgent"));
    tags.push_back(String::from_str(&env, "services"));

    let invoice_id = client.store_invoice(
        &business,
        &amount,
        &currency,
        &due_date,
        &description,
        &InvoiceCategory::Services,
        &tags,
    );

    // Retrieve and validate tags
    let invoice = client.try_get_invoice(&invoice_id).unwrap();
    assert_eq!(invoice.tags.len(), 2, "Should have 2 tags");
    assert_eq!(
        invoice.tags.get(0).unwrap(),
        String::from_str(&env, "urgent")
    );
    assert_eq!(
        invoice.tags.get(1).unwrap(),
        String::from_str(&env, "services")
    );
}

// ============================================================================
// GET_BID TESTS
// ============================================================================

/// Test 8: Get bid with valid data - Some case
#[test]
fn test_get_bid_some_with_correct_data() {
    let (env, client) = setup_contract();
    let business = create_verified_business(&env, &client);
    let investor = create_verified_investor(&env, &client, 100_000);

    let invoice_id = create_and_verify_invoice(&env, &client, &business, 5000, InvoiceCategory::Services);

    // Place a bid
    let bid_amount = 4500i128;
    let expected_return = 5000i128;
    let bid_id = place_bid(&env, &client, &investor, &invoice_id, bid_amount, expected_return);

    // Test get_bid - should return Some with correct data
    let result = client.try_get_bid(&bid_id);
    assert!(result.is_ok(), "get_bid should succeed for valid bid ID");

    let bid_option = result.unwrap();
    assert!(
        bid_option.is_some(),
        "get_bid should return Some for valid bid"
    );

    let bid = bid_option.unwrap();
    assert_eq!(bid.bid_id, bid_id, "Bid ID should match");
    assert_eq!(bid.invoice_id, invoice_id, "Invoice ID should match");
    assert_eq!(bid.investor, investor, "Investor should match");
    assert_eq!(bid.bid_amount, bid_amount, "Bid amount should match");
    assert_eq!(
        bid.expected_return, expected_return,
        "Expected return should match"
    );
    assert_eq!(bid.status, BidStatus::Placed, "Status should be Placed");
}

/// Test 9: Get bid with multiple bids on one invoice
#[test]
fn test_get_bid_some_multiple_bids_same_invoice() {
    let (env, client) = setup_contract();
    let business = create_verified_business(&env, &client);
    let invoice_id = create_and_verify_invoice(&env, &client, &business, 10_000, InvoiceCategory::Services);

    let mut bid_ids = Vec::new(&env);

    // Place 3 different bids
    for i in 0..3 {
        let investor = create_verified_investor(&env, &client, 100_000);
        let bid_amount = 9000i128 + (i as i128) * 100;
        let expected_return = 10_000i128 + (i as i128) * 100;

        let bid_id = place_bid(&env, &client, &investor, &invoice_id, bid_amount, expected_return);
        bid_ids.push_back(bid_id);
    }

    // Verify each bid can be retrieved
    for (i, bid_id) in bid_ids.iter().enumerate() {
        let result = client.try_get_bid(&bid_id);
        assert!(result.is_ok(), "get_bid should succeed for bid {}", i);

        let bid_option = result.unwrap();
        assert!(
            bid_option.is_some(),
            "Should return Some for bid {}",
            i
        );

        let bid = bid_option.unwrap();
        assert_eq!(bid.bid_id, *bid_id);
        assert_eq!(bid.invoice_id, invoice_id);
    }
}

/// Test 10: Get bid after status changes
#[test]
fn test_get_bid_some_after_status_changes() {
    let (env, client) = setup_contract();
    let business = create_verified_business(&env, &client);
    let investor = create_verified_investor(&env, &client, 100_000);

    let invoice_id = create_and_verify_invoice(&env, &client, &business, 5000, InvoiceCategory::Services);
    let bid_id = place_bid(&env, &client, &investor, &invoice_id, 4500, 5000);

    // Check initial status
    let bid = client.try_get_bid(&bid_id).unwrap().unwrap();
    assert_eq!(bid.status, BidStatus::Placed);

    // Withdraw the bid
    client.withdraw_bid(&investor, &bid_id);

    // Check status after withdrawal
    let bid = client.try_get_bid(&bid_id).unwrap().unwrap();
    assert_eq!(bid.status, BidStatus::Withdrawn);
}

/// Test 11: Get bid with nonexistent ID - None case
#[test]
fn test_get_bid_none_nonexistent_bid() {
    let (env, client) = setup_contract();

    // Generate a random BytesN<32> that doesn't exist
    let nonexistent_id = BytesN::from_array(&env, &[2u8; 32]);

    let result = client.try_get_bid(&nonexistent_id);
    assert!(
        result.is_ok(),
        "get_bid should not error for nonexistent ID (returns None)"
    );

    let bid_option = result.unwrap();
    assert!(
        bid_option.is_none(),
        "get_bid should return None for nonexistent bid"
    );
}

/// Test 12: Get bid with multiple random IDs - None case
#[test]
fn test_get_bid_none_multiple_random_bytesn32() {
    let (env, client) = setup_contract();

    // Test with multiple different random IDs
    for i in 0..5u8 {
        let mut random_bytes = [0u8; 32];
        random_bytes[0] = i;
        let random_id = BytesN::from_array(&env, &random_bytes);

        let result = client.try_get_bid(&random_id);
        assert!(
            result.is_ok(),
            "get_bid should not error for random ID {}",
            i
        );
        let bid_option = result.unwrap();
        assert!(
            bid_option.is_none(),
            "get_bid should return None for random ID {}",
            i
        );
    }
}

/// Test 13: Get bid immediately after placement
#[test]
fn test_get_bid_some_immediately_after_placement() {
    let (env, client) = setup_contract();
    let business = create_verified_business(&env, &client);
    let investor = create_verified_investor(&env, &client, 100_000);

    let invoice_id = create_and_verify_invoice(&env, &client, &business, 8000, InvoiceCategory::Services);

    let bid_amount = 7500i128;
    let expected_return = 8000i128;
    let bid_id = place_bid(&env, &client, &investor, &invoice_id, bid_amount, expected_return);

    // Immediately retrieve and validate all fields
    let bid = client.try_get_bid(&bid_id).unwrap().unwrap();

    assert_eq!(bid.bid_id, bid_id);
    assert_eq!(bid.invoice_id, invoice_id);
    assert_eq!(bid.investor, investor);
    assert_eq!(bid.bid_amount, bid_amount);
    assert_eq!(bid.expected_return, expected_return);
    assert_eq!(bid.status, BidStatus::Placed);
    assert!(bid.timestamp > 0, "Timestamp should be set");
    assert!(bid.expiration_timestamp > bid.timestamp, "Expiration should be in future");
}

/// Test 14: Get bid with different investors - validates correct isolation
#[test]
fn test_get_bid_some_different_investors() {
    let (env, client) = setup_contract();
    let business = create_verified_business(&env, &client);
    let invoice_id = create_and_verify_invoice(&env, &client, &business, 10_000, InvoiceCategory::Services);

    let mut bid_ids = Vec::new(&env);
    let mut investors = Vec::new(&env);

    // Create bids from 3 different investors
    for _i in 0..3 {
        let investor = create_verified_investor(&env, &client, 50_000);
        investors.push_back(investor.clone());

        let bid_id = place_bid(&env, &client, &investor, &invoice_id, 9000, 10_000);
        bid_ids.push_back(bid_id);
    }

    // Verify each bid returns correct investor
    for (i, bid_id) in bid_ids.iter().enumerate() {
        let bid = client.try_get_bid(&bid_id).unwrap().unwrap();
        let expected_investor = investors.get(i).unwrap();
        assert_eq!(
            bid.investor, expected_investor,
            "Bid {} should have correct investor",
            i
        );
    }
}

// ============================================================================
// INTEGRATION TESTS - Get Invoice and Get Bid Together
// ============================================================================

/// Test 15: Get invoice and all its bids - integration test
#[test]
fn test_get_invoice_and_all_related_bids() {
    let (env, client) = setup_contract();
    let business = create_verified_business(&env, &client);
    let invoice_id = create_and_verify_invoice(&env, &client, &business, 10_000, InvoiceCategory::Services);

    // Get invoice
    let invoice = client.try_get_invoice(&invoice_id).unwrap();
    assert_eq!(invoice.id, invoice_id);
    assert_eq!(invoice.amount, 10_000);

    // Place multiple bids
    let mut bid_ids = Vec::new(&env);
    for i in 0..3 {
        let investor = create_verified_investor(&env, &client, 50_000);
        let bid_id = place_bid(&env, &client, &investor, &invoice_id, 9000 + (i as i128) * 10, 10_000);
        bid_ids.push_back(bid_id);
    }

    // Verify all bids are retrievable and relate to the invoice
    for bid_id in bid_ids.iter() {
        let bid = client.try_get_bid(&bid_id).unwrap().unwrap();
        assert_eq!(bid.invoice_id, invoice_id, "Bid should reference correct invoice");

        // Verify the invoice is still retrievable
        let invoice_check = client.try_get_invoice(&invoice_id).unwrap();
        assert_eq!(invoice_check.id, invoice_id);
    }
}

/// Test 16: Get invoice with expired bids - data consistency
#[test]
fn test_get_bid_none_after_expiration() {
    let (env, client) = setup_contract();
    let business = create_verified_business(&env, &client);
    let investor = create_verified_investor(&env, &client, 100_000);

    let invoice_id = create_and_verify_invoice(&env, &client, &business, 5000, InvoiceCategory::Services);
    let bid_id = place_bid(&env, &client, &investor, &invoice_id, 4500, 5000);

    // Verify bid exists
    let bid = client.try_get_bid(&bid_id).unwrap().unwrap();
    assert!(
        !bid.is_expired(env.ledger().timestamp()),
        "Bid should not be expired initially"
    );

    // Advance time significantly to expire the bid
    env.ledger()
        .set(soroban_sdk::testutils::Ledger::default().with_timestamp(bid.expiration_timestamp + 1000));

    // Bid should still be retrievable (expired status is computed, not stored differently)
    let bid_after = client.try_get_bid(&bid_id).unwrap().unwrap();
    assert_eq!(bid_after.bid_id, bid_id);
    assert!(
        bid_after.is_expired(env.ledger().timestamp()),
        "Bid should now be expired"
    );
}
