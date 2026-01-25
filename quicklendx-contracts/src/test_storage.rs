//! Storage Schema and Core Types Test Suite
//!
//! This module provides comprehensive tests for:
//! - Storage layer operations (put/get/update/remove)
//! - Index management (business, status, metadata, investor indexes)
//! - Core type serialization and state transitions
//! - ID generation and uniqueness
//! - Edge cases (missing keys, duplicates, invalid transitions)
//!
//! # Security Notes
//! - All storage operations use Soroban's instance storage for deterministic behavior
//! - ID generation uses timestamp + sequence + counter for collision resistance
//! - Index operations prevent duplicates through explicit checks
//!
//! # Test Coverage Target: 95%+

use super::*;
use crate::bid::{Bid, BidStatus, BidStorage};
use crate::errors::QuickLendXError;
use crate::investment::{Investment, InvestmentStatus, InvestmentStorage};
use crate::invoice::{
    DisputeStatus, Invoice, InvoiceCategory, InvoiceMetadata, InvoiceRating, InvoiceStatus,
    InvoiceStorage, LineItemRecord,
};
use crate::payments::{Escrow, EscrowStatus, EscrowStorage};
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    Address, BytesN, Env, String, Vec,
};

// ============================================================================
// HELPER FUNCTIONS
// ============================================================================

/// Create a test environment with contract registered
fn setup() -> (Env, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    (env, contract_id)
}

/// Generate a test invoice with minimal required fields (must be called inside as_contract)
fn create_test_invoice(env: &Env, business: &Address) -> Invoice {
    Invoice::new(
        env,
        business.clone(),
        10_000,
        Address::generate(env),
        env.ledger().timestamp() + 86400,
        String::from_str(env, "Test Invoice"),
        InvoiceCategory::Services,
        Vec::new(env),
    )
}

/// Create test metadata for invoice
fn create_test_metadata(env: &Env) -> InvoiceMetadata {
    let mut line_items = Vec::new(env);
    line_items.push_back(LineItemRecord(
        String::from_str(env, "Service A"),
        10,
        1000,
        10_000,
    ));
    InvoiceMetadata {
        customer_name: String::from_str(env, "Test Customer"),
        customer_address: String::from_str(env, "123 Test St"),
        tax_id: String::from_str(env, "TAX-12345"),
        line_items,
        notes: String::from_str(env, "Test notes"),
    }
}

/// Create a test bid (must be called inside as_contract)
fn create_test_bid(env: &Env, invoice_id: &BytesN<32>, investor: &Address) -> Bid {
    let bid_id = BidStorage::generate_unique_bid_id(env);
    let now = env.ledger().timestamp();
    Bid {
        bid_id,
        invoice_id: invoice_id.clone(),
        investor: investor.clone(),
        bid_amount: 5_000,
        expected_return: 5_500,
        timestamp: now,
        status: BidStatus::Placed,
        expiration_timestamp: Bid::default_expiration(now),
    }
}

/// Create a test investment (must be called inside as_contract)
fn create_test_investment(env: &Env, invoice_id: &BytesN<32>, investor: &Address) -> Investment {
    let investment_id = InvestmentStorage::generate_unique_investment_id(env);
    Investment {
        investment_id,
        invoice_id: invoice_id.clone(),
        investor: investor.clone(),
        amount: 10_000,
        funded_at: env.ledger().timestamp(),
        status: InvestmentStatus::Active,
        insurance: Vec::new(env),
    }
}

/// Create a test escrow (must be called inside as_contract)
fn create_test_escrow(
    env: &Env,
    invoice_id: &BytesN<32>,
    investor: &Address,
    business: &Address,
) -> Escrow {
    let escrow_id = EscrowStorage::generate_unique_escrow_id(env);
    Escrow {
        escrow_id,
        invoice_id: invoice_id.clone(),
        investor: investor.clone(),
        business: business.clone(),
        amount: 10_000,
        currency: Address::generate(env),
        created_at: env.ledger().timestamp(),
        status: EscrowStatus::Held,
    }
}

// ============================================================================
// INVOICE STORAGE TESTS
// ============================================================================

/// Test: Basic invoice storage put/get operations
#[test]
fn test_invoice_storage_put_get() {
    let (env, contract_id) = setup();
    let business = Address::generate(&env);

    env.as_contract(&contract_id, || {
        let invoice = create_test_invoice(&env, &business);
        let invoice_id = invoice.id.clone();

        // Store invoice
        InvoiceStorage::store_invoice(&env, &invoice);

        // Retrieve invoice
        let retrieved = InvoiceStorage::get_invoice(&env, &invoice_id);
        assert!(retrieved.is_some(), "Invoice should be retrievable after storage");

        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.id, invoice_id, "Invoice ID should match");
        assert_eq!(retrieved.business, business, "Business address should match");
        assert_eq!(retrieved.amount, 10_000, "Amount should match");
        assert_eq!(retrieved.status, InvoiceStatus::Pending, "Initial status should be Pending");
    });
}

/// Test: Invoice storage returns None for non-existent key
#[test]
fn test_invoice_storage_missing_key() {
    let (env, contract_id) = setup();

    env.as_contract(&contract_id, || {
        let fake_id = BytesN::from_array(&env, &[0u8; 32]);
        let result = InvoiceStorage::get_invoice(&env, &fake_id);
        assert!(result.is_none(), "Non-existent invoice should return None");
    });
}

/// Test: Invoice update operation
#[test]
fn test_invoice_storage_update() {
    let (env, contract_id) = setup();
    let business = Address::generate(&env);

    env.as_contract(&contract_id, || {
        let mut invoice = create_test_invoice(&env, &business);
        let invoice_id = invoice.id.clone();

        // Store initial invoice
        InvoiceStorage::store_invoice(&env, &invoice);

        // Update invoice status
        invoice.status = InvoiceStatus::Verified;
        InvoiceStorage::update_invoice(&env, &invoice);

        // Verify update
        let retrieved = InvoiceStorage::get_invoice(&env, &invoice_id).unwrap();
        assert_eq!(
            retrieved.status,
            InvoiceStatus::Verified,
            "Status should be updated to Verified"
        );
    });
}

/// Test: Business invoice index - adding and retrieving
#[test]
fn test_invoice_business_index() {
    let (env, contract_id) = setup();
    let business1 = Address::generate(&env);
    let business2 = Address::generate(&env);

    env.as_contract(&contract_id, || {
        // Create and store invoices for business1
        let invoice1 = create_test_invoice(&env, &business1);
        let invoice2 = create_test_invoice(&env, &business1);
        let id1 = invoice1.id.clone();
        let id2 = invoice2.id.clone();

        InvoiceStorage::store_invoice(&env, &invoice1);
        InvoiceStorage::store_invoice(&env, &invoice2);

        // Create and store invoice for business2
        let invoice3 = create_test_invoice(&env, &business2);
        let id3 = invoice3.id.clone();
        InvoiceStorage::store_invoice(&env, &invoice3);

        // Verify business1 has 2 invoices
        let business1_invoices = InvoiceStorage::get_business_invoices(&env, &business1);
        assert_eq!(business1_invoices.len(), 2, "Business1 should have 2 invoices");
        assert!(business1_invoices.contains(&id1), "Business1 should contain invoice1");
        assert!(business1_invoices.contains(&id2), "Business1 should contain invoice2");

        // Verify business2 has 1 invoice
        let business2_invoices = InvoiceStorage::get_business_invoices(&env, &business2);
        assert_eq!(business2_invoices.len(), 1, "Business2 should have 1 invoice");
        assert!(business2_invoices.contains(&id3), "Business2 should contain invoice3");

        // Verify unknown business returns empty
        let unknown_business = Address::generate(&env);
        let unknown_invoices = InvoiceStorage::get_business_invoices(&env, &unknown_business);
        assert_eq!(
            unknown_invoices.len(),
            0,
            "Unknown business should have 0 invoices"
        );
    });
}

/// Test: Status index - adding invoices to status lists
#[test]
fn test_invoice_status_index_add() {
    let (env, contract_id) = setup();
    let business = Address::generate(&env);

    env.as_contract(&contract_id, || {
        // Create and store multiple invoices
        let invoice1 = create_test_invoice(&env, &business);
        let invoice2 = create_test_invoice(&env, &business);
        let id1 = invoice1.id.clone();
        let id2 = invoice2.id.clone();

        InvoiceStorage::store_invoice(&env, &invoice1);
        InvoiceStorage::store_invoice(&env, &invoice2);

        // Verify pending status index
        let pending = InvoiceStorage::get_invoices_by_status(&env, &InvoiceStatus::Pending);
        assert_eq!(pending.len(), 2, "Should have 2 pending invoices");
        assert!(pending.contains(&id1), "Pending should contain invoice1");
        assert!(pending.contains(&id2), "Pending should contain invoice2");

        // Verify other statuses are empty
        let verified = InvoiceStorage::get_invoices_by_status(&env, &InvoiceStatus::Verified);
        assert_eq!(verified.len(), 0, "Verified list should be empty initially");

        let funded = InvoiceStorage::get_invoices_by_status(&env, &InvoiceStatus::Funded);
        assert_eq!(funded.len(), 0, "Funded list should be empty initially");
    });
}

/// Test: Status index - removing invoices from status lists
#[test]
fn test_invoice_status_index_remove() {
    let (env, contract_id) = setup();
    let business = Address::generate(&env);

    env.as_contract(&contract_id, || {
        let invoice = create_test_invoice(&env, &business);
        let invoice_id = invoice.id.clone();
        InvoiceStorage::store_invoice(&env, &invoice);

        // Verify invoice is in pending
        let pending_before = InvoiceStorage::get_invoices_by_status(&env, &InvoiceStatus::Pending);
        assert!(
            pending_before.contains(&invoice_id),
            "Invoice should be in pending list"
        );

        // Remove from pending status
        InvoiceStorage::remove_from_status_invoices(&env, &InvoiceStatus::Pending, &invoice_id);

        // Verify removal
        let pending_after = InvoiceStorage::get_invoices_by_status(&env, &InvoiceStatus::Pending);
        assert!(
            !pending_after.contains(&invoice_id),
            "Invoice should be removed from pending list"
        );
    });
}

