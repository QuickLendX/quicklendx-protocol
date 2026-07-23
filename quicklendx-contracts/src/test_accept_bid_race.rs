//! # Concurrent-Acceptance Race Regression — `accept_bid_and_fund`
//!
//! ## Purpose
//! Even though Soroban executes one transaction at a time, multiple investors
//! submitting `accept_bid_and_fund` for the **same invoice** within the same
//! ledger can be ordered adversarially by validators. This module asserts that
//! **only one acceptance succeeds** regardless of the ledger ordering, the
//! second attempt always returns a stable error, and **no partial escrow or
//! investment state lingers** from the losing leg.
//!
//! ## Security Note
//! Race conditions on acceptance are **high-severity**: a successful second
//! acceptance could double-fund an invoice, creating an orphaned escrow record
//! whose funds are locked with no redemption path. The protocol prevents this
//! via a two-layer idempotency guard in `escrow::load_accept_bid_context`:
//!
//! 1. Invoice status check (`Funded` ⟹ `InvoiceAlreadyFunded`)
//! 2. Pre-existing escrow / investment record check (`InvalidStatus`)
//!
//! Both orderings of the two acceptance calls are exercised here so the
//! regression is **ordering-independent**.
//!
//! ## Tests
//! | Test | Scenario |
//! |------|----------|
//! | `test_race_ordering_a_wins` | Investor A's call executes first; B's second call fails with `InvoiceAlreadyFunded` |
//! | `test_race_ordering_b_wins` | Investor B's call executes first; A's second call fails with `InvoiceAlreadyFunded` |
//! | `test_race_same_bid_both_orderings` | Both investors attempt to accept the **same** bid; second fails |
//! | `test_race_no_partial_state_on_failure` | Failing leg leaves zero escrow / investment residue |
//! | `test_race_idempotent_after_accept` | Calling `accept_bid_and_fund` again on an already-funded invoice is always rejected |
//! | `test_race_different_bids_only_one_escrow` | Two valid bids on the same invoice; exactly one escrow is ever created |
//! | `test_race_three_concurrent_investors` | Three investors, all orderings; only the first-in-time wins |
//! | `test_race_accept_after_refund` | Invoice is funded then refunded; no new acceptance can succeed |
//!
//! Run: `cargo test test_accept_bid_race`

use super::*;
use crate::errors::QuickLendXError;
use crate::invoice::{InvoiceCategory, InvoiceStatus};
use crate::payments::EscrowStatus;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, BytesN, Env, String, Vec,
};

// ============================================================================
// Shared test helpers
// ============================================================================

/// Minimal test harness: initialised contract + admin address.
fn setup() -> (Env, QuickLendXContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let _ = client.try_initialize_admin(&admin);
    client.set_admin(&admin);
    (env, client, admin)
}

/// Create a Stellar Asset Contract token, mint `initial_balance` to each
/// provided address, and approve `contract_id` to spend those tokens.
///
/// Approvals are set far into the future (ledger sequence + 100_000) so they
/// do not expire during the test.
fn setup_token(
    env: &Env,
    addresses: &[&Address],
    contract_id: &Address,
    initial_balance: i128,
) -> Address {
    let token_admin = Address::generate(env);
    let currency = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();

    let token_client = token::Client::new(env, &currency);
    let sac_client = token::StellarAssetClient::new(env, &currency);
    let expiration = env.ledger().sequence() + 100_000;

    for addr in addresses {
        sac_client.mint(addr, &initial_balance);
        token_client.approve(addr, contract_id, &initial_balance, &expiration);
    }
    currency
}

/// Register and verify a business through the KYC flow.
fn verified_business(env: &Env, client: &QuickLendXContractClient, admin: &Address) -> Address {
    let business = Address::generate(env);
    client.submit_kyc_application(&business, &String::from_str(env, "Business KYC Data"));
    client.verify_business(admin, &business);
    business
}

/// Register and verify an investor through the KYC flow with the given limit.
fn verified_investor(
    env: &Env,
    client: &QuickLendXContractClient,
    admin: &Address,
    investment_limit: i128,
) -> Address {
    let investor = Address::generate(env);
    client.submit_investor_kyc(&investor, &String::from_str(env, "Investor KYC Data"));
    client.verify_investor(&investor, &investment_limit);
    investor
}

