//! Exhaustive maintenance write-gating matrix for QuickLendX protocol.
//!
//! **Invariant**: Every mutating entrypoint must call `require_write_allowed` to enforce
//! maintenance mode uniformly. This matrix test enumerates representative mutations across
//! all contract categories and verifies that:
//!
//! 1. All mutating entrypoints reject with `MaintenanceModeActive` during maintenance.
//! 2. Read operations succeed during maintenance (queries are always allowed).
//! 3. Maintenance can be disabled to restore normal operation.
//! 4. Reason strings round-trip correctly (`get_maintenance_reason`).
//! 5. Edge cases (empty reason, max-length reason, toggle mid-test) are handled.
//!
//! **Test Categories**:
//! - Invoice mutations: `store_invoice`, `verify_invoice`, `cancel_invoice`, `update_invoice_category`
//! - Bid mutations: `place_bid`, `accept_bid_and_fund`, `cancel_bid`
//! - Settlement mutations: `settle_invoice`, `process_partial_payment`, `refund_escrow_funds`
//! - Dispute mutations: `create_dispute`
//! - Admin mutations: `set_protocol_config`, `add_currency`, `verify_business`, `set_bid_ttl_days`
//! - KYC mutations: `submit_kyc_application`, `submit_investor_kyc`
//!
//! **Test Finding Documentation**:
//! If a mutation does NOT reject during maintenance, it is recorded with a comment
//! and should be investigated as a potential finding (missing guard).
//!
//! **Coverage Target**: ≥95% of branch coverage for maintenance-gating paths.

#![cfg(test)]

use crate::errors::QuickLendXError;
use crate::invoice::{InvoiceCategory, InvoiceStatus};
use crate::maintenance::{MaintenanceControl, MAX_REASON_LEN};
use crate::{QuickLendXContract, QuickLendXContractClient};
use soroban_sdk::{testutils::Address as _, Address, BytesN, Env, String, Vec};

// ============================================================================
// Setup & Helper Functions
// ============================================================================

/// Sets up a fresh contract instance with an admin.
fn setup(env: &Env) -> (QuickLendXContractClient<'static>, Address) {
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    client.initialize_admin(&admin);
    (client, admin)
}

/// Helper to create a String reason from &str.
fn reason(env: &Env, msg: &str) -> String {
    String::from_str(env, msg)
}

/// Helper to create a test invoice for use in bid and settlement tests.
fn make_invoice(
    env: &Env,
    client: &QuickLendXContractClient,
    business: &Address,
    currency: &Address,
) -> BytesN<32> {
    let due_date = env.ledger().timestamp() + 86_400;
    client.store_invoice(
        business,
        &1_000i128,
        currency,
        &due_date,
        &String::from_str(env, "Test invoice for maintenance matrix"),
        &InvoiceCategory::Services,
        &Vec::new(env),
    )
}

/// Enables maintenance mode with a given reason and verifies it is active.
fn enable_maintenance(env: &Env, client: &QuickLendXContractClient, admin: &Address, msg: &str) {
    client.set_maintenance_mode(admin, &true, &reason(env, msg));
    assert!(
        client.is_maintenance_mode(),
        "Maintenance mode must be active after enable"
    );
}

/// Disables maintenance mode and verifies it is inactive.
fn disable_maintenance(env: &Env, client: &QuickLendXContractClient, admin: &Address) {
    client.set_maintenance_mode(admin, &false, &reason(env, ""));
    assert!(
        !client.is_maintenance_mode(),
        "Maintenance mode must be inactive after disable"
    );
}

// ============================================================================
// INVOICE MUTATIONS - Write-Gated Matrix
// ============================================================================

/// Tests that `store_invoice` is blocked during maintenance.
/// **Category**: Invoice creation (high-impact mutation).
#[test]
fn test_maintenance_blocks_store_invoice() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86_400;

    enable_maintenance(&env, &client, &admin, "Upgrade in progress");

    let result = client.try_store_invoice(
        &business,
        &1_000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Should be blocked"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    assert_eq!(
        result.unwrap_err().unwrap(),
        QuickLendXError::MaintenanceModeActive,
        "store_invoice must return MaintenanceModeActive during maintenance"
    );
}

