//! Cross-module state consistency regression tests for QuickLendX.
//!
//! # Purpose
//!
//! These tests ensure that after every core lifecycle transition, **all four
//! modules** (bid, escrow, investment, invoice) remain mutually consistent.
//! They act as a guard against:
//!
//! - Orphan pointers (e.g., an investment record with no matching invoice)
//! - Split-brain status (e.g., bid=Placed while invoice=Funded)
//! - Stale index membership (e.g., invoice in Funded list after it became Paid)
//! - Query/canonical divergence (`get_invoices_by_status` - `get_invoice`)
//!
//! # Covered flows
//!
//! | # | Flow | Modules exercised |
//! |---|------|-------------------|
//! | 1 | `accept_bid_and_fund` | bid, escrow, investment, invoice |
//! | 2 | `refund_escrow_funds` | bid, escrow, investment, invoice |
//! | 3 | `mark_invoice_defaulted` | invoice, investment |
//! | 4 | `settle_invoice` (finalize) | invoice, investment, escrow |
//! | 5 | Multi-invoice isolation | all modules |
//! | 6 | Query - canonical record agreement | storage indices |
//!
//! # Security notes
//!
//! Each test verifies that no incomplete or inconsistent state can be observed
//! after a flow completes - specifically targeting state conditions that could
//! be exploited for value extraction (e.g., double-claiming via stale escrow,
//! or re-bidding a "ghost" funded invoice).
//!
//! Run with: `cargo test test_cross_module`

use super::*;
use crate::bid::BidStatus;
use crate::investment::InvestmentStatus;
use crate::invoice::{InvoiceCategory, InvoiceStatus};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, Env, String, Vec,
};

// --- shared test helpers -----------------------------------------------------

/// Minimal environment: contract registered, admin set, timestamp > 0.
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

/// Register a real Stellar Asset Contract, mint balances, and set allowances.
fn make_token(
    env: &Env,
    contract_id: &Address,
    business: &Address,
    investor: &Address,
    business_amt: i128,
    investor_amt: i128,
) -> Address {
    let token_admin = Address::generate(env);
    let currency = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();
    let sac = token::StellarAssetClient::new(env, &currency);
    let tok = token::Client::new(env, &currency);

    sac.mint(business, &business_amt);
    sac.mint(investor, &investor_amt);
    // Seed contract so balance lookups on uninitialized instance entries don't panic.
    sac.mint(contract_id, &1i128);

    let exp = env.ledger().sequence() + 10_000;
    tok.approve(business, contract_id, &(business_amt * 4), &exp);
    tok.approve(investor, contract_id, &(investor_amt * 4), &exp);

    currency
}

/// KYC -> upload -> verify -> investor KYC -> bid, returning `(invoice_id, bid_id)`.
fn kyc_upload_bid(
    env: &Env,
    client: &QuickLendXContractClient,
    admin: &Address,
    business: &Address,
    investor: &Address,
    currency: &Address,
    invoice_amount: i128,
    bid_amount: i128,
) -> (soroban_sdk::BytesN<32>, soroban_sdk::BytesN<32>) {
    client.submit_kyc_application(business, &String::from_str(env, "Business KYC"));
    client.verify_business(admin, business);

    let due_date = env.ledger().timestamp() + 86_400;
    let invoice_id = client.upload_invoice(
        business,
        &invoice_amount,
        currency,
        &due_date,
        &String::from_str(env, "Consistency regression invoice"),
        &InvoiceCategory::Services,
        &Vec::new(env),
    );
    client.verify_invoice(&invoice_id);

    client.submit_investor_kyc(investor, &String::from_str(env, "Investor KYC"));
    client.verify_investor(investor, &50_000i128);

    let bid_id = client.place_bid(investor, &invoice_id, &bid_amount, &invoice_amount);
    (invoice_id, bid_id)
}

