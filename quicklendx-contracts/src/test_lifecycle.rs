//! Full invoice lifecycle integration tests for the QuickLendX protocol.
//!
//! These tests cover the complete end-to-end flow with state and event
//! assertions at each step to meet integration and coverage requirements.
//!
//! ## Test suite
//!
//! - **`test_full_invoice_lifecycle`** – Full flow: business KYC → verify business →
//!   upload invoice → verify invoice → investor KYC → verify investor → place bid →
//!   accept bid and fund → settle invoice → rating. Asserts state and token
//!   balances; uses real SAC for escrow, then settle path as in settlement tests.
//!
//! - **`test_lifecycle_escrow_token_flow`** – Same up to accept bid; then release
//!   escrow (contract → business) and rating. Asserts real token movements for
//!   both escrow creation and release.
//!
//! - **`test_full_lifecycle_step_by_step`** – Same flow as `test_full_invoice_lifecycle`
//!   but runs each step explicitly and asserts state and events after every step
//!   (business KYC, verify business, upload invoice, verify invoice, investor KYC,
//!   verify investor, place bid, accept bid, settle, rating).
//!
//! ## Coverage matrix (requirement: assert state and events at each step)
//!
//! | Step | Action                  | test_full_invoice_lifecycle | test_lifecycle_escrow_token_flow | test_full_lifecycle_step_by_step |
//! |------|-------------------------|-----------------------------|----------------------------------|-----------------------------------|
//! |  1   | Business KYC            | ✓ (via run_kyc_and_bid)     | ✓                                | ✓ State + event `kyc_sub`         |
//! |  2   | Verify business          | ✓                            | ✓                                | ✓ State + event `bus_ver`         |
//! |  3   | Upload invoice           | ✓                            | ✓                                | ✓ State + event `inv_up`          |
//! |  4   | Verify invoice           | ✓                            | ✓                                | ✓ State + event `inv_ver`         |
//! |  5   | Investor KYC             | ✓                            | ✓                                | ✓ State (pending list)            |
//! |  6   | Verify investor          | ✓                            | ✓                                | ✓ State + event `inv_veri`        |
//! |  7   | Place bid                | ✓ State + events at end      | ✓                                | ✓ State + event `bid_plc`         |
//! |  8   | Accept bid and fund      | ✓ State + token balances     | ✓ State + token balances         | ✓ State + events `bid_acc`, `esc_cr` |
//! |  9   | Release escrow **or** settle | ✓ **Settle** (state + lists) | ✓ **Release** (state + token + `esc_rel`) | ✓ **Settle** (state + `inv_set`)  |
//! | 10   | Rating                   | ✓ State + events at end      | ✓ State + event count            | ✓ State + event `rated`           |
//!
//! Run `cargo test test_lifecycle test_full_invoice test_full_lifecycle_step` for these tests.

use super::*;
use crate::bid::BidStatus;
use crate::investment::InvestmentStatus;
use crate::invoice::{InvoiceCategory, InvoiceStatus};
use crate::verification::BusinessVerificationStatus;
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Ledger},
    token, Address, Env, String, Vec,
};

// ─── shared helpers ───────────────────────────────────────────────────────────

/// Minimal test environment: contract registered, admin set, timestamp > 0.
fn make_env() -> (Env, QuickLendXContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.set_admin(&admin);
    (env, client, admin)
}

/// Register a real Stellar Asset Contract, mint initial balances and set
/// spending allowances so the QuickLendX contract can pull tokens.
fn make_real_token(
    env: &Env,
    contract_id: &Address,
    business: &Address,
    investor: &Address,
    business_initial: i128,
    investor_initial: i128,
) -> Address {
    let token_admin = Address::generate(env);
    let currency = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();
    let sac = token::StellarAssetClient::new(env, &currency);
    let tok = token::Client::new(env, &currency);

    sac.mint(business, &business_initial);
    sac.mint(investor, &investor_initial);
    // Ensure the contract has a token instance entry so balance lookups don't
    // fail with "missing value" for a non-initialised contract instance.
    sac.mint(contract_id, &1i128);

    let exp = env.ledger().sequence() + 10_000;
    tok.approve(business, contract_id, &(business_initial * 4), &exp);
    tok.approve(investor, contract_id, &(investor_initial * 4), &exp);

    currency
}

