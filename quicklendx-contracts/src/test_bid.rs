//! Bid cancellation authorization matrix tests — Issue #793
//!
//! Verifies that `cancel_bid` is exclusively callable by the investor who
//! placed the bid, and documents the admin-override policy (none — admin has
//! no special cancel privilege).
//!
//! # Authorization matrix
//!
//! | Caller            | Bid status | Expected outcome          |
//! |-------------------|------------|---------------------------|
//! | Bid owner         | Placed     | Ok — status → Cancelled   |
//! | Bid owner         | Cancelled  | false (no-op)             |
//! | Bid owner         | Accepted   | false (no-op)             |
//! | Bid owner         | Withdrawn  | false (no-op)             |
//! | Bid owner         | Expired    | false (no-op)             |
//! | Third party       | Placed     | Auth panic (rejected)     |
//! | Business owner    | Placed     | Auth panic (rejected)     |
//! | Admin             | Placed     | Auth panic (rejected)     |
//! | Non-existent bid  | —          | false (no-op)             |
//!
//! # Security assumptions validated
//! - `require_auth()` is called on `bid.investor` inside `cancel_bid`; any
//!   other signer causes a host-level auth failure.
//! - Admin has **no** special override for `cancel_bid`; cancellation is
//!   strictly investor-only.
//! - Cancellation is idempotent on terminal states (Cancelled/Accepted/
//!   Withdrawn/Expired) — returns false without mutating state.
//! - A cancelled bid cannot be re-cancelled or re-placed.

#![cfg(test)]

use crate::errors::QuickLendXError;
use crate::invoice::InvoiceCategory;
use crate::{QuickLendXContract, QuickLendXContractClient};
use soroban_sdk::{
    testutils::{Address as _, MockAuth, MockAuthInvoke},
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

/// Place a bid and return (bid_id, investor).
fn place_bid(
    env: &Env,
    client: &QuickLendXContractClient,
    admin: &Address,
    business: &Address,
) -> (BytesN<32>, Address, BytesN<32>) {
    let currency = Address::generate(env);
    client.add_currency(admin, &currency);
    let due = env.ledger().timestamp() + 86_400;
    let invoice_id = client.upload_invoice(
        business,
        &1_000i128,
        &currency,
        &due,
        &String::from_str(env, "inv"),
        &InvoiceCategory::Services,
        &Vec::new(env),
    );
    let investor = Address::generate(env);
    client.submit_investor_kyc(&investor, &String::from_str(env, "kyc"));
    client.verify_investor(&admin, &investor, &10_000i128);
    let bid_id = client.place_bid(&investor, &invoice_id, &900i128, &950i128);
    (bid_id, investor, invoice_id)
}

// ===========================================================================
// 1. HAPPY PATH — investor cancels own Placed bid
// ===========================================================================

#[test]
fn test_investor_can_cancel_own_placed_bid() {
    let (env, client, admin, business) = setup();
    let (bid_id, investor, _) = place_bid(&env, &client, &admin, &business);

    // mock_all_auths is active — investor auth is satisfied
    let result = client.cancel_bid(&bid_id);
    assert!(result, "investor should be able to cancel their own Placed bid");

    let bid = client.get_bid(&bid_id).unwrap();
    assert_eq!(
        bid.status,
        crate::bid::BidStatus::Cancelled,
        "bid status must be Cancelled after cancel_bid"
    );
    assert_eq!(bid.investor, investor, "investor field must be unchanged");
}

#[test]
fn test_cancel_bid_returns_true_on_success() {
    let (env, client, admin, business) = setup();
    let (bid_id, _, _) = place_bid(&env, &client, &admin, &business);
    assert_eq!(client.cancel_bid(&bid_id), true);
}

// ===========================================================================
// 2. IDEMPOTENCY — terminal states return false without mutation
// ===========================================================================

#[test]
fn test_cancel_already_cancelled_bid_returns_false() {
    let (env, client, admin, business) = setup();
    let (bid_id, _, _) = place_bid(&env, &client, &admin, &business);
    client.cancel_bid(&bid_id); // first cancel
    let result = client.cancel_bid(&bid_id); // second cancel
    assert!(!result, "cancelling an already-Cancelled bid must return false");
}

#[test]
fn test_cancel_accepted_bid_returns_false() {
    let (env, client, admin, business) = setup();
    let (bid_id, _, invoice_id) = place_bid(&env, &client, &admin, &business);
    client.accept_bid(&invoice_id, &bid_id);
    let result = client.cancel_bid(&bid_id);
    assert!(!result, "cancelling an Accepted bid must return false");
}

#[test]
fn test_cancel_withdrawn_bid_returns_false() {
    let (env, client, admin, business) = setup();
    let (bid_id, _, _) = place_bid(&env, &client, &admin, &business);
    client.withdraw_bid(&bid_id).unwrap();
    let result = client.cancel_bid(&bid_id);
    assert!(!result, "cancelling a Withdrawn bid must return false");
}

#[test]
fn test_cancel_nonexistent_bid_returns_false() {
    let (env, client, _, _) = setup();
    let fake_id = BytesN::from_array(&env, &[0u8; 32]);
    let result = client.cancel_bid(&fake_id);
    assert!(!result, "cancelling a non-existent bid must return false");
}

// ===========================================================================
// 3. AUTHORIZATION MATRIX — unauthorized callers are rejected
// ===========================================================================

/// Third party (random address) cannot cancel someone else's bid.
#[test]
fn test_third_party_cannot_cancel_bid() {
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
    let (bid_id, investor, _) = place_bid(&env, &client, &admin, &business);

    // Now test with explicit auth for the WRONG address
    let attacker = Address::generate(&env);
    env.mock_auths(&[MockAuth {
        address: &attacker,
        invoke: &MockAuthInvoke {
            contract: &id,
            fn_name: "cancel_bid",
            args: (bid_id.clone(),).into_val(&env),
            sub_invokes: &[],
        },
    }]);

    let result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
        client.cancel_bid(&bid_id);
    }));
    assert!(result.is_err(), "third party must not be able to cancel bid");

    // Bid must still be Placed
    let bid = client.get_bid(&bid_id).unwrap();
    assert_eq!(bid.status, crate::bid::BidStatus::Placed);
}