/// Test: Status transition storage - full lifecycle
#[test]
fn test_invoice_status_transition_storage() {
    let (env, contract_id) = setup();
    let business = Address::generate(&env);
    let investor = Address::generate(&env);

    env.as_contract(&contract_id, || {
        let mut invoice = create_test_invoice(&env, &business);
        let invoice_id = invoice.id.clone();
        InvoiceStorage::store_invoice(&env, &invoice);

        // Transition: Pending -> Verified
        InvoiceStorage::remove_from_status_invoices(&env, &InvoiceStatus::Pending, &invoice_id);
        invoice.status = InvoiceStatus::Verified;
        InvoiceStorage::update_invoice(&env, &invoice);
        InvoiceStorage::add_to_status_invoices(&env, &InvoiceStatus::Verified, &invoice_id);

        let pending = InvoiceStorage::get_invoices_by_status(&env, &InvoiceStatus::Pending);
        let verified = InvoiceStorage::get_invoices_by_status(&env, &InvoiceStatus::Verified);
        assert!(!pending.contains(&invoice_id), "Invoice should not be in pending");
        assert!(verified.contains(&invoice_id), "Invoice should be in verified");

        // Transition: Verified -> Funded
        InvoiceStorage::remove_from_status_invoices(&env, &InvoiceStatus::Verified, &invoice_id);
        invoice.mark_as_funded(&env, investor.clone(), 10_000, env.ledger().timestamp());
        InvoiceStorage::update_invoice(&env, &invoice);
        InvoiceStorage::add_to_status_invoices(&env, &InvoiceStatus::Funded, &invoice_id);

        let verified = InvoiceStorage::get_invoices_by_status(&env, &InvoiceStatus::Verified);
        let funded = InvoiceStorage::get_invoices_by_status(&env, &InvoiceStatus::Funded);
        assert!(!verified.contains(&invoice_id), "Invoice should not be in verified");
        assert!(funded.contains(&invoice_id), "Invoice should be in funded");

        // Transition: Funded -> Paid
        InvoiceStorage::remove_from_status_invoices(&env, &InvoiceStatus::Funded, &invoice_id);
        invoice.mark_as_paid(&env, business.clone(), env.ledger().timestamp());
        InvoiceStorage::update_invoice(&env, &invoice);
        InvoiceStorage::add_to_status_invoices(&env, &InvoiceStatus::Paid, &invoice_id);

        let funded = InvoiceStorage::get_invoices_by_status(&env, &InvoiceStatus::Funded);
        let paid = InvoiceStorage::get_invoices_by_status(&env, &InvoiceStatus::Paid);
        assert!(!funded.contains(&invoice_id), "Invoice should not be in funded");
        assert!(paid.contains(&invoice_id), "Invoice should be in paid");
    });
}

/// Test: Metadata indexes - customer name and tax ID
#[test]
fn test_invoice_metadata_indexes() {
    let (env, contract_id) = setup();
    let business = Address::generate(&env);

    env.as_contract(&contract_id, || {
        let mut invoice = create_test_invoice(&env, &business);
        let invoice_id = invoice.id.clone();
        InvoiceStorage::store_invoice(&env, &invoice);

        // Set metadata
        let metadata = create_test_metadata(&env);
        invoice.set_metadata(&env, Some(metadata.clone()));
        InvoiceStorage::update_invoice(&env, &invoice);
        InvoiceStorage::add_metadata_indexes(&env, &invoice);

        // Query by customer name
        let by_customer = InvoiceStorage::get_invoices_by_customer(&env, &metadata.customer_name);
        assert!(
            by_customer.contains(&invoice_id),
            "Invoice should be findable by customer name"
        );

        // Query by tax ID
        let by_tax = InvoiceStorage::get_invoices_by_tax_id(&env, &metadata.tax_id);
        assert!(
            by_tax.contains(&invoice_id),
            "Invoice should be findable by tax ID"
        );
    });
}

/// Test: Metadata index removal
#[test]
fn test_invoice_metadata_index_removal() {
    let (env, contract_id) = setup();
    let business = Address::generate(&env);

    env.as_contract(&contract_id, || {
        let mut invoice = create_test_invoice(&env, &business);
        let invoice_id = invoice.id.clone();
        InvoiceStorage::store_invoice(&env, &invoice);

        // Add metadata and indexes
        let metadata = create_test_metadata(&env);
        invoice.set_metadata(&env, Some(metadata.clone()));
        InvoiceStorage::update_invoice(&env, &invoice);
        InvoiceStorage::add_metadata_indexes(&env, &invoice);

        // Verify indexes exist
        let by_customer_before =
            InvoiceStorage::get_invoices_by_customer(&env, &metadata.customer_name);
        assert!(by_customer_before.contains(&invoice_id));

        // Remove metadata indexes
        InvoiceStorage::remove_metadata_indexes(&env, &metadata, &invoice_id);

        // Verify indexes are removed
        let by_customer_after =
            InvoiceStorage::get_invoices_by_customer(&env, &metadata.customer_name);
        assert!(
            !by_customer_after.contains(&invoice_id),
            "Invoice should be removed from customer index"
        );

        let by_tax_after = InvoiceStorage::get_invoices_by_tax_id(&env, &metadata.tax_id);
        assert!(
            !by_tax_after.contains(&invoice_id),
            "Invoice should be removed from tax ID index"
        );
    });
}

/// Test: Category-based invoice queries
#[test]
fn test_invoice_category_queries() {
    let (env, contract_id) = setup();
    let business = Address::generate(&env);

    env.as_contract(&contract_id, || {
        // Create invoices with different categories
        let mut invoice1 = create_test_invoice(&env, &business);
        invoice1.category = InvoiceCategory::Services;
        InvoiceStorage::store_invoice(&env, &invoice1);

        let mut invoice2 = create_test_invoice(&env, &business);
        invoice2.category = InvoiceCategory::Technology;
        InvoiceStorage::store_invoice(&env, &invoice2);

        let mut invoice3 = create_test_invoice(&env, &business);
        invoice3.category = InvoiceCategory::Services;
        InvoiceStorage::store_invoice(&env, &invoice3);

        // Query by category
        let services = InvoiceStorage::get_invoices_by_category(&env, &InvoiceCategory::Services);
        assert_eq!(services.len(), 2, "Should have 2 Services invoices");

        let technology =
            InvoiceStorage::get_invoices_by_category(&env, &InvoiceCategory::Technology);
        assert_eq!(technology.len(), 1, "Should have 1 Technology invoice");

        let healthcare =
            InvoiceStorage::get_invoices_by_category(&env, &InvoiceCategory::Healthcare);
        assert_eq!(healthcare.len(), 0, "Should have 0 Healthcare invoices");
    });
}

/// Test: Tag-based invoice queries
#[test]
fn test_invoice_tag_queries() {
    let (env, contract_id) = setup();
    let business = Address::generate(&env);

    env.as_contract(&contract_id, || {
        let mut invoice1 = create_test_invoice(&env, &business);
        invoice1.add_tag(&env, String::from_str(&env, "urgent")).unwrap();
        invoice1.add_tag(&env, String::from_str(&env, "priority")).unwrap();
        InvoiceStorage::store_invoice(&env, &invoice1);

        let mut invoice2 = create_test_invoice(&env, &business);
        invoice2.add_tag(&env, String::from_str(&env, "urgent")).unwrap();
        InvoiceStorage::store_invoice(&env, &invoice2);

        // Query by single tag
        let urgent = InvoiceStorage::get_invoices_by_tag(&env, &String::from_str(&env, "urgent"));
        assert_eq!(urgent.len(), 2, "Should have 2 invoices with 'urgent' tag");

        let priority =
            InvoiceStorage::get_invoices_by_tag(&env, &String::from_str(&env, "priority"));
        assert_eq!(priority.len(), 1, "Should have 1 invoice with 'priority' tag");

        // Query by multiple tags (AND logic)
        let mut tags = Vec::new(&env);
        tags.push_back(String::from_str(&env, "urgent"));
        tags.push_back(String::from_str(&env, "priority"));
        let both = InvoiceStorage::get_invoices_by_tags(&env, &tags);
        assert_eq!(both.len(), 1, "Should have 1 invoice with both tags");
    });
}

// ============================================================================
// BID STORAGE TESTS
// ============================================================================

/// Test: Basic bid storage put/get operations
#[test]
fn test_bid_storage_put_get() {
    let (env, contract_id) = setup();
    let investor = Address::generate(&env);

    env.as_contract(&contract_id, || {
        let invoice_id = BytesN::from_array(&env, &[1u8; 32]);
        let bid = create_test_bid(&env, &invoice_id, &investor);
        let bid_id = bid.bid_id.clone();

        // Store bid
        BidStorage::store_bid(&env, &bid);

        // Retrieve bid
        let retrieved = BidStorage::get_bid(&env, &bid_id);
        assert!(retrieved.is_some(), "Bid should be retrievable");

        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.bid_id, bid_id, "Bid ID should match");
        assert_eq!(retrieved.investor, investor, "Investor should match");
        assert_eq!(retrieved.bid_amount, 5_000, "Bid amount should match");
        assert_eq!(retrieved.status, BidStatus::Placed, "Status should be Placed");
    });
}

/// Test: Bid storage returns None for non-existent key
#[test]
fn test_bid_storage_missing_key() {
    let (env, contract_id) = setup();

    env.as_contract(&contract_id, || {
        let fake_id = BytesN::from_array(&env, &[0u8; 32]);
        let result = BidStorage::get_bid(&env, &fake_id);
        assert!(result.is_none(), "Non-existent bid should return None");
    });
}

/// Test: Bid update operation
#[test]
fn test_bid_storage_update() {
    let (env, contract_id) = setup();
    let investor = Address::generate(&env);

    env.as_contract(&contract_id, || {
        let invoice_id = BytesN::from_array(&env, &[1u8; 32]);
        let mut bid = create_test_bid(&env, &invoice_id, &investor);
        let bid_id = bid.bid_id.clone();
        BidStorage::store_bid(&env, &bid);

        // Update bid status
        bid.status = BidStatus::Accepted;
        BidStorage::update_bid(&env, &bid);

        // Verify update
        let retrieved = BidStorage::get_bid(&env, &bid_id).unwrap();
        assert_eq!(
            retrieved.status,
            BidStatus::Accepted,
            "Status should be updated to Accepted"
        );
    });
}

/// Test: Bid invoice index
#[test]
fn test_bid_invoice_index() {
    let (env, contract_id) = setup();
    let investor1 = Address::generate(&env);
    let investor2 = Address::generate(&env);

    env.as_contract(&contract_id, || {
        let invoice_id = BytesN::from_array(&env, &[1u8; 32]);

        // Create and store bids for same invoice
        let bid1 = create_test_bid(&env, &invoice_id, &investor1);
        let bid2 = create_test_bid(&env, &invoice_id, &investor2);
        let bid1_id = bid1.bid_id.clone();
        let bid2_id = bid2.bid_id.clone();

        BidStorage::store_bid(&env, &bid1);
        BidStorage::add_bid_to_invoice(&env, &invoice_id, &bid1_id);

        BidStorage::store_bid(&env, &bid2);
        BidStorage::add_bid_to_invoice(&env, &invoice_id, &bid2_id);

        // Verify invoice has both bids
        let invoice_bids = BidStorage::get_bids_for_invoice(&env, &invoice_id);
        assert_eq!(invoice_bids.len(), 2, "Invoice should have 2 bids");
        assert!(invoice_bids.contains(&bid1_id), "Should contain bid1");
        assert!(invoice_bids.contains(&bid2_id), "Should contain bid2");
    });
}

