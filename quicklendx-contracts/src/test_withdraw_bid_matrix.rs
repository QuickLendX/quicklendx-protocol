//! Withdraw bid authorization and state-precondition matrix tests - Issue #1XXX
//!
//! Verifies that `withdraw_bid` is exclusively callable by the investor who
//! placed the bid, and only from `Placed` status. Withdrawn bids are excluded from
//! ranking and selection operations.
//!
//! # Authorization matrix
//!
//! | Caller            | Bid status | Expected outcome              |
//! |-------------------|------------|-------------------------------|
//! | Bid owner         | Placed     | Ok - status -> Withdrawn      |
//! | Bid owner         | Withdrawn  | OperationNotAllowed (no-op)   |
//! | Bid owner         | Cancelled  | OperationNotAllowed (no-op)   |
//! | Bid owner         | Accepted   | OperationNotAllowed (no-op)   |
//! | Bid owner         | Expired    | OperationNotAllowed (no-op)   |
//! | Third party       | Placed     | Auth panic (rejected)         |
//! | Business owner    | Placed     | Auth panic (rejected)         |
//! | Admin             | Placed     | Auth panic (rejected)         |
//! | Non-existent bid  | -          | StorageKeyNotFound            |
//!
//! # Ranking & selection invariants
//!
//! - Withdrawn bids **excluded** from `get_best_bid` results
//! - Withdrawn bids **excluded** from `rank_bids` results
//! - Only `Placed` bids participate in ranking
//!
//! # Index & state consistency
//!
//! - `bids_by_status(Withdrawn)` includes the withdrawn bid after transition
//! - All bid fields except `status` remain unchanged
//! - Withdrawn bid cannot later be accepted
//!
//! # Security assumptions validated
//!
//! - `require_auth()` is called on `bid.investor` inside `withdraw_bid`; any
//!   other signer causes a host-level auth failure.
//! - Admin has **no** special override for `withdraw_bid`; withdrawal is
//!   strictly investor-only.
//! - Withdrawal is only allowed from `Placed` - other statuses return
//!   `OperationNotAllowed` without mutating state.
//! - A withdrawn bid cannot be re-withdrawn, and cannot be re-placed via normal
//!   bid placement (only the investor's next new bid placement succeeds).

#![cfg(test)]