/// Tests that `verify_invoice` is blocked during maintenance.
/// **Category**: Invoice state mutation (verification).
#[test]
fn test_maintenance_blocks_verify_invoice() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let business = Address::generate(&env);
    let currency = Address::generate(&env);

    // Create invoice before entering maintenance.
    let invoice_id = make_invoice(&env, &client, &business, &currency);

    enable_maintenance(&env, &client, &admin, "Maintenance window");

    let result = client.try_verify_invoice(&invoice_id);
    assert_eq!(
        result.unwrap_err().unwrap(),
        QuickLendXError::MaintenanceModeActive,
        "verify_invoice must reject during maintenance"
    );
}

/// Tests that `cancel_invoice` is blocked during maintenance.
/// **Category**: Invoice lifecycle mutation.
#[test]
fn test_maintenance_blocks_cancel_invoice() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let business = Address::generate(&env);
    let currency = Address::generate(&env);

    let invoice_id = make_invoice(&env, &client, &business, &currency);

    enable_maintenance(&env, &client, &admin, "Maintenance");

    let result = client.try_cancel_invoice(&invoice_id);
    assert_eq!(
        result.unwrap_err().unwrap(),
        QuickLendXError::MaintenanceModeActive,
        "cancel_invoice must reject during maintenance"
    );
}

/// Tests that `update_invoice_category` is blocked during maintenance.
/// **Category**: Invoice metadata mutation.
#[test]
fn test_maintenance_blocks_update_invoice_category() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let business = Address::generate(&env);
    let currency = Address::generate(&env);

    let invoice_id = make_invoice(&env, &client, &business, &currency);

    enable_maintenance(&env, &client, &admin, "Category update blocked");

    let result = client.try_update_invoice_category(&invoice_id, &InvoiceCategory::Consulting);
    assert_eq!(
        result.unwrap_err().unwrap(),
        QuickLendXError::MaintenanceModeActive,
        "update_invoice_category must reject during maintenance"
    );
}

// ============================================================================
// BID MUTATIONS - Write-Gated Matrix
// ============================================================================

/// Tests that `place_bid` is blocked during maintenance.
/// **Category**: Bid creation (high-impact mutation).
#[test]
fn test_maintenance_blocks_place_bid() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let investor = Address::generate(&env);
    let business = Address::generate(&env);
    let currency = Address::generate(&env);

    let invoice_id = make_invoice(&env, &client, &business, &currency);

    enable_maintenance(&env, &client, &admin, "Bid placement disabled");

    let result = client.try_place_bid(&investor, &invoice_id, &500i128, &600i128);
    assert_eq!(
        result.unwrap_err().unwrap(),
        QuickLendXError::MaintenanceModeActive,
        "place_bid must reject during maintenance"
    );
}

/// Tests that `accept_bid_and_fund` is blocked during maintenance.
/// **Category**: Escrow creation and settlement (critical mutation).
#[test]
fn test_maintenance_blocks_accept_bid_and_fund() {
    let env = Env::default();
    let (client, admin) = setup(&env);

    // Use dummy IDs; exact existence is not critical for this test.
    let invoice_id = BytesN::from_array(&env, &[0u8; 32]);
    let bid_id = BytesN::from_array(&env, &[1u8; 32]);

    enable_maintenance(&env, &client, &admin, "Escrow operations suspended");

    let result = client.try_accept_bid_and_fund(&invoice_id, &bid_id);
    assert_eq!(
        result.unwrap_err().unwrap(),
        QuickLendXError::MaintenanceModeActive,
        "accept_bid_and_fund must reject during maintenance"
    );
}

/// Tests that `cancel_bid` is blocked during maintenance.
/// **Category**: Bid lifecycle mutation.
#[test]
fn test_maintenance_blocks_cancel_bid() {
    let env = Env::default();
    let (client, admin) = setup(&env);

    let bid_id = BytesN::from_array(&env, &[2u8; 32]);

    enable_maintenance(&env, &client, &admin, "Bid management disabled");

    let result = client.try_cancel_bid(&bid_id);
    assert_eq!(
        result.unwrap_err().unwrap(),
        QuickLendXError::MaintenanceModeActive,
        "cancel_bid must reject during maintenance"
    );
}