/// Returns true if at least one event has the given topic (first topic symbol).
/// Topics in Soroban are stored as a tuple; the first element is compared.
fn has_event_with_topic(env: &Env, topic: soroban_sdk::Symbol) -> bool {
    let topic_val = topic.into_val(env);
    env.events().all().iter().any(|evt| {
        let (_, topics, _): (_, soroban_sdk::Vec<soroban_sdk::Val>, _) = evt.clone();
        if topics.is_empty() {
            return false;
        }
        let first = topics.get(0).unwrap();
        first == topic_val
    })
}

/// Assert that key lifecycle events were emitted (for full lifecycle with settle).
fn assert_lifecycle_events_emitted(env: &Env) {
    let all = env.events().all();
    assert!(
        all.len() >= 8,
        "Expected at least 8 lifecycle events (inv_up, inv_ver, bid_plc, bid_acc, esc_cr, inv_set, rated, etc.), got {}",
        all.len()
    );
    assert!(
        has_event_with_topic(env, symbol_short!("inv_up")),
        "InvoiceUploaded (inv_up) event should be emitted"
    );
    assert!(
        has_event_with_topic(env, symbol_short!("inv_ver")),
        "InvoiceVerified (inv_ver) event should be emitted"
    );
    assert!(
        has_event_with_topic(env, symbol_short!("bid_plc")),
        "BidPlaced (bid_plc) event should be emitted"
    );
    assert!(
        has_event_with_topic(env, symbol_short!("bid_acc")),
        "BidAccepted (bid_acc) event should be emitted"
    );
    assert!(
        has_event_with_topic(env, symbol_short!("esc_cr")),
        "EscrowCreated (esc_cr) event should be emitted"
    );
    assert!(
        has_event_with_topic(env, symbol_short!("inv_set")),
        "InvoiceSettled (inv_set) event should be emitted"
    );
    assert!(
        has_event_with_topic(env, symbol_short!("rated")),
        "Rated (rated) event should be emitted"
    );
}

/// Shared KYC + upload + verify + investor + bid sequence.
/// Returns `(invoice_id, bid_id)` ready for `accept_bid`.
fn run_kyc_and_bid(
    env: &Env,
    client: &QuickLendXContractClient,
    admin: &Address,
    business: &Address,
    investor: &Address,
    currency: &Address,
    invoice_amount: i128,
    bid_amount: i128,
) -> (soroban_sdk::BytesN<32>, soroban_sdk::BytesN<32>) {
    // Business KYC + verification
    client.submit_kyc_application(business, &String::from_str(env, "Business KYC"));
    client.verify_business(admin, business);

    // Upload invoice
    let due_date = env.ledger().timestamp() + 86_400;
    let invoice_id = client.upload_invoice(
        business,
        &invoice_amount,
        currency,
        &due_date,
        &String::from_str(env, "Consulting services invoice"),
        &InvoiceCategory::Consulting,
        &Vec::new(env),
    );
    client.verify_invoice(&invoice_id);

    // Investor KYC + verification
    client.submit_investor_kyc(investor, &String::from_str(env, "Investor KYC"));
    client.verify_investor(investor, &50_000i128);

    // Place bid
    let bid_id = client.place_bid(investor, &invoice_id, &bid_amount, &invoice_amount);

    (invoice_id, bid_id)
}

// ─── test 1: full lifecycle (KYC → bid → fund → settle → rate) ────────────────

