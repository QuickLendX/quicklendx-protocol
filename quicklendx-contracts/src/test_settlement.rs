use super::*;
use crate::investment::InvestmentStatus;
use crate::invoice::{InvoiceCategory, InvoiceStatus};
use crate::profits::calculate_profit;
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Events, Ledger},
    token, Address, BytesN, Env, String, Vec,
};

/// Helper function to verify investor for testing.
fn verify_investor_for_test(
    env: &Env,
    client: &QuickLendXContractClient,
    investor: &Address,
    limit: i128,
) {
    client.submit_investor_kyc(investor, &String::from_str(env, "Investor KYC"));
    client.verify_investor(investor, &limit);
}

/// Helper function to initialize a real token for settlement balance assertions.
fn init_currency_for_test(
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

/// Helper function to set up a funded invoice for testing.
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
        &String::from_str(env, "Test invoice for settlement"),
        &InvoiceCategory::Services,
        &Vec::new(env),
    );
    client.verify_invoice(&invoice_id);

    verify_investor_for_test(env, client, investor, 10_000);
    let bid_id = client.place_bid(investor, &invoice_id, &investment_amount, &invoice_amount);
    client.accept_bid(&invoice_id, &bid_id);

    invoice_id
}

fn has_event_with_topic(env: &Env, topic: soroban_sdk::Symbol) -> bool {
    use soroban_sdk::xdr::{ContractEventBody, ScVal};

    let topic_str = topic.to_string();
    let events = env.events().all();

    for event in events.events() {
        if let ContractEventBody::V0(v0) = &event.body {
            for candidate in v0.topics.iter() {
                if let ScVal::Symbol(symbol) = candidate {
                    if symbol.0.as_slice() == topic_str.as_bytes() {
                        return true;
                    }
                }
            }
        }
    }

    false
}

/// Test that unfunded invoices cannot be settled.
#[test]
fn test_cannot_settle_unfunded_invoice() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    let admin = Address::generate(&env);
    client.set_admin(&admin);

    client.submit_kyc_application(&business, &String::from_str(&env, "KYC data"));
    client.verify_business(&admin, &business);

    let due_date = env.ledger().timestamp() + 86_400;
    let invoice_id = client.store_invoice(
        &business,
        &1_000,
        &currency,
        &due_date,
        &String::from_str(&env, "Unfunded invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    client.verify_invoice(&invoice_id);

    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Verified);
    assert_eq!(invoice.funded_amount, 0);
    assert!(invoice.investor.is_none());

    let result = client.try_settle_invoice(&invoice_id, &1_000);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().unwrap(), QuickLendXError::InvalidStatus);
}

/// Test that settlement with Pending status fails.
#[test]
fn test_cannot_settle_pending_invoice() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    let admin = Address::generate(&env);
    client.set_admin(&admin);

    client.submit_kyc_application(&business, &String::from_str(&env, "KYC data"));
    client.verify_business(&admin, &business);

    let due_date = env.ledger().timestamp() + 86_400;
    let invoice_id = client.store_invoice(
        &business,
        &1_000,
        &currency,
        &due_date,
        &String::from_str(&env, "Pending invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Pending);

    let result = client.try_settle_invoice(&invoice_id, &1_000);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().unwrap(), QuickLendXError::InvalidStatus);
}

/// Test that payout matches expected return calculation.
#[test]
fn test_payout_matches_expected_return() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let business = Address::generate(&env);
    let investor = Address::generate(&env);
    let currency = init_currency_for_test(&env, &contract_id, &business, &investor);
    let invoice_amount = 1_000i128;
    let investment_amount = 900i128;
    let payment_amount = 1_000i128;

    let invoice_id = setup_funded_invoice(
        &env,
        &client,
        &business,
        &investor,
        &currency,
        invoice_amount,
        investment_amount,
    );

    let token_client = token::Client::new(&env, &currency);
    let initial_business_balance = token_client.balance(&business);
    let initial_investor_balance = token_client.balance(&investor);
    let initial_platform_balance = token_client.balance(&contract_id);

    let (expected_investor_return, expected_platform_fee) =
        calculate_profit(&env, investment_amount, payment_amount);

    client.settle_invoice(&invoice_id, &payment_amount);

    let final_business_balance = token_client.balance(&business);
    let final_investor_balance = token_client.balance(&investor);
    let final_platform_balance = token_client.balance(&contract_id);

    assert_eq!(
        initial_business_balance - payment_amount,
        final_business_balance,
    );
    assert_eq!(
        final_investor_balance - initial_investor_balance,
        expected_investor_return,
    );
    assert_eq!(
        final_platform_balance - initial_platform_balance,
        expected_platform_fee,
    );
    assert_eq!(
        expected_investor_return + expected_platform_fee,
        payment_amount,
    );
}

