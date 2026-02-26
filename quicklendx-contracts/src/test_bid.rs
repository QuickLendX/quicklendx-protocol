/// Minimized test suite for bid functionality
/// Coverage: placement/withdrawal, invoice status gating, indexing/query correctness
///
/// Test Categories (Core Only):
/// 1. Status Gating - verify bids only work on verified invoices
/// 2. Withdrawal - authorize only bid owner can withdraw
/// 3. Indexing - multiple bids properly indexed and queryable
/// 4. Ranking - profit-based bid comparison works correctly
use super::*;
use crate::bid::BidStatus;
use crate::invoice::InvoiceCategory;
use crate::protocol_limits::compute_min_bid_amount;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, BytesN, Env, String, Vec,
};

// Helper: Setup contract with admin
fn setup() -> (Env, QuickLendXContractClient<'static>) {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    (env, client)
}

// Helper: Create verified investor - using same pattern as test.rs
fn add_verified_investor(env: &Env, client: &QuickLendXContractClient, limit: i128) -> Address {
    let investor = Address::generate(env);
    client.submit_investor_kyc(&investor, &String::from_str(env, "KYC"));
    client.verify_investor(&investor, &limit);
    investor
}

// Helper: Create verified invoice
fn create_verified_invoice(
    env: &Env,
    client: &QuickLendXContractClient,
    admin: &Address,
    business: &Address,
    amount: i128,
) -> BytesN<32> {
    let currency = Address::generate(env);
    let due_date = env.ledger().timestamp() + 86400;
    let _ = client.add_currency(admin, &currency);

    let invoice_id = client.store_invoice(
        business,
        &amount,
        &currency,
        &due_date,
        &String::from_str(env, "Invoice"),
        &InvoiceCategory::Services,
        &Vec::new(env),
    );

    let _ = client.try_verify_invoice(&invoice_id);
    invoice_id
}

// ============================================================================
// Category 1: Status Gating - Invoice Verification Required
// ============================================================================