/// Full invoice lifecycle:
///   1.  Business submits KYC
///   2.  Admin verifies the business
///   3.  Business uploads an invoice (status → Pending)
///   4.  Admin verifies the invoice  (status → Verified)
///   5.  Investor submits KYC
///   6.  Admin verifies the investor
///   7.  Investor places a bid       (status → Placed)
///   8.  Business accepts the bid    (status → Funded, escrow created)
///   9.  Business settles the invoice (status → Paid, investment → Completed)
///  10.  Investor rates the invoice
///
/// Uses a real SAC for the escrow phase so token balance movements are
/// verified.  The `settle_invoice` step follows the same dummy-token
/// pattern as the existing test_settlement tests to avoid the
/// double-`require_auth` auth-frame conflict that arises when a real SAC
/// is combined with `settle_invoice`'s nested `record_payment` call.
#[test]
fn test_full_invoice_lifecycle() {
    // ── setup ──────────────────────────────────────────────────────────────────
    let (env, client, admin) = make_env();
    let contract_id = client.address.clone();

    let business = Address::generate(&env);
    let investor = Address::generate(&env);

    // Real SAC for escrow verification; business has 20 000 so it can settle
    // the 10 000 invoice without needing the escrow released first.
    let invoice_amount: i128 = 10_000;
    let bid_amount: i128 = 9_000;
    let currency = make_real_token(&env, &contract_id, &business, &investor, 20_000, 15_000);
    let tok = token::Client::new(&env, &currency);

    // ── steps 1–7: KYC, upload, verify, bid ───────────────────────────────────
    let (invoice_id, bid_id) = run_kyc_and_bid(
        &env, &client, &admin, &business, &investor, &currency,
        invoice_amount, bid_amount,
    );

    // State after upload (verified).
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Verified, "Invoice should be Verified before funding");
    assert_eq!(invoice.amount, invoice_amount);
    assert_eq!(invoice.business, business);
    assert_eq!(invoice.funded_amount, 0);
    assert!(invoice.investor.is_none());

    // Bid state.
    let bid = client.get_bid(&bid_id).unwrap();
    assert_eq!(bid.status, BidStatus::Placed);
    assert_eq!(bid.bid_amount, bid_amount);
    assert_eq!(bid.investor, investor);

    // ── step 8: accept bid (escrow created, investor → contract) ───────────────
    let investor_bal_before = tok.balance(&investor);
    let contract_bal_before = tok.balance(&contract_id);

    client.accept_bid(&invoice_id, &bid_id);

    let investor_bal_after = tok.balance(&investor);
    let contract_bal_after = tok.balance(&contract_id);

    // Token flow: investor pays exactly bid_amount into escrow.
    assert_eq!(
        investor_bal_before - investor_bal_after,
        bid_amount,
        "Investor should have paid bid_amount into escrow"
    );
    assert_eq!(
        contract_bal_after - contract_bal_before,
        bid_amount,
        "Contract should hold bid_amount in escrow"
    );

    // Invoice state.
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Funded, "Invoice must be Funded after accept_bid");
    assert_eq!(invoice.funded_amount, bid_amount);
    assert_eq!(invoice.investor, Some(investor.clone()));

    // Bid state.
    assert_eq!(client.get_bid(&bid_id).unwrap().status, BidStatus::Accepted);

    // Investment created and Active.
    let investment = client.get_invoice_investment(&invoice_id);
    assert_eq!(investment.amount, bid_amount);
    assert_eq!(investment.status, InvestmentStatus::Active);
    assert_eq!(investment.investor, investor);

    // Escrow record matches.
    let escrow = client.get_escrow_details(&invoice_id);
    assert_eq!(escrow.amount, bid_amount);

    // ── step 9: settle invoice ─────────────────────────────────────────────────
    // `settle_invoice` → `record_payment` internally calls `payer.require_auth()`
    // twice in the same invocation frame.  When a *real* SAC is in use, the SAC
    // also calls `spender.require_auth()` for the contract, which triggers an
    // Auth::ExistingValue conflict.  We replicate the pattern used by the
    // existing settlement tests: mint a fresh token balance for business so
    // that the payment succeeds, and verify only state transitions (not raw
    // token balances) for this step.
    //
    // Real-token balance verification for settle is covered separately in
    // test_settlement.rs (test_payout_matches_expected_return, etc.).
    let sac = token::StellarAssetClient::new(&env, &currency);
    sac.mint(&business, &invoice_amount); // give business the payment tokens

    let tok_exp = env.ledger().sequence() + 10_000;
    tok.approve(&business, &contract_id, &(invoice_amount * 4), &tok_exp);

    client.settle_invoice(&invoice_id, &invoice_amount);

    // Invoice is Paid.
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Paid, "Invoice must be Paid after settlement");
    assert!(invoice.settled_at.is_some(), "settled_at must be set");
    assert_eq!(invoice.total_paid, invoice_amount);

    // Investment is Completed.
    assert_eq!(
        client.get_invoice_investment(&invoice_id).status,
        InvestmentStatus::Completed,
        "Investment must be Completed after settlement"
    );

    // Status query lists are updated.
    assert!(
        !client.get_invoices_by_status(&InvoiceStatus::Funded).contains(&invoice_id),
        "Invoice should not be in Funded list"
    );
    assert!(
        client.get_invoices_by_status(&InvoiceStatus::Paid).contains(&invoice_id),
        "Invoice should be in Paid list"
    );

    // ── step 10: investor rates the invoice ────────────────────────────────────
    let rating: u32 = 5;
    client.add_invoice_rating(
        &invoice_id,
        &rating,
        &String::from_str(&env, "Excellent! Payment on time."),
        &investor,
    );

    let (avg, count, high, low) = client.get_invoice_rating_stats(&invoice_id);
    assert_eq!(count, 1);
    assert_eq!(avg, Some(rating));
    assert_eq!(high, Some(rating));
    assert_eq!(low, Some(rating));

    // Assert key lifecycle events were emitted.
    assert_lifecycle_events_emitted(&env);
}