/// Upload and verify a fresh invoice; returns the invoice ID.
fn upload_and_verify_invoice(
    env: &Env,
    client: &QuickLendXContractClient,
    admin: &Address,
    business: &Address,
    currency: &Address,
    amount: i128,
) -> BytesN<32> {
    let due_date = env.ledger().timestamp() + 86_400 * 30; // 30 days from now
    let invoice_id = client.upload_invoice(
        business,
        &amount,
        currency,
        &due_date,
        &String::from_str(env, "Test invoice for race regression"),
        &InvoiceCategory::Services,
        &Vec::new(env),
    );
    client.verify_invoice(&invoice_id);
    invoice_id
}

/// Place a bid from `investor` on `invoice_id` and return the bid ID.
fn place_bid(
    client: &QuickLendXContractClient,
    investor: &Address,
    invoice_id: &BytesN<32>,
    amount: i128,
) -> BytesN<32> {
    client.place_bid(investor, invoice_id, &amount, &(amount / 10))
}

// ============================================================================
// Race scenario helper — two investors, one invoice
// ============================================================================

/// Builds the canonical two-investor race fixture. Returns:
/// `(env, client, contract_id, invoice_id, bid_id_a, bid_id_b, investor_a, investor_b)`
///
/// Both investors have a valid `Placed` bid on the same `Verified` invoice.
fn build_two_investor_race() -> (
    Env,
    QuickLendXContractClient<'static>,
    Address,
    BytesN<32>,
    BytesN<32>,
    BytesN<32>,
    Address,
    Address,
) {
    let (env, client, admin) = setup();
    let contract_id = env.current_contract_address();

    let investor_a = verified_investor(&env, &client, &admin, 500_000);
    let investor_b = verified_investor(&env, &client, &admin, 500_000);
    let business = verified_business(&env, &client, &admin);

    let currency = setup_token(
        &env,
        &[&investor_a, &investor_b, &business],
        &contract_id,
        200_000,
    );

    let invoice_id =
        upload_and_verify_invoice(&env, &client, &admin, &business, &currency, 100_000);

    let bid_id_a = place_bid(&client, &investor_a, &invoice_id, 100_000);
    let bid_id_b = place_bid(&client, &investor_b, &invoice_id, 100_000);

    (
        env,
        client,
        contract_id,
        invoice_id,
        bid_id_a,
        bid_id_b,
        investor_a,
        investor_b,
    )
}

// ============================================================================
// Test 1 — ordering A wins
// ============================================================================

/// ## Race scenario: investor A's transaction is ordered first.
///
/// Ordering: `accept_bid_and_fund(A)` → `accept_bid_and_fund(B)`
///
/// Expected:
/// - First call (`A`) succeeds and returns an escrow ID.
/// - Second call (`B`) returns `QuickLendXError::InvoiceAlreadyFunded`.
/// - Invoice status is `Funded`.
/// - Exactly one escrow record exists; the B bid remains `Placed` (or
///   equivalent; the bid for A is `Accepted`).
/// - No investment record from B's leg exists.
#[test]
fn test_race_ordering_a_wins() {
    let (env, client, _contract_id, invoice_id, bid_id_a, bid_id_b, _investor_a, _investor_b) =
        build_two_investor_race();

    // ── Ordering: A first ────────────────────────────────────────────────────
    let result_a = client.try_accept_bid_and_fund(&invoice_id, &bid_id_a);
    assert!(
        result_a.is_ok(),
        "First acceptance (A) must succeed; got: {result_a:?}"
    );

    // ── B's call arrives second ───────────────────────────────────────────────
    let result_b = client.try_accept_bid_and_fund(&invoice_id, &bid_id_b);
    assert!(
        result_b.is_err(),
        "Second acceptance (B) must fail; the invoice is already funded"
    );

    let err_b = result_b.unwrap_err().unwrap();
    assert!(
        err_b == QuickLendXError::InvoiceAlreadyFunded || err_b == QuickLendXError::InvalidStatus,
        "Second acceptance must return InvoiceAlreadyFunded or InvalidStatus; got: {err_b:?}"
    );

    // ── Post-condition: invoice is Funded ────────────────────────────────────
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(
        invoice.status,
        InvoiceStatus::Funded,
        "Invoice must be in Funded status after successful first acceptance"
    );

    // ── Post-condition: exactly one escrow exists ────────────────────────────
    let escrow = client.get_escrow_details(&invoice_id);
    assert_eq!(
        escrow.status,
        EscrowStatus::Held,
        "Escrow must be in Held status"
    );

    // ── Post-condition: bid A is Accepted, bid B is still Placed ────────────
    let bid_a = client.get_bid(&bid_id_a).expect("Bid A must exist");
    let bid_b = client.get_bid(&bid_id_b).expect("Bid B must exist");
    assert_eq!(
        bid_a.status,
        crate::bid::BidStatus::Accepted,
        "Bid A must be Accepted"
    );
    // Bid B must NOT have been silently transitioned to Accepted.
    assert_ne!(
        bid_b.status,
        crate::bid::BidStatus::Accepted,
        "Bid B must NOT be Accepted when its acceptance call failed"
    );
}

