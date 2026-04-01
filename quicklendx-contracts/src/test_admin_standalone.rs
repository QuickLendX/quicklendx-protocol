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

    println!("🔧 Testing Hardened Admin Implementation");
    println!("Admin1: {:?}", admin1);
    println!("Admin2: {:?}", admin2);
    println!("Unauthorized: {:?}", unauthorized);

    // ========================================
    // 1. Test Secure One-Time Initialization
    // ========================================
    println!("\n📋 Phase 1: Testing Secure One-Time Initialization");

    // Initially no admin should exist
    assert!(!AdminStorage::is_initialized(&env));
    assert_eq!(AdminStorage::get_admin(&env), None);
    println!("✅ Initial state: No admin exists");

    // Initialize admin (requires admin's authorization)
    let result = AdminStorage::initialize(&env, &admin1);
    assert!(result.is_ok());
    println!("✅ Admin initialized successfully");

    // Verify admin is set
    assert!(AdminStorage::is_initialized(&env));
    assert_eq!(AdminStorage::get_admin(&env), Some(admin1.clone()));
    assert!(AdminStorage::is_admin(&env, &admin1));
    assert!(!AdminStorage::is_admin(&env, &admin2));
    println!("✅ Admin verification works correctly");

    // Test one-time initialization protection
    let result = AdminStorage::initialize(&env, &admin2);
    assert_eq!(result, Err(QuickLendXError::OperationNotAllowed));
    println!("✅ Double initialization prevented");

    // ========================================
    // 2. Test Protocol Initialization
    // ========================================
    println!("\n📋 Phase 2: Testing Protocol Initialization");

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
    println!("✅ Protocol initialized successfully");

    // Verify protocol configuration
    let config = ProtocolInitializer::get_protocol_config(&env).unwrap();
    assert_eq!(config.min_invoice_amount, 1000);
    assert_eq!(config.max_due_date_days, 365);
    assert_eq!(config.grace_period_seconds, 86400);
    println!("✅ Protocol configuration verified");

    // ========================================
    // 3. Test Authorization Framework
    // ========================================
    println!("\n📋 Phase 3: Testing Authorization Framework");

    // Test admin authorization works
    let result = AdminStorage::require_admin(&env, &admin1);
    assert!(result.is_ok());
    println!("✅ Admin authorization check passed");

    // Test unauthorized access is blocked
    let result = AdminStorage::require_admin(&env, &unauthorized);
    assert_eq!(result, Err(QuickLendXError::NotAdmin));
    println!("✅ Unauthorized access blocked");

    // Test current admin helper
    let result = AdminStorage::require_current_admin(&env);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), admin1);
    println!("✅ Current admin helper works");

    // ========================================
    // 4. Test Authenticated Admin Transfer
    // ========================================
    println!("\n📋 Phase 4: Testing Authenticated Admin Transfer");

    // Test unauthorized transfer is blocked
    let result = AdminStorage::transfer_admin(&env, &unauthorized, &admin2);
    assert_eq!(result, Err(QuickLendXError::NotAdmin));
    println!("✅ Unauthorized transfer blocked");

    // Test self-transfer is blocked
    let result = AdminStorage::transfer_admin(&env, &admin1, &admin1);
    assert_eq!(result, Err(QuickLendXError::OperationNotAllowed));
    println!("✅ Self-transfer blocked");

    // Test successful admin transfer
    let result = AdminStorage::transfer_admin(&env, &admin1, &admin2);
    assert!(result.is_ok());
    println!("✅ Admin transfer successful");

    // Verify new admin is active
    assert_eq!(AdminStorage::get_admin(&env), Some(admin2.clone()));
    assert!(!AdminStorage::is_admin(&env, &admin1));
    assert!(AdminStorage::is_admin(&env, &admin2));
    println!("✅ New admin verified");

    // Test old admin can no longer perform admin operations
    let result = AdminStorage::require_admin(&env, &admin1);
    assert_eq!(result, Err(QuickLendXError::NotAdmin));
    println!("✅ Old admin access revoked");

    // ========================================
    // 5. Test Admin-Protected Operations
    // ========================================
    println!("\n📋 Phase 5: Testing Admin-Protected Operations");

    // Test protocol configuration update (admin-only)
    let result = ProtocolInitializer::set_fee_config(&env, &admin2, 300);
    assert!(result.is_ok());
    assert_eq!(ProtocolInitializer::get_fee_bps(&env), 300);
    println!("✅ Admin-only fee config update successful");

    // Test unauthorized config update is blocked
    let result = ProtocolInitializer::set_fee_config(&env, &admin1, 400);
    assert_eq!(result, Err(QuickLendXError::NotAdmin));
    println!("✅ Unauthorized config update blocked");

    // Test treasury update
    let new_treasury = Address::generate(&env);
    let result = ProtocolInitializer::set_treasury(&env, &admin2, &new_treasury);
    assert!(result.is_ok());
    assert_eq!(ProtocolInitializer::get_treasury(&env), Some(new_treasury));
    println!("✅ Treasury update successful");

    // ========================================
    // 6. Test Authorization Wrapper Functions
    // ========================================
    println!("\n📋 Phase 6: Testing Authorization Wrappers");

    // Test with_admin_auth wrapper
    let mut operation_executed = false;
    let result = AdminStorage::with_admin_auth(&env, &admin2, || {
        operation_executed = true;
        Ok(42)
    });
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), 42);
    assert!(operation_executed);
    println!("✅ Admin auth wrapper works");

    // Test with_current_admin wrapper
    let mut admin_received = None;
    let result = AdminStorage::with_current_admin(&env, |admin| {
        admin_received = Some(admin.clone());
        Ok("success")
    });
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "success");
    assert_eq!(admin_received, Some(admin2.clone()));
    println!("✅ Current admin wrapper works");

    // ========================================
    // 7. Test Legacy Compatibility
    // ========================================
    println!("\n📋 Phase 7: Testing Legacy Compatibility");

    // Transfer admin using legacy function
    let admin3 = Address::generate(&env);
    let result = AdminStorage::set_admin(&env, &admin3);
    assert!(result.is_ok());
    assert_eq!(AdminStorage::get_admin(&env), Some(admin3));
    println!("✅ Legacy set_admin function works");

    println!("\n🎉 All Hardened Admin Implementation Tests Passed!");
    println!("✅ Secure one-time initialization");
    println!("✅ Authenticated admin transfers");
    println!("✅ Proper authorization checks");
    println!("✅ Protection against unauthorized operations");
    println!("✅ Admin-protected configuration updates");
    println!("✅ Authorization wrapper functions");
    println!("✅ Legacy compatibility maintained");
}