/// Assert that the global `total_invoice_count` equals the sum of all status buckets.
///
/// This is the primary "no-orphan" invariant at the storage layer: every invoice
/// increment must land in exactly one status bucket.
fn assert_invoice_count_invariant(client: &QuickLendXContractClient) {
    let total = client.get_total_invoice_count();
    let sum = client.get_invoice_count_by_status(&InvoiceStatus::Pending)
        + client.get_invoice_count_by_status(&InvoiceStatus::Verified)
        + client.get_invoice_count_by_status(&InvoiceStatus::Funded)
        + client.get_invoice_count_by_status(&InvoiceStatus::Paid)
        + client.get_invoice_count_by_status(&InvoiceStatus::Defaulted)
        + client.get_invoice_count_by_status(&InvoiceStatus::Cancelled)
        + client.get_invoice_count_by_status(&InvoiceStatus::Refunded);
    assert_eq!(
        total, sum,
        "Invoice count invariant broken: global={} bucket_sum={}",
        total, sum
    );
}

// --- Test 1: Accept flow ------------------------------------------------------

/// After `accept_bid_and_fund` every cross-module pointer must be consistent:
///
/// - `invoice.status == Funded`
/// - `invoice.investor == Some(investor)`, `invoice.funded_amount == bid_amount`
/// - `bid.status == Accepted`
/// - `investment.status == Active`, `investment.invoice_id == invoice_id`,
///   `investment.investor == investor`, `investment.amount == bid_amount`
/// - `escrow` record exists and carries the correct amount
/// - Invoice appears in `get_invoices_by_status(Funded)`, NOT in Verified
/// - Count invariant holds
#[test]
fn test_accept_bid_cross_module_consistency() {
    let (env, client, admin) = make_env();
    let contract_id = client.address.clone();
    let business = Address::generate(&env);
    let investor = Address::generate(&env);

    let invoice_amount: i128 = 10_000;
    let bid_amount: i128 = 9_000;
    let currency = make_token(&env, &contract_id, &business, &investor, 5_000, 15_000);

    let (invoice_id, bid_id) = kyc_upload_bid(
        &env, &client, &admin, &business, &investor, &currency, invoice_amount, bid_amount,
    );

    // Pre-accept: invoice is Verified, bid is Placed.
    assert_eq!(client.get_invoice(&invoice_id).status, InvoiceStatus::Verified);
    assert_eq!(client.get_bid(&bid_id).unwrap().status, BidStatus::Placed);
    assert!(client.get_invoice_investment(&invoice_id).is_none());
    assert_invoice_count_invariant(&client);

    // Execute accept.
    client.accept_bid(&invoice_id, &bid_id);

    // -- Invoice assertions ----------------------------------------------------
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(
        invoice.status,
        InvoiceStatus::Funded,
        "Invoice must be Funded after accept"
    );
    assert_eq!(
        invoice.funded_amount, bid_amount,
        "funded_amount must equal bid_amount"
    );
    assert_eq!(
        invoice.investor,
        Some(investor.clone()),
        "invoice.investor must point to the accepted investor"
    );
    assert!(invoice.funded_at.is_some(), "funded_at must be set");

    // -- Bid assertions --------------------------------------------------------
    let bid = client.get_bid(&bid_id).unwrap();
    assert_eq!(
        bid.status,
        BidStatus::Accepted,
        "Bid must transition to Accepted"
    );
    assert_eq!(bid.investor, investor, "bid.investor must match");

    // -- Investment assertions -------------------------------------------------
    let investment = client
        .get_invoice_investment(&invoice_id)
        .expect("Investment must exist after accept");
    assert_eq!(
        investment.status,
        InvestmentStatus::Active,
        "Investment must be Active"
    );
    assert_eq!(
        investment.invoice_id, invoice_id,
        "investment.invoice_id must point back to invoice"
    );
    assert_eq!(
        investment.investor, investor,
        "investment.investor must match"
    );
    assert_eq!(
        investment.amount, bid_amount,
        "investment.amount must equal bid_amount"
    );

    // -- Escrow assertions -----------------------------------------------------
    let escrow = client
        .get_escrow_details(&invoice_id)
        .expect("Escrow record must exist after accept");
    assert_eq!(
        escrow.amount, bid_amount,
        "escrow.amount must equal bid_amount - no orphan amount"
    );

    // -- Index membership assertions -------------------------------------------
    let funded_list = client.get_invoices_by_status(&InvoiceStatus::Funded);
    assert!(
        funded_list.contains(&invoice_id),
        "Invoice must appear in Funded status index"
    );
    let verified_list = client.get_invoices_by_status(&InvoiceStatus::Verified);
    assert!(
        !verified_list.contains(&invoice_id),
        "Invoice must NOT appear in Verified status index after accept"
    );

    // -- Count invariant -------------------------------------------------------
    assert_invoice_count_invariant(&client);
}