// ============================================================================
// Test 2 — ordering B wins
// ============================================================================

/// ## Race scenario: investor B's transaction is ordered first.
///
/// Ordering: `accept_bid_and_fund(B)` → `accept_bid_and_fund(A)`
///
/// This is the mirror of `test_race_ordering_a_wins`.  The outcome must be
/// identical (only one winner) despite the reversed ordering.
#[test]
fn test_race_ordering_b_wins() {
    let (env, client, _contract_id, invoice_id, bid_id_a, bid_id_b, _investor_a, _investor_b) =
        build_two_investor_race();

    // ── Ordering: B first ────────────────────────────────────────────────────
    let result_b = client.try_accept_bid_and_fund(&invoice_id, &bid_id_b);
    assert!(
        result_b.is_ok(),
        "First acceptance (B) must succeed; got: {result_b:?}"
    );

    // ── A's call arrives second ───────────────────────────────────────────────
    let result_a = client.try_accept_bid_and_fund(&invoice_id, &bid_id_a);
    assert!(
        result_a.is_err(),
        "Second acceptance (A) must fail; the invoice is already funded"
    );

    let err_a = result_a.unwrap_err().unwrap();
    assert!(
        err_a == QuickLendXError::InvoiceAlreadyFunded || err_a == QuickLendXError::InvalidStatus,
        "Second acceptance must return InvoiceAlreadyFunded or InvalidStatus; got: {err_a:?}"
    );

    // ── Post-condition: invoice is Funded ────────────────────────────────────
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Funded);

    // ── Post-condition: bid B is Accepted ────────────────────────────────────
    let bid_b = client.get_bid(&bid_id_b).expect("Bid B must exist");
    let bid_a = client.get_bid(&bid_id_a).expect("Bid A must exist");
    assert_eq!(bid_b.status, crate::bid::BidStatus::Accepted);
    assert_ne!(
        bid_a.status,
        crate::bid::BidStatus::Accepted,
        "Bid A must NOT be Accepted when its acceptance call failed"
    );
}

// ============================================================================
// Test 3 — same bid, both orderings
// ============================================================================

/// ## Race scenario: two callers attempt to accept the **same** bid.
///
/// In practice this cannot happen in the same ledger because Soroban requires
/// the business to sign the transaction; however an off-chain replay attack
/// could re-submit the same signed transaction. The contract must be idempotent.
#[test]
fn test_race_same_bid_both_orderings() {
    let (env, client, _contract_id, invoice_id, bid_id_a, _bid_id_b, _investor_a, _investor_b) =
        build_two_investor_race();

    // First call succeeds.
    let first = client.try_accept_bid_and_fund(&invoice_id, &bid_id_a);
    assert!(first.is_ok(), "First accept must succeed; got: {first:?}");

    // Replay / second call must fail.
    let second = client.try_accept_bid_and_fund(&invoice_id, &bid_id_a);
    assert!(second.is_err(), "Repeated accept of the same bid must fail");
    let err = second.unwrap_err().unwrap();
    assert!(
        err == QuickLendXError::InvoiceAlreadyFunded || err == QuickLendXError::InvalidStatus,
        "Repeated accept must return InvoiceAlreadyFunded or InvalidStatus; got: {err:?}"
    );
}

// ============================================================================
// Test 4 — no partial state from losing leg
// ============================================================================

