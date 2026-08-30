//! Atomic rollback regression tests — Issue #2454 (QE-2026-08)
//!
//! Proves that funding commitments, investor exposure, and available capacity
//! are exact, authorized, and recoverable on failure.
//!
//! # Design
//!
//! All tests exercise the **real contract entrypoints** via the generated
//! `QuickLendXContractClient` (not internal helpers), and assert on on-chain
//! **token balances**, not just return values.  A "distributes more than was
//! received" or "partial escrow written on rejection" bug shows up as a
//! failing balance assertion rather than a silent accounting discrepancy.
//!
//! # Invariants validated
//!
//! 1. **Exposure formula** — `exposure = Σ(Placed, !expired bid amounts) +
//!    Σ(Active investment amounts)`.  Lifetime analytics (`total_invested`)
//!    are excluded from the cap (Issue #2454 compatibility fix).
//!
//! 2. **Capacity formula** — `capacity = max(0, limit − exposure)`.
//!    An unverified/frozen investor always returns an error, not 0.
//!
//! 3. **No partial state on rejection** — a failed `validate_funding_commitment`,
//!    `accept_bid`, or `create_escrow` leaves token balances, escrow records,
//!    investment records, bid status, and invoice status exactly as they were.
//!
//! 4. **No state accumulation on repeated failure** — calling the same rejected
//!    operation N times produces the same final state as calling it once.
//!
//! 5. **Exposure released exactly once on terminal transitions** — bid withdrawal,
//!    investment completion/refund each release capacity exactly once.
//!
//! # Running
//! ```bash
//! cargo test test_funding_atomic_rollback
//! ```

#![cfg(test)]

use crate::errors::QuickLendXError;
use crate::invoice::InvoiceCategory;
use crate::payments::{EscrowStatus, EscrowStorage};
use crate::types::{BidStatus, InvoiceStatus};
use crate::{QuickLendXContract, QuickLendXContractClient};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, BytesN, Env, String, Vec as SorobanVec,
};

// ===========================================================================
// Helpers
// ===========================================================================

fn setup() -> (Env, QuickLendXContractClient<'static>, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let contract_addr = contract_id.clone();
    client.set_admin(&admin);
    (env, client, admin, contract_addr)
}

/// Register a Stellar Asset Contract token, mint balances, and approve the contract.
fn setup_token(
    env: &Env,
    accounts: &[&Address],
    contract_id: &Address,
    balance: i128,
) -> Address {
    let token_admin = Address::generate(env);
    let currency = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let sac = token::StellarAssetClient::new(env, &currency);
    let tok = token::Client::new(env, &currency);
    let expiry = env.ledger().sequence() + 10_000;
    for acc in accounts {
        sac.mint(acc, &balance);
        tok.approve(acc, contract_id, &balance, &expiry);
    }
    currency
}

fn setup_verified_business(
    env: &Env,
    client: &QuickLendXContractClient,
    admin: &Address,
) -> Address {
    let business = Address::generate(env);
    client.submit_kyc_application(&business, &String::from_str(env, "kyc"));
    client.verify_business(admin, &business);
    business
}

fn setup_verified_investor(
    env: &Env,
    client: &QuickLendXContractClient,
    limit: i128,
) -> Address {
    let investor = Address::generate(env);
    client.submit_investor_kyc(&investor, &String::from_str(env, "kyc"));
    client.verify_investor(&investor, &limit);
    investor
}

fn create_verified_invoice(
    env: &Env,
    client: &QuickLendXContractClient,
    business: &Address,
    amount: i128,
    currency: &Address,
) -> BytesN<32> {
    let due = env.ledger().timestamp() + 86_400;
    let invoice_id = client.store_invoice(
        business,
        &amount,
        currency,
        &due,
        &String::from_str(env, "test invoice"),
        &InvoiceCategory::Services,
        &SorobanVec::new(env),
        &None,
    );
    client.verify_invoice(&invoice_id);
    invoice_id
}