use crate::bid::BidStatus;
use crate::errors::QuickLendXError;
use crate::invoice::InvoiceCategory;
use crate::{QuickLendXContract, QuickLendXContractClient};
use soroban_sdk::{
    testutils::{Address as _, Ledger, MockAuth, MockAuthInvoke},
    Address, BytesN, Env, IntoVal, String, Vec,
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Minimal environment with mock_all_auths for setup convenience.
fn setup() -> (Env, QuickLendXContractClient<'static>, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &id);
    let admin = Address::generate(&env);
    client.set_admin(&admin);
    let business = Address::generate(&env);
    client.submit_kyc_application(&business, &String::from_str(&env, "kyc"));
    client.verify_business(&admin, &business);
    (env, client, admin, business)
}

/// Place a bid and return (bid_id, investor, invoice_id).
fn place_bid(
    env: &Env,
    client: &QuickLendXContractClient,
    admin: &Address,
    business: &Address,
) -> (BytesN<32>, Address, BytesN<32>) {
    let currency = Address::generate(env);
    client.add_currency(admin, &currency);
    let due = env.ledger().timestamp() + 86_400;
    let invoice_id = client.store_invoice(
        business,
        &1_000i128,
        &currency,
        &due,
        &String::from_str(env, "inv").into_bytes(),
        &InvoiceCategory::Services,
        &Vec::new(env),
    ).unwrap();
    client.verify_invoice(&invoice_id);
    let investor = Address::generate(env);
    client.submit_investor_kyc(&investor, &String::from_str(env, "kyc").into_bytes()).unwrap();
    client.verify_investor(admin, &investor, &10_000i128).unwrap();
    let bid_id = client.place_bid(&investor, &invoice_id, &900i128, &950i128);
    (bid_id, investor, invoice_id)
}

// ===========================================================================
// 1. HAPPY PATH - investor withdraws own Placed bid
// ===========================================================================

#[test]
fn test_investor_can_withdraw_own_placed_bid() {
    let (env, client, admin, business) = setup();
    let (bid_id, investor, _) = place_bid(&env, &client, &admin, &business);

    // mock_all_auths is active - investor auth is satisfied
    let result = client.withdraw_bid(&bid_id);
    assert!(result.is_ok(), "investor should be able to withdraw their own Placed bid");

    let bid = client.get_bid(&bid_id).unwrap();
    assert_eq!(
        bid.status, BidStatus::Withdrawn,
        "bid status must be Withdrawn after withdraw_bid"
    );
    assert_eq!(bid.investor, investor, "investor field must be unchanged");
}

#[test]
fn test_withdraw_bid_returns_ok_on_success() {
    let (env, client, admin, business) = setup();
    let (bid_id, _, _) = place_bid(&env, &client, &admin, &business);
    let result = client.withdraw_bid(&bid_id);
    assert!(result.is_ok(), "withdraw_bid should return Ok for Placed bid");
}

// ===========================================================================
// 2. STATE PRECONDITION MATRIX - invalid states reject withdrawal
// ===========================================================================

#[test]
fn test_withdraw_already_withdrawn_bid_fails() {
    let (env, client, admin, business) = setup();
    let (bid_id, _, _) = place_bid(&env, &client, &admin, &business);

    // First withdraw
    client.withdraw_bid(&bid_id).unwrap();

    // Second withdraw attempt
    let result = client.withdraw_bid(&bid_id);
    assert!(result.is_err(), "withdrawing an already-Withdrawn bid must fail");
    assert_eq!(
        result.unwrap_err(),
        QuickLendXError::OperationNotAllowed,
        "error must be OperationNotAllowed"
    );
}

#[test]
fn test_withdraw_cancelled_bid_fails() {
    let (env, client, admin, business) = setup();
    let (bid_id, _, _) = place_bid(&env, &client, &admin, &business);

    // Cancel the bid first
    client.cancel_bid(&bid_id);

    // Try to withdraw
    let result = client.withdraw_bid(&bid_id);
    assert!(result.is_err(), "withdrawing a Cancelled bid must fail");
    assert_eq!(
        result.unwrap_err(),
        QuickLendXError::OperationNotAllowed,
        "error must be OperationNotAllowed"
    );

    // Bid must still be Cancelled
    let bid = client.get_bid(&bid_id).unwrap();
    assert_eq!(bid.status, BidStatus::Cancelled);
}

#[test]
fn test_withdraw_accepted_bid_fails() {
    let (env, client, admin, business) = setup();
    let (bid_id, _, invoice_id) = place_bid(&env, &client, &admin, &business);

    // Accept the bid
    client.accept_bid(&invoice_id, &bid_id).unwrap();

    // Try to withdraw
    let result = client.withdraw_bid(&bid_id);
    assert!(result.is_err(), "withdrawing an Accepted bid must fail");
    assert_eq!(
        result.unwrap_err(),
        QuickLendXError::OperationNotAllowed,
        "error must be OperationNotAllowed"
    );

    // Bid must still be Accepted
    let bid = client.get_bid(&bid_id).unwrap();
    assert_eq!(bid.status, BidStatus::Accepted);
}

#[test]
fn test_withdraw_expired_bid_fails() {
    let (env, client, admin, business) = setup();
    let (bid_id, _, _) = place_bid(&env, &client, &admin, &business);

    // Advance time past TTL (default 7 days = 604_800 seconds)
    env.ledger().set_timestamp(env.ledger().timestamp() + 604_801);

    // Try to withdraw the expired bid
    let result = client.withdraw_bid(&bid_id);
    assert!(result.is_err(), "withdrawing an Expired bid must fail");
    assert_eq!(
        result.unwrap_err(),
        QuickLendXError::OperationNotAllowed,
        "error must be OperationNotAllowed"
    );

    // Bid should be Expired
    let bid = client.get_bid(&bid_id).unwrap();
    assert_eq!(bid.status, BidStatus::Expired);
}

#[test]
fn test_withdraw_nonexistent_bid_fails() {
    let (env, client, _, _) = setup();
    let fake_id = BytesN::from_array(&env, &[0u8; 32]);
    let result = client.withdraw_bid(&fake_id);
    assert!(result.is_err(), "withdrawing a non-existent bid must fail");
    assert_eq!(
        result.unwrap_err(),
        QuickLendXError::StorageKeyNotFound,
        "error must be StorageKeyNotFound"
    );
}

// ===========================================================================
// 3. AUTHORIZATION MATRIX - unauthorized callers are rejected
// ===========================================================================

/// Third party (random address) cannot withdraw someone else's bid.
#[test]
fn test_third_party_cannot_withdraw_bid() {
    let env = Env::default();
    let id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &id);

    // Use mock_all_auths only for setup
    env.mock_all_auths();
    let admin = Address::generate(&env);
    client.set_admin(&admin);
    let business = Address::generate(&env);
    client.submit_kyc_application(&business, &String::from_str(&env, "kyc"));
    client.verify_business(&admin, &business);
    let (bid_id, _, _) = place_bid(&env, &client, &admin, &business);

    // Now test with explicit auth for the WRONG address
    let attacker = Address::generate(&env);
    env.mock_auths(&[MockAuth {
        address: &attacker,
        invoke: &MockAuthInvoke {
            contract: &id,
            fn_name: "withdraw_bid",
            args: (bid_id.clone(),).into_val(&env),
            sub_invokes: &[],
        },
    }]);

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = client.withdraw_bid(&bid_id);
    }));
    assert!(result.is_err(), "third party must not be able to withdraw bid");

    // Bid must still be Placed
    let bid = client.get_bid(&bid_id).unwrap();
    assert_eq!(bid.status, BidStatus::Placed);
}