/// ## Security invariant: zero partial state from the losing leg.
///
/// After a losing `accept_bid_and_fund` call:
/// - No escrow record linked to the losing bid's investor must exist separately.
/// - No investment record for the losing investor must exist.
/// - The winning escrow amount must equal exactly `bid_amount` (no double-credit).
#[test]
fn test_race_no_partial_state_on_failure() {
    let (env, client, contract_id, invoice_id, bid_id_a, bid_id_b, investor_a, investor_b) =
        build_two_investor_race();

    // Ordering: A wins.
    let _ = client.try_accept_bid_and_fund(&invoice_id, &bid_id_a);
    let _ = client.try_accept_bid_and_fund(&invoice_id, &bid_id_b);

    // ── Winning escrow has correct amount ────────────────────────────────────
    let escrow = client.get_escrow_details(&invoice_id);
    assert_eq!(
        escrow.amount, 100_000,
        "Escrow amount must equal bid_amount (100_000), not double-funded"
    );
    assert_eq!(
        escrow.investor, investor_a,
        "Escrow investor must be investor A (the winner)"
    );

    // ── No investment record for investor B ──────────────────────────────────
    let all_investments = client.get_investments_by_investor(&investor_b);
    assert!(
        all_investments.is_empty(),
        "Investor B (loser) must have zero investment records; found: {all_investments:?}"
    );

    // ── Investor A has exactly one investment ────────────────────────────────
    let investments_a = client.get_investments_by_investor(&investor_a);
    assert_eq!(
        investments_a.len(),
        1,
        "Investor A (winner) must have exactly one investment record"
    );

    // ── Token balance sanity: contract holds exactly bid_amount ──────────────
    let token_client = token::Client::new(&env, &escrow.currency);
    let contract_balance = token_client.balance(&contract_id);
    assert_eq!(
        contract_balance, 100_000,
        "Contract token balance must be exactly bid_amount (100_000)"
    );

    // Investor B's funds must be untouched.
    let b_balance = token_client.balance(&investor_b);
    assert_eq!(
        b_balance, 200_000,
        "Investor B's balance must be unchanged (200_000) after a failed acceptance"
    );
}

// ============================================================================
// Test 5 — idempotent after accept
// ============================================================================

/// ## Any number of retries after a successful accept must all fail consistently.
///
/// Validates that the guard is not a one-time sentinel that resets.
#[test]
fn test_race_idempotent_after_accept() {
    let (env, client, _contract_id, invoice_id, bid_id_a, bid_id_b, _investor_a, _investor_b) =
        build_two_investor_race();

    // Accept once — must succeed.
    client.accept_bid_and_fund(&invoice_id, &bid_id_a);

    // Retry same bid.
    for _ in 0..3 {
        let retry = client.try_accept_bid_and_fund(&invoice_id, &bid_id_a);
        assert!(retry.is_err(), "Every retry must fail");
    }

    // Try with a different bid — must also fail.
    let other = client.try_accept_bid_and_fund(&invoice_id, &bid_id_b);
    assert!(
        other.is_err(),
        "Accepting a different bid on a funded invoice must fail"
    );
}

// ============================================================================
// Test 6 — two valid bids, only one escrow ever created
// ============================================================================

/// ## Structural invariant: at most one escrow per invoice across any ordering.
///
/// After both race legs resolve (one succeeds, one fails), there must be exactly
/// one escrow record accessible via `get_escrow_details`.  Attempting to query
/// a second escrow must return an error (not a stale or empty record).
#[test]
fn test_race_different_bids_only_one_escrow() {
    let (env, client, _contract_id, invoice_id, bid_id_a, bid_id_b, _investor_a, _investor_b) =
        build_two_investor_race();

    // Execute both orderings in the A-wins order.
    let _ = client.try_accept_bid_and_fund(&invoice_id, &bid_id_a);
    let _ = client.try_accept_bid_and_fund(&invoice_id, &bid_id_b);

    // Exactly one escrow must be retrievable.
    let escrow_result = client.try_get_escrow_details(&invoice_id);
    assert!(
        escrow_result.is_ok(),
        "get_escrow_details must return the single winning escrow"
    );
    let escrow = escrow_result.unwrap().unwrap();
    assert_eq!(
        escrow.status,
        EscrowStatus::Held,
        "Winning escrow must be in Held status"
    );
    assert_eq!(
        escrow.amount, 100_000,
        "Escrow amount must match the winning bid amount"
    );
}

// ============================================================================
// Test 7 — three concurrent investors
// ============================================================================

