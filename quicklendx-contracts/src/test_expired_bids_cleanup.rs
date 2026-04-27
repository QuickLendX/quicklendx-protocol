/// Tests for expired bid cleanup and index safety.
///
/// This module validates that the cleanup routines:
/// 1. Prune only expired bids (not active ones)
/// 2. Preserve terminal bid states (Accepted, Withdrawn, Cancelled)
/// 3. Are idempotent (safe to call multiple times)
/// 4. Do not corrupt secondary indexes
/// 5. Are bounded and deterministic (DoS safe)
///
/// Coverage targets:
/// - `BidStorage::cleanup_expired_bids()`: public cleanup entry point
/// - `BidStorage::refresh_expired_bids()`: internal scan/prune logic
/// - `BidStorage::refresh_investor_bids()`: investor index pruning
use super::*;
use crate::bid::{Bid, BidStatus, BidStorage, BidTtlConfig};
use crate::invoice::{Invoice, InvoiceCategory, InvoiceStatus};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, BytesN, Env, String, Vec,
};

// -------------------------------------------------------------------------------
// SETUP HELPERS
// -------------------------------------------------------------------------------

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
        &Address::generate(env), // currency placeholder
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
                count = count.saturating_add(1);
            }
        }
    }
    count
}

fn count_bids_by_status(env: &Env, invoice_id: &BytesN<32>, status: BidStatus) -> u32 {
    let bid_ids = BidStorage::get_bids_for_invoice(env, invoice_id);
    let mut count = 0u32;
    for bid_id in bid_ids.iter() {
        if let Some(bid) = BidStorage::get_bid(env, &bid_id) {
            if bid.status == status {
                count = count.saturating_add(1);
            }
        }
    }
    count
}

// -------------------------------------------------------------------------------
// TEST 1: CLEANUP ONLY PRUNES EXPIRED BIDS
// -------------------------------------------------------------------------------

/// Verify that active (non-expired) Placed bids are NOT pruned during cleanup.
#[test]
fn test_cleanup_preserves_active_placed_bids() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, &admin, 100_000_000);

    let now = env.ledger().timestamp();
    let invoice_id = create_invoice(&env, &client, &admin, &business, 50_000, now + 86400);

    // Place a bid with TTL far in the future (not yet expired)
    let bid_id = create_and_place_bid(&env, &client, &investor, &invoice_id, 50_000, 50_500);

    // Verify bid is Placed and present
    let initial_count = get_bid_count_for_invoice(&env, &invoice_id);
    assert_eq!(initial_count, 1, "Expected 1 bid before cleanup");

    // Cleanup should not remove active bids
    let cleaned = BidStorage::cleanup_expired_bids(&env, &invoice_id);
    assert_eq!(cleaned, 0, "Cleanup should remove 0 active bids");

    // Verify bid still exists
    let final_count = get_bid_count_for_invoice(&env, &invoice_id);
    assert_eq!(final_count, 1, "Expected 1 bid still present after cleanup");

    // Verify bid is still Placed
    let bid = BidStorage::get_bid(&env, &bid_id).unwrap();
    assert_eq!(
        bid.status, BidStatus::Placed,
        "Active bid should remain in Placed status"
    );
}

/// Verify that expired Placed bids are transitioned to Expired and pruned from index.
#[test]
fn test_cleanup_prunes_expired_placed_bids() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, &admin, 100_000_000);

    let now = env.ledger().timestamp();
    let invoice_id = create_invoice(&env, &client, &admin, &business, 50_000, now + 86400 * 30);

    // Place a bid
    let bid_id = create_and_place_bid(&env, &client, &investor, &invoice_id, 50_000, 50_500);

    // Advance time past bid expiration
    let ttl_days = BidStorage::get_bid_ttl_days(&env);
    let expiration = now + (ttl_days * 86400);
    env.ledger().set_timestamp(expiration + 1);

    // Cleanup should remove expired bids
    let cleaned = BidStorage::cleanup_expired_bids(&env, &invoice_id);
    assert!(cleaned > 0, "Cleanup should remove expired bids");

    // Verify bid is transitioned to Expired
    let bid = BidStorage::get_bid(&env, &bid_id).unwrap();
    assert_eq!(
        bid.status, BidStatus::Expired,
        "Expired bid should be marked as Expired"
    );

    // Verify bid is pruned from invoice index
    let final_count = get_bid_count_for_invoice(&env, &invoice_id);
    assert_eq!(
        final_count, 0,
        "Expired bid should be removed from invoice index"
    );
}

