#![cfg(test)]

use super::*;
use crate::init::InitializationParams;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env, IntoVal, Vec};

fn setup() -> (Env, QuickLendXContractClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    (env, client)
}

#[test]
fn test_successful_initialization() {
    let (env, client) = setup();

    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    let initial_currencies = Vec::from_array(&env, [Address::generate(&env)]);

    let params = InitializationParams {
        admin: admin.clone(),
        treasury: treasury.clone(),
        fee_bps: 200,
        min_invoice_amount: 1_000_000,
        max_due_date_days: 365,
        grace_period_seconds: 604800,
        initial_currencies: initial_currencies.clone(),
    };

    client.initialize(&params);

    assert!(client.is_initialized());
}

#[test]
fn test_double_initialization_fails() {
    let (env, client) = setup();

    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);

    let params = InitializationParams {
        admin: admin.clone(),
        treasury: treasury.clone(),
        fee_bps: 200,
        min_invoice_amount: 1_000_000,
        max_due_date_days: 365,
        grace_period_seconds: 604800,
        initial_currencies: Vec::new(&env),
    };

    client.initialize(&params);

    // Second initialization should fail
    let result = client.try_initialize(&params);
    assert_eq!(result, Err(Ok(QuickLendXError::OperationNotAllowed)));
}

#[test]
#[should_panic(expected = "HostError: Error(Auth, InvalidAction)")]
fn test_initialization_requires_admin_auth() {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    // No mock_all_auths()
    let admin = Address::generate(&env);
    let params = InitializationParams {
        admin: admin.clone(),
        treasury: Address::generate(&env),
        fee_bps: 200,
        min_invoice_amount: 1_000_000,
        max_due_date_days: 365,
        grace_period_seconds: 604800,
        initial_currencies: Vec::new(&env),
    };

    client.initialize(&params);
}

#[test]
fn test_validation_invalid_fees() {
    let (env, client) = setup();
    let admin = Address::generate(&env);

    let params = InitializationParams {
        admin: admin.clone(),
        treasury: Address::generate(&env),
        fee_bps: 1001, // 10.01% > 10%
        min_invoice_amount: 1_000_000,
        max_due_date_days: 365,
        grace_period_seconds: 604800,
        initial_currencies: Vec::new(&env),
    };

    let result = client.try_initialize(&params);
    assert_eq!(result, Err(Ok(QuickLendXError::InvalidFeeBasisPoints)));
}

#[test]
fn test_validation_invalid_amount() {
    let (env, client) = setup();
    let admin = Address::generate(&env);

    let params = InitializationParams {
        admin: admin.clone(),
        treasury: Address::generate(&env),
        fee_bps: 200,
        min_invoice_amount: 0, // Should be > 0
        max_due_date_days: 365,
        grace_period_seconds: 604800,
        initial_currencies: Vec::new(&env),
    };

    let result = client.try_initialize(&params);
    assert_eq!(result, Err(Ok(QuickLendXError::InvalidAmount)));
}

#[test]
fn test_validation_invalid_due_date() {
    let (env, client) = setup();
    let admin = Address::generate(&env);

    let params = InitializationParams {
        admin: admin.clone(),
        treasury: Address::generate(&env),
        fee_bps: 200,
        min_invoice_amount: 1_000_000,
        max_due_date_days: 731, // Max 730
        grace_period_seconds: 604800,
        initial_currencies: Vec::new(&env),
    };

    let result = client.try_initialize(&params);
    assert_eq!(result, Err(Ok(QuickLendXError::InvoiceDueDateInvalid)));
}

#[test]
fn test_validation_invalid_grace_period() {
    let (env, client) = setup();
    let admin = Address::generate(&env);

    let params = InitializationParams {
        admin: admin.clone(),
        treasury: Address::generate(&env),
        fee_bps: 200,
        min_invoice_amount: 1_000_000,
        max_due_date_days: 365,
        grace_period_seconds: 2_592_001, // Max 30 days
        initial_currencies: Vec::new(&env),
    };

    let result = client.try_initialize(&params);
    assert_eq!(result, Err(Ok(QuickLendXError::InvalidTimestamp)));
}

#[test]
fn test_get_version_before_initialization() {
    let (env, client) = setup();
    
    // Before initialization, should return the current PROTOCOL_VERSION constant
    let version = client.get_version();
    assert_eq!(version, 1);
    
    // Contract should not be initialized yet
    assert!(!client.is_initialized());
}