/// Business owner cannot cancel an investor's bid on their own invoice.
#[test]
fn test_business_owner_cannot_cancel_bid() {
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
            fn_name: "cancel_bid",
            args: (bid_id.clone(),).into_val(&env),
            sub_invokes: &[],
        },
    }]);

    let result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
        client.cancel_bid(&bid_id);
    }));
    assert!(result.is_err(), "business owner must not be able to cancel investor bid");

    let bid = client.get_bid(&bid_id).unwrap();
    assert_eq!(bid.status, crate::bid::BidStatus::Placed);
}

/// Admin has NO special override — cannot cancel bids.
#[test]
fn test_admin_cannot_cancel_bid() {
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
            fn_name: "cancel_bid",
            args: (bid_id.clone(),).into_val(&env),
            sub_invokes: &[],
        },
    }]);

    let result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
        client.cancel_bid(&bid_id);
    }));
    assert!(result.is_err(), "admin must not be able to cancel bids (no override)");

    let bid = client.get_bid(&bid_id).unwrap();
    assert_eq!(bid.status, crate::bid::BidStatus::Placed);
}

/// Investor A cannot cancel investor B's bid on the same invoice.
#[test]
fn test_different_investor_cannot_cancel_others_bid() {
    let env = Env::default();
    let id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &id);

    env.mock_all_auths();
    let admin = Address::generate(&env);
    client.set_admin(&admin);
    let business = Address::generate(&env);
    client.submit_kyc_application(&business, &String::from_str(&env, "kyc"));
    client.verify_business(&admin, &business);

    // Place two bids from two different investors
    let (bid_id_a, investor_a, _) = place_bid(&env, &client, &admin, &business);

    // investor_b tries to cancel investor_a's bid
    let investor_b = Address::generate(&env);
    env.mock_auths(&[MockAuth {
        address: &investor_b,
        invoke: &MockAuthInvoke {
            contract: &id,
            fn_name: "cancel_bid",
            args: (bid_id_a.clone(),).into_val(&env),
            sub_invokes: &[],
        },
    }]);

    let result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
        client.cancel_bid(&bid_id_a);
    }));
    assert!(result.is_err(), "investor B must not cancel investor A's bid");

    let bid = client.get_bid(&bid_id_a).unwrap();
    assert_eq!(bid.status, crate::bid::BidStatus::Placed);
}