/// Test: Bid investor index
#[test]
fn test_bid_investor_index() {
    let (env, contract_id) = setup();
    let investor = Address::generate(&env);

    env.as_contract(&contract_id, || {
        let invoice1 = BytesN::from_array(&env, &[1u8; 32]);
        let invoice2 = BytesN::from_array(&env, &[2u8; 32]);

        // Create bids by same investor for different invoices
        let bid1 = create_test_bid(&env, &invoice1, &investor);
        let bid2 = create_test_bid(&env, &invoice2, &investor);
        let bid1_id = bid1.bid_id.clone();
        let bid2_id = bid2.bid_id.clone();

        BidStorage::store_bid(&env, &bid1);
        BidStorage::store_bid(&env, &bid2);

        // Verify investor index
        let investor_bids = BidStorage::get_bids_by_investor_all(&env, &investor);
        assert_eq!(investor_bids.len(), 2, "Investor should have 2 bids");
        assert!(investor_bids.contains(&bid1_id), "Should contain bid1");
        assert!(investor_bids.contains(&bid2_id), "Should contain bid2");
    });
}

/// Test: Duplicate prevention in bid indexes
#[test]
fn test_bid_index_duplicate_prevention() {
    let (env, contract_id) = setup();
    let investor = Address::generate(&env);

    env.as_contract(&contract_id, || {
        let invoice_id = BytesN::from_array(&env, &[1u8; 32]);

        let bid = create_test_bid(&env, &invoice_id, &investor);
        let bid_id = bid.bid_id.clone();
        BidStorage::store_bid(&env, &bid);

        // Add same bid to invoice index multiple times
        BidStorage::add_bid_to_invoice(&env, &invoice_id, &bid_id);
        BidStorage::add_bid_to_invoice(&env, &invoice_id, &bid_id);
        BidStorage::add_bid_to_invoice(&env, &invoice_id, &bid_id);

        // Verify no duplicates
        let invoice_bids = BidStorage::get_bids_for_invoice(&env, &invoice_id);
        assert_eq!(
            invoice_bids.len(),
            1,
            "Should have only 1 bid despite multiple adds"
        );
    });
}

/// Test: Bid expiration check
#[test]
fn test_bid_expiration() {
    let (env, contract_id) = setup();
    let investor = Address::generate(&env);

    env.as_contract(&contract_id, || {
        let invoice_id = BytesN::from_array(&env, &[1u8; 32]);
        let now = env.ledger().timestamp();
        let bid = Bid {
            bid_id: BidStorage::generate_unique_bid_id(&env),
            invoice_id: invoice_id.clone(),
            investor: investor.clone(),
            bid_amount: 5_000,
            expected_return: 5_500,
            timestamp: now,
            status: BidStatus::Placed,
            expiration_timestamp: now + 1000, // Expires in 1000 seconds
        };

        // Not expired at current time
        assert!(!bid.is_expired(now), "Bid should not be expired at creation");
        assert!(
            !bid.is_expired(now + 500),
            "Bid should not be expired before expiration"
        );

        // Expired after expiration timestamp
        assert!(
            bid.is_expired(now + 1001),
            "Bid should be expired after expiration timestamp"
        );
    });
}

/// Test: Bid cleanup of expired bids
#[test]
fn test_bid_cleanup_expired() {
    let (env, contract_id) = setup();
    let investor1 = Address::generate(&env);
    let investor2 = Address::generate(&env);

    // Set ledger timestamp to a non-zero value so expiration logic works
    env.ledger().set_timestamp(1000);

    env.as_contract(&contract_id, || {
        let invoice_id = BytesN::from_array(&env, &[1u8; 32]);
        let now = env.ledger().timestamp();

        // Create an expired bid (expiration in the past)
        let expired_bid = Bid {
            bid_id: BidStorage::generate_unique_bid_id(&env),
            invoice_id: invoice_id.clone(),
            investor: investor1.clone(),
            bid_amount: 5_000,
            expected_return: 5_500,
            timestamp: now,
            status: BidStatus::Placed,
            expiration_timestamp: now - 1, // Already expired
        };
        BidStorage::store_bid(&env, &expired_bid);
        BidStorage::add_bid_to_invoice(&env, &invoice_id, &expired_bid.bid_id);

        // Create an active bid
        let active_bid = create_test_bid(&env, &invoice_id, &investor2);
        let active_bid_id = active_bid.bid_id.clone();
        BidStorage::store_bid(&env, &active_bid);
        BidStorage::add_bid_to_invoice(&env, &invoice_id, &active_bid_id);

        // Verify both bids exist initially
        let bids_before = BidStorage::get_bids_for_invoice(&env, &invoice_id);
        assert_eq!(bids_before.len(), 2, "Should have 2 bids before cleanup");

        // Cleanup expired bids
        let expired_count = BidStorage::cleanup_expired_bids(&env, &invoice_id);
        assert_eq!(expired_count, 1, "Should have cleaned up 1 expired bid");

        // Verify only active bid remains in index
        let bids_after = BidStorage::get_bids_for_invoice(&env, &invoice_id);
        assert_eq!(bids_after.len(), 1, "Should have 1 bid after cleanup");
        assert!(
            bids_after.contains(&active_bid_id),
            "Active bid should remain"
        );
    });
}

/// Test: Bid ranking by profit margin
#[test]
fn test_bid_ranking() {
    let (env, contract_id) = setup();

    env.as_contract(&contract_id, || {
        let invoice_id = BytesN::from_array(&env, &[1u8; 32]);

        // Create bids with different profit margins
        let investor1 = Address::generate(&env);
        let mut bid1 = create_test_bid(&env, &invoice_id, &investor1);
        bid1.bid_amount = 1000;
        bid1.expected_return = 1100; // 100 profit
        BidStorage::store_bid(&env, &bid1);
        BidStorage::add_bid_to_invoice(&env, &invoice_id, &bid1.bid_id);

        let investor2 = Address::generate(&env);
        let mut bid2 = create_test_bid(&env, &invoice_id, &investor2);
        bid2.bid_amount = 1000;
        bid2.expected_return = 1200; // 200 profit (best)
        BidStorage::store_bid(&env, &bid2);
        BidStorage::add_bid_to_invoice(&env, &invoice_id, &bid2.bid_id);

        let investor3 = Address::generate(&env);
        let mut bid3 = create_test_bid(&env, &invoice_id, &investor3);
        bid3.bid_amount = 1000;
        bid3.expected_return = 1050; // 50 profit
        BidStorage::store_bid(&env, &bid3);
        BidStorage::add_bid_to_invoice(&env, &invoice_id, &bid3.bid_id);

        // Get ranked bids
        let ranked = BidStorage::rank_bids(&env, &invoice_id);
        assert_eq!(ranked.len(), 3, "Should have 3 ranked bids");

        // Best bid should be first (highest profit margin)
        let best = ranked.get(0).unwrap();
        assert_eq!(best.expected_return, 1200, "Best bid should have 1200 return");
    });
}

/// Test: Get best bid
#[test]
fn test_get_best_bid() {
    let (env, contract_id) = setup();

    env.as_contract(&contract_id, || {
        let invoice_id = BytesN::from_array(&env, &[1u8; 32]);

        // Create multiple bids
        let investor1 = Address::generate(&env);
        let mut bid1 = create_test_bid(&env, &invoice_id, &investor1);
        bid1.bid_amount = 1000;
        bid1.expected_return = 1100;
        BidStorage::store_bid(&env, &bid1);
        BidStorage::add_bid_to_invoice(&env, &invoice_id, &bid1.bid_id);

        let investor2 = Address::generate(&env);
        let mut bid2 = create_test_bid(&env, &invoice_id, &investor2);
        bid2.bid_amount = 1000;
        bid2.expected_return = 1300; // Higher profit
        BidStorage::store_bid(&env, &bid2);
        BidStorage::add_bid_to_invoice(&env, &invoice_id, &bid2.bid_id);

        let best = BidStorage::get_best_bid(&env, &invoice_id);
        assert!(best.is_some(), "Should find best bid");
        assert_eq!(
            best.unwrap().expected_return,
            1300,
            "Best bid should have highest return"
        );
    });
}

/// Test: Get best bid returns None for invoice with no bids
#[test]
fn test_get_best_bid_empty() {
    let (env, contract_id) = setup();

    env.as_contract(&contract_id, || {
        let invoice_id = BytesN::from_array(&env, &[99u8; 32]);
        let best = BidStorage::get_best_bid(&env, &invoice_id);
        assert!(
            best.is_none(),
            "Should return None for invoice with no bids"
        );
    });
}

// ============================================================================
// INVESTMENT STORAGE TESTS
// ============================================================================

/// Test: Basic investment storage put/get operations
#[test]
fn test_investment_storage_put_get() {
    let (env, contract_id) = setup();
    let investor = Address::generate(&env);

    env.as_contract(&contract_id, || {
        let invoice_id = BytesN::from_array(&env, &[1u8; 32]);
        let investment = create_test_investment(&env, &invoice_id, &investor);
        let investment_id = investment.investment_id.clone();

        // Store investment
        InvestmentStorage::store_investment(&env, &investment);

        // Retrieve by ID
        let retrieved = InvestmentStorage::get_investment(&env, &investment_id);
        assert!(retrieved.is_some(), "Investment should be retrievable");

        let retrieved = retrieved.unwrap();
        assert_eq!(
            retrieved.investment_id, investment_id,
            "Investment ID should match"
        );
        assert_eq!(retrieved.invoice_id, invoice_id, "Invoice ID should match");
        assert_eq!(retrieved.investor, investor, "Investor should match");
        assert_eq!(retrieved.amount, 10_000, "Amount should match");
        assert_eq!(
            retrieved.status,
            InvestmentStatus::Active,
            "Status should be Active"
        );
    });
}

/// Test: Investment storage returns None for non-existent key
#[test]
fn test_investment_storage_missing_key() {
    let (env, contract_id) = setup();

    env.as_contract(&contract_id, || {
        let fake_id = BytesN::from_array(&env, &[0u8; 32]);
        let result = InvestmentStorage::get_investment(&env, &fake_id);
        assert!(
            result.is_none(),
            "Non-existent investment should return None"
        );
    });
}