/// Business owner cannot withdraw an investor's bid on their own invoice.
#[test]
fn test_business_owner_cannot_withdraw_bid() {
    let env = Env::default();
    let id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &id);

    env.mock_all_auths();
    let admin = Address::generate(&env);
    client.set_admin(&admin);
    let business = Address::generate(&env);
    client.submit_kyc_application(&business, &String::from_str(&env, "kyc"));
    client.verify_business(&admin, &business);
    let (bid_id, _, _) = place_bid(&env, &client, &admin, &business);

    // Mock auth as business, not investor
    env.mock_auths(&[MockAuth {
        address: &business,
        invoke: &MockAuthInvoke {
            contract: &id,
            fn_name: "withdraw_bid",
            args: (bid_id.clone(),).into_val(&env),
            sub_invokes: &[],
        },
    }]);

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = client.withdraw_bid(&bid_id);
    }));
    assert!(
        result.is_err(),
        "business owner must not be able to withdraw investor bid"
    );

    let bid = client.get_bid(&bid_id).unwrap();
    assert_eq!(bid.status, BidStatus::Placed);
}

/// Admin has NO special override - cannot withdraw bids.
#[test]
fn test_admin_cannot_withdraw_bid() {
    let env = Env::default();
    let id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &id);

    env.mock_all_auths();
    let admin = Address::generate(&env);
    client.set_admin(&admin);
    let business = Address::generate(&env);
    client.submit_kyc_application(&business, &String::from_str(&env, "kyc"));
    client.verify_business(&admin, &business);
    let (bid_id, _, _) = place_bid(&env, &client, &admin, &business);

    // Mock auth as admin only
    env.mock_auths(&[MockAuth {
        address: &admin,
        invoke: &MockAuthInvoke {
            contract: &id,
            fn_name: "withdraw_bid",
            args: (bid_id.clone(),).into_val(&env),
            sub_invokes: &[],
        },
    }]);

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = client.withdraw_bid(&bid_id);
    }));
    assert!(
        result.is_err(),
        "admin must not be able to withdraw bids (no override)"
    );

    let bid = client.get_bid(&bid_id).unwrap();
    assert_eq!(bid.status, BidStatus::Placed);
}

/// Investor A cannot withdraw investor B's bid on the same invoice.
#[test]
fn test_different_investor_cannot_withdraw_others_bid() {
    let env = Env::default();
    let id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &id);

    env.mock_all_auths();
    let admin = Address::generate(&env);
    client.set_admin(&admin);
    let business = Address::generate(&env);
    client.submit_kyc_application(&business, &String::from_str(&env, "kyc"));
    client.verify_business(&admin, &business);

    // Place bid from investor A
    let (bid_id_a, _, _) = place_bid(&env, &client, &admin, &business);

    // Investor B tries to withdraw investor A's bid
    let investor_b = Address::generate(&env);
    env.mock_auths(&[MockAuth {
        address: &investor_b,
        invoke: &MockAuthInvoke {
            contract: &id,
            fn_name: "withdraw_bid",
            args: (bid_id_a.clone(),).into_val(&env),
            sub_invokes: &[],
        },
    }]);

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = client.withdraw_bid(&bid_id_a);
    }));
    assert!(
        result.is_err(),
        "investor B must not withdraw investor A's bid"
    );

    let bid = client.get_bid(&bid_id_a).unwrap();
    assert_eq!(bid.status, BidStatus::Placed);
}

