use soroban_sdk::{testutils::Address as _, vec, Address, Env, String};

use crate::{
    errors::QuickLendXError,
    invoice::{InvoiceCategory, InvoiceStatus},
    pause,
    test_pause::setup_contract_with_admin,
    QuickLendXContract, QuickLendXContractClient,
};

/// Helper to create a test environment with a paused contract.
fn setup_paused_contract() -> (Env, QuickLendXContractClient<'static>, Address, Address) {
    let (env, client, admin, business) = setup_contract_with_admin();
    client.pause(&admin);
    assert!(client.is_paused());
    (env, client, admin, business)
}

/// Helper to create a verified investor for testing.
fn setup_verified_investor(env: &Env, client: &QuickLendXContractClient, admin: &Address) -> Address {
    let investor = Address::generate(env);
    client.submit_investor_kyc(&investor, &String::from_str(env, "kyc-data"));
    client.verify_investor(&investor, &1_000_000i128);
    investor
}

// ============================================================================
// Determinism Tests
// ============================================================================

#[test]
fn test_pause_behavior_is_deterministic() {
    let (env, client, admin, business) = setup_contract_with_admin();

    // Cycle pause/unpause multiple times and verify consistent behavior
    for _ in 0..3 {
        assert!(!client.is_paused());

        // Store invoice succeeds when unpaused
        let invoice_id = client
            .store_invoice(
                &business,
                &1000i128,
                &Address::generate(&env),
                &env.ledger().timestamp().saturating_add(86400),
                &String::from_str(&env, "Test invoice"),
                &InvoiceCategory::Services,
                &vec![&env],
            )
            .unwrap();

        // Pause
        client.pause(&admin);
        assert!(client.is_paused());

        // Same operation fails deterministically with ContractPaused
        let result = client.try_store_invoice(
            &business,
            &1000i128,
            &Address::generate(&env),
            &env.ledger().timestamp().saturating_add(86400),
            &String::from_str(&env, "Test invoice 2"),
            &InvoiceCategory::Services,
            &vec![&env],
        );
        assert!(result.is_err());

        // Unpause
        client.unpause(&admin);
        assert!(!client.is_paused());

        // Operation succeeds again
        let invoice_id_2 = client
            .store_invoice(
                &business,
                &1000i128,
                &Address::generate(&env),
                &env.ledger().timestamp().saturating_add(86400),
                &String::from_str(&env, "Test invoice 3"),
                &InvoiceCategory::Services,
                &vec![&env],
            )
            .unwrap();

        assert_ne!(invoice_id, invoice_id_2);
    }
}

// ============================================================================
// No-Bypass Security Tests
// ============================================================================

#[test]
fn test_no_bypass_via_internal_functions() {
    let (_env, client, _admin, business) = setup_paused_contract();

    // Direct store_invoice call is blocked
    let result = client.try_store_invoice(
        &business,
        &1000i128,
        &Address::generate(&_env),
        &_env.ledger().timestamp().saturating_add(86400),
        &String::from_str(&_env, "Test"),
        &InvoiceCategory::Services,
        &vec![&_env],
    );
    assert!(result.is_err(), "store_invoice must be blocked when paused");
}

#[test]
fn test_all_user_mutating_flows_blocked() {
    let (env, client, admin, business) = setup_paused_contract();
    let investor = setup_verified_investor(&env, &client, &admin);
    let token = Address::generate(&env);

    // Create an invoice while unpaused for use in blocked-operation tests
    client.unpause(&admin);
    let invoice_id = client
        .store_invoice(
            &business,
            &1000i128,
            &token,
            &env.ledger().timestamp().saturating_add(86400),
            &String::from_str(&env, "Test invoice"),
            &InvoiceCategory::Services,
            &vec![&env],
        )
        .unwrap();
    client.verify_invoice(&invoice_id);
    client.pause(&admin);

    // 1. Invoice creation
    assert!(
        client
            .try_store_invoice(
                &business,
                &1000i128,
                &token,
                &env.ledger().timestamp().saturating_add(86400),
                &String::from_str(&env, "Test"),
                &InvoiceCategory::Services,
                &vec![&env],
            )
            .is_err(),
        "store_invoice blocked"
    );

    // 2. Bid placement
    assert!(
        client
            .try_place_bid(&investor, &invoice_id, &500i128, &600i128)
            .is_err(),
        "place_bid blocked"
    );

    // 3. Invoice settlement
    assert!(
        client
            .try_settle_invoice(&invoice_id, &1000i128)
            .is_err(),
        "settle_invoice blocked"
    );

    // 4. KYC submission
    let new_business = Address::generate(&env);
    assert!(
        client
            .try_submit_kyc_application(&new_business, &String::from_str(&env, "kyc"))
            .is_err(),
        "submit_kyc_application blocked"
    );

    // 5. Dispute creation
    assert!(
        client
            .try_create_dispute(
                &invoice_id,
                &business,
                &String::from_str(&env, "reason"),
                &String::from_str(&env, "evidence"),
            )
            .is_err(),
        "create_dispute blocked"
    );

    // 6. Fund invoice
    assert!(
        client
            .try_fund_invoice(&investor, &invoice_id, &500i128)
            .is_err(),
        "fund_invoice blocked"
    );

    // 7. Update invoice category
    assert!(
        client
            .try_update_invoice_category(&invoice_id, &InvoiceCategory::Goods)
            .is_err(),
        "update_invoice_category blocked"
    );

    // 8. Add invoice tag
    assert!(
        client
            .try_add_invoice_tag(&invoice_id, &String::from_str(&env, "tag"))
            .is_err(),
        "add_invoice_tag blocked"
    );
}