#[test]
fn test_get_version_after_initialization() {
    let (env, client) = setup();
    
    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    let initial_currencies = Vec::from_array(&env, [Address::generate(&env)]);

    let params = InitializationParams {
        admin: admin.clone(),
        treasury: treasury.clone(),
        fee_bps: 200,
        min_invoice_amount: 1_000_000,
        max_due_date_days: 365,
        grace_period_seconds: 604800,
        initial_currencies: initial_currencies.clone(),
    };

    // Before initialization
    let version_before = client.get_version();
    assert_eq!(version_before, 1);
    
    // Initialize the contract
    client.initialize(&params);
    
    // After initialization, version should still be the same
    let version_after = client.get_version();
    assert_eq!(version_after, 1);
    
    // Contract should now be initialized
    assert!(client.is_initialized());
    
    // Version should be consistent before and after initialization
    assert_eq!(version_before, version_after);
}

#[test]
fn test_version_immutability() {
    let (env, client) = setup();
    
    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    let initial_currencies = Vec::from_array(&env, [Address::generate(&env)]);

    let params = InitializationParams {
        admin: admin.clone(),
        treasury: treasury.clone(),
        fee_bps: 200,
        min_invoice_amount: 1_000_000,
        max_due_date_days: 365,
        grace_period_seconds: 604800,
        initial_currencies: initial_currencies.clone(),
    };

    // Initialize the contract
    client.initialize(&params);
    
    // Get version multiple times - should always return the same value
    let version1 = client.get_version();
    let version2 = client.get_version();
    let version3 = client.get_version();
    
    assert_eq!(version1, 1);
    assert_eq!(version2, 1);
    assert_eq!(version3, 1);
    
    // Version should remain constant across multiple calls
    assert_eq!(version1, version2);
    assert_eq!(version2, version3);
}

#[test]
fn test_version_format_documentation() {
    let (env, client) = setup();
    
    // Test that version follows the documented format (simple integer)
    let version = client.get_version();
    
    // Version should be a positive integer
    assert!(version > 0);
    assert!(version <= u32::MAX);
    
    // Current version should be 1 based on PROTOCOL_VERSION constant
    assert_eq!(version, 1);
    
    // Verify it's a simple integer format (not semver or complex format)
    let version_str = version.to_string();
    assert!(version_str.parse::<u32>().is_ok());
}

#[test]
fn test_version_consistency_across_operations() {
    let (env, client) = setup();
    
    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    let initial_currencies = Vec::from_array(&env, [Address::generate(&env)]);

    let params = InitializationParams {
        admin: admin.clone(),
        treasury: treasury.clone(),
        fee_bps: 200,
        min_invoice_amount: 1_000_000,
        max_due_date_days: 365,
        grace_period_seconds: 604800,
        initial_currencies: initial_currencies.clone(),
    };

    // Get initial version
    let initial_version = client.get_version();
    
    // Initialize
    client.initialize(&params);
    
    // Perform various operations
    let current_admin = client.get_current_admin().unwrap();
    client.transfer_admin(&current_admin, &Address::generate(&env));
    
    // Add currency
    let new_currency = Address::generate(&env);
    client.add_currency(&current_admin, &new_currency);
    
    // Version should remain unchanged throughout all operations
    let final_version = client.get_version();
    assert_eq!(initial_version, final_version);
    assert_eq!(final_version, 1);
}

#[test]
fn test_version_edge_cases() {
    let (env, client) = setup();
    
    // Test version behavior in edge cases
    
    // 1. Fresh contract instance
    let version1 = client.get_version();
    assert_eq!(version1, 1);
    
    // 2. After failed initialization attempt
    let admin = Address::generate(&env);
    let invalid_params = InitializationParams {
        admin: admin.clone(),
        treasury: Address::generate(&env),
        fee_bps: 1001, // Invalid - should fail
        min_invoice_amount: 1_000_000,
        max_due_date_days: 365,
        grace_period_seconds: 604800,
        initial_currencies: Vec::new(&env),
    };
    
    // This should fail
    let result = client.try_initialize(&invalid_params);
    assert!(result.is_err());
    
    // Version should still be accessible and unchanged
    let version2 = client.get_version();
    assert_eq!(version2, 1);
    assert_eq!(version1, version2);
    
    // 3. After successful initialization
    let valid_params = InitializationParams {
        admin: admin.clone(),
        treasury: Address::generate(&env),
        fee_bps: 200,
        min_invoice_amount: 1_000_000,
        max_due_date_days: 365,
        grace_period_seconds: 604800,
        initial_currencies: Vec::new(&env),
    };
    
    client.initialize(&valid_params);
    let version3 = client.get_version();
    assert_eq!(version3, 1);
    assert_eq!(version1, version3);
}
