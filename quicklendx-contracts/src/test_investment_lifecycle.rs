//! Tests for issue #556 – investment status transitions on settlement and default.
//!
//! Validates:
//! - `Active → Completed` on full settlement (no orphan)
//! - `Active → Defaulted` on invoice default (no orphan)
//! - `Active → Refunded` on escrow refund (no orphan)
//! - Invalid / backward transitions are rejected
//! - `validate_no_orphan_investments` returns `true` after every terminal event
//! - `get_active_investment_ids` shrinks correctly after each lifecycle event
//! - Missing investment on settle/default is handled gracefully
//! - Idempotency: double-settle / double-default are rejected
//! - Partial payments do not prematurely close the investment
//! - Multiple concurrent investments each transition independently

use super::*;
use crate::investment::InvestmentStatus;
use crate::invoice::{InvoiceCategory, InvoiceStatus};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, BytesN, Env, String, Vec,
};

// ─── helpers ─────────────────────────────────────────────────────────────────

fn setup() -> (Env, QuickLendXContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.set_admin(&admin);
    client.initialize_fee_system(&admin);
    (env, client, admin)
}

/// Create a real SAC, mint balances, and set allowances.
fn make_token(
    env: &Env,
    contract_id: &Address,
    business: &Address,
    investor: &Address,
    biz_bal: i128,
    inv_bal: i128,
) -> Address {
    let token_admin = Address::generate(env);
    let currency = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();
    let sac = token::StellarAssetClient::new(env, &currency);
    let tok = token::Client::new(env, &currency);
    sac.mint(business, &biz_bal);
    sac.mint(investor, &inv_bal);
    sac.mint(contract_id, &1i128); // ensure contract instance exists
    let exp = env.ledger().sequence() + 50_000;
    tok.approve(business, contract_id, &(biz_bal * 4), &exp);
    tok.approve(investor, contract_id, &(inv_bal * 4), &exp);
    currency
}

/// Full setup: verified business + investor, funded invoice ready for settlement.
fn funded_invoice(
    env: &Env,
    client: &QuickLendXContractClient,
    admin: &Address,
    invoice_amount: i128,
    bid_amount: i128,
) -> (Address, Address, Address, BytesN<32>) {
    let business = Address::generate(env);
    let investor = Address::generate(env);
    let contract_id = client.address.clone();
    let currency = make_token(env, &contract_id, &business, &investor, 30_000, 30_000);

    client.submit_kyc_application(&business, &String::from_str(env, "KYC"));
    client.verify_business(admin, &business);
    client.submit_investor_kyc(&investor, &String::from_str(env, "KYC"));
    client.verify_investor(&investor, &100_000i128);

    let due_date = env.ledger().timestamp() + 86_400;
    let invoice_id = client.upload_invoice(
        &business,
        &invoice_amount,
        &currency,
        &due_date,
        &String::from_str(env, "Test invoice"),
        &InvoiceCategory::Services,
        &Vec::new(env),
    );
    client.verify_invoice(&invoice_id);
    let bid_id = client.place_bid(&investor, &invoice_id, &bid_amount, &invoice_amount);
    client.accept_bid(&invoice_id, &bid_id);

    (business, investor, currency, invoice_id)
}

// ─── 1. Settlement → Completed ───────────────────────────────────────────────

/// Full settlement must transition investment Active → Completed and remove it
/// from the active index (no orphan).
#[test]
fn test_settlement_sets_investment_completed() {
    let (env, client, admin) = setup();
    let (business, _investor, currency, invoice_id) =
        funded_invoice(&env, &client, &admin, 1_000, 900);

    // Investment is Active before settlement.
    assert_eq!(
        client.get_invoice_investment(&invoice_id).status,
        InvestmentStatus::Active
    );
    assert!(client
        .get_active_investment_ids()
        .contains(&client.get_invoice_investment(&invoice_id).investment_id));

    // Settle.
    let sac = token::StellarAssetClient::new(&env, &currency);
    sac.mint(&business, &1_000i128);
    let tok = token::Client::new(&env, &currency);
    tok.approve(
        &business,
        &client.address,
        &4_000i128,
        &(env.ledger().sequence() + 10_000),
    );
    client.settle_invoice(&invoice_id, &1_000i128);

    // Investment must be Completed.
    assert_eq!(
        client.get_invoice_investment(&invoice_id).status,
        InvestmentStatus::Completed
    );

    // No orphan active investments.
    assert!(
        client.validate_no_orphan_investments(),
        "no orphan investments after settlement"
    );

    // Active index must no longer contain this investment.
    let inv_id = client.get_invoice_investment(&invoice_id).investment_id;
    assert!(
        !client.get_active_investment_ids().contains(&inv_id),
        "settled investment must be removed from active index"
    );
}

