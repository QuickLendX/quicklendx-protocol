//! Full investment lifecycle transition matrix (Issue #1949).
//!
//! Pins every `(from, to)` pair for [`InvestmentStatus::validate_transition`]
//! and exercises the four legal Active → terminal entrypoint paths plus
//! terminal immutability sad paths.
//!
//! ## Legal transition set
//!
//! | From | Allowed To |
//! |------|------------|
//! | Active | Completed, Defaulted, Refunded, Withdrawn |
//! | Completed / Defaulted / Refunded / Withdrawn | *(terminal — none)* |
//!
//! Self-transitions are illegal at the guard. Same-status storage updates skip
//! the guard (no-op) and are not covered here.
//!
//! This module is `#[cfg(test)]` only (no `legacy-tests` gate) so it runs on
//! every CI matrix entry.

#![cfg(test)]

use crate::errors::QuickLendXError;
use crate::investment::InvestmentStatus;
use crate::invoice::InvoiceCategory;
use crate::types::InvoiceStatus;
use crate::{QuickLendXContract, QuickLendXContractClient};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, BytesN, Env, String, Vec,
};

const ALL_STATUSES: [InvestmentStatus; 5] = [
    InvestmentStatus::Active,
    InvestmentStatus::Withdrawn,
    InvestmentStatus::Completed,
    InvestmentStatus::Defaulted,
    InvestmentStatus::Refunded,
];

/// Expected legality for `from → to` (mirrors `InvestmentStatus::validate_transition`).
fn is_legal_transition(from: InvestmentStatus, to: InvestmentStatus) -> bool {
    matches!(
        (from, to),
        (
            InvestmentStatus::Active,
            InvestmentStatus::Completed
                | InvestmentStatus::Defaulted
                | InvestmentStatus::Refunded
                | InvestmentStatus::Withdrawn
        )
    )
}

// ============================================================================
// Helpers
// ============================================================================

fn setup() -> (Env, QuickLendXContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.set_admin(&admin);
    client.initialize_fee_system(&admin);
    // Zero FeeManager platform fee so settlement payouts stay above dust MIN_TRANSFER.
    client.update_platform_fee_bps(&0u32);
    (env, client, admin)
}

fn make_token(
    env: &Env,
    contract_id: &Address,
    business: &Address,
    investor: &Address,
) -> Address {
    let token_admin = Address::generate(env);
    let currency = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();
    let sac = token::StellarAssetClient::new(env, &currency);
    let tok = token::Client::new(env, &currency);
    sac.mint(business, &30_000i128);
    sac.mint(investor, &30_000i128);
    sac.mint(contract_id, &1i128);
    let exp = env.ledger().sequence() + 50_000;
    tok.approve(business, contract_id, &120_000i128, &exp);
    tok.approve(investor, contract_id, &120_000i128, &exp);
    currency
}

fn funded_invoice(
    env: &Env,
    client: &QuickLendXContractClient,
    admin: &Address,
    invoice_amount: i128,
    bid_amount: i128,
) -> (Address, Address, Address, BytesN<32>) {
    let business = Address::generate(env);
    let investor = Address::generate(env);
    let currency = make_token(env, &client.address, &business, &investor);

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
        &String::from_str(env, "matrix invoice"),
        &InvoiceCategory::Services,
        &Vec::new(env), &None);
    client.verify_invoice(&invoice_id);
    let bid_id = client.place_bid(&investor, &invoice_id, &bid_amount, &invoice_amount, &BytesN::from_array(&env, &[0u8; 32]));
    client.accept_bid(&invoice_id, &bid_id);

    (business, investor, currency, invoice_id)
}

fn mint_and_approve_settlement(
    env: &Env,
    client: &QuickLendXContractClient,
    business: &Address,
    currency: &Address,
    amount: i128,
) {
    let sac = token::StellarAssetClient::new(env, currency);
    sac.mint(business, &amount);
    let tok = token::Client::new(env, currency);
    tok.approve(
        business,
        &client.address,
        &(amount * 4),
        &(env.ledger().sequence() + 10_000),
    );
}

