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
    _admin: &Address,
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
