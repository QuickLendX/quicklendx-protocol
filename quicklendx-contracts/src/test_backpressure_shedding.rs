//! Backpressure load-shedding tests against real contract entrypoints.
//!
//! This test suite validates the load-shedding protection mechanism (maintenance mode)
//! that blocks mutating calls when the "threshold" is crossed (maintenance enabled) and
//! recovers when load drops below the threshold (maintenance disabled).
//!
//! # Invariant
//! When load-shedding is active (maintenance mode enabled):
//! - All state-mutating entrypoints MUST reject with `MaintenanceModeActive`
//! - Read-only entrypoints MUST continue to succeed
//! - Admin operations that toggle the load-shedding state are exempt
//!
//! When load-shedding is inactive (maintenance mode disabled):
//! - All entrypoints MUST operate normally
//! - Recovery from shed state must be complete and immediate
//!
//! # Threshold Behavior
//! The "threshold" in this context is the maintenance mode toggle:
//! - Crossing threshold: admin enables maintenance mode
//! - Dropping below threshold: admin disables maintenance mode
//! - This is the load-shedding mechanism for Soroban contracts (stateless, no persistent metrics)
//!
//! # Coverage
//! - Shedding at threshold: mutating calls rejected when maintenance active
//! - Recovery: mutating calls succeed when maintenance disabled
//! - Read availability: reads succeed during shedding
//! - Real entrypoints: tests against actual contract functions (store_invoice, place_bid, etc.)

#![cfg(test)]

use crate::errors::QuickLendXError;
use crate::invoice::InvoiceCategory;
use crate::maintenance::MaintenanceControl;
use crate::{QuickLendXContract, QuickLendXContractClient};
use soroban_sdk::{testutils::Address as _, Address, Env, String, Vec};

// ============================================================================
// Helpers
// ============================================================================

fn setup(env: &Env) -> (QuickLendXContractClient<'static>, Address) {
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    client.initialize_admin(&admin);
    (client, admin)
}

fn enable_maintenance(client: &QuickLendXContractClient, admin: &Address, env: &Env) {
    let reason = String::from_str(env, "Load shedding active");
    client.set_maintenance_mode(admin, &true, &reason);
    assert!(client.is_maintenance_mode(), "Maintenance mode must be enabled");
}

fn disable_maintenance(client: &QuickLendXContractClient, admin: &Address, env: &Env) {
    let reason = String::from_str(env, "");
    client.set_maintenance_mode(admin, &false, &reason);
    assert!(!client.is_maintenance_mode(), "Maintenance mode must be disabled");
}

fn create_verified_invoice(
    env: &Env,
    client: &QuickLendXContractClient,
    business: &Address,
    currency: &Address,
) -> soroban_sdk::BytesN<32> {
    let due_date = env.ledger().timestamp() + 86_400;
    let invoice_id = client.store_invoice(
        business,
        &1_000i128,
        currency,
        &due_date,
        &String::from_str(env, "Test invoice"),
        &InvoiceCategory::Services,
        &Vec::new(env),
    );
    client.verify_invoice(&invoice_id);
    invoice_id
}

// ============================================================================
// 1. Shedding at Threshold - Mutating Calls Rejected
// ============================================================================

#[test]
fn test_shedding_store_invoice_at_threshold() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86_400;

    // Cross threshold: enable maintenance mode
    enable_maintenance(&client, &admin, &env);

    // Mutating call must be shed with MaintenanceModeActive
    let result = client.try_store_invoice(
        &business,
        &1_000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Blocked"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    assert_eq!(
        result.unwrap_err().unwrap(),
        QuickLendXError::MaintenanceModeActive,
        "store_invoice must be shed at threshold"
    );
}

#[test]
fn test_shedding_place_bid_at_threshold() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let investor = Address::generate(&env);
    let invoice_id = soroban_sdk::BytesN::from_array(&env, &[0u8; 32]);

    // Cross threshold
    enable_maintenance(&client, &admin, &env);

    let result = client.try_place_bid(&investor, &invoice_id, &1_000i128, &1_100i128);
    assert_eq!(
        result.unwrap_err().unwrap(),
        QuickLendXError::MaintenanceModeActive,
        "place_bid must be shed at threshold"
    );
}

#[test]
fn test_shedding_verify_invoice_at_threshold() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let business = Address::generate(&env);
    let currency = Address::generate(&env);

    // Create invoice before crossing threshold
    let invoice_id = client.store_invoice(
        &business,
        &1_000i128,
        &currency,
        &(env.ledger().timestamp() + 86_400),
        &String::from_str(env, "Test"),
        &InvoiceCategory::Services,
        &Vec::new(env),
    );

    // Cross threshold
    enable_maintenance(&client, &admin, &env);

    let result = client.try_verify_invoice(&invoice_id);
    assert_eq!(
        result.unwrap_err().unwrap(),
        QuickLendXError::MaintenanceModeActive,
        "verify_invoice must be shed at threshold"
    );
}