// ============================================================================
// Full 5×5 validate_transition matrix
// ============================================================================

/// Exhaustive cartesian product over all five `InvestmentStatus` values.
/// Asserts `Ok(())` only for the four legal Active → terminal edges; every
/// other cell (including `Active → Active` and all terminal self-loops) must
/// return `InvalidStatus`.
#[test]
fn test_investment_status_transition_matrix() {
    let mut checked = 0u32;

    for from in ALL_STATUSES {
        for to in ALL_STATUSES {
            let result = InvestmentStatus::validate_transition(&from, &to);
            let expected_ok = is_legal_transition(from, to);

            assert_eq!(
                result.is_ok(),
                expected_ok,
                "transition {:?} → {:?} expected ok={}, got {:?}",
                from,
                to,
                expected_ok,
                result
            );

            if !expected_ok {
                assert_eq!(
                    result.unwrap_err(),
                    QuickLendXError::InvalidStatus,
                    "illegal {:?} → {:?} must return InvalidStatus",
                    from,
                    to
                );
            }

            checked += 1;
        }
    }

    assert_eq!(checked, 25, "full matrix must cover 5×5 = 25 cells");
}

/// Table-driven pin of the four legal edges (happy path at the guard).
#[test]
fn test_active_to_terminal_transitions_succeed() {
    let allowed = [
        InvestmentStatus::Completed,
        InvestmentStatus::Defaulted,
        InvestmentStatus::Refunded,
        InvestmentStatus::Withdrawn,
    ];
    for to in allowed {
        assert!(
            InvestmentStatus::validate_transition(&InvestmentStatus::Active, &to).is_ok(),
            "Active → {:?} must be allowed",
            to
        );
    }
}

/// Explicit sad path: Active may not self-transition.
#[test]
fn test_active_to_active_returns_invalid_status() {
    let result =
        InvestmentStatus::validate_transition(&InvestmentStatus::Active, &InvestmentStatus::Active);
    assert_eq!(result.unwrap_err(), QuickLendXError::InvalidStatus);
}

/// Every terminal status rejects transitions to every status (including self).
#[test]
fn test_terminal_statuses_reject_all_targets() {
    let terminals = [
        InvestmentStatus::Completed,
        InvestmentStatus::Defaulted,
        InvestmentStatus::Refunded,
        InvestmentStatus::Withdrawn,
    ];
    for from in terminals {
        for to in ALL_STATUSES {
            let result = InvestmentStatus::validate_transition(&from, &to);
            assert_eq!(
                result.unwrap_err(),
                QuickLendXError::InvalidStatus,
                "terminal {:?} → {:?} must be rejected",
                from,
                to
            );
        }
    }
}

// ============================================================================
// Entrypoint happy paths (Active → each terminal)
// ============================================================================

#[test]
fn test_entrypoint_active_to_completed_via_settle() {
    let (env, client, admin) = setup();
    let (business, _investor, currency, invoice_id) =
        funded_invoice(&env, &client, &admin, 1_000, 900);

    assert_eq!(
        client.get_invoice_investment(&invoice_id).status,
        InvestmentStatus::Active
    );

    mint_and_approve_settlement(&env, &client, &business, &currency, 1_000);
    client.settle_invoice(&invoice_id, &1_000i128);

    let inv = client.get_invoice_investment(&invoice_id);
    assert_eq!(inv.status, InvestmentStatus::Completed);
    assert!(!client.get_active_investment_ids().contains(&inv.investment_id));
    assert!(client.validate_no_orphan_investments());
    assert_eq!(client.get_invoice(&invoice_id).status, InvoiceStatus::Paid);
}

