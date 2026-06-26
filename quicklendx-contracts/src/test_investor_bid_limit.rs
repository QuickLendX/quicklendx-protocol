//! Focused tests for the per-investor active-bid cap.
//!
//! Validates:
//! - Placing bids up to the cap succeeds; the next one is rejected.
//! - Expiring a bid frees a slot so a new bid succeeds.
//! - Cancelling a bid frees a slot so a new bid succeeds.
//! - Accepting a bid frees a slot so a new bid succeeds.
//! - `count_active_placed_bids_for_investor` counts only live `Placed` bids
//!   (excludes Withdrawn / Cancelled / Expired / Accepted).
//! - Setting limit to `INVESTOR_BID_LIMIT_DISABLED` (0) removes the cap.
//! - `reset_max_active_bids_per_investor` restores the compile-time default.
//!
//! Fixtures are modelled after `test_bid.rs` (simple currency address, no real
//! token needed) and `test_bid_ttl.rs` (ledger-time manipulation, real tokens
//! for the accept path).
//!
//! # Finding
//! No miscounts were observed during development of these tests.
//! `count_active_placed_bids_for_investor` correctly calls
//! `refresh_investor_bids` which prunes newly-expired entries before counting,
//! so the count always reflects live `Placed` bids only.

#![cfg(test)]

use super::*;
use crate::bid::{BidLimitConfig, BidStorage, BidStatus, INVESTOR_BID_LIMIT_DISABLED};
use crate::errors::QuickLendXError;
use crate::invoice::InvoiceCategory;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, BytesN, Env, String, Vec,
};

const SECS_PER_DAY: u64 = 86_400;

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

/// Minimal environment used for tests that do NOT need token transfers
/// (cancel / expire / count / limit-config scenarios).
fn setup() -> (Env, QuickLendXContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000_000);
    let id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &id);
    let admin = Address::generate(&env);
    client.set_admin(&admin);
    client.initialize_fee_system(&admin);
    (env, client, admin)
}

/// Mint a SAC token with sufficient balances/allowances for escrow tests.
fn make_token(env: &Env, contract_id: &Address, business: &Address, investor: &Address) -> Address {
    let token_admin = Address::generate(env);
    let currency = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();
    let sac = token::StellarAssetClient::new(env, &currency);
    let tok = token::Client::new(env, &currency);
    sac.mint(business, &500_000i128);
    sac.mint(investor, &500_000i128);
    sac.mint(contract_id, &1i128);
    let exp = env.ledger().sequence() + 100_000;
    tok.approve(investor, contract_id, &500_000i128, &exp);
    tok.approve(business, contract_id, &500_000i128, &exp);
    currency
}

/// Create a verified investor + verified invoice backed by a real SAC token.
/// Reuses the `funded_setup` shape from `test_bid_ttl.rs`.
fn funded_setup(
    env: &Env,
    client: &QuickLendXContractClient,
    admin: &Address,
) -> (Address, Address, BytesN<32>) {
    let business = Address::generate(env);
    let investor = Address::generate(env);
    let currency = make_token(env, &client.address, &business, &investor);

    client.submit_kyc_application(&business, &String::from_str(env, "KYC"));
    client.verify_business(admin, &business);
    client.submit_investor_kyc(&investor, &String::from_str(env, "KYC"));
    client.verify_investor(&investor, &500_000i128);
    client.add_currency(admin, &currency);

    let due = env.ledger().timestamp() + 30 * SECS_PER_DAY;
    let invoice_id = client.upload_invoice(
        &business,
        &10_000i128,
        &currency,
        &due,
        &String::from_str(env, "test invoice"),
        &InvoiceCategory::Services,
        &Vec::new(env),
    );
    client.verify_invoice(&invoice_id);
    (business, investor, invoice_id)
}