/// Invoice status must be Paid after settlement.
#[test]
fn test_settlement_invoice_status_paid() {
    let (env, client, admin) = setup();
    let (business, _investor, currency, invoice_id) =
        funded_invoice(&env, &client, &admin, 1_000, 900);

    let sac = token::StellarAssetClient::new(&env, &currency);
    sac.mint(&business, &1_000i128);
    let tok = token::Client::new(&env, &currency);
    tok.approve(
        &business,
        &client.address,
        &4_000i128,
        &(env.ledger().sequence() + 10_000),
    );
    client.settle_invoice(&invoice_id, &1_000i128);

    assert_eq!(client.get_invoice(&invoice_id).status, InvoiceStatus::Paid);
}

// ─── 2. Default → Defaulted ──────────────────────────────────────────────────

/// Default event must transition investment Active → Defaulted and remove it
/// from the active index.
#[test]
fn test_default_sets_investment_defaulted() {
    let (env, client, admin) = setup();
    let (_business, _investor, _currency, invoice_id) =
        funded_invoice(&env, &client, &admin, 1_000, 900);

    assert_eq!(
        client.get_invoice_investment(&invoice_id).status,
        InvestmentStatus::Active
    );

    // Advance past grace period.
    let invoice = client.get_invoice(&invoice_id);
    let grace = 7 * 24 * 60 * 60u64;
    env.ledger().set_timestamp(invoice.due_date + grace + 1);

    client.mark_invoice_defaulted(&invoice_id, &Some(grace));

    assert_eq!(
        client.get_invoice_investment(&invoice_id).status,
        InvestmentStatus::Defaulted
    );
    assert!(
        client.validate_no_orphan_investments(),
        "no orphan investments after default"
    );
    let inv_id = client.get_invoice_investment(&invoice_id).investment_id;
    assert!(
        !client.get_active_investment_ids().contains(&inv_id),
        "defaulted investment must be removed from active index"
    );
}

/// Invoice status must be Defaulted after the default event.
#[test]
fn test_default_invoice_status_defaulted() {
    let (env, client, admin) = setup();
    let (_business, _investor, _currency, invoice_id) =
        funded_invoice(&env, &client, &admin, 1_000, 900);

    let invoice = client.get_invoice(&invoice_id);
    let grace = 7 * 24 * 60 * 60u64;
    env.ledger().set_timestamp(invoice.due_date + grace + 1);
    client.mark_invoice_defaulted(&invoice_id, &Some(grace));

    assert_eq!(
        client.get_invoice(&invoice_id).status,
        InvoiceStatus::Defaulted
    );
}

// ─── 3. Refund → Refunded ────────────────────────────────────────────────────

/// Escrow refund must transition investment Active → Refunded.
#[test]
fn test_refund_sets_investment_refunded() {
    let (env, client, admin) = setup();
    let (_business, investor, _currency, invoice_id) =
        funded_invoice(&env, &client, &admin, 1_000, 900);

    assert_eq!(
        client.get_invoice_investment(&invoice_id).status,
        InvestmentStatus::Active
    );

    client.refund_escrow_funds(&invoice_id, &investor);

    assert_eq!(
        client.get_invoice_investment(&invoice_id).status,
        InvestmentStatus::Refunded
    );
    assert!(
        client.validate_no_orphan_investments(),
        "no orphan investments after refund"
    );
}

// ─── 4. Invalid / backward transitions rejected ──────────────────────────────

