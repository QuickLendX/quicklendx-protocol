use super::*;
use crate::alloc::string::ToString;
use crate::errors::QuickLendXError;
use crate::investment::{InvestmentStatus, InvestmentStorage};
use crate::invoice::InvoiceCategory;
use crate::payments::{EscrowStatus, EscrowStorage};
use crate::types::InvoiceStatus;
use crate::events::TOPIC_INVESTMENT_WITHDRAWN;
use soroban_sdk::{
    testutils::{Address as _, Events, Ledger},
    token, xdr, Address, BytesN, Env, String, Symbol, TryFromVal, Val, Vec,
};

fn setup_env() -> (Env, QuickLendXContractClient<'static>, Address, Address, Address, Address) {
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
    );
    client.verify_invoice(&invoice_id);

    let bid_id = client.place_bid(investor, &invoice_id, &bid_amount, &(bid_amount + 100));
    client.accept_bid(&invoice_id, &bid_id);

    invoice_id
}

fn get_investment(
    env: &Env,
    contract_id: &Address,
    invoice_id: &BytesN<32>,
) -> crate::types::Investment {
    env.as_contract(contract_id, || {
        InvestmentStorage::get_investment_by_invoice(env, invoice_id)
            .expect("Investment should exist")
    })
}

fn is_in_active_index(
    env: &Env,
    contract_id: &Address,
    investment_id: &BytesN<32>,
) -> bool {
    env.as_contract(contract_id, || {
        InvestmentStorage::get_active_investment_ids(env)
            .iter()
            .any(|id| id == *investment_id)
    })
}

/// Test: Investor successfully withdraws an active investment
#[test]
fn test_withdrawal_success() {
    let (env, client, contract_id, admin, business, investor) = setup_env();
    let currency = make_token(&env, &contract_id, &business, &investor);

    let invoice_id = setup_funded_investment(
        &env, &client, &admin, &business, &investor, &currency, 1000, 1000,
    );

    let investment = get_investment(&env, &contract_id, &invoice_id);
    assert_eq!(investment.status, InvestmentStatus::Active);
    assert!(is_in_active_index(&env, &contract_id, &investment.investment_id));

    client.withdraw_investment(&invoice_id, &investor);

    let withdrawn = get_investment(&env, &contract_id, &invoice_id);
    assert_eq!(withdrawn.status, InvestmentStatus::Withdrawn);
    assert!(!is_in_active_index(&env, &contract_id, &investment.investment_id));

    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Verified);
    assert_eq!(invoice.funded_amount, 0);
    assert!(invoice.funded_at.is_none());
    assert!(invoice.investor.is_none());

    let escrow = env.as_contract(&contract_id, || {
        EscrowStorage::get_escrow_by_invoice(&env, &invoice_id).unwrap()
    });
    assert_eq!(escrow.status, EscrowStatus::Refunded);

    assert!(env.as_contract(&contract_id, || {
        InvestmentStorage::validate_no_orphan_investments(&env)
    }));
}

/// Test: Withdrawal after settlement (already Completed) is rejected
#[test]
fn test_withdraw_after_settlement_rejected() {
    let (env, client, contract_id, admin, business, investor) = setup_env();
    let currency = make_token(&env, &contract_id, &business, &investor);

    let invoice_id = setup_funded_investment(
        &env, &client, &admin, &business, &investor, &currency, 1000, 1000,
    );

    client.settle_invoice(&invoice_id, &1000);

    let err = client.try_withdraw_investment(&invoice_id, &investor).unwrap_err().unwrap();
    assert_eq!(err, QuickLendXError::InvalidStatus);
}

/// Test: Withdrawal by non-investor is rejected
#[test]
fn test_withdraw_by_non_investor_rejected() {
    let (env, client, contract_id, admin, business, investor) = setup_env();
    let currency = make_token(&env, &contract_id, &business, &investor);

    let invoice_id = setup_funded_investment(
        &env, &client, &admin, &business, &investor, &currency, 1000, 1000,
    );

    let stranger = Address::generate(&env);

    let err = client.try_withdraw_investment(&invoice_id, &stranger).unwrap_err().unwrap();
    assert_eq!(err, QuickLendXError::Unauthorized);
}

/// Test: Double withdrawal is rejected (investment already terminal)
#[test]
fn test_double_withdraw_rejected() {
    let (env, client, contract_id, admin, business, investor) = setup_env();
    let currency = make_token(&env, &contract_id, &business, &investor);

    let invoice_id = setup_funded_investment(
        &env, &client, &admin, &business, &investor, &currency, 1000, 1000,
    );

    client.withdraw_investment(&invoice_id, &investor);

    let err = client.try_withdraw_investment(&invoice_id, &investor).unwrap_err().unwrap();
    assert_eq!(err, QuickLendXError::InvalidStatus);
}

/// Test: Withdrawal when escrow already released is rejected
#[test]
fn test_withdraw_after_escrow_released_rejected() {
    let (env, client, contract_id, admin, business, investor) = setup_env();
    let currency = make_token(&env, &contract_id, &business, &investor);

    let invoice_id = setup_funded_investment(
        &env, &client, &admin, &business, &investor, &currency, 1000, 1000,
    );

    client.release_escrow_funds(&invoice_id);

    let err = client.try_withdraw_investment(&invoice_id, &investor).unwrap_err().unwrap();
    assert_eq!(err, QuickLendXError::InvalidStatus);
}