#[test]
fn test_shedding_accept_bid_at_threshold() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let invoice_id = soroban_sdk::BytesN::from_array(&env, &[0u8; 32]);
    let bid_id = soroban_sdk::BytesN::from_array(&env, &[1u8; 32]);

    // Cross threshold
    enable_maintenance(&client, &admin, &env);

    let result = client.try_accept_bid(&invoice_id, &bid_id);
    assert_eq!(
        result.unwrap_err().unwrap(),
        QuickLendXError::MaintenanceModeActive,
        "accept_bid must be shed at threshold"
    );
}

#[test]
fn test_shedding_withdraw_bid_at_threshold() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let bid_id = soroban_sdk::BytesN::from_array(&env, &[0u8; 32]);

    // Cross threshold
    enable_maintenance(&client, &admin, &env);

    let result = client.try_withdraw_bid(&bid_id);
    assert_eq!(
        result.unwrap_err().unwrap(),
        QuickLendXError::MaintenanceModeActive,
        "withdraw_bid must be shed at threshold"
    );
}

#[test]
fn test_shedding_submit_kyc_at_threshold() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let business = Address::generate(&env);

    // Cross threshold
    enable_maintenance(&client, &admin, &env);

    let result = client.try_submit_kyc_application(&business, &String::from_str(&env, "{}"));
    assert_eq!(
        result.unwrap_err().unwrap(),
        QuickLendXError::MaintenanceModeActive,
        "submit_kyc_application must be shed at threshold"
    );
}

#[test]
fn test_shedding_update_invoice_metadata_at_threshold() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let business = Address::generate(&env);
    let currency = Address::generate(&env);

    // Create invoice before crossing threshold
    let invoice_id = client.store_invoice(
        &business,
        &1_000i128,
        &currency,
        &(env.ledger().timestamp() + 86_400),
        &String::from_str(env, "Test"),
        &InvoiceCategory::Services,
        &Vec::new(env),
    );

    // Cross threshold
    enable_maintenance(&client, &admin, &env);

    let metadata = crate::types::InvoiceMetadata {
        customer_name: Some(String::from_str(env, "Test Customer")),
        tax_id: None,
    };
    let result = client.try_update_invoice_metadata(&invoice_id, metadata);
    assert_eq!(
        result.unwrap_err().unwrap(),
        QuickLendXError::MaintenanceModeActive,
        "update_invoice_metadata must be shed at threshold"
    );
}

// ============================================================================
// 2. Recovery - Mutating Calls Succeed Below Threshold
// ============================================================================

#[test]
fn test_recovery_store_invoice_below_threshold() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86_400;

    // Cross threshold
    enable_maintenance(&client, &admin, &env);

    // Verify shedding
    let result = client.try_store_invoice(
        &business,
        &1_000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Blocked"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    assert_eq!(result.unwrap_err().unwrap(), QuickLendXError::MaintenanceModeActive);

    // Drop below threshold: disable maintenance mode
    disable_maintenance(&client, &admin, &env);

    // Mutating call must succeed again (recovery)
    let invoice_id = client.store_invoice(
        &business,
        &1_000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Recovered"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    assert_ne!(invoice_id, soroban_sdk::BytesN::from_array(&env, &[0u8; 32]));
}

#[test]
fn test_recovery_place_bid_below_threshold() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let investor = Address::generate(&env);
    let invoice_id = soroban_sdk::BytesN::from_array(&env, &[0u8; 32]);

    // Cross threshold
    enable_maintenance(&client, &admin, &env);

    // Verify shedding
    let result = client.try_place_bid(&investor, &invoice_id, &1_000i128, &1_100i128);
    assert_eq!(result.unwrap_err().unwrap(), QuickLendXError::MaintenanceModeActive);

    // Drop below threshold
    disable_maintenance(&client, &admin, &env);

    // Mutating call must succeed again
    let bid_id = client.place_bid(&investor, &invoice_id, &1_000i128, &1_100i128);
    assert_ne!(bid_id, soroban_sdk::BytesN::from_array(&env, &[0u8; 32]));
}