/// Test demonstrating security protections work under various attack scenarios
#[test]
fn test_security_attack_scenarios() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let attacker = Address::generate(&env);
    let treasury = Address::generate(&env);

    println!("🛡️ Testing Security Attack Scenarios");

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
    println!("\n🔴 Attack 1: Reinitialize admin");
    let result = AdminStorage::initialize(&env, &attacker);
    assert_eq!(result, Err(QuickLendXError::OperationNotAllowed));
    println!("✅ Attack blocked: Cannot reinitialize admin");

    // Attack Scenario 2: Attacker tries to transfer admin without authorization
    println!("\n🔴 Attack 2: Unauthorized admin transfer");
    let result = AdminStorage::transfer_admin(&env, &attacker, &attacker);
    assert_eq!(result, Err(QuickLendXError::NotAdmin));
    println!("✅ Attack blocked: Unauthorized transfer rejected");

    // Attack Scenario 3: Attacker tries to modify protocol configuration
    println!("\n🔴 Attack 3: Unauthorized config modification");
    let result = ProtocolInitializer::set_fee_config(&env, &attacker, 9999);
    assert_eq!(result, Err(QuickLendXError::NotAdmin));
    println!("✅ Attack blocked: Unauthorized config change rejected");

    // Attack Scenario 4: Attacker tries to change treasury
    println!("\n🔴 Attack 4: Unauthorized treasury change");
    let result = ProtocolInitializer::set_treasury(&env, &attacker, &attacker);
    assert_eq!(result, Err(QuickLendXError::NotAdmin));
    println!("✅ Attack blocked: Unauthorized treasury change rejected");

    // Verify system integrity after attacks
    assert_eq!(AdminStorage::get_admin(&env), Some(admin));
    assert_eq!(ProtocolInitializer::get_fee_bps(&env), 250);
    assert_eq!(ProtocolInitializer::get_treasury(&env), Some(treasury));
    println!("✅ System integrity maintained after all attacks");

    println!("\n🛡️ All Security Attack Scenarios Successfully Blocked!");
}

/// Test demonstrating parameter validation works correctly
#[test]
fn test_parameter_validation() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);

    println!("🔍 Testing Parameter Validation");

    // Initialize admin
    AdminStorage::initialize(&env, &admin).unwrap();

    // Test invalid fee basis points (> 1000 = 10%)
    println!("\n🔴 Testing invalid fee basis points");
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
    println!("✅ Invalid fee basis points rejected");

    // Test invalid min invoice amount (zero)
    println!("\n🔴 Testing invalid min invoice amount");
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
    println!("✅ Invalid min invoice amount rejected");

    // Test invalid max due date (too long)
    println!("\n🔴 Testing invalid max due date");
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
    println!("✅ Invalid max due date rejected");

    // Test invalid grace period (too long)
    println!("\n🔴 Testing invalid grace period");
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
    println!("✅ Invalid grace period rejected");

    // Test valid parameters work
    println!("\n✅ Testing valid parameters");
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
    println!("✅ Valid parameters accepted");

    println!("\n🔍 All Parameter Validation Tests Passed!");
}
