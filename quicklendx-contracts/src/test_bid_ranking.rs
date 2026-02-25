/// Comprehensive test suite for get_best_bid and get_ranked_bids edge cases
/// Coverage: None cases, single/multiple bids, withdrawn/expired exclusion, ranking order
///
/// Test Categories:
/// 1. get_best_bid edge cases - None for no bids, None for only withdrawn/expired, Some for valid bids
/// 2. get_ranked_bids edge cases - empty list, single bid, multiple bids with proper ordering
/// 3. Exclusion logic - withdrawn and expired bids are properly excluded
/// 4. Ranking consistency - best bid equals first ranked bid
use super::*;
use crate::bid::BidStatus;
use crate::invoice::InvoiceCategory;
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

// Helper: Create verified investor
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
    business: &Address,
    amount: i128,
) -> BytesN<32> {
    let currency = Address::generate(env);
    let due_date = env.ledger().timestamp() + 86400;

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
// Category 1: get_best_bid Edge Cases - None scenarios
// ============================================================================

/// Test: get_best_bid returns None for non-existent invoice
#[test]
fn test_empty_bid_list() {
    let (env, client) = setup();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let _ = client.set_admin(&admin);

    // Create a fake invoice ID that doesn't exist
    let fake_invoice_id = BytesN::from_array(&env, &[0u8; 32]);

    let best_bid = client.get_best_bid(&fake_invoice_id);
    assert!(best_bid.is_none(), "Should return None for non-existent invoice");
}

/// Test: get_best_bid returns None when invoice has only withdrawn bids
#[test]
fn test_best_bid_excludes_withdrawn() {
    let (env, client) = setup();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let _ = client.set_admin(&admin);
    
    let business = Address::generate(&env);
    let investor1 = add_verified_investor(&env, &client, 100_000);
    let investor2 = add_verified_investor(&env, &client, 100_000);

    let invoice_id = create_verified_invoice(&env, &client, &business, 10_000);

    // Place two bids
    let bid1 = client.place_bid(&investor1, &invoice_id, &5_000, &6_000);
    let bid2 = client.place_bid(&investor2, &invoice_id, &5_500, &6_500);

    // Withdraw both bids
    client.cancel_bid(&bid1);
    client.cancel_bid(&bid2);

    // Best bid should be None
    let best_bid = client.get_best_bid(&invoice_id);
    assert!(best_bid.is_none(), "Should return None when all bids are withdrawn");
}

/// Test: get_best_bid returns None when invoice has only expired bids
#[test]
fn test_best_bid_excludes_expired() {
    let (env, client) = setup();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let _ = client.set_admin(&admin);
    
    let business = Address::generate(&env);
    let investor = add_verified_investor(&env, &client, 100_000);

    let invoice_id = create_verified_invoice(&env, &client, &business, 10_000);

    // Place bid with short expiration
    let _bid = client.place_bid(&investor, &invoice_id, &5_000, &6_000);

    // Advance time beyond bid expiration (default 7 days)
    env.ledger().with_mut(|li| {
        li.timestamp = li.timestamp + (8 * 86400); // 8 days
    });

    // Best bid should be None after expiration
    let best_bid = client.get_best_bid(&invoice_id);
    assert!(best_bid.is_none(), "Should return None when all bids are expired");
}

// ============================================================================
// Category 2: get_best_bid Edge Cases - Some scenarios
// ============================================================================

/// Test: get_best_bid returns Some for single valid bid
#[test]
fn test_single_bid_ranking_and_best_selection() {
    let (env, client) = setup();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let _ = client.set_admin(&admin);
    
    let business = Address::generate(&env);
    let investor = add_verified_investor(&env, &client, 100_000);

    let invoice_id = create_verified_invoice(&env, &client, &business, 10_000);

    // Place single bid
    let bid_id = client.place_bid(&investor, &invoice_id, &5_000, &6_000);

    // Best bid should be Some
    let best_bid = client.get_best_bid(&invoice_id);
    assert!(best_bid.is_some(), "Should return Some for single valid bid");
    assert_eq!(best_bid.unwrap().bid_id, bid_id, "Should return the correct bid");
}

/// Test: get_best_bid returns highest profit bid from multiple bids
#[test]
fn test_ranking_with_multiple_bids() {
    let (env, client) = setup();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let _ = client.set_admin(&admin);
    
    let business = Address::generate(&env);
    let investor1 = add_verified_investor(&env, &client, 100_000);
    let investor2 = add_verified_investor(&env, &client, 100_000);
    let investor3 = add_verified_investor(&env, &client, 100_000);

    let invoice_id = create_verified_invoice(&env, &client, &business, 10_000);

    // Place bids with different profit margins
    // Bid 1: 5000 -> 6000 (profit: 1000, margin: 20%)
    let bid1 = client.place_bid(&investor1, &invoice_id, &5_000, &6_000);
    
    // Bid 2: 5000 -> 7000 (profit: 2000, margin: 40%) - BEST
    let bid2 = client.place_bid(&investor2, &invoice_id, &5_000, &7_000);
    
    // Bid 3: 5000 -> 6500 (profit: 1500, margin: 30%)
    let bid3 = client.place_bid(&investor3, &invoice_id, &5_000, &6_500);

    // Best bid should be bid2 (highest profit)
    let best_bid = client.get_best_bid(&invoice_id);
    assert!(best_bid.is_some(), "Should return Some for multiple valid bids");
    assert_eq!(best_bid.unwrap().bid_id, bid2, "Should return highest profit bid");
}

/// Test: get_best_bid excludes withdrawn bids and selects from remaining
#[test]
fn test_best_bid_after_withdrawal() {
    let (env, client) = setup();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let _ = client.set_admin(&admin);
    
    let business = Address::generate(&env);
    let investor1 = add_verified_investor(&env, &client, 100_000);
    let investor2 = add_verified_investor(&env, &client, 100_000);

    let invoice_id = create_verified_invoice(&env, &client, &business, 10_000);

    // Bid 1: 5000 -> 7000 (profit: 2000) - BEST initially
    let bid1 = client.place_bid(&investor1, &invoice_id, &5_000, &7_000);
    
    // Bid 2: 5000 -> 6000 (profit: 1000)
    let bid2 = client.place_bid(&investor2, &invoice_id, &5_000, &6_000);

    // Withdraw best bid
    client.cancel_bid(&bid1);

    // Best bid should now be bid2
    let best_bid = client.get_best_bid(&invoice_id);
    assert!(best_bid.is_some(), "Should return Some after withdrawal");
    assert_eq!(best_bid.unwrap().bid_id, bid2, "Should return next best bid after withdrawal");
}

// ============================================================================
// Category 3: get_ranked_bids Edge Cases
// ============================================================================

/// Test: get_ranked_bids returns empty for non-existent invoice
#[test]
fn test_empty_ranked_and_best_for_nonexistent_invoice() {
    let (env, client) = setup();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let _ = client.set_admin(&admin);

    let fake_invoice_id = BytesN::from_array(&env, &[0u8; 32]);

    let ranked_bids = client.get_ranked_bids(&fake_invoice_id);
    assert_eq!(ranked_bids.len(), 0, "Should return empty vec for non-existent invoice");
    
    let best_bid = client.get_best_bid(&fake_invoice_id);
    assert!(best_bid.is_none(), "Should return None for non-existent invoice");
}

/// Test: get_ranked_bids excludes withdrawn and expired bids
#[test]
fn test_ranked_excludes_withdrawn_and_expired() {
    let (env, client) = setup();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let _ = client.set_admin(&admin);
    
    let business = Address::generate(&env);
    let investor1 = add_verified_investor(&env, &client, 100_000);
    let investor2 = add_verified_investor(&env, &client, 100_000);
    let investor3 = add_verified_investor(&env, &client, 100_000);
    let investor4 = add_verified_investor(&env, &client, 100_000);

    let invoice_id = create_verified_invoice(&env, &client, &business, 10_000);

    // Place four bids
    let bid1 = client.place_bid(&investor1, &invoice_id, &5_000, &7_000);
    let _bid2 = client.place_bid(&investor2, &invoice_id, &5_000, &6_800);
    let _bid3 = client.place_bid(&investor3, &invoice_id, &5_000, &6_500);

    // Withdraw bid1 (highest profit)
    client.cancel_bid(&bid1);

    // Advance time to expire all existing bids
    env.ledger().with_mut(|li| {
        li.timestamp = li.timestamp + (8 * 86400); // 8 days
    });

    // Place a new bid after time advancement (this one won't be expired)
    let bid4 = client.place_bid(&investor4, &invoice_id, &5_000, &6_000);

    // Only bid4 should remain in ranked list (others are expired or withdrawn)
    let ranked_bids = client.get_ranked_bids(&invoice_id);
    assert_eq!(ranked_bids.len(), 1, "Should only include active bids");
    assert_eq!(ranked_bids.get(0).unwrap().bid_id, bid4, "Should only include non-withdrawn, non-expired bid");
}

/// Test: get_ranked_bids orders by profit descending
#[test]
fn test_ranked_bids_profit_ordering() {
    let (env, client) = setup();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let _ = client.set_admin(&admin);
    
    let business = Address::generate(&env);
    let investor1 = add_verified_investor(&env, &client, 100_000);
    let investor2 = add_verified_investor(&env, &client, 100_000);
    let investor3 = add_verified_investor(&env, &client, 100_000);

    let invoice_id = create_verified_invoice(&env, &client, &business, 10_000);

    // Place bids in non-sorted order
    // Bid 1: profit 1000 (should be 3rd)
    let bid1 = client.place_bid(&investor1, &invoice_id, &5_000, &6_000);
    
    // Bid 2: profit 2000 (should be 1st)
    let bid2 = client.place_bid(&investor2, &invoice_id, &5_000, &7_000);
    
    // Bid 3: profit 1500 (should be 2nd)
    let bid3 = client.place_bid(&investor3, &invoice_id, &5_000, &6_500);

    let ranked_bids = client.get_ranked_bids(&invoice_id);
    assert_eq!(ranked_bids.len(), 3, "Should have all 3 bids");
    
    // Verify ordering: bid2, bid3, bid1
    assert_eq!(ranked_bids.get(0).unwrap().bid_id, bid2, "First should be highest profit");
    assert_eq!(ranked_bids.get(1).unwrap().bid_id, bid3, "Second should be middle profit");
    assert_eq!(ranked_bids.get(2).unwrap().bid_id, bid1, "Third should be lowest profit");
}

// ============================================================================
// Category 4: Consistency Between get_best_bid and get_ranked_bids
// ============================================================================

/// Test: get_best_bid equals first element of get_ranked_bids
#[test]
fn test_best_bid_equals_first_ranked() {
    let (env, client) = setup();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let _ = client.set_admin(&admin);
    
    let business = Address::generate(&env);
    let investor1 = add_verified_investor(&env, &client, 100_000);
    let investor2 = add_verified_investor(&env, &client, 100_000);
    let investor3 = add_verified_investor(&env, &client, 100_000);

    let invoice_id = create_verified_invoice(&env, &client, &business, 10_000);

    // Place multiple bids
    let _bid1 = client.place_bid(&investor1, &invoice_id, &5_000, &6_000);
    let _bid2 = client.place_bid(&investor2, &invoice_id, &5_000, &7_000);
    let _bid3 = client.place_bid(&investor3, &invoice_id, &5_000, &6_500);

    let best_bid = client.get_best_bid(&invoice_id);
    let ranked_bids = client.get_ranked_bids(&invoice_id);

    assert!(best_bid.is_some(), "Best bid should exist");
    assert!(ranked_bids.len() > 0, "Ranked bids should not be empty");
    
    assert_eq!(
        best_bid.unwrap().bid_id,
        ranked_bids.get(0).unwrap().bid_id,
        "Best bid should equal first ranked bid"
    );
}

// ============================================================================
// Category 5: Tie-Breaking Edge Cases
// ============================================================================

/// Test: Equal profit bids are tie-broken by timestamp (earlier wins)
#[test]
fn test_equal_bids_tie_break_by_timestamp() {
    let (env, client) = setup();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let _ = client.set_admin(&admin);
    
    let business = Address::generate(&env);
    let investor1 = add_verified_investor(&env, &client, 100_000);
    let investor2 = add_verified_investor(&env, &client, 100_000);

    let invoice_id = create_verified_invoice(&env, &client, &business, 10_000);

    // Place two bids with identical profit
    let bid1 = client.place_bid(&investor1, &invoice_id, &5_000, &6_000);
    
    // Advance time slightly
    env.ledger().with_mut(|li| {
        li.timestamp = li.timestamp + 100;
    });
    
    let bid2 = client.place_bid(&investor2, &invoice_id, &5_000, &6_000);

    let best_bid = client.get_best_bid(&invoice_id);
    let ranked_bids = client.get_ranked_bids(&invoice_id);

    // Earlier bid (bid1) should win the tie
    assert_eq!(best_bid.unwrap().bid_id, bid1, "Earlier bid should win tie");
    assert_eq!(ranked_bids.get(0).unwrap().bid_id, bid1, "Earlier bid should be ranked first");
    assert_eq!(ranked_bids.get(1).unwrap().bid_id, bid2, "Later bid should be ranked second");
}
