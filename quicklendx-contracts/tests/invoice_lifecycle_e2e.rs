//! Full invoice lifecycle end-to-end integration tests (Issue #1103).
//!
//! These tests exercise the complete on-chain lifecycle of an invoice from
//! upload through settlement (or default), asserting state, status, and
//! analytics at every step.
//!
//! ## Test scenarios
//!
//! | Test | Description |
//! |------|-------------|
//! | `test_invoice_lifecycle_happy_path`   | Upload → Verify → Bid → Fund → Partial payment → Settle |
//! | `test_invoice_lifecycle_default_branch` | Upload → Verify → Bid → Fund → Expire → Refund |
//! | `test_partial_then_full_settle`       | Upload → Verify → Bid → Fund → Multiple partials → Final settle |

use quicklendx_contracts::{
    errors::QuickLendXError,
    payments::EscrowStatus,
    types::{BidStatus, InvestmentStatus, InvoiceCategory, InvoiceStatus},
    QuickLendXContract, QuickLendXContractClient,
};
use soroban_sdk::testutils::{Address as _, Ledger as _};
use soroban_sdk::{token, Address, BytesN, Env, String, Vec};

// ============================================================================
// Setup helpers
// ============================================================================

// Shared test fixture returned by `setup_contract`.
struct Fixture {
    // The contract client.
    client: QuickLendXContractClient<'static>,
    // Admin address (controls verification and protocol config).
    _admin: Address,
    // Business address (invoice owner / payer).
    business: Address,
    // Investor address (places bids, receives returns).
    investor: Address,
    // Whitelisted token address (real SAC for balance assertions).
    currency: Address,
    // The contract's own address (used for escrow balance checks).
    contract_id: Address,
}

// Create a fully-initialised contract with a real Stellar Asset Contract
// (SAC) token, a verified business, and a verified investor.
//
// Token balances minted:
// - business: 200 000 000 (enough to settle a 100 000 000 invoice after escrow release)
// - investor: 150 000 000 (enough to fund a 50 000 000 bid)
// - contract:         1 (initialises the SAC instance so balance lookups work)
//
// Both business and investor approve the contract for their full balance so
// `accept_bid_and_fund` and `settle_invoice` can pull tokens without extra
// approval steps inside each test.
fn setup_contract(env: &Env) -> Fixture {
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);

    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(env, &contract_id);

    let admin = Address::generate(env);
    let business = Address::generate(env);
    let investor = Address::generate(env);

    // Admin bootstrap
    client.initialize_admin(&admin);
    client.set_admin(&admin);
    client.initialize_protocol_limits(&admin, &1i128, &365u64, &86_400u64);

    // Real SAC token
    let token_admin = Address::generate(env);
    let currency = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();
    let sac = token::StellarAssetClient::new(env, &currency);
    let tok = token::Client::new(env, &currency);

    sac.mint(&business, &200_000_000i128);
    sac.mint(&investor, &150_000_000i128);
    sac.mint(&contract_id, &1i128); // initialise SAC instance

    let exp = env.ledger().sequence() + 100_000;
    tok.approve(&business, &contract_id, &200_000_000i128, &exp);
    tok.approve(&investor, &contract_id, &150_000_000i128, &exp);

    // Whitelist the currency
    client.add_currency(&admin, &currency);

    // Verify business (KYC submit + admin approve)
    client.submit_kyc_application(&business, &String::from_str(env, "Business KYC"));
    client.verify_business(&admin, &business);

    // Verify investor (KYC submit + admin approve, limit = 150 000 000)
    client.submit_investor_kyc(&investor, &String::from_str(env, "Investor KYC"));
    client.verify_investor(&investor, &150_000_000i128);

    Fixture {
        client,
        _admin: admin,
        business,
        investor,
        currency,
        contract_id,
    }
}

