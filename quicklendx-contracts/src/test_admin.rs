//! Comprehensive test suite for hardened admin role management.
//!
//! Test Coverage:
//! 1. Initialization — admin setup, double-init prevention, authorization
//! 2. Transfer — success path, authorization, validation, atomicity
//! 3. Query Functions — get_admin, is_admin, is_initialized
//! 4. Authorization — require_admin, require_current_admin
//! 5. Security — transfer locks, concurrent operations, edge cases
//! 6. Events — initialization, transfer audit trail
//! 7. Utilities — with_admin_auth, with_current_admin
//! 8. Legacy Compatibility — set_admin routing
//!
//! Target: 95%+ test coverage for admin.rs

#[cfg(test)]
mod test_admin {
    use crate::admin::AdminStorage;
    use crate::errors::QuickLendXError;
    use crate::{QuickLendXContract, QuickLendXContractClient};
    use soroban_sdk::{
        testutils::{Address as _, Events, MockAuth, MockAuthInvoke},
        Address, Env, IntoVal, Vec,
    };

    fn setup() -> (Env, QuickLendXContractClient<'static>) {
        let env = Env::default();
        let contract_id = env.register(QuickLendXContract, ());
        let client = QuickLendXContractClient::new(&env, &contract_id);
        (env, client)
    }

    fn setup_with_admin() -> (Env, QuickLendXContractClient<'static>, Address) {
        let (env, client) = setup();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        client.initialize_admin(&admin);
        (env, client, admin)
    }

    // ============================================================================
    // 1. Initialization Tests
    // ============================================================================

    #[test]
    fn test_initialize_admin_succeeds() {
        let (env, client) = setup();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let result = client.try_initialize_admin(&admin);

        assert!(result.is_ok(), "First initialization must succeed");
        assert_eq!(
            client.get_current_admin(),
            Some(admin.clone()),
            "Stored admin must match initialized address"
        );
        assert!(
            AdminStorage::is_initialized(&env),
            "Admin system must be marked as initialized"
        );
    }

    #[test]
    fn test_initialize_admin_requires_authorization() {
        let env = Env::default();
        let contract_id = env.register(QuickLendXContract, ());
        let client = QuickLendXContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);

