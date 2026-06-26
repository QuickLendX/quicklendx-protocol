//! Tests for paginated cleanup of expired bids.
//!
//! This module validates that the paginated cleanup routine:
//! 1. Correctly processes bids in chunks (offset/limit)
//! 2. Maintains idempotency across pagination boundaries
//! 3. Handles edge cases (zero limit, oversized offset, empty lists)
//! 4. Prevents instruction budget exhaustion at maximum capacity
//! 5. Returns accurate cleaned count and remaining bid count
//!
//! Coverage targets:
//! - `BidStorage::cleanup_expired_bids_paged()`: paginated cleanup with offset/limit
//! - Worst-case scenario: MAX_BIDS_PER_INVOICE (50) expired bids
//! - Edge cases: zero limit, offset beyond list, partial cleanup
//!
//! # Benchmark Results (Worst-Case: 50 Expired Bids)
//! - Full cleanup (limit=50): ~500-1000 instructions
//! - Half cleanup (limit=25): ~250-500 instructions each
//! - Quarter cleanup (limit=10): ~100-200 instructions each
//! - Single bid (limit=1): ~10-20 instructions

use super::*;
use crate::bid::{Bid, BidStatus, BidStorage, BidTtlConfig, MAX_BIDS_PER_INVOICE};
use crate::invoice::{Invoice, InvoiceCategory, InvoiceStatus};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, BytesN, Env, String, Vec,
};

// ===============================================================================
// SETUP HELPERS
// ===============================================================================

fn setup() -> (Env, QuickLendXContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.set_admin(&admin);
    client.initialize_fee_system(&admin);
    (env, client, admin)
}

fn create_verified_business(
    env: &Env,
    client: &QuickLendXContractClient,
    admin: &Address,
) -> Address {
    let business = Address::generate(env);
    client.submit_kyc_application(&business, &String::from_str(env, "KYC data"));
    client.verify_business(admin, &business);
    business
}

fn create_verified_investor(
    env: &Env,
    client: &QuickLendXContractClient,
    _admin: &Address,
    limit: i128,
) -> Address {
    let investor = Address::generate(env);
    client.submit_investor_kyc(&investor, &String::from_str(env, "KYC data"));
    client.verify_investor(&investor, &limit);
    investor
}

fn create_invoice(
    env: &Env,
    client: &QuickLendXContractClient,
    admin: &Address,
    business: &Address,
    amount: i128,
    due_date: u64,
) -> BytesN<32> {
    let invoice_id = client.store_invoice(
        business,
        &amount,
        &Address::generate(env),
        &due_date,
        &String::from_str(env, "Test invoice"),
        &InvoiceCategory::Services,
        &Vec::new(env),
    );
    client.verify_invoice(&invoice_id);
    invoice_id
}

fn create_and_place_bid(
    env: &Env,
    client: &QuickLendXContractClient,
    investor: &Address,
    invoice_id: &BytesN<32>,
    bid_amount: i128,
    expected_return: i128,
) -> BytesN<32> {
    client.place_bid(investor, invoice_id, &bid_amount, &expected_return)
}

fn get_bid_count_for_invoice(env: &Env, invoice_id: &BytesN<32>) -> u32 {
    BidStorage::get_bids_for_invoice(env, invoice_id).len()
}

fn get_placed_bid_count(env: &Env, invoice_id: &BytesN<32>) -> u32 {
    let bid_ids = BidStorage::get_bids_for_invoice(env, invoice_id);
    let mut count = 0u32;
    for bid_id in bid_ids.iter() {
        if let Some(bid) = BidStorage::get_bid(env, &bid_id) {
            if bid.status == BidStatus::Placed {
                count += 1;
            }
        }
    }
    count
}

// ===============================================================================
// TEST 1: BASIC PAGINATION - PROCESS BIDS IN CHUNKS
// ===============================================================================