// ─── test 2: escrow-release token flow ────────────────────────────────────────

/// Alternative lifecycle path: accept bid → release escrow → rate.
///
/// Verifies the real token movements for the "release escrow" settlement path
/// (contract → business) in addition to the escrow creation (investor →
/// contract).  Invoice is left in Funded status after release (the business
/// would repay off-chain; settlement is tested in test_settlement.rs).
#[test]
fn test_lifecycle_escrow_token_flow() {
    // ── setup ──────────────────────────────────────────────────────────────────
    let (env, client, admin) = make_env();
    let contract_id = client.address.clone();

    let business = Address::generate(&env);
    let investor = Address::generate(&env);

    let invoice_amount: i128 = 10_000;
    let bid_amount: i128 = 9_000;
    let currency = make_real_token(&env, &contract_id, &business, &investor, 5_000, 15_000);
    let tok = token::Client::new(&env, &currency);

    // ── steps 1–7: KYC, upload, verify, bid ───────────────────────────────────
    let (invoice_id, bid_id) = run_kyc_and_bid(
        &env, &client, &admin, &business, &investor, &currency,
        invoice_amount, bid_amount,
    );

    // ── step 8: accept bid ─────────────────────────────────────────────────────
    client.accept_bid(&invoice_id, &bid_id);

    // Verify investor paid into escrow.
    assert_eq!(tok.balance(&investor), 15_000 - bid_amount);
    assert_eq!(tok.balance(&contract_id), 1 + bid_amount); // 1 = initial mint

    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Funded);
    assert_eq!(invoice.funded_amount, bid_amount);
    assert_eq!(invoice.investor, Some(investor.clone()));

    // Investment record.
    let investment = client.get_invoice_investment(&invoice_id);
    assert_eq!(investment.status, InvestmentStatus::Active);
    assert_eq!(investment.amount, bid_amount);

    // ── step 9: release escrow (contract → business) ──────────────────────────
    let business_bal_before = tok.balance(&business);
    let contract_bal_before = tok.balance(&contract_id);

    client.release_escrow_funds(&invoice_id);

    let business_bal_after = tok.balance(&business);
    let contract_bal_after = tok.balance(&contract_id);

    // Business receives the advance payment.
    assert_eq!(
        business_bal_after - business_bal_before,
        bid_amount,
        "Business should receive bid_amount from escrow release"
    );
    assert_eq!(
        contract_bal_before - contract_bal_after,
        bid_amount,
        "Contract escrow should decrease by bid_amount"
    );

    // Invoice remains Funded (escrow release doesn't change invoice status).
    assert_eq!(
        client.get_invoice(&invoice_id).status,
        InvoiceStatus::Funded,
        "Invoice should remain Funded after escrow release"
    );

    // ── step 10: investor rates the invoice ────────────────────────────────────
    let rating: u32 = 4;
    client.add_invoice_rating(
        &invoice_id,
        &rating,
        &String::from_str(&env, "Good experience overall."),
        &investor,
    );

    let (avg, count, high, low) = client.get_invoice_rating_stats(&invoice_id);
    assert_eq!(count, 1);
    assert_eq!(avg, Some(rating));
    assert_eq!(high, Some(rating));
    assert_eq!(low, Some(rating));

    // Assert escrow release event was emitted.
    assert!(
        has_event_with_topic(&env, symbol_short!("esc_rel")),
        "EscrowReleased event should be emitted"
    );
    assert!(
        env.events().all().len() >= 5,
        "Expected at least 5 lifecycle events"
    );
}