/// Create a *separate* verified invoice for `investor` using a plain (non-SAC)
/// currency address — sufficient for place/cancel/expire tests that never call
/// `accept_bid` (which triggers an escrow token transfer).
fn plain_invoice(
    env: &Env,
    client: &QuickLendXContractClient,
    admin: &Address,
    business: &Address,
    currency: &Address,
) -> BytesN<32> {
    let due = env.ledger().timestamp() + 30 * SECS_PER_DAY;
    let invoice_id = client.upload_invoice(
        business,
        &10_000i128,
        currency,
        &due,
        &String::from_str(env, "inv"),
        &InvoiceCategory::Services,
        &Vec::new(env),
    );
    client.verify_invoice(&invoice_id);
    invoice_id
}

// ---------------------------------------------------------------------------
// 1. Cap is enforced: the (cap+1)th bid must fail
// ---------------------------------------------------------------------------

#[test]
fn test_cap_blocks_next_bid() {
    let (env, client, admin) = setup();

    // Use a small cap so the test stays fast.
    client.set_max_active_bids_per_investor(&3u32).unwrap();

    let business = Address::generate(&env);
    client.submit_kyc_application(&business, &String::from_str(&env, "KYC"));
    client.verify_business(&admin, &business);
    let currency = Address::generate(&env);
    client.add_currency(&admin, &currency);

    let investor = Address::generate(&env);
    client.submit_investor_kyc(&investor, &String::from_str(&env, "KYC"));
    client.verify_investor(&investor, &1_000_000i128);

    // Place exactly `cap` bids on separate invoices (so the per-invoice cap is
    // never a confound).
    for _ in 0..3u32 {
        let inv = plain_invoice(&env, &client, &admin, &business, &currency);
        client.place_bid(&investor, &inv, &1_000i128, &1_100i128);
    }

    assert_eq!(
        BidStorage::count_active_placed_bids_for_investor(&env, &investor),
        3,
        "active count must equal cap"
    );

    let cfg = client.get_bid_limit_config();
    assert_eq!(cfg.limit, 3, "getter must expose the active cap");
    assert!(!cfg.is_disabled, "getter must report the cap is active");

    // The (cap+1)th bid must be rejected with MaxActiveBidsPerInvestorExceeded.
    let overflow_inv = plain_invoice(&env, &client, &admin, &business, &currency);
    let result = client.try_place_bid(&investor, &overflow_inv, &1_000i128, &1_100i128);
    assert!(result.is_err(), "bid beyond cap must be rejected");
    assert_eq!(
        result.unwrap_err().expect("expected contract error"),
        QuickLendXError::MaxActiveBidsPerInvestorExceeded,
        "error must be MaxActiveBidsPerInvestorExceeded"
    );
}

// ---------------------------------------------------------------------------
// 2. Expiry frees a slot
// ---------------------------------------------------------------------------

#[test]
fn test_expiry_frees_slot() {
    let (env, client, admin) = setup();
    client.set_max_active_bids_per_investor(&2u32).unwrap();
    // 1-day TTL so we can expire bids by advancing the ledger.
    client.set_bid_ttl_days(&1u64).unwrap();

    let business = Address::generate(&env);
    client.submit_kyc_application(&business, &String::from_str(&env, "KYC"));
    client.verify_business(&admin, &business);
    let currency = Address::generate(&env);
    client.add_currency(&admin, &currency);

    let investor = Address::generate(&env);
    client.submit_investor_kyc(&investor, &String::from_str(&env, "KYC"));
    client.verify_investor(&investor, &1_000_000i128);

    // Fill the cap.
    let inv0 = plain_invoice(&env, &client, &admin, &business, &currency);
    let inv1 = plain_invoice(&env, &client, &admin, &business, &currency);
    let bid0 = client.place_bid(&investor, &inv0, &1_000i128, &1_100i128);
    client.place_bid(&investor, &inv1, &1_000i128, &1_100i128);

    // Third bid must be blocked.
    let inv2 = plain_invoice(&env, &client, &admin, &business, &currency);
    assert!(
        client.try_place_bid(&investor, &inv2, &1_000i128, &1_100i128).is_err(),
        "cap must be enforced"
    );

    // Advance past the TTL of bid0 and sweep expired bids.
    let expiry = client.get_bid(&bid0).unwrap().expiration_timestamp;
    env.ledger().set_timestamp(expiry + 1);
    client.cleanup_expired_bids(&inv0);

    assert_eq!(
        client.get_bid(&bid0).unwrap().status,
        BidStatus::Expired,
        "bid0 must be Expired after TTL elapses"
    );
    assert_eq!(
        BidStorage::count_active_placed_bids_for_investor(&env, &investor),
        1,
        "count must drop to 1 after expiry"
    );

    // Now the third bid must succeed.
    assert!(
        client.try_place_bid(&investor, &inv2, &1_000i128, &1_100i128).is_ok(),
        "new bid must succeed after expiry freed a slot"
    );
}

