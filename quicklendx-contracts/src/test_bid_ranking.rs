//! Tests for bid ranking algorithm correctness (#158).
//!
//! Covers: empty bid list, single bid, multiple bids with correct sort order,
//! equal bids (tie-break), best bid selection, and non-existent invoice.

#![cfg(test)]
use core::cmp::Ordering;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, BytesN, Env, String, Vec,
};

use crate::bid::{Bid, BidStatus, BidStorage};
use crate::invoice::InvoiceCategory;
use crate::{QuickLendXContract, QuickLendXContractClient};

fn setup() -> (Env, QuickLendXContractClient<'static>) {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    (env, client)
}

fn add_verified_investor(env: &Env, client: &QuickLendXContractClient, limit: i128) -> Address {
    let investor = Address::generate(env);
    client.submit_investor_kyc(&investor, &String::from_str(env, "KYC"));
    client.verify_investor(&investor, &limit);
    investor
}

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

// =============================================================================
// Empty bid list
// =============================================================================

/// Empty bid list: get_ranked_bids returns empty, get_best_bid returns None.
#[test]
fn test_empty_bid_list() {
    let (env, client) = setup();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let _ = client.set_admin(&admin);
    let business = Address::generate(&env);

    let invoice_id = create_verified_invoice(&env, &client, &admin, &business, 10_000);

    let ranked = client.get_ranked_bids(&invoice_id);
    assert_eq!(ranked.len(), 0, "ranked bids must be empty when no bids");

    let best = client.get_best_bid(&invoice_id);
    assert!(best.is_none(), "best bid must be None when no bids");
}

/// Non-existent invoice: get_ranked_bids returns empty, get_best_bid returns None.
#[test]
fn test_empty_ranked_and_best_for_nonexistent_invoice() {
    let (env, client) = setup();
    env.mock_all_auths();
    let _ = client.set_admin(&Address::generate(&env));

    let invalid_invoice_id = BytesN::from_array(&env, &[0xff; 32]);

    let ranked = client.get_ranked_bids(&invalid_invoice_id);
    assert_eq!(
        ranked.len(),
        0,
        "ranked must be empty for non-existent invoice"
    );

    let best = client.get_best_bid(&invalid_invoice_id);
    assert!(
        best.is_none(),
        "best bid must be None for non-existent invoice"
    );
}

// =============================================================================
// Single bid
// =============================================================================

/// Single bid: get_ranked_bids has one element, get_best_bid is that bid.
#[test]
fn test_single_bid_ranking_and_best_selection() {
    let (env, client) = setup();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let _ = client.set_admin(&admin);
    let investor = add_verified_investor(&env, &client, 100_000);
    let business = Address::generate(&env);

    let invoice_id = create_verified_invoice(&env, &client, &admin, &business, 10_000);
    let bid_id = client.place_bid(&investor, &invoice_id, &5_000, &6_000);

    let ranked = client.get_ranked_bids(&invoice_id);
    assert_eq!(ranked.len(), 1, "ranked must contain single bid");
    assert_eq!(ranked.get(0).unwrap().bid_id, bid_id);

    let best = client.get_best_bid(&invoice_id);
    assert!(best.is_some());
    assert_eq!(best.as_ref().unwrap().bid_id, bid_id);
    assert_eq!(best.as_ref().unwrap().bid_id, ranked.get(0).unwrap().bid_id);
}

// =============================================================================
// Multiple bids – sorting order
// =============================================================================

