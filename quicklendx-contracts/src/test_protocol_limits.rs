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
    assert!(client
        .try_set_protocol_limits(&admin, &5_000_000, &1, &43200)
        .is_ok());
    assert!(client
        .try_set_protocol_limits(&admin, &5_000_000, &730, &43200)
        .is_ok());
    let result = client.try_set_protocol_limits(&admin, &5_000_000, &731, &43200);
    assert_eq!(result, Err(Ok(QuickLendXError::InvoiceDueDateInvalid)));
}

#[test]
fn test_update_validates_grace_period() {
    let (env, admin) = setup();
    let client = create_client(&env);
    client.initialize(&admin);
    assert!(client
        .try_set_protocol_limits(&admin, &5_000_000, &180, &0)
        .is_ok());
    assert!(client
        .try_set_protocol_limits(&admin, &5_000_000, &180, &2_592_000)
        .is_ok());
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

// ============================================================================
// MIN INVOICE AMOUNT FROM CONFIG TESTS (Issue #494)
// ============================================================================

#[test]
fn test_store_invoice_below_min_amount_fails() {
    let (env, admin) = setup();
    let client = create_client(&env);
    client.initialize(&admin);
    
    // Set min_invoice_amount to 1_000_000
    client.set_protocol_limits(&admin, &1_000_000, &365, &86400);
    
    // Attempt to validate invoice with amount below minimum
    let below_min_amount = 999_999i128;
    let due_date = env.ledger().timestamp() + 86400;
    
    let result = client.validate_invoice(&below_min_amount, &due_date);
    assert!(!result, "Invoice with amount below min should fail validation");
}

#[test]
fn test_store_invoice_at_min_amount_succeeds() {
    let (env, admin) = setup();
    let client = create_client(&env);
    client.initialize(&admin);
    
    // Set min_invoice_amount to 1_000_000
    client.set_protocol_limits(&admin, &1_000_000, &365, &86400);
    
    // Validate invoice with amount exactly at minimum
    let at_min_amount = 1_000_000i128;
    let due_date = env.ledger().timestamp() + 86400;
    
    let result = client.validate_invoice(&at_min_amount, &due_date);
    assert!(result, "Invoice with amount at min should pass validation");
}

#[test]
fn test_store_invoice_above_min_amount_succeeds() {
    let (env, admin) = setup();
    let client = create_client(&env);
    client.initialize(&admin);
    
    // Set min_invoice_amount to 1_000_000
    client.set_protocol_limits(&admin, &1_000_000, &365, &86400);
    
    // Validate invoice with amount above minimum
    let above_min_amount = 5_000_000i128;
    let due_date = env.ledger().timestamp() + 86400;
    
    let result = client.validate_invoice(&above_min_amount, &due_date);
    assert!(result, "Invoice with amount above min should pass validation");
}

#[test]
fn test_admin_updates_min_new_minimum_enforced() {
    let (env, admin) = setup();
    let client = create_client(&env);
    client.initialize(&admin);
    
    // Initial min_invoice_amount is 1_000_000
    client.set_protocol_limits(&admin, &1_000_000, &365, &86400);
    let due_date = env.ledger().timestamp() + 86400;
    
    // 500_000 should fail with initial min of 1_000_000
    assert!(!client.validate_invoice(&500_000, &due_date));
    
    // Admin updates min to 500_000
    client.set_protocol_limits(&admin, &500_000, &365, &86400);
    
    // Now 500_000 should succeed
    assert!(client.validate_invoice(&500_000, &due_date));
    
    // And 499_999 should fail
    assert!(!client.validate_invoice(&499_999, &due_date));
}

#[test]
fn test_admin_increases_min_new_minimum_enforced() {
    let (env, admin) = setup();
    let client = create_client(&env);
    client.initialize(&admin);
    
    // Initial min_invoice_amount is 1_000_000
    client.set_protocol_limits(&admin, &1_000_000, &365, &86400);
    let due_date = env.ledger().timestamp() + 86400;
    
    // 1_500_000 should pass with initial min of 1_000_000
    assert!(client.validate_invoice(&1_500_000, &due_date));
    
    // Admin increases min to 2_000_000
    client.set_protocol_limits(&admin, &2_000_000, &365, &86400);
    
    // Now 1_500_000 should fail
    assert!(!client.validate_invoice(&1_500_000, &due_date));
    
    // And 2_000_000 should succeed
    assert!(client.validate_invoice(&2_000_000, &due_date));
}

#[test]
fn test_default_min_amount_when_config_not_set() {
    let (env, _admin) = setup();
    let client = create_client(&env);
    
    // Don't initialize - get default limits
    let limits = client.get_protocol_limits();
    
    // Default min_invoice_amount should be 1_000_000
    assert_eq!(limits.min_invoice_amount, 1_000_000);
}

#[test]
fn test_min_amount_boundary_one_below() {
    let (env, admin) = setup();
    let client = create_client(&env);
    client.initialize(&admin);
    
    // Set min to specific value
    let min_amount = 2_500_000i128;
    client.set_protocol_limits(&admin, &min_amount, &365, &86400);
    let due_date = env.ledger().timestamp() + 86400;
    
    // One below min should fail
    assert!(!client.validate_invoice(&(min_amount - 1), &due_date));
    
    // Exactly at min should succeed
    assert!(client.validate_invoice(&min_amount, &due_date));
    
    // One above min should succeed
    assert!(client.validate_invoice(&(min_amount + 1), &due_date));
}

#[test]
fn test_min_amount_zero_not_allowed() {
    let (env, admin) = setup();
    let client = create_client(&env);
    client.initialize(&admin);
    
    // Attempt to set min_invoice_amount to 0 should fail
    let result = client.try_set_protocol_limits(&admin, &0, &365, &86400);
    assert_eq!(result, Err(Ok(QuickLendXError::InvalidAmount)));
}

#[test]
fn test_min_amount_negative_not_allowed() {
    let (env, admin) = setup();
    let client = create_client(&env);
    client.initialize(&admin);
    
    // Attempt to set min_invoice_amount to negative should fail
    let result = client.try_set_protocol_limits(&admin, &(-100), &365, &86400);
    assert_eq!(result, Err(Ok(QuickLendXError::InvalidAmount)));
}

#[test]
fn test_min_amount_very_small_positive() {
    let (env, admin) = setup();
    let client = create_client(&env);
    client.initialize(&admin);
    
    // Set min to smallest positive value (1)
    client.set_protocol_limits(&admin, &1, &365, &86400);
    let due_date = env.ledger().timestamp() + 86400;
    
    // 0 should fail
    assert!(!client.validate_invoice(&0, &due_date));
    
    // 1 should succeed
    assert!(client.validate_invoice(&1, &due_date));
}

#[test]
fn test_min_amount_very_large_value() {
    let (env, admin) = setup();
    let client = create_client(&env);
    client.initialize(&admin);
    
    // Set min to very large value
    let large_min = 1_000_000_000_000i128; // 1 trillion
    client.set_protocol_limits(&admin, &large_min, &365, &86400);
    let due_date = env.ledger().timestamp() + 86400;
    
    // Below large min should fail
    assert!(!client.validate_invoice(&(large_min - 1), &due_date));
    
    // At large min should succeed
    assert!(client.validate_invoice(&large_min, &due_date));
}

#[test]
fn test_min_amount_multiple_updates() {
    let (env, admin) = setup();
    let client = create_client(&env);
    client.initialize(&admin);
    let due_date = env.ledger().timestamp() + 86400;
    
    // First update
    client.set_protocol_limits(&admin, &100_000, &365, &86400);
    assert!(client.validate_invoice(&100_000, &due_date));
    assert!(!client.validate_invoice(&99_999, &due_date));
    
    // Second update - increase
    client.set_protocol_limits(&admin, &500_000, &365, &86400);
    assert!(!client.validate_invoice(&100_000, &due_date));
    assert!(client.validate_invoice(&500_000, &due_date));
    
    // Third update - decrease
    client.set_protocol_limits(&admin, &50_000, &365, &86400);
    assert!(client.validate_invoice(&50_000, &due_date));
    assert!(!client.validate_invoice(&49_999, &due_date));
}

#[test]
fn test_min_amount_persists_after_retrieval() {
    let (env, admin) = setup();
    let client = create_client(&env);
    client.initialize(&admin);
    
    let custom_min = 3_333_333i128;
    client.set_protocol_limits(&admin, &custom_min, &365, &86400);
    
    // Multiple retrievals should return same value
    let limits1 = client.get_protocol_limits();
    let limits2 = client.get_protocol_limits();
    let limits3 = client.get_protocol_limits();
    
    assert_eq!(limits1.min_invoice_amount, custom_min);
    assert_eq!(limits2.min_invoice_amount, custom_min);
    assert_eq!(limits3.min_invoice_amount, custom_min);
}

#[test]
fn test_non_admin_cannot_update_min_amount() {
    let (env, admin) = setup();
    let client = create_client(&env);
    client.initialize(&admin);
    
    let non_admin = Address::generate(&env);
    
    // Non-admin attempt to update min should fail
    let result = client.try_set_protocol_limits(&non_admin, &500_000, &365, &86400);
    assert_eq!(result, Err(Ok(QuickLendXError::Unauthorized)));
    
    // Original min should still be in effect
    let limits = client.get_protocol_limits();
    assert_eq!(limits.min_invoice_amount, 1_000_000);
}

#[test]
fn test_min_amount_independent_of_other_limits() {
    let (env, admin) = setup();
    let client = create_client(&env);
    client.initialize(&admin);
    
    // Update min_invoice_amount while keeping other limits different
    client.set_protocol_limits(&admin, &2_000_000, &180, &43200);
    
    let limits = client.get_protocol_limits();
    assert_eq!(limits.min_invoice_amount, 2_000_000);
    assert_eq!(limits.max_due_date_days, 180);
    assert_eq!(limits.grace_period_seconds, 43200);
    
    // Update again with different values
    client.set_protocol_limits(&admin, &3_000_000, &270, &86400);
    
    let limits2 = client.get_protocol_limits();
    assert_eq!(limits2.min_invoice_amount, 3_000_000);
    assert_eq!(limits2.max_due_date_days, 270);
    assert_eq!(limits2.grace_period_seconds, 86400);
}

#[test]
fn test_validate_invoice_uses_current_min_amount() {
    let (env, admin) = setup();
    let client = create_client(&env);
    client.initialize(&admin);
    let due_date = env.ledger().timestamp() + 86400;
    
    // Set initial min
    client.set_protocol_limits(&admin, &1_000_000, &365, &86400);
    
    // Validate passes at 1_000_000
    assert!(client.validate_invoice(&1_000_000, &due_date));
    
    // Update min to higher value
    client.set_protocol_limits(&admin, &2_000_000, &365, &86400);
    
    // Same amount now fails
    assert!(!client.validate_invoice(&1_000_000, &due_date));
    
    // New min passes
    assert!(client.validate_invoice(&2_000_000, &due_date));
}