#[test]
fn test_entrypoint_active_to_defaulted_via_mark_defaulted() {
    let (env, client, admin) = setup();
    let (_business, _investor, _currency, invoice_id) =
        funded_invoice(&env, &client, &admin, 1_000, 900);

    let invoice = client.get_invoice(&invoice_id);
    let grace = 7 * 24 * 60 * 60u64;
    env.ledger().set_timestamp(invoice.due_date + grace + 1);
    client.mark_invoice_defaulted(&invoice_id, &Some(grace));

    let inv = client.get_invoice_investment(&invoice_id);
    assert_eq!(inv.status, InvestmentStatus::Defaulted);
    assert!(!client.get_active_investment_ids().contains(&inv.investment_id));
    assert!(client.validate_no_orphan_investments());
}

#[test]
fn test_entrypoint_active_to_refunded_via_refund_escrow() {
    let (env, client, admin) = setup();
    let (business, _investor, _currency, invoice_id) =
        funded_invoice(&env, &client, &admin, 1_000, 900);

    // refund_escrow_funds authorizes admin or the invoice business (not investor).
    client.refund_escrow_funds(&invoice_id, &business);

    let inv = client.get_invoice_investment(&invoice_id);
    assert_eq!(inv.status, InvestmentStatus::Refunded);
    assert!(!client.get_active_investment_ids().contains(&inv.investment_id));
    assert!(client.validate_no_orphan_investments());
}

#[test]
fn test_entrypoint_active_to_withdrawn_via_withdraw() {
    let (env, client, admin) = setup();
    let (_business, investor, _currency, invoice_id) =
        funded_invoice(&env, &client, &admin, 1_000, 900);

    client.withdraw_investment(&invoice_id, &investor);

    let inv = client.get_invoice_investment(&invoice_id);
    assert_eq!(inv.status, InvestmentStatus::Withdrawn);
    assert!(!client.get_active_investment_ids().contains(&inv.investment_id));
    assert!(client.validate_no_orphan_investments());
}

// ============================================================================
// Entrypoint sad paths (terminal immutability)
// ============================================================================

#[test]
fn test_entrypoint_double_settle_rejected() {
    let (env, client, admin) = setup();
    let (business, _investor, currency, invoice_id) =
        funded_invoice(&env, &client, &admin, 1_000, 900);

    mint_and_approve_settlement(&env, &client, &business, &currency, 2_000);
    client.settle_invoice(&invoice_id, &1_000i128);

    let result = client.try_settle_invoice(&invoice_id, &1_000i128);
    assert!(result.is_err(), "second settle must fail");
    assert_eq!(
        client.get_invoice_investment(&invoice_id).status,
        InvestmentStatus::Completed
    );
}

#[test]
fn test_entrypoint_double_default_rejected() {
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
        client.get_invoice_investment(&invoice_id).status,
        InvestmentStatus::Defaulted
    );
}

#[test]
fn test_entrypoint_settle_after_refund_rejected() {
    let (env, client, admin) = setup();
    let (business, _investor, currency, invoice_id) =
        funded_invoice(&env, &client, &admin, 1_000, 900);

    client.refund_escrow_funds(&invoice_id, &business);
    assert_eq!(
        client.get_invoice_investment(&invoice_id).status,
        InvestmentStatus::Refunded
    );

    mint_and_approve_settlement(&env, &client, &business, &currency, 1_000);
    let result = client.try_settle_invoice(&invoice_id, &1_000i128);
    assert!(result.is_err(), "settle after refund must fail");
    assert_eq!(
        client.get_invoice_investment(&invoice_id).status,
        InvestmentStatus::Refunded
    );
}

#[test]
fn test_entrypoint_withdraw_after_completed_rejected() {
    let (env, client, admin) = setup();
    let (business, investor, currency, invoice_id) =
        funded_invoice(&env, &client, &admin, 1_000, 900);

    mint_and_approve_settlement(&env, &client, &business, &currency, 1_000);
    client.settle_invoice(&invoice_id, &1_000i128);

    let result = client.try_withdraw_investment(&invoice_id, &investor);
    assert!(result.is_err(), "withdraw after completed must fail");
    assert_eq!(
        client.get_invoice_investment(&invoice_id).status,
        InvestmentStatus::Completed
    );
}