#[test]
fn test_recovery_verify_invoice_below_threshold() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let business = Address::generate(&env);
    let currency = Address::generate(&env);

    // Create invoice before crossing threshold
    let invoice_id = client.store_invoice(
        &business,
        &1_000i128,
        &currency,
        &(env.ledger().timestamp() + 86_400),
        &String::from_str(env, "Test"),
        &InvoiceCategory::Services,
        &Vec::new(env),
    );

    // Cross threshold
    enable_maintenance(&client, &admin, &env);

    // Verify shedding
    let result = client.try_verify_invoice(&invoice_id);
    assert_eq!(result.unwrap_err().unwrap(), QuickLendXError::MaintenanceModeActive);

    // Drop below threshold
    disable_maintenance(&client, &admin, &env);

    // Mutating call must succeed again
    client.verify_invoice(&invoice_id);
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, crate::invoice::InvoiceStatus::Verified);
}

#[test]
fn test_recovery_submit_kyc_below_threshold() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let business = Address::generate(&env);

    // Cross threshold
    enable_maintenance(&client, &admin, &env);

    // Verify shedding
    let result = client.try_submit_kyc_application(&business, &String::from_str(&env, "{}"));
    assert_eq!(result.unwrap_err().unwrap(), QuickLendXError::MaintenanceModeActive);

    // Drop below threshold
    disable_maintenance(&client, &admin, &env);

    // Mutating call must succeed again
    client.submit_kyc_application(&business, &String::from_str(&env, "{}")).unwrap();
}

#[test]
fn test_recovery_multiple_cycles() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86_400;

    // Cycle 1: Normal -> Shed -> Recover
    let invoice_id_1 = client.store_invoice(
        &business,
        &1_000i128,
        &currency,
        &due_date,
        &String::from_str(env, "Cycle 1"),
        &InvoiceCategory::Services,
        &Vec::new(env),
    );
    enable_maintenance(&client, &admin, &env);
    let result = client.try_store_invoice(
        &business,
        &2_000i128,
        &currency,
        &due_date,
        &String::from_str(env, "Blocked"),
        &InvoiceCategory::Services,
        &Vec::new(env),
    );
    assert_eq!(result.unwrap_err().unwrap(), QuickLendXError::MaintenanceModeActive);
    disable_maintenance(&client, &admin, &env);
    let invoice_id_2 = client.store_invoice(
        &business,
        &2_000i128,
        &currency,
        &due_date,
        &String::from_str(env, "Cycle 1 Recovered"),
        &InvoiceCategory::Services,
        &Vec::new(env),
    );

    // Cycle 2: Normal -> Shed -> Recover
    enable_maintenance(&client, &admin, &env);
    let result = client.try_store_invoice(
        &business,
        &3_000i128,
        &currency,
        &due_date,
        &String::from_str(env, "Blocked"),
        &InvoiceCategory::Services,
        &Vec::new(env),
    );
    assert_eq!(result.unwrap_err().unwrap(), QuickLendXError::MaintenanceModeActive);
    disable_maintenance(&client, &admin, &env);
    let invoice_id_3 = client.store_invoice(
        &business,
        &3_000i128,
        &currency,
        &due_date,
        &String::from_str(env, "Cycle 2 Recovered"),
        &InvoiceCategory::Services,
        &Vec::new(env),
    );

    // Verify all successful writes persisted
    assert_ne!(invoice_id_1, invoice_id_2);
    assert_ne!(invoice_id_2, invoice_id_3);
}

// ============================================================================
// 3. Read Availability - Reads Not Shed
// ============================================================================

#[test]
fn test_reads_not_shed_get_invoice() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let business = Address::generate(&env);
    let currency = Address::generate(&env);

    // Create invoice before crossing threshold
    let invoice_id = create_verified_invoice(&env, &client, &business, &currency);

    // Cross threshold
    enable_maintenance(&client, &admin, &env);

    // Read must succeed during shedding
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, crate::invoice::InvoiceStatus::Verified);
    assert_eq!(invoice.amount, 1_000i128);
}

#[test]
fn test_reads_not_shed_get_available_invoices() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let business = Address::generate(&env);
    let currency = Address::generate(&env);

    // Create verified invoices before crossing threshold
    let _invoice_id_1 = create_verified_invoice(&env, &client, &business, &currency);
    let _invoice_id_2 = create_verified_invoice(&env, &client, &business, &currency);

    // Cross threshold
    enable_maintenance(&client, &admin, &env);

    // Read must succeed during shedding
    let available = client.get_available_invoices();
    assert_eq!(available.len(), 2);
}