// ===========================================================================
// 4. RANKING & SELECTION - withdrawn bids excluded
// ===========================================================================

#[test]
fn test_withdrawn_bid_excluded_from_get_best_bid() {
    let (env, client, admin, business) = setup();
    let (bid_id_a, investor_a, invoice_id) = place_bid(&env, &client, &admin, &business);

    // Place a second bid with lower profit
    let investor_b = Address::generate(&env);
    client.submit_investor_kyc(&investor_b, &String::from_str(&env, "kyc"));
    client.verify_investor(&admin, &investor_b, &10_000i128);
    let bid_id_b = client.place_bid(&investor_b, &invoice_id, &800i128, &900i128);

    // Bid A has higher profit (950 - 900 = 50) than B (900 - 800 = 100)
    // Actually B has higher profit. Let me recalculate: A is (950-900)=50, B is (900-800)=100
    // So B should be best before withdraw.

    // Actually the numbers show A: bid=900, return=950 (profit=50), B: bid=800, return=900 (profit=100)
    // So B is the best bid. If we withdraw A, B should still be best.
    // Let's verify: best_bid before = B
    let best_before = client.get_best_bid(&invoice_id);
    assert_eq!(
        best_before.as_ref().map(|b| &b.bid_id),
        Some(&bid_id_b),
        "B should be best bid (higher profit)"
    );

    // Withdraw B
    client.withdraw_bid(&bid_id_b).unwrap();

    // Now A should be best (only Placed bid left)
    let best_after = client.get_best_bid(&invoice_id);
    assert_eq!(
        best_after.as_ref().map(|b| &b.bid_id),
        Some(&bid_id_a),
        "A should be best bid after B withdrawn"
    );

    // Verify B is withdrawn
    let bid_b = client.get_bid(&bid_id_b).unwrap();
    assert_eq!(bid_b.status, BidStatus::Withdrawn);
}

#[test]
fn test_withdrawn_bid_excluded_from_rank_bids() {
    let (env, client, admin, business) = setup();
    let (bid_id_a, _, invoice_id) = place_bid(&env, &client, &admin, &business);

    // Place two more bids
    let investor_b = Address::generate(&env);
    client.submit_investor_kyc(&investor_b, &String::from_str(&env, "kyc"));
    client.verify_investor(&admin, &investor_b, &10_000i128);
    let bid_id_b = client.place_bid(&investor_b, &invoice_id, &800i128, &900i128);

    let investor_c = Address::generate(&env);
    client.submit_investor_kyc(&investor_c, &String::from_str(&env, "kyc"));
    client.verify_investor(&admin, &investor_c, &10_000i128);
    let bid_id_c = client.place_bid(&investor_c, &invoice_id, &700i128, &850i128);

    // Get ranked bids before withdrawal
    let ranked_before = client.get_ranked_bids(&invoice_id);
    assert_eq!(ranked_before.len(), 3, "should have 3 bids before withdrawal");
    assert!(
        ranked_before.iter().any(|b| b.bid_id == bid_id_a),
        "A should be in ranking"
    );
    assert!(
        ranked_before.iter().any(|b| b.bid_id == bid_id_b),
        "B should be in ranking"
    );
    assert!(
        ranked_before.iter().any(|b| b.bid_id == bid_id_c),
        "C should be in ranking"
    );

    // Withdraw B
    client.withdraw_bid(&bid_id_b).unwrap();

    // Get ranked bids after withdrawal
    let ranked_after = client.get_ranked_bids(&invoice_id);
    assert_eq!(ranked_after.len(), 2, "should have 2 bids after B withdrawn");
    assert!(
        ranked_after.iter().any(|b| b.bid_id == bid_id_a),
        "A should still be in ranking"
    );
    assert!(
        !ranked_after.iter().any(|b| b.bid_id == bid_id_b),
        "B should NOT be in ranking after withdrawal"
    );
    assert!(
        ranked_after.iter().any(|b| b.bid_id == bid_id_c),
        "C should still be in ranking"
    );

    // Verify all remaining are Placed
    for bid in ranked_after.iter() {
        assert_eq!(bid.status, BidStatus::Placed);
    }
}