/// Test payout calculation with profit.
#[test]
fn test_payout_with_profit() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let business = Address::generate(&env);
    let investor = Address::generate(&env);
    let currency = init_currency_for_test(&env, &contract_id, &business, &investor);
    let invoice_amount = 1_000i128;
    let investment_amount = 800i128;
    let payment_amount = 1_000i128;

    let invoice_id = setup_funded_invoice(
        &env,
        &client,
        &business,
        &investor,
        &currency,
        invoice_amount,
        investment_amount,
    );

    let token_client = token::Client::new(&env, &currency);
    let initial_investor_balance = token_client.balance(&investor);
    let (expected_investor_return, _) = calculate_profit(&env, investment_amount, payment_amount);

    client.settle_invoice(&invoice_id, &payment_amount);

    let final_investor_balance = token_client.balance(&investor);
    let investor_received = final_investor_balance - initial_investor_balance;

    assert_eq!(investor_received, expected_investor_return);
    assert!(investor_received > investment_amount);
}

/// `settle_invoice` uses configured profit split correctly.
#[test]
fn test_settle_invoice_profit_split_matches_calculate_profit_and_config() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    client.set_admin(&admin);
    client.initialize_fee_system(&admin);
    client.update_platform_fee_bps(&500u32);

    let business = Address::generate(&env);
    let investor = Address::generate(&env);
    let currency = init_currency_for_test(&env, &contract_id, &business, &investor);
    let invoice_amount = 1_000i128;
    let investment_amount = 900i128;
    let payment_amount = 1_000i128;

    let invoice_id = setup_funded_invoice(
        &env,
        &client,
        &business,
        &investor,
        &currency,
        invoice_amount,
        investment_amount,
    );

    let config = client.get_platform_fee_config();
    assert_eq!(config.fee_bps, 500);

    let (expected_investor_return, expected_platform_fee) =
        client.calculate_profit(&investment_amount, &payment_amount);
    assert_eq!(
        expected_investor_return + expected_platform_fee,
        payment_amount,
    );

    let token_client = token::Client::new(&env, &currency);
    let initial_investor = token_client.balance(&investor);
    let initial_contract = token_client.balance(&contract_id);

    client.settle_invoice(&invoice_id, &payment_amount);

    let investor_received = token_client.balance(&investor) - initial_investor;
    let platform_received = token_client.balance(&contract_id) - initial_contract;

    assert_eq!(investor_received, expected_investor_return);
    assert_eq!(platform_received, expected_platform_fee);
}

/// Settlement amounts should match the configured platform fee basis points.
#[test]
fn test_settle_invoice_verify_amounts_with_get_platform_fee_config() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    client.set_admin(&admin);
    client.initialize_fee_system(&admin);
    client.update_platform_fee_bps(&200u32);

    let business = Address::generate(&env);
    let investor = Address::generate(&env);
    let currency = init_currency_for_test(&env, &contract_id, &business, &investor);
    let investment_amount = 800i128;
    let payment_amount = 1_000i128;

    let invoice_id = setup_funded_invoice(
        &env,
        &client,
        &business,
        &investor,
        &currency,
        1_000i128,
        investment_amount,
    );

    let config = client.get_platform_fee_config();
    assert_eq!(config.fee_bps, 200);

    let (investor_return, platform_fee) =
        client.calculate_profit(&investment_amount, &payment_amount);
    assert_eq!(platform_fee, 4);
    assert_eq!(investor_return, 996);

    let token_client = token::Client::new(&env, &currency);
    let initial_investor = token_client.balance(&investor);
    let initial_platform = token_client.balance(&contract_id);

    client.settle_invoice(&invoice_id, &payment_amount);

    assert_eq!(
        token_client.balance(&investor) - initial_investor,
        investor_return
    );
    assert_eq!(
        token_client.balance(&contract_id) - initial_platform,
        platform_fee
    );
}