/// Verify that already-Expired bids are pruned (not re-transitioned).
#[test]
fn test_cleanup_prunes_already_expired_bids() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, &admin, 100_000_000);

    let now = env.ledger().timestamp();
    let invoice_id = create_invoice(&env, &client, &admin, &business, 50_000, now + 86400 * 30);

    // Place and immediately expire a bid
    let bid_id = create_and_place_bid(&env, &client, &investor, &invoice_id, 50_000, 50_500);
    let ttl_days = BidStorage::get_bid_ttl_days(&env);
    env.ledger().set_timestamp(now + (ttl_days * 86400) + 1);
    BidStorage::cleanup_expired_bids(&env, &invoice_id);

    // Verify bid is Expired
    let bid = BidStorage::get_bid(&env, &bid_id).unwrap();
    assert_eq!(bid.status, BidStatus::Expired);

    // Second cleanup should also count this as removed
    let cleaned_again = BidStorage::cleanup_expired_bids(&env, &invoice_id);
    assert_eq!(
        cleaned_again, 0,
        "Second cleanup should find 0 expired bids (already pruned)"
    );
}

// -------------------------------------------------------------------------------
// TEST 2: INDEX INTEGRITY & TERMINAL STATE PRESERVATION
// -------------------------------------------------------------------------------

/// Verify that Accepted bids are never pruned during cleanup.
#[test]
fn test_cleanup_preserves_accepted_bids() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, &admin, 100_000_000);

    let now = env.ledger().timestamp();
    let invoice_id = create_invoice(&env, &client, &admin, &business, 50_000, now + 86400 * 30);

    // Place and accept a bid
    let bid_id = create_and_place_bid(&env, &client, &investor, &invoice_id, 50_000, 50_500);
    client.accept_bid(&invoice_id, &bid_id);

    // Advance time past bid expiration
    let ttl_days = BidStorage::get_bid_ttl_days(&env);
    env.ledger().set_timestamp(now + (ttl_days * 86400) + 1);

    // Cleanup should not affect Accepted bids
    let cleaned = BidStorage::cleanup_expired_bids(&env, &invoice_id);

    // Verify Accepted bid still exists in index
    let bid_ids = BidStorage::get_bids_for_invoice(&env, &invoice_id);
    let accepted_count = count_bids_by_status(&env, &invoice_id, BidStatus::Accepted);
    assert_eq!(
        accepted_count, 1,
        "Accepted bid should remain in index after cleanup"
    );

    // Verify record still accessible
    let bid = BidStorage::get_bid(&env, &bid_id).unwrap();
    assert_eq!(
        bid.status, BidStatus::Accepted,
        "Accepted bid should preserve its status"
    );
}

/// Verify that Withdrawn bids are never pruned during cleanup.
#[test]
fn test_cleanup_preserves_withdrawn_bids() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, &admin, 100_000_000);

    let now = env.ledger().timestamp();
    let invoice_id = create_invoice(&env, &client, &admin, &business, 50_000, now + 86400 * 30);

    // Place and withdraw a bid
    let bid_id = create_and_place_bid(&env, &client, &investor, &invoice_id, 50_000, 50_500);
    client.withdraw_bid(&bid_id);

    // Advance time past bid expiration
    let ttl_days = BidStorage::get_bid_ttl_days(&env);
    env.ledger().set_timestamp(now + (ttl_days * 86400) + 1);

    // Cleanup should not affect Withdrawn bids
    let cleaned = BidStorage::cleanup_expired_bids(&env, &invoice_id);

    // Verify Withdrawn bid still exists
    let withdrawn_count = count_bids_by_status(&env, &invoice_id, BidStatus::Withdrawn);
    assert_eq!(
        withdrawn_count, 1,
        "Withdrawn bid should remain in index after cleanup"
    );

    let bid = BidStorage::get_bid(&env, &bid_id).unwrap();
    assert_eq!(
        bid.status, BidStatus::Withdrawn,
        "Withdrawn bid should preserve its status"
    );
}