#[test]
fn test_all_withdrawn_bids_results_empty_ranking() {
    let (env, client, admin, business) = setup();
    let (bid_id_a, _, invoice_id) = place_bid(&env, &client, &admin, &business);

    let investor_b = Address::generate(&env);
    client.submit_investor_kyc(&investor_b, &String::from_str(&env, "kyc"));
    client.verify_investor(&admin, &investor_b, &10_000i128);
    let bid_id_b = client.place_bid(&investor_b, &invoice_id, &800i128, &900i128);

    // Withdraw all bids
    client.withdraw_bid(&bid_id_a).unwrap();
    client.withdraw_bid(&bid_id_b).unwrap();

    // Ranking should be empty
    let ranked = client.get_ranked_bids(&invoice_id);
    assert_eq!(ranked.len(), 0, "ranking should be empty when all bids withdrawn");

    // Best bid should be None
    let best = client.get_best_bid(&invoice_id);
    assert!(best.is_none(), "best bid should be None when all bids withdrawn");
}

// ===========================================================================
// 5. INDEX & STATE CONSISTENCY
// ===========================================================================

#[test]
fn test_bids_by_status_reflects_withdrawn_transition() {
    let (env, client, admin, business) = setup();
    let (bid_id, _, invoice_id) = place_bid(&env, &client, &admin, &business);

    // Check that bid is in Placed status initially
    let placed_before = client.get_bids_by_status(&invoice_id, &BidStatus::Placed);
    assert!(
        placed_before.iter().any(|b| b.bid_id == bid_id),
        "bid should be in Placed status"
    );

    let withdrawn_before = client.get_bids_by_status(&invoice_id, &BidStatus::Withdrawn);
    assert!(
        !withdrawn_before.iter().any(|b| b.bid_id == bid_id),
        "bid should not be in Withdrawn status initially"
    );

    // Withdraw the bid
    client.withdraw_bid(&bid_id).unwrap();

    // Check that bid moved to Withdrawn
    let placed_after = client.get_bids_by_status(&invoice_id, &BidStatus::Placed);
    assert!(
        !placed_after.iter().any(|b| b.bid_id == bid_id),
        "bid should no longer be in Placed status"
    );

    let withdrawn_after = client.get_bids_by_status(&invoice_id, &BidStatus::Withdrawn);
    assert!(
        withdrawn_after.iter().any(|b| b.bid_id == bid_id),
        "bid should now be in Withdrawn status"
    );
}

#[test]
fn test_withdraw_bid_preserves_all_fields_except_status() {
    let (env, client, admin, business) = setup();
    let (bid_id, investor, _) = place_bid(&env, &client, &admin, &business);

    let bid_before = client.get_bid(&bid_id).unwrap();
    client.withdraw_bid(&bid_id).unwrap();
    let bid_after = client.get_bid(&bid_id).unwrap();

    assert_eq!(bid_after.bid_id, bid_before.bid_id);
    assert_eq!(bid_after.invoice_id, bid_before.invoice_id);
    assert_eq!(bid_after.investor, bid_before.investor);
    assert_eq!(bid_after.bid_amount, bid_before.bid_amount);
    assert_eq!(bid_after.expected_return, bid_before.expected_return);
    assert_eq!(bid_after.timestamp, bid_before.timestamp);
    assert_eq!(
        bid_after.status,
        BidStatus::Withdrawn,
        "only status should change"
    );
}

#[test]
fn test_withdraw_does_not_affect_other_bids_on_same_invoice() {
    let (env, client, admin, business) = setup();
    let (bid_id_a, _, invoice_id) = place_bid(&env, &client, &admin, &business);

    // Place a second bid from a different investor
    let investor_b = Address::generate(&env);
    client.submit_investor_kyc(&investor_b, &String::from_str(&env, "kyc"));
    client.verify_investor(&admin, &investor_b, &10_000i128);
    let bid_id_b = client.place_bid(&investor_b, &invoice_id, &800i128, &850i128);

    // Withdraw only bid A
    client.withdraw_bid(&bid_id_a).unwrap();

    // Bid B must still be Placed
    let bid_b = client.get_bid(&bid_id_b).unwrap();
    assert_eq!(
        bid_b.status, BidStatus::Placed,
        "withdrawing bid A must not affect bid B"
    );
}