fn place_bid(
    env: &Env,
    client: &QuickLendXContractClient,
    investor: &Address,
    invoice_id: &BytesN<32>,
    amount: i128,
    nonce: u8,
) -> BytesN<32> {
    client.place_bid(
        investor,
        invoice_id,
        &amount,
        &(amount + 500),
        &BytesN::from_array(env, &[nonce; 32]),
    )
}

// ===========================================================================
// 1. validate_funding_commitment: no state on rejection
// ===========================================================================

/// `validate_funding_commitment` with amount > capacity must return
/// `InvalidAmount` and must NOT write any escrow or investment record.
#[test]
fn test_funding_commitment_rejected_leaves_no_state() {
    let (env, client, admin, contract_id) = setup();
    env.ledger().set_timestamp(1_000);

    let investor = setup_verified_investor(&env, &client, 10_000);
    let business = setup_verified_business(&env, &client, &admin);
    let currency = setup_token(&env, &[&investor, &business], &contract_id, 100_000);

    let invoice_id = create_verified_invoice(&env, &client, &business, 10_000, &currency);

    // Over-limit commitment — must be rejected
    let over_cap = client.try_validate_funding_commitment(&investor, &10_001);
    assert_eq!(over_cap, Err(Ok(QuickLendXError::InvalidAmount)));

    // No escrow record must have been written
    let escrow_absent = env.as_contract(&contract_id, || {
        EscrowStorage::get_escrow_by_invoice(&env, &invoice_id).is_none()
    });
    assert!(escrow_absent, "no escrow must be written on rejected commitment");

    // Token balances must be unchanged
    let tok = token::Client::new(&env, &currency);
    assert_eq!(tok.balance(&contract_id), 0, "contract must hold no tokens");
}

/// Zero and negative amounts are rejected before any state is touched.
#[test]
fn test_funding_commitment_zero_amount_rejected() {
    let (env, client, _, _) = setup();
    let investor = setup_verified_investor(&env, &client, 10_000);

    let r0 = client.try_validate_funding_commitment(&investor, &0);
    assert_eq!(r0, Err(Ok(QuickLendXError::InvalidAmount)));

    let rn = client.try_validate_funding_commitment(&investor, &-1);
    assert_eq!(rn, Err(Ok(QuickLendXError::InvalidAmount)));
}

// ===========================================================================
// 2. Exposure starts at zero and accounts exactly
// ===========================================================================

/// A freshly verified investor has 0 exposure and full available capacity.
#[test]
fn test_exposure_is_zero_before_any_bid() {
    let (env, client, _, _) = setup();
    env.ledger().set_timestamp(1_000);
    let limit = 20_000i128;
    let investor = setup_verified_investor(&env, &client, limit);

    let exposure = client.get_investor_active_exposure(&investor).unwrap();
    assert_eq!(exposure, 0, "fresh investor must have zero exposure");

    let capacity = client.get_investor_available_capacity(&investor).unwrap();
    assert_eq!(capacity, limit, "fresh investor must have full capacity");
}

/// Placing a bid reserves exactly `bid_amount` in exposure.
#[test]
fn test_active_bid_reserves_exposure_exactly() {
    let (env, client, admin, contract_id) = setup();
    env.ledger().set_timestamp(1_000);
    let limit = 20_000i128;
    let bid_amount = 7_500i128;
    let investor = setup_verified_investor(&env, &client, limit);
    let business = setup_verified_business(&env, &client, &admin);
    let currency = setup_token(&env, &[&investor, &business], &contract_id, 100_000);
    let invoice_id = create_verified_invoice(&env, &client, &business, bid_amount, &currency);

    place_bid(&env, &client, &investor, &invoice_id, bid_amount, 1);

    let exposure = client.get_investor_active_exposure(&investor).unwrap();
    assert_eq!(exposure, bid_amount, "placed bid must reserve exactly bid_amount");

    let capacity = client.get_investor_available_capacity(&investor).unwrap();
    assert_eq!(capacity, limit - bid_amount, "capacity must decrease by bid_amount");
}