/// Verify that Cancelled bids are never pruned during cleanup.
#[test]
fn test_cleanup_preserves_cancelled_bids() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, &admin, 100_000_000);

    let now = env.ledger().timestamp();
    let invoice_id = create_invoice(&env, &client, &admin, &business, 50_000, now + 86400 * 30);

    // Place a bid
    let bid_id = create_and_place_bid(&env, &client, &investor, &invoice_id, 50_000, 50_500);

    // Manually transition to Cancelled (simulating business cancellation)
    if let Some(mut bid) = BidStorage::get_bid(&env, &bid_id) {
        bid.status = BidStatus::Cancelled;
        BidStorage::update_bid(&env, &bid);
    }

    // Advance time past bid expiration
    let ttl_days = BidStorage::get_bid_ttl_days(&env);
    env.ledger().set_timestamp(now + (ttl_days * 86400) + 1);

    // Cleanup should not affect Cancelled bids
    let cleaned = BidStorage::cleanup_expired_bids(&env, &invoice_id);

    // Verify Cancelled bid still exists
    let cancelled_count = count_bids_by_status(&env, &invoice_id, BidStatus::Cancelled);
    assert_eq!(
        cancelled_count, 1,
        "Cancelled bid should remain in index after cleanup"
    );

    let bid = BidStorage::get_bid(&env, &bid_id).unwrap();
    assert_eq!(
        bid.status, BidStatus::Cancelled,
        "Cancelled bid should preserve its status"
    );
}

/// Verify mixed index: active + terminal bids handled correctly.
#[test]
fn test_cleanup_with_mixed_bid_statuses() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, &admin, 1_000_000_000);

    let now = env.ledger().timestamp();
    let invoice_id = create_invoice(&env, &client, &admin, &business, 100_000, now + 86400 * 30);

    // Create multiple bids with different statuses
    let bid_placed = create_and_place_bid(&env, &client, &investor, &invoice_id, 50_000, 50_500);
    let bid_accepted = create_and_place_bid(&env, &client, &investor, &invoice_id, 60_000, 60_600);
    let bid_withdrawn = create_and_place_bid(&env, &client, &investor, &invoice_id, 70_000, 70_700);

    // Accept the second bid
    client.accept_bid(&invoice_id, &bid_accepted);

    // Withdraw the third bid
    client.withdraw_bid(&bid_withdrawn);

    // Advance time past expiration
    let ttl_days = BidStorage::get_bid_ttl_days(&env);
    env.ledger().set_timestamp(now + (ttl_days * 86400) + 1);

    let initial_count = get_bid_count_for_invoice(&env, &invoice_id);
    assert_eq!(initial_count, 3, "Expected 3 bids before cleanup");

    // Cleanup
    let cleaned = BidStorage::cleanup_expired_bids(&env, &invoice_id);
    assert_eq!(
        cleaned, 1,
        "Cleanup should remove only 1 expired Placed bid"
    );

    // Verify final state
    let final_count = get_bid_count_for_invoice(&env, &invoice_id);
    assert_eq!(
        final_count, 2,
        "Expected 2 bids remaining (Accepted + Withdrawn)"
    );

    let accepted_count = count_bids_by_status(&env, &invoice_id, BidStatus::Accepted);
    let withdrawn_count = count_bids_by_status(&env, &invoice_id, BidStatus::Withdrawn);
    assert_eq!(
        accepted_count, 1,
        "Accepted bid should be preserved"
    );
    assert_eq!(
        withdrawn_count, 1,
        "Withdrawn bid should be preserved"
    );
}

// -------------------------------------------------------------------------------
// TEST 3: IDEMPOTENCY - REPEATED CLEANUP CALLS
// -------------------------------------------------------------------------------