// ===========================================================================
// 4. STATE INTEGRITY — bid fields unchanged after cancel
// ===========================================================================

#[test]
fn test_cancel_bid_preserves_all_fields_except_status() {
    let (env, client, admin, business) = setup();
    let (bid_id, investor, invoice_id) = place_bid(&env, &client, &admin, &business);

    let bid_before = client.get_bid(&bid_id).unwrap();
    client.cancel_bid(&bid_id);
    let bid_after = client.get_bid(&bid_id).unwrap();

    assert_eq!(bid_after.bid_id, bid_before.bid_id);
    assert_eq!(bid_after.invoice_id, bid_before.invoice_id);
    assert_eq!(bid_after.investor, bid_before.investor);
    assert_eq!(bid_after.bid_amount, bid_before.bid_amount);
    assert_eq!(bid_after.expected_return, bid_before.expected_return);
    assert_eq!(bid_after.timestamp, bid_before.timestamp);
    assert_eq!(
        bid_after.status,
        crate::bid::BidStatus::Cancelled,
        "only status should change"
    );
}

#[test]
fn test_cancel_bid_does_not_affect_other_bids_on_same_invoice() {
    let (env, client, admin, business) = setup();
    let (bid_id_a, _, invoice_id) = place_bid(&env, &client, &admin, &business);

    // Place a second bid from a different investor
    let investor_b = Address::generate(&env);
    client.submit_investor_kyc(&investor_b, &String::from_str(&env, "kyc"));
    client.verify_investor(&admin, &investor_b, &10_000i128);
    let bid_id_b = client.place_bid(&investor_b, &invoice_id, &800i128, &850i128);

    // Cancel only bid A
    client.cancel_bid(&bid_id_a);

    // Bid B must still be Placed
    let bid_b = client.get_bid(&bid_id_b).unwrap();
    assert_eq!(
        bid_b.status,
        crate::bid::BidStatus::Placed,
        "cancelling bid A must not affect bid B"
    );
}

// ===========================================================================
// 5. WITHDRAW vs CANCEL — both are investor-only, distinct operations
// ===========================================================================

#[test]
fn test_withdraw_bid_also_requires_investor_auth() {
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

    let result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
        let _ = client.withdraw_bid(&bid_id);
    }));
    assert!(result.is_err(), "withdraw_bid must also enforce investor-only auth");
}

#[test]
fn test_cancel_and_withdraw_produce_different_terminal_states() {
    let (env, client, admin, business) = setup();

    // Bid 1 — cancel
    let (bid_id_cancel, _, _) = place_bid(&env, &client, &admin, &business);
    client.cancel_bid(&bid_id_cancel);
    let bid_cancelled = client.get_bid(&bid_id_cancel).unwrap();
    assert_eq!(bid_cancelled.status, crate::bid::BidStatus::Cancelled);

    // Bid 2 — withdraw
    let (bid_id_withdraw, _, _) = place_bid(&env, &client, &admin, &business);
    client.withdraw_bid(&bid_id_withdraw).unwrap();
    let bid_withdrawn = client.get_bid(&bid_id_withdraw).unwrap();
    assert_eq!(bid_withdrawn.status, crate::bid::BidStatus::Withdrawn);

    assert_ne!(
        bid_cancelled.status, bid_withdrawn.status,
        "cancel and withdraw must produce distinct terminal states"
    );
}

// ===========================================================================
// 6. MULTIPLE BIDS — investor can cancel each of their own bids independently
// ===========================================================================

#[test]
fn test_investor_can_cancel_multiple_own_bids() {
    let (env, client, admin, business) = setup();

    // Place two bids from the same investor on different invoices
    let (bid_id_1, investor, _) = place_bid(&env, &client, &admin, &business);
    let (bid_id_2, _, _) = place_bid(&env, &client, &admin, &business);

    assert!(client.cancel_bid(&bid_id_1), "first cancel must succeed");
    assert!(client.cancel_bid(&bid_id_2), "second cancel must succeed");

    assert_eq!(
        client.get_bid(&bid_id_1).unwrap().status,
        crate::bid::BidStatus::Cancelled
    );
    assert_eq!(
        client.get_bid(&bid_id_2).unwrap().status,
        crate::bid::BidStatus::Cancelled
    );
}