// ============================================================================
// SETTLEMENT & PAYMENT MUTATIONS - Write-Gated Matrix
// ============================================================================

/// Tests that `settle_invoice` is blocked during maintenance.
/// **Category**: Settlement/payment mutation (critical).
#[test]
fn test_maintenance_blocks_settle_invoice() {
    let env = Env::default();
    let (client, admin) = setup(&env);

    let invoice_id = BytesN::from_array(&env, &[3u8; 32]);

    enable_maintenance(&env, &client, &admin, "Settlement suspended");

    let result = client.try_settle_invoice(&invoice_id, &1_000i128);
    assert_eq!(
        result.unwrap_err().unwrap(),
        QuickLendXError::MaintenanceModeActive,
        "settle_invoice must reject during maintenance"
    );
}

/// Tests that `process_partial_payment` is blocked during maintenance.
/// **Category**: Partial payment mutation.
#[test]
fn test_maintenance_blocks_process_partial_payment() {
    let env = Env::default();
    let (client, admin) = setup(&env);

    let invoice_id = BytesN::from_array(&env, &[4u8; 32]);

    enable_maintenance(&env, &client, &admin, "Payments blocked");

    let result = client.try_process_partial_payment(
        &invoice_id,
        &500i128,
        &String::from_str(&env, "tx_12345"),
    );
    assert_eq!(
        result.unwrap_err().unwrap(),
        QuickLendXError::MaintenanceModeActive,
        "process_partial_payment must reject during maintenance"
    );
}

/// Tests that `refund_escrow_funds` is blocked during maintenance.
/// **Category**: Escrow refund mutation (critical).
#[test]
fn test_maintenance_blocks_refund_escrow_funds() {
    let env = Env::default();
    let (client, admin) = setup(&env);

    let invoice_id = BytesN::from_array(&env, &[5u8; 32]);

    enable_maintenance(&env, &client, &admin, "Escrow refunds suspended");

    let result = client.try_refund_escrow_funds(&invoice_id);
    assert_eq!(
        result.unwrap_err().unwrap(),
        QuickLendXError::MaintenanceModeActive,
        "refund_escrow_funds must reject during maintenance"
    );
}

// ============================================================================
// DISPUTE MUTATIONS - Write-Gated Matrix
// ============================================================================

/// Tests that `create_dispute` is blocked during maintenance.
/// **Category**: Dispute creation mutation.
#[test]
fn test_maintenance_blocks_create_dispute() {
    let env = Env::default();
    let (client, admin) = setup(&env);

    let invoice_id = BytesN::from_array(&env, &[6u8; 32]);
    let creator = Address::generate(&env);

    enable_maintenance(&env, &client, &admin, "Dispute creation suspended");

    let result = client.try_create_dispute(
        &invoice_id,
        &creator,
        &String::from_str(&env, "Payment not received"),
        &String::from_str(&env, "evidence.json"),
    );
    assert_eq!(
        result.unwrap_err().unwrap(),
        QuickLendXError::MaintenanceModeActive,
        "create_dispute must reject during maintenance"
    );
}

// ============================================================================
// KYC MUTATIONS - Write-Gated Matrix
// ============================================================================

/// Tests that `submit_kyc_application` is blocked during maintenance.
/// **Category**: Business KYC submission mutation.
#[test]
fn test_maintenance_blocks_submit_kyc_application() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let business = Address::generate(&env);

    enable_maintenance(&env, &client, &admin, "KYC submission disabled");

    let result = client.try_submit_kyc_application(
        &business,
        &String::from_str(&env, r#"{"name":"Test","tax_id":"12345"}"#),
    );
    assert_eq!(
        result.unwrap_err().unwrap(),
        QuickLendXError::MaintenanceModeActive,
        "submit_kyc_application must reject during maintenance"
    );
}