/// Test: Investment invoice index (1:1 mapping)
#[test]
fn test_investment_invoice_index() {
    let (env, contract_id) = setup();
    let investor = Address::generate(&env);

    env.as_contract(&contract_id, || {
        let invoice_id = BytesN::from_array(&env, &[1u8; 32]);
        let investment = create_test_investment(&env, &invoice_id, &investor);
        InvestmentStorage::store_investment(&env, &investment);

        // Retrieve by invoice ID
        let by_invoice = InvestmentStorage::get_investment_by_invoice(&env, &invoice_id);
        assert!(by_invoice.is_some(), "Should find investment by invoice ID");
        assert_eq!(
            by_invoice.unwrap().investor,
            investor,
            "Investor should match"
        );
    });
}

/// Test: Investment investor index
#[test]
fn test_investment_investor_index() {
    let (env, contract_id) = setup();
    let investor = Address::generate(&env);

    env.as_contract(&contract_id, || {
        let invoice1 = BytesN::from_array(&env, &[1u8; 32]);
        let invoice2 = BytesN::from_array(&env, &[2u8; 32]);

        // Create investments for same investor
        let investment1 = create_test_investment(&env, &invoice1, &investor);
        let investment2 = create_test_investment(&env, &invoice2, &investor);
        let id1 = investment1.investment_id.clone();
        let id2 = investment2.investment_id.clone();

        InvestmentStorage::store_investment(&env, &investment1);
        InvestmentStorage::store_investment(&env, &investment2);

        // Query by investor
        let investor_investments = InvestmentStorage::get_investments_by_investor(&env, &investor);
        assert_eq!(
            investor_investments.len(),
            2,
            "Investor should have 2 investments"
        );
        assert!(
            investor_investments.contains(&id1),
            "Should contain investment1"
        );
        assert!(
            investor_investments.contains(&id2),
            "Should contain investment2"
        );
    });
}

/// Test: Investment investor index duplicate prevention
#[test]
fn test_investment_investor_index_duplicate_prevention() {
    let (env, contract_id) = setup();
    let investor = Address::generate(&env);

    env.as_contract(&contract_id, || {
        let invoice_id = BytesN::from_array(&env, &[1u8; 32]);
        let investment = create_test_investment(&env, &invoice_id, &investor);
        let investment_id = investment.investment_id.clone();
        InvestmentStorage::store_investment(&env, &investment);

        // Try to add same investment to index multiple times
        InvestmentStorage::add_to_investor_index(&env, &investor, &investment_id);
        InvestmentStorage::add_to_investor_index(&env, &investor, &investment_id);

        // Verify no duplicates
        let investments = InvestmentStorage::get_investments_by_investor(&env, &investor);
        assert_eq!(
            investments.len(),
            1,
            "Should have only 1 investment despite multiple adds"
        );
    });
}

/// Test: Investment update operation
#[test]
fn test_investment_storage_update() {
    let (env, contract_id) = setup();
    let investor = Address::generate(&env);

    env.as_contract(&contract_id, || {
        let invoice_id = BytesN::from_array(&env, &[1u8; 32]);
        let mut investment = create_test_investment(&env, &invoice_id, &investor);
        let investment_id = investment.investment_id.clone();
        InvestmentStorage::store_investment(&env, &investment);

        // Update status
        investment.status = InvestmentStatus::Completed;
        InvestmentStorage::update_investment(&env, &investment);

        // Verify update
        let retrieved = InvestmentStorage::get_investment(&env, &investment_id).unwrap();
        assert_eq!(
            retrieved.status,
            InvestmentStatus::Completed,
            "Status should be updated"
        );
    });
}

// ============================================================================
// ESCROW STORAGE TESTS
// ============================================================================

/// Test: Basic escrow storage put/get operations
#[test]
fn test_escrow_storage_put_get() {
    let (env, contract_id) = setup();
    let investor = Address::generate(&env);
    let business = Address::generate(&env);

    env.as_contract(&contract_id, || {
        let invoice_id = BytesN::from_array(&env, &[1u8; 32]);
        let escrow = create_test_escrow(&env, &invoice_id, &investor, &business);
        let escrow_id = escrow.escrow_id.clone();

        // Store escrow
        EscrowStorage::store_escrow(&env, &escrow);

        // Retrieve by ID
        let retrieved = EscrowStorage::get_escrow(&env, &escrow_id);
        assert!(retrieved.is_some(), "Escrow should be retrievable");

        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.escrow_id, escrow_id, "Escrow ID should match");
        assert_eq!(retrieved.invoice_id, invoice_id, "Invoice ID should match");
        assert_eq!(retrieved.investor, investor, "Investor should match");
        assert_eq!(retrieved.business, business, "Business should match");
        assert_eq!(retrieved.amount, 10_000, "Amount should match");
        assert_eq!(retrieved.status, EscrowStatus::Held, "Status should be Held");
    });
}

/// Test: Escrow storage returns None for non-existent key
#[test]
fn test_escrow_storage_missing_key() {
    let (env, contract_id) = setup();

    env.as_contract(&contract_id, || {
        let fake_id = BytesN::from_array(&env, &[0u8; 32]);
        let result = EscrowStorage::get_escrow(&env, &fake_id);
        assert!(result.is_none(), "Non-existent escrow should return None");
    });
}

/// Test: Escrow invoice index
#[test]
fn test_escrow_invoice_index() {
    let (env, contract_id) = setup();
    let investor = Address::generate(&env);
    let business = Address::generate(&env);

    env.as_contract(&contract_id, || {
        let invoice_id = BytesN::from_array(&env, &[1u8; 32]);
        let escrow = create_test_escrow(&env, &invoice_id, &investor, &business);
        EscrowStorage::store_escrow(&env, &escrow);

        // Retrieve by invoice ID
        let by_invoice = EscrowStorage::get_escrow_by_invoice(&env, &invoice_id);
        assert!(by_invoice.is_some(), "Should find escrow by invoice ID");
        assert_eq!(
            by_invoice.unwrap().investor,
            investor,
            "Investor should match"
        );
    });
}

/// Test: Escrow update operation
#[test]
fn test_escrow_storage_update() {
    let (env, contract_id) = setup();
    let investor = Address::generate(&env);
    let business = Address::generate(&env);

    env.as_contract(&contract_id, || {
        let invoice_id = BytesN::from_array(&env, &[1u8; 32]);
        let mut escrow = create_test_escrow(&env, &invoice_id, &investor, &business);
        let escrow_id = escrow.escrow_id.clone();
        EscrowStorage::store_escrow(&env, &escrow);

        // Update status
        escrow.status = EscrowStatus::Released;
        EscrowStorage::update_escrow(&env, &escrow);

        // Verify update
        let retrieved = EscrowStorage::get_escrow(&env, &escrow_id).unwrap();
        assert_eq!(
            retrieved.status,
            EscrowStatus::Released,
            "Status should be updated"
        );
    });
}

// ============================================================================
// ID GENERATION TESTS
// ============================================================================

/// Test: Invoice ID generation uniqueness
#[test]
fn test_invoice_id_uniqueness() {
    let (env, contract_id) = setup();
    let business = Address::generate(&env);

    env.as_contract(&contract_id, || {
        let mut ids: Vec<BytesN<32>> = Vec::new(&env);

        // Generate multiple invoice IDs
        for _ in 0..10 {
            let invoice = create_test_invoice(&env, &business);
            // Verify ID is unique
            for existing_id in ids.iter() {
                assert_ne!(existing_id, invoice.id, "Invoice IDs should be unique");
            }
            ids.push_back(invoice.id.clone());
        }
    });
}

/// Test: Bid ID generation uniqueness
#[test]
fn test_bid_id_uniqueness() {
    let (env, contract_id) = setup();

    env.as_contract(&contract_id, || {
        let mut ids: Vec<BytesN<32>> = Vec::new(&env);

        // Generate multiple bid IDs
        for _ in 0..10 {
            let id = BidStorage::generate_unique_bid_id(&env);
            // Verify ID is unique
            for existing_id in ids.iter() {
                assert_ne!(existing_id, id, "Bid IDs should be unique");
            }
            ids.push_back(id);
        }
    });
}

/// Test: Investment ID generation uniqueness
#[test]
fn test_investment_id_uniqueness() {
    let (env, contract_id) = setup();

    env.as_contract(&contract_id, || {
        let mut ids: Vec<BytesN<32>> = Vec::new(&env);

        // Generate multiple investment IDs
        for _ in 0..10 {
            let id = InvestmentStorage::generate_unique_investment_id(&env);
            // Verify ID is unique
            for existing_id in ids.iter() {
                assert_ne!(existing_id, id, "Investment IDs should be unique");
            }
            ids.push_back(id);
        }
    });
}

/// Test: Escrow ID generation uniqueness
#[test]
fn test_escrow_id_uniqueness() {
    let (env, contract_id) = setup();

    env.as_contract(&contract_id, || {
        let mut ids: Vec<BytesN<32>> = Vec::new(&env);

        // Generate multiple escrow IDs
        for _ in 0..10 {
            let id = EscrowStorage::generate_unique_escrow_id(&env);
            // Verify ID is unique
            for existing_id in ids.iter() {
                assert_ne!(existing_id, id, "Escrow IDs should be unique");
            }
            ids.push_back(id);
        }
    });
}

/// Test: ID generation counter persistence
#[test]
fn test_id_counter_persistence() {
    let (env, contract_id) = setup();

    env.as_contract(&contract_id, || {
        // Generate first bid ID
        let id1 = BidStorage::generate_unique_bid_id(&env);

        // Generate second bid ID - counter should have incremented
        let id2 = BidStorage::generate_unique_bid_id(&env);

        // IDs should be different due to counter
        assert_ne!(id1, id2, "Sequential IDs should be different");

        // Counter bytes (10-18) should show increment
        let id1_bytes: [u8; 32] = id1.to_array();
        let id2_bytes: [u8; 32] = id2.to_array();

        // Extract counter from bytes 10-18
        let counter1 = u64::from_be_bytes(id1_bytes[10..18].try_into().unwrap());
        let counter2 = u64::from_be_bytes(id2_bytes[10..18].try_into().unwrap());

        assert_eq!(counter2, counter1 + 1, "Counter should increment by 1");
    });
}

// ============================================================================
// CORE TYPE TESTS - INVOICE
// ============================================================================

/// Test: Invoice status enum values
#[test]
fn test_invoice_status_enum_values() {
    let (env, contract_id) = setup();
    let business = Address::generate(&env);

    env.as_contract(&contract_id, || {
        let invoice = create_test_invoice(&env, &business);
        assert_eq!(invoice.status, InvoiceStatus::Pending);

        // Verify all enum variants exist and are distinct
        assert_ne!(InvoiceStatus::Pending, InvoiceStatus::Verified);
        assert_ne!(InvoiceStatus::Verified, InvoiceStatus::Funded);
        assert_ne!(InvoiceStatus::Funded, InvoiceStatus::Paid);
        assert_ne!(InvoiceStatus::Paid, InvoiceStatus::Defaulted);
        assert_ne!(InvoiceStatus::Defaulted, InvoiceStatus::Cancelled);
    });
}

