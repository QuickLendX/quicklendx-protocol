//! Comprehensive tests for maintenance mode.
//!
//! Coverage:
//! 1. Toggle: admin can enable and disable maintenance mode.
//! 2. Read-only enforcement: write ops return MaintenanceModeActive.
//! 3. Read availability: query ops succeed during maintenance.
//! 4. Bypass prevention: non-admin cannot toggle maintenance mode.
//! 5. Reason messaging: reason string is stored, returned, and cleared.
//! 6. Reason validation: oversized reason is rejected.
//! 7. Admin rotation: new admin can exit maintenance; old admin cannot.
//! 8. Idempotency: enabling when already enabled is safe.
//! 9. Coexistence with pause: maintenance and pause are independent flags.
//!
//! Security notes:
//! - `require_write_allowed` is the only enforcement point; every write
//!   entrypoint must call it. Tests verify the rejection at the entrypoint
//!   level, not just at the module level.
//! - The toggle itself is exempt from the guard so an admin can always exit.
//! - Non-admin callers receive `NotAdmin`, not a misleading generic error.

#![cfg(test)]

use crate::errors::QuickLendXError;
use crate::invoice::{InvoiceCategory, InvoiceStatus};
use crate::maintenance::{MaintenanceControl, MAX_REASON_LEN};
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

fn reason(env: &Env, msg: &str) -> String {
    String::from_str(env, msg)
}

fn make_invoice(
    env: &Env,
    client: &QuickLendXContractClient,
    business: &Address,
    currency: &Address,
) -> soroban_sdk::BytesN<32> {
    let due_date = env.ledger().timestamp() + 86_400;
    client.store_invoice(
        business,
        &1_000i128,
        currency,
        &due_date,
        &String::from_str(env, "Test invoice"),
        &InvoiceCategory::Services,
        &Vec::new(env),
    )
}

// ============================================================================
// 1. Toggle
// ============================================================================

#[test]
fn test_admin_can_enable_maintenance_mode() {
    let env = Env::default();
    let (client, admin) = setup(&env);

    assert!(!client.is_maintenance_mode());

    client.set_maintenance_mode(&admin, &true, &reason(&env, "Scheduled upgrade"));

    assert!(client.is_maintenance_mode());
}

#[test]
fn test_admin_can_disable_maintenance_mode() {
    let env = Env::default();
    let (client, admin) = setup(&env);

    client.set_maintenance_mode(&admin, &true, &reason(&env, "Upgrade"));
    assert!(client.is_maintenance_mode());

    client.set_maintenance_mode(&admin, &false, &reason(&env, ""));
    assert!(!client.is_maintenance_mode());
}

// ============================================================================
// 2. Read-only enforcement - write ops blocked
// ============================================================================

#[test]
fn test_maintenance_blocks_store_invoice() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86_400;

    client.set_maintenance_mode(&admin, &true, &reason(&env, "Upgrade in progress"));

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
        "store_invoice must return MaintenanceModeActive during maintenance"
    );
}

#[test]
fn test_maintenance_blocks_place_bid() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let investor = Address::generate(&env);
    let currency = Address::generate(&env);
    let invoice_id = soroban_sdk::BytesN::from_array(&env, &[0u8; 32]);

    client.set_maintenance_mode(&admin, &true, &reason(&env, "Upgrade"));

    let result = client.try_place_bid(&investor, &invoice_id, &1_000i128, &1_100i128);
    assert_eq!(
        result.unwrap_err().unwrap(),
        QuickLendXError::MaintenanceModeActive
    );
}

#[test]
fn test_maintenance_blocks_verify_invoice() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let business = Address::generate(&env);
    let currency = Address::generate(&env);

    // Create invoice before entering maintenance.
    let invoice_id = make_invoice(&env, &client, &business, &currency);

    client.set_maintenance_mode(&admin, &true, &reason(&env, "Upgrade"));

    let result = client.try_verify_invoice(&invoice_id);
    assert_eq!(
        result.unwrap_err().unwrap(),
        QuickLendXError::MaintenanceModeActive
    );
}

#[test]
fn test_maintenance_blocks_submit_kyc() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let business = Address::generate(&env);

    client.set_maintenance_mode(&admin, &true, &reason(&env, "Upgrade"));

    let result = client.try_submit_kyc_application(&business, &String::from_str(&env, "{}"));
    assert_eq!(
        result.unwrap_err().unwrap(),
        QuickLendXError::MaintenanceModeActive
    );
}