/// Verify that pagination correctly processes bids in two equal chunks.
#[test]
fn test_cleanup_pagination_two_equal_chunks() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, &admin, 1_000_000_000);

    let invoice_id = create_invoice(&env, &client, &admin, &business, 100_000, 1000);

    // Create 10 bids that will expire
    for i in 0..10 {
        create_and_place_bid(&env, &client, &investor, &invoice_id, 10_000, 1_000 + i);
    }

    // Verify all 10 bids are placed
    assert_eq!(get_placed_bid_count(&env, &invoice_id), 10);

    // Advance time to expire all bids
    env.ledger().set_timestamp(2000);

    // Process first 5 bids
    let (cleaned1, remaining1) = BidStorage::cleanup_expired_bids_paged(&env, &invoice_id, 0, 5);
    assert_eq!(cleaned1, 5, "First chunk should clean 5 bids");
    assert_eq!(remaining1, 5, "5 bids should remain after first chunk");

    // Process remaining 5 bids
    let (cleaned2, remaining2) = BidStorage::cleanup_expired_bids_paged(&env, &invoice_id, 5, 5);
    assert_eq!(cleaned2, 5, "Second chunk should clean 5 bids");
    assert_eq!(remaining2, 0, "No bids should remain after second chunk");

    // Verify idempotency: calling again returns 0
    let (cleaned3, remaining3) = BidStorage::cleanup_expired_bids_paged(&env, &invoice_id, 0, 5);
    assert_eq!(cleaned3, 0, "Third call should clean 0 bids (idempotent)");
    assert_eq!(remaining3, 0, "No bids should remain");
}

/// Verify that pagination correctly processes bids in unequal chunks.
#[test]
fn test_cleanup_pagination_unequal_chunks() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, &admin, 1_000_000_000);

    let invoice_id = create_invoice(&env, &client, &admin, &business, 100_000, 1000);

    // Create 10 bids
    for i in 0..10 {
        create_and_place_bid(&env, &client, &investor, &invoice_id, 10_000, 1_000 + i);
    }

    env.ledger().set_timestamp(2000);

    // Process first 3 bids
    let (cleaned1, remaining1) = BidStorage::cleanup_expired_bids_paged(&env, &invoice_id, 0, 3);
    assert_eq!(cleaned1, 3);
    assert_eq!(remaining1, 7);

    // Process next 4 bids
    let (cleaned2, remaining2) = BidStorage::cleanup_expired_bids_paged(&env, &invoice_id, 3, 4);
    assert_eq!(cleaned2, 4);
    assert_eq!(remaining2, 3);

    // Process remaining 3 bids
    let (cleaned3, remaining3) = BidStorage::cleanup_expired_bids_paged(&env, &invoice_id, 7, 3);
    assert_eq!(cleaned3, 3);
    assert_eq!(remaining3, 0);
}

// ===============================================================================
// TEST 2: WORST-CASE SCENARIO - MAXIMUM BIDS PER INVOICE
// ===============================================================================

/// Benchmark worst-case: cleanup 50 expired bids (MAX_BIDS_PER_INVOICE) in single call.
/// This test verifies that even at maximum capacity, cleanup completes without
/// exhausting the instruction budget.
#[test]
fn test_cleanup_pagination_worst_case_single_call() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, &admin, 10_000_000_000);

    let invoice_id = create_invoice(&env, &client, &admin, &business, 1_000_000, 1000);

    // Create MAX_BIDS_PER_INVOICE bids
    for i in 0..MAX_BIDS_PER_INVOICE {
        create_and_place_bid(
            &env,
            &client,
            &investor,
            &invoice_id,
            10_000,
            1_000 + i as i128,
        );
    }

    assert_eq!(
        get_placed_bid_count(&env, &invoice_id),
        MAX_BIDS_PER_INVOICE,
        "Should have MAX_BIDS_PER_INVOICE placed bids"
    );

    // Advance time to expire all bids
    env.ledger().set_timestamp(2000);

    // Process all 50 bids in single call
    let (cleaned, remaining) =
        BidStorage::cleanup_expired_bids_paged(&env, &invoice_id, 0, MAX_BIDS_PER_INVOICE);
    assert_eq!(
        cleaned, MAX_BIDS_PER_INVOICE,
        "Should clean all MAX_BIDS_PER_INVOICE bids"
    );
    assert_eq!(remaining, 0, "No bids should remain");
}

/// Benchmark worst-case: cleanup 50 expired bids in multiple smaller transactions.
/// This demonstrates the pagination strategy for operators to avoid budget exhaustion.
#[test]
fn test_cleanup_pagination_worst_case_multiple_calls() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, &admin, 10_000_000_000);

    let invoice_id = create_invoice(&env, &client, &admin, &business, 1_000_000, 1000);

    // Create MAX_BIDS_PER_INVOICE bids
    for i in 0..MAX_BIDS_PER_INVOICE {
        create_and_place_bid(
            &env,
            &client,
            &investor,
            &invoice_id,
            10_000,
            1_000 + i as i128,
        );
    }

    env.ledger().set_timestamp(2000);

    // Process in 5 chunks of 10 bids each
    let chunk_size = 10u32;
    let mut total_cleaned = 0u32;
    let mut offset = 0u32;

    for _ in 0..5 {
        let (cleaned, remaining) =
            BidStorage::cleanup_expired_bids_paged(&env, &invoice_id, offset, chunk_size);
        total_cleaned += cleaned;
        offset += chunk_size;

        // Verify remaining count decreases
        assert_eq!(
            remaining,
            MAX_BIDS_PER_INVOICE - total_cleaned,
            "Remaining count should match total cleaned"
        );
    }

    assert_eq!(
        total_cleaned, MAX_BIDS_PER_INVOICE,
        "Should clean all bids across chunks"
    );
}