/// Test: Invoice valid status transitions
#[test]
fn test_invoice_valid_status_transitions() {
    let (env, contract_id) = setup();
    let business = Address::generate(&env);
    let investor = Address::generate(&env);

    env.as_contract(&contract_id, || {
        let mut invoice = create_test_invoice(&env, &business);

        // Pending -> Verified (via verify method)
        invoice.verify(&env, business.clone());
        assert_eq!(invoice.status, InvoiceStatus::Verified);

        // Verified -> Funded (via mark_as_funded)
        invoice.mark_as_funded(&env, investor.clone(), 10_000, env.ledger().timestamp());
        assert_eq!(invoice.status, InvoiceStatus::Funded);

        // Funded -> Paid (via mark_as_paid)
        invoice.mark_as_paid(&env, business.clone(), env.ledger().timestamp());
        assert_eq!(invoice.status, InvoiceStatus::Paid);
    });
}

/// Test: Invoice cancel - valid from Pending
#[test]
fn test_invoice_cancel_from_pending() {
    let (env, contract_id) = setup();
    let business = Address::generate(&env);

    env.as_contract(&contract_id, || {
        let mut invoice = create_test_invoice(&env, &business);
        assert_eq!(invoice.status, InvoiceStatus::Pending);

        let result = invoice.cancel(&env, business.clone());
        assert!(result.is_ok(), "Should allow cancel from Pending");
        assert_eq!(invoice.status, InvoiceStatus::Cancelled);
    });
}

/// Test: Invoice cancel - valid from Verified
#[test]
fn test_invoice_cancel_from_verified() {
    let (env, contract_id) = setup();
    let business = Address::generate(&env);

    env.as_contract(&contract_id, || {
        let mut invoice = create_test_invoice(&env, &business);
        invoice.verify(&env, business.clone());
        assert_eq!(invoice.status, InvoiceStatus::Verified);

        let result = invoice.cancel(&env, business.clone());
        assert!(result.is_ok(), "Should allow cancel from Verified");
        assert_eq!(invoice.status, InvoiceStatus::Cancelled);
    });
}

/// Test: Invoice cancel - invalid from Funded
#[test]
fn test_invoice_cancel_from_funded_fails() {
    let (env, contract_id) = setup();
    let business = Address::generate(&env);
    let investor = Address::generate(&env);

    env.as_contract(&contract_id, || {
        let mut invoice = create_test_invoice(&env, &business);
        invoice.verify(&env, business.clone());
        invoice.mark_as_funded(&env, investor.clone(), 10_000, env.ledger().timestamp());
        assert_eq!(invoice.status, InvoiceStatus::Funded);

        let result = invoice.cancel(&env, business.clone());
        assert!(result.is_err(), "Should not allow cancel from Funded");
        assert_eq!(
            result.unwrap_err(),
            QuickLendXError::InvalidStatus,
            "Should return InvalidStatus error"
        );
    });
}

/// Test: Invoice mark_as_defaulted
#[test]
fn test_invoice_mark_as_defaulted() {
    let (env, contract_id) = setup();
    let business = Address::generate(&env);
    let investor = Address::generate(&env);

    env.as_contract(&contract_id, || {
        let mut invoice = create_test_invoice(&env, &business);
        invoice.verify(&env, business.clone());
        invoice.mark_as_funded(&env, investor.clone(), 10_000, env.ledger().timestamp());

        invoice.mark_as_defaulted();
        assert_eq!(invoice.status, InvoiceStatus::Defaulted);
    });
}

/// Test: Invoice is_available_for_funding
#[test]
fn test_invoice_is_available_for_funding() {
    let (env, contract_id) = setup();
    let business = Address::generate(&env);
    let investor = Address::generate(&env);

    env.as_contract(&contract_id, || {
        let mut invoice = create_test_invoice(&env, &business);

        // Pending - not available
        assert!(
            !invoice.is_available_for_funding(),
            "Pending invoice should not be available"
        );

        // Verified - available
        invoice.verify(&env, business.clone());
        assert!(
            invoice.is_available_for_funding(),
            "Verified invoice should be available"
        );

        // Funded - not available
        invoice.mark_as_funded(&env, investor.clone(), 10_000, env.ledger().timestamp());
        assert!(
            !invoice.is_available_for_funding(),
            "Funded invoice should not be available"
        );
    });
}

/// Test: Invoice is_overdue
#[test]
fn test_invoice_is_overdue() {
    let (env, contract_id) = setup();
    let business = Address::generate(&env);

    env.as_contract(&contract_id, || {
        let due_date = env.ledger().timestamp() + 1000;
        let invoice = Invoice::new(
            &env,
            business.clone(),
            10_000,
            Address::generate(&env),
            due_date,
            String::from_str(&env, "Test"),
            InvoiceCategory::Services,
            Vec::new(&env),
        );

        assert!(
            !invoice.is_overdue(due_date - 1),
            "Should not be overdue before due date"
        );
        assert!(!invoice.is_overdue(due_date), "Should not be overdue on due date");
        assert!(invoice.is_overdue(due_date + 1), "Should be overdue after due date");
    });
}

/// Test: Invoice grace deadline calculation
#[test]
fn test_invoice_grace_deadline() {
    let (env, contract_id) = setup();
    let business = Address::generate(&env);

    env.as_contract(&contract_id, || {
        let due_date = 1000u64;
        let invoice = Invoice::new(
            &env,
            business.clone(),
            10_000,
            Address::generate(&env),
            due_date,
            String::from_str(&env, "Test"),
            InvoiceCategory::Services,
            Vec::new(&env),
        );

        let grace_period = 14 * 24 * 60 * 60; // 14 days
        let deadline = invoice.grace_deadline(grace_period);
        assert_eq!(
            deadline,
            due_date + grace_period,
            "Grace deadline should be due_date + grace_period"
        );
    });
}

/// Test: Invoice payment recording
#[test]
fn test_invoice_payment_recording() {
    let (env, contract_id) = setup();
    let business = Address::generate(&env);

    env.as_contract(&contract_id, || {
        let mut invoice = create_test_invoice(&env, &business);
        assert_eq!(invoice.total_paid, 0, "Initial total_paid should be 0");
        assert_eq!(
            invoice.payment_history.len(),
            0,
            "Initial payment_history should be empty"
        );

        // Record first payment
        let result = invoice.record_payment(&env, 3_000, String::from_str(&env, "TX-001"));
        assert!(result.is_ok(), "Payment recording should succeed");
        assert_eq!(invoice.total_paid, 3_000, "total_paid should be 3000");
        assert_eq!(
            invoice.payment_history.len(),
            1,
            "Should have 1 payment record"
        );

        // Record second payment
        let result = invoice.record_payment(&env, 5_000, String::from_str(&env, "TX-002"));
        assert!(result.is_ok());
        assert_eq!(invoice.total_paid, 8_000, "total_paid should be 8000");
        assert_eq!(
            invoice.payment_history.len(),
            2,
            "Should have 2 payment records"
        );
    });
}

/// Test: Invoice payment recording - invalid amount
#[test]
fn test_invoice_payment_recording_invalid_amount() {
    let (env, contract_id) = setup();
    let business = Address::generate(&env);

    env.as_contract(&contract_id, || {
        let mut invoice = create_test_invoice(&env, &business);

        // Try to record zero payment
        let result = invoice.record_payment(&env, 0, String::from_str(&env, "TX-000"));
        assert!(result.is_err(), "Zero payment should fail");
        assert_eq!(result.unwrap_err(), QuickLendXError::InvalidAmount);

        // Try to record negative payment
        let result = invoice.record_payment(&env, -100, String::from_str(&env, "TX-NEG"));
        assert!(result.is_err(), "Negative payment should fail");
    });
}

/// Test: Invoice payment progress calculation
#[test]
fn test_invoice_payment_progress() {
    let (env, contract_id) = setup();
    let business = Address::generate(&env);

    env.as_contract(&contract_id, || {
        let mut invoice = create_test_invoice(&env, &business); // amount = 10_000

        assert_eq!(invoice.payment_progress(), 0, "Initial progress should be 0%");

        invoice
            .record_payment(&env, 2_500, String::from_str(&env, "TX-1"))
            .unwrap();
        assert_eq!(invoice.payment_progress(), 25, "Progress should be 25%");

        invoice
            .record_payment(&env, 5_000, String::from_str(&env, "TX-2"))
            .unwrap();
        assert_eq!(invoice.payment_progress(), 75, "Progress should be 75%");

        invoice
            .record_payment(&env, 2_500, String::from_str(&env, "TX-3"))
            .unwrap();
        assert_eq!(invoice.payment_progress(), 100, "Progress should be 100%");
    });
}

/// Test: Invoice is_fully_paid
#[test]
fn test_invoice_is_fully_paid() {
    let (env, contract_id) = setup();
    let business = Address::generate(&env);

    env.as_contract(&contract_id, || {
        let mut invoice = create_test_invoice(&env, &business); // amount = 10_000

        assert!(!invoice.is_fully_paid(), "Should not be fully paid initially");

        invoice
            .record_payment(&env, 5_000, String::from_str(&env, "TX-1"))
            .unwrap();
        assert!(!invoice.is_fully_paid(), "Should not be fully paid at 50%");

        invoice
            .record_payment(&env, 5_000, String::from_str(&env, "TX-2"))
            .unwrap();
        assert!(invoice.is_fully_paid(), "Should be fully paid at 100%");

        // Overpayment
        invoice
            .record_payment(&env, 1_000, String::from_str(&env, "TX-3"))
            .unwrap();
        assert!(
            invoice.is_fully_paid(),
            "Should still be fully paid with overpayment"
        );
    });
}

// ============================================================================
// CORE TYPE TESTS - INVOICE TAGS
// ============================================================================

/// Test: Invoice add_tag
#[test]
fn test_invoice_add_tag() {
    let (env, contract_id) = setup();
    let business = Address::generate(&env);

    env.as_contract(&contract_id, || {
        let mut invoice = create_test_invoice(&env, &business);
        assert_eq!(invoice.tags.len(), 0, "Initial tags should be empty");

        let result = invoice.add_tag(&env, String::from_str(&env, "urgent"));
        assert!(result.is_ok(), "Adding tag should succeed");
        assert_eq!(invoice.tags.len(), 1, "Should have 1 tag");
        assert!(
            invoice.has_tag(String::from_str(&env, "urgent")),
            "Should have 'urgent' tag"
        );
    });
}