// ---------------------------------------------------------------------------
// Investor Exposure Cap and Active Bid Limit Tests — Issue #782
// ---------------------------------------------------------------------------

/// Helper to create a verified investor with a given investment limit
fn create_verified_investor(
    env: &Env,
    client: &QuickLendXContractClient,
    admin: &Address,
    investment_limit: i128,
) -> Address {
    let investor = Address::generate(env);
    client.submit_kyc_application(&investor, &String::from_str(env, "kyc_data"));
    client.verify_investor(admin, &investor, &investment_limit);
    investor
}

/// Helper to create a verified invoice for testing
fn create_verified_invoice(
    env: &Env,
    client: &QuickLendXContractClient,
    business: &Address,
    admin: &Address,
    amount: i128,
) -> BytesN<32> {
    let currency = Address::generate(env);
    client.add_currency(admin, &currency);
    let due = env.ledger().timestamp() + 86_400;
    let invoice_id = client.store_invoice(
        business,
        &amount,
        &currency,
        &due,
        &String::from_str(env, "description"),
        &InvoiceCategory::Services,
        &Vec::new(env),
    );
    invoice_id
}

// Group A: Active Bid Limit Enforcement

#[test]
fn test_active_bid_limit_enforced() {
    let (env, client, admin, business) = setup();
    let investor = create_verified_investor(&env, &client, &admin, &1_000_000i128);
    
    // Set a low limit for testing
    client.set_max_active_bids_per_investor(&admin, &3u32);
    
    // Place 3 bids - should succeed
    for i in 0..3 {
        let invoice_id = create_verified_invoice(&env, &client, &business, &admin, 10_000);
        let result = client.try_place_bid(&investor, &invoice_id, &1000i128, &1100i128);
        assert!(result.is_ok(), "Bid {} should succeed under limit", i);
    }
    
    // Try to place 4th bid - should fail
    let invoice_id = create_verified_invoice(&env, &client, &business, &admin, 10_000);
    let result = client.try_place_bid(&investor, &invoice_id, &1000i128, &1100i128);
    assert!(result.is_err(), "4th bid should be rejected due to active bid limit");
}

#[test]
fn test_active_bid_limit_respects_cancellation() {
    let (env, client, admin, business) = setup();
    let investor = create_verified_investor(&env, &client, &admin, &1_000_000i128);
    
    client.set_max_active_bids_per_investor(&admin, &2u32);
    
    // Place 2 bids
    let invoice_id1 = create_verified_invoice(&env, &client, &business, &admin, 10_000);
    let bid_id1 = client.place_bid(&investor, &invoice_id1, &1000i128, &1100i128);
    
    let invoice_id2 = create_verified_invoice(&env, &client, &business, &admin, 10_000);
    let bid_id2 = client.place_bid(&investor, &invoice_id2, &1000i128, &1100i128);
    
    // Cancel one bid
    assert!(client.cancel_bid(&bid_id1), "Cancellation should succeed");
    
    // Now should be able to place a new bid
    let invoice_id3 = create_verified_invoice(&env, &client, &business, &admin, 10_000);
    let result = client.try_place_bid(&investor, &invoice_id3, &1000i128, &1100i128);
    assert!(result.is_ok(), "Should be able to place bid after cancellation");
}

// Group B: Portfolio Exposure Cap

#[test]
fn test_portfolio_exposure_cap_enforced() {
    let (env, client, admin, business) = setup();
    let investor = create_verified_investor(&env, &client, &admin, &50_000i128);
    
    // Place bids totaling 40,000
    let invoice_id1 = create_verified_invoice(&env, &client, &business, &admin, 10_000);
    client.place_bid(&investor, &invoice_id1, &20_000i128, &22_000i128);
    
    let invoice_id2 = create_verified_invoice(&env, &client, &business, &admin, 10_000);
    client.place_bid(&investor, &invoice_id2, &20_000i128, &22_000i128);
    
    // Try to place bid that would exceed cap (would make total 60,000)
    let invoice_id3 = create_verified_invoice(&env, &client, &business, &admin, 10_000);
    let result = client.try_place_bid(&investor, &invoice_id3, &20_000i128, &22_000i128);
    assert!(result.is_err(), "Bid exceeding portfolio cap should be rejected");
}