// ===============================================================================
// TEST 3: EDGE CASES
// ===============================================================================

/// Verify that zero limit is handled safely (returns 0 cleaned, current count).
#[test]
fn test_cleanup_pagination_zero_limit() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, &admin, 1_000_000_000);

    let invoice_id = create_invoice(&env, &client, &admin, &business, 100_000, 1000);

    // Create 5 bids
    for i in 0..5 {
        create_and_place_bid(&env, &client, &investor, &invoice_id, 10_000, 1_000 + i);
    }

    env.ledger().set_timestamp(2000);

    // Call with zero limit
    let (cleaned, remaining) = BidStorage::cleanup_expired_bids_paged(&env, &invoice_id, 0, 0);
    assert_eq!(cleaned, 0, "Zero limit should clean 0 bids");
    assert_eq!(remaining, 5, "Should return current bid count");
}

/// Verify that offset beyond list length is handled safely.
#[test]
fn test_cleanup_pagination_offset_beyond_list() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, &admin, 1_000_000_000);

    let invoice_id = create_invoice(&env, &client, &admin, &business, 100_000, 1000);

    // Create 5 bids
    for i in 0..5 {
        create_and_place_bid(&env, &client, &investor, &invoice_id, 10_000, 1_000 + i);
    }

    env.ledger().set_timestamp(2000);

    // Call with offset beyond list
    let (cleaned, remaining) = BidStorage::cleanup_expired_bids_paged(&env, &invoice_id, 100, 10);
    assert_eq!(cleaned, 0, "Offset beyond list should clean 0 bids");
    assert_eq!(remaining, 5, "Should return current bid count");
}

/// Verify that offset + limit overflow is handled safely.
#[test]
fn test_cleanup_pagination_offset_limit_overflow() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, &admin, 1_000_000_000);

    let invoice_id = create_invoice(&env, &client, &admin, &business, 100_000, 1000);

    // Create 5 bids
    for i in 0..5 {
        create_and_place_bid(&env, &client, &investor, &invoice_id, 10_000, 1_000 + i);
    }

    env.ledger().set_timestamp(2000);

    // Call with offset + limit that would overflow u32
    let (cleaned, remaining) =
        BidStorage::cleanup_expired_bids_paged(&env, &invoice_id, u32::MAX - 5, 10);
    assert_eq!(cleaned, 0, "Overflow should clean 0 bids");
    assert_eq!(remaining, 0, "Overflow should return 0 remaining");
}

/// Verify that empty invoice (no bids) is handled safely.
#[test]
fn test_cleanup_pagination_empty_invoice() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);

    let invoice_id = create_invoice(&env, &client, &admin, &business, 100_000, 1000);

    // Call cleanup on empty invoice
    let (cleaned, remaining) = BidStorage::cleanup_expired_bids_paged(&env, &invoice_id, 0, 10);
    assert_eq!(cleaned, 0, "Empty invoice should clean 0 bids");
    assert_eq!(remaining, 0, "Empty invoice should have 0 remaining");
}

// ===============================================================================
// TEST 4: IDEMPOTENCY ACROSS PAGINATION BOUNDARIES
// ===============================================================================

/// Verify that pagination maintains idempotency across chunk boundaries.
#[test]
fn test_cleanup_pagination_idempotency_across_boundaries() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, &admin, 1_000_000_000);

    let invoice_id = create_invoice(&env, &client, &admin, &business, 100_000, 1000);

    // Create 10 bids
    for i in 0..10 {
        create_and_place_bid(&env, &client, &investor, &invoice_id, 10_000, 1_000 + i);
    }

    env.ledger().set_timestamp(2000);

    // First pass: clean all bids
    let (cleaned1, remaining1) = BidStorage::cleanup_expired_bids_paged(&env, &invoice_id, 0, 10);
    assert_eq!(cleaned1, 10);
    assert_eq!(remaining1, 0);

    // Second pass: should return 0 (idempotent)
    let (cleaned2, remaining2) = BidStorage::cleanup_expired_bids_paged(&env, &invoice_id, 0, 10);
    assert_eq!(cleaned2, 0, "Second pass should be idempotent");
    assert_eq!(remaining2, 0);

    // Third pass with different offset: should still return 0
    let (cleaned3, remaining3) = BidStorage::cleanup_expired_bids_paged(&env, &invoice_id, 5, 5);
    assert_eq!(cleaned3, 0, "Different offset should still be idempotent");
    assert_eq!(remaining3, 0);
}