/// Tests that `submit_investor_kyc` is blocked during maintenance.
/// **Category**: Investor KYC submission mutation.
#[test]
fn test_maintenance_blocks_submit_investor_kyc() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let investor = Address::generate(&env);

    enable_maintenance(&env, &client, &admin, "Investor KYC disabled");

    let result = client
        .try_submit_investor_kyc(&investor, &String::from_str(&env, r#"{"accredited":true}"#));
    assert_eq!(
        result.unwrap_err().unwrap(),
        QuickLendXError::MaintenanceModeActive,
        "submit_investor_kyc must reject during maintenance"
    );
}

// ============================================================================
// ADMIN MUTATIONS - Write-Gated Matrix
// ============================================================================

/// Tests that `add_currency` is blocked during maintenance.
/// **Category**: Admin configuration mutation.
#[test]
fn test_maintenance_blocks_add_currency() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let currency = Address::generate(&env);

    enable_maintenance(&env, &client, &admin, "Admin operations suspended");

    let result = client.try_add_currency(&admin, &currency);
    assert_eq!(
        result.unwrap_err().unwrap(),
        QuickLendXError::MaintenanceModeActive,
        "add_currency must reject during maintenance"
    );
}

/// Tests that `verify_business` is blocked during maintenance.
/// **Category**: Admin verification mutation.
#[test]
fn test_maintenance_blocks_verify_business() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let business = Address::generate(&env);

    enable_maintenance(&env, &client, &admin, "Business verification suspended");

    let result = client.try_verify_business(&admin, &business);
    assert_eq!(
        result.unwrap_err().unwrap(),
        QuickLendXError::MaintenanceModeActive,
        "verify_business must reject during maintenance"
    );
}

/// Tests that `verify_investor` is blocked during maintenance.
/// **Category**: Admin investor verification mutation.
#[test]
fn test_maintenance_blocks_verify_investor() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let investor = Address::generate(&env);

    enable_maintenance(&env, &client, &admin, "Investor verification suspended");

    let result = client.try_verify_investor(&admin, &investor);
    assert_eq!(
        result.unwrap_err().unwrap(),
        QuickLendXError::MaintenanceModeActive,
        "verify_investor must reject during maintenance"
    );
}

/// Tests that `set_bid_ttl_days` is blocked during maintenance.
/// **Category**: Admin configuration mutation.
#[test]
fn test_maintenance_blocks_set_bid_ttl_days() {
    let env = Env::default();
    let (client, admin) = setup(&env);

    enable_maintenance(&env, &client, &admin, "Configuration changes suspended");

    let result = client.try_set_bid_ttl_days(&admin, &30u32);
    assert_eq!(
        result.unwrap_err().unwrap(),
        QuickLendXError::MaintenanceModeActive,
        "set_bid_ttl_days must reject during maintenance"
    );
}

// ============================================================================
// READ OPERATIONS - Allowed During Maintenance
// ============================================================================

/// Tests that `get_invoice` succeeds during maintenance.
/// **Invariant**: Read operations must always be available.
#[test]
fn test_maintenance_allows_get_invoice() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let business = Address::generate(&env);
    let currency = Address::generate(&env);

    let invoice_id = make_invoice(&env, &client, &business, &currency);

    enable_maintenance(&env, &client, &admin, "Read-only mode");

    // get_invoice is a read - must succeed.
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(
        invoice.status,
        InvoiceStatus::Pending,
        "get_invoice must work during maintenance"
    );
}

/// Tests that `is_maintenance_mode` query succeeds during maintenance.
/// **Invariant**: Status queries are always available.
#[test]
fn test_maintenance_allows_is_maintenance_mode_query() {
    let env = Env::default();
    let (client, admin) = setup(&env);

    enable_maintenance(&env, &client, &admin, "Upgrade in progress");

    // Querying the flag itself must always work.
    assert!(
        client.is_maintenance_mode(),
        "is_maintenance_mode query must work during maintenance"
    );
}

/// Tests that `get_maintenance_reason` succeeds during maintenance.
/// **Invariant**: Maintenance state queries are always available.
#[test]
fn test_maintenance_allows_get_maintenance_reason_query() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let msg = "Scheduled database migration - back in 15 minutes";

    enable_maintenance(&env, &client, &admin, msg);

    let stored = client.get_maintenance_reason();
    assert_eq!(
        stored.unwrap(),
        reason(&env, msg),
        "get_maintenance_reason must work during maintenance"
    );
}

// ============================================================================
// RECOVERY AFTER DISABLE - Write Operations Resume
// ============================================================================

