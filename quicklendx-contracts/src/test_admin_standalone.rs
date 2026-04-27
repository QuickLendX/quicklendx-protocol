#![cfg(test)]

use soroban_sdk::{testutils::Address as _, Address, Env};

use crate::{
    admin::AdminStorage,
    init::{InitializationParams, ProtocolInitializer},
    QuickLendXError,
};

/// Standalone test demonstrating the hardened admin implementation
/// This test shows:
/// 1. Secure one-time initialization
/// 2. Authenticated admin transfers
/// 3. Proper authorization checks
/// 4. Protection against unauthorized operations
#[test]
fn test_hardened_admin_implementation_standalone() {
    let env = Env::default();
    env.mock_all_auths();

    // Generate test addresses
    let admin1 = Address::generate(&env);
    let admin2 = Address::generate(&env);
    let unauthorized = Address::generate(&env);
    let treasury = Address::generate(&env);


    // ========================================
    // 1. Test Secure One-Time Initialization
    // ========================================

    // Initially no admin should exist
    assert!(!AdminStorage::is_initialized(&env));
    assert_eq!(AdminStorage::get_admin(&env), None);

    // Initialize admin (requires admin's authorization)
    let result = AdminStorage::initialize(&env, &admin1);
    assert!(result.is_ok());

    // Verify admin is set
    assert!(AdminStorage::is_initialized(&env));
    assert_eq!(AdminStorage::get_admin(&env), Some(admin1.clone()));
    assert!(AdminStorage::is_admin(&env, &admin1));
    assert!(!AdminStorage::is_admin(&env, &admin2));

    // Test one-time initialization protection
    let result = AdminStorage::initialize(&env, &admin2);
    assert_eq!(result, Err(QuickLendXError::OperationNotAllowed));

    // ========================================
    // 2. Test Protocol Initialization
    // ========================================

    let init_params = InitializationParams {
        admin: admin1.clone(),
        treasury: treasury.clone(),
        fee_bps: 250, // 2.5%
        min_invoice_amount: 1000,
        max_due_date_days: 365,
        grace_period_seconds: 86400, // 1 day
    };

    let result = ProtocolInitializer::initialize(&env, &init_params);
    assert!(result.is_ok());

    // Verify protocol configuration
    let config = ProtocolInitializer::get_protocol_config(&env).unwrap();
    assert_eq!(config.min_invoice_amount, 1000);
    assert_eq!(config.max_due_date_days, 365);
    assert_eq!(config.grace_period_seconds, 86400);

    // ========================================
    // 3. Test Authorization Framework
    // ========================================

    // Test admin authorization works
    let result = AdminStorage::require_admin(&env, &admin1);
    assert!(result.is_ok());

    // Test unauthorized access is blocked
    let result = AdminStorage::require_admin(&env, &unauthorized);
    assert_eq!(result, Err(QuickLendXError::NotAdmin));

    // Test current admin helper
    let result = AdminStorage::require_current_admin(&env);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), admin1);

    // ========================================
    // 4. Test Authenticated Admin Transfer
    // ========================================

    // Test unauthorized transfer is blocked
    let result = AdminStorage::transfer_admin(&env, &unauthorized, &admin2);
    assert_eq!(result, Err(QuickLendXError::NotAdmin));

    // Test self-transfer is blocked
    let result = AdminStorage::transfer_admin(&env, &admin1, &admin1);
    assert_eq!(result, Err(QuickLendXError::OperationNotAllowed));

    // Test successful admin transfer
    let result = AdminStorage::transfer_admin(&env, &admin1, &admin2);
    assert!(result.is_ok());

    // Verify new admin is active
    assert_eq!(AdminStorage::get_admin(&env), Some(admin2.clone()));
    assert!(!AdminStorage::is_admin(&env, &admin1));
    assert!(AdminStorage::is_admin(&env, &admin2));

    // Test old admin can no longer perform admin operations
    let result = AdminStorage::require_admin(&env, &admin1);
    assert_eq!(result, Err(QuickLendXError::NotAdmin));

    // ========================================
    // 5. Test Admin-Protected Operations
    // ========================================

    // Test protocol configuration update (admin-only)
    let result = ProtocolInitializer::set_fee_config(&env, &admin2, 300);
    assert!(result.is_ok());
    assert_eq!(ProtocolInitializer::get_fee_bps(&env), 300);

    // Test unauthorized config update is blocked
    let result = ProtocolInitializer::set_fee_config(&env, &admin1, 400);
    assert_eq!(result, Err(QuickLendXError::NotAdmin));

    // Test treasury update
    let new_treasury = Address::generate(&env);
    let result = ProtocolInitializer::set_treasury(&env, &admin2, &new_treasury);
    assert!(result.is_ok());
    assert_eq!(ProtocolInitializer::get_treasury(&env), Some(new_treasury));

    // ========================================
    // 6. Test Authorization Wrapper Functions
    // ========================================

    // Test with_admin_auth wrapper
    let mut operation_executed = false;
    let result = AdminStorage::with_admin_auth(&env, &admin2, || {
        operation_executed = true;
        Ok(42)
    });
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), 42);
    assert!(operation_executed);

    // Test with_current_admin wrapper
    let mut admin_received = None;
    let result = AdminStorage::with_current_admin(&env, |admin| {
        admin_received = Some(admin.clone());
        Ok("success")
    });
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "success");
    assert_eq!(admin_received, Some(admin2.clone()));

    // ========================================
    // 7. Test Legacy Compatibility
    // ========================================

    // Transfer admin using legacy function
    let admin3 = Address::generate(&env);
    let result = AdminStorage::set_admin(&env, &admin3);
    assert!(result.is_ok());
    assert_eq!(AdminStorage::get_admin(&env), Some(admin3));

}