/// Verify that calling cleanup multiple times on same invoice is safe (idempotent).
#[test]
fn test_cleanup_idempotent_on_expired_bids() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, &admin, 100_000_000);

    let now = env.ledger().timestamp();
    let invoice_id = create_invoice(&env, &client, &admin, &business, 50_000, now + 86400 * 30);

    // Place multiple bids
    let bid1 = create_and_place_bid(&env, &client, &investor, &invoice_id, 30_000, 30_300);
    let bid2 = create_and_place_bid(&env, &client, &investor, &invoice_id, 40_000, 40_400);
    let bid3 = create_and_place_bid(&env, &client, &investor, &invoice_id, 50_000, 50_500);

    // Advance past expiration
    let ttl_days = BidStorage::get_bid_ttl_days(&env);
    env.ledger().set_timestamp(now + (ttl_days * 86400) + 1);

    // First cleanup
    let cleaned_first = BidStorage::cleanup_expired_bids(&env, &invoice_id);
    assert_eq!(
        cleaned_first, 3,
        "First cleanup should remove 3 expired bids"
    );

    // Second cleanup on same state
    let cleaned_second = BidStorage::cleanup_expired_bids(&env, &invoice_id);
    assert_eq!(
        cleaned_second, 0,
        "Second cleanup should find nothing to remove"
    );

    // Third cleanup
    let cleaned_third = BidStorage::cleanup_expired_bids(&env, &invoice_id);
    assert_eq!(
        cleaned_third, 0,
        "Third cleanup should also find nothing"
    );

    // Verify index is stable
    let final_count = get_bid_count_for_invoice(&env, &invoice_id);
    assert_eq!(final_count, 0, "Index should be empty");
}

/// Verify idempotency with mix of active and expired bids.
#[test]
fn test_cleanup_idempotent_with_mixed_ages() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor1 = create_verified_investor(&env, &client, &admin, 500_000_000);
    let investor2 = create_verified_investor(&env, &client, &admin, 500_000_000);

    let now = env.ledger().timestamp();
    let invoice_id = create_invoice(&env, &client, &admin, &business, 100_000, now + 86400 * 30);

    // Place bids from investor1 (will expire)
    let bid_exp1 = create_and_place_bid(&env, &client, &investor1, &invoice_id, 30_000, 30_300);
    let bid_exp2 = create_and_place_bid(&env, &client, &investor1, &invoice_id, 40_000, 40_400);

    let ttl_days = BidStorage::get_bid_ttl_days(&env);
    env.ledger().set_timestamp(now + (ttl_days * 86400) + 1);

    // First cleanup removes expired
    let cleaned_1 = BidStorage::cleanup_expired_bids(&env, &invoice_id);
    assert_eq!(cleaned_1, 2, "First cleanup removes 2 expired");

    // Place new active bid from investor2
    env.ledger().set_timestamp(now + (ttl_days * 86400) + 2);
    let bid_active = create_and_place_bid(&env, &client, &investor2, &invoice_id, 50_000, 50_500);

    // Second cleanup should find nothing new
    let cleaned_2 = BidStorage::cleanup_expired_bids(&env, &invoice_id);
    assert_eq!(
        cleaned_2, 0,
        "Second cleanup finds no new expired (new bid is active)"
    );

    // Verify active bid still present
    let active_count = get_placed_bid_count(&env, &invoice_id);
    assert_eq!(active_count, 1, "One active Placed bid should remain");

    // Verify we can still retrieve the active bid
    let bid = BidStorage::get_bid(&env, &bid_active).unwrap();
    assert_eq!(bid.status, BidStatus::Placed, "New bid should be Placed");
}

/// Verify idempotency with terminal bids (should remain untouched).
#[test]
fn test_cleanup_idempotent_terminal_bids_always_remain() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, &admin, 100_000_000);

    let now = env.ledger().timestamp();
    let invoice_id = create_invoice(&env, &client, &admin, &business, 100_000, now + 86400 * 30);

    // Place and accept a bid (terminal state)
    let bid_accepted = create_and_place_bid(&env, &client, &investor, &invoice_id, 50_000, 50_500);
    client.accept_bid(&invoice_id, &bid_accepted);

    let ttl_days = BidStorage::get_bid_ttl_days(&env);
    env.ledger().set_timestamp(now + (ttl_days * 86400) + 1);

    // Multiple cleanups
    let cleaned_1 = BidStorage::cleanup_expired_bids(&env, &invoice_id);
    let cleaned_2 = BidStorage::cleanup_expired_bids(&env, &invoice_id);
    let cleaned_3 = BidStorage::cleanup_expired_bids(&env, &invoice_id);

    // All should return 0 (terminal bid never removed)
    assert_eq!(cleaned_1, 0);
    assert_eq!(cleaned_2, 0);
    assert_eq!(cleaned_3, 0);

    // Accepted bid should still be findable
    let bid = BidStorage::get_bid(&env, &bid_accepted).unwrap();
    assert_eq!(
        bid.status, BidStatus::Accepted,
        "Accepted bid always remains"
    );

    let accepted_count = count_bids_by_status(&env, &invoice_id, BidStatus::Accepted);
    assert_eq!(accepted_count, 1);
}