fn upload_verified_invoice(fx: &Fixture, amount: i128, description: &str) -> BytesN<32> {
    let due_date = fx.client.env.ledger().timestamp() + 86_400;
    let invoice_id = fx.client.upload_invoice(
        &fx.business,
        &amount,
        &fx.currency,
        &due_date,
        &String::from_str(&fx.client.env, description),
        &InvoiceCategory::Goods,
        &Vec::new(&fx.client.env),
        &None,
        &None,
        &None,
    );

    fx.client.verify_invoice(&invoice_id);

    invoice_id
}

fn assert_invoice_still_verified_and_unfunded(fx: &Fixture, invoice_id: &BytesN<32>, amount: i128) {
    let invoice = fx.client.get_invoice(invoice_id);

    assert_eq!(invoice.amount, amount);
    assert_eq!(invoice.status, InvoiceStatus::Verified);
    assert_eq!(invoice.funded_amount, 0);
    assert_eq!(invoice.total_paid, 0);
    assert!(invoice.investor.is_none());
    assert_eq!(invoice.payment_history.len(), 0);
}

fn assert_no_bid_or_escrow_created(fx: &Fixture, invoice_id: &BytesN<32>) {
    assert_eq!(fx.client.get_bids_for_invoice(invoice_id).len(), 0);

    let escrow_result = fx.client.try_get_escrow_details(invoice_id);
    assert!(
        matches!(escrow_result, Err(Ok(QuickLendXError::StorageKeyNotFound))),
        "expected no escrow"
    );
}

// ============================================================================
// Test 1 — Happy path: Upload → Verify → Bid → Fund → Partial → Settle
// ============================================================================