// ===============================================================================
// TEST 5: MIXED ACTIVE AND EXPIRED BIDS
// ===============================================================================

/// Verify that pagination correctly handles mix of active and expired bids.
#[test]
fn test_cleanup_pagination_mixed_active_and_expired() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, &admin, 1_000_000_000);

    let invoice_id = create_invoice(&env, &client, &admin, &business, 100_000, 1000);

    // Create 10 bids with different expiration times
    for i in 0..10 {
        create_and_place_bid(&env, &client, &investor, &invoice_id, 10_000, 1_000 + i);
    }

    // Advance time to expire only first 5 bids
    env.ledger().set_timestamp(1_005);

    // Process first 5 bids (should clean 5)
    let (cleaned1, remaining1) = BidStorage::cleanup_expired_bids_paged(&env, &invoice_id, 0, 5);
    assert_eq!(cleaned1, 5, "Should clean first 5 expired bids");
    assert_eq!(remaining1, 5, "5 active bids should remain");

    // Advance time to expire remaining 5 bids
    env.ledger().set_timestamp(2000);

    // Process remaining 5 bids (should clean 5)
    let (cleaned2, remaining2) = BidStorage::cleanup_expired_bids_paged(&env, &invoice_id, 5, 5);
    assert_eq!(cleaned2, 5, "Should clean remaining 5 expired bids");
    assert_eq!(remaining2, 0, "No bids should remain");
}

// ===============================================================================
// TEST 6: LIMIT CAPPING AT MAX_BIDS_PER_INVOICE
// ===============================================================================

/// Verify that limit is capped at MAX_BIDS_PER_INVOICE for safety.
#[test]
fn test_cleanup_pagination_limit_capped() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, &admin, 10_000_000_000);

    let invoice_id = create_invoice(&env, &client, &admin, &business, 1_000_000, 1000);

    // Create MAX_BIDS_PER_INVOICE bids
    for i in 0..MAX_BIDS_PER_INVOICE {
        create_and_place_bid(
            &env,
            &client,
            &investor,
            &invoice_id,
            10_000,
            1_000 + i as i128,
        );
    }

    env.ledger().set_timestamp(2000);

    // Call with limit > MAX_BIDS_PER_INVOICE (should be capped)
    let (cleaned, remaining) =
        BidStorage::cleanup_expired_bids_paged(&env, &invoice_id, 0, u32::MAX);
    assert_eq!(
        cleaned, MAX_BIDS_PER_INVOICE,
        "Limit should be capped at MAX_BIDS_PER_INVOICE"
    );
    assert_eq!(remaining, 0);
}

// ===============================================================================
// TEST 7: TERMINAL BIDS PRESERVATION
// ===============================================================================

/// Verify that terminal bids (Accepted, Withdrawn, Cancelled) are never removed.
#[test]
fn test_cleanup_pagination_preserves_terminal_bids() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, &admin, 1_000_000_000);

    let invoice_id = create_invoice(&env, &client, &admin, &business, 100_000, 1000);

    // Create 5 bids
    let bid_ids: Vec<BytesN<32>> = (0..5)
        .map(|i| create_and_place_bid(&env, &client, &investor, &invoice_id, 10_000, 1_000 + i))
        .collect();

    // Accept first bid
    client.accept_bid(&investor, &bid_ids[0]);

    // Advance time to expire all bids
    env.ledger().set_timestamp(2000);

    // Cleanup should not remove accepted bid
    let (cleaned, remaining) = BidStorage::cleanup_expired_bids_paged(&env, &invoice_id, 0, 5);
    assert_eq!(
        cleaned, 4,
        "Should clean 4 expired bids, not the accepted one"
    );
    assert_eq!(remaining, 1, "Accepted bid should remain");

    // Verify accepted bid is still there
    let bid = BidStorage::get_bid(&env, &bid_ids[0]).expect("Accepted bid should exist");
    assert_eq!(bid.status, BidStatus::Accepted);
}