// ---------------------------------------------------------------------------
// 3. Cancellation frees a slot
// ---------------------------------------------------------------------------

#[test]
fn test_cancellation_frees_slot() {
    let (env, client, admin) = setup();
    client.set_max_active_bids_per_investor(&2u32).unwrap();

    let business = Address::generate(&env);
    client.submit_kyc_application(&business, &String::from_str(&env, "KYC"));
    client.verify_business(&admin, &business);
    let currency = Address::generate(&env);
    client.add_currency(&admin, &currency);

    let investor = Address::generate(&env);
    client.submit_investor_kyc(&investor, &String::from_str(&env, "KYC"));
    client.verify_investor(&investor, &1_000_000i128);

    let inv0 = plain_invoice(&env, &client, &admin, &business, &currency);
    let inv1 = plain_invoice(&env, &client, &admin, &business, &currency);
    let inv2 = plain_invoice(&env, &client, &admin, &business, &currency);

    let bid0 = client.place_bid(&investor, &inv0, &1_000i128, &1_100i128);
    client.place_bid(&investor, &inv1, &1_000i128, &1_100i128);

    assert!(
        client.try_place_bid(&investor, &inv2, &1_000i128, &1_100i128).is_err(),
        "cap must be enforced"
    );

    // Cancel bid0 — transitions Placed → Cancelled.
    assert!(client.cancel_bid(&bid0), "cancel_bid must return true");
    assert_eq!(client.get_bid(&bid0).unwrap().status, BidStatus::Cancelled);

    assert_eq!(
        BidStorage::count_active_placed_bids_for_investor(&env, &investor),
        1,
        "count must drop to 1 after cancellation"
    );

    // Third bid now succeeds.
    assert!(
        client.try_place_bid(&investor, &inv2, &1_000i128, &1_100i128).is_ok(),
        "new bid must succeed after cancellation freed a slot"
    );
}

// ---------------------------------------------------------------------------
// 4. Acceptance frees a slot
// ---------------------------------------------------------------------------

#[test]
fn test_acceptance_frees_slot() {
    let (env, client, admin) = setup();
    client.set_max_active_bids_per_investor(&2u32).unwrap();

    // Use funded_setup (real SAC token) for the invoice that will be accepted.
    let (_, investor, inv_accepted) = funded_setup(&env, &client, &admin);

    // One extra plain invoice that shares the same currency (whitelist already set).
    let currency = client
        .get_invoice(&inv_accepted)
        .unwrap()
        .currency;
    let business_plain = Address::generate(&env);
    client.submit_kyc_application(&business_plain, &String::from_str(&env, "KYC"));
    client.verify_business(&admin, &business_plain);

    let inv_plain = plain_invoice(&env, &client, &admin, &business_plain, &currency);
    let inv_new   = plain_invoice(&env, &client, &admin, &business_plain, &currency);

    // Fill cap: one bid on the to-be-accepted invoice, one on the plain invoice.
    let bid_accept = client.place_bid(&investor, &inv_accepted, &5_000i128, &5_500i128);
    client.place_bid(&investor, &inv_plain, &1_000i128, &1_100i128);

    assert!(
        client.try_place_bid(&investor, &inv_new, &1_000i128, &1_100i128).is_err(),
        "cap must be enforced"
    );

    // Business accepts bid_accept — transitions Placed → Accepted.
    client.accept_bid(&inv_accepted, &bid_accept).unwrap();
    assert_eq!(
        client.get_bid(&bid_accept).unwrap().status,
        BidStatus::Accepted,
        "bid must be Accepted"
    );

    assert_eq!(
        BidStorage::count_active_placed_bids_for_investor(&env, &investor),
        1,
        "count must drop to 1 after acceptance"
    );

    // New bid must now succeed.
    assert!(
        client.try_place_bid(&investor, &inv_new, &1_000i128, &1_100i128).is_ok(),
        "new bid must succeed after acceptance freed a slot"
    );
}