/// Test: Invoice add_tag - duplicate handling
#[test]
fn test_invoice_add_tag_duplicate() {
    let (env, contract_id) = setup();
    let business = Address::generate(&env);

    env.as_contract(&contract_id, || {
        let mut invoice = create_test_invoice(&env, &business);

        invoice
            .add_tag(&env, String::from_str(&env, "urgent"))
            .unwrap();
        invoice
            .add_tag(&env, String::from_str(&env, "urgent"))
            .unwrap(); // Duplicate

        // Should not add duplicate
        assert_eq!(invoice.tags.len(), 1, "Should still have only 1 tag");
    });
}

/// Test: Invoice add_tag - limit exceeded
#[test]
fn test_invoice_add_tag_limit_exceeded() {
    let (env, contract_id) = setup();
    let business = Address::generate(&env);

    env.as_contract(&contract_id, || {
        let mut invoice = create_test_invoice(&env, &business);

        // Add 10 tags (limit) - using distinct tag names
        let tag_names = [
            "tag0", "tag1", "tag2", "tag3", "tag4", "tag5", "tag6", "tag7", "tag8", "tag9",
        ];
        for tag_name in tag_names.iter() {
            let tag = String::from_str(&env, tag_name);
            let result = invoice.add_tag(&env, tag);
            assert!(result.is_ok(), "Should allow up to 10 tags");
        }

        // Try to add 11th tag
        let result = invoice.add_tag(&env, String::from_str(&env, "tag10"));
        assert!(result.is_err(), "Should fail when exceeding tag limit");
        assert_eq!(result.unwrap_err(), QuickLendXError::TagLimitExceeded);
    });
}

/// Test: Invoice add_tag - invalid tag (empty)
#[test]
fn test_invoice_add_tag_invalid_empty() {
    let (env, contract_id) = setup();
    let business = Address::generate(&env);

    env.as_contract(&contract_id, || {
        let mut invoice = create_test_invoice(&env, &business);

        let result = invoice.add_tag(&env, String::from_str(&env, ""));
        assert!(result.is_err(), "Empty tag should fail");
        assert_eq!(result.unwrap_err(), QuickLendXError::InvalidTag);
    });
}

/// Test: Invoice remove_tag
#[test]
fn test_invoice_remove_tag() {
    let (env, contract_id) = setup();
    let business = Address::generate(&env);

    env.as_contract(&contract_id, || {
        let mut invoice = create_test_invoice(&env, &business);
        invoice
            .add_tag(&env, String::from_str(&env, "urgent"))
            .unwrap();
        invoice
            .add_tag(&env, String::from_str(&env, "priority"))
            .unwrap();

        let result = invoice.remove_tag(String::from_str(&env, "urgent"));
        assert!(result.is_ok(), "Removing existing tag should succeed");
        assert_eq!(invoice.tags.len(), 1, "Should have 1 tag remaining");
        assert!(
            !invoice.has_tag(String::from_str(&env, "urgent")),
            "Should not have 'urgent' tag"
        );
        assert!(
            invoice.has_tag(String::from_str(&env, "priority")),
            "Should still have 'priority' tag"
        );
    });
}

/// Test: Invoice remove_tag - non-existent tag
#[test]
fn test_invoice_remove_tag_not_found() {
    let (env, contract_id) = setup();
    let business = Address::generate(&env);

    env.as_contract(&contract_id, || {
        let mut invoice = create_test_invoice(&env, &business);
        invoice
            .add_tag(&env, String::from_str(&env, "urgent"))
            .unwrap();

        let result = invoice.remove_tag(String::from_str(&env, "nonexistent"));
        assert!(result.is_err(), "Removing non-existent tag should fail");
        assert_eq!(result.unwrap_err(), QuickLendXError::InvalidTag);
    });
}

// ============================================================================
// CORE TYPE TESTS - INVOICE RATINGS
// ============================================================================

/// Test: Invoice add_rating - valid
#[test]
fn test_invoice_add_rating_valid() {
    let (env, contract_id) = setup();
    let business = Address::generate(&env);
    let investor = Address::generate(&env);

    env.as_contract(&contract_id, || {
        let mut invoice = create_test_invoice(&env, &business);
        invoice.verify(&env, business.clone());
        invoice.mark_as_funded(&env, investor.clone(), 10_000, env.ledger().timestamp());

        let result = invoice.add_rating(
            4,
            String::from_str(&env, "Great experience!"),
            investor.clone(),
            env.ledger().timestamp(),
        );

        assert!(result.is_ok(), "Adding valid rating should succeed");
        assert_eq!(invoice.total_ratings, 1, "Should have 1 rating");
        assert_eq!(invoice.average_rating, Some(4), "Average should be 4");
        assert!(invoice.has_ratings(), "Should have ratings");
    });
}

/// Test: Invoice add_rating - not funded
#[test]
fn test_invoice_add_rating_not_funded() {
    let (env, contract_id) = setup();
    let business = Address::generate(&env);
    let investor = Address::generate(&env);

    env.as_contract(&contract_id, || {
        let mut invoice = create_test_invoice(&env, &business);

        let result = invoice.add_rating(
            4,
            String::from_str(&env, "Rating"),
            investor.clone(),
            env.ledger().timestamp(),
        );

        assert!(result.is_err(), "Rating unfunded invoice should fail");
        assert_eq!(result.unwrap_err(), QuickLendXError::NotFunded);
    });
}

/// Test: Invoice add_rating - not rater (wrong investor)
#[test]
fn test_invoice_add_rating_not_rater() {
    let (env, contract_id) = setup();
    let business = Address::generate(&env);
    let investor = Address::generate(&env);
    let other_investor = Address::generate(&env);

    env.as_contract(&contract_id, || {
        let mut invoice = create_test_invoice(&env, &business);
        invoice.verify(&env, business.clone());
        invoice.mark_as_funded(&env, investor.clone(), 10_000, env.ledger().timestamp());

        let result = invoice.add_rating(
            4,
            String::from_str(&env, "Rating"),
            other_investor.clone(), // Different investor
            env.ledger().timestamp(),
        );

        assert!(result.is_err(), "Rating by non-investor should fail");
        assert_eq!(result.unwrap_err(), QuickLendXError::NotRater);
    });
}

/// Test: Invoice add_rating - invalid rating value
#[test]
fn test_invoice_add_rating_invalid_value() {
    let (env, contract_id) = setup();
    let business = Address::generate(&env);
    let investor = Address::generate(&env);

    env.as_contract(&contract_id, || {
        let mut invoice = create_test_invoice(&env, &business);
        invoice.verify(&env, business.clone());
        invoice.mark_as_funded(&env, investor.clone(), 10_000, env.ledger().timestamp());

        // Rating too low
        let result = invoice.add_rating(
            0,
            String::from_str(&env, "Rating"),
            investor.clone(),
            env.ledger().timestamp(),
        );
        assert!(result.is_err(), "Rating of 0 should fail");
        assert_eq!(result.unwrap_err(), QuickLendXError::InvalidRating);

        // Rating too high
        let result = invoice.add_rating(
            6,
            String::from_str(&env, "Rating"),
            investor.clone(),
            env.ledger().timestamp(),
        );
        assert!(result.is_err(), "Rating of 6 should fail");
        assert_eq!(result.unwrap_err(), QuickLendXError::InvalidRating);
    });
}

/// Test: Invoice add_rating - already rated
#[test]
fn test_invoice_add_rating_already_rated() {
    let (env, contract_id) = setup();
    let business = Address::generate(&env);
    let investor = Address::generate(&env);

    env.as_contract(&contract_id, || {
        let mut invoice = create_test_invoice(&env, &business);
        invoice.verify(&env, business.clone());
        invoice.mark_as_funded(&env, investor.clone(), 10_000, env.ledger().timestamp());

        // First rating
        invoice
            .add_rating(
                4,
                String::from_str(&env, "Rating 1"),
                investor.clone(),
                env.ledger().timestamp(),
            )
            .unwrap();

        // Second rating by same investor
        let result = invoice.add_rating(
            5,
            String::from_str(&env, "Rating 2"),
            investor.clone(),
            env.ledger().timestamp(),
        );

        assert!(result.is_err(), "Duplicate rating should fail");
        assert_eq!(result.unwrap_err(), QuickLendXError::AlreadyRated);
    });
}

/// Test: Invoice get_ratings_above threshold
#[test]
fn test_invoice_get_ratings_above() {
    let (env, contract_id) = setup();
    let business = Address::generate(&env);

    env.as_contract(&contract_id, || {
        // Create invoice with pre-populated ratings for testing
        let mut invoice = create_test_invoice(&env, &business);
        invoice.status = InvoiceStatus::Funded;

        // Manually add ratings to bypass validation
        invoice.ratings.push_back(InvoiceRating {
            rating: 3,
            feedback: String::from_str(&env, "OK"),
            rated_by: Address::generate(&env),
            rated_at: 1000,
        });
        invoice.ratings.push_back(InvoiceRating {
            rating: 5,
            feedback: String::from_str(&env, "Excellent"),
            rated_by: Address::generate(&env),
            rated_at: 2000,
        });
        invoice.ratings.push_back(InvoiceRating {
            rating: 2,
            feedback: String::from_str(&env, "Poor"),
            rated_by: Address::generate(&env),
            rated_at: 3000,
        });
        invoice.total_ratings = 3;

        let above_4 = invoice.get_ratings_above(&env, 4);
        assert_eq!(above_4.len(), 1, "Should have 1 rating >= 4");

        let above_2 = invoice.get_ratings_above(&env, 2);
        assert_eq!(above_2.len(), 3, "Should have 3 ratings >= 2");

        let above_5 = invoice.get_ratings_above(&env, 5);
        assert_eq!(above_5.len(), 1, "Should have 1 rating >= 5");
    });
}

/// Test: Invoice highest and lowest rating
#[test]
fn test_invoice_highest_lowest_rating() {
    let (env, contract_id) = setup();
    let business = Address::generate(&env);

    env.as_contract(&contract_id, || {
        let mut invoice = create_test_invoice(&env, &business);

        // No ratings
        assert!(invoice.get_highest_rating().is_none());
        assert!(invoice.get_lowest_rating().is_none());

        // Add ratings
        invoice.ratings.push_back(InvoiceRating {
            rating: 3,
            feedback: String::from_str(&env, "OK"),
            rated_by: Address::generate(&env),
            rated_at: 1000,
        });
        invoice.ratings.push_back(InvoiceRating {
            rating: 5,
            feedback: String::from_str(&env, "Excellent"),
            rated_by: Address::generate(&env),
            rated_at: 2000,
        });
        invoice.ratings.push_back(InvoiceRating {
            rating: 1,
            feedback: String::from_str(&env, "Bad"),
            rated_by: Address::generate(&env),
            rated_at: 3000,
        });

        assert_eq!(invoice.get_highest_rating(), Some(5), "Highest should be 5");
        assert_eq!(invoice.get_lowest_rating(), Some(1), "Lowest should be 1");
    });
}