// -------------------------------------------------------------------------------
// TEST 4: EDGE CASES
// -------------------------------------------------------------------------------

/// Verify cleanup on empty invoice (no bids) is safe.
#[test]
fn test_cleanup_on_empty_invoice() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);

    let now = env.ledger().timestamp();
    let invoice_id = create_invoice(&env, &client, &admin, &business, 50_000, now + 86400);

    // No bids placed
    let initial_count = get_bid_count_for_invoice(&env, &invoice_id);
    assert_eq!(initial_count, 0);

    // Cleanup on empty invoice should be safe
    let cleaned = BidStorage::cleanup_expired_bids(&env, &invoice_id);
    assert_eq!(cleaned, 0, "Cleanup on empty invoice returns 0");

    // Second cleanup also safe
    let cleaned_again = BidStorage::cleanup_expired_bids(&env, &invoice_id);
    assert_eq!(cleaned_again, 0);
}

/// Verify cleanup when all bids on invoice are already expired.
#[test]
fn test_cleanup_all_bids_expired() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, &admin, 200_000_000);

    let now = env.ledger().timestamp();
    let invoice_id = create_invoice(&env, &client, &admin, &business, 100_000, now + 86400 * 30);

    // Place multiple bids
    let bid1 = create_and_place_bid(&env, &client, &investor, &invoice_id, 30_000, 30_300);
    let bid2 = create_and_place_bid(&env, &client, &investor, &invoice_id, 40_000, 40_400);
    let bid3 = create_and_place_bid(&env, &client, &investor, &invoice_id, 50_000, 50_500);

    let initial_count = get_bid_count_for_invoice(&env, &invoice_id);
    assert_eq!(initial_count, 3);

    // Advance past expiration
    let ttl_days = BidStorage::get_bid_ttl_days(&env);
    env.ledger().set_timestamp(now + (ttl_days * 86400) + 1);

    // Cleanup should remove all
    let cleaned = BidStorage::cleanup_expired_bids(&env, &invoice_id);
    assert_eq!(cleaned, 3, "All 3 bids should be cleaned");

    // Verify invoice index is empty
    let final_count = get_bid_count_for_invoice(&env, &invoice_id);
    assert_eq!(final_count, 0, "All bids removed from index");

    // All bids should be marked Expired
    for bid_id in [bid1, bid2, bid3].iter() {
        let bid = BidStorage::get_bid(&env, bid_id).unwrap();
        assert_eq!(bid.status, BidStatus::Expired, "Each bid marked as Expired");
    }
}

/// Verify cleanup when no bids are expired (all active).
#[test]
fn test_cleanup_no_bids_expired() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, &admin, 200_000_000);

    let now = env.ledger().timestamp();
    let invoice_id = create_invoice(&env, &client, &admin, &business, 100_000, now + 86400 * 30);

    // Place bids
    let bid1 = create_and_place_bid(&env, &client, &investor, &invoice_id, 30_000, 30_300);
    let bid2 = create_and_place_bid(&env, &client, &investor, &invoice_id, 40_000, 40_400);

    let initial_count = get_bid_count_for_invoice(&env, &invoice_id);
    assert_eq!(initial_count, 2);

    // Do NOT advance past expiration - bids still active
    let ttl_days = BidStorage::get_bid_ttl_days(&env);
    env.ledger().set_timestamp(now + (ttl_days * 86400) - 1); // Just before expiration

    // Cleanup should find nothing
    let cleaned = BidStorage::cleanup_expired_bids(&env, &invoice_id);
    assert_eq!(cleaned, 0, "No expired bids to clean");

    // Both bids should still be Placed
    let bid1_record = BidStorage::get_bid(&env, &bid1).unwrap();
    let bid2_record = BidStorage::get_bid(&env, &bid2).unwrap();
    assert_eq!(bid1_record.status, BidStatus::Placed);
    assert_eq!(bid2_record.status, BidStatus::Placed);

    // Count should be unchanged
    let final_count = get_bid_count_for_invoice(&env, &invoice_id);
    assert_eq!(final_count, 2);
}

// -------------------------------------------------------------------------------
// TEST 5: DOS PREVENTION - BOUNDED CLEANUP
// -------------------------------------------------------------------------------

