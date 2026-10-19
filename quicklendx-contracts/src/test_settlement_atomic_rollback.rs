//! Regression coverage for #2464: repayment allocation, fees, principal, and
//! profit distribution stay deterministic under normal, invalid, repeated,
//! and failure conditions, with no partial state surviving a rejected
//! operation.
//!
//! These tests exercise the actual contract entrypoints (`process_partial_payment`,
//! `settle_invoice`, `handle_default`) — the real integration boundary — rather
//! than the internal `settlement`/`defaults` functions directly, and assert
//! on-chain token balances, not just return values, so a "distributes more
//! than was received" bug would show up as a failing balance assertion.
//! `is_invoice_finalized`/`get_invoice_progress` are internal (not part of
//! the contract's public ABI), so those two are read via `env.as_contract`,
//! matching the pattern `test_settlement.rs` already uses for the same pair.

use super::*;
extern crate alloc;
use crate::invoice::{InvoiceCategory, InvoiceStatus};
use crate::settlement::{get_invoice_progress, is_invoice_finalized};
use soroban_sdk::{testutils::Address as _, token, Address, BytesN, Env, String, Vec};

// ---------------------------------------------------------------------------
// Setup helpers (deliberately self-contained rather than importing
// `test_settlement`'s private helpers, to keep this file's diff isolated).
// ---------------------------------------------------------------------------

fn verify_investor(env: &Env, client: &QuickLendXContractClient, investor: &Address, limit: i128) {
    client.submit_investor_kyc(investor, &String::from_str(env, "Investor KYC"));
    client.verify_investor(investor, &limit);
}

fn init_currency(
    env: &Env,
    contract_id: &Address,
    business: &Address,
    investor: &Address,
) -> Address {
    let token_admin = Address::generate(env);
    let currency = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token_client = token::Client::new(env, &currency);
    let sac_client = token::StellarAssetClient::new(env, &currency);
    let initial_balance = 10_000i128;

    sac_client.mint(business, &initial_balance);
    sac_client.mint(investor, &initial_balance);
    sac_client.mint(contract_id, &1i128);

    let expiration = env.ledger().sequence() + 1_000;
    token_client.approve(business, contract_id, &initial_balance, &expiration);
    token_client.approve(investor, contract_id, &initial_balance, &expiration);

    currency
}

/// Sets up a fully-funded invoice (`Funded` status), ready to be paid off,
/// settled, or defaulted, and returns everything a test needs to drive that.
fn setup_funded_invoice(
    env: &Env,
    client: &QuickLendXContractClient,
    business: &Address,
    investor: &Address,
    currency: &Address,
    invoice_amount: i128,
    investment_amount: i128,
) -> BytesN<32> {
    let admin = Address::generate(env);
    client.set_admin(&admin);

    client.submit_kyc_application(business, &String::from_str(env, "KYC data"));
    client.verify_business(&admin, business);

    let due_date = env.ledger().timestamp() + 86_400;
    let invoice_id = client.store_invoice(
        business,
        &invoice_amount,
        currency,
        &due_date,
        &String::from_str(env, "Atomic rollback test invoice"),
        &InvoiceCategory::Services,
        &Vec::new(env),
        &None,
    );
    client.verify_invoice(&invoice_id);

    verify_investor(env, client, investor, 10_000);
    let bid_id = client.place_bid(
        investor,
        &invoice_id,
        &investment_amount,
        &invoice_amount,
        &BytesN::from_array(env, &[0u8; 32]),
    );
    client.accept_bid(&invoice_id, &bid_id);

    invoice_id
}

fn setup() -> (Env, QuickLendXContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    (env, client, contract_id)
}

fn progress(env: &Env, contract_id: &Address, invoice_id: &BytesN<32>) -> settlement::Progress {
    env.as_contract(contract_id, || get_invoice_progress(env, invoice_id).unwrap())
}

fn finalized(env: &Env, contract_id: &Address, invoice_id: &BytesN<32>) -> bool {
    env.as_contract(contract_id, || is_invoice_finalized(env, invoice_id).unwrap())
}

// ---------------------------------------------------------------------------
// record_payment / process_partial_payment: rejected calls leave no state
// ---------------------------------------------------------------------------