/// Core Test: Bid on pending (non-verified) invoice fails
#[test]
fn test_bid_placement_non_verified_invoice_fails() {
    let (env, client) = setup();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let _ = client.set_admin(&admin);
    let investor = add_verified_investor(&env, &client, 100_000);
    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    let _ = client.add_currency(&admin, &currency);

    // Create pending invoice (not verified)
    let invoice_id = client.store_invoice(
        &business,
        &10_000,
        &currency,
        &(env.ledger().timestamp() + 86400),
        &String::from_str(&env, "Pending"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    // Attempt bid on pending invoice should fail
    let result = client.try_place_bid(&investor, &invoice_id, &5_000, &6_000);
    assert!(result.is_err(), "Bid on pending invoice must fail");
}

/// Core Test: Bid on verified invoice succeeds
#[test]
fn test_bid_placement_verified_invoice_succeeds() {
    let (env, client) = setup();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let _ = client.set_admin(&admin);
    let investor = add_verified_investor(&env, &client, 100_000);
    let business = Address::generate(&env);

    let invoice_id = create_verified_invoice(&env, &client, &admin, &business, 10_000);

    // Bid on verified invoice should succeed
    let result = client.try_place_bid(&investor, &invoice_id, &5_000, &6_000);
    assert!(result.is_ok(), "Bid on verified invoice must succeed");

    let bid_id = result.unwrap().unwrap();
    let bid = client.get_bid(&bid_id);
    assert!(bid.is_some());
    assert_eq!(bid.unwrap().status, BidStatus::Placed);
}

/// Core Test: Minimum bid amount enforced (absolute floor + percentage of invoice)
#[test]
fn test_bid_minimum_amount_enforced() {
    let (env, client) = setup();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let _ = client.set_admin(&admin);
    let investor = add_verified_investor(&env, &client, 1_000_000);
    let business = Address::generate(&env);

    let invoice_amount = 200_000;
    let invoice_id = create_verified_invoice(&env, &client, &admin, &business, invoice_amount);

    let min_bid = compute_min_bid_amount(
        invoice_amount,
        &crate::protocol_limits::ProtocolLimits {
            min_invoice_amount: 1_000_000,
            min_bid_amount: 100,
            min_bid_bps: 100,
            max_due_date_days: 365,
            grace_period_seconds: 86400,
        },
    );
    let below_min = min_bid.saturating_sub(1);

    let result = client.try_place_bid(&investor, &invoice_id, &below_min, &(min_bid + 100));
    assert!(result.is_err(), "Bid below minimum must fail");

    let result = client.try_place_bid(&investor, &invoice_id, &min_bid, &(min_bid + 100));
    assert!(result.is_ok(), "Bid at minimum must succeed");
}

/// Core Test: Investment limit enforced
#[test]
fn test_bid_placement_respects_investment_limit() {
    let (env, client) = setup();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let _ = client.set_admin(&admin);
    let investor = add_verified_investor(&env, &client, 1_000); // Low limit
    let business = Address::generate(&env);

    let invoice_id = create_verified_invoice(&env, &client, &admin, &business, 10_000);

    // Bid exceeding limit should fail
    let result = client.try_place_bid(&investor, &invoice_id, &2_000, &3_000);
    assert!(result.is_err(), "Bid exceeding investment limit must fail");
}

// ============================================================================
// Category 2: Withdrawal - Authorization and State Constraints
// ============================================================================

/// Core Test: Bid owner can withdraw own bid
#[test]
fn test_bid_withdrawal_by_owner_succeeds() {
    let (env, client) = setup();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let _ = client.set_admin(&admin);
    let investor = add_verified_investor(&env, &client, 100_000);
    let business = Address::generate(&env);

    let invoice_id = create_verified_invoice(&env, &client, &admin, &business, 10_000);

    // Place bid
    let bid_id = client.place_bid(&investor, &invoice_id, &5_000, &6_000);

    // Withdraw should succeed
    let result = client.try_withdraw_bid(&bid_id);
    assert!(result.is_ok(), "Owner bid withdrawal must succeed");

    // Verify withdrawn
    let bid = client.get_bid(&bid_id);
    assert!(bid.is_some());
    assert_eq!(bid.unwrap().status, BidStatus::Withdrawn);
}

/// Core Test: Only Placed bids can be withdrawn
#[test]
fn test_bid_withdrawal_only_placed_bids() {
    let (env, client) = setup();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let _ = client.set_admin(&admin);
    let investor = add_verified_investor(&env, &client, 100_000);
    let business = Address::generate(&env);

    let invoice_id = create_verified_invoice(&env, &client, &admin, &business, 10_000);

    let bid_id = client.place_bid(&investor, &invoice_id, &5_000, &6_000);

    // Withdraw once
    let _ = client.try_withdraw_bid(&bid_id);

    // Second withdraw attempt should fail
    let result = client.try_withdraw_bid(&bid_id);
    assert!(result.is_err(), "Cannot withdraw non-Placed bid");
}

// ============================================================================
// Category 3: Indexing & Query Correctness - Multiple Bids
// ============================================================================

/// Core Test: Multiple bids indexed and queryable by status
#[test]
fn test_multiple_bids_indexing_and_query() {
    let (env, client) = setup();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let _ = client.set_admin(&admin);
    let investor1 = add_verified_investor(&env, &client, 100_000);
    let investor2 = add_verified_investor(&env, &client, 100_000);
    let investor3 = add_verified_investor(&env, &client, 100_000);
    let business = Address::generate(&env);

    let invoice_id = create_verified_invoice(&env, &client, &admin, &business, 100_000);

    // Place 3 bids
    let bid_id_1 = client.place_bid(&investor1, &invoice_id, &10_000, &12_000);
    let bid_id_2 = client.place_bid(&investor2, &invoice_id, &15_000, &18_000);
    let bid_id_3 = client.place_bid(&investor3, &invoice_id, &20_000, &24_000);

    // Query placed bids
    let placed_bids = client.get_bids_by_status(&invoice_id, &BidStatus::Placed);
    assert_eq!(placed_bids.len(), 3, "Should have 3 placed bids");

    // Verify all bid IDs present
    let found_1 = placed_bids.iter().any(|b| b.bid_id == bid_id_1);
    let found_2 = placed_bids.iter().any(|b| b.bid_id == bid_id_2);
    let found_3 = placed_bids.iter().any(|b| b.bid_id == bid_id_3);
    assert!(found_1 && found_2 && found_3, "All bid IDs must be indexed");

    // Withdraw one and verify status filtering
    let _ = client.try_withdraw_bid(&bid_id_1);
    let placed_after = client.get_bids_by_status(&invoice_id, &BidStatus::Placed);
    assert_eq!(
        placed_after.len(),
        2,
        "Should have 2 placed bids after withdrawal"
    );

// ============================================================================
// Bid TTL configuration tests
// ============================================================================

#[test]
fn test_default_bid_ttl_used_in_place_bid() {
    let (env, client) = setup();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let _ = client.set_admin(&admin);

    let investor = add_verified_investor(&env, &client, 100_000);
    let business = Address::generate(&env);
    let invoice_id = create_verified_invoice(&env, &client, &admin, &business, 10_000);

    let current_ts = env.ledger().timestamp();
    let bid_id = client.place_bid(&investor, &invoice_id, &5_000, &6_000);
    let bid = client.get_bid(&bid_id).unwrap();

    let expected = current_ts + (7u64 * 86400u64);
    assert_eq!(bid.expiration_timestamp, expected);
}

#[test]
fn test_admin_can_update_ttl_and_bid_uses_new_value() {
    let (env, client) = setup();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let _ = client.set_admin(&admin);

    // Update TTL to 14 days
    let _ = client.set_bid_ttl_days(&14u64);

    let investor = add_verified_investor(&env, &client, 100_000);
    let business = Address::generate(&env);
    let invoice_id = create_verified_invoice(&env, &client, &admin, &business, 10_000);

    let current_ts = env.ledger().timestamp();
    let bid_id = client.place_bid(&investor, &invoice_id, &5_000, &6_000);
    let bid = client.get_bid(&bid_id).unwrap();

    let expected = current_ts + (14u64 * 86400u64);
    assert_eq!(bid.expiration_timestamp, expected);
}

#[test]
fn test_set_bid_ttl_bounds_enforced() {
    let (env, client) = setup();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let _ = client.set_admin(&admin);

    // Too small
    let result = client.try_set_bid_ttl_days(&0u64);
    assert!(result.is_err());

    // Too large
    let result = client.try_set_bid_ttl_days(&31u64);
    assert!(result.is_err());
}

    let withdrawn_bids = client.get_bids_by_status(&invoice_id, &BidStatus::Withdrawn);
    assert_eq!(withdrawn_bids.len(), 1, "Should have 1 withdrawn bid");
}

/// Core Test: Query by investor works correctly
#[test]
fn test_query_bids_by_investor() {
    let (env, client) = setup();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let _ = client.set_admin(&admin);
    let investor1 = add_verified_investor(&env, &client, 100_000);
    let investor2 = add_verified_investor(&env, &client, 100_000);
    let business = Address::generate(&env);

    let invoice_id_1 = create_verified_invoice(&env, &client, &admin, &business, 100_000);
    let invoice_id_2 = create_verified_invoice(&env, &client, &admin, &business, 100_000);

    // Investor1 places 2 bids on different invoices
    let _bid_1a = client.place_bid(&investor1, &invoice_id_1, &10_000, &12_000);
    let _bid_1b = client.place_bid(&investor1, &invoice_id_2, &15_000, &18_000);

    // Investor2 places 1 bid
    let _bid_2 = client.place_bid(&investor2, &invoice_id_1, &20_000, &24_000);

    // Query investor1 bids on invoice 1
    let inv1_bids = client.get_bids_by_investor(&invoice_id_1, &investor1);
    assert_eq!(
        inv1_bids.len(),
        1,
        "Investor1 should have 1 bid on invoice 1"
    );

    // Query investor2 bids on invoice 1
    let inv2_bids = client.get_bids_by_investor(&invoice_id_1, &investor2);
    assert_eq!(
        inv2_bids.len(),
        1,
        "Investor2 should have 1 bid on invoice 1"
    );
}

// ============================================================================
// Category 4: Bid Ranking - Profit-Based Comparison Logic
// ============================================================================

/// Core Test: Best bid selection based on profit margin
#[test]
fn test_bid_ranking_by_profit() {
    let (env, client) = setup();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let _ = client.set_admin(&admin);
    let investor1 = add_verified_investor(&env, &client, 100_000);
    let investor2 = add_verified_investor(&env, &client, 100_000);
    let investor3 = add_verified_investor(&env, &client, 100_000);
    let business = Address::generate(&env);

    let invoice_id = create_verified_invoice(&env, &client, &admin, &business, 100_000);

    // Place bids with different profit margins
    // investor1: profit = 12k - 10k = 2k
    let _bid_1 = client.place_bid(&investor1, &invoice_id, &10_000, &12_000);

    // investor2: profit = 18k - 15k = 3k (highest)
    let _bid_2 = client.place_bid(&investor2, &invoice_id, &15_000, &18_000);

    // investor3: profit = 13k - 12k = 1k (lowest)
    let _bid_3 = client.place_bid(&investor3, &invoice_id, &12_000, &13_000);

    // Best bid should be investor2 (highest profit)
    let best_bid = client.get_best_bid(&invoice_id);
    assert!(best_bid.is_some());
    assert_eq!(
        best_bid.unwrap().investor,
        investor2,
        "Best bid must have highest profit"
    );

    // Ranked bids should order by profit descending
    let ranked = client.get_ranked_bids(&invoice_id);
    assert_eq!(ranked.len(), 3, "Should have 3 ranked bids");
    assert_eq!(
        ranked.get(0).unwrap().investor,
        investor2,
        "Rank 1: investor2 (profit 3k)"
    );
    assert_eq!(
        ranked.get(1).unwrap().investor,
        investor1,
        "Rank 2: investor1 (profit 2k)"
    );
    assert_eq!(
        ranked.get(2).unwrap().investor,
        investor3,
        "Rank 3: investor3 (profit 1k)"
    );
}

/// Core Test: Best bid ignores withdrawn bids
#[test]
fn test_best_bid_excludes_withdrawn() {
    let (env, client) = setup();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let _ = client.set_admin(&admin);
    let investor1 = add_verified_investor(&env, &client, 100_000);
    let investor2 = add_verified_investor(&env, &client, 100_000);
    let business = Address::generate(&env);

    let invoice_id = create_verified_invoice(&env, &client, &admin, &business, 100_000);

    // investor1: profit = 2k
    let _bid_1 = client.place_bid(&investor1, &invoice_id, &10_000, &12_000);

    // investor2: profit = 10k (best initially)
    let bid_2 = client.place_bid(&investor2, &invoice_id, &15_000, &25_000);

    // Withdraw best bid
    let _ = client.try_withdraw_bid(&bid_2);

    // Best bid should now be investor1
    let best = client.get_best_bid(&invoice_id);
    assert!(best.is_some());
    assert_eq!(
        best.unwrap().investor,
        investor1,
        "Best bid must skip withdrawn bids"
    );
}

/// Core Test: Bid expiration cleanup
#[test]
fn test_bid_expiration_and_cleanup() {
    let (env, client) = setup();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let _ = client.set_admin(&admin);
    let investor = add_verified_investor(&env, &client, 100_000);
    let business = Address::generate(&env);

    let invoice_id = create_verified_invoice(&env, &client, &admin, &business, 10_000);

    // Place bid
    let bid_id = client.place_bid(&investor, &invoice_id, &5_000, &6_000);

    let placed = client.get_bids_by_status(&invoice_id, &BidStatus::Placed);
    assert_eq!(placed.len(), 1, "Should have 1 placed bid");

    // Advance time past expiration (7 days = 604800 seconds)
    env.ledger()
        .set_timestamp(env.ledger().timestamp() + 604800 + 1);

    // Query to trigger cleanup
    let placed_after = client.get_bids_by_status(&invoice_id, &BidStatus::Placed);
    assert_eq!(
        placed_after.len(),
        0,
        "Placed bids should be empty after expiration"
    );

    // Bid should be marked expired
    let bid = client.get_bid(&bid_id);
    assert!(bid.is_some());
    assert_eq!(
        bid.unwrap().status,
        BidStatus::Expired,
        "Bid must be marked expired"
    );
}

// ============================================================================
// Category 6: Bid Expiration - Default TTL and Cleanup
// ============================================================================

/// Test: Bid uses default TTL (7 days) when placed
#[test]
fn test_bid_default_ttl_seven_days() {
    let (env, client) = setup();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let _ = client.set_admin(&admin);
    let investor = add_verified_investor(&env, &client, 100_000);
    let business = Address::generate(&env);

    let invoice_id = create_verified_invoice(&env, &client, &admin, &business, 10_000);
    
    let initial_timestamp = env.ledger().timestamp();
    let bid_id = client.place_bid(&investor, &invoice_id, &5_000, &6_000);
    
    let bid = client.get_bid(&bid_id).unwrap();
    let expected_expiration = initial_timestamp + (7 * 24 * 60 * 60); // 7 days in seconds
    
    assert_eq!(
        bid.expiration_timestamp, expected_expiration,
        "Bid expiration should be 7 days from placement"
    );
}

/// Test: cleanup_expired_bids returns count of removed bids
#[test]
fn test_cleanup_expired_bids_returns_count() {
    let (env, client) = setup();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let _ = client.set_admin(&admin);
    let investor1 = add_verified_investor(&env, &client, 100_000);
    let investor2 = add_verified_investor(&env, &client, 100_000);
    let investor3 = add_verified_investor(&env, &client, 100_000);
    let business = Address::generate(&env);

    let invoice_id = create_verified_invoice(&env, &client, &admin, &business, 100_000);
    
    // Place 3 bids
    let bid_1 = client.place_bid(&investor1, &invoice_id, &10_000, &12_000);
    let bid_2 = client.place_bid(&investor2, &invoice_id, &15_000, &18_000);
    let bid_3 = client.place_bid(&investor3, &invoice_id, &20_000, &24_000);
    
    // Advance time past expiration
    env.ledger().set_timestamp(env.ledger().timestamp() + 604800 + 1);
    
    // Cleanup should return count of 3
    let removed_count = client.cleanup_expired_bids(&invoice_id);
    assert_eq!(removed_count, 3, "Should remove all 3 expired bids");
    
    // Verify all bids are marked expired (check individual bid records)
    let bid_1_status = client.get_bid(&bid_1).unwrap();
    assert_eq!(bid_1_status.status, BidStatus::Expired, "Bid 1 should be expired");
    
    let bid_2_status = client.get_bid(&bid_2).unwrap();
    assert_eq!(bid_2_status.status, BidStatus::Expired, "Bid 2 should be expired");
    
    let bid_3_status = client.get_bid(&bid_3).unwrap();
    assert_eq!(bid_3_status.status, BidStatus::Expired, "Bid 3 should be expired");
    
    // Verify no bids are in Placed status
    let placed_bids = client.get_bids_by_status(&invoice_id, &BidStatus::Placed);
    assert_eq!(placed_bids.len(), 0, "No bids should be in Placed status");
}

/// Test: get_ranked_bids excludes expired bids
#[test]
fn test_get_ranked_bids_excludes_expired() {
    let (env, client) = setup();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let _ = client.set_admin(&admin);
    let investor1 = add_verified_investor(&env, &client, 100_000);
    let investor2 = add_verified_investor(&env, &client, 100_000);
    let investor3 = add_verified_investor(&env, &client, 100_000);
    let business = Address::generate(&env);

    let invoice_id = create_verified_invoice(&env, &client, &admin, &business, 100_000);
    
    // Place 3 bids with different profits
    // investor1: profit = 2k
    let _bid_1 = client.place_bid(&investor1, &invoice_id, &10_000, &12_000);
    // investor2: profit = 3k (best)
    let _bid_2 = client.place_bid(&investor2, &invoice_id, &15_000, &18_000);
    // investor3: profit = 1k
    let _bid_3 = client.place_bid(&investor3, &invoice_id, &12_000, &13_000);
    
    // Verify all 3 bids are ranked
    let ranked_before = client.get_ranked_bids(&invoice_id);
    assert_eq!(ranked_before.len(), 3, "Should have 3 ranked bids initially");
    
    // Advance time past expiration
    env.ledger().set_timestamp(env.ledger().timestamp() + 604800 + 1);
    
    // get_ranked_bids should trigger cleanup and exclude expired bids
    let ranked_after = client.get_ranked_bids(&invoice_id);
    assert_eq!(ranked_after.len(), 0, "Ranked bids should be empty after expiration");
}

/// Test: get_best_bid excludes expired bids
#[test]
fn test_get_best_bid_excludes_expired() {
    let (env, client) = setup();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let _ = client.set_admin(&admin);
    let investor1 = add_verified_investor(&env, &client, 100_000);
    let investor2 = add_verified_investor(&env, &client, 100_000);
    let business = Address::generate(&env);

    let invoice_id = create_verified_invoice(&env, &client, &admin, &business, 100_000);
    
    // investor1: profit = 2k
    let _bid_1 = client.place_bid(&investor1, &invoice_id, &10_000, &12_000);
    // investor2: profit = 10k (best)
    let _bid_2 = client.place_bid(&investor2, &invoice_id, &15_000, &25_000);
    
    // Verify best bid is investor2
    let best_before = client.get_best_bid(&invoice_id);
    assert!(best_before.is_some());
    assert_eq!(best_before.unwrap().investor, investor2, "Best bid should be investor2");
    
    // Advance time past expiration
    env.ledger().set_timestamp(env.ledger().timestamp() + 604800 + 1);
    
    // get_best_bid should return None after all bids expire
    let best_after = client.get_best_bid(&invoice_id);
    assert!(best_after.is_none(), "Best bid should be None after all bids expire");
}

/// Test: place_bid cleans up expired bids before placing new bid
#[test]
fn test_place_bid_cleans_up_expired_before_placing() {
    let (env, client) = setup();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let _ = client.set_admin(&admin);
    let investor1 = add_verified_investor(&env, &client, 100_000);
    let investor2 = add_verified_investor(&env, &client, 100_000);
    let business = Address::generate(&env);

    let invoice_id = create_verified_invoice(&env, &client, &admin, &business, 100_000);
    
    // Place initial bid
    let bid_1 = client.place_bid(&investor1, &invoice_id, &10_000, &12_000);
    
    // Verify bid is placed
    let placed_before = client.get_bids_by_status(&invoice_id, &BidStatus::Placed);
    assert_eq!(placed_before.len(), 1, "Should have 1 placed bid");
    
    // Advance time past expiration
    env.ledger().set_timestamp(env.ledger().timestamp() + 604800 + 1);
    
    // Place new bid - should trigger cleanup of expired bid
    let _bid_2 = client.place_bid(&investor2, &invoice_id, &15_000, &18_000);
    
    // Verify old bid is expired and new bid is placed
    let placed_after = client.get_bids_by_status(&invoice_id, &BidStatus::Placed);
    assert_eq!(placed_after.len(), 1, "Should have only 1 placed bid (new one)");
    
    // Verify the expired bid is marked as expired (check individual record)
    let bid_1_status = client.get_bid(&bid_1).unwrap();
    assert_eq!(bid_1_status.status, BidStatus::Expired, "First bid should be expired");
}

/// Test: Partial expiration - only expired bids are cleaned up
#[test]
fn test_partial_expiration_cleanup() {
    let (env, client) = setup();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let _ = client.set_admin(&admin);
    let investor1 = add_verified_investor(&env, &client, 100_000);
    let investor2 = add_verified_investor(&env, &client, 100_000);
    let investor3 = add_verified_investor(&env, &client, 100_000);
    let business = Address::generate(&env);

    let invoice_id = create_verified_invoice(&env, &client, &admin, &business, 100_000);
    
    // Place first bid
    let bid_1 = client.place_bid(&investor1, &invoice_id, &10_000, &12_000);
    
    // Advance time by 3 days (not expired yet)
    env.ledger().set_timestamp(env.ledger().timestamp() + (3 * 24 * 60 * 60));
    
    // Place second bid
    let bid_2 = client.place_bid(&investor2, &invoice_id, &15_000, &18_000);
    
    // Advance time by 5 more days (total 8 days - first bid expired, second not)
    env.ledger().set_timestamp(env.ledger().timestamp() + (5 * 24 * 60 * 60));
    
    // Place third bid - should clean up only first expired bid
    let _bid_3 = client.place_bid(&investor3, &invoice_id, &20_000, &24_000);
    
    // Verify first bid is expired (check individual record)
    let bid_1_status = client.get_bid(&bid_1).unwrap();
    assert_eq!(bid_1_status.status, BidStatus::Expired, "First bid should be expired");
    
    // Verify second and third bids are still placed
    let bid_2_status = client.get_bid(&bid_2).unwrap();
    assert_eq!(bid_2_status.status, BidStatus::Placed, "Second bid should still be placed");
    
    let placed_bids = client.get_bids_by_status(&invoice_id, &BidStatus::Placed);
    assert_eq!(placed_bids.len(), 2, "Should have 2 placed bids (second and third)");
}

/// Test: Cleanup is triggered when querying bids after expiration
#[test]
fn test_cleanup_triggered_on_query_after_expiration() {
    let (env, client) = setup();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let _ = client.set_admin(&admin);
    let investor1 = add_verified_investor(&env, &client, 100_000);
    let investor2 = add_verified_investor(&env, &client, 100_000);
    let business = Address::generate(&env);

    let invoice_id = create_verified_invoice(&env, &client, &admin, &business, 100_000);
    
    // Place two bids at different times
    let bid_1 = client.place_bid(&investor1, &invoice_id, &10_000, &12_000);
    
    // Advance time by 1 day
    env.ledger().set_timestamp(env.ledger().timestamp() + (1 * 24 * 60 * 60));
    
    let _bid_2 = client.place_bid(&investor2, &invoice_id, &15_000, &18_000);
    
    // Advance time by 7 more days (first bid expired, second still valid)
    env.ledger().set_timestamp(env.ledger().timestamp() + (7 * 24 * 60 * 60));
    
    // Query bids - should trigger cleanup
    let placed_bids = client.get_bids_by_status(&invoice_id, &BidStatus::Placed);
    assert_eq!(placed_bids.len(), 1, "Should have only 1 placed bid after cleanup");
    
    // Verify first bid is expired (check individual record)
    let bid_1_status = client.get_bid(&bid_1).unwrap();
    assert_eq!(bid_1_status.status, BidStatus::Expired, "First bid should be expired");
}

/// Test: Cannot accept expired bid
#[test]
fn test_cannot_accept_expired_bid() {
    let (env, client) = setup();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let _ = client.set_admin(&admin);
    let investor = add_verified_investor(&env, &client, 100_000);
    let business = Address::generate(&env);

    let invoice_id = create_verified_invoice(&env, &client, &admin, &business, 100_000);
    
    // Place bid
    let bid_id = client.place_bid(&investor, &invoice_id, &10_000, &12_000);
    
    // Advance time past expiration
    env.ledger().set_timestamp(env.ledger().timestamp() + 604800 + 1);
    
    // Try to accept expired bid - should fail (cleanup happens during accept_bid)
    let result = client.try_accept_bid(&invoice_id, &bid_id);
    assert!(result.is_err(), "Should not be able to accept expired bid");
}

/// Test: Bid at exact expiration boundary (not expired)
#[test]
fn test_bid_at_exact_expiration_not_expired() {
    let (env, client) = setup();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let _ = client.set_admin(&admin);
    let investor = add_verified_investor(&env, &client, 100_000);
    let business = Address::generate(&env);

    let invoice_id = create_verified_invoice(&env, &client, &admin, &business, 100_000);
    
    // Place bid
    let bid_id = client.place_bid(&investor, &invoice_id, &10_000, &12_000);
    let bid = client.get_bid(&bid_id).unwrap();
    
    // Set time to exactly expiration timestamp (not past it)
    env.ledger().set_timestamp(bid.expiration_timestamp);
    
    // Bid should still be valid (not expired)
    let placed_bids = client.get_bids_by_status(&invoice_id, &BidStatus::Placed);
    assert_eq!(placed_bids.len(), 1, "Bid at exact expiration should still be placed");
    
    // Verify bid status is still Placed
    let bid_status = client.get_bid(&bid_id).unwrap();
    assert_eq!(bid_status.status, BidStatus::Placed, "Bid should still be placed at exact expiration");
}

/// Test: Bid one second past expiration (expired)
#[test]
fn test_bid_one_second_past_expiration_expired() {
    let (env, client) = setup();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let _ = client.set_admin(&admin);
    let investor = add_verified_investor(&env, &client, 100_000);
    let business = Address::generate(&env);

    let invoice_id = create_verified_invoice(&env, &client, &admin, &business, 100_000);
    
    // Place bid
    let bid_id = client.place_bid(&investor, &invoice_id, &10_000, &12_000);
    let bid = client.get_bid(&bid_id).unwrap();
    
    // Set time to one second past expiration
    env.ledger().set_timestamp(bid.expiration_timestamp + 1);
    
    // Trigger cleanup
    let removed = client.cleanup_expired_bids(&invoice_id);
    assert_eq!(removed, 1, "Should remove 1 expired bid");
    
    // Verify bid is expired
    let bid_status = client.get_bid(&bid_id).unwrap();
    assert_eq!(bid_status.status, BidStatus::Expired, "Bid should be expired one second past expiration");
}

/// Test: Cleanup with no expired bids returns zero
#[test]
fn test_cleanup_with_no_expired_bids_returns_zero() {
    let (env, client) = setup();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let _ = client.set_admin(&admin);
    let investor = add_verified_investor(&env, &client, 100_000);
    let business = Address::generate(&env);

    let invoice_id = create_verified_invoice(&env, &client, &admin, &business, 100_000);
    
    // Place bid
    let _bid_id = client.place_bid(&investor, &invoice_id, &10_000, &12_000);
    
    // Cleanup immediately (no expired bids)
    let removed = client.cleanup_expired_bids(&invoice_id);
    assert_eq!(removed, 0, "Should remove 0 bids when none are expired");
    
    // Verify bid is still placed
    let placed_bids = client.get_bids_by_status(&invoice_id, &BidStatus::Placed);
    assert_eq!(placed_bids.len(), 1, "Bid should still be placed");
}

/// Test: Cleanup on invoice with no bids returns zero
#[test]
fn test_cleanup_on_invoice_with_no_bids() {
    let (env, client) = setup();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let _ = client.set_admin(&admin);
    let business = Address::generate(&env);

    let invoice_id = create_verified_invoice(&env, &client, &admin, &business, 100_000);
    
    // Cleanup on invoice with no bids
    let removed = client.cleanup_expired_bids(&invoice_id);
    assert_eq!(removed, 0, "Should remove 0 bids when invoice has no bids");
}

/// Test: Withdrawn bids are not affected by expiration cleanup
#[test]
fn test_withdrawn_bids_not_affected_by_expiration() {
    let (env, client) = setup();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let _ = client.set_admin(&admin);
    let investor1 = add_verified_investor(&env, &client, 100_000);
    let investor2 = add_verified_investor(&env, &client, 100_000);
    let business = Address::generate(&env);

    let invoice_id = create_verified_invoice(&env, &client, &admin, &business, 100_000);
    
    // Place two bids
    let bid_1 = client.place_bid(&investor1, &invoice_id, &10_000, &12_000);
    let bid_2 = client.place_bid(&investor2, &invoice_id, &15_000, &18_000);
    
    // Withdraw first bid
    let _ = client.try_withdraw_bid(&bid_1);
    
    // Advance time past expiration
    env.ledger().set_timestamp(env.ledger().timestamp() + 604800 + 1);
    
    // Cleanup should only affect placed bids
    let removed = client.cleanup_expired_bids(&invoice_id);
    assert_eq!(removed, 1, "Should remove only 1 placed bid");
    
    // Verify first bid is still withdrawn (not expired) - check individual record
    let bid_1_status = client.get_bid(&bid_1).unwrap();
    assert_eq!(bid_1_status.status, BidStatus::Withdrawn, "Withdrawn bid should remain withdrawn");
    
    // Verify second bid is expired - check individual record
    let bid_2_status = client.get_bid(&bid_2).unwrap();
    assert_eq!(bid_2_status.status, BidStatus::Expired, "Placed bid should be expired");
}

/// Test: Cancelled bids are not affected by expiration cleanup
#[test]
fn test_cancelled_bids_not_affected_by_expiration() {
    let (env, client) = setup();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let _ = client.set_admin(&admin);
    let investor1 = add_verified_investor(&env, &client, 100_000);
    let investor2 = add_verified_investor(&env, &client, 100_000);
    let business = Address::generate(&env);

    let invoice_id = create_verified_invoice(&env, &client, &admin, &business, 100_000);
    
    // Place two bids
    let bid_1 = client.place_bid(&investor1, &invoice_id, &10_000, &12_000);
    let bid_2 = client.place_bid(&investor2, &invoice_id, &15_000, &18_000);
    
    // Cancel first bid
    let _ = client.cancel_bid(&bid_1);
    
    // Advance time past expiration
    env.ledger().set_timestamp(env.ledger().timestamp() + 604800 + 1);
    
    // Cleanup should only affect placed bids
    let removed = client.cleanup_expired_bids(&invoice_id);
    assert_eq!(removed, 1, "Should remove only 1 placed bid");
    
    // Verify first bid is still cancelled (not expired) - check individual record
    let bid_1_status = client.get_bid(&bid_1).unwrap();
    assert_eq!(bid_1_status.status, BidStatus::Cancelled, "Cancelled bid should remain cancelled");
    
    // Verify second bid is expired - check individual record
    let bid_2_status = client.get_bid(&bid_2).unwrap();
    assert_eq!(bid_2_status.status, BidStatus::Expired, "Placed bid should be expired");
}

/// Test: Mixed status bids - only Placed bids expire
#[test]
fn test_mixed_status_bids_only_placed_expire() {
    let (env, client) = setup();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let _ = client.set_admin(&admin);
    let investor1 = add_verified_investor(&env, &client, 100_000);
    let investor2 = add_verified_investor(&env, &client, 100_000);
    let investor3 = add_verified_investor(&env, &client, 100_000);
    let investor4 = add_verified_investor(&env, &client, 100_000);
    let business = Address::generate(&env);

    let invoice_id = create_verified_invoice(&env, &client, &admin, &business, 100_000);
    
    // Place four bids
    let bid_1 = client.place_bid(&investor1, &invoice_id, &10_000, &12_000);
    let bid_2 = client.place_bid(&investor2, &invoice_id, &15_000, &18_000);
    let bid_3 = client.place_bid(&investor3, &invoice_id, &20_000, &24_000);
    let bid_4 = client.place_bid(&investor4, &invoice_id, &25_000, &30_000);
    
    // Withdraw bid 1
    let _ = client.try_withdraw_bid(&bid_1);
    
    // Cancel bid 2
    let _ = client.cancel_bid(&bid_2);
    
    // Leave bid 3 and 4 as Placed
    
    // Advance time past expiration
    env.ledger().set_timestamp(env.ledger().timestamp() + 604800 + 1);
    
    // Cleanup should only affect placed bids (3 and 4)
    let removed = client.cleanup_expired_bids(&invoice_id);
    assert_eq!(removed, 2, "Should remove 2 placed bids");
    
    // Verify statuses
    assert_eq!(client.get_bid(&bid_1).unwrap().status, BidStatus::Withdrawn);
    assert_eq!(client.get_bid(&bid_2).unwrap().status, BidStatus::Cancelled);
    assert_eq!(client.get_bid(&bid_3).unwrap().status, BidStatus::Expired);
    assert_eq!(client.get_bid(&bid_4).unwrap().status, BidStatus::Expired);
}

/// Test: Expiration cleanup is isolated per invoice
#[test]
fn test_expiration_cleanup_isolated_per_invoice() {
    let (env, client) = setup();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let _ = client.set_admin(&admin);
    let investor = add_verified_investor(&env, &client, 100_000);
    let business = Address::generate(&env);

    // Create two invoices
    let invoice_id_1 = create_verified_invoice(&env, &client, &admin, &business, 50_000);
    let invoice_id_2 = create_verified_invoice(&env, &client, &admin, &business, 50_000);
    
    // Place bids on both invoices
    let bid_1 = client.place_bid(&investor, &invoice_id_1, &10_000, &12_000);
    let bid_2 = client.place_bid(&investor, &invoice_id_2, &15_000, &18_000);
    
    // Advance time past expiration
    env.ledger().set_timestamp(env.ledger().timestamp() + 604800 + 1);
    
    // Cleanup only invoice 1
    let removed_1 = client.cleanup_expired_bids(&invoice_id_1);
    assert_eq!(removed_1, 1, "Should remove 1 bid from invoice 1");
    
    // Verify invoice 1 bid is expired
    let bid_1_status = client.get_bid(&bid_1).unwrap();
    assert_eq!(bid_1_status.status, BidStatus::Expired, "Invoice 1 bid should be expired");
    
    // Verify invoice 2 bid is still placed (cleanup not triggered)
    let bid_2_status = client.get_bid(&bid_2).unwrap();
    assert_eq!(bid_2_status.status, BidStatus::Placed, "Invoice 2 bid should still be placed");
    
    // Now cleanup invoice 2
    let removed_2 = client.cleanup_expired_bids(&invoice_id_2);
    assert_eq!(removed_2, 1, "Should remove 1 bid from invoice 2");
    
    // Verify invoice 2 bid is now expired
    let bid_2_status_after = client.get_bid(&bid_2).unwrap();
    assert_eq!(bid_2_status_after.status, BidStatus::Expired, "Invoice 2 bid should now be expired");
}

/// Test: Expired bids removed from invoice bid list
#[test]
fn test_expired_bids_removed_from_invoice_list() {
    let (env, client) = setup();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let _ = client.set_admin(&admin);
    let investor1 = add_verified_investor(&env, &client, 100_000);
    let investor2 = add_verified_investor(&env, &client, 100_000);
    let business = Address::generate(&env);

    let invoice_id = create_verified_invoice(&env, &client, &admin, &business, 100_000);
    
    // Place two bids
    let _bid_1 = client.place_bid(&investor1, &invoice_id, &10_000, &12_000);
    let _bid_2 = client.place_bid(&investor2, &invoice_id, &15_000, &18_000);
    
    // Get bids for invoice before expiration
    let bids_before = client.get_bids_for_invoice(&invoice_id);
    assert_eq!(bids_before.len(), 2, "Should have 2 bids in invoice list");
    
    // Advance time past expiration
    env.ledger().set_timestamp(env.ledger().timestamp() + 604800 + 1);
    
    // Cleanup
    let _ = client.cleanup_expired_bids(&invoice_id);
    
    // Get bids for invoice after expiration - should be empty
    let bids_after = client.get_bids_for_invoice(&invoice_id);
    assert_eq!(bids_after.len(), 0, "Expired bids should be removed from invoice list");
}

/// Test: Ranking after expiration returns empty list
#[test]
fn test_ranking_after_all_bids_expire() {
    let (env, client) = setup();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let _ = client.set_admin(&admin);
    let investor1 = add_verified_investor(&env, &client, 100_000);
    let investor2 = add_verified_investor(&env, &client, 100_000);
    let investor3 = add_verified_investor(&env, &client, 100_000);
    let business = Address::generate(&env);

    let invoice_id = create_verified_invoice(&env, &client, &admin, &business, 100_000);
    
    // Place three bids with different profits
    let _bid_1 = client.place_bid(&investor1, &invoice_id, &10_000, &12_000);
    let _bid_2 = client.place_bid(&investor2, &invoice_id, &15_000, &20_000);
    let _bid_3 = client.place_bid(&investor3, &invoice_id, &20_000, &24_000);
    
    // Verify ranking works before expiration
    let ranked_before = client.get_ranked_bids(&invoice_id);
    assert_eq!(ranked_before.len(), 3, "Should have 3 ranked bids");
    assert_eq!(ranked_before.get(0).unwrap().investor, investor2, "Best bid should be investor2");
    
    // Advance time past expiration
    env.ledger().set_timestamp(env.ledger().timestamp() + 604800 + 1);
    
    // Ranking should return empty after all bids expire
    let ranked_after = client.get_ranked_bids(&invoice_id);
    assert_eq!(ranked_after.len(), 0, "Ranking should be empty after all bids expire");
    
    // Best bid should be None
    let best_after = client.get_best_bid(&invoice_id);
    assert!(best_after.is_none(), "Best bid should be None after all bids expire");
}
// ============================================================================
// Category 5: Investment Limit Management
// ============================================================================

/// Test: Admin can set investment limit for verified investor
#[test]
fn test_set_investment_limit_succeeds() {
    let (env, client) = setup();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let _ = client.set_admin(&admin);

    // Create investor with initial limit
    let investor = add_verified_investor(&env, &client, 50_000);

    // Verify initial limit (will be adjusted by tier/risk multipliers)
    let verification = client.get_investor_verification(&investor).unwrap();
    let initial_limit = verification.investment_limit;

    // Admin updates limit
    client.set_investment_limit(&investor, &100_000);

    // Verify limit was updated (should be higher than initial)
    let updated_verification = client.get_investor_verification(&investor).unwrap();
    assert!(
        updated_verification.investment_limit > initial_limit,
        "Investment limit should be increased"
    );
}

/// Test: Non-admin cannot set investment limit
#[test]
fn test_set_investment_limit_non_admin_fails() {
    let (env, client) = setup();
    env.mock_all_auths();

    // Create an unverified investor (no admin setup)
    let investor = Address::generate(&env);
    client.submit_investor_kyc(&investor, &String::from_str(&env, "KYC"));

    // Try to set limit without admin setup - should fail with NotAdmin error
    let result = client.try_set_investment_limit(&investor, &100_000);
    assert!(result.is_err(), "Should fail when no admin is configured");
}

/// Test: Cannot set limit for unverified investor
#[test]
fn test_set_investment_limit_unverified_fails() {
    let (env, client) = setup();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let _ = client.set_admin(&admin);

    let unverified_investor = Address::generate(&env);

    // Try to set limit for unverified investor
    let result = client.try_set_investment_limit(&unverified_investor, &100_000);
    assert!(
        result.is_err(),
        "Should not be able to set limit for unverified investor"
    );
}

/// Test: Cannot set invalid investment limit
#[test]
fn test_set_investment_limit_invalid_amount_fails() {
    let (env, client) = setup();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let _ = client.set_admin(&admin);

    let investor = add_verified_investor(&env, &client, 50_000);

    // Try to set zero or negative limit
    let result = client.try_set_investment_limit(&investor, &0);
    assert!(
        result.is_err(),
        "Should not be able to set zero investment limit"
    );

    let result = client.try_set_investment_limit(&investor, &-1000);
    assert!(
        result.is_err(),
        "Should not be able to set negative investment limit"
    );
}

/// Test: Updated limit is enforced in bid placement
#[test]
fn test_updated_limit_enforced_in_bidding() {
    let (env, client) = setup();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let _ = client.set_admin(&admin);

    // Create investor with low initial limit
    let investor = add_verified_investor(&env, &client, 10_000);
    let business = Address::generate(&env);

    let invoice_id = create_verified_invoice(&env, &client, &admin, &business, 50_000);

    // Bid above initial limit should fail
    let result = client.try_place_bid(&investor, &invoice_id, &15_000, &16_000);
    assert!(result.is_err(), "Bid above initial limit should fail");

    // Admin increases limit
    let _ = client.set_investment_limit(&investor, &50_000);

    // Now the same bid should succeed
    let result = client.try_place_bid(&investor, &invoice_id, &15_000, &16_000);
    assert!(result.is_ok(), "Bid should succeed after limit increase");
}

/// Test: cancel_bid transitions Placed → Cancelled
#[test]
fn test_cancel_bid_succeeds() {
    let (env, client) = setup();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let _ = client.set_admin(&admin);
    let investor = add_verified_investor(&env, &client, 100_000);
    let business = Address::generate(&env);

    let invoice_id = create_verified_invoice(&env, &client, &admin, &business, 10_000);
    let bid_id = client.place_bid(&investor, &invoice_id, &5_000, &6_000);

    let result = client.cancel_bid(&bid_id);
    assert!(result, "cancel_bid should return true for a Placed bid");

    let bid = client.get_bid(&bid_id).unwrap();
    assert_eq!(bid.status, BidStatus::Cancelled, "Bid must be Cancelled");
}

/// Test: cancel_bid on already Withdrawn bid returns false
#[test]
fn test_cancel_bid_on_withdrawn_returns_false() {
    let (env, client) = setup();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let _ = client.set_admin(&admin);
    let investor = add_verified_investor(&env, &client, 100_000);
    let business = Address::generate(&env);

    let invoice_id = create_verified_invoice(&env, &client, &admin, &business, 10_000);
    let bid_id = client.place_bid(&investor, &invoice_id, &5_000, &6_000);

    client.withdraw_bid(&bid_id);
    let result = client.cancel_bid(&bid_id);
    assert!(!result, "cancel_bid must return false for non-Placed bid");
}

/// Test: cancel_bid on already Cancelled bid returns false
#[test]
fn test_cancel_bid_on_cancelled_returns_false() {
    let (env, client) = setup();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let _ = client.set_admin(&admin);
    let investor = add_verified_investor(&env, &client, 100_000);
    let business = Address::generate(&env);

    let invoice_id = create_verified_invoice(&env, &client, &admin, &business, 10_000);
    let bid_id = client.place_bid(&investor, &invoice_id, &5_000, &6_000);

    client.cancel_bid(&bid_id);
    let result = client.cancel_bid(&bid_id);
    assert!(!result, "Double cancel must return false");
}

/// Test: cancel_bid on non-existent bid_id returns false
#[test]
fn test_cancel_bid_nonexistent_returns_false() {
    let (env, client) = setup();
    env.mock_all_auths();
    let fake_bid_id = BytesN::from_array(&env, &[0u8; 32]);
    let result = client.cancel_bid(&fake_bid_id);
    assert!(!result, "cancel_bid on unknown ID must return false");
}

/// Test: cancelled bid excluded from ranking
#[test]
fn test_cancelled_bid_excluded_from_ranking() {
    let (env, client) = setup();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let _ = client.set_admin(&admin);
    let investor1 = add_verified_investor(&env, &client, 100_000);
    let investor2 = add_verified_investor(&env, &client, 100_000);
    let business = Address::generate(&env);

    let invoice_id = create_verified_invoice(&env, &client, &admin, &business, 100_000);

    // investor1 profit = 5k (best)
    let bid_1 = client.place_bid(&investor1, &invoice_id, &10_000, &15_000);
    // investor2 profit = 2k
    let _bid_2 = client.place_bid(&investor2, &invoice_id, &10_000, &12_000);

    client.cancel_bid(&bid_1);

    let best = client.get_best_bid(&invoice_id).unwrap();
    assert_eq!(
        best.investor, investor2,
        "Cancelled bid must be excluded from ranking"
    );
}

/// Test: get_all_bids_by_investor returns bids across multiple invoices
#[test]
fn test_get_all_bids_by_investor_cross_invoice() {
    let (env, client) = setup();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let _ = client.set_admin(&admin);
    let investor = add_verified_investor(&env, &client, 100_000);
    let business = Address::generate(&env);

    let invoice_id_1 = create_verified_invoice(&env, &client, &admin, &business, 50_000);
    let invoice_id_2 = create_verified_invoice(&env, &client, &admin, &business, 50_000);

    client.place_bid(&investor, &invoice_id_1, &10_000, &12_000);
    client.place_bid(&investor, &invoice_id_2, &15_000, &18_000);

    let all_bids = client.get_all_bids_by_investor(&investor);
    assert_eq!(all_bids.len(), 2, "Must return bids across all invoices");
}

/// Test: get_all_bids_by_investor returns empty for investor with no bids
#[test]
fn test_get_all_bids_by_investor_empty() {
    let (env, client) = setup();
    env.mock_all_auths();
    let investor = Address::generate(&env);
    let all_bids = client.get_all_bids_by_investor(&investor);
    assert_eq!(all_bids.len(), 0, "Must return empty for unknown investor");
}

// ============================================================================
// Multiple Investors - Same Invoice Tests (Issue #343)
// ============================================================================

/// Test: Multiple investors place bids on same invoice - all bids are tracked
#[test]
fn test_multiple_investors_place_bids_on_same_invoice() {
    let (env, client) = setup();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let _ = client.set_admin(&admin);
    
    // Create 5 verified investors
    let investor1 = add_verified_investor(&env, &client, 100_000);
    let investor2 = add_verified_investor(&env, &client, 100_000);
    let investor3 = add_verified_investor(&env, &client, 100_000);
    let investor4 = add_verified_investor(&env, &client, 100_000);
    let investor5 = add_verified_investor(&env, &client, 100_000);
    let business = Address::generate(&env);

    let invoice_id = create_verified_invoice(&env, &client, &admin, &business, 100_000);

    // All 5 investors place bids with different amounts and profits
    let bid_id1 = client.place_bid(&investor1, &invoice_id, &10_000, &12_000); // profit: 2k
    let bid_id2 = client.place_bid(&investor2, &invoice_id, &15_000, &20_000); // profit: 5k (best)
    let bid_id3 = client.place_bid(&investor3, &invoice_id, &20_000, &24_000); // profit: 4k
    let bid_id4 = client.place_bid(&investor4, &invoice_id, &12_000, &15_000); // profit: 3k
    let bid_id5 = client.place_bid(&investor5, &invoice_id, &18_000, &21_000); // profit: 3k

    // Verify all bids are in Placed status
    let placed_bids = client.get_bids_by_status(&invoice_id, &BidStatus::Placed);
    assert_eq!(placed_bids.len(), 5, "All 5 bids should be in Placed status");

    // Verify get_bids_for_invoice returns all bid IDs
    let all_bid_ids = client.get_bids_for_invoice(&invoice_id);
    assert_eq!(all_bid_ids.len(), 5, "get_bids_for_invoice should return all 5 bid IDs");
    
    // Verify all specific bid IDs are present
    assert!(all_bid_ids.iter().any(|id| id == bid_id1), "bid_id1 should be in list");
    assert!(all_bid_ids.iter().any(|id| id == bid_id2), "bid_id2 should be in list");
    assert!(all_bid_ids.iter().any(|id| id == bid_id3), "bid_id3 should be in list");
    assert!(all_bid_ids.iter().any(|id| id == bid_id4), "bid_id4 should be in list");
    assert!(all_bid_ids.iter().any(|id| id == bid_id5), "bid_id5 should be in list");
}

/// Test: Multiple investors bids are correctly ranked by profit
#[test]
fn test_multiple_investors_bids_ranking_order() {
    let (env, client) = setup();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let _ = client.set_admin(&admin);
    
    let investor1 = add_verified_investor(&env, &client, 100_000);
    let investor2 = add_verified_investor(&env, &client, 100_000);
    let investor3 = add_verified_investor(&env, &client, 100_000);
    let investor4 = add_verified_investor(&env, &client, 100_000);
    let investor5 = add_verified_investor(&env, &client, 100_000);
    let business = Address::generate(&env);

    let invoice_id = create_verified_invoice(&env, &client, &admin, &business, 100_000);

    // Place bids with different profit margins
    let _bid1 = client.place_bid(&investor1, &invoice_id, &10_000, &12_000); // profit: 2k
    let _bid2 = client.place_bid(&investor2, &invoice_id, &15_000, &20_000); // profit: 5k (best)
    let _bid3 = client.place_bid(&investor3, &invoice_id, &20_000, &24_000); // profit: 4k
    let _bid4 = client.place_bid(&investor4, &invoice_id, &12_000, &15_000); // profit: 3k
    let _bid5 = client.place_bid(&investor5, &invoice_id, &18_000, &21_000); // profit: 3k

    // Get ranked bids
    let ranked = client.get_ranked_bids(&invoice_id);
    assert_eq!(ranked.len(), 5, "Should have 5 ranked bids");

    // Verify ranking order by profit (descending)
    assert_eq!(ranked.get(0).unwrap().investor, investor2, "Rank 1: investor2 (profit 5k)");
    assert_eq!(ranked.get(1).unwrap().investor, investor3, "Rank 2: investor3 (profit 4k)");
    // investor4 and investor5 both have 3k profit - either order is valid
    let rank3_investor = ranked.get(2).unwrap().investor;
    let rank4_investor = ranked.get(3).unwrap().investor;
    assert!(
        (rank3_investor == investor4 && rank4_investor == investor5) ||
        (rank3_investor == investor5 && rank4_investor == investor4),
        "Ranks 3-4: investor4 and investor5 (both profit 3k)"
    );
    assert_eq!(ranked.get(4).unwrap().investor, investor1, "Rank 5: investor1 (profit 2k)");

    // Verify best bid is investor2
    let best = client.get_best_bid(&invoice_id).unwrap();
    assert_eq!(best.investor, investor2, "Best bid should be investor2 with highest profit");
}

/// Test: Business accepts one bid, others remain Placed
#[test]
fn test_business_accepts_one_bid_others_remain_placed() {
    let (env, client) = setup();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let _ = client.set_admin(&admin);
    
    let investor1 = add_verified_investor(&env, &client, 100_000);
    let investor2 = add_verified_investor(&env, &client, 100_000);
    let investor3 = add_verified_investor(&env, &client, 100_000);
    let business = Address::generate(&env);

    let invoice_id = create_verified_invoice(&env, &client, &admin, &business, 100_000);

    // Three investors place bids
    let bid_id1 = client.place_bid(&investor1, &invoice_id, &10_000, &12_000);
    let bid_id2 = client.place_bid(&investor2, &invoice_id, &15_000, &20_000);
    let bid_id3 = client.place_bid(&investor3, &invoice_id, &20_000, &24_000);

    // Business accepts bid2
    let result = client.try_accept_bid(&invoice_id, &bid_id2);
    assert!(result.is_ok(), "Business should be able to accept bid2");

    // Verify bid2 is Accepted
    let bid2 = client.get_bid(&bid_id2).unwrap();
    assert_eq!(bid2.status, BidStatus::Accepted, "Accepted bid should have Accepted status");

    // Verify bid1 and bid3 remain Placed
    let bid1 = client.get_bid(&bid_id1).unwrap();
    assert_eq!(bid1.status, BidStatus::Placed, "Non-accepted bid1 should remain Placed");
    
    let bid3 = client.get_bid(&bid_id3).unwrap();
    assert_eq!(bid3.status, BidStatus::Placed, "Non-accepted bid3 should remain Placed");

    // Verify invoice is now Funded
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Funded, "Invoice should be Funded after accepting bid");
}

/// Test: Only one escrow is created when business accepts a bid
#[test]
fn test_only_one_escrow_created_for_accepted_bid() {
    let (env, client) = setup();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let _ = client.set_admin(&admin);
    
    let investor1 = add_verified_investor(&env, &client, 100_000);
    let investor2 = add_verified_investor(&env, &client, 100_000);
    let investor3 = add_verified_investor(&env, &client, 100_000);
    let business = Address::generate(&env);

    let invoice_id = create_verified_invoice(&env, &client, &admin, &business, 100_000);

    // Three investors place bids
    let _bid_id1 = client.place_bid(&investor1, &invoice_id, &10_000, &12_000);
    let bid_id2 = client.place_bid(&investor2, &invoice_id, &15_000, &20_000);
    let _bid_id3 = client.place_bid(&investor3, &invoice_id, &20_000, &24_000);

    // Business accepts bid2
    client.accept_bid(&invoice_id, &bid_id2);

    // Verify exactly one escrow exists for this invoice
    let escrow = client.get_escrow_details(&invoice_id);
    assert_eq!(escrow.status, crate::payments::EscrowStatus::Held, "Escrow should be in Held status");
    assert_eq!(escrow.investor, investor2, "Escrow should reference investor2");
    assert_eq!(escrow.amount, 15_000, "Escrow should hold the accepted bid amount");
    assert_eq!(escrow.invoice_id, invoice_id, "Escrow should reference correct invoice");

    // Verify invoice funded amount matches escrow amount
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.funded_amount, 15_000, "Invoice funded amount should match escrow");
    assert_eq!(invoice.investor, Some(investor2), "Invoice should reference investor2");
}