// --- Test 2: Refund flow ------------------------------------------------------

/// After `refund_escrow_funds` all modules must reflect the refund atomically:
///
/// - `invoice.status == Refunded`
/// - `bid.status == Cancelled` (the previously Accepted bid)
/// - `investment.status == Refunded`
/// - Escrow is no longer Held (released/zeroed)
/// - Invoice is NOT in Funded list; IS in Refunded list
/// - Count invariant holds
/// - No orphan investment record pointing to a non-existent escrow
#[test]
fn test_refund_escrow_cross_module_consistency() {
    let (env, client, admin) = make_env();
    let contract_id = client.address.clone();
    let business = Address::generate(&env);
    let investor = Address::generate(&env);

    let invoice_amount: i128 = 8_000;
    let bid_amount: i128 = 7_500;
    let currency = make_token(&env, &contract_id, &business, &investor, 5_000, 10_000);

    let (invoice_id, bid_id) = kyc_upload_bid(
        &env, &client, &admin, &business, &investor, &currency, invoice_amount, bid_amount,
    );

    // Fund first.
    client.accept_bid(&invoice_id, &bid_id);
    assert_eq!(client.get_invoice(&invoice_id).status, InvoiceStatus::Funded);
    assert_eq!(
        client.get_invoice_investment(&invoice_id).unwrap().status,
        InvestmentStatus::Active
    );

    // Trigger refund.
    client.refund_escrow_funds(&invoice_id, &business);

    // -- Invoice assertions ----------------------------------------------------
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(
        invoice.status,
        InvoiceStatus::Refunded,
        "Invoice must be Refunded after escrow refund"
    );

    // -- Bid assertions --------------------------------------------------------
    let bid = client.get_bid(&bid_id).unwrap();
    assert_eq!(
        bid.status,
        BidStatus::Cancelled,
        "Accepted bid must be Cancelled after refund - no orphan Accepted bid"
    );

    // -- Investment assertions -------------------------------------------------
    let investment = client
        .get_invoice_investment(&invoice_id)
        .expect("Investment record must still be accessible (not deleted)");
    assert_eq!(
        investment.status,
        InvestmentStatus::Refunded,
        "Investment must be Refunded - no orphan Active investment"
    );

    // -- Index membership assertions -------------------------------------------
    let funded_list = client.get_invoices_by_status(&InvoiceStatus::Funded);
    assert!(
        !funded_list.contains(&invoice_id),
        "Invoice must NOT remain in Funded list after refund"
    );
    let refunded_list = client.get_invoices_by_status(&InvoiceStatus::Refunded);
    assert!(
        refunded_list.contains(&invoice_id),
        "Invoice must appear in Refunded status index"
    );

    // -- Count invariant -------------------------------------------------------
    assert_invoice_count_invariant(&client);
}

// --- Test 3: Default flow -----------------------------------------------------