#[test]
fn test_reads_not_shed_get_business_invoices() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let business = Address::generate(&env);
    let currency = Address::generate(&env);

    // Create invoices before crossing threshold
    let _invoice_id_1 = create_verified_invoice(&env, &client, &business, &currency);
    let _invoice_id_2 = create_verified_invoice(&env, &client, &business, &currency);

    // Cross threshold
    enable_maintenance(&client, &admin, &env);

    // Read must succeed during shedding
    let business_invoices = client.get_business_invoices(&business);
    assert_eq!(business_invoices.len(), 2);
}

#[test]
fn test_reads_not_shed_get_total_invoice_count() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let business = Address::generate(&env);
    let currency = Address::generate(&env);

    // Create invoices before crossing threshold
    let _invoice_id_1 = create_verified_invoice(&env, &client, &business, &currency);
    let _invoice_id_2 = create_verified_invoice(&env, &client, &business, &currency);

    // Cross threshold
    enable_maintenance(&client, &admin, &env);

    // Read must succeed during shedding
    let count = client.get_total_invoice_count();
    assert_eq!(count, 2);
}

#[test]
fn test_reads_not_shed_get_bid() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    let investor = Address::generate(&env);

    // Create invoice and bid before crossing threshold
    let invoice_id = create_verified_invoice(&env, &client, &business, &currency);
    let bid_id = client.place_bid(&investor, &invoice_id, &1_000i128, &1_100i128);

    // Cross threshold
    enable_maintenance(&client, &admin, &env);

    // Read must succeed during shedding
    let bid = client.get_bid(&bid_id);
    assert!(bid.is_some());
    assert_eq!(bid.unwrap().bid_amount, 1_000i128);
}

#[test]
fn test_reads_not_shed_get_maintenance_reason() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let reason = String::from_str(env, "Load shedding for testing");

    // Cross threshold
    client.set_maintenance_mode(&admin, &true, &reason);

    // Read the reason - must succeed during shedding
    let stored_reason = client.get_maintenance_reason();
    assert_eq!(stored_reason.unwrap(), reason);
}

#[test]
fn test_reads_not_shed_is_maintenance_mode() {
    let env = Env::default();
    let (client, admin) = setup(&env);

    // Cross threshold
    enable_maintenance(&client, &admin, &env);

    // Query the flag - must succeed during shedding
    assert!(client.is_maintenance_mode());
}

// ============================================================================
// 4. Threshold Boundary Conditions
// ============================================================================

#[test]
fn test_exactly_at_threshold_shedding() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86_400;

    // Exactly at threshold: maintenance mode just enabled
    enable_maintenance(&client, &admin, &env);

    // Must shed
    let result = client.try_store_invoice(
        &business,
        &1_000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Blocked"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    assert_eq!(result.unwrap_err().unwrap(), QuickLendXError::MaintenanceModeActive);
}

#[test]
fn test_exactly_below_threshold_recovery() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86_400;

    // Cross threshold
    enable_maintenance(&client, &admin, &env);

    // Exactly below threshold: maintenance mode just disabled
    disable_maintenance(&client, &admin, &env);

    // Must recover immediately
    let invoice_id = client.store_invoice(
        &business,
        &1_000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Recovered"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    assert_ne!(invoice_id, soroban_sdk::BytesN::from_array(&env, &[0u8; 32]));
}

#[test]
fn test_reads_during_shedding_multiple_queries() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let business = Address::generate(&env);
    let currency = Address::generate(&env);

    // Create multiple invoices before crossing threshold
    for _ in 0..5 {
        let _ = create_verified_invoice(&env, &client, &business, &currency);
    }

    // Cross threshold
    enable_maintenance(&client, &admin, &env);

    // Multiple reads must all succeed during shedding
    let total_count = client.get_total_invoice_count();
    assert_eq!(total_count, 5);

    let available = client.get_available_invoices();
    assert_eq!(available.len(), 5);

    let business_invoices = client.get_business_invoices(&business);
    assert_eq!(business_invoices.len(), 5);

    // Verify individual reads
    for invoice_id in available.iter() {
        let invoice = client.get_invoice(&invoice_id);
        assert_eq!(invoice.status, crate::invoice::InvoiceStatus::Verified);
    }
}

// ============================================================================
// 5. Constants and Reset Behavior
// ============================================================================

#[test]
fn test_maintenance_mode_key_constant() {
    let env = Env::default();
    let (client, admin) = setup(&env);

    // Verify the constant matches the storage key used
    client.set_maintenance_mode(&admin, &true, &String::from_str(env, "Test"));
    assert!(client.is_maintenance_mode());

    // The constant is defined in maintenance.rs as MAINTENANCE_MODE_KEY
    // This test verifies the behavior matches the constant's purpose
    assert_eq!(
        crate::maintenance::MAINTENANCE_MODE_KEY,
        soroban_sdk::symbol_short!("maint")
    );
}