        // Should panic without authorization
        let result = panic::catch_unwind(|| {
            client.initialize_admin(&admin);
        });
        assert!(result.is_err(), "Initialization without auth must fail");
    }

    #[test]
    fn test_initialize_admin_double_init_fails() {
        let (env, client) = setup();
        env.mock_all_auths();

        let admin1 = Address::generate(&env);
        let admin2 = Address::generate(&env);

        // First initialization succeeds
        client.initialize_admin(&admin1);

        // Second initialization fails
        let result = client.try_initialize_admin(&admin2);
        assert!(result.is_err(), "Double initialization must be rejected");

        // Original admin remains
        assert_eq!(
            client.get_current_admin(),
            Some(admin1),
            "Original admin must remain after failed re-init"
        );
    }

    #[test]
    fn test_initialize_admin_same_address_twice_fails() {
        let (env, client) = setup();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        client.initialize_admin(&admin);

        let result = client.try_initialize_admin(&admin);
        assert!(
            result.is_err(),
            "Re-initializing with same address must fail"
        );
    }

    #[test]
    fn test_initialize_admin_emits_event() {
        let (env, client) = setup();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        client.initialize_admin(&admin);

        let events = env.events().all();
        assert!(!events.is_empty(), "Initialization must emit event");

        let event = &events[0];
        assert_eq!(event.0, (soroban_sdk::symbol_short!("adm_init"),));
    }

    // ============================================================================
    // 2. Admin Transfer Tests
    // ============================================================================

    #[test]
    fn test_transfer_admin_succeeds() {
        let (env, client, admin1) = setup_with_admin();
        let admin2 = Address::generate(&env);

        let result = client.try_transfer_admin(&admin1, &admin2);
        assert!(result.is_ok(), "Admin transfer must succeed");

        assert_eq!(
            client.get_current_admin(),
            Some(admin2),
            "New admin must be stored"
        );
        assert!(
            !AdminStorage::is_admin(&env, &admin1),
            "Old admin must no longer be admin"
        );
        assert!(
            AdminStorage::is_admin(&env, &admin2),
            "New admin must be recognized"
        );
    }

    #[test]
    fn test_transfer_admin_requires_current_admin_auth() {
        let (env, client, admin1) = setup_with_admin();
        let admin2 = Address::generate(&env);
        let non_admin = Address::generate(&env);

        // Non-admin cannot transfer
        let result = client.try_transfer_admin(&non_admin, &admin2);
        assert!(result.is_err(), "Non-admin transfer must fail");

        // Admin remains unchanged
        assert_eq!(
            client.get_current_admin(),
            Some(admin1),
            "Admin must remain unchanged after failed transfer"
        );
    }

    #[test]
    fn test_transfer_admin_to_self_fails() {
        let (env, client, admin) = setup_with_admin();

        let result = client.try_transfer_admin(&admin, &admin);
        assert!(result.is_err(), "Transfer to self must fail");

        assert_eq!(
            client.get_current_admin(),
            Some(admin),
            "Admin must remain unchanged"
        );
    }

    #[test]
    fn test_transfer_admin_without_initialization_fails() {
        let (env, client) = setup();
        env.mock_all_auths();

        let admin1 = Address::generate(&env);
        let admin2 = Address::generate(&env);

        let result = client.try_transfer_admin(&admin1, &admin2);
        assert!(result.is_err(), "Transfer without initialization must fail");
    }

    #[test]
    fn test_transfer_admin_emits_event() {
        let (env, client, admin1) = setup_with_admin();
        let admin2 = Address::generate(&env);

        client.transfer_admin(&admin1, &admin2);

        let events = env.events().all();
        let transfer_events: Vec<_> = events
            .iter()
            .filter(|e| e.0 == (soroban_sdk::symbol_short!("adm_trf"),))
            .collect();

        assert!(!transfer_events.is_empty(), "Transfer must emit event");
    }

    #[test]
    fn test_transfer_admin_chain() {
        let (env, client, admin1) = setup_with_admin();
        let admin2 = Address::generate(&env);
        let admin3 = Address::generate(&env);

        // Transfer from admin1 to admin2
        client.transfer_admin(&admin1, &admin2);
        assert_eq!(client.get_current_admin(), Some(admin2.clone()));

        // Transfer from admin2 to admin3
        client.transfer_admin(&admin2, &admin3);
        assert_eq!(client.get_current_admin(), Some(admin3));

        // admin1 can no longer transfer
        let result = client.try_transfer_admin(&admin1, &admin2);
        assert!(result.is_err(), "Old admin cannot transfer");
    }

    // ============================================================================
    // 3. Query Function Tests
    // ============================================================================

    #[test]
    fn test_get_admin_before_initialization() {
        let (env, client) = setup();

        assert_eq!(
            client.get_current_admin(),
            None,
            "Admin must be None before initialization"
        );
        assert!(
            !AdminStorage::is_initialized(&env),
            "System must not be initialized"
        );
    }

    #[test]
    fn test_get_admin_after_initialization() {
        let (env, client, admin) = setup_with_admin();

        assert_eq!(
            client.get_current_admin(),
            Some(admin.clone()),
            "Admin must be returned after initialization"
        );
        assert!(
            AdminStorage::is_initialized(&env),
            "System must be initialized"
        );
    }

    #[test]
    fn test_is_admin_checks() {
        let (env, client, admin) = setup_with_admin();
        let non_admin = Address::generate(&env);

        assert!(
            AdminStorage::is_admin(&env, &admin),
            "Admin address must return true"
        );
        assert!(
            !AdminStorage::is_admin(&env, &non_admin),
            "Non-admin address must return false"
        );
    }

    #[test]
    fn test_is_admin_before_initialization() {
        let (env, _client) = setup();
        let address = Address::generate(&env);

        assert!(
            !AdminStorage::is_admin(&env, &address),
            "No address should be admin before initialization"
        );
    }

    // ============================================================================
    // 4. Authorization Tests
    // ============================================================================

    #[test]
    fn test_require_admin_succeeds_for_admin() {
        let (env, _client, admin) = setup_with_admin();

        let result = AdminStorage::require_admin(&env, &admin);
        assert!(result.is_ok(), "require_admin must succeed for admin");
    }

    #[test]
    fn test_require_admin_fails_for_non_admin() {
        let (env, _client, _admin) = setup_with_admin();
        let non_admin = Address::generate(&env);

        let result = AdminStorage::require_admin(&env, &non_admin);
        assert_eq!(
            result,
            Err(QuickLendXError::NotAdmin),
            "require_admin must fail for non-admin"
        );
    }

    #[test]
    fn test_require_admin_fails_before_initialization() {
        let (env, _client) = setup();
        let address = Address::generate(&env);

        let result = AdminStorage::require_admin(&env, &address);
        assert_eq!(
            result,
            Err(QuickLendXError::OperationNotAllowed),
            "require_admin must fail before initialization"
        );
    }

    #[test]
    fn test_require_current_admin_succeeds() {
        let (env, _client, admin) = setup_with_admin();

        let result = AdminStorage::require_current_admin(&env);
        assert!(result.is_ok(), "require_current_admin must succeed");
        assert_eq!(result.unwrap(), admin, "Must return correct admin address");
    }

    #[test]
    fn test_require_current_admin_fails_before_initialization() {
        let (env, _client) = setup();

        let result = AdminStorage::require_current_admin(&env);
        assert_eq!(
            result,
            Err(QuickLendXError::OperationNotAllowed),
            "require_current_admin must fail before initialization"
        );
    }

    // ============================================================================
    // 5. Security Tests
    // ============================================================================

    #[test]
    fn test_admin_operations_atomic() {
        let (env, client, admin1) = setup_with_admin();
        let admin2 = Address::generate(&env);

        // Verify atomicity by checking state before and after
        assert!(AdminStorage::is_admin(&env, &admin1));
        assert!(!AdminStorage::is_admin(&env, &admin2));

        client.transfer_admin(&admin1, &admin2);

        // State should be completely switched
        assert!(!AdminStorage::is_admin(&env, &admin1));
        assert!(AdminStorage::is_admin(&env, &admin2));
    }

    #[test]
    fn test_initialization_state_consistency() {
        let (env, client) = setup();
        env.mock_all_auths();

        // Before initialization
        assert!(!AdminStorage::is_initialized(&env));
        assert_eq!(AdminStorage::get_admin(&env), None);

        let admin = Address::generate(&env);
        client.initialize_admin(&admin);

        // After initialization
        assert!(AdminStorage::is_initialized(&env));
        assert_eq!(AdminStorage::get_admin(&env), Some(admin));
    }

    // ============================================================================
    // 6. Utility Function Tests
    // ============================================================================

    #[test]
    fn test_with_admin_auth_succeeds() {
        let (env, _client, admin) = setup_with_admin();

        let result = AdminStorage::with_admin_auth(&env, &admin, || Ok("success".to_string()));

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "success");
    }

    #[test]
    fn test_with_admin_auth_fails_for_non_admin() {
        let (env, _client, _admin) = setup_with_admin();
        let non_admin = Address::generate(&env);

        let result = AdminStorage::with_admin_auth(&env, &non_admin, || Ok("should not execute"));

        assert_eq!(result, Err(QuickLendXError::NotAdmin));
    }

    #[test]
    fn test_with_current_admin_succeeds() {
        let (env, _client, admin) = setup_with_admin();

        let result = AdminStorage::with_current_admin(&env, |current_admin| {
            assert_eq!(current_admin, &admin);
            Ok("success")
        });

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "success");
    }

    #[test]
    fn test_with_current_admin_fails_before_initialization() {
        let (env, _client) = setup();

        let result = AdminStorage::with_current_admin(&env, |_| Ok("should not execute"));

        assert_eq!(result, Err(QuickLendXError::OperationNotAllowed));
    }

    // ============================================================================
    // 7. Legacy Compatibility Tests
    // ============================================================================

    #[test]
    fn test_set_admin_routes_to_initialize() {
        let (env, client) = setup();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let result = client.try_set_admin(&admin);

        assert!(result.is_ok(), "set_admin must route to initialize");
        assert_eq!(client.get_current_admin(), Some(admin));
        assert!(AdminStorage::is_initialized(&env));
    }

    #[test]
    fn test_set_admin_routes_to_transfer() {
        let (env, client, admin1) = setup_with_admin();
        let admin2 = Address::generate(&env);

        let result = client.try_set_admin(&admin2);

        assert!(result.is_ok(), "set_admin must route to transfer");
        assert_eq!(client.get_current_admin(), Some(admin2));
    }

    #[test]
    fn test_set_admin_rejects_spoofed_current_admin_signature() {
        let env = Env::default();
        let contract_id = env.register(QuickLendXContract, ());
        let client = QuickLendXContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let attacker = Address::generate(&env);
        let replacement = Address::generate(&env);

        client.mock_all_auths().initialize_admin(&admin);

        let spoofed_auth = MockAuth {
            address: &attacker,
            invoke: &MockAuthInvoke {
                contract: &contract_id,
                fn_name: "set_admin",
                args: (replacement.clone(),).into_val(&env),
                sub_invokes: &[],
            },
        };

        let result = client.mock_auths(&[spoofed_auth]).try_set_admin(&replacement);
        let invoke_err = result
            .err()
            .expect("spoofed transfer must fail")
            .err()
            .expect("spoofed transfer must fail at auth");
        assert_eq!(invoke_err, soroban_sdk::InvokeError::Abort);
        assert_eq!(client.get_current_admin(), Some(admin));
    }

    // ============================================================================
    // 8. Edge Cases and Error Conditions
    // ============================================================================

    #[test]
    fn test_multiple_rapid_transfers() {
        let (env, client, mut current_admin) = setup_with_admin();

        // Perform multiple transfers in sequence
        for i in 0..5 {
            let new_admin = Address::generate(&env);
            client.transfer_admin(&current_admin, &new_admin);

            assert_eq!(
                client.get_current_admin(),
                Some(new_admin.clone()),
                "Transfer {} must succeed",
                i
            );
            current_admin = new_admin;
        }
    }

    #[test]
    fn test_admin_state_after_failed_operations() {
        let (env, client, admin) = setup_with_admin();
        let non_admin = Address::generate(&env);

        // Failed transfer should not change state
        let _result = client.try_transfer_admin(&non_admin, &admin);
        assert_eq!(
            client.get_current_admin(),
            Some(admin),
            "Failed transfer must not change admin"
        );

        // Failed initialization should not change state
        let _result = client.try_initialize_admin(&non_admin);
        assert_eq!(
            client.get_current_admin(),
            Some(admin),
            "Failed re-initialization must not change admin"
        );
    }

    #[test]
    fn test_event_emission_consistency() {
        let (env, client) = setup();
        env.mock_all_auths();

        let admin1 = Address::generate(&env);
        let admin2 = Address::generate(&env);

        // Initialize and transfer
        client.initialize_admin(&admin1);
        client.transfer_admin(&admin1, &admin2);

        let events = env.events().all();

        // Should have initialization and transfer events
        let init_events: Vec<_> = events
            .iter()
            .filter(|e| e.0 == (soroban_sdk::symbol_short!("adm_init"),))
            .collect();
        let transfer_events: Vec<_> = events
            .iter()
            .filter(|e| e.0 == (soroban_sdk::symbol_short!("adm_trf"),))
            .collect();

        assert_eq!(init_events.len(), 1, "Must have one init event");
        assert_eq!(transfer_events.len(), 1, "Must have one transfer event");
    }

    // ============================================================================
    // 9. Integration Tests
    // ============================================================================

    #[test]
    fn test_full_admin_lifecycle() {
        let (env, client) = setup();
        env.mock_all_auths();

        // 1. Initial state
        assert!(!AdminStorage::is_initialized(&env));
        assert_eq!(AdminStorage::get_admin(&env), None);

        // 2. Initialize admin
        let admin1 = Address::generate(&env);
        client.initialize_admin(&admin1);
        assert!(AdminStorage::is_initialized(&env));
        assert_eq!(AdminStorage::get_admin(&env), Some(admin1.clone()));

        // 3. Transfer admin
        let admin2 = Address::generate(&env);
        client.transfer_admin(&admin1, &admin2);
        assert_eq!(AdminStorage::get_admin(&env), Some(admin2.clone()));

        // 4. Verify old admin cannot operate
        let admin3 = Address::generate(&env);
        let result = client.try_transfer_admin(&admin1, &admin3);
        assert!(result.is_err());

        // 5. Verify new admin can operate
        client.transfer_admin(&admin2, &admin3);
        assert_eq!(AdminStorage::get_admin(&env), Some(admin3));
    }
}