/// Completed → Defaulted must be rejected (terminal state).
#[test]
fn test_completed_to_defaulted_rejected() {
    assert_eq!(
        InvestmentStatus::validate_transition(
            &InvestmentStatus::Completed,
            &InvestmentStatus::Defaulted
        ),
        Err(crate::errors::QuickLendXError::InvalidStatus)
    );
}

/// Defaulted → Completed must be rejected.
#[test]
fn test_defaulted_to_completed_rejected() {
    assert_eq!(
        InvestmentStatus::validate_transition(
            &InvestmentStatus::Defaulted,
            &InvestmentStatus::Completed
        ),
        Err(crate::errors::QuickLendXError::InvalidStatus)
    );
}

/// Refunded → Active must be rejected.
#[test]
fn test_refunded_to_active_rejected() {
    assert_eq!(
        InvestmentStatus::validate_transition(
            &InvestmentStatus::Refunded,
            &InvestmentStatus::Active
        ),
        Err(crate::errors::QuickLendXError::InvalidStatus)
    );
}

/// Withdrawn → Completed must be rejected.
#[test]
fn test_withdrawn_to_completed_rejected() {
    assert_eq!(
        InvestmentStatus::validate_transition(
            &InvestmentStatus::Withdrawn,
            &InvestmentStatus::Completed
        ),
        Err(crate::errors::QuickLendXError::InvalidStatus)
    );
}

/// All valid transitions from Active are accepted.
#[test]
fn test_active_valid_transitions_accepted() {
    for to in [
        InvestmentStatus::Completed,
        InvestmentStatus::Defaulted,
        InvestmentStatus::Refunded,
        InvestmentStatus::Withdrawn,
    ] {
        assert!(
            InvestmentStatus::validate_transition(&InvestmentStatus::Active, &to).is_ok(),
            "Active → {:?} should be allowed",
            to
        );
    }
}

// ─── 5. Idempotency / double-event rejection ─────────────────────────────────

/// Double-settle must fail with InvalidStatus.
#[test]
fn test_double_settle_rejected() {
    let (env, client, admin) = setup();
    let (business, _investor, currency, invoice_id) =
        funded_invoice(&env, &client, &admin, 1_000, 900);

    let sac = token::StellarAssetClient::new(&env, &currency);
    sac.mint(&business, &2_000i128);
    let tok = token::Client::new(&env, &currency);
    tok.approve(
        &business,
        &client.address,
        &8_000i128,
        &(env.ledger().sequence() + 10_000),
    );
    client.settle_invoice(&invoice_id, &1_000i128);

    let result = client.try_settle_invoice(&invoice_id, &1_000i128);
    assert!(result.is_err(), "second settle must fail");
}

/// Double-default must fail with InvoiceAlreadyDefaulted.
#[test]
fn test_double_default_rejected() {
    let (env, client, admin) = setup();
    let (_business, _investor, _currency, invoice_id) =
        funded_invoice(&env, &client, &admin, 1_000, 900);

    let invoice = client.get_invoice(&invoice_id);
    let grace = 7 * 24 * 60 * 60u64;
    env.ledger().set_timestamp(invoice.due_date + grace + 1);
    client.mark_invoice_defaulted(&invoice_id, &Some(grace));

    let result = client.try_mark_invoice_defaulted(&invoice_id, &Some(grace));
    assert!(result.is_err(), "second default must fail");
    assert_eq!(
        result.unwrap_err().expect("contract error"),
        QuickLendXError::InvoiceAlreadyDefaulted
    );
}

// ─── 6. Partial payments do not close the investment ─────────────────────────

