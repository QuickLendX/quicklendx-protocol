#![cfg(test)]

use crate::protocol_limits::{ProtocolLimitsContract, ProtocolLimitsContractClient};
use crate::QuickLendXError;
use soroban_sdk::{testutils::Address as _, Address, Env};

fn setup() -> (Env, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    (env, admin)
}

fn create_client<'a>(env: &'a Env) -> ProtocolLimitsContractClient<'a> {
    let contract_id = env.register(ProtocolLimitsContract, ());
    ProtocolLimitsContractClient::new(env, &contract_id)
}

#[test]
fn test_initialize_success() {
    let (env, admin) = setup();
    let client = create_client(&env);
    client.initialize(&admin);
    let limits = client.get_protocol_limits();
    assert_eq!(limits.min_invoice_amount, 1_000_000);
    assert_eq!(limits.max_due_date_days, 365);
    assert_eq!(limits.grace_period_seconds, 86400);
}

#[test]
fn test_initialize_twice_fails() {
    let (env, admin) = setup();
    let client = create_client(&env);
    client.initialize(&admin);
    let result = client.try_initialize(&admin);
    assert_eq!(result, Err(Ok(QuickLendXError::OperationNotAllowed)));
}

#[test]
fn test_update_success() {
    let (env, admin) = setup();
    let client = create_client(&env);
    client.initialize(&admin);
    client.set_protocol_limits(&admin, &5_000_000, &180, &43200);
    let limits = client.get_protocol_limits();
    assert_eq!(limits.min_invoice_amount, 5_000_000);
    assert_eq!(limits.max_due_date_days, 180);
    assert_eq!(limits.grace_period_seconds, 43200);
}

#[test]
fn test_update_requires_admin() {
    let (env, admin) = setup();
    let client = create_client(&env);
    client.initialize(&admin);
    let non_admin = Address::generate(&env);
    let result = client.try_set_protocol_limits(&non_admin, &5_000_000, &180, &43200);
    assert_eq!(result, Err(Ok(QuickLendXError::Unauthorized)));
}

#[test]
fn test_update_validates_amount_zero() {
    let (env, admin) = setup();
    let client = create_client(&env);
    client.initialize(&admin);
    let result = client.try_set_protocol_limits(&admin, &0, &180, &43200);
    assert_eq!(result, Err(Ok(QuickLendXError::InvalidAmount)));
}

#[test]
fn test_update_validates_amount_negative() {
    let (env, admin) = setup();
    let client = create_client(&env);
    client.initialize(&admin);
    let result = client.try_set_protocol_limits(&admin, &(-1000), &180, &43200);
    assert_eq!(result, Err(Ok(QuickLendXError::InvalidAmount)));
}

#[test]
fn test_update_validates_days_zero() {
    let (env, admin) = setup();
    let client = create_client(&env);
    client.initialize(&admin);
    let result = client.try_set_protocol_limits(&admin, &5_000_000, &0, &43200);
    assert_eq!(result, Err(Ok(QuickLendXError::InvoiceDueDateInvalid)));
}

#[test]
fn test_update_validates_days_boundary() {
    let (env, admin) = setup();
    let client = create_client(&env);
    client.initialize(&admin);
    assert!(client.try_set_protocol_limits(&admin, &5_000_000, &1, &43200).is_ok());
    assert!(client.try_set_protocol_limits(&admin, &5_000_000, &730, &43200).is_ok());
    let result = client.try_set_protocol_limits(&admin, &5_000_000, &731, &43200);
    assert_eq!(result, Err(Ok(QuickLendXError::InvoiceDueDateInvalid)));
}

#[test]
fn test_update_validates_grace_period() {
    let (env, admin) = setup();
    let client = create_client(&env);
    client.initialize(&admin);
    assert!(client.try_set_protocol_limits(&admin, &5_000_000, &180, &0).is_ok());
    assert!(client.try_set_protocol_limits(&admin, &5_000_000, &180, &2_592_000).is_ok());
    let result = client.try_set_protocol_limits(&admin, &5_000_000, &180, &2_592_001);
    assert_eq!(result, Err(Ok(QuickLendXError::InvalidTimestamp)));
}

#[test]
fn test_update_uninitialized_fails() {
    let (env, admin) = setup();
    let client = create_client(&env);
    let result = client.try_set_protocol_limits(&admin, &5_000_000, &180, &43200);
    assert_eq!(result, Err(Ok(QuickLendXError::NotAdmin)));
}

#[test]
fn test_get_limits_before_initialization() {
    let (env, _) = setup();
    let client = create_client(&env);
    let limits = client.get_protocol_limits();
    assert_eq!(limits.min_invoice_amount, 1_000_000);
    assert_eq!(limits.max_due_date_days, 365);
    assert_eq!(limits.grace_period_seconds, 86400);
}

#[test]
fn test_validate_invoice_amount() {
    let (env, admin) = setup();
    let client = create_client(&env);
    client.initialize(&admin);
    client.set_protocol_limits(&admin, &1_000_000, &365, &86400);
    assert!(!client.validate_invoice(&999_999, &(env.ledger().timestamp() + 86400)));
    assert!(client.validate_invoice(&1_000_000, &(env.ledger().timestamp() + 86400)));
    assert!(client.validate_invoice(&5_000_000, &(env.ledger().timestamp() + 86400)));
}

#[test]
fn test_validate_invoice_due_date() {
    let (env, admin) = setup();
    let client = create_client(&env);
    client.initialize(&admin);
    client.set_protocol_limits(&admin, &1_000_000, &365, &86400);
    let current_time = env.ledger().timestamp();
    assert!(!client.validate_invoice(&1_000_000, &(current_time + 366 * 86400)));
    assert!(client.validate_invoice(&1_000_000, &(current_time + 365 * 86400)));
    assert!(client.validate_invoice(&1_000_000, &(current_time + 30 * 86400)));
}

#[test]
fn test_get_default_date() {
    let (env, admin) = setup();
    let client = create_client(&env);
    client.initialize(&admin);
    client.set_protocol_limits(&admin, &1_000_000, &365, &86400);
    assert_eq!(client.get_default_date(&1_000_000), 1_086_400);
    client.set_protocol_limits(&admin, &1_000_000, &365, &0);
    assert_eq!(client.get_default_date(&1_000_000), 1_000_000);
}

#[test]
fn test_limits_persist() {
    let (env, admin) = setup();
    let client = create_client(&env);
    client.initialize(&admin);
    client.set_protocol_limits(&admin, &7_500_000, &270, &129600);
    let limits1 = client.get_protocol_limits();
    let limits2 = client.get_protocol_limits();
    assert_eq!(limits1.min_invoice_amount, limits2.min_invoice_amount);
    assert_eq!(limits1.max_due_date_days, limits2.max_due_date_days);
    assert_eq!(limits1.grace_period_seconds, limits2.grace_period_seconds);
}