/// Tests that mutations succeed after disabling maintenance.
/// **Invariant**: Disabling maintenance must restore normal operations.
#[test]
fn test_mutations_resume_after_disable() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86_400;

    enable_maintenance(&env, &client, &admin, "Brief maintenance");

    // Mutation fails during maintenance.
    let fail_result = client.try_store_invoice(
        &business,
        &1_000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Test"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    assert_eq!(
        fail_result.unwrap_err().unwrap(),
        QuickLendXError::MaintenanceModeActive
    );

    // Disable maintenance.
    disable_maintenance(&env, &client, &admin);

    // Same mutation succeeds after disable.
    let success_result = client.try_store_invoice(
        &business,
        &1_000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Now allowed"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    assert!(
        success_result.is_ok(),
        "Mutations must succeed after disabling maintenance"
    );
}

// ============================================================================
// REASON ROUND-TRIP - State Consistency
// ============================================================================

/// Tests that reason string is stored and retrieved correctly.
/// **Invariant**: Reason must persist and be queryable.
#[test]
fn test_reason_round_trip_empty() {
    let env = Env::default();
    let (client, admin) = setup(&env);

    enable_maintenance(&env, &client, &admin, "");

    let retrieved = client.get_maintenance_reason();
    // Empty reason may be Some("") or None depending on implementation.
    // Most implementations clear on empty, so we check:
    if let Some(msg) = retrieved {
        assert_eq!(msg, reason(&env, ""));
    }
}

/// Tests that reason string at max length is preserved.
/// **Invariant**: Reason must support full MAX_REASON_LEN bytes.
#[test]
fn test_reason_round_trip_max_length() {
    let env = Env::default();
    let (client, admin) = setup(&env);

    // Create a reason string at maximum length.
    let max_reason: String = {
        let bytes = soroban_sdk::Bytes::from_slice(&env, &vec![b'a'; MAX_REASON_LEN as usize]);
        String::try_from_bytes(&bytes).unwrap()
    };

    enable_maintenance(&env, &client, &admin, "dummy");
    client.set_maintenance_mode(&admin, &true, &max_reason);

    let retrieved = client.get_maintenance_reason().unwrap();
    assert_eq!(
        retrieved.len(),
        MAX_REASON_LEN,
        "Max-length reason must be fully preserved"
    );
}

/// Tests that reason is cleared when maintenance is disabled.
/// **Invariant**: Disabling maintenance must clear the reason.
#[test]
fn test_reason_cleared_on_disable() {
    let env = Env::default();
    let (client, admin) = setup(&env);

    enable_maintenance(&env, &client, &admin, "Upgrade in progress");

    disable_maintenance(&env, &client, &admin);

    let retrieved = client.get_maintenance_reason();
    assert!(
        retrieved.is_none(),
        "Reason must be cleared when maintenance is disabled"
    );
}

// ============================================================================
// EDGE CASES - Boundary Conditions
// ============================================================================

/// Tests that an oversized reason is rejected with InvalidDescription.
/// **Invariant**: Reason length must be validated.
#[test]
fn test_oversized_reason_rejected() {
    let env = Env::default();
    let (client, admin) = setup(&env);

    // Create a reason one byte over the limit.
    let oversized: String = {
        let bytes =
            soroban_sdk::Bytes::from_slice(&env, &vec![b'x'; (MAX_REASON_LEN + 1) as usize]);
        String::try_from_bytes(&bytes).unwrap()
    };

    let result = client.try_set_maintenance_mode(&admin, &true, &oversized);
    assert_eq!(
        result.unwrap_err().unwrap(),
        QuickLendXError::InvalidDescription,
        "Reason exceeding MAX_REASON_LEN must be rejected with InvalidDescription"
    );
    assert!(
        !client.is_maintenance_mode(),
        "Maintenance flag must not change on rejected reason"
    );
}