/// Test: Non-accepted investors can withdraw their bids after one is accepted
#[test]
fn test_non_accepted_investors_can_withdraw_after_acceptance() {
    let (env, client) = setup();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let _ = client.set_admin(&admin);
    
    let investor1 = add_verified_investor(&env, &client, 100_000);
    let investor2 = add_verified_investor(&env, &client, 100_000);
    let investor3 = add_verified_investor(&env, &client, 100_000);
    let business = Address::generate(&env);

    let invoice_id = create_verified_invoice(&env, &client, &admin, &business, 100_000);

    // Three investors place bids
    let bid_id1 = client.place_bid(&investor1, &invoice_id, &10_000, &12_000);
    let bid_id2 = client.place_bid(&investor2, &invoice_id, &15_000, &20_000);
    let bid_id3 = client.place_bid(&investor3, &invoice_id, &20_000, &24_000);

    // Business accepts bid2
    client.accept_bid(&invoice_id, &bid_id2);

    // investor1 withdraws their bid
    let result1 = client.try_withdraw_bid(&bid_id1);
    assert!(result1.is_ok(), "investor1 should be able to withdraw their bid");
    
    let bid1 = client.get_bid(&bid_id1).unwrap();
    assert_eq!(bid1.status, BidStatus::Withdrawn, "bid1 should be Withdrawn");

    // investor3 withdraws their bid
    let result3 = client.try_withdraw_bid(&bid_id3);
    assert!(result3.is_ok(), "investor3 should be able to withdraw their bid");
    
    let bid3 = client.get_bid(&bid_id3).unwrap();
    assert_eq!(bid3.status, BidStatus::Withdrawn, "bid3 should be Withdrawn");

    // Verify bid2 remains Accepted
    let bid2 = client.get_bid(&bid_id2).unwrap();
    assert_eq!(bid2.status, BidStatus::Accepted, "bid2 should remain Accepted");

    // Verify only Accepted bid remains in Placed status query
    let placed_bids = client.get_bids_by_status(&invoice_id, &BidStatus::Placed);
    assert_eq!(placed_bids.len(), 0, "No bids should be in Placed status after withdrawals");

    let withdrawn_bids = client.get_bids_by_status(&invoice_id, &BidStatus::Withdrawn);
    assert_eq!(withdrawn_bids.len(), 2, "Two bids should be Withdrawn");
}