// ============================================================================
// CORE TYPE TESTS - BID STATUS
// ============================================================================

/// Test: Bid status enum values
#[test]
fn test_bid_status_enum_values() {
    assert_ne!(BidStatus::Placed, BidStatus::Withdrawn);
    assert_ne!(BidStatus::Withdrawn, BidStatus::Accepted);
    assert_ne!(BidStatus::Accepted, BidStatus::Expired);
}

/// Test: Bid default expiration calculation
#[test]
fn test_bid_default_expiration() {
    let now = 1000u64;
    let expected_ttl = 7 * 24 * 60 * 60; // 7 days in seconds

    let expiration = Bid::default_expiration(now);
    assert_eq!(
        expiration,
        now + expected_ttl,
        "Expiration should be 7 days from now"
    );
}

// ============================================================================
// CORE TYPE TESTS - INVESTMENT
// ============================================================================

/// Test: Investment status enum values
#[test]
fn test_investment_status_enum_values() {
    assert_ne!(InvestmentStatus::Active, InvestmentStatus::Withdrawn);
    assert_ne!(InvestmentStatus::Withdrawn, InvestmentStatus::Completed);
    assert_ne!(InvestmentStatus::Completed, InvestmentStatus::Defaulted);
}

/// Test: Investment calculate_premium
#[test]
fn test_investment_calculate_premium() {
    // Normal case: 10,000 amount, 50% coverage = 5,000 coverage, 2% premium = 100
    let premium = Investment::calculate_premium(10_000, 50);
    assert_eq!(
        premium, 100,
        "Premium should be 100 for 50% coverage of 10,000"
    );

    // Full coverage: 10,000 amount, 100% coverage = 10,000 coverage, 2% premium = 200
    let premium = Investment::calculate_premium(10_000, 100);
    assert_eq!(
        premium, 200,
        "Premium should be 200 for 100% coverage of 10,000"
    );

    // Zero amount
    let premium = Investment::calculate_premium(0, 50);
    assert_eq!(premium, 0, "Premium should be 0 for zero amount");

    // Zero coverage
    let premium = Investment::calculate_premium(10_000, 0);
    assert_eq!(premium, 0, "Premium should be 0 for zero coverage");

    // Minimum premium (small amounts round up to 1)
    let premium = Investment::calculate_premium(100, 1);
    assert_eq!(premium, 1, "Minimum premium should be 1");
}

/// Test: Investment add_insurance
#[test]
fn test_investment_add_insurance() {
    let (env, contract_id) = setup();
    let investor = Address::generate(&env);
    let provider = Address::generate(&env);

    env.as_contract(&contract_id, || {
        let invoice_id = BytesN::from_array(&env, &[1u8; 32]);
        let mut investment = create_test_investment(&env, &invoice_id, &investor);
        assert!(
            !investment.has_active_insurance(),
            "Should not have insurance initially"
        );

        let result = investment.add_insurance(provider.clone(), 50, 100);
        assert!(result.is_ok(), "Adding insurance should succeed");
        assert!(
            investment.has_active_insurance(),
            "Should have active insurance"
        );

        let coverage_amount = result.unwrap();
        assert_eq!(
            coverage_amount, 5_000,
            "Coverage amount should be 50% of 10,000"
        );
    });
}

/// Test: Investment add_insurance - invalid coverage percentage
#[test]
fn test_investment_add_insurance_invalid_coverage() {
    let (env, contract_id) = setup();
    let investor = Address::generate(&env);
    let provider = Address::generate(&env);

    env.as_contract(&contract_id, || {
        let invoice_id = BytesN::from_array(&env, &[1u8; 32]);
        let mut investment = create_test_investment(&env, &invoice_id, &investor);

        // Zero coverage
        let result = investment.add_insurance(provider.clone(), 0, 100);
        assert!(result.is_err(), "Zero coverage should fail");
        assert_eq!(
            result.unwrap_err(),
            QuickLendXError::InvalidCoveragePercentage
        );

        // Over 100% coverage
        let result = investment.add_insurance(provider.clone(), 101, 100);
        assert!(result.is_err(), "Over 100% coverage should fail");
        assert_eq!(
            result.unwrap_err(),
            QuickLendXError::InvalidCoveragePercentage
        );
    });
}

/// Test: Investment add_insurance - invalid premium
#[test]
fn test_investment_add_insurance_invalid_premium() {
    let (env, contract_id) = setup();
    let investor = Address::generate(&env);
    let provider = Address::generate(&env);

    env.as_contract(&contract_id, || {
        let invoice_id = BytesN::from_array(&env, &[1u8; 32]);
        let mut investment = create_test_investment(&env, &invoice_id, &investor);

        let result = investment.add_insurance(provider.clone(), 50, 0);
        assert!(result.is_err(), "Zero premium should fail");
        assert_eq!(result.unwrap_err(), QuickLendXError::InvalidAmount);

        let result = investment.add_insurance(provider.clone(), 50, -100);
        assert!(result.is_err(), "Negative premium should fail");
    });
}

/// Test: Investment add_insurance - already has active insurance
#[test]
fn test_investment_add_insurance_already_active() {
    let (env, contract_id) = setup();
    let investor = Address::generate(&env);
    let provider1 = Address::generate(&env);
    let provider2 = Address::generate(&env);

    env.as_contract(&contract_id, || {
        let invoice_id = BytesN::from_array(&env, &[1u8; 32]);
        let mut investment = create_test_investment(&env, &invoice_id, &investor);

        // Add first insurance
        investment
            .add_insurance(provider1.clone(), 50, 100)
            .unwrap();

        // Try to add second insurance while first is active
        let result = investment.add_insurance(provider2.clone(), 30, 60);
        assert!(
            result.is_err(),
            "Adding insurance while one is active should fail"
        );
        assert_eq!(result.unwrap_err(), QuickLendXError::OperationNotAllowed);
    });
}

/// Test: Investment process_insurance_claim
#[test]
fn test_investment_process_insurance_claim() {
    let (env, contract_id) = setup();
    let investor = Address::generate(&env);
    let provider = Address::generate(&env);

    env.as_contract(&contract_id, || {
        let invoice_id = BytesN::from_array(&env, &[1u8; 32]);
        let mut investment = create_test_investment(&env, &invoice_id, &investor);
        investment
            .add_insurance(provider.clone(), 50, 100)
            .unwrap();

        assert!(
            investment.has_active_insurance(),
            "Should have active insurance"
        );

        // Process claim
        let claim = investment.process_insurance_claim();
        assert!(claim.is_some(), "Should return claim details");

        let (claim_provider, claim_amount) = claim.unwrap();
        assert_eq!(claim_provider, provider, "Claim provider should match");
        assert_eq!(
            claim_amount, 5_000,
            "Claim amount should be coverage amount"
        );

        assert!(
            !investment.has_active_insurance(),
            "Insurance should be deactivated after claim"
        );

        // Second claim should return None
        let second_claim = investment.process_insurance_claim();
        assert!(
            second_claim.is_none(),
            "No claim should be available after processing"
        );
    });
}

// ============================================================================
// CORE TYPE TESTS - ESCROW STATUS
// ============================================================================

/// Test: Escrow status enum values
#[test]
fn test_escrow_status_enum_values() {
    assert_ne!(EscrowStatus::Held, EscrowStatus::Released);
    assert_ne!(EscrowStatus::Released, EscrowStatus::Refunded);
    assert_ne!(EscrowStatus::Held, EscrowStatus::Refunded);
}

// ============================================================================
// CORE TYPE TESTS - DISPUTE STATUS
// ============================================================================

/// Test: Dispute status enum values
#[test]
fn test_dispute_status_enum_values() {
    assert_ne!(DisputeStatus::None, DisputeStatus::Disputed);
    assert_ne!(DisputeStatus::Disputed, DisputeStatus::UnderReview);
    assert_ne!(DisputeStatus::UnderReview, DisputeStatus::Resolved);
}

// ============================================================================
// CORE TYPE TESTS - INVOICE CATEGORY
// ============================================================================

/// Test: Invoice category enum values
#[test]
fn test_invoice_category_enum_values() {
    assert_ne!(InvoiceCategory::Services, InvoiceCategory::Products);
    assert_ne!(InvoiceCategory::Products, InvoiceCategory::Consulting);
    assert_ne!(InvoiceCategory::Consulting, InvoiceCategory::Manufacturing);
    assert_ne!(InvoiceCategory::Manufacturing, InvoiceCategory::Technology);
    assert_ne!(InvoiceCategory::Technology, InvoiceCategory::Healthcare);
    assert_ne!(InvoiceCategory::Healthcare, InvoiceCategory::Other);
}

/// Test: Invoice update_category
#[test]
fn test_invoice_update_category() {
    let (env, contract_id) = setup();
    let business = Address::generate(&env);

    env.as_contract(&contract_id, || {
        let mut invoice = create_test_invoice(&env, &business);
        assert_eq!(invoice.category, InvoiceCategory::Services);

        invoice.update_category(InvoiceCategory::Technology);
        assert_eq!(invoice.category, InvoiceCategory::Technology);
    });
}

// ============================================================================
// EDGE CASE TESTS
// ============================================================================

/// Test: Empty business invoices list
#[test]
fn test_empty_business_invoices() {
    let (env, contract_id) = setup();
    let unknown_business = Address::generate(&env);

    env.as_contract(&contract_id, || {
        let invoices = InvoiceStorage::get_business_invoices(&env, &unknown_business);
        assert_eq!(
            invoices.len(),
            0,
            "Unknown business should have empty invoice list"
        );
    });
}

/// Test: Empty status invoices list
#[test]
fn test_empty_status_invoices() {
    let (env, contract_id) = setup();

    env.as_contract(&contract_id, || {
        let invoices = InvoiceStorage::get_invoices_by_status(&env, &InvoiceStatus::Verified);
        assert_eq!(
            invoices.len(),
            0,
            "Fresh env should have empty verified list"
        );
    });
}

/// Test: Empty bid list for invoice
#[test]
fn test_empty_bid_list() {
    let (env, contract_id) = setup();

    env.as_contract(&contract_id, || {
        let invoice_id = BytesN::from_array(&env, &[99u8; 32]);
        let bids = BidStorage::get_bids_for_invoice(&env, &invoice_id);
        assert_eq!(bids.len(), 0, "Unknown invoice should have empty bid list");
    });
}