// # Balance flow (happy path)
//
// ```text
// Initial:
//   business  = 20 000
//   investor  = 15 000
//   contract  =      1
//
// After accept_bid_and_fund (bid_amount = 9 000):
//   investor  = 15 000 - 9 000 = 6 000   (tokens locked in escrow)
//   contract  =      1 + 9 000 = 9 001
//
// After settle_invoice (invoice_amount = 10 000):
//   business  = 20 000 - 10 000 + 9 000 = 19 000
//               (pays 10 000, receives 9 000 escrow release)
//   investor  = 6 000 + investor_return   (return ≈ 9 000 + profit share)
//   contract  = 9 001 - 9 000 + platform_fee
//
// All funds accounted for: no tokens created or destroyed.
// ```
#[test]
fn test_invoice_lifecycle_happy_path() {
    let env = Env::default();
    let fx = setup_contract(&env);
    let tok = token::Client::new(&env, &fx.currency);

    let invoice_amount: i128 = 100_000_000;
    let bid_amount: i128 = 50_000_000;

    // ── Stage 1: Upload invoice ──────────────────────────────────────────────
    // Business uploads an invoice; status must be Pending.
    let due_date = env.ledger().timestamp() + 86_400;
    let invoice_id = fx.client.upload_invoice(
        &fx.business,
        &invoice_amount,
        &fx.currency,
        &due_date,
        &String::from_str(&env, "Consulting services"),
        &InvoiceCategory::Consulting,
        &Vec::new(&env),
        &None,
        &None,
        &None,
    );

    let invoice = fx.client.get_invoice(&invoice_id);
    assert_eq!(
        invoice.status,
        InvoiceStatus::Pending,
        "Stage 1: status must be Pending after upload"
    );
    assert_eq!(invoice.amount, invoice_amount, "Stage 1: amount must match");
    assert_eq!(
        invoice.business, fx.business,
        "Stage 1: business must match"
    );
    assert!(invoice.investor.is_none(), "Stage 1: no investor yet");

    // Analytics: 1 invoice total
    let metrics = fx.client.get_platform_metrics();
    assert_eq!(
        metrics.total_invoices, 1,
        "Stage 1: analytics must count 1 invoice"
    );

    // ── Stage 2: Verify invoice ──────────────────────────────────────────────
    // Admin verifies the invoice; status must change to Verified.
    fx.client.verify_invoice(&invoice_id);

    let invoice = fx.client.get_invoice(&invoice_id);
    assert_eq!(
        invoice.status,
        InvoiceStatus::Verified,
        "Stage 2: status must be Verified"
    );

    let verified_ids = fx.client.get_invoices_by_status(&InvoiceStatus::Verified);
    assert!(
        verified_ids.iter().any(|id| id == invoice_id),
        "Stage 2: invoice must appear in Verified bucket"
    );

    // ── Stage 3: Place bid ───────────────────────────────────────────────────
    // Investor places a bid; bid must be recorded with Placed status.
    let bid_id = fx.client.place_bid(
        &fx.investor,
        &invoice_id,
        &bid_amount,
        &invoice_amount,
        &BytesN::from_array(&fx.client.env, &[0u8; 32]), // expected_return = full invoice amount
    );

    let bid = fx.client.get_bid(&bid_id).unwrap();
    assert_eq!(bid.bid_amount, bid_amount, "Stage 3: bid amount must match");
    assert_eq!(bid.investor, fx.investor, "Stage 3: investor must match");
    assert_eq!(
        bid.status,
        BidStatus::Placed,
        "Stage 3: bid status must be Placed"
    );

    let bids_for_invoice = fx.client.get_bids_for_invoice(&invoice_id);
    assert_eq!(
        bids_for_invoice.len(),
        1,
        "Stage 3: exactly one bid on invoice"
    );

    // ── Stage 4: Accept bid and fund ─────────────────────────────────────────
    // Business accepts the bid; escrow is created, invoice becomes Funded.
    let investor_bal_before = tok.balance(&fx.investor);
    let contract_bal_before = tok.balance(&fx.contract_id);

    let _escrow_id = fx.client.accept_bid_and_fund(&invoice_id, &bid_id);

    let investor_bal_after = tok.balance(&fx.investor);
    let contract_bal_after = tok.balance(&fx.contract_id);

    // Token flow: investor paid bid_amount into escrow
    assert_eq!(
        investor_bal_before - investor_bal_after,
        bid_amount,
        "Stage 4: investor must have paid bid_amount into escrow"
    );
    assert_eq!(
        contract_bal_after - contract_bal_before,
        bid_amount,
        "Stage 4: contract must hold bid_amount in escrow"
    );

    let invoice = fx.client.get_invoice(&invoice_id);
    assert_eq!(
        invoice.status,
        InvoiceStatus::Funded,
        "Stage 4: status must be Funded"
    );
    assert_eq!(
        invoice.funded_amount, bid_amount,
        "Stage 4: funded_amount must equal bid_amount"
    );
    assert_eq!(
        invoice.investor,
        Some(fx.investor.clone()),
        "Stage 4: investor must be set"
    );

    // Investment record must exist and be Active
    let investment = fx.client.get_invoice_investment(&invoice_id);
    assert_eq!(
        investment.status,
        InvestmentStatus::Active,
        "Stage 4: investment must be Active"
    );
    assert_eq!(
        investment.amount, bid_amount,
        "Stage 4: investment amount must equal bid_amount"
    );

    // Analytics: 1 investment
    let metrics = fx.client.get_platform_metrics();
    assert_eq!(
        metrics.total_investments, 1,
        "Stage 4: analytics must count 1 investment"
    );

    // ── Stage 5: Process partial payment ────────────────────────────────────
    // Business makes a partial payment; total_paid must update, status stays Funded.
    let partial_amount: i128 = 40_000_000;
    fx.client.process_partial_payment(
        &invoice_id,
        &partial_amount,
        &String::from_str(&env, "partial-pay-1"),
    );

    let invoice = fx.client.get_invoice(&invoice_id);
    assert_eq!(
        invoice.total_paid, partial_amount,
        "Stage 5: total_paid must equal partial_amount"
    );
    assert_eq!(
        invoice.status,
        InvoiceStatus::Funded,
        "Stage 5: status must still be Funded"
    );
    assert_eq!(
        invoice.payment_history.len(),
        1,
        "Stage 5: payment_history must have 1 entry"
    );

    // ── Stage 6: Settle invoice ──────────────────────────────────────────────
    // Business pays the remaining amount; invoice settles, investment completes.
    let remaining = invoice_amount - partial_amount; // 6 000
    let business_bal_before = tok.balance(&fx.business);
    let investor_bal_before_settle = tok.balance(&fx.investor);

    let snap = fx.client.get_investment_by_invoice(&invoice_id);
    fx.client.settle_invoice(&invoice_id, &remaining, &snap);

    let invoice = fx.client.get_invoice(&invoice_id);
    assert_eq!(
        invoice.status,
        InvoiceStatus::Paid,
        "Stage 6: status must be Paid after settle"
    );
    assert_eq!(
        invoice.total_paid, invoice_amount,
        "Stage 6: total_paid must equal invoice_amount"
    );

    // Investment must be Completed
    let investment = fx.client.get_invoice_investment(&invoice_id);
    assert_eq!(
        investment.status,
        InvestmentStatus::Completed,
        "Stage 6: investment must be Completed"
    );

    // Balance reconciliation: business paid invoice_amount net, investor received return
    let business_bal_after = tok.balance(&fx.business);
    let investor_bal_after_settle = tok.balance(&fx.investor);

    // Business net: paid invoice_amount, received bid_amount from escrow release
    // net change = -(invoice_amount - bid_amount) = -(10 000 - 9 000) = -1 000
    assert_eq!(
        business_bal_before - business_bal_after,
        invoice_amount - bid_amount,
        "Stage 6: business net cost must be invoice_amount - bid_amount"
    );

    // Investor must have received at least bid_amount back (profit may vary by fee config)
    assert!(
        investor_bal_after_settle > investor_bal_before_settle,
        "Stage 6: investor must have received a return"
    );

    // Analytics: success_rate > 0 after a paid invoice
    let metrics = fx.client.get_platform_metrics();
    assert!(
        metrics.success_rate > 0,
        "Stage 6: analytics success_rate must be > 0"
    );
    assert_eq!(
        metrics.default_rate, 0,
        "Stage 6: analytics default_rate must be 0"
    );

    // Status bucket invariant
    let paid_ids = fx.client.get_invoices_by_status(&InvoiceStatus::Paid);
    assert!(
        paid_ids.iter().any(|id| id == invoice_id),
        "Stage 6: invoice must appear in Paid bucket"
    );
}