/// Ranking with multiple bids: order by profit (desc), then expected_return (desc),
/// then bid_amount (desc), then timestamp (earlier wins).
#[test]
fn test_ranking_with_multiple_bids() {
    let (env, client) = setup();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let _ = client.set_admin(&admin);
    let inv_a = add_verified_investor(&env, &client, 100_000);
    let inv_b = add_verified_investor(&env, &client, 100_000);
    let inv_c = add_verified_investor(&env, &client, 100_000);
    let business = Address::generate(&env);

    let invoice_id = create_verified_invoice(&env, &client, &admin, &business, 100_000);

    // A: 10k -> 12k (profit 2k)
    let _ = client.place_bid(&inv_a, &invoice_id, &10_000, &12_000);
    // B: 15k -> 18k (profit 3k) – best
    let bid_b = client.place_bid(&inv_b, &invoice_id, &15_000, &18_000);
    // C: 12k -> 13k (profit 1k)
    let _ = client.place_bid(&inv_c, &invoice_id, &12_000, &13_000);

    let ranked = client.get_ranked_bids(&invoice_id);
    assert_eq!(ranked.len(), 3);
    assert_eq!(
        ranked.get(0).unwrap().investor,
        inv_b,
        "highest profit first"
    );
    assert_eq!(ranked.get(0).unwrap().bid_id, bid_b);

    let best = client.get_best_bid(&invoice_id).unwrap();
    assert_eq!(best.bid_id, ranked.get(0).unwrap().bid_id);
    assert_eq!(best.investor, inv_b);
}

/// Best bid selection: get_best_bid equals first element of get_ranked_bids.
#[test]
fn test_best_bid_equals_first_ranked() {
    let (env, client) = setup();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let _ = client.set_admin(&admin);
    let inv_a = add_verified_investor(&env, &client, 100_000);
    let inv_b = add_verified_investor(&env, &client, 100_000);
    let inv_c = add_verified_investor(&env, &client, 100_000);
    let business = Address::generate(&env);

    let invoice_id = create_verified_invoice(&env, &client, &admin, &business, 100_000);
    let _ = client.place_bid(&inv_a, &invoice_id, &7_000, &8_800);
    let _ = client.place_bid(&inv_b, &invoice_id, &8_000, &10_500);
    let _ = client.place_bid(&inv_c, &invoice_id, &9_000, &12_000);

    let ranked = client.get_ranked_bids(&invoice_id);
    let best = client.get_best_bid(&invoice_id);
    assert!(best.is_some());
    assert_eq!(best.as_ref().unwrap().bid_id, ranked.get(0).unwrap().bid_id);
    assert_eq!(
        best.as_ref().unwrap().investor,
        ranked.get(0).unwrap().investor
    );
}

// =============================================================================
// Equal bids – tie-break (deterministic order)
// =============================================================================

/// Equal bid amounts and expected return: tie-break by timestamp (earlier wins).
#[test]
fn test_equal_bids_tie_break_by_timestamp() {
    let (env, client) = setup();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let _ = client.set_admin(&admin);
    let inv_a = add_verified_investor(&env, &client, 100_000);
    let inv_b = add_verified_investor(&env, &client, 100_000);
    let business = Address::generate(&env);

    let invoice_id = create_verified_invoice(&env, &client, &admin, &business, 20_000);
    // Same terms: 10k -> 11k (profit 1k)
    let bid_id_a = client.place_bid(&inv_a, &invoice_id, &10_000, &11_000);
    env.ledger().set_timestamp(env.ledger().timestamp() + 1);
    let bid_id_b = client.place_bid(&inv_b, &invoice_id, &10_000, &11_000);

    let ranked = client.get_ranked_bids(&invoice_id);
    assert_eq!(ranked.len(), 2);
    // Earlier timestamp wins: A placed first, so A should be first
    assert_eq!(ranked.get(0).unwrap().bid_id, bid_id_a);
    assert_eq!(ranked.get(1).unwrap().bid_id, bid_id_b);

    let best = client.get_best_bid(&invoice_id).unwrap();
    assert_eq!(best.bid_id, bid_id_a);
}

// =============================================================================
// compare_bids unit tests (algorithm correctness)
// =============================================================================

/// compare_bids: higher profit => Greater for first bid.
#[test]
fn test_compare_bids_by_profit() {
    let env = Env::default();
    let bid_id = BytesN::from_array(&env, &[0; 32]);
    let invoice_id = BytesN::from_array(&env, &[0; 32]);
    let investor = Address::generate(&env);

    let high_profit = Bid {
        bid_id: bid_id.clone(),
        invoice_id: invoice_id.clone(),
        investor: investor.clone(),
        bid_amount: 1000,
        expected_return: 1500,
        timestamp: 100,
        status: BidStatus::Placed,
        expiration_timestamp: 1000,
    };
    let low_profit = Bid {
        bid_amount: 1000,
        expected_return: 1200,
        ..high_profit.clone()
    };

    assert_eq!(
        BidStorage::compare_bids(&high_profit, &low_profit),
        Ordering::Greater
    );
    assert_eq!(
        BidStorage::compare_bids(&low_profit, &high_profit),
        Ordering::Less
    );
}

