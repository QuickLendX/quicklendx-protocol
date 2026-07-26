#![cfg(test)]

use super::*;
use crate::errors::QuickLendXError;
use crate::investment::{InvestmentStatus, InvestmentStorage};
use crate::invoice::InvoiceCategory;
use crate::payments::{EscrowStatus, EscrowStorage};
use crate::types::InvoiceStatus;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, BytesN, Env, String, Vec,
};

fn setup_env() -> (
    Env,
    QuickLendXContractClient<'static>,
    Address,
    Address,
    Address,
    Address,
) {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000_000);
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.set_admin(&admin);
    client.initialize_fee_system(&admin);
    let business = Address::generate(&env);
    let investor = Address::generate(&env);
    (env, client, contract_id, admin, business, investor)
}

fn make_token(env: &Env, contract_id: &Address, business: &Address, investor: &Address) -> Address {
    let token_admin = Address::generate(env);
    let currency = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();
    let sac = token::StellarAssetClient::new(env, &currency);
    sac.mint(business, &100_000i128);
    sac.mint(investor, &100_000i128);
    sac.mint(contract_id, &1i128);
    let tok = token::Client::new(env, &currency);
    let exp = env.ledger().sequence() + 50_000;
    tok.approve(business, contract_id, &400_000i128, &exp);
    tok.approve(investor, contract_id, &400_000i128, &exp);
    currency
}

fn setup_funded_investment(
    env: &Env,
    client: &QuickLendXContractClient<'static>,
    admin: &Address,
    business: &Address,
    investor: &Address,
    currency: &Address,
    invoice_amount: i128,
    bid_amount: i128,
) -> BytesN<32> {
    client.submit_kyc_application(business, &String::from_str(env, "KYC"));
    client.verify_business(admin, business);

    client.submit_investor_kyc(investor, &String::from_str(env, "KYC"));
    client.verify_investor(investor, &200_000i128);

    let due_date = env.ledger().timestamp() + 86_400;
    let invoice_id = client.upload_invoice(
        business,
        &invoice_amount,
        currency,
        &due_date,
        &String::from_str(env, "Test invoice"),
        &InvoiceCategory::Services,
        &Vec::new(env),
        &None);
    client.verify_invoice(&invoice_id);

    let bid_id = client.place_bid(investor, &invoice_id, &bid_amount, &(bid_amount + 100), &BytesN::from_array(&env, &[0u8; 32]));
    client.accept_bid(&invoice_id, &bid_id);

    invoice_id
}

/// Happy Path: Adding insurance on an Active investment succeeds.
#[test]
fn test_add_insurance_succeeds_when_investment_is_active() {
    let (env, client, contract_id, admin, business, investor) = setup_env();
    let currency = make_token(&env, &contract_id, &business, &investor);
    let invoice_id = setup_funded_investment(
        &env, &client, &admin, &business, &investor, &currency, 1000, 1000,
    );

    let investment = env.as_contract(&contract_id, || {
        InvestmentStorage::get_investment_by_invoice(&env, &invoice_id).unwrap()
    });
    assert_eq!(investment.status, InvestmentStatus::Active);

    let provider = Address::generate(&env);
    // Mint tokens for provider to pay premium
    let sac = token::StellarAssetClient::new(&env, &currency);
    sac.mint(&provider, &100_000i128);
    let tok = token::Client::new(&env, &currency);
    tok.approve(
        &provider,
        &contract_id,
        &400_000i128,
        &(env.ledger().sequence() + 50_000),
    );

    let res = client.try_add_investment_insurance(&investment.investment_id, &provider, &50u32);
    assert!(
        res.is_ok(),
        "Should succeed in adding insurance when Active"
    );
}