/// Test: get_bids_for_invoice returns all bids regardless of status
#[test]
fn test_get_bids_for_invoice_returns_all_bids() {
    let (env, client) = setup();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let _ = client.set_admin(&admin);
    
    let investor1 = add_verified_investor(&env, &client, 100_000);
    let investor2 = add_verified_investor(&env, &client, 100_000);
    let investor3 = add_verified_investor(&env, &client, 100_000);
    let investor4 = add_verified_investor(&env, &client, 100_000);
    let business = Address::generate(&env);

    let invoice_id = create_verified_invoice(&env, &client, &admin, &business, 100_000);

    // Four investors place bids
    let bid_id1 = client.place_bid(&investor1, &invoice_id, &10_000, &12_000);
    let bid_id2 = client.place_bid(&investor2, &invoice_id, &15_000, &20_000);
    let bid_id3 = client.place_bid(&investor3, &invoice_id, &20_000, &24_000);
    let bid_id4 = client.place_bid(&investor4, &invoice_id, &12_000, &15_000);

    // Initial state: all bids should be returned
    let all_bids = client.get_bids_for_invoice(&invoice_id);
    assert_eq!(all_bids.len(), 4, "Should return all 4 bids initially");

    // Business accepts bid2
    client.accept_bid(&invoice_id, &bid_id2);

    // investor1 withdraws
    client.withdraw_bid(&bid_id1);

    // investor4 cancels
    client.cancel_bid(&bid_id4);

    // get_bids_for_invoice should still return all bid IDs
    // Note: This returns bid IDs, not full records
    let all_bids_after = client.get_bids_for_invoice(&invoice_id);
    assert_eq!(all_bids_after.len(), 4, "Should still return all 4 bid IDs");

    // Verify we can retrieve each bid with different statuses
    assert_eq!(client.get_bid(&bid_id1).unwrap().status, BidStatus::Withdrawn);
    assert_eq!(client.get_bid(&bid_id2).unwrap().status, BidStatus::Accepted);
    assert_eq!(client.get_bid(&bid_id3).unwrap().status, BidStatus::Placed);
    assert_eq!(client.get_bid(&bid_id4).unwrap().status, BidStatus::Cancelled);
}