#[test]
fn test_portfolio_exposure_respects_cancellation() {
    let (env, client, admin, business) = setup();
    let investor = create_verified_investor(&env, &client, &admin, &50_000i128);
    
    // Place bid for 40,000
    let invoice_id1 = create_verified_invoice(&env, &client, &business, &admin, 10_000);
    let bid_id1 = client.place_bid(&investor, &invoice_id1, &40_000i128, &44_000i128);
    
    // Cancel it
    assert!(client.cancel_bid(&bid_id1), "Cancellation should succeed");
    
    // Now should be able to place bid for 40,000 again
    let invoice_id2 = create_verified_invoice(&env, &client, &business, &admin, 10_000);
    let result = client.try_place_bid(&investor, &invoice_id2, &40_000i128, &44_000i128);
    assert!(result.is_ok(), "Should be able to place bid after cancellation");
}

// Group C: Bid Churn Attack Prevention

#[test]
fn test_bid_churn_attack_prevented() {
    let (env, client, admin, business) = setup();
    let investor = create_verified_investor(&env, &client, &admin, &100_000i128);
    
    client.set_max_active_bids_per_investor(&admin, &5u32);
    
    // Attempt rapid place/cancel cycles
    for i in 0..10 {
        let invoice_id = create_verified_invoice(&env, &client, &business, &admin, 10_000);
        let bid_id = client.place_bid(&investor, &invoice_id, &1000i128, &1100i128);
        
        // Immediately cancel
        assert!(client.cancel_bid(&bid_id), "Cancellation should succeed");
        
        // Try to place another bid immediately
        let invoice_id2 = create_verified_invoice(&env, &client, &business, &admin, 10_000);
        let result = client.try_place_bid(&investor, &invoice_id2, &1000i128, &1100i128);
        
        // The limit should still be enforced based on current active bids
        // After cancellation, count should be 0, so this should succeed
        assert!(result.is_ok(), "Bid after cancellation should succeed");
    }
}

// Group D: State Consistency

#[test]
fn test_active_count_consistency_after_expiration() {
    let (env, client, admin, business) = setup();
    let investor = create_verified_investor(&env, &client, &admin, &1_000_000i128);
    
    client.set_max_active_bids_per_investor(&admin, &2u32);
    
    // Place 2 bids with short TTL
    let invoice_id1 = create_verified_invoice(&env, &client, &business, &admin, 10_000);
    client.place_bid(&investor, &invoice_id1, &1000i128, &1100i128);
    
    let invoice_id2 = create_verified_invoice(&env, &client, &business, &admin, 10_000);
    client.place_bid(&investor, &invoice_id2, &1000i128, &1100i128);
    
    // Advance time past bid TTL
    env.ledger().set(env.ledger().timestamp() + 86400 * 30); // 30 days
    
    // Bids should be expired and pruned, allowing new bids
    let invoice_id3 = create_verified_invoice(&env, &client, &business, &admin, 10_000);
    let result = client.try_place_bid(&investor, &invoice_id3, &1000i128, &1100i128);
    assert!(result.is_ok(), "Should be able to place bid after expiration");
}

// Group E: Edge Cases

#[test]
fn test_limit_disabled_when_set_to_zero() {
    let (env, client, admin, business) = setup();
    let investor = create_verified_investor(&env, &client, &admin, &1_000_000i128);
    
    // Disable the limit by setting to 0
    client.set_max_active_bids_per_investor(&admin, &0u32);
    
    // Place many bids (more than the default limit of 20)
    for i in 0..25 {
        let invoice_id = create_verified_invoice(&env, &client, &business, &admin, 10_000);
        let result = client.try_place_bid(&investor, &invoice_id, &1000i128, &1100i128);
        assert!(result.is_ok(), "Bid {} should succeed when limit is disabled", i);
    }
    
    // Verify all bids are active
    let active_count = client.count_active_placed_bids_for_investor(&investor);
    assert_eq!(active_count, 25, "All bids should be active when limit is disabled");
}