// ============================================================================
// Recovery Flow Tests (allowed during pause)
// ============================================================================

#[test]
fn test_admin_recovery_flows_allowed_during_pause() {
    let (env, client, admin, _business) = setup_paused_contract();
    let token = Address::generate(&env);
    let target = Address::generate(&env);

    // 1. Emergency withdraw initiation works while paused
    client
        .try_initiate_emergency_withdraw(&admin, &token, &100i128, &target)
        .expect("initiate_emergency_withdraw must work during pause");

    // 2. Cancel emergency withdraw works while paused
    client
        .try_cancel_emergency_withdraw(&admin)
        .expect("cancel_emergency_withdraw must work during pause");

    // 3. Unpause works (admin recovery)
    client
        .try_unpause(&admin)
        .expect("unpause must work during pause");
    assert!(!client.is_paused());
}

#[test]
fn test_admin_governance_allowed_during_pause() {
    let (env, client, admin, _business) = setup_paused_contract();

    // Verify business works during pause
    let business = Address::generate(&env);
    client
        .try_verify_business(&admin, &business)
        .expect("verify_business must work during pause");

    // Verify investor works during pause
    let investor = Address::generate(&env);
    client.submit_investor_kyc(&investor, &String::from_str(&env, "kyc"));
    client
        .try_verify_investor(&investor, &1_000_000i128)
        .expect("verify_investor must work during pause");

    // Currency management works during pause
    let currency = Address::generate(&env);
    client
        .try_add_currency(&admin, &currency)
        .expect("add_currency must work during pause");
}

#[test]
fn test_query_functions_always_allowed() {
    let (env, client, _admin, business) = setup_paused_contract();

    // All getters should work regardless of pause state
    client.get_current_admin();
    client.is_paused();
    client.get_bid_ttl_days();
    client.get_whitelisted_currencies();
    client.currency_count();
    client.get_total_invoice_count();
    client.get_available_invoices();
    client.get_invoice_by_business(&business);
    client.get_platform_fee();
    client.get_pending_businesses();
    client.get_verified_businesses();
    client.get_pending_investors();
    client.get_verified_investors();
}

// ============================================================================
// Emergency Withdraw + Pause Interaction Tests
// ============================================================================

#[test]
fn test_emergency_withdraw_full_lifecycle_during_pause() {
    let (env, client, admin, _business) = setup_paused_contract();
    let token = Address::generate(&env);
    let target = Address::generate(&env);

    // Initiate while paused
    client.initiate_emergency_withdraw(&admin, &token, &1000i128, &target);

    let pending = client.get_pending_emergency_withdraw().unwrap();
    assert_eq!(pending.amount, 1000i128);
    assert_eq!(pending.target_address, target);
    assert!(!pending.cancelled);

    // Advance ledger past timelock
    env.ledger().set_timestamp(env.ledger().timestamp() + 100_000);

    // Execute while paused
    client.execute_emergency_withdraw(&admin);

    // After execution, pending is cleared
    assert!(client.get_pending_emergency_withdraw().is_none());
}

#[test]
fn test_emergency_withdraw_cancel_during_pause() {
    let (env, client, admin, _business) = setup_paused_contract();
    let token = Address::generate(&env);
    let target = Address::generate(&env);

    // Initiate while paused
    client.initiate_emergency_withdraw(&admin, &token, &1000i128, &target);
    let nonce_before = client.get_pending_emergency_withdraw().unwrap().nonce;

    // Cancel while paused
    client.cancel_emergency_withdraw(&admin);

    let pending_after = client.get_pending_emergency_withdraw().unwrap();
    assert!(pending_after.cancelled);
    assert_eq!(pending_after.nonce, nonce_before);
}

// ============================================================================
// Coexistence with Maintenance Mode
// ============================================================================

#[test]
fn test_pause_and_maintenance_are_independent() {
    let (env, client, admin, business) = setup_contract_with_admin();

    // Enable maintenance mode
    env.storage().instance().set(&crate::maintenance::MAINTENANCE_MODE_KEY, &true);

    // Pause should still work independently
    client.pause(&admin);
    assert!(client.is_paused());

    // Unpause should work
    client.unpause(&admin);
    assert!(!client.is_paused());
}

// ============================================================================
// Edge Case: Re-pausing an already paused contract
// ============================================================================

#[test]
fn test_double_pause_is_idempotent() {
    let (_env, client, admin, _business) = setup_paused_contract();

    // Second pause should succeed (idempotent)
    client.pause(&admin);
    assert!(client.is_paused());
}

#[test]
fn test_double_unpause_is_idempotent() {
    let (env, client, admin, _business) = setup_contract_with_admin();

    assert!(!client.is_paused());
    client.pause(&admin);
    client.unpause(&admin);

    // Second unpause should succeed (idempotent)
    client.unpause(&admin);
    assert!(!client.is_paused());
}