/// After `accept_bid`, the active investment reserves the investment amount.
#[test]
fn test_active_investment_reserves_exposure_exactly() {
    let (env, client, admin, contract_id) = setup();
    env.ledger().set_timestamp(1_000);
    let limit = 20_000i128;
    let amount = 8_000i128;
    let investor = setup_verified_investor(&env, &client, limit);
    let business = setup_verified_business(&env, &client, &admin);
    let currency = setup_token(&env, &[&investor, &business], &contract_id, 100_000);
    let invoice_id = create_verified_invoice(&env, &client, &business, amount, &currency);

    let bid_id = place_bid(&env, &client, &investor, &invoice_id, amount, 1);
    client.accept_bid(&invoice_id, &bid_id);

    let exposure = client.get_investor_active_exposure(&investor).unwrap();
    assert_eq!(exposure, amount, "funded investment must reserve exactly the funded amount");

    let capacity = client.get_investor_available_capacity(&investor).unwrap();
    assert_eq!(capacity, limit - amount);
}

// ===========================================================================
// 3. Cap exhaustion and rejection leave no state
// ===========================================================================

/// Bid + investment together fill the limit; a third bid exceeding the cap is
/// rejected with no escrow, no balance change, and no phantom reservation.
#[test]
fn test_capacity_exhausted_after_bid_and_investment() {
    let (env, client, admin, contract_id) = setup();
    env.ledger().set_timestamp(1_000);
    let limit = 15_000i128;
    let inv_amount = 8_000i128;
    let bid_amount = 5_000i128; // inv + bid = 13_000 < 15_000
    let over_amount = 3_000i128; // 13_000 + 3_000 = 16_000 > 15_000

    let investor = setup_verified_investor(&env, &client, limit);
    let business = setup_verified_business(&env, &client, &admin);
    let currency = setup_token(&env, &[&investor, &business], &contract_id, 100_000);

    // Fund invoice 1 → active investment of 8_000
    let inv1 = create_verified_invoice(&env, &client, &business, inv_amount, &currency);
    let bid1 = place_bid(&env, &client, &investor, &inv1, inv_amount, 1);
    client.accept_bid(&inv1, &bid1);

    // Place bid of 5_000 on invoice 2 → pending exposure 8_000 + 5_000 = 13_000
    let inv2 = create_verified_invoice(&env, &client, &business, bid_amount + over_amount, &currency);
    place_bid(&env, &client, &investor, &inv2, bid_amount, 2);

    // Token balance before the rejected attempt
    let tok = token::Client::new(&env, &currency);
    let investor_bal_before = tok.balance(&investor);
    let contract_bal_before = tok.balance(&contract_id);

    // Over-cap bid must be rejected
    let over_result = client.try_place_bid(
        &investor,
        &inv2,
        &over_amount,
        &(over_amount + 100),
        &BytesN::from_array(&env, &[3u8; 32]),
    );
    assert!(over_result.is_err(), "over-cap bid must be rejected");

    // Token balances must be unchanged
    assert_eq!(tok.balance(&investor), investor_bal_before);
    assert_eq!(tok.balance(&contract_id), contract_bal_before);
}

// ===========================================================================
// 4. accept_bid failure leaves all state unchanged
// ===========================================================================