// ---------------------------------------------------------------------------
// 5. count_active_placed_bids_for_investor excludes all non-Placed statuses
// ---------------------------------------------------------------------------

#[test]
fn test_count_excludes_non_placed_statuses() {
    let (env, client, admin) = setup();
    // High cap so placement never blocks.
    client.set_max_active_bids_per_investor(&50u32).unwrap();
    client.set_bid_ttl_days(&1u64).unwrap();

    // funded_setup gives us a real token for the accept path.
    let (_, investor, inv_accepted) = funded_setup(&env, &client, &admin);
    let currency = client.get_invoice(&inv_accepted).unwrap().currency;

    let business = Address::generate(&env);
    client.submit_kyc_application(&business, &String::from_str(&env, "KYC"));
    client.verify_business(&admin, &business);

    // Place 5 bids — one per transition under test + one left as Placed.
    let inv_placed   = plain_invoice(&env, &client, &admin, &business, &currency);
    let inv_cancel   = plain_invoice(&env, &client, &admin, &business, &currency);
    let inv_withdraw = plain_invoice(&env, &client, &admin, &business, &currency);
    let inv_expire   = plain_invoice(&env, &client, &admin, &business, &currency);

    let bid_stay     = client.place_bid(&investor, &inv_placed,   &1_000i128, &1_100i128);
    let bid_cancel   = client.place_bid(&investor, &inv_cancel,   &1_000i128, &1_100i128);
    let bid_withdraw = client.place_bid(&investor, &inv_withdraw,  &1_000i128, &1_100i128);
    let bid_expire   = client.place_bid(&investor, &inv_expire,   &1_000i128, &1_100i128);
    let bid_accept   = client.place_bid(&investor, &inv_accepted, &5_000i128, &5_500i128);

    assert_eq!(
        BidStorage::count_active_placed_bids_for_investor(&env, &investor),
        5,
        "all 5 bids must be active initially"
    );

    // Transition bid_cancel → Cancelled.
    client.cancel_bid(&bid_cancel);

    // Transition bid_withdraw → Withdrawn.
    client.withdraw_bid(&bid_withdraw).unwrap();

    // Transition bid_expire → Expired via ledger advancement + cleanup.
    let exp_ts = client.get_bid(&bid_expire).unwrap().expiration_timestamp;
    env.ledger().set_timestamp(exp_ts + 1);
    client.cleanup_expired_bids(&inv_expire);

    // Transition bid_accept → Accepted.
    client.accept_bid(&inv_accepted, &bid_accept).unwrap();

    // Only bid_stay should remain in the active count.
    let count = BidStorage::count_active_placed_bids_for_investor(&env, &investor);
    assert_eq!(count, 1, "only the untouched Placed bid must be counted; got {}", count);

    // Double-check each status for clarity.
    assert_eq!(client.get_bid(&bid_stay).unwrap().status,     BidStatus::Placed);
    assert_eq!(client.get_bid(&bid_cancel).unwrap().status,   BidStatus::Cancelled);
    assert_eq!(client.get_bid(&bid_withdraw).unwrap().status, BidStatus::Withdrawn);
    assert_eq!(client.get_bid(&bid_expire).unwrap().status,   BidStatus::Expired);
    assert_eq!(client.get_bid(&bid_accept).unwrap().status,   BidStatus::Accepted);
}

// ---------------------------------------------------------------------------
// 6. INVESTOR_BID_LIMIT_DISABLED (0) removes the cap entirely
// ---------------------------------------------------------------------------