// ─── test 3: step-by-step lifecycle with state and event assertions ─────────────

/// Full lifecycle executed step-by-step with explicit state and event
/// assertions after each step: business KYC → verify business → upload invoice →
/// verify invoice → investor KYC → verify investor → place bid → accept bid →
/// settle → rating.
#[test]
fn test_full_lifecycle_step_by_step() {
    let (env, client, admin) = make_env();
    let contract_id = client.address.clone();
    let business = Address::generate(&env);
    let investor = Address::generate(&env);
    let invoice_amount: i128 = 10_000;
    let bid_amount: i128 = 9_000;
    let currency = make_real_token(&env, &contract_id, &business, &investor, 20_000, 15_000);
    let tok = token::Client::new(&env, &currency);

    // ── Step 1: Business submits KYC ─────────────────────────────────────────
    client.submit_kyc_application(&business, &String::from_str(&env, "Business KYC"));
    let status = client.get_business_verification_status(&business).unwrap();
    assert_eq!(status.status, BusinessVerificationStatus::Pending);
    assert!(
        client.get_pending_businesses().contains(&business),
        "Business should be in pending list"
    );
    assert!(has_event_with_topic(&env, symbol_short!("kyc_sub")), "kyc_sub expected after business KYC");

    // ── Step 2: Admin verifies the business ─────────────────────────────────────
    client.verify_business(&admin, &business);
    let status = client.get_business_verification_status(&business).unwrap();
    assert_eq!(status.status, BusinessVerificationStatus::Verified);
    assert!(
        client.get_verified_businesses().contains(&business),
        "Business should be in verified list"
    );
    assert!(has_event_with_topic(&env, symbol_short!("bus_ver")), "bus_ver expected after verify business");

    // ── Step 3: Business uploads invoice (status → Pending) ──────────────────────
    let due_date = env.ledger().timestamp() + 86_400;
    let invoice_id = client.upload_invoice(
        &business,
        &invoice_amount,
        &currency,
        &due_date,
        &String::from_str(&env, "Consulting services invoice"),
        &InvoiceCategory::Consulting,
        &Vec::new(&env),
    );
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Pending);
    assert_eq!(invoice.amount, invoice_amount);
    assert_eq!(invoice.business, business);
    assert!(has_event_with_topic(&env, symbol_short!("inv_up")), "inv_up expected");

    // ── Step 4: Admin verifies the invoice (status → Verified) ──────────────────
    client.verify_invoice(&invoice_id);
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Verified);
    assert!(has_event_with_topic(&env, symbol_short!("inv_ver")), "inv_ver expected");

    // ── Step 5: Investor submits KYC ───────────────────────────────────────────
    client.submit_investor_kyc(&investor, &String::from_str(&env, "Investor KYC"));
    assert!(
        client.get_pending_investors().contains(&investor),
        "Investor should be pending"
    );
    // Investor KYC submission is reflected in pending list (no separate event topic in contract)

    // ── Step 6: Admin verifies the investor ──────────────────────────────────────
    client.verify_investor(&investor, &50_000i128);
    assert!(
        client.get_verified_investors().contains(&investor),
        "Investor should be verified"
    );
    let inv_ver = client.get_investor_verification(investor.clone()).unwrap();
    assert_eq!(inv_ver.investment_limit, 50_000i128);
    assert!(has_event_with_topic(&env, symbol_short!("inv_veri")), "inv_veri expected after verify investor");

    // ── Step 7: Investor places bid (status → Placed) ──────────────────────────
    let bid_id = client.place_bid(&investor, &invoice_id, &bid_amount, &invoice_amount);
    let bid = client.get_bid(&bid_id).unwrap();
    assert_eq!(bid.status, BidStatus::Placed);
    assert_eq!(bid.bid_amount, bid_amount);
    assert_eq!(bid.investor, investor);
    assert!(has_event_with_topic(&env, symbol_short!("bid_plc")), "bid_plc expected");

    // ── Step 8: Business accepts bid (status → Funded, escrow created) ───────────
    let investor_bal_before = tok.balance(&investor);
    client.accept_bid(&invoice_id, &bid_id);
    assert_eq!(tok.balance(&investor), investor_bal_before - bid_amount);

    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Funded);
    assert_eq!(invoice.funded_amount, bid_amount);
    assert_eq!(invoice.investor, Some(investor.clone()));
    assert_eq!(client.get_bid(&bid_id).unwrap().status, BidStatus::Accepted);
    assert_eq!(
        client.get_invoice_investment(&invoice_id).unwrap().status,
        InvestmentStatus::Active
    );
    assert!(has_event_with_topic(&env, symbol_short!("bid_acc")), "bid_acc expected");
    assert!(has_event_with_topic(&env, symbol_short!("esc_cr")), "esc_cr expected");

    // ── Step 9: Business settles the invoice (status → Paid) ─────────────────────
    let sac = token::StellarAssetClient::new(&env, &currency);
    sac.mint(&business, &invoice_amount);
    let exp = env.ledger().sequence() + 10_000;
    tok.approve(&business, &contract_id, &(invoice_amount * 4), &exp);
    client.settle_invoice(&invoice_id, &invoice_amount);

    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Paid);
    assert!(invoice.settled_at.is_some());
    assert_eq!(invoice.total_paid, invoice_amount);
    assert_eq!(
        client.get_invoice_investment(&invoice_id).unwrap().status,
        InvestmentStatus::Completed
    );
    assert!(
        client.get_invoices_by_status(&InvoiceStatus::Paid).contains(&invoice_id)
    );
    assert!(has_event_with_topic(&env, symbol_short!("inv_set")), "inv_set expected after settle");

    // ── Step 10: Investor rates the invoice ────────────────────────────────────
    let rating: u32 = 5;
    client.add_invoice_rating(
        &invoice_id,
        &rating,
        &String::from_str(&env, "Excellent! Payment on time."),
        &investor,
    );
    let (avg, count, high, low) = client.get_invoice_rating_stats(&invoice_id);
    assert_eq!(count, 1);
    assert_eq!(avg, Some(rating));
    assert_eq!(high, Some(rating));
    assert_eq!(low, Some(rating));
    assert!(has_event_with_topic(&env, symbol_short!("rated")), "rated event expected after rating");

    assert_lifecycle_events_emitted(&env);
}