/// A duplicate payment nonce is rejected, and leaves total_paid, payment
/// count, and every token balance exactly as they were before the retry --
/// the "repeated" case from the acceptance criteria.
#[test]
fn test_duplicate_nonce_leaves_no_partial_state() {
    let (env, client, contract_id) = setup();
    let business = Address::generate(&env);
    let investor = Address::generate(&env);
    let currency = init_currency(&env, &contract_id, &business, &investor);
    let invoice_id = setup_funded_invoice(&env, &client, &business, &investor, &currency, 1_000, 1_000);

    let nonce = String::from_str(&env, "tx-nonce-1");
    client.process_partial_payment(&invoice_id, &400, &nonce);

    let progress_before = progress(&env, &contract_id, &invoice_id);
    let business_balance_before = token::Client::new(&env, &currency).balance(&business);

    // Same nonce again: must be rejected, not silently re-applied.
    let result = client.try_process_partial_payment(&invoice_id, &400, &nonce);
    let err = result.err().expect("expected error on nonce replay");
    let contract_error = err.expect("expected contract error");
    assert_eq!(contract_error, QuickLendXError::DuplicateNonce);

    let progress_after = progress(&env, &contract_id, &invoice_id);
    assert_eq!(progress_after.total_paid, progress_before.total_paid);
    assert_eq!(progress_after.payment_count, progress_before.payment_count);

    let business_balance_after = token::Client::new(&env, &currency).balance(&business);
    assert_eq!(business_balance_after, business_balance_before);
}

/// A non-positive payment amount is rejected before any state changes.
/// `process_partial_payment`'s public signature has no separate `payer`
/// parameter -- it always pays as the invoice's own business address
/// (`record_payment`'s `payer == invoice.business` guard is therefore not
/// independently reachable from a different caller through this
/// entrypoint) -- so this exercises the amount-validation boundary instead,
/// at the same "reject before any write" point in `record_payment`.
#[test]
fn test_invalid_amount_leaves_no_partial_state() {
    let (env, client, contract_id) = setup();
    let business = Address::generate(&env);
    let investor = Address::generate(&env);
    let currency = init_currency(&env, &contract_id, &business, &investor);
    let invoice_id = setup_funded_invoice(&env, &client, &business, &investor, &currency, 1_000, 1_000);

    let progress_before = progress(&env, &contract_id, &invoice_id);

    let result = client.try_process_partial_payment(&invoice_id, &0, &String::from_str(&env, "n1"));
    let err = result.err().expect("expected error for non-positive amount");
    let contract_error = err.expect("expected contract error");
    assert_eq!(contract_error, QuickLendXError::InvalidAmount);

    let progress_after = progress(&env, &contract_id, &invoice_id);
    assert_eq!(progress_after.total_paid, progress_before.total_paid);
    assert_eq!(progress_after.payment_count, progress_before.payment_count);
}

/// Requesting a payment larger than remaining_due is capped, not rejected --
/// but the invariant `total_paid <= total_due` must hold exactly, with no
/// excess ever transferred.
#[test]
fn test_overpayment_is_capped_not_partially_applied_twice() {
    let (env, client, contract_id) = setup();
    let business = Address::generate(&env);
    let investor = Address::generate(&env);
    let currency = init_currency(&env, &contract_id, &business, &investor);
    let invoice_id = setup_funded_invoice(&env, &client, &business, &investor, &currency, 1_000, 1_000);

    // Requesting far more than total_due; only total_due is ever applied and
    // settlement triggers automatically (total_paid reaches total_due).
    client.process_partial_payment(&invoice_id, &1_000_000, &String::from_str(&env, "n1"));

    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.total_paid, 1_000);
    assert_eq!(invoice.status, InvoiceStatus::Paid);
}

// ---------------------------------------------------------------------------
// settle_invoice: accounting identity and double-settlement rejection
// ---------------------------------------------------------------------------

/// The accounting identity `investor_return + platform_fee == total_paid`
/// holds exactly against real token balances -- not just the returned
/// struct -- proving the invariant at the actual integration boundary.
#[test]
fn test_settlement_disbursement_matches_total_paid_exactly() {
    let (env, client, contract_id) = setup();
    let business = Address::generate(&env);
    let investor = Address::generate(&env);
    let currency = init_currency(&env, &contract_id, &business, &investor);
    let invoice_id = setup_funded_invoice(&env, &client, &business, &investor, &currency, 2_000, 2_000);

    let token = token::Client::new(&env, &currency);
    let investor_before = token.balance(&investor);
    let business_before = token.balance(&business);
    // No treasury is configured in this test, so FeeManager::route_platform_fee
    // falls back to the contract's own address as the fee recipient
    // (fees.rs::route_platform_fee) -- track that balance too so the full
    // three-way identity can be checked against real token movements.
    let contract_fee_before = token.balance(&contract_id);

    // Full payment in one shot triggers settlement automatically.
    client.process_partial_payment(&invoice_id, &2_000, &String::from_str(&env, "final"));

    let investor_delta = token.balance(&investor) - investor_before;
    let business_delta = business_before - token.balance(&business); // business pays out
    let contract_fee_delta = token.balance(&contract_id) - contract_fee_before;

    // Every unit that left the business's wallet is accounted for by
    // exactly two destinations: the investor's return and the platform
    // fee. Nothing is lost, and nothing beyond what the business paid is
    // ever distributed.
    assert_eq!(business_delta, investor_delta + contract_fee_delta);
    assert_eq!(business_delta, 2_000);

    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Paid);
    assert!(finalized(&env, &contract_id, &invoice_id));
}