#[test]
fn test_maintenance_reason_key_constant() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let reason = String::from_str(env, "Test reason");

    client.set_maintenance_mode(&admin, &true, &reason);

    // Verify the reason is stored under the correct constant key
    let stored = client.get_maintenance_reason();
    assert_eq!(stored.unwrap(), reason);

    // The constant is defined in maintenance.rs as MAINTENANCE_REASON_KEY
    assert_eq!(
        crate::maintenance::MAINTENANCE_REASON_KEY,
        soroban_sdk::symbol_short!("maint_rsn")
    );
}

#[test]
fn test_reset_clears_reason() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let reason = String::from_str(env, "Test reason");

    // Enable with reason
    client.set_maintenance_mode(&admin, &true, &reason);
    assert_eq!(client.get_maintenance_reason().unwrap(), reason);

    // Reset (disable) - reason must be cleared
    client.set_maintenance_mode(&admin, &false, &String::from_str(env, ""));
    assert!(client.get_maintenance_reason().is_none(), "Reason must be cleared on reset");
}

#[test]
fn test_max_reason_length_constant() {
    let env = Env::default();
    let (client, admin) = setup(&env);

    // Verify the constant matches the validation logic
    let max_len = crate::maintenance::MAX_REASON_LEN;
    assert_eq!(max_len, 256);

    // Create a reason exactly at the limit
    let max_reason: String = {
        let bytes = soroban_sdk::Bytes::from_slice(&env, &vec![b'a'; max_len as usize]);
        String::try_from_bytes(&bytes).unwrap()
    };

    // Should succeed
    client.set_maintenance_mode(&admin, &true, &max_reason);
    assert!(client.is_maintenance_mode());

    // Reset
    client.set_maintenance_mode(&admin, &false, &String::from_str(env, ""));

    // Create a reason one byte over the limit
    let oversized: String = {
        let bytes = soroban_sdk::Bytes::from_slice(&env, &vec![b'x'; (max_len + 1) as usize]);
        String::try_from_bytes(&bytes).unwrap()
    };

    // Should fail
    let result = client.try_set_maintenance_mode(&admin, &true, &oversized);
    assert_eq!(result.unwrap_err().unwrap(), QuickLendXError::InvalidDescription);
}

// ============================================================================
// 6. Integration - Real Contract Entrypoints
// ============================================================================

#[test]
fn test_integration_full_lifecycle_with_shedding() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    let investor = Address::generate(&env);

    // Phase 1: Normal operation - create invoice and bid
    let invoice_id = create_verified_invoice(&env, &client, &business, &currency);
    let bid_id = client.place_bid(&investor, &invoice_id, &1_000i128, &1_100i128);

    // Verify reads work
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, crate::invoice::InvoiceStatus::Verified);

    let bid = client.get_bid(&bid_id);
    assert!(bid.is_some());

    // Phase 2: Cross threshold - shedding active
    enable_maintenance(&client, &admin, &env);

    // Verify writes are shed
    let result = client.try_store_invoice(
        &business,
        &2_000i128,
        &currency,
        &(env.ledger().timestamp() + 86_400),
        &String::from_str(env, "Blocked"),
        &InvoiceCategory::Services,
        &Vec::new(env),
    );
    assert_eq!(result.unwrap_err().unwrap(), QuickLendXError::MaintenanceModeActive);

    let result = client.try_place_bid(&investor, &invoice_id, &2_000i128, &2_200i128);
    assert_eq!(result.unwrap_err().unwrap(), QuickLendXError::MaintenanceModeActive);

    // Verify reads still work
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, crate::invoice::InvoiceStatus::Verified);

    let bid = client.get_bid(&bid_id);
    assert!(bid.is_some());

    // Phase 3: Drop below threshold - recovery
    disable_maintenance(&client, &admin, &env);

    // Verify writes work again
    let invoice_id_2 = client.store_invoice(
        &business,
        &2_000i128,
        &currency,
        &(env.ledger().timestamp() + 86_400),
        &String::from_str(env, "Recovered"),
        &InvoiceCategory::Services,
        &Vec::new(env),
    );
    assert_ne!(invoice_id_2, invoice_id);

    let bid_id_2 = client.place_bid(&investor, &invoice_id_2, &2_000i128, &2_200i128);
    assert_ne!(bid_id_2, bid_id);

    // Verify reads work
    let invoice_2 = client.get_invoice(&invoice_id_2);
    assert_eq!(invoice_2.amount, 2_000i128);
}