// ============================================================================
// Issue #1937: Overbid refund path boundaries
// ============================================================================

#[test]
fn test_boundary_bid_equal_to_invoice_amount_accepts_and_funds_exactly() {
    let env = Env::default();
    let fx = setup_contract(&env);
    let tok = token::Client::new(&env, &fx.currency);

    let invoice_amount: i128 = 10_000;
    let invoice_id =
        upload_verified_invoice(&fx, invoice_amount, "Boundary bid equal to invoice amount");
    let bid_salt = BytesN::from_array(&env, &[11u8; 32]);

    let investor_balance_before = tok.balance(&fx.investor);
    let contract_balance_before = tok.balance(&fx.contract_id);

    let bid_id = fx.client.place_bid(
        &fx.investor,
        &invoice_id,
        &invoice_amount,
        &(invoice_amount + 1_000),
        &bid_salt,
    );

    let bid = fx.client.get_bid(&bid_id).unwrap();
    assert_eq!(bid.status, BidStatus::Placed);
    assert_eq!(bid.bid_amount, invoice_amount);
    assert_eq!(fx.client.get_bids_for_invoice(&invoice_id).len(), 1);

    fx.client.accept_bid_and_fund(&invoice_id, &bid_id);

    let escrow = fx.client.get_escrow_details(&invoice_id);
    assert!(matches!(escrow.status, EscrowStatus::Held));
    assert_eq!(escrow.amount, invoice_amount);
    assert_eq!(escrow.investor, fx.investor);

    let invoice = fx.client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Funded);
    assert_eq!(invoice.funded_amount, invoice_amount);
    assert_eq!(invoice.investor, Some(fx.investor.clone()));

    assert_eq!(
        tok.balance(&fx.investor),
        investor_balance_before - invoice_amount
    );
    assert_eq!(
        tok.balance(&fx.contract_id),
        contract_balance_before + invoice_amount
    );
}