/// compare_bids: same profit, higher expected_return => Greater.
#[test]
fn test_compare_bids_by_expected_return_when_profit_equal() {
    let env = Env::default();
    let bid_id = BytesN::from_array(&env, &[0; 32]);
    let invoice_id = BytesN::from_array(&env, &[0; 32]);
    let investor = Address::generate(&env);

    let a = Bid {
        bid_id: bid_id.clone(),
        invoice_id: invoice_id.clone(),
        investor: investor.clone(),
        bid_amount: 1000,
        expected_return: 1500,
        timestamp: 100,
        status: BidStatus::Placed,
        expiration_timestamp: 1000,
    };
    let b = Bid {
        bid_amount: 1200,
        expected_return: 1700, // same profit 500
        ..a.clone()
    };

    assert_eq!(BidStorage::compare_bids(&b, &a), Ordering::Greater);
    assert_eq!(BidStorage::compare_bids(&a, &b), Ordering::Less);
}

/// compare_bids: full tie, earlier timestamp => Greater (first-come advantage).
#[test]
fn test_compare_bids_tie_break_timestamp() {
    let env = Env::default();
    let bid_id = BytesN::from_array(&env, &[0; 32]);
    let invoice_id = BytesN::from_array(&env, &[0; 32]);
    let investor = Address::generate(&env);

    let earlier = Bid {
        bid_id: bid_id.clone(),
        invoice_id: invoice_id.clone(),
        investor: investor.clone(),
        bid_amount: 1000,
        expected_return: 1500,
        timestamp: 100,
        status: BidStatus::Placed,
        expiration_timestamp: 1000,
    };
    let later = Bid {
        timestamp: 200,
        ..earlier.clone()
    };

    // Earlier timestamp wins: earlier > later in our ordering
    assert_eq!(
        BidStorage::compare_bids(&earlier, &later),
        Ordering::Greater
    );
    assert_eq!(BidStorage::compare_bids(&later, &earlier), Ordering::Less);
}

/// compare_bids: identical bids => Equal.
#[test]
fn test_compare_bids_equal() {
    let env = Env::default();
    let bid_id = BytesN::from_array(&env, &[0; 32]);
    let invoice_id = BytesN::from_array(&env, &[0; 32]);
    let investor = Address::generate(&env);

    let bid = Bid {
        bid_id: bid_id.clone(),
        invoice_id: invoice_id.clone(),
        investor: investor.clone(),
        bid_amount: 1000,
        expected_return: 1500,
        timestamp: 100,
        status: BidStatus::Placed,
        expiration_timestamp: 1000,
    };
    assert_eq!(BidStorage::compare_bids(&bid, &bid), Ordering::Equal);
}

// =============================================================================
// Ranked list excludes non-Placed bids
// =============================================================================

/// get_ranked_bids and get_best_bid only consider Placed bids; withdrawn/expired excluded.
#[test]
fn test_ranked_excludes_withdrawn_and_expired() {
    let (env, client) = setup();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let _ = client.set_admin(&admin);
    let inv_a = add_verified_investor(&env, &client, 100_000);
    let inv_b = add_verified_investor(&env, &client, 100_000);
    let business = Address::generate(&env);

    let invoice_id = create_verified_invoice(&env, &client, &admin, &business, 20_000);
    let _ = client.place_bid(&inv_a, &invoice_id, &5_000, &6_000);
    let bid_b = client.place_bid(&inv_b, &invoice_id, &10_000, &12_000);

    let ranked_before = client.get_ranked_bids(&invoice_id);
    assert_eq!(ranked_before.len(), 2);
    client.withdraw_bid(&bid_b);

    let ranked_after = client.get_ranked_bids(&invoice_id);
    assert_eq!(ranked_after.len(), 1, "withdrawn bid must be excluded");
    assert_eq!(ranked_after.get(0).unwrap().investor, inv_a);

    let best = client.get_best_bid(&invoice_id).unwrap();
    assert_eq!(best.investor, inv_a);
}