/// Test: Withdrawal when escrow already refunded is rejected
#[test]
fn test_withdraw_after_escrow_refunded_rejected() {
    let (env, client, contract_id, admin, business, investor) = setup_env();
    let currency = make_token(&env, &contract_id, &business, &investor);

    let invoice_id = setup_funded_investment(
        &env, &client, &admin, &business, &investor, &currency, 1000, 1000,
    );

    client.refund_escrow_funds(&invoice_id, &business);

    let err = client.try_withdraw_investment(&invoice_id, &investor).unwrap_err().unwrap();
    assert_eq!(err, QuickLendXError::InvalidStatus);
}

fn count_events_with_topic(env: &Env, topic_str: &str) -> usize {
    let topic_sym = Symbol::new(env, topic_str);
    let topic_xdr = xdr::ScVal::try_from_val(env, &topic_sym).expect("topic to ScVal");
    env.events()
        .all()
        .events()
        .iter()
        .filter(|e| match &e.body {
            xdr::ContractEventBody::V0(body) => body.topics.first() == Some(&topic_xdr),
        })
        .count()
}

/// Test: Event emission for successful withdrawal
#[test]
fn test_withdrawal_emits_events() {
    let (env, client, contract_id, admin, business, investor) = setup_env();
    let currency = make_token(&env, &contract_id, &business, &investor);

    let invoice_id = setup_funded_investment(
        &env, &client, &admin, &business, &investor, &currency, 1000, 1000,
    );

    let before = count_events_with_topic(&env, TOPIC_INVESTMENT_WITHDRAWN);
    client.withdraw_investment(&invoice_id, &investor);
    let after = count_events_with_topic(&env, TOPIC_INVESTMENT_WITHDRAWN);
    assert_eq!(after, before + 1, "withdrawal should emit exactly one InvestmentWithdrawn event");
}

/// Test: Withdrawn investment is immutable (no further transitions)
#[test]
fn test_withdrawn_immutable() {
    let result = InvestmentStatus::validate_transition(
        &InvestmentStatus::Withdrawn,
        &InvestmentStatus::Active,
    );
    assert!(result.is_err());

    let result = InvestmentStatus::validate_transition(
        &InvestmentStatus::Withdrawn,
        &InvestmentStatus::Completed,
    );
    assert!(result.is_err());

    let result = InvestmentStatus::validate_transition(
        &InvestmentStatus::Withdrawn,
        &InvestmentStatus::Defaulted,
    );
    assert!(result.is_err());

    let result = InvestmentStatus::validate_transition(
        &InvestmentStatus::Withdrawn,
        &InvestmentStatus::Refunded,
    );
    assert!(result.is_err());
}

/// Test: Active index does not contain withdrawn investments
#[test]
fn test_withdrawn_not_in_active_index() {
    let (env, client, contract_id, admin, business, investor) = setup_env();
    let currency = make_token(&env, &contract_id, &business, &investor);

    let invoice_id = setup_funded_investment(
        &env, &client, &admin, &business, &investor, &currency, 1000, 1000,
    );

    let investment = get_investment(&env, &contract_id, &invoice_id);
    client.withdraw_investment(&invoice_id, &investor);

    env.as_contract(&contract_id, || {
        let active_ids = InvestmentStorage::get_active_investment_ids(&env);
        for id in active_ids.iter() {
            let inv = InvestmentStorage::get_investment(&env, &id).unwrap();
            assert_eq!(inv.status, InvestmentStatus::Active);
        }
    });
    assert!(!is_in_active_index(&env, &contract_id, &investment.investment_id));
    assert!(env.as_contract(&contract_id, || {
        InvestmentStorage::validate_no_orphan_investments(&env)
    }));
}

/// Test: Reentrant withdrawal inside payment guard is rejected
#[test]
fn test_withdraw_reentrant_rejected() {
    let (env, client, contract_id, admin, business, investor) = setup_env();
    let currency = make_token(&env, &contract_id, &business, &investor);

    let invoice_id = setup_funded_investment(
        &env, &client, &admin, &business, &investor, &currency, 1000, 1000,
    );

    let guard_key = soroban_sdk::symbol_short!("pay_lock");
    env.as_contract(&contract_id, || {
        env.storage().instance().set(&guard_key, &true);
    });

    let err = client.try_withdraw_investment(&invoice_id, &investor).unwrap_err().unwrap();
    assert_eq!(err, QuickLendXError::OperationNotAllowed);

    env.as_contract(&contract_id, || {
        env.storage().instance().set(&guard_key, &false);
    });
}

/// Test: Investor can invest in a new invoice after withdrawal
#[test]
fn test_investor_can_invest_again_after_withdrawal() {
    let (env, client, contract_id, admin, business, investor) = setup_env();
    let currency = make_token(&env, &contract_id, &business, &investor);

    let invoice_id = setup_funded_investment(
        &env, &client, &admin, &business, &investor, &currency, 1000, 1000,
    );

    client.withdraw_investment(&invoice_id, &investor);

    let due_date = env.ledger().timestamp() + 86_400;
    let invoice2_id = client.upload_invoice(
        &business,
        &2000,
        &currency,
        &due_date,
        &String::from_str(&env, "Second invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    client.verify_invoice(&invoice2_id);

    let bid2_id = client.place_bid(&investor, &invoice2_id, &2000, &2100);
    client.accept_bid(&invoice2_id, &bid2_id);

    let investment2 = get_investment(&env, &contract_id, &invoice2_id);
    assert_eq!(investment2.status, InvestmentStatus::Active);
}