/// Overpayment attempts during final settlement must be rejected without side effects.
#[test]
fn test_settle_invoice_rejects_overpayment_without_mutating_accounting() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let business = Address::generate(&env);
    let investor = Address::generate(&env);
    let currency = init_currency_for_test(&env, &contract_id, &business, &investor);
    let invoice_id =
        setup_funded_invoice(&env, &client, &business, &investor, &currency, 1_000, 900);

    client.process_partial_payment(&invoice_id, &400, &String::from_str(&env, "prepay-1"));

    let token_client = token::Client::new(&env, &currency);
    let business_before = token_client.balance(&business);
    let investor_before = token_client.balance(&investor);
    let platform_before = token_client.balance(&contract_id);
    let events_before = env.events().all().events().len();
    let invoice_before = client.get_invoice(&invoice_id);
    let investment_before = client.get_invoice_investment(&invoice_id);

    let result = client.try_settle_invoice(&invoice_id, &700);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().unwrap(), QuickLendXError::InvalidAmount);

    let invoice_after = client.get_invoice(&invoice_id);
    assert_eq!(invoice_after.total_paid, invoice_before.total_paid);
    assert_eq!(invoice_after.status, InvoiceStatus::Funded);
    assert_eq!(
        invoice_after.payment_history.len(),
        invoice_before.payment_history.len()
    );

    let investment_after = client.get_invoice_investment(&invoice_id);
    assert_eq!(investment_after.amount, investment_before.amount);
    assert_eq!(investment_after.status, InvestmentStatus::Active);

    assert_eq!(token_client.balance(&business), business_before);
    assert_eq!(token_client.balance(&investor), investor_before);
    assert_eq!(token_client.balance(&contract_id), platform_before);
    assert_eq!(env.events().all().events().len(), events_before);
}

/// Exact remaining-due settlement should preserve accounting totals and emit exact-value events.
#[test]
fn test_settle_invoice_exact_remaining_due_preserves_totals_and_emits_final_events() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let business = Address::generate(&env);
    let investor = Address::generate(&env);
    let currency = init_currency_for_test(&env, &contract_id, &business, &investor);
    let invoice_amount = 1_000i128;
    let investment_amount = 900i128;
    let invoice_id = setup_funded_invoice(
        &env,
        &client,
        &business,
        &investor,
        &currency,
        invoice_amount,
        investment_amount,
    );

    env.ledger().set_timestamp(4_000);
    client.process_partial_payment(&invoice_id, &400, &String::from_str(&env, "prepay-2"));

    let token_client = token::Client::new(&env, &currency);
    let business_before = token_client.balance(&business);
    let investor_before = token_client.balance(&investor);
    let platform_before = token_client.balance(&contract_id);

    env.ledger().set_timestamp(4_500);
    client.settle_invoice(&invoice_id, &600);

    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.total_paid, invoice_amount);
    assert_eq!(invoice.status, InvoiceStatus::Paid);

    let investment = client.get_invoice_investment(&invoice_id);
    assert_eq!(investment.status, InvestmentStatus::Completed);

    let (expected_investor_return, expected_platform_fee) =
        calculate_profit(&env, investment_amount, invoice_amount);
    assert_eq!(
        token_client.balance(&business),
        business_before - invoice_amount
    );
    assert_eq!(
        token_client.balance(&investor) - investor_before,
        expected_investor_return,
    );
    assert_eq!(
        token_client.balance(&contract_id) - platform_before,
        expected_platform_fee,
    );

    assert!(
        has_event_with_topic(&env, symbol_short!("pay_rec")),
        "expected payment-recorded event for the exact remaining due",
    );
    assert!(
        has_event_with_topic(&env, symbol_short!("inv_stlf")),
        "expected final settlement event after exact settlement",
    );
}