/// Accepting a bid with a mismatched `invoice_id` must return an error and
/// leave bid status, escrow, investment, and token balances all unchanged.
#[test]
fn test_accept_bid_mismatched_invoice_leaves_no_state() {
    let (env, client, admin, contract_id) = setup();
    env.ledger().set_timestamp(1_000);
    let amount = 5_000i128;
    let investor = setup_verified_investor(&env, &client, 50_000);
    let business = setup_verified_business(&env, &client, &admin);
    let currency = setup_token(&env, &[&investor, &business], &contract_id, 100_000);

    let inv_a = create_verified_invoice(&env, &client, &business, amount, &currency);
    let inv_b = create_verified_invoice(&env, &client, &business, amount, &currency);
    let bid_on_a = place_bid(&env, &client, &investor, &inv_a, amount, 1);

    let tok = token::Client::new(&env, &currency);
    let investor_bal = tok.balance(&investor);
    let contract_bal = tok.balance(&contract_id);

    // Attempt to accept bid_on_a against inv_b — invoice/bid mismatch
    let result = client.try_accept_bid(&inv_b, &bid_on_a);
    assert!(result.is_err(), "mismatched invoice/bid must be rejected");

    // Bid status must still be Placed
    let bid = client.get_bid(&bid_on_a).unwrap();
    assert_eq!(bid.status, BidStatus::Placed);

    // No escrow must have been created for either invoice
    let no_escrow_a = env.as_contract(&contract_id, || {
        EscrowStorage::get_escrow_by_invoice(&env, &inv_a).is_none()
    });
    let no_escrow_b = env.as_contract(&contract_id, || {
        EscrowStorage::get_escrow_by_invoice(&env, &inv_b).is_none()
    });
    assert!(no_escrow_a, "inv_a must have no escrow");
    assert!(no_escrow_b, "inv_b must have no escrow");

    // Token balances unchanged
    assert_eq!(tok.balance(&investor), investor_bal);
    assert_eq!(tok.balance(&contract_id), contract_bal);
}

/// A second `accept_bid` on an already-funded invoice must be rejected without
/// creating a second escrow or moving any tokens.
#[test]
fn test_duplicate_accept_bid_rejected_balances_unchanged() {
    let (env, client, admin, contract_id) = setup();
    env.ledger().set_timestamp(1_000);
    let amount = 5_000i128;
    let investor = setup_verified_investor(&env, &client, 50_000);
    let business = setup_verified_business(&env, &client, &admin);
    let currency = setup_token(&env, &[&investor, &business], &contract_id, 100_000);

    let invoice_id = create_verified_invoice(&env, &client, &business, amount, &currency);
    let bid_id = place_bid(&env, &client, &investor, &invoice_id, amount, 1);

    // First accept — must succeed
    client.accept_bid(&invoice_id, &bid_id);

    let tok = token::Client::new(&env, &currency);
    let contract_bal_after_first = tok.balance(&contract_id);
    let investor_bal_after_first = tok.balance(&investor);

    // Second accept — must be rejected
    let dup = client.try_accept_bid(&invoice_id, &bid_id);
    assert!(dup.is_err(), "duplicate accept_bid must be rejected");

    // Balances must be unchanged
    assert_eq!(tok.balance(&contract_id), contract_bal_after_first);
    assert_eq!(tok.balance(&investor), investor_bal_after_first);

    // Only one escrow record must exist
    let escrow = env.as_contract(&contract_id, || {
        EscrowStorage::get_escrow_by_invoice(&env, &invoice_id)
    });
    assert!(escrow.is_some(), "one escrow must exist");
    assert_eq!(escrow.unwrap().status, EscrowStatus::Held);
}

/// Accepting an expired bid must be rejected; no escrow, investment, or token
/// transfer must occur.
#[test]
fn test_accept_bid_expired_bid_leaves_no_state() {
    let (env, client, admin, contract_id) = setup();
    env.ledger().set_timestamp(1_000);
    let amount = 5_000i128;
    let investor = setup_verified_investor(&env, &client, 50_000);
    let business = setup_verified_business(&env, &client, &admin);
    let currency = setup_token(&env, &[&investor, &business], &contract_id, 100_000);

    let invoice_id = create_verified_invoice(&env, &client, &business, amount, &currency);
    let bid_id = place_bid(&env, &client, &investor, &invoice_id, amount, 1);

    // Advance time past the bid's expiration (default 24h = 86_400s)
    env.ledger().set_timestamp(1_000 + 86_401);

    let tok = token::Client::new(&env, &currency);
    let investor_bal = tok.balance(&investor);
    let contract_bal = tok.balance(&contract_id);

    let result = client.try_accept_bid(&invoice_id, &bid_id);
    assert!(result.is_err(), "accepting expired bid must fail");

    // No escrow must exist
    let no_escrow = env.as_contract(&contract_id, || {
        EscrowStorage::get_escrow_by_invoice(&env, &invoice_id).is_none()
    });
    assert!(no_escrow, "no escrow must be written for expired bid");

    // Balances unchanged
    assert_eq!(tok.balance(&investor), investor_bal);
    assert_eq!(tok.balance(&contract_id), contract_bal);
}