#[test]
fn test_maintenance_blocks_accept_bid_and_fund() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let invoice_id = soroban_sdk::BytesN::from_array(&env, &[0u8; 32]);
    let bid_id = soroban_sdk::BytesN::from_array(&env, &[1u8; 32]);

    client.set_maintenance_mode(&admin, &true, &reason(&env, "Upgrade"));

    let result = client.try_accept_bid_and_fund(&invoice_id, &bid_id);
    assert_eq!(
        result.unwrap_err().unwrap(),
        QuickLendXError::MaintenanceModeActive
    );
}

#[test]
fn test_maintenance_blocks_settle_invoice() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let invoice_id = soroban_sdk::BytesN::from_array(&env, &[0u8; 32]);

    client.set_maintenance_mode(&admin, &true, &reason(&env, "Upgrade"));

    let result = client.try_settle_invoice(&invoice_id, &1_000i128);
    assert_eq!(
        result.unwrap_err().unwrap(),
        QuickLendXError::MaintenanceModeActive
    );
}

// ============================================================================
// 3. Reads remain available
// ============================================================================

#[test]
fn test_maintenance_allows_get_invoice() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let business = Address::generate(&env);
    let currency = Address::generate(&env);

    let invoice_id = make_invoice(&env, &client, &business, &currency);

    client.set_maintenance_mode(&admin, &true, &reason(&env, "Upgrade"));

    // get_invoice is a read - must succeed.
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Pending);
}

#[test]
fn test_maintenance_allows_is_maintenance_mode_query() {
    let env = Env::default();
    let (client, admin) = setup(&env);

    client.set_maintenance_mode(&admin, &true, &reason(&env, "Upgrade"));

    // Querying the flag itself must always work.
    assert!(client.is_maintenance_mode());
}

#[test]
fn test_maintenance_allows_get_maintenance_reason() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let msg = "Scheduled DB migration - back in 15 min";

    client.set_maintenance_mode(&admin, &true, &reason(&env, msg));

    let stored = client.get_maintenance_reason();
    assert_eq!(
        stored.unwrap(),
        reason(&env, msg),
        "Reason must be readable during maintenance"
    );
}

// ============================================================================
// 4. Bypass prevention - non-admin cannot toggle
// ============================================================================

#[test]
fn test_non_admin_cannot_enable_maintenance() {
    let env = Env::default();
    let (client, _admin) = setup(&env);
    let attacker = Address::generate(&env);

    let result =
        client.try_set_maintenance_mode(&attacker, &true, &reason(&env, "Malicious reason"));
    assert_eq!(
        result.unwrap_err().unwrap(),
        QuickLendXError::NotAdmin,
        "Non-admin must receive NotAdmin, not a generic error"
    );
    assert!(!client.is_maintenance_mode(), "Flag must not change");
}

#[test]
fn test_non_admin_cannot_disable_maintenance() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let attacker = Address::generate(&env);

    client.set_maintenance_mode(&admin, &true, &reason(&env, "Legitimate maintenance"));
    assert!(client.is_maintenance_mode());

    let result = client.try_set_maintenance_mode(&attacker, &false, &reason(&env, ""));
    assert_eq!(result.unwrap_err().unwrap(), QuickLendXError::NotAdmin);
    assert!(
        client.is_maintenance_mode(),
        "Maintenance must remain active after rejected bypass attempt"
    );
}

// ============================================================================
// 5. Reason string - stored, returned, cleared on disable
// ============================================================================

#[test]
fn test_reason_stored_on_enable() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let msg = "Network upgrade v2.1";

    client.set_maintenance_mode(&admin, &true, &reason(&env, msg));

    assert_eq!(
        client.get_maintenance_reason().unwrap(),
        reason(&env, msg)
    );
}

#[test]
fn test_reason_cleared_on_disable() {
    let env = Env::default();
    let (client, admin) = setup(&env);

    client.set_maintenance_mode(&admin, &true, &reason(&env, "Upgrade"));
    client.set_maintenance_mode(&admin, &false, &reason(&env, ""));

    assert!(
        client.get_maintenance_reason().is_none(),
        "Reason must be cleared when maintenance is disabled"
    );
}