/// Test: Cannot accept second bid after one is already accepted
#[test]
fn test_cannot_accept_second_bid_after_first_accepted() {
    let (env, client) = setup();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let _ = client.set_admin(&admin);
    
    let investor1 = add_verified_investor(&env, &client, 100_000);
    let investor2 = add_verified_investor(&env, &client, 100_000);
    let business = Address::generate(&env);

    let invoice_id = create_verified_invoice(&env, &client, &admin, &business, 100_000);

    // Two investors place bids
    let bid_id1 = client.place_bid(&investor1, &invoice_id, &10_000, &12_000);
    let bid_id2 = client.place_bid(&investor2, &invoice_id, &15_000, &20_000);

    // Business accepts bid1
    let result = client.try_accept_bid(&invoice_id, &bid_id1);
    assert!(result.is_ok(), "First accept should succeed");

    // Attempt to accept bid2 should fail (invoice already funded)
    let result = client.try_accept_bid(&invoice_id, &bid_id2);
    assert!(result.is_err(), "Second accept should fail - invoice already funded");

    // Verify only bid1 is Accepted
    assert_eq!(client.get_bid(&bid_id1).unwrap().status, BidStatus::Accepted);
    assert_eq!(client.get_bid(&bid_id2).unwrap().status, BidStatus::Placed);

    // Verify invoice is Funded with bid1's amount
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Funded);
    assert_eq!(invoice.funded_amount, 10_000);
    assert_eq!(invoice.investor, Some(investor1));
}