#[test]
fn test_single_overbid_rejected_without_balance_or_state_changes() {
    let env = Env::default();
    let fx = setup_contract(&env);
    let tok = token::Client::new(&env, &fx.currency);

    let invoice_amount: i128 = 10_000;
    let invoice_id = upload_verified_invoice(&fx, invoice_amount, "Single overbid rejected");
    let overbid_amount = invoice_amount + 1;
    let bid_salt = BytesN::from_array(&env, &[12u8; 32]);

    let investor_balance_before = tok.balance(&fx.investor);
    let contract_balance_before = tok.balance(&fx.contract_id);

    let result = fx.client.try_place_bid(
        &fx.investor,
        &invoice_id,
        &overbid_amount,
        &(overbid_amount + 1_000),
        &bid_salt,
    );

    assert!(
        matches!(result, Err(Ok(QuickLendXError::InvoiceAmountInvalid))),
        "expected InvoiceAmountInvalid for overbid"
    );
    assert_eq!(tok.balance(&fx.investor), investor_balance_before);
    assert_eq!(tok.balance(&fx.contract_id), contract_balance_before);
    assert_no_bid_or_escrow_created(&fx, &invoice_id);
    assert_invoice_still_verified_and_unfunded(&fx, &invoice_id, invoice_amount);
}

#[test]
fn test_multiple_overbids_rejected_independently_without_side_effects() {
    let env = Env::default();
    let fx = setup_contract(&env);
    let tok = token::Client::new(&env, &fx.currency);

    let invoice_amount: i128 = 10_000;
    let invoice_id = upload_verified_invoice(&fx, invoice_amount, "Multiple overbids rejected");
    let investor_balance_before = tok.balance(&fx.investor);
    let contract_balance_before = tok.balance(&fx.contract_id);

    for (salt_byte, overbid_amount) in [(21u8, 10_001i128), (22, 10_500), (23, 14_999)] {
        let bid_salt = BytesN::from_array(&env, &[salt_byte; 32]);
        let result = fx.client.try_place_bid(
            &fx.investor,
            &invoice_id,
            &overbid_amount,
            &(overbid_amount + 1),
            &bid_salt,
        );

        assert!(
            matches!(result, Err(Ok(QuickLendXError::InvoiceAmountInvalid))),
            "expected InvoiceAmountInvalid for overbid {overbid_amount}"
        );
        assert_eq!(tok.balance(&fx.investor), investor_balance_before);
        assert_eq!(tok.balance(&fx.contract_id), contract_balance_before);
        assert_no_bid_or_escrow_created(&fx, &invoice_id);
        assert_invoice_still_verified_and_unfunded(&fx, &invoice_id, invoice_amount);
    }
}

// ============================================================================
// Test 2 — Default branch: Upload → Verify → Bid → Fund → Expire → Refund
// ============================================================================