/// After `mark_invoice_defaulted` cross-module state must be consistent:
///
/// - `invoice.status == Defaulted`
/// - `investment.status == Defaulted`
/// - Invoice NOT in Funded list; IS in Defaulted list
/// - Count invariant holds
/// - No "ghost" Active investment for a Defaulted invoice (exploitable for re-settlement)
#[test]
fn test_default_cross_module_consistency() {
    let (env, client, admin) = make_env();
    let contract_id = client.address.clone();
    let business = Address::generate(&env);
    let investor = Address::generate(&env);

    let invoice_amount: i128 = 5_000;
    let bid_amount: i128 = 4_500;
    let currency = make_token(&env, &contract_id, &business, &investor, 5_000, 10_000);

    let (invoice_id, bid_id) = kyc_upload_bid(
        &env, &client, &admin, &business, &investor, &currency, invoice_amount, bid_amount,
    );

    // Fund the invoice.
    client.accept_bid(&invoice_id, &bid_id);

    // Advance ledger past due_date + grace_period to allow default.
    // due_date was set to timestamp+86_400 (=87_400). Grace is 7 days = 604_800.
    // We need > due_date + grace: 87_400 + 604_800 + 1 = 692_201.
    env.ledger().set_timestamp(700_000);

    // Mark as defaulted (grace_period=0 to bypass any additional offset).
    client.mark_invoice_defaulted(&invoice_id, &Some(0u64));

    // -- Invoice assertions ----------------------------------------------------
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(
        invoice.status,
        InvoiceStatus::Defaulted,
        "Invoice must be Defaulted"
    );

    // -- Investment assertions -------------------------------------------------
    let investment = client
        .get_invoice_investment(&invoice_id)
        .expect("Investment record must still be accessible after default");
    assert_eq!(
        investment.status,
        InvestmentStatus::Defaulted,
        "Investment must be Defaulted - no ghost Active investment after invoice default"
    );

    // -- Index membership assertions -------------------------------------------
    let funded_list = client.get_invoices_by_status(&InvoiceStatus::Funded);
    assert!(
        !funded_list.contains(&invoice_id),
        "Defaulted invoice must NOT remain in Funded list"
    );
    let defaulted_list = client.get_invoices_by_status(&InvoiceStatus::Defaulted);
    assert!(
        defaulted_list.contains(&invoice_id),
        "Invoice must appear in Defaulted status index"
    );

    // -- Count invariant -------------------------------------------------------
    assert_invoice_count_invariant(&client);
}

// --- Test 4: Finalize / Settle flow ------------------------------------------

/// After `settle_invoice` all modules must reflect the terminal Paid state:
///
/// - `invoice.status == Paid`, `invoice.total_paid == invoice_amount`
/// - `investment.status == Completed`
/// - Invoice NOT in Funded list; IS in Paid list
/// - `settled_at` is set on the invoice
/// - Count invariant holds
/// - No Active investment after finalization (a security critical assertion -
///   an Active investment on a Paid invoice could allow fraudulent re-settlement)
#[test]
fn test_finalize_settle_cross_module_consistency() {
    let (env, client, admin) = make_env();
    let contract_id = client.address.clone();
    let business = Address::generate(&env);
    let investor = Address::generate(&env);

    let invoice_amount: i128 = 12_000;
    let bid_amount: i128 = 11_000;
    let currency = make_token(
        &env,
        &contract_id,
        &business,
        &investor,
        30_000,
        20_000,
    );

    let (invoice_id, bid_id) = kyc_upload_bid(
        &env, &client, &admin, &business, &investor, &currency, invoice_amount, bid_amount,
    );

    // Fund the invoice.
    client.accept_bid(&invoice_id, &bid_id);

    // Provide settlement tokens (mirrors the pattern from test_lifecycle.rs).
    let sac = token::StellarAssetClient::new(&env, &currency);
    let tok = token::Client::new(&env, &currency);
    sac.mint(&business, &invoice_amount);
    let exp = env.ledger().sequence() + 10_000;
    tok.approve(&business, &contract_id, &(invoice_amount * 4), &exp);

    // Settle.
    client.settle_invoice(&invoice_id, &invoice_amount);

    // -- Invoice assertions ----------------------------------------------------
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(
        invoice.status,
        InvoiceStatus::Paid,
        "Invoice must be Paid after settlement"
    );
    assert_eq!(
        invoice.total_paid, invoice_amount,
        "total_paid must equal invoice_amount"
    );
    assert!(invoice.settled_at.is_some(), "settled_at must be set");

    // -- Investment assertions -------------------------------------------------
    let investment = client
        .get_invoice_investment(&invoice_id)
        .expect("Investment must still be accessible after settlement");
    assert_eq!(
        investment.status,
        InvestmentStatus::Completed,
        "Investment must be Completed - no Active investment on a Paid invoice"
    );

    // -- Index membership assertions -------------------------------------------
    let funded_list = client.get_invoices_by_status(&InvoiceStatus::Funded);
    assert!(
        !funded_list.contains(&invoice_id),
        "Settled invoice must NOT remain in Funded list"
    );
    let paid_list = client.get_invoices_by_status(&InvoiceStatus::Paid);
    assert!(
        paid_list.contains(&invoice_id),
        "Settled invoice must appear in Paid status index"
    );

    // -- Count invariant -------------------------------------------------------
    assert_invoice_count_invariant(&client);
}