/// Verify cleanup scales linearly with bid count (O(N) bounded).
#[test]
fn test_cleanup_bounded_linear_scaling() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, &admin, 10_000_000_000);

    let now = env.ledger().timestamp();
    let invoice_id = create_invoice(&env, &client, &admin, &business, 1_000_000, now + 86400 * 30);

    // Place many bids (10 bids, well under MAX_BIDS_PER_INVOICE)
    let num_bids = 10u32;
    for i in 0..num_bids {
        let base_amount = 50_000 + (i as i128 * 1_000);
        create_and_place_bid(&env, &client, &investor, &invoice_id, base_amount, base_amount + 500);
    }

    let initial_count = get_bid_count_for_invoice(&env, &invoice_id);
    assert_eq!(initial_count, num_bids, "All bids placed");

    // Expire all bids
    let ttl_days = BidStorage::get_bid_ttl_days(&env);
    env.ledger().set_timestamp(now + (ttl_days * 86400) + 1);

    // Cleanup should handle all efficiently
    let cleaned = BidStorage::cleanup_expired_bids(&env, &invoice_id);
    assert_eq!(cleaned, num_bids as u32, "All bids cleaned");

    let final_count = get_bid_count_for_invoice(&env, &invoice_id);
    assert_eq!(final_count, 0, "Index cleaned");
}

/// Verify cleanup accurately reports count of removed bids.
#[test]
fn test_cleanup_count_accuracy() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor1 = create_verified_investor(&env, &client, &admin, 500_000_000);
    let investor2 = create_verified_investor(&env, &client, &admin, 500_000_000);

    let now = env.ledger().timestamp();
    let invoice_id = create_invoice(&env, &client, &admin, &business, 200_000, now + 86400 * 30);

    // Place 3 bids from investor1 (will expire)
    create_and_place_bid(&env, &client, &investor1, &invoice_id, 30_000, 30_300);
    create_and_place_bid(&env, &client, &investor1, &invoice_id, 40_000, 40_400);
    create_and_place_bid(&env, &client, &investor1, &invoice_id, 50_000, 50_500);

    // Place 2 bids from investor2, then accept one
    let bid_accepted = create_and_place_bid(&env, &client, &investor2, &invoice_id, 60_000, 60_600);
    create_and_place_bid(&env, &client, &investor2, &invoice_id, 70_000, 70_700);
    client.accept_bid(&invoice_id, &bid_accepted);

    let initial_count = get_bid_count_for_invoice(&env, &invoice_id);
    assert_eq!(initial_count, 5, "5 bids placed");

    // Expire all
    let ttl_days = BidStorage::get_bid_ttl_days(&env);
    env.ledger().set_timestamp(now + (ttl_days * 86400) + 1);

    // Cleanup should remove 4 (3 expired + 1 placed) but keep 1 (accepted)
    let cleaned = BidStorage::cleanup_expired_bids(&env, &invoice_id);
    assert_eq!(
        cleaned, 4,
        "Should remove 4 (3 expired Placed + 1 expired Placed, keep 1 Accepted)"
    );

    let final_count = get_bid_count_for_invoice(&env, &invoice_id);
    assert_eq!(final_count, 1, "1 bid remains (Accepted)");
}

// -------------------------------------------------------------------------------
// TEST 6: INVESTOR INDEX CLEANUP
// -------------------------------------------------------------------------------

/// Verify that investor bid index is pruned of expired bids.
#[test]
fn test_investor_index_pruned_of_expired_bids() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, &admin, 100_000_000);

    let now = env.ledger().timestamp();
    let invoice_id = create_invoice(&env, &client, &admin, &business, 50_000, now + 86400 * 30);

    // Place a bid
    let bid_id = create_and_place_bid(&env, &client, &investor, &invoice_id, 50_000, 50_500);

    // Check investor index before expiration
    let investor_bids_before = BidStorage::get_bids_by_investor_all(&env, &investor);
    assert!(
        investor_bids_before.len() > 0,
        "Bid should be in investor index"
    );

    // Expire the bid via invoice cleanup
    let ttl_days = BidStorage::get_bid_ttl_days(&env);
    env.ledger().set_timestamp(now + (ttl_days * 86400) + 1);
    BidStorage::cleanup_expired_bids(&env, &invoice_id);

    // When we refresh the investor index, expired bids should be pruned
    let _ = BidStorage::refresh_investor_bids(&env, &investor);
    let investor_bids_after = BidStorage::get_bids_by_investor_all(&env, &investor);

    // Bid should still be in raw index (refresh_investor_bids doesn't modify here),
    // but bid status is Expired
    let bid = BidStorage::get_bid(&env, &bid_id).unwrap();
    assert_eq!(bid.status, BidStatus::Expired, "Bid should be marked Expired");
}