#[test]
fn test_disabled_limit_allows_placement_beyond_previous_cap() {
    let (env, client, admin) = setup();

    // Set a cap of 2 then disable it.
    client.set_max_active_bids_per_investor(&2u32).unwrap();
    client.set_max_active_bids_per_investor(&INVESTOR_BID_LIMIT_DISABLED).unwrap();

    assert!(
        !BidStorage::is_investor_bid_limit_active(&env),
        "limit must be inactive when set to INVESTOR_BID_LIMIT_DISABLED"
    );

    let cfg = BidStorage::get_bid_limit_config(&env);
    assert_eq!(cfg.limit, 0);
    assert!(cfg.is_disabled);

    let business = Address::generate(&env);
    client.submit_kyc_application(&business, &String::from_str(&env, "KYC"));
    client.verify_business(&admin, &business);
    let currency = Address::generate(&env);
    client.add_currency(&admin, &currency);

    let investor = Address::generate(&env);
    client.submit_investor_kyc(&investor, &String::from_str(&env, "KYC"));
    client.verify_investor(&investor, &1_000_000i128);

    // Placing more than the old cap must all succeed.
    for _ in 0..5u32 {
        let inv = plain_invoice(&env, &client, &admin, &business, &currency);
        assert!(
            client.try_place_bid(&investor, &inv, &1_000i128, &1_100i128).is_ok(),
            "placement must succeed when limit is disabled"
        );
    }

    assert_eq!(
        BidStorage::count_active_placed_bids_for_investor(&env, &investor),
        5
    );
}

// ---------------------------------------------------------------------------
// 7. reset_max_active_bids_per_investor restores the compile-time default
// ---------------------------------------------------------------------------

#[test]
fn test_reset_restores_default() {
    let (env, client, admin) = setup();

    // Disable the limit, then reset.
    client.set_max_active_bids_per_investor(&INVESTOR_BID_LIMIT_DISABLED).unwrap();
    BidStorage::reset_max_active_bids_per_investor(&env, &admin).unwrap();

    let cfg = BidStorage::get_bid_limit_config(&env);
    assert_eq!(cfg.limit, cfg.default_limit, "limit must equal the default after reset");
    assert_eq!(cfg.default_limit, 20, "compile-time default must be 20");
    assert!(!cfg.is_custom,   "is_custom must be false after reset");
    assert!(!cfg.is_disabled, "is_disabled must be false after reset");
    assert!(
        BidStorage::is_investor_bid_limit_active(&env),
        "limit must be active after reset"
    );
}

// ---------------------------------------------------------------------------
// 8. get_bid_limit_config snapshot reflects every state transition correctly
// ---------------------------------------------------------------------------

#[test]
fn test_bid_limit_config_snapshot() {
    let (env, client, admin) = setup();

    // Fresh: default, not custom, not disabled.
    let cfg = BidStorage::get_bid_limit_config(&env);
    assert_eq!(cfg.limit, 20);
    assert_eq!(cfg.default_limit, 20);
    assert!(!cfg.is_custom);
    assert!(!cfg.is_disabled);

    // Custom value.
    client.set_max_active_bids_per_investor(&5u32).unwrap();
    let cfg = BidStorage::get_bid_limit_config(&env);
    assert_eq!(cfg.limit, 5);
    assert!(cfg.is_custom);
    assert!(!cfg.is_disabled);

    // Disabled.
    client.set_max_active_bids_per_investor(&INVESTOR_BID_LIMIT_DISABLED).unwrap();
    let cfg = BidStorage::get_bid_limit_config(&env);
    assert_eq!(cfg.limit, 0);
    assert!(cfg.is_custom);
    assert!(cfg.is_disabled);

    // Reset.
    BidStorage::reset_max_active_bids_per_investor(&env, &admin).unwrap();
    let cfg = BidStorage::get_bid_limit_config(&env);
    assert_eq!(cfg.limit, 20);
    assert!(!cfg.is_custom);
    assert!(!cfg.is_disabled);
}