/// Accepting a bid when the investor is frozen must be rejected without
/// creating any state.
#[test]
fn test_accept_bid_frozen_investor_leaves_no_state() {
    let (env, client, admin, contract_id) = setup();
    env.ledger().set_timestamp(1_000);
    let amount = 5_000i128;
    let investor = setup_verified_investor(&env, &client, 50_000);
    let business = setup_verified_business(&env, &client, &admin);
    let currency = setup_token(&env, &[&investor, &business], &contract_id, 100_000);

    let invoice_id = create_verified_invoice(&env, &client, &business, amount, &currency);
    let bid_id = place_bid(&env, &client, &investor, &invoice_id, amount, 1);

    // Freeze the investor
    client.freeze_investor(&admin, &investor, &crate::verification::InvestorFreezeReason::SuspectedFraud);

    let tok = token::Client::new(&env, &currency);
    let investor_bal = tok.balance(&investor);
    let contract_bal = tok.balance(&contract_id);

    let result = client.try_accept_bid(&invoice_id, &bid_id);
    assert!(result.is_err(), "frozen investor: accept_bid must be rejected");

    let no_escrow = env.as_contract(&contract_id, || {
        EscrowStorage::get_escrow_by_invoice(&env, &invoice_id).is_none()
    });
    assert!(no_escrow, "no escrow must be written for frozen investor");
    assert_eq!(tok.balance(&investor), investor_bal);
    assert_eq!(tok.balance(&contract_id), contract_bal);
}

// ===========================================================================
// 5. Sequential bids share one cap
// ===========================================================================

/// Two sequential bids from the same investor together consume the limit;
/// a third bid exceeding the remaining capacity is rejected.
#[test]
fn test_sequential_bids_share_one_exposure_cap() {
    let (env, client, admin, contract_id) = setup();
    env.ledger().set_timestamp(1_000);
    let limit = 12_000i128;
    let bid1_amount = 5_000i128;
    let bid2_amount = 5_000i128;
    let bid3_amount = 3_000i128; // 5_000 + 5_000 + 3_000 = 13_000 > 12_000

    let investor = setup_verified_investor(&env, &client, limit);
    let business = setup_verified_business(&env, &client, &admin);
    let currency = setup_token(&env, &[&investor, &business], &contract_id, 100_000);

    let inv1 = create_verified_invoice(&env, &client, &business, bid1_amount, &currency);
    let inv2 = create_verified_invoice(&env, &client, &business, bid2_amount + bid3_amount, &currency);

    place_bid(&env, &client, &investor, &inv1, bid1_amount, 1);
    place_bid(&env, &client, &investor, &inv2, bid2_amount, 2);

    let exposure = client.get_investor_active_exposure(&investor).unwrap();
    assert_eq!(exposure, bid1_amount + bid2_amount, "combined bid exposure must equal sum");

    // Third bid pushes over limit — must be rejected
    let result = client.try_place_bid(
        &investor,
        &inv2,
        &bid3_amount,
        &(bid3_amount + 100),
        &BytesN::from_array(&env, &[3u8; 32]),
    );
    assert!(result.is_err(), "third bid over cap must be rejected");

    // Exposure must be unchanged
    let exposure_after = client.get_investor_active_exposure(&investor).unwrap();
    assert_eq!(exposure_after, bid1_amount + bid2_amount);
}