/// Tests that idempotent enable (enable when already enabled) is safe.
/// **Invariant**: Toggling the same state twice must be idempotent.
#[test]
fn test_enable_when_already_enabled_is_idempotent() {
    let env = Env::default();
    let (client, admin) = setup(&env);

    enable_maintenance(&env, &client, &admin, "First reason");
    assert!(client.is_maintenance_mode());

    // Enable again with a different reason.
    client.set_maintenance_mode(&admin, &true, &reason(&env, "Updated reason"));

    assert!(client.is_maintenance_mode(), "Must remain enabled");
    assert_eq!(
        client.get_maintenance_reason().unwrap(),
        reason(&env, "Updated reason"),
        "Reason must be updated on re-enable"
    );
}

/// Tests that idempotent disable (disable when already disabled) is safe.
/// **Invariant**: Disabling when already disabled must be safe.
#[test]
fn test_disable_when_already_disabled_is_idempotent() {
    let env = Env::default();
    let (client, admin) = setup(&env);

    assert!(!client.is_maintenance_mode(), "Must start disabled");

    // Disable when already off.
    client.set_maintenance_mode(&admin, &false, &reason(&env, ""));

    assert!(
        !client.is_maintenance_mode(),
        "Must remain disabled after idempotent disable"
    );
}

/// Tests that toggling maintenance mid-test multiple times works correctly.
/// **Invariant**: Rapid toggles must maintain consistency.
#[test]
fn test_toggle_maintenance_multiple_times() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86_400;

    // Toggle 1: Enable
    enable_maintenance(&env, &client, &admin, "Cycle 1");
    let result1 = client.try_store_invoice(
        &business,
        &100i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Test1"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    assert_eq!(
        result1.unwrap_err().unwrap(),
        QuickLendXError::MaintenanceModeActive
    );

    // Toggle 2: Disable
    disable_maintenance(&env, &client, &admin);
    let result2 = client.try_store_invoice(
        &business,
        &200i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Test2"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    assert!(result2.is_ok(), "Must succeed when disabled");

    // Toggle 3: Re-enable
    enable_maintenance(&env, &client, &admin, "Cycle 2");
    let result3 = client.try_store_invoice(
        &business,
        &300i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Test3"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    assert_eq!(
        result3.unwrap_err().unwrap(),
        QuickLendXError::MaintenanceModeActive
    );

    // Toggle 4: Disable again
    disable_maintenance(&env, &client, &admin);
    let result4 = client.try_store_invoice(
        &business,
        &400i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Test4"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    assert!(result4.is_ok(), "Must succeed after final disable");
}

/// Tests that non-admin cannot toggle maintenance during maintenance.
/// **Invariant**: Authorization must be enforced even during maintenance.
#[test]
fn test_non_admin_cannot_disable_during_maintenance() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let attacker = Address::generate(&env);

    enable_maintenance(&env, &client, &admin, "Active maintenance");

    // Non-admin tries to disable.
    let result = client.try_set_maintenance_mode(&attacker, &false, &reason(&env, ""));
    assert_eq!(
        result.unwrap_err().unwrap(),
        QuickLendXError::NotAdmin,
        "Non-admin must be rejected with NotAdmin, not MaintenanceModeActive"
    );
    assert!(
        client.is_maintenance_mode(),
        "Maintenance must remain active after rejected bypass"
    );
}

// ============================================================================
// SUMMARY & COVERAGE NOTES
// ============================================================================

// **Test Execution Checklist**:
// - Run: `cargo test -p quicklendx-contracts test_maintenance_write_matrix -- --nocapture`
// - Measure: `cargo tarpaulin -p quicklendx-contracts --out Html --timeout 300`
// - Lint: `cargo clippy -p quicklendx-contracts -- -D warnings`
//
// **Expected Results**:
// - All mutations reject with MaintenanceModeActive (once guards are added).
// - All reads succeed during maintenance.
// - Reason round-trip is correct.
// - Recovery after disable works.
// - Edge cases are handled safely.
// - Coverage ≥95% of maintenance-gating paths.
//
// **Known Limitations**:
// - This matrix uses dummy IDs for some tests (e.g., invalid invoice/bid IDs).
//   The guard is tested at the entrypoint boundary; internal logic errors
//   after the guard do not invalidate the gate itself.
// - Admin bypass (ability to toggle maintenance even during maintenance) is by design
//   to allow recovery; it is tested separately.
// - Pause and maintenance are independent; this test does not cover
//   interaction with pause mode (see test_maintenance.rs for those).