/// Sad Path: Adding insurance on a Completed investment fails.
#[test]
fn test_add_insurance_fails_when_investment_is_completed() {
    let (env, client, contract_id, admin, business, investor) = setup_env();
    let currency = make_token(&env, &contract_id, &business, &investor);
    let invoice_id = setup_funded_investment(
        &env, &client, &admin, &business, &investor, &currency, 1000, 1000,
    );

    // Settle to transition status to Completed
    let sac = token::StellarAssetClient::new(&env, &currency);
    sac.mint(&business, &2_000i128);
    let tok = token::Client::new(&env, &currency);
    tok.approve(
        &business,
        &contract_id,
        &400_000i128,
        &(env.ledger().sequence() + 50_000),
    );

    client.settle_invoice(&invoice_id, &1000);

    let investment = env.as_contract(&contract_id, || {
        InvestmentStorage::get_investment_by_invoice(&env, &invoice_id).unwrap()
    });
    assert_eq!(investment.status, InvestmentStatus::Completed);

    let provider = Address::generate(&env);
    let err = client
        .try_add_investment_insurance(&investment.investment_id, &provider, &50u32)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, QuickLendXError::InvalidStatus);
}

/// Sad Path: Adding insurance on a Defaulted investment fails.
#[test]
fn test_add_insurance_fails_when_investment_is_defaulted() {
    let (env, client, contract_id, admin, business, investor) = setup_env();
    let currency = make_token(&env, &contract_id, &business, &investor);
    let invoice_id = setup_funded_investment(
        &env, &client, &admin, &business, &investor, &currency, 1000, 1000,
    );

    // Advance time past due date + grace period
    env.ledger()
        .set_timestamp(env.ledger().timestamp() + 86_400 * 40);
    client.handle_overdue_invoices(&100u32);

    let investment = env.as_contract(&contract_id, || {
        InvestmentStorage::get_investment_by_invoice(&env, &invoice_id).unwrap()
    });
    assert_eq!(investment.status, InvestmentStatus::Defaulted);

    let provider = Address::generate(&env);
    let err = client
        .try_add_investment_insurance(&investment.investment_id, &provider, &50u32)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, QuickLendXError::InvalidStatus);
}

/// Sad Path: Adding insurance on a Refunded investment fails.
#[test]
fn test_add_insurance_fails_when_investment_is_refunded() {
    let (env, client, contract_id, admin, business, investor) = setup_env();
    let currency = make_token(&env, &contract_id, &business, &investor);
    let invoice_id = setup_funded_investment(
        &env, &client, &admin, &business, &investor, &currency, 1000, 1000,
    );

    // Cancel invoice to transition investment to Refunded
    client.cancel_invoice(&invoice_id);

    let investment = env.as_contract(&contract_id, || {
        InvestmentStorage::get_investment_by_invoice(&env, &invoice_id).unwrap()
    });
    assert_eq!(investment.status, InvestmentStatus::Refunded);

    let provider = Address::generate(&env);
    let err = client
        .try_add_investment_insurance(&investment.investment_id, &provider, &50u32)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, QuickLendXError::InvalidStatus);
}

/// Sad Path: Adding insurance on a Withdrawn investment fails.
#[test]
fn test_add_insurance_fails_when_investment_is_withdrawn() {
    let (env, client, contract_id, admin, business, investor) = setup_env();
    let currency = make_token(&env, &contract_id, &business, &investor);
    let invoice_id = setup_funded_investment(
        &env, &client, &admin, &business, &investor, &currency, 1000, 1000,
    );

    // Withdraw investment to transition status to Withdrawn
    client.withdraw_investment(&invoice_id, &investor);

    let investment = env.as_contract(&contract_id, || {
        InvestmentStorage::get_investment_by_invoice(&env, &invoice_id).unwrap()
    });
    assert_eq!(investment.status, InvestmentStatus::Withdrawn);

    let provider = Address::generate(&env);
    let err = client
        .try_add_investment_insurance(&investment.investment_id, &provider, &50u32)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, QuickLendXError::InvalidStatus);
}

/// Happy Path: Withdrawing an Active investment succeeds.
#[test]
fn test_withdraw_succeeds_when_investment_is_active() {
    let (env, client, contract_id, admin, business, investor) = setup_env();
    let currency = make_token(&env, &contract_id, &business, &investor);
    let invoice_id = setup_funded_investment(
        &env, &client, &admin, &business, &investor, &currency, 1000, 1000,
    );

    let res = client.try_withdraw_investment(&invoice_id, &investor);
    assert!(
        res.is_ok(),
        "Should succeed in withdrawing active investment"
    );
}