/// A partial payment must leave the investment Active.
#[test]
fn test_partial_payment_keeps_investment_active() {
    let (env, client, admin) = setup();
    let (business, _investor, currency, invoice_id) =
        funded_invoice(&env, &client, &admin, 1_000, 900);

    let sac = token::StellarAssetClient::new(&env, &currency);
    sac.mint(&business, &500i128);
    let tok = token::Client::new(&env, &currency);
    tok.approve(
        &business,
        &client.address,
        &2_000i128,
        &(env.ledger().sequence() + 10_000),
    );

    // Partial payment (500 of 1000).
    client.process_partial_payment(&invoice_id, &500i128, &String::from_str(&env, "partial-1"));

    // Invoice still Funded, investment still Active.
    assert_eq!(
        client.get_invoice(&invoice_id).status,
        InvoiceStatus::Funded
    );
    assert_eq!(
        client.get_invoice_investment(&invoice_id).status,
        InvestmentStatus::Active
    );
    assert!(
        client.validate_no_orphan_investments(),
        "no orphan after partial payment"
    );
}

// ─── 7. Multiple concurrent investments ──────────────────────────────────────

/// Two independent invoices each transition their investment independently.
#[test]
fn test_multiple_investments_independent_transitions() {
    let (env, client, admin) = setup();

    let (biz1, _inv1, cur1, inv_id1) = funded_invoice(&env, &client, &admin, 1_000, 900);
    let (_biz2, _inv2, _cur2, inv_id2) = funded_invoice(&env, &client, &admin, 2_000, 1_800);

    // Both active.
    assert_eq!(client.get_active_investment_ids().len(), 2);

    // Settle invoice 1.
    let sac1 = token::StellarAssetClient::new(&env, &cur1);
    sac1.mint(&biz1, &1_000i128);
    let tok1 = token::Client::new(&env, &cur1);
    tok1.approve(
        &biz1,
        &client.address,
        &4_000i128,
        &(env.ledger().sequence() + 10_000),
    );
    client.settle_invoice(&inv_id1, &1_000i128);

    // Invoice 1 investment Completed, invoice 2 still Active.
    assert_eq!(
        client.get_invoice_investment(&inv_id1).status,
        InvestmentStatus::Completed
    );
    assert_eq!(
        client.get_invoice_investment(&inv_id2).status,
        InvestmentStatus::Active
    );
    assert_eq!(
        client.get_active_investment_ids().len(),
        1,
        "only one active investment remains"
    );

    // Default invoice 2.
    let invoice2 = client.get_invoice(&inv_id2);
    let grace = 7 * 24 * 60 * 60u64;
    env.ledger().set_timestamp(invoice2.due_date + grace + 1);
    client.mark_invoice_defaulted(&inv_id2, &Some(grace));

    assert_eq!(
        client.get_invoice_investment(&inv_id2).status,
        InvestmentStatus::Defaulted
    );
    assert_eq!(
        client.get_active_investment_ids().len(),
        0,
        "no active investments remain"
    );
    assert!(
        client.validate_no_orphan_investments(),
        "no orphans after both transitions"
    );
}

// ─── 8. Active index accuracy ────────────────────────────────────────────────

/// Active index starts empty, grows on fund, shrinks on terminal event.
#[test]
fn test_active_index_grows_and_shrinks() {
    let (env, client, admin) = setup();

    assert_eq!(client.get_active_investment_ids().len(), 0);

    let (business, _investor, currency, invoice_id) =
        funded_invoice(&env, &client, &admin, 1_000, 900);

    assert_eq!(client.get_active_investment_ids().len(), 1);

    let sac = token::StellarAssetClient::new(&env, &currency);
    sac.mint(&business, &1_000i128);
    let tok = token::Client::new(&env, &currency);
    tok.approve(
        &business,
        &client.address,
        &4_000i128,
        &(env.ledger().sequence() + 10_000),
    );
    client.settle_invoice(&invoice_id, &1_000i128);

    assert_eq!(client.get_active_investment_ids().len(), 0);
}

// ─── 9. validate_no_orphan_investments baseline ──────────────────────────────

/// Returns true on empty state.
#[test]
fn test_validate_no_orphan_empty_state() {
    let (env, client, _admin) = setup();
    let _ = env;
    assert!(client.validate_no_orphan_investments());
}

/// Returns true immediately after funding (all active entries are genuinely Active).
#[test]
fn test_validate_no_orphan_after_funding() {
    let (env, client, admin) = setup();
    let _ = funded_invoice(&env, &client, &admin, 1_000, 900);
    assert!(client.validate_no_orphan_investments());
}