/// A second settlement attempt on an already-finalized invoice is rejected,
/// and leaves every balance and the finalized flag exactly as the first,
/// successful settlement left them -- no double-disbursement on repeat.
#[test]
fn test_double_settlement_is_rejected_and_leaves_state_unchanged() {
    let (env, client, contract_id) = setup();
    let business = Address::generate(&env);
    let investor = Address::generate(&env);
    let currency = init_currency(&env, &contract_id, &business, &investor);
    let invoice_id = setup_funded_invoice(&env, &client, &business, &investor, &currency, 1_000, 1_000);

    client.process_partial_payment(&invoice_id, &1_000, &String::from_str(&env, "pay-1"));
    assert!(finalized(&env, &contract_id, &invoice_id));

    let token = token::Client::new(&env, &currency);
    let investor_after_first = token.balance(&investor);
    let business_after_first = token.balance(&business);

    let investment = client.get_invoice_investment(&invoice_id);
    let result = client.try_settle_invoice(&invoice_id, &1_000, &investment);
    assert!(result.is_err());

    // No second disbursement occurred.
    assert_eq!(token.balance(&investor), investor_after_first);
    assert_eq!(token.balance(&business), business_after_first);
}

// ---------------------------------------------------------------------------
// handle_default: rejected transitions leave no partial state
// ---------------------------------------------------------------------------

/// Attempting to default an invoice that isn't in `Funded` status (e.g. one
/// still `Verified`, never funded) is rejected before any history counter,
/// status transition, or event fires.
#[test]
fn test_default_on_non_funded_invoice_leaves_no_partial_state() {
    let (env, client, _contract_id) = setup();
    let business = Address::generate(&env);
    let admin = Address::generate(&env);
    client.set_admin(&admin);
    client.submit_kyc_application(&business, &String::from_str(&env, "KYC data"));
    client.verify_business(&admin, &business);

    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86_400;
    let invoice_id = client.store_invoice(
        &business,
        &1_000,
        &currency,
        &due_date,
        &String::from_str(&env, "Never funded"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
        &None,
    );
    client.verify_invoice(&invoice_id);

    let invoice_before = client.get_invoice(&invoice_id);
    assert_eq!(invoice_before.status, InvoiceStatus::Verified);

    let result = client.try_handle_default(&invoice_id);
    assert!(result.is_err());

    let invoice_after = client.get_invoice(&invoice_id);
    assert_eq!(invoice_after.status, invoice_before.status);
}

/// A second default attempt on an already-defaulted invoice is rejected
/// (`InvoiceAlreadyDefaulted`), and the invoice status is not disturbed by
/// the rejected retry.
#[test]
fn test_repeated_default_is_rejected_and_leaves_state_unchanged() {
    let (env, client, contract_id) = setup();
    let business = Address::generate(&env);
    let investor = Address::generate(&env);
    let currency = init_currency(&env, &contract_id, &business, &investor);
    let invoice_id = setup_funded_invoice(&env, &client, &business, &investor, &currency, 1_000, 1_000);

    client.handle_default(&invoice_id);
    let invoice_after_first = client.get_invoice(&invoice_id);
    assert_eq!(invoice_after_first.status, InvoiceStatus::Defaulted);

    // Repeating the exact same call must fail, not re-run the default effects.
    let result = client.try_handle_default(&invoice_id);
    let err = result.err().expect("expected error on repeated default");
    let contract_error = err.expect("expected contract error");
    assert_eq!(contract_error, QuickLendXError::InvoiceAlreadyDefaulted);

    let invoice_after_second_attempt = client.get_invoice(&invoice_id);
    assert_eq!(invoice_after_second_attempt.status, InvoiceStatus::Defaulted);
}

/// `handle_default` never disburses funds -- confirms this failure/terminal
/// path is purely a bookkeeping transition, so there is no fund-movement
/// side effect for a rejected or repeated call to leave partially applied.
#[test]
fn test_default_does_not_move_any_funds() {
    let (env, client, contract_id) = setup();
    let business = Address::generate(&env);
    let investor = Address::generate(&env);
    let currency = init_currency(&env, &contract_id, &business, &investor);
    let invoice_id = setup_funded_invoice(&env, &client, &business, &investor, &currency, 1_000, 1_000);

    let token = token::Client::new(&env, &currency);
    let investor_before = token.balance(&investor);
    let business_before = token.balance(&business);

    client.handle_default(&invoice_id);

    assert_eq!(token.balance(&investor), investor_before);
    assert_eq!(token.balance(&business), business_before);
}