/// Sad Path: Withdrawing a Completed investment fails.
#[test]
fn test_withdraw_fails_when_investment_is_completed() {
    let (env, client, contract_id, admin, business, investor) = setup_env();
    let currency = make_token(&env, &contract_id, &business, &investor);
    let invoice_id = setup_funded_investment(
        &env, &client, &admin, &business, &investor, &currency, 1000, 1000,
    );

    // Settle to transition status to Completed
    let sac = token::StellarAssetClient::new(&env, &currency);
    sac.mint(&business, &2_000i128);
    let tok = token::Client::new(&env, &currency);
    tok.approve(
        &business,
        &contract_id,
        &400_000i128,
        &(env.ledger().sequence() + 50_000),
    );

    client.settle_invoice(&invoice_id, &1000);

    let err = client
        .try_withdraw_investment(&invoice_id, &investor)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, QuickLendXError::InvalidStatus);
}

/// Sad Path: Withdrawing a Defaulted investment fails.
#[test]
fn test_withdraw_fails_when_investment_is_defaulted() {
    let (env, client, contract_id, admin, business, investor) = setup_env();
    let currency = make_token(&env, &contract_id, &business, &investor);
    let invoice_id = setup_funded_investment(
        &env, &client, &admin, &business, &investor, &currency, 1000, 1000,
    );

    // Advance time past due date + grace period
    env.ledger()
        .set_timestamp(env.ledger().timestamp() + 86_400 * 40);
    client.handle_overdue_invoices(&100u32);

    let err = client
        .try_withdraw_investment(&invoice_id, &investor)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, QuickLendXError::InvalidStatus);
}

/// Sad Path: Withdrawing a Refunded investment fails.
#[test]
fn test_withdraw_fails_when_investment_is_refunded() {
    let (env, client, contract_id, admin, business, investor) = setup_env();
    let currency = make_token(&env, &contract_id, &business, &investor);
    let invoice_id = setup_funded_investment(
        &env, &client, &admin, &business, &investor, &currency, 1000, 1000,
    );

    // Cancel invoice to transition investment to Refunded
    client.cancel_invoice(&invoice_id);

    let err = client
        .try_withdraw_investment(&invoice_id, &investor)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, QuickLendXError::InvalidStatus);
}

/// Sad Path: Withdrawing a Withdrawn investment fails.
#[test]
fn test_withdraw_fails_when_investment_is_already_withdrawn() {
    let (env, client, contract_id, admin, business, investor) = setup_env();
    let currency = make_token(&env, &contract_id, &business, &investor);
    let invoice_id = setup_funded_investment(
        &env, &client, &admin, &business, &investor, &currency, 1000, 1000,
    );

    // Withdraw once
    client.withdraw_investment(&invoice_id, &investor);

    let err = client
        .try_withdraw_investment(&invoice_id, &investor)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, QuickLendXError::InvalidStatus);
}

/// Sad Path: Settling an invoice fails when the associated investment is Withdrawn.
#[test]
fn test_settle_fails_when_investment_is_withdrawn() {
    let (env, client, contract_id, admin, business, investor) = setup_env();
    let currency = make_token(&env, &contract_id, &business, &investor);
    let invoice_id = setup_funded_investment(
        &env, &client, &admin, &business, &investor, &currency, 1000, 1000,
    );

    // Withdraw investment to transition status to Withdrawn
    client.withdraw_investment(&invoice_id, &investor);

    // Mint tokens for business to pay
    let sac = token::StellarAssetClient::new(&env, &currency);
    sac.mint(&business, &2_000i128);
    let tok = token::Client::new(&env, &currency);
    tok.approve(
        &business,
        &contract_id,
        &400_000i128,
        &(env.ledger().sequence() + 50_000),
    );

    let err = client
        .try_settle_invoice(&invoice_id, &1000)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, QuickLendXError::InvalidStatus);
}