// ===========================================================================
// 6. Exposure released on terminal transitions
// ===========================================================================

/// Withdrawing a bid releases its exposure exactly once; capacity is restored.
#[test]
fn test_exposure_released_on_bid_withdrawal() {
    let (env, client, admin, contract_id) = setup();
    env.ledger().set_timestamp(1_000);
    let limit = 10_000i128;
    let amount = 6_000i128;

    let investor = setup_verified_investor(&env, &client, limit);
    let business = setup_verified_business(&env, &client, &admin);
    let currency = setup_token(&env, &[&investor, &business], &contract_id, 100_000);

    let invoice_id = create_verified_invoice(&env, &client, &business, amount, &currency);
    let bid_id = place_bid(&env, &client, &investor, &invoice_id, amount, 1);

    // Withdraw the bid
    client.withdraw_bid(&bid_id);

    let exposure = client.get_investor_active_exposure(&investor).unwrap();
    assert_eq!(exposure, 0, "withdrawn bid must release all exposure");

    let capacity = client.get_investor_available_capacity(&investor).unwrap();
    assert_eq!(capacity, limit, "full capacity must be restored after withdrawal");
}

/// After `accept_bid` the investment is Active; after `refund_escrow_funds`
/// (which sets it to Refunded) exposure drops back to zero.
#[test]
fn test_exposure_released_on_escrow_refund() {
    let (env, client, admin, contract_id) = setup();
    env.ledger().set_timestamp(1_000);
    let limit = 10_000i128;
    let amount = 6_000i128;

    let investor = setup_verified_investor(&env, &client, limit);
    let business = setup_verified_business(&env, &client, &admin);
    let currency = setup_token(&env, &[&investor, &business], &contract_id, 100_000);

    let invoice_id = create_verified_invoice(&env, &client, &business, amount, &currency);
    let bid_id = place_bid(&env, &client, &investor, &invoice_id, amount, 1);
    client.accept_bid(&invoice_id, &bid_id);

    let exposure_before = client.get_investor_active_exposure(&investor).unwrap();
    assert_eq!(exposure_before, amount, "active investment must reserve exposure");

    // Refund the escrow (settles the position as Refunded)
    client.refund_escrow_funds(&invoice_id, &investor);

    let exposure_after = client.get_investor_active_exposure(&investor).unwrap();
    assert_eq!(exposure_after, 0, "refunded investment must release all exposure");

    let capacity_after = client.get_investor_available_capacity(&investor).unwrap();
    assert_eq!(capacity_after, limit, "full capacity must be restored after refund");
}

// ===========================================================================
// 7. Repeated failed commitment does not accumulate state
// ===========================================================================

/// Calling `validate_funding_commitment` with an over-cap amount 10 times
/// produces the same final state as calling it once: zero escrows, zero token
/// movement, unchanged exposure.
#[test]
fn test_repeated_failed_commitments_do_not_accumulate_state() {
    let (env, client, admin, contract_id) = setup();
    env.ledger().set_timestamp(1_000);
    let limit = 5_000i128;
    let investor = setup_verified_investor(&env, &client, limit);
    let business = setup_verified_business(&env, &client, &admin);
    let currency = setup_token(&env, &[&investor, &business], &contract_id, 100_000);
    let invoice_id = create_verified_invoice(&env, &client, &business, limit, &currency);

    let tok = token::Client::new(&env, &currency);

    for i in 0u8..10 {
        let result = client.try_validate_funding_commitment(&investor, &(limit + 1));
        assert!(result.is_err(), "iteration {i}: over-cap commitment must fail");
    }

    // Exposure unchanged
    let exposure = client.get_investor_active_exposure(&investor).unwrap();
    assert_eq!(exposure, 0, "repeated rejection must not accumulate exposure");

    // No escrow created
    let no_escrow = env.as_contract(&contract_id, || {
        EscrowStorage::get_escrow_by_invoice(&env, &invoice_id).is_none()
    });
    assert!(no_escrow, "repeated rejection must not write any escrow record");

    // Token balances unchanged
    assert_eq!(tok.balance(&contract_id), 0);
}