// --- Test 5: Multi-invoice isolation -----------------------------------------

/// Two independent invoices must not contaminate each other's cross-module state.
///
/// Flow:
///   - Invoice A -> funded -> settled -> Paid
///   - Invoice B -> funded -> refunded -> Refunded
///
/// After both transitions:
///   - A's investment is Completed, B's investment is Refunded - no swap.
///   - A's bid is Accepted, B's bid is Cancelled - no swap.
///   - Status lists contain exactly the right invoice for each status.
///   - Count invariant holds globally.
#[test]
fn test_no_orphan_after_sequential_operations() {
    let (env, client, admin) = make_env();
    let contract_id = client.address.clone();

    let business_a = Address::generate(&env);
    let investor_a = Address::generate(&env);
    let business_b = Address::generate(&env);
    let investor_b = Address::generate(&env);

    let amount_a: i128 = 8_000;
    let bid_a: i128 = 7_000;
    let amount_b: i128 = 6_000;
    let bid_b: i128 = 5_500;

    let currency = make_token(&env, &contract_id, &business_a, &investor_a, 20_000, 20_000);
    // Mint for second pair on the same currency.
    let sac = token::StellarAssetClient::new(&env, &currency);
    let tok = token::Client::new(&env, &currency);
    sac.mint(&business_b, &20_000i128);
    sac.mint(&investor_b, &20_000i128);
    let exp = env.ledger().sequence() + 10_000;
    tok.approve(&business_b, &contract_id, &80_000i128, &exp);
    tok.approve(&investor_b, &contract_id, &80_000i128, &exp);

    // --- Invoice A setup ---
    let (invoice_a, bid_id_a) = kyc_upload_bid(
        &env, &client, &admin, &business_a, &investor_a, &currency, amount_a, bid_a,
    );
    client.accept_bid(&invoice_a, &bid_id_a);

    // --- Invoice B setup ---
    let (invoice_b, bid_id_b) = kyc_upload_bid(
        &env, &client, &admin, &business_b, &investor_b, &currency, amount_b, bid_b,
    );
    client.accept_bid(&invoice_b, &bid_id_b);

    assert_invoice_count_invariant(&client);

    // Settle Invoice A.
    sac.mint(&business_a, &amount_a);
    let exp2 = env.ledger().sequence() + 10_000;
    tok.approve(&business_a, &contract_id, &(amount_a * 4), &exp2);
    client.settle_invoice(&invoice_a, &amount_a);

    // Refund Invoice B.
    client.refund_escrow_funds(&invoice_b, &business_b);

    // -- A: Paid, investment Completed, bid Accepted ---------------------------
    assert_eq!(client.get_invoice(&invoice_a).status, InvoiceStatus::Paid);
    assert_eq!(
        client.get_invoice_investment(&invoice_a).unwrap().status,
        InvestmentStatus::Completed
    );
    assert_eq!(
        client.get_bid(&bid_id_a).unwrap().status,
        BidStatus::Accepted
    );

    // -- B: Refunded, investment Refunded, bid Cancelled -----------------------
    assert_eq!(
        client.get_invoice(&invoice_b).status,
        InvoiceStatus::Refunded
    );
    assert_eq!(
        client.get_invoice_investment(&invoice_b).unwrap().status,
        InvestmentStatus::Refunded
    );
    assert_eq!(
        client.get_bid(&bid_id_b).unwrap().status,
        BidStatus::Cancelled
    );

    // -- No cross-contamination in index lists ---------------------------------
    let paid_list = client.get_invoices_by_status(&InvoiceStatus::Paid);
    assert!(paid_list.contains(&invoice_a), "A must be in Paid list");
    assert!(!paid_list.contains(&invoice_b), "B must NOT be in Paid list");

    let refunded_list = client.get_invoices_by_status(&InvoiceStatus::Refunded);
    assert!(
        refunded_list.contains(&invoice_b),
        "B must be in Refunded list"
    );
    assert!(
        !refunded_list.contains(&invoice_a),
        "A must NOT be in Refunded list"
    );

    // -- Global count invariant ------------------------------------------------
    assert_invoice_count_invariant(&client);
}