/// Test: Empty investor index
#[test]
fn test_empty_investor_index() {
    let (env, contract_id) = setup();
    let unknown_investor = Address::generate(&env);

    env.as_contract(&contract_id, || {
        let bids = BidStorage::get_bids_by_investor_all(&env, &unknown_investor);
        assert_eq!(bids.len(), 0, "Unknown investor should have empty bid list");

        let investments = InvestmentStorage::get_investments_by_investor(&env, &unknown_investor);
        assert_eq!(
            investments.len(),
            0,
            "Unknown investor should have empty investment list"
        );
    });
}

/// Test: Get escrow for invoice with no escrow
#[test]
fn test_escrow_not_found_by_invoice() {
    let (env, contract_id) = setup();

    env.as_contract(&contract_id, || {
        let invoice_id = BytesN::from_array(&env, &[99u8; 32]);
        let escrow = EscrowStorage::get_escrow_by_invoice(&env, &invoice_id);
        assert!(
            escrow.is_none(),
            "Should return None for invoice with no escrow"
        );
    });
}

/// Test: Multiple status transitions - ensure index consistency
#[test]
fn test_status_index_consistency_multiple_invoices() {
    let (env, contract_id) = setup();
    let business = Address::generate(&env);

    env.as_contract(&contract_id, || {
        // Create 5 invoices
        let mut invoice_ids = Vec::new(&env);
        for _ in 0..5 {
            let invoice = create_test_invoice(&env, &business);
            invoice_ids.push_back(invoice.id.clone());
            InvoiceStorage::store_invoice(&env, &invoice);
        }

        // Verify all are pending
        let pending = InvoiceStorage::get_invoices_by_status(&env, &InvoiceStatus::Pending);
        assert_eq!(pending.len(), 5, "All 5 should be pending");

        // Transition first 2 to Verified
        for i in 0..2 {
            let id = invoice_ids.get(i).unwrap();
            InvoiceStorage::remove_from_status_invoices(&env, &InvoiceStatus::Pending, &id);
            InvoiceStorage::add_to_status_invoices(&env, &InvoiceStatus::Verified, &id);
        }

        let pending = InvoiceStorage::get_invoices_by_status(&env, &InvoiceStatus::Pending);
        let verified = InvoiceStorage::get_invoices_by_status(&env, &InvoiceStatus::Verified);
        assert_eq!(pending.len(), 3, "3 should remain pending");
        assert_eq!(verified.len(), 2, "2 should be verified");

        // Verify specific IDs
        assert!(pending.contains(&invoice_ids.get(2).unwrap()));
        assert!(pending.contains(&invoice_ids.get(3).unwrap()));
        assert!(pending.contains(&invoice_ids.get(4).unwrap()));
        assert!(verified.contains(&invoice_ids.get(0).unwrap()));
        assert!(verified.contains(&invoice_ids.get(1).unwrap()));
    });
}

/// Test: Large number of invoices for single business
#[test]
fn test_large_business_invoice_list() {
    let (env, contract_id) = setup();
    let business = Address::generate(&env);

    env.as_contract(&contract_id, || {
        let count = 50u32;
        let mut ids = Vec::new(&env);

        for _ in 0..count {
            let invoice = create_test_invoice(&env, &business);
            ids.push_back(invoice.id.clone());
            InvoiceStorage::store_invoice(&env, &invoice);
        }

        let business_invoices = InvoiceStorage::get_business_invoices(&env, &business);
        assert_eq!(business_invoices.len(), count, "Business should have 50 invoices");

        // Verify all IDs are present
        for id in ids.iter() {
            assert!(
                business_invoices.contains(&id),
                "Business invoices should contain ID"
            );
        }
    });
}

/// Test: Metadata with empty fields
#[test]
fn test_metadata_empty_fields() {
    let (env, contract_id) = setup();
    let business = Address::generate(&env);

    env.as_contract(&contract_id, || {
        let mut invoice = create_test_invoice(&env, &business);
        InvoiceStorage::store_invoice(&env, &invoice);

        // Set metadata with empty customer name and tax ID
        let metadata = InvoiceMetadata {
            customer_name: String::from_str(&env, ""),
            customer_address: String::from_str(&env, "123 Test St"),
            tax_id: String::from_str(&env, ""),
            line_items: Vec::new(&env),
            notes: String::from_str(&env, "Notes"),
        };

        invoice.set_metadata(&env, Some(metadata.clone()));
        InvoiceStorage::update_invoice(&env, &invoice);
        InvoiceStorage::add_metadata_indexes(&env, &invoice);

        // Empty fields should not be indexed
        let by_customer = InvoiceStorage::get_invoices_by_customer(&env, &metadata.customer_name);
        assert!(
            !by_customer.contains(&invoice.id),
            "Empty customer name should not be indexed"
        );

        let by_tax = InvoiceStorage::get_invoices_by_tax_id(&env, &metadata.tax_id);
        assert!(
            !by_tax.contains(&invoice.id),
            "Empty tax ID should not be indexed"
        );
    });
}

/// Test: Invoice serialization round-trip via storage
#[test]
fn test_invoice_serialization_roundtrip() {
    let (env, contract_id) = setup();
    let business = Address::generate(&env);

    env.as_contract(&contract_id, || {
        // Create invoice with all fields populated
        let mut invoice = Invoice::new(
            &env,
            business.clone(),
            25_000,
            Address::generate(&env),
            env.ledger().timestamp() + 86400,
            String::from_str(&env, "Complex invoice for serialization test"),
            InvoiceCategory::Consulting,
            Vec::new(&env),
        );

        // Add tags
        invoice
            .add_tag(&env, String::from_str(&env, "important"))
            .unwrap();
        invoice
            .add_tag(&env, String::from_str(&env, "q4"))
            .unwrap();

        // Set metadata
        let mut line_items = Vec::new(&env);
        line_items.push_back(LineItemRecord(
            String::from_str(&env, "Consulting Hours"),
            50,
            500,
            25_000,
        ));
        invoice.set_metadata(
            &env,
            Some(InvoiceMetadata {
                customer_name: String::from_str(&env, "ACME Corp"),
                customer_address: String::from_str(&env, "456 Business Ave"),
                tax_id: String::from_str(&env, "TAX-99999"),
                line_items,
                notes: String::from_str(&env, "Quarterly consulting engagement"),
            }),
        );

        // Store and retrieve
        InvoiceStorage::store_invoice(&env, &invoice);
        let retrieved = InvoiceStorage::get_invoice(&env, &invoice.id).unwrap();

        // Verify all fields
        assert_eq!(retrieved.id, invoice.id);
        assert_eq!(retrieved.business, business);
        assert_eq!(retrieved.amount, 25_000);
        assert_eq!(retrieved.category, InvoiceCategory::Consulting);
        assert_eq!(retrieved.tags.len(), 2);
        assert!(retrieved.has_tag(String::from_str(&env, "important")));
        assert!(retrieved.has_tag(String::from_str(&env, "q4")));

        let metadata = retrieved.metadata().expect("Metadata should be present");
        assert_eq!(metadata.customer_name, String::from_str(&env, "ACME Corp"));
        assert_eq!(metadata.tax_id, String::from_str(&env, "TAX-99999"));
        assert_eq!(metadata.line_items.len(), 1);
    });
}

/// Test: Bid serialization round-trip via storage
#[test]
fn test_bid_serialization_roundtrip() {
    let (env, contract_id) = setup();
    let investor = Address::generate(&env);

    env.as_contract(&contract_id, || {
        let invoice_id = BytesN::from_array(&env, &[1u8; 32]);

        let bid = Bid {
            bid_id: BidStorage::generate_unique_bid_id(&env),
            invoice_id: invoice_id.clone(),
            investor: investor.clone(),
            bid_amount: 15_000,
            expected_return: 16_500,
            timestamp: env.ledger().timestamp(),
            status: BidStatus::Placed,
            expiration_timestamp: env.ledger().timestamp() + 604800,
        };

        BidStorage::store_bid(&env, &bid);
        let retrieved = BidStorage::get_bid(&env, &bid.bid_id).unwrap();

        assert_eq!(retrieved.bid_id, bid.bid_id);
        assert_eq!(retrieved.invoice_id, invoice_id);
        assert_eq!(retrieved.investor, investor);
        assert_eq!(retrieved.bid_amount, 15_000);
        assert_eq!(retrieved.expected_return, 16_500);
        assert_eq!(retrieved.status, BidStatus::Placed);
    });
}

/// Test: Investment serialization round-trip via storage
#[test]
fn test_investment_serialization_roundtrip() {
    let (env, contract_id) = setup();
    let investor = Address::generate(&env);
    let provider = Address::generate(&env);

    env.as_contract(&contract_id, || {
        let invoice_id = BytesN::from_array(&env, &[1u8; 32]);

        let mut investment = Investment {
            investment_id: InvestmentStorage::generate_unique_investment_id(&env),
            invoice_id: invoice_id.clone(),
            investor: investor.clone(),
            amount: 20_000,
            funded_at: env.ledger().timestamp(),
            status: InvestmentStatus::Active,
            insurance: Vec::new(&env),
        };

        // Add insurance
        investment
            .add_insurance(provider.clone(), 75, 300)
            .unwrap();

        InvestmentStorage::store_investment(&env, &investment);
        let retrieved =
            InvestmentStorage::get_investment(&env, &investment.investment_id).unwrap();

        assert_eq!(retrieved.investment_id, investment.investment_id);
        assert_eq!(retrieved.invoice_id, invoice_id);
        assert_eq!(retrieved.investor, investor);
        assert_eq!(retrieved.amount, 20_000);
        assert_eq!(retrieved.status, InvestmentStatus::Active);
        assert!(retrieved.has_active_insurance());
    });
}

/// Test: Escrow serialization round-trip via storage
#[test]
fn test_escrow_serialization_roundtrip() {
    let (env, contract_id) = setup();
    let investor = Address::generate(&env);
    let business = Address::generate(&env);
    let currency = Address::generate(&env);

    env.as_contract(&contract_id, || {
        let invoice_id = BytesN::from_array(&env, &[1u8; 32]);

        let escrow = Escrow {
            escrow_id: EscrowStorage::generate_unique_escrow_id(&env),
            invoice_id: invoice_id.clone(),
            investor: investor.clone(),
            business: business.clone(),
            amount: 30_000,
            currency: currency.clone(),
            created_at: env.ledger().timestamp(),
            status: EscrowStatus::Held,
        };

        EscrowStorage::store_escrow(&env, &escrow);
        let retrieved = EscrowStorage::get_escrow(&env, &escrow.escrow_id).unwrap();

        assert_eq!(retrieved.escrow_id, escrow.escrow_id);
        assert_eq!(retrieved.invoice_id, invoice_id);
        assert_eq!(retrieved.investor, investor);
        assert_eq!(retrieved.business, business);
        assert_eq!(retrieved.amount, 30_000);
        assert_eq!(retrieved.currency, currency);
        assert_eq!(retrieved.status, EscrowStatus::Held);
    });
}