// # Balance flow (default branch)
//
// ```text
// Initial:
//   business  = 20 000
//   investor  = 15 000
//   contract  =      1
//
// After accept_bid_and_fund (bid_amount = 9 000):
//   investor  = 15 000 - 9 000 = 6 000
//   contract  =      1 + 9 000 = 9 001
//
// After refund_escrow (escrow returned to investor):
//   investor  = 6 000 + 9 000 = 15 000   (fully restored)
//   contract  = 9 001 - 9 000 =      1   (fully restored)
//
// No funds lost: business balance unchanged, investor fully refunded.
// ```
#[test]
fn test_invoice_lifecycle_default_branch() {
    let env = Env::default();
    let fx = setup_contract(&env);
    let tok = token::Client::new(&env, &fx.currency);

    let invoice_amount: i128 = 10_000_000;
    let bid_amount: i128 = 9_000_000;

    // ── Stage 1: Upload invoice ──────────────────────────────────────────────
    // Business uploads an invoice.
    let due_date = env.ledger().timestamp() + 86_400; // 1 day from now
    let invoice_id = fx.client.upload_invoice(
        &fx.business,
        &invoice_amount,
        &fx.currency,
        &due_date,
        &String::from_str(&env, "Goods delivery"),
        &InvoiceCategory::Consulting,
        &Vec::new(&env),
        &None,
        &None,
        &None,
    );

    assert_eq!(
        fx.client.get_invoice(&invoice_id).status,
        InvoiceStatus::Pending,
        "Stage 1: status must be Pending"
    );

    // ── Stage 2: Verify invoice ──────────────────────────────────────────────
    // Admin verifies the invoice.
    fx.client.verify_invoice(&invoice_id);
    assert_eq!(
        fx.client.get_invoice(&invoice_id).status,
        InvoiceStatus::Verified,
        "Stage 2: status must be Verified"
    );

    // ── Stage 3: Place bid ───────────────────────────────────────────────────
    // Investor places a bid.
    let bid_id = fx.client.place_bid(
        &fx.investor,
        &invoice_id,
        &bid_amount,
        &invoice_amount,
        &BytesN::from_array(&fx.client.env, &[0u8; 32]),
    );
    assert_eq!(
        fx.client.get_bid(&bid_id).unwrap().status,
        BidStatus::Placed,
        "Stage 3: bid must be Placed"
    );

    // ── Stage 4: Accept bid and fund ─────────────────────────────────────────
    // Business accepts the bid; escrow is created.
    let investor_bal_before = tok.balance(&fx.investor);
    let contract_bal_before = tok.balance(&fx.contract_id);

    fx.client.accept_bid_and_fund(&invoice_id, &bid_id);

    assert_eq!(
        fx.client.get_invoice(&invoice_id).status,
        InvoiceStatus::Funded,
        "Stage 4: status must be Funded"
    );
    assert_eq!(
        tok.balance(&fx.investor),
        investor_bal_before - bid_amount,
        "Stage 4: investor must have paid bid_amount into escrow"
    );
    assert_eq!(
        tok.balance(&fx.contract_id),
        contract_bal_before + bid_amount,
        "Stage 4: contract must hold bid_amount"
    );

    // ── Stage 5: Advance time past due date ──────────────────────────────────
    // Move ledger timestamp past the invoice due date so expire_invoice succeeds.
    env.ledger().set_timestamp(due_date + 1);

    // ── Stage 6: Expire invoice ──────────────────────────────────────────────
    // Expire the invoice (emits InvoiceExpired event).
    // expire_invoice only emits the event; it does not change the invoice status.
    fx.client.expire_invoice(&invoice_id);

    // Invoice is still Funded after expire_invoice — status only changes via
    // mark_invoice_defaulted or refund_escrow.
    assert_eq!(
        fx.client.get_invoice(&invoice_id).status,
        InvoiceStatus::Funded,
        "Stage 6: status must still be Funded after expire_invoice"
    );

    // ── Stage 7: Refund escrow ───────────────────────────────────────────────
    // Admin triggers escrow refund while invoice is still Funded.
    // refund_escrow transitions the invoice to Refunded and returns funds to investor.
    let investor_bal_before_refund = tok.balance(&fx.investor);
    let contract_bal_before_refund = tok.balance(&fx.contract_id);

    fx.client.refund_escrow(&invoice_id);

    let investor_bal_after_refund = tok.balance(&fx.investor);
    let contract_bal_after_refund = tok.balance(&fx.contract_id);

    // Invoice must now be Refunded
    let invoice = fx.client.get_invoice(&invoice_id);
    assert_eq!(
        invoice.status,
        InvoiceStatus::Refunded,
        "Stage 7: status must be Refunded after escrow refund"
    );

    // Investor fully refunded
    assert_eq!(
        investor_bal_after_refund - investor_bal_before_refund,
        bid_amount,
        "Stage 7: investor must be refunded bid_amount"
    );
    // Contract balance restored
    assert_eq!(
        contract_bal_before_refund - contract_bal_after_refund,
        bid_amount,
        "Stage 7: contract balance must decrease by bid_amount"
    );

    // No funds lost: business balance unchanged throughout
    assert_eq!(
        tok.balance(&fx.business),
        200_000_000i128,
        "Stage 7: business balance must be unchanged"
    );

    // Analytics: default_rate == 0 (refund, not default), success_rate == 0
    let metrics = fx.client.get_platform_metrics();
    assert_eq!(
        metrics.success_rate, 0,
        "Stage 7: analytics success_rate must be 0"
    );
}