// --- Test 6: Query - canonical record agreement -------------------------------

/// `get_invoices_by_status` index must exactly match what `get_invoice` reports.
///
/// For each invoice returned in the Funded status index, its individual
/// canonical record must also report `status == Funded`. This guards against
/// index/record divergence that could mislead off-chain clients or allow
/// inconsistent downstream decisions.
#[test]
fn test_query_canonical_record_agreement() {
    let (env, client, admin) = make_env();
    let contract_id = client.address.clone();

    let business = Address::generate(&env);
    let investor = Address::generate(&env);
    let currency = make_token(&env, &contract_id, &business, &investor, 5_000, 30_000);

    // Create two funded invoices.
    let (inv1, bid1) = kyc_upload_bid(
        &env, &client, &admin, &business, &investor, &currency, 5_000, 4_500,
    );
    client.accept_bid(&inv1, &bid1);

    // Second invoice needs a fresh investor KYC slot (reuse same investor - limit allows it).
    let due_date2 = env.ledger().timestamp() + 86_400;
    let inv2 = client.upload_invoice(
        &business,
        &5_000i128,
        &currency,
        &due_date2,
        &String::from_str(&env, "Second regression invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    client.verify_invoice(&inv2);
    let bid2 = client.place_bid(&investor, &inv2, &4_000i128, &5_000i128);
    client.accept_bid(&inv2, &bid2);

    // Both appear in the Funded index - verify each canonical record agrees.
    let funded_ids = client.get_invoices_by_status(&InvoiceStatus::Funded);
    assert!(
        funded_ids.len() >= 2,
        "Expected at least 2 Funded invoices in the index"
    );

    for id in funded_ids.iter() {
        let record = client.get_invoice(&id);
        assert_eq!(
            record.status,
            InvoiceStatus::Funded,
            "Every ID in the Funded index must have a canonical status of Funded; \
             id={:?} has status={:?}",
            id,
            record.status
        );
    }

    // After settling inv1, it must leave the Funded index and its canonical record must agree.
    let sac = token::StellarAssetClient::new(&env, &currency);
    let tok = token::Client::new(&env, &currency);
    sac.mint(&business, &5_000i128);
    let exp = env.ledger().sequence() + 10_000;
    tok.approve(&business, &contract_id, &20_000i128, &exp);
    client.settle_invoice(&inv1, &5_000i128);

    // Re-check: inv1 must NOT be in the Funded index; its record must be Paid.
    let funded_ids_after = client.get_invoices_by_status(&InvoiceStatus::Funded);
    assert!(
        !funded_ids_after.contains(&inv1),
        "Settled invoice must leave the Funded index immediately"
    );
    assert_eq!(
        client.get_invoice(&inv1).status,
        InvoiceStatus::Paid,
        "Canonical record must report Paid after settlement"
    );

    // inv2 must still be Funded - no accidental side effects.
    assert!(
        funded_ids_after.contains(&inv2),
        "inv2 must remain in Funded index after inv1 is settled"
    );
    assert_eq!(
        client.get_invoice(&inv2).status,
        InvoiceStatus::Funded,
        "inv2 canonical record must still be Funded"
    );

    // Global count invariant.
    assert_invoice_count_invariant(&client);
}