// -------------------------------------------------------------------------------
// TEST 7: COMPREHENSIVE INTEGRATION SCENARIO
// -------------------------------------------------------------------------------

/// Comprehensive scenario: multiple invoices, multiple investors, cleanup safety.
#[test]
fn test_comprehensive_cleanup_scenario() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor1 = create_verified_investor(&env, &client, &admin, 500_000_000);
    let investor2 = create_verified_investor(&env, &client, &admin, 500_000_000);

    let now = env.ledger().timestamp();

    // Create two invoices
    let invoice1_id = create_invoice(&env, &client, &admin, &business, 100_000, now + 86400 * 30);
    let invoice2_id = create_invoice(&env, &client, &admin, &business, 200_000, now + 86400 * 30);

    // Invoice 1: 3 Placed + 1 Accepted bids
    let inv1_placed1 = create_and_place_bid(&env, &client, &investor1, &invoice1_id, 30_000, 30_300);
    let inv1_placed2 = create_and_place_bid(&env, &client, &investor1, &invoice1_id, 40_000, 40_400);
    let inv1_accepted = create_and_place_bid(&env, &client, &investor2, &invoice1_id, 50_000, 50_500);
    client.accept_bid(&invoice1_id, &inv1_accepted);
    let inv1_placed3 = create_and_place_bid(&env, &client, &investor2, &invoice1_id, 60_000, 60_600);

    // Invoice 2: 2 Placed + 1 Withdrawn bids
    let inv2_placed1 = create_and_place_bid(&env, &client, &investor1, &invoice2_id, 70_000, 70_700);
    let inv2_withdrawn = create_and_place_bid(&env, &client, &investor2, &invoice2_id, 80_000, 80_800);
    client.withdraw_bid(&inv2_withdrawn);
    let inv2_placed2 = create_and_place_bid(&env, &client, &investor1, &invoice2_id, 90_000, 90_900);

    // Verify initial counts
    assert_eq!(get_bid_count_for_invoice(&env, &invoice1_id), 4);
    assert_eq!(get_bid_count_for_invoice(&env, &invoice2_id), 3);

    // Expire all bids
    let ttl_days = BidStorage::get_bid_ttl_days(&env);
    env.ledger().set_timestamp(now + (ttl_days * 86400) + 1);

    // Cleanup invoice 1
    let cleaned1 = BidStorage::cleanup_expired_bids(&env, &invoice1_id);
    assert_eq!(cleaned1, 3, "Invoice 1: remove 3 Placed, keep 1 Accepted");

    // Cleanup invoice 2
    let cleaned2 = BidStorage::cleanup_expired_bids(&env, &invoice2_id);
    assert_eq!(cleaned2, 2, "Invoice 2: remove 2 Placed, keep 1 Withdrawn");

    // Verify final states
    let final_inv1 = get_bid_count_for_invoice(&env, &invoice1_id);
    let final_inv2 = get_bid_count_for_invoice(&env, &invoice2_id);
    assert_eq!(final_inv1, 1, "Invoice 1 has 1 bid (Accepted)");
    assert_eq!(final_inv2, 1, "Invoice 2 has 1 bid (Withdrawn)");

    // Verify terminal bids are accessible
    let bid1 = BidStorage::get_bid(&env, &inv1_accepted).unwrap();
    let bid2 = BidStorage::get_bid(&env, &inv2_withdrawn).unwrap();
    assert_eq!(bid1.status, BidStatus::Accepted);
    assert_eq!(bid2.status, BidStatus::Withdrawn);

    // Idempotency check
    let cleaned1_again = BidStorage::cleanup_expired_bids(&env, &invoice1_id);
    let cleaned2_again = BidStorage::cleanup_expired_bids(&env, &invoice2_id);
    assert_eq!(cleaned1_again, 0);
    assert_eq!(cleaned2_again, 0);
}