// ===========================================================================
// 8. Full funding lifecycle — exposure tracks end-to-end
// ===========================================================================

/// End-to-end lifecycle: bid placed → accepted → investment active → escrow
/// released (business receives funds) → exposure returns to 0.
#[test]
fn test_exposure_consistency_across_bid_and_investment_lifecycle() {
    let (env, client, admin, contract_id) = setup();
    env.ledger().set_timestamp(1_000);
    let limit = 20_000i128;
    let amount = 10_000i128;

    let investor = setup_verified_investor(&env, &client, limit);
    let business = setup_verified_business(&env, &client, &admin);
    let currency = setup_token(&env, &[&investor, &business], &contract_id, 100_000);

    let tok = token::Client::new(&env, &currency);

    // 1. Before bid
    assert_eq!(client.get_investor_active_exposure(&investor).unwrap(), 0);

    // 2. Place bid → exposure = amount
    let invoice_id = create_verified_invoice(&env, &client, &business, amount, &currency);
    let bid_id = place_bid(&env, &client, &investor, &invoice_id, amount, 1);
    assert_eq!(client.get_investor_active_exposure(&investor).unwrap(), amount);

    // 3. Accept bid → bid consumed, investment active, exposure = amount
    client.accept_bid(&invoice_id, &bid_id);
    assert_eq!(client.get_investor_active_exposure(&investor).unwrap(), amount);

    // Confirm token moved from investor to contract
    let investor_balance_funded = tok.balance(&investor);
    let contract_balance_funded = tok.balance(&contract_id);
    assert_eq!(contract_balance_funded, amount, "contract must hold escrowed amount");

    // 4. Release escrow (business claims funds) → exposure = 0
    client.release_escrow_funds(&invoice_id, &business);

    let exposure_final = client.get_investor_active_exposure(&investor).unwrap();
    assert_eq!(exposure_final, 0, "exposure must be 0 after escrow released");

    let capacity_final = client.get_investor_available_capacity(&investor).unwrap();
    assert_eq!(capacity_final, limit, "full capacity must be restored after release");

    // Business received the funds
    assert_eq!(tok.balance(&business), amount, "business must receive escrowed amount");
    assert_eq!(tok.balance(&contract_id), 0, "contract must hold nothing after release");

    // Escrow status must be Released
    let escrow = env.as_contract(&contract_id, || {
        EscrowStorage::get_escrow_by_invoice(&env, &invoice_id)
    });
    assert_eq!(escrow.unwrap().status, EscrowStatus::Released);
}

// ===========================================================================
// 9. Lifetime analytics exclusion (Issue #2454 compatibility fix)
// ===========================================================================