/// Test demonstrating security protections work under various attack scenarios
#[test]
fn test_security_attack_scenarios() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let attacker = Address::generate(&env);
    let treasury = Address::generate(&env);


    // Initialize system
    AdminStorage::initialize(&env, &admin).unwrap();
    let init_params = InitializationParams {
        admin: admin.clone(),
        treasury: treasury.clone(),
        fee_bps: 250,
        min_invoice_amount: 1000,
        max_due_date_days: 365,
        grace_period_seconds: 86400,
    };
    ProtocolInitializer::initialize(&env, &init_params).unwrap();

    // Attack Scenario 1: Attacker tries to reinitialize admin
    let result = AdminStorage::initialize(&env, &attacker);
    assert_eq!(result, Err(QuickLendXError::OperationNotAllowed));

    // Attack Scenario 2: Attacker tries to transfer admin without authorization
    let result = AdminStorage::transfer_admin(&env, &attacker, &attacker);
    assert_eq!(result, Err(QuickLendXError::NotAdmin));

    // Attack Scenario 3: Attacker tries to modify protocol configuration
    let result = ProtocolInitializer::set_fee_config(&env, &attacker, 9999);
    assert_eq!(result, Err(QuickLendXError::NotAdmin));

    // Attack Scenario 4: Attacker tries to change treasury
    let result = ProtocolInitializer::set_treasury(&env, &attacker, &attacker);
    assert_eq!(result, Err(QuickLendXError::NotAdmin));

    // Verify system integrity after attacks
    assert_eq!(AdminStorage::get_admin(&env), Some(admin));
    assert_eq!(ProtocolInitializer::get_fee_bps(&env), 250);
    assert_eq!(ProtocolInitializer::get_treasury(&env), Some(treasury));

}

/// Test demonstrating parameter validation works correctly
#[test]
fn test_parameter_validation() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);


    // Initialize admin
    AdminStorage::initialize(&env, &admin).unwrap();

    // Test invalid fee basis points (> 1000 = 10%)
    let init_params = InitializationParams {
        admin: admin.clone(),
        treasury: treasury.clone(),
        fee_bps: 1001, // Invalid: > 10%
        min_invoice_amount: 1000,
        max_due_date_days: 365,
        grace_period_seconds: 86400,
    };
    let result = ProtocolInitializer::initialize(&env, &init_params);
    assert_eq!(result, Err(QuickLendXError::InvalidFeeBasisPoints));

    // Test invalid min invoice amount (zero)
    let init_params = InitializationParams {
        admin: admin.clone(),
        treasury: treasury.clone(),
        fee_bps: 250,
        min_invoice_amount: 0, // Invalid: must be positive
        max_due_date_days: 365,
        grace_period_seconds: 86400,
    };
    let result = ProtocolInitializer::initialize(&env, &init_params);
    assert_eq!(result, Err(QuickLendXError::InvalidAmount));

    // Test invalid max due date (too long)
    let init_params = InitializationParams {
        admin: admin.clone(),
        treasury: treasury.clone(),
        fee_bps: 250,
        min_invoice_amount: 1000,
        max_due_date_days: 731, // Invalid: > 2 years
        grace_period_seconds: 86400,
    };
    let result = ProtocolInitializer::initialize(&env, &init_params);
    assert_eq!(result, Err(QuickLendXError::InvoiceDueDateInvalid));

    // Test invalid grace period (too long)
    let init_params = InitializationParams {
        admin: admin.clone(),
        treasury: treasury.clone(),
        fee_bps: 250,
        min_invoice_amount: 1000,
        max_due_date_days: 365,
        grace_period_seconds: 2_592_001, // Invalid: > 30 days
    };
    let result = ProtocolInitializer::initialize(&env, &init_params);
    assert_eq!(result, Err(QuickLendXError::InvalidTimestamp));

    // Test valid parameters work
    let init_params = InitializationParams {
        admin: admin.clone(),
        treasury: treasury.clone(),
        fee_bps: 250,
        min_invoice_amount: 1000,
        max_due_date_days: 365,
        grace_period_seconds: 86400,
    };
    let result = ProtocolInitializer::initialize(&env, &init_params);
    assert!(result.is_ok());

}