// ============================================================================
// Test 3 — Multiple partial payments then full settle
// ============================================================================

// # Balance flow (partial then full settle)
//
// ```text
// Initial:
//   business  = 20 000
//   investor  = 15 000
//   contract  =      1
//
// After accept_bid_and_fund (bid_amount = 8 000):
//   investor  = 15 000 - 8 000 = 7 000
//   contract  =      1 + 8 000 = 8 001
//
// Partial payments (3 × 2 000 = 6 000 paid by business):
//   business  = 20 000 - 6 000 = 14 000
//
// Final settle (remaining 4 000):
//   business  = 14 000 - 4 000 + 8 000 = 18 000
//               (pays 4 000, receives 8 000 escrow release)
//   investor  = 7 000 + investor_return
//
// Total business outflow = 10 000 (invoice_amount), net = 10 000 - 8 000 = 2 000.
// All funds accounted for.
// ```
#[test]
fn test_partial_then_full_settle() {
    let env = Env::default();
    let fx = setup_contract(&env);
    let tok = token::Client::new(&env, &fx.currency);

    let invoice_amount: i128 = 100_000_000;
    let bid_amount: i128 = 50_000_000;

    // ── Stage 1: Upload invoice ──────────────────────────────────────────────
    // Business uploads an invoice.
    let due_date = env.ledger().timestamp() + 86_400;
    let invoice_id = fx.client.upload_invoice(
        &fx.business,
        &invoice_amount,
        &fx.currency,
        &due_date,
        &String::from_str(&env, "Technology services"),
        &InvoiceCategory::Consulting,
        &Vec::new(&env),
        &None,
        &None,
        &None,
    );
    assert_eq!(
        fx.client.get_invoice(&invoice_id).status,
        InvoiceStatus::Pending,
        "Stage 1: status must be Pending"
    );

    // ── Stage 2: Verify invoice ──────────────────────────────────────────────
    // Admin verifies the invoice.
    fx.client.verify_invoice(&invoice_id);
    assert_eq!(
        fx.client.get_invoice(&invoice_id).status,
        InvoiceStatus::Verified,
        "Stage 2: status must be Verified"
    );

    // ── Stage 3: Place bid ───────────────────────────────────────────────────
    // Investor places a bid.
    let bid_id = fx.client.place_bid(
        &fx.investor,
        &invoice_id,
        &bid_amount,
        &invoice_amount,
        &BytesN::from_array(&fx.client.env, &[0u8; 32]),
    );
    assert_eq!(
        fx.client.get_bid(&bid_id).unwrap().bid_amount,
        bid_amount,
        "Stage 3: bid_amount must match"
    );

    // ── Stage 4: Accept bid and fund ─────────────────────────────────────────
    // Business accepts the bid; escrow is created.
    fx.client.accept_bid_and_fund(&invoice_id, &bid_id);
    assert_eq!(
        fx.client.get_invoice(&invoice_id).status,
        InvoiceStatus::Funded,
        "Stage 4: status must be Funded"
    );

    // ── Stage 5: Multiple partial payments ──────────────────────────────────
    // Business makes three partial payments of 2 000 each (total 6 000).
    env.ledger().set_timestamp(2_000);
    fx.client.process_partial_payment(
        &invoice_id,
        &20_000_000i128,
        &String::from_str(&env, "partial-1"),
    );
    let invoice = fx.client.get_invoice(&invoice_id);
    assert_eq!(
        invoice.total_paid, 20_000_000,
        "Stage 5a: total_paid must be 2 000"
    );
    assert_eq!(
        invoice.status,
        InvoiceStatus::Funded,
        "Stage 5a: still Funded"
    );

    env.ledger().set_timestamp(3_000);
    fx.client.process_partial_payment(
        &invoice_id,
        &20_000_000i128,
        &String::from_str(&env, "partial-2"),
    );
    let invoice = fx.client.get_invoice(&invoice_id);
    assert_eq!(
        invoice.total_paid, 40_000_000,
        "Stage 5b: total_paid must be 4 000"
    );
    assert_eq!(
        invoice.status,
        InvoiceStatus::Funded,
        "Stage 5b: still Funded"
    );

    env.ledger().set_timestamp(4_000);
    fx.client.process_partial_payment(
        &invoice_id,
        &20_000_000i128,
        &String::from_str(&env, "partial-3"),
    );
    let invoice = fx.client.get_invoice(&invoice_id);
    assert_eq!(
        invoice.total_paid, 60_000_000,
        "Stage 5c: total_paid must be 6 000"
    );
    assert_eq!(
        invoice.status,
        InvoiceStatus::Funded,
        "Stage 5c: still Funded"
    );
    assert_eq!(
        invoice.payment_history.len(),
        3,
        "Stage 5c: payment_history must have 3 entries"
    );

    // ── Stage 6: Final payment that settles the invoice ──────────────────────
    // Business pays the remaining 4 000; invoice auto-settles to Paid.
    let business_bal_before = tok.balance(&fx.business);
    let investor_bal_before = tok.balance(&fx.investor);

    env.ledger().set_timestamp(5_000);
    let snap = fx.client.get_investment_by_invoice(&invoice_id);
    fx.client
        .settle_invoice(&invoice_id, &40_000_000i128, &snap);

    let invoice = fx.client.get_invoice(&invoice_id);
    assert_eq!(
        invoice.status,
        InvoiceStatus::Paid,
        "Stage 6: status must be Paid"
    );
    assert_eq!(
        invoice.total_paid, invoice_amount,
        "Stage 6: total_paid must equal invoice_amount"
    );

    // Investment must be Completed
    let investment = fx.client.get_invoice_investment(&invoice_id);
    assert_eq!(
        investment.status,
        InvestmentStatus::Completed,
        "Stage 6: investment must be Completed"
    );

    // Balance reconciliation
    let business_bal_after = tok.balance(&fx.business);
    let investor_bal_after = tok.balance(&fx.investor);

    // Business net outflow = invoice_amount - bid_amount (escrow released back)
    // = 10 000 - 8 000 = 2 000
    assert_eq!(
        business_bal_before - business_bal_after,
        invoice_amount - bid_amount,
        "Stage 6: business net cost must be invoice_amount - bid_amount"
    );

    // Investor must have received at least bid_amount back
    assert!(
        investor_bal_after > investor_bal_before,
        "Stage 6: investor must have received a return"
    );

    // Analytics: success_rate > 0, default_rate == 0
    let metrics = fx.client.get_platform_metrics();
    assert!(
        metrics.success_rate > 0,
        "Stage 6: analytics success_rate must be > 0"
    );
    assert_eq!(
        metrics.default_rate, 0,
        "Stage 6: analytics default_rate must be 0"
    );

    // Status bucket invariant
    let paid_ids = fx.client.get_invoices_by_status(&InvoiceStatus::Paid);
    assert!(
        paid_ids.iter().any(|id| id == invoice_id),
        "Stage 6: invoice must appear in Paid bucket"
    );
}