// ===========================================================================
// 6. EDGE CASES
// ===========================================================================

#[test]
fn test_withdraw_expired_but_not_cleaned_bid() {
    let (env, client, admin, business) = setup();
    let (bid_id, _, _) = place_bid(&env, &client, &admin, &business);

    // Advance time past TTL
    env.ledger().set_timestamp(env.ledger().timestamp() + 604_801);

    // Bid is now Expired but hasn't been cleaned up yet
    let bid = client.get_bid(&bid_id).unwrap();
    assert_eq!(
        bid.status, BidStatus::Expired,
        "bid should be Expired after TTL"
    );

    // Try to withdraw - should fail because it's not Placed
    let result = client.withdraw_bid(&bid_id);
    assert!(result.is_err(), "cannot withdraw an Expired bid");
    assert_eq!(result.unwrap_err(), QuickLendXError::OperationNotAllowed);
}

#[test]
fn test_withdraw_then_place_new_bid_from_same_investor() {
    let (env, client, admin, business) = setup();
    let (bid_id_1, investor, invoice_id) = place_bid(&env, &client, &admin, &business);

    // Withdraw the first bid
    client.withdraw_bid(&bid_id_1).unwrap();
    let bid_1 = client.get_bid(&bid_id_1).unwrap();
    assert_eq!(bid_1.status, BidStatus::Withdrawn);

    // Place a new bid from the same investor on the same invoice
    let bid_id_2 = client.place_bid(&investor, &invoice_id, &850i128, &920i128);

    // New bid should be Placed
    let bid_2 = client.get_bid(&bid_id_2).unwrap();
    assert_eq!(bid_2.status, BidStatus::Placed);
    assert_ne!(
        bid_id_1, bid_id_2,
        "withdrawn and new bid should be different"
    );

    // New bid should be in rankings, old withdrawn bid should not
    let ranked = client.get_ranked_bids(&invoice_id);
    assert!(
        ranked.iter().any(|b| b.bid_id == bid_id_2),
        "new bid should be in ranking"
    );
    assert!(
        !ranked.iter().any(|b| b.bid_id == bid_id_1),
        "withdrawn bid should not be in ranking"
    );
}

#[test]
fn test_cannot_accept_withdrawn_bid() {
    let (env, client, admin, business) = setup();
    let (bid_id, _, invoice_id) = place_bid(&env, &client, &admin, &business);

    // Withdraw the bid
    client.withdraw_bid(&bid_id).unwrap();

    // Try to accept the withdrawn bid - should fail
    let result = client.try_accept_bid(&invoice_id, &bid_id);
    assert!(
        result.is_err(),
        "cannot accept a Withdrawn bid"
    );
}

#[test]
fn test_withdraw_and_cancel_are_different_terminal_states() {
    let (env, client, admin, business) = setup();

    // Bid 1 - withdraw
    let (bid_id_withdraw, _, invoice_id) = place_bid(&env, &client, &admin, &business);
    client.withdraw_bid(&bid_id_withdraw).unwrap();
    let bid_withdrawn = client.get_bid(&bid_id_withdraw).unwrap();
    assert_eq!(bid_withdrawn.status, BidStatus::Withdrawn);

    // Bid 2 - cancel
    let investor_b = Address::generate(&env);
    client.submit_investor_kyc(&investor_b, &String::from_str(&env, "kyc"));
    client.verify_investor(&admin, &investor_b, &10_000i128);
    let bid_id_cancel = client.place_bid(&investor_b, &invoice_id, &800i128, &850i128);
    client.cancel_bid(&bid_id_cancel);
    let bid_cancelled = client.get_bid(&bid_id_cancel).unwrap();
    assert_eq!(bid_cancelled.status, BidStatus::Cancelled);

    // Both should be excluded from ranking
    let ranked = client.get_ranked_bids(&invoice_id);
    assert_eq!(
        ranked.len(),
        0,
        "both Withdrawn and Cancelled should be excluded from ranking"
    );
    assert!(
        !ranked.iter().any(|b| b.bid_id == bid_id_withdraw),
        "Withdrawn bid not in ranking"
    );
    assert!(
        !ranked.iter().any(|b| b.bid_id == bid_id_cancel),
        "Cancelled bid not in ranking"
    );
}