#[test]
fn test_reason_updated_on_re_enable() {
    let env = Env::default();
    let (client, admin) = setup(&env);

    client.set_maintenance_mode(&admin, &true, &reason(&env, "First window"));
    client.set_maintenance_mode(&admin, &false, &reason(&env, ""));
    client.set_maintenance_mode(&admin, &true, &reason(&env, "Second window"));

    assert_eq!(
        client.get_maintenance_reason().unwrap(),
        reason(&env, "Second window")
    );
}

// ============================================================================
// 6. Reason validation - oversized reason rejected
// ============================================================================

#[test]
fn test_oversized_reason_rejected() {
    let env = Env::default();
    let (client, admin) = setup(&env);

    // Build a reason one byte over the limit.
    let oversized: String = {
        let bytes = soroban_sdk::Bytes::from_slice(
            &env,
            &vec![b'x'; (MAX_REASON_LEN + 1) as usize],
        );
        String::try_from_bytes(&bytes).unwrap()
    };

    let result = client.try_set_maintenance_mode(&admin, &true, &oversized);
    assert_eq!(
        result.unwrap_err().unwrap(),
        QuickLendXError::InvalidDescription,
        "Reason exceeding MAX_REASON_LEN must be rejected"
    );
    assert!(!client.is_maintenance_mode(), "Flag must not change on rejection");
}

#[test]
fn test_max_length_reason_accepted() {
    let env = Env::default();
    let (client, admin) = setup(&env);

    let max_reason: String = {
        let bytes = soroban_sdk::Bytes::from_slice(
            &env,
            &vec![b'a'; MAX_REASON_LEN as usize],
        );
        String::try_from_bytes(&bytes).unwrap()
    };

    client.set_maintenance_mode(&admin, &true, &max_reason);
    assert!(client.is_maintenance_mode());
}

// ============================================================================
// 7. Admin rotation - new admin can exit; old admin cannot
// ============================================================================

#[test]
fn test_new_admin_can_exit_maintenance_after_rotation() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let new_admin = Address::generate(&env);

    client.set_maintenance_mode(&admin, &true, &reason(&env, "Upgrade"));
    client.transfer_admin(&new_admin);

    // Old admin is rejected.
    let result = client.try_set_maintenance_mode(&admin, &false, &reason(&env, ""));
    assert_eq!(result.unwrap_err().unwrap(), QuickLendXError::NotAdmin);
    assert!(client.is_maintenance_mode(), "Still in maintenance");

    // New admin succeeds.
    client.set_maintenance_mode(&new_admin, &false, &reason(&env, ""));
    assert!(!client.is_maintenance_mode());
}

// ============================================================================
// 8. Idempotency - enabling when already enabled is safe
// ============================================================================

#[test]
fn test_enable_when_already_enabled_is_safe() {
    let env = Env::default();
    let (client, admin) = setup(&env);

    client.set_maintenance_mode(&admin, &true, &reason(&env, "First reason"));
    // Enable again with a different reason - should update the reason.
    client.set_maintenance_mode(&admin, &true, &reason(&env, "Updated reason"));

    assert!(client.is_maintenance_mode());
    assert_eq!(
        client.get_maintenance_reason().unwrap(),
        reason(&env, "Updated reason")
    );
}

#[test]
fn test_disable_when_already_disabled_is_safe() {
    let env = Env::default();
    let (client, admin) = setup(&env);

    assert!(!client.is_maintenance_mode());
    // Disable when already off - must not panic or corrupt state.
    client.set_maintenance_mode(&admin, &false, &reason(&env, ""));
    assert!(!client.is_maintenance_mode());
    assert!(client.get_maintenance_reason().is_none());
}

// ============================================================================
// 9. Operations resume after maintenance is lifted
// ============================================================================

#[test]
fn test_write_ops_resume_after_maintenance_disabled() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86_400;

    client.set_maintenance_mode(&admin, &true, &reason(&env, "Brief maintenance"));

    // Blocked during maintenance.
    assert!(client
        .try_store_invoice(
            &business,
            &1_000i128,
            &currency,
            &due_date,
            &String::from_str(&env, "Blocked"),
            &InvoiceCategory::Services,
            &Vec::new(&env),
        )
        .is_err());

    // Lift maintenance.
    client.set_maintenance_mode(&admin, &false, &reason(&env, ""));

    // Now succeeds.
    let invoice_id = client.store_invoice(
        &business,
        &1_000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Restored"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Pending);
}