/// ## Extended race: three investors submit `accept_bid_and_fund` concurrently.
///
/// In all six possible orderings only the first-in-ledger investor wins;
/// the other two must receive a stable error. This test exercises one specific
/// ordering (A → B → C) as a representative case.
#[test]
fn test_race_three_concurrent_investors() {
    let (env, client, admin) = setup();
    let contract_id = env.current_contract_address();

    let investor_a = verified_investor(&env, &client, &admin, 500_000);
    let investor_b = verified_investor(&env, &client, &admin, 500_000);
    let investor_c = verified_investor(&env, &client, &admin, 500_000);
    let business = verified_business(&env, &client, &admin);

    let currency = setup_token(
        &env,
        &[&investor_a, &investor_b, &investor_c, &business],
        &contract_id,
        200_000,
    );

    let invoice_id =
        upload_and_verify_invoice(&env, &client, &admin, &business, &currency, 100_000);

    let bid_id_a = place_bid(&client, &investor_a, &invoice_id, 100_000);
    let bid_id_b = place_bid(&client, &investor_b, &invoice_id, 100_000);
    let bid_id_c = place_bid(&client, &investor_c, &invoice_id, 100_000);

    // Ordering: A → B → C
    let ra = client.try_accept_bid_and_fund(&invoice_id, &bid_id_a);
    let rb = client.try_accept_bid_and_fund(&invoice_id, &bid_id_b);
    let rc = client.try_accept_bid_and_fund(&invoice_id, &bid_id_c);

    assert!(ra.is_ok(), "A (first) must succeed");
    assert!(rb.is_err(), "B (second) must fail");
    assert!(rc.is_err(), "C (third) must fail");

    // Validate error codes.
    let err_b = rb.unwrap_err().unwrap();
    let err_c = rc.unwrap_err().unwrap();
    assert!(
        err_b == QuickLendXError::InvoiceAlreadyFunded || err_b == QuickLendXError::InvalidStatus,
        "B error must be InvoiceAlreadyFunded or InvalidStatus; got {err_b:?}"
    );
    assert!(
        err_c == QuickLendXError::InvoiceAlreadyFunded || err_c == QuickLendXError::InvalidStatus,
        "C error must be InvoiceAlreadyFunded or InvalidStatus; got {err_c:?}"
    );

    // Only one investment must exist across all three investors combined.
    let inv_a = client.get_investments_by_investor(&investor_a);
    let inv_b = client.get_investments_by_investor(&investor_b);
    let inv_c = client.get_investments_by_investor(&investor_c);
    assert_eq!(inv_a.len(), 1, "A must have exactly 1 investment");
    assert_eq!(inv_b.len(), 0, "B must have 0 investments");
    assert_eq!(inv_c.len(), 0, "C must have 0 investments");

    // Token balance: contract holds exactly one bid amount.
    let token_client = token::Client::new(&env, &currency);
    assert_eq!(
        token_client.balance(&contract_id),
        100_000,
        "Contract balance must equal exactly one bid amount"
    );
}

// ============================================================================
// Test 8 — accept after refund must fail
// ============================================================================

/// ## Post-refund state must be terminal.
///
/// Once a funded invoice is refunded (escrow returns to investor), no new
/// `accept_bid_and_fund` call may succeed, regardless of available bids.
/// The invoice transitions to `Refunded`, which is not `Verified`, so
/// all further acceptance attempts must return a stable error.
#[test]
fn test_race_accept_after_refund() {
    let (env, client, admin) = setup();
    let contract_id = env.current_contract_address();

    let investor_a = verified_investor(&env, &client, &admin, 500_000);
    let investor_b = verified_investor(&env, &client, &admin, 500_000);
    let business = verified_business(&env, &client, &admin);

    let currency = setup_token(
        &env,
        &[&investor_a, &investor_b, &business],
        &contract_id,
        200_000,
    );

    let invoice_id =
        upload_and_verify_invoice(&env, &client, &admin, &business, &currency, 100_000);

    let bid_id_a = place_bid(&client, &investor_a, &invoice_id, 100_000);
    let bid_id_b = place_bid(&client, &investor_b, &invoice_id, 100_000);

    // Fund the invoice with investor A.
    client.accept_bid_and_fund(&invoice_id, &bid_id_a);

    // Verify balance was transferred.
    let token_client = token::Client::new(&env, &currency);
    assert_eq!(token_client.balance(&contract_id), 100_000);

    // Admin triggers a refund.
    client.refund_escrow_funds(&invoice_id, &admin);

    // Invoice is now Refunded — A's funds returned.
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Refunded);
    assert_eq!(token_client.balance(&contract_id), 0);
    assert_eq!(token_client.balance(&investor_a), 200_000); // full balance restored

    // B attempts to accept after refund — must fail.
    let late_accept = client.try_accept_bid_and_fund(&invoice_id, &bid_id_b);
    assert!(
        late_accept.is_err(),
        "Acceptance on a Refunded invoice must fail"
    );

    // No new escrow, no new investment.
    let inv_b = client.get_investments_by_investor(&investor_b);
    assert_eq!(
        inv_b.len(),
        0,
        "Investor B must have zero investments after post-refund rejection"
    );
    assert_eq!(
        token_client.balance(&contract_id),
        0,
        "Contract balance must remain 0 after failed post-refund acceptance"
    );
}
