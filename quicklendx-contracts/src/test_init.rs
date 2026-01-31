use crate::admin::AdminStorage;
use crate::currency::CurrencyWhitelist;
use crate::errors::QuickLendXError;
use crate::init::{InitializationParams, ProtocolInitializer};
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::{Address, Env, Vec};

fn build_params(
    env: &Env,
    admin: &Address,
    treasury: &Address,
    fee_bps: u32,
    min_invoice_amount: i128,
    max_due_date_days: u64,
    grace_period_seconds: u64,
    currencies: &[Address],
) -> InitializationParams {
    let mut initial_currencies = Vec::new(env);
    for currency in currencies.iter() {
        initial_currencies.push_back(currency.clone());
    }

    InitializationParams {
        admin: admin.clone(),
        treasury: treasury.clone(),
        fee_bps,
        min_invoice_amount,
        max_due_date_days,
        grace_period_seconds,
        initial_currencies,
    }
}

#[test]
fn test_protocol_initialize_success() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_700_000_000);

    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    let currency_a = Address::generate(&env);
    let currency_b = Address::generate(&env);

    let params = build_params(
        &env,
        &admin,
        &treasury,
        200,
        1_000_000,
        365,
        86_400,
        &[currency_a.clone(), currency_b.clone()],
    );

    let result = ProtocolInitializer::initialize(&env, &params);
    assert!(result.is_ok(), "Initialization must succeed");

    assert!(ProtocolInitializer::is_initialized(&env));
    assert_eq!(AdminStorage::get_admin(&env), Some(admin.clone()));
    assert_eq!(ProtocolInitializer::get_treasury(&env), Some(treasury.clone()));
    assert_eq!(ProtocolInitializer::get_fee_bps(&env), 200);

    let config = ProtocolInitializer::get_protocol_config(&env).unwrap();
    assert_eq!(config.min_invoice_amount, 1_000_000);
    assert_eq!(config.max_due_date_days, 365);
    assert_eq!(config.grace_period_seconds, 86_400);
    assert_eq!(config.updated_at, env.ledger().timestamp());
    assert_eq!(config.updated_by, admin.clone());

    let whitelist = CurrencyWhitelist::get_whitelisted_currencies(&env);
    assert_eq!(whitelist.len(), 2);
    assert!(whitelist.iter().any(|a| a == currency_a));
    assert!(whitelist.iter().any(|a| a == currency_b));
}

#[test]
fn test_protocol_initialize_twice_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    let params = build_params(&env, &admin, &treasury, 200, 1_000_000, 365, 86_400, &[]);

    assert!(ProtocolInitializer::initialize(&env, &params).is_ok());

    let second_admin = Address::generate(&env);
    let second_params =
        build_params(&env, &second_admin, &treasury, 300, 2_000_000, 180, 43_200, &[]);

    let result = ProtocolInitializer::initialize(&env, &second_params);
    assert_eq!(result, Err(QuickLendXError::OperationNotAllowed));
    assert_eq!(AdminStorage::get_admin(&env), Some(admin));
}

#[test]
fn test_protocol_initialize_rejects_invalid_fee() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    let params = build_params(&env, &admin, &treasury, 1001, 1_000_000, 365, 86_400, &[]);

    let result = ProtocolInitializer::initialize(&env, &params);
    assert_eq!(result, Err(QuickLendXError::InvalidFeeBasisPoints));
    assert!(!ProtocolInitializer::is_initialized(&env));
    assert_eq!(AdminStorage::get_admin(&env), None);
}

#[test]
fn test_protocol_initialize_rejects_invalid_min_amount() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    let params = build_params(&env, &admin, &treasury, 200, 0, 365, 86_400, &[]);

    let result = ProtocolInitializer::initialize(&env, &params);
    assert_eq!(result, Err(QuickLendXError::InvalidAmount));
    assert!(!ProtocolInitializer::is_initialized(&env));
}

#[test]
fn test_protocol_initialize_rejects_invalid_due_date_days() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    let params = build_params(&env, &admin, &treasury, 200, 1_000_000, 0, 86_400, &[]);

    let result = ProtocolInitializer::initialize(&env, &params);
    assert_eq!(result, Err(QuickLendXError::InvoiceDueDateInvalid));
    assert!(!ProtocolInitializer::is_initialized(&env));
}

#[test]
fn test_protocol_initialize_rejects_invalid_grace_period() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    let params = build_params(
        &env,
        &admin,
        &treasury,
        200,
        1_000_000,
        365,
        2_592_001,
        &[],
    );

    let result = ProtocolInitializer::initialize(&env, &params);
    assert_eq!(result, Err(QuickLendXError::InvalidTimestamp));
    assert!(!ProtocolInitializer::is_initialized(&env));
}

#[test]
fn test_protocol_config_updates_require_admin() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_700_000_100);

    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    let params = build_params(&env, &admin, &treasury, 200, 1_000_000, 365, 86_400, &[]);
    ProtocolInitializer::initialize(&env, &params).unwrap();

    let not_admin = Address::generate(&env);
    let result = ProtocolInitializer::set_protocol_config(&env, &not_admin, 2_000_000, 180, 43_200);
    assert_eq!(result, Err(QuickLendXError::NotAdmin));

    let result = ProtocolInitializer::set_protocol_config(&env, &admin, 2_000_000, 180, 43_200);
    assert!(result.is_ok());

    let config = ProtocolInitializer::get_protocol_config(&env).unwrap();
    assert_eq!(config.min_invoice_amount, 2_000_000);
    assert_eq!(config.max_due_date_days, 180);
    assert_eq!(config.grace_period_seconds, 43_200);
    assert_eq!(config.updated_by, admin);
}

#[test]
fn test_fee_and_treasury_updates() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    let params = build_params(&env, &admin, &treasury, 200, 1_000_000, 365, 86_400, &[]);
    ProtocolInitializer::initialize(&env, &params).unwrap();

    let result = ProtocolInitializer::set_fee_config(&env, &admin, 999);
    assert!(result.is_ok());
    assert_eq!(ProtocolInitializer::get_fee_bps(&env), 999);

    let invalid = ProtocolInitializer::set_fee_config(&env, &admin, 1500);
    assert_eq!(invalid, Err(QuickLendXError::InvalidFeeBasisPoints));

    let new_treasury = Address::generate(&env);
    let result = ProtocolInitializer::set_treasury(&env, &admin, &new_treasury);
    assert!(result.is_ok());
    assert_eq!(ProtocolInitializer::get_treasury(&env), Some(new_treasury));
}