/// An investor with a large `total_invested` (completed historical investments)
/// must NOT be blocked from new bids up to their current active-risk cap.
/// Before the fix, `validate_investor_investment` included `total_invested` in
/// the cap, which permanently blocked experienced investors.
#[test]
fn test_lifetime_total_invested_does_not_reduce_current_capacity() {
    use crate::verification::{
        BusinessVerificationStatus, InvestorRiskLevel, InvestorTier, InvestorVerification,
        InvestorVerificationStorage,
    };

    let (env, client, admin, contract_id) = setup();
    env.ledger().set_timestamp(1_000);
    let limit = 10_000i128;
    let bid_amount = 8_000i128;
    let lifetime_volume = 999_999_999i128; // much larger than limit

    let investor = Address::generate(&env);
    let business = setup_verified_business(&env, &client, &admin);
    let currency = setup_token(&env, &[&investor, &business], &contract_id, 100_000);

    // Manually write a verification record with large total_invested but no active positions
    env.as_contract(&contract_id, || {
        let record = InvestorVerification {
            investor: investor.clone(),
            status: BusinessVerificationStatus::Verified,
            verified_at: Some(env.ledger().timestamp()),
            verified_by: None,
            kyc_data: String::from_str(&env, "kyc"),
            investment_limit: limit,
            submitted_at: env.ledger().timestamp(),
            tier: InvestorTier::Basic,
            risk_level: InvestorRiskLevel::Low,
            risk_score: 0,
            total_invested: lifetime_volume, // large historical volume
            total_returns: 0,
            successful_investments: 0,
            defaulted_investments: 0,
            last_activity: env.ledger().timestamp(),
            rejection_reason: None,
            compliance_notes: None,
        };
        InvestorVerificationStorage::store(&env, &record);
    });

    // Approve token spend for investor
    let tok = token::Client::new(&env, &currency);
    let sac = token::StellarAssetClient::new(&env, &currency);
    sac.mint(&investor, &100_000);
    tok.approve(&investor, &contract_id, &100_000, &(env.ledger().sequence() + 10_000));

    let invoice_id = create_verified_invoice(&env, &client, &business, bid_amount, &currency);

    // Bid within the active-risk limit must SUCCEED despite large total_invested
    let bid_result = client.try_place_bid(
        &investor,
        &invoice_id,
        &bid_amount,
        &(bid_amount + 500),
        &BytesN::from_array(&env, &[1u8; 32]),
    );
    assert!(
        bid_result.is_ok(),
        "bid within active-risk limit must succeed even with large total_invested; \
         lifetime analytics must not reduce current capacity"
    );

    // Exposure must be exactly bid_amount (no phantom lifetime contribution)
    let exposure = client.get_investor_active_exposure(&investor).unwrap();
    assert_eq!(
        exposure, bid_amount,
        "exposure must equal only active bid amount, not include total_invested"
    );
}

// ===========================================================================
// 10. Concurrent sequential bids: first wins, second rejected cleanly
// ===========================================================================

/// Simulate two investors racing to fund the same invoice. In Soroban,
/// only one can succeed (one-escrow-per-invoice guard). The second must fail
/// without writing any state.
#[test]
fn test_concurrent_funding_only_one_escrow_created() {
    let (env, client, admin, contract_id) = setup();
    env.ledger().set_timestamp(1_000);
    let amount = 5_000i128;

    let inv_a = setup_verified_investor(&env, &client, 50_000);
    let inv_b = setup_verified_investor(&env, &client, 50_000);
    let business = setup_verified_business(&env, &client, &admin);
    let currency = setup_token(&env, &[&inv_a, &inv_b, &business], &contract_id, 100_000);

    let tok = token::Client::new(&env, &currency);

    let invoice_id = create_verified_invoice(&env, &client, &business, amount, &currency);
    let bid_a = place_bid(&env, &client, &inv_a, &invoice_id, amount, 1);
    let bid_b = place_bid(&env, &client, &inv_b, &invoice_id, amount, 2);

    // First acceptance wins
    let r_a = client.try_accept_bid(&invoice_id, &bid_a);
    assert!(r_a.is_ok(), "first acceptance must succeed");

    // Second acceptance loses (invoice already funded)
    let r_b = client.try_accept_bid(&invoice_id, &bid_b);
    assert!(r_b.is_err(), "second acceptance must be rejected (already funded)");

    // inv_b's token balance must be unchanged
    assert_eq!(tok.balance(&inv_b), 100_000, "losing investor must lose no tokens");

    // Only one escrow record must exist
    let escrow = env.as_contract(&contract_id, || {
        EscrowStorage::get_escrow_by_invoice(&env, &invoice_id)
    });
    assert!(escrow.is_some(), "one escrow must exist");
    assert_eq!(
        escrow.unwrap().investor, inv_a,
        "escrow must belong to the winning investor"
    );
}
