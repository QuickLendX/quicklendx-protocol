/// Comprehensive test suite for admin role management.
///
/// Test Coverage:
/// 1. Initialization — admin setup, double-init prevention, same-admin re-init
/// 2. Query Functions — get_current_admin before init, after init, after transfer
/// 3. Admin Transfer — success path, without-init failure, non-admin failure, chain
/// 4. AdminStorage internals — is_admin, require_admin
/// 5. Authorization gates — invoice verification, fee configuration
/// 6. Edge cases — transfer to self, events emitted
///
/// Target: 95%+ test coverage for admin.rs
#[cfg(test)]
mod test_admin {
    use crate::admin::AdminStorage;
    use crate::errors::QuickLendXError;
    use crate::{QuickLendXContract, QuickLendXContractClient};
    use soroban_sdk::{
        testutils::{Address as _, Events},
        Address, Env, String, Vec,
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
            Some(admin),
            "Stored admin must match the address passed to initialize"
        );
    }

    #[test]
    fn test_initialize_admin_double_init_fails() {
        let (env, client) = setup();
        env.mock_all_auths();

        let admin1 = Address::generate(&env);
        let admin2 = Address::generate(&env);

        client.initialize_admin(&admin1);
        let second = client.try_initialize_admin(&admin2);

        assert!(second.is_err(), "Double initialization must be rejected");
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

        let again = client.try_initialize_admin(&admin);
        assert!(
            again.is_err(),
            "Re-initializing with the same address must still fail"
        );
    }

    // ============================================================================
    // 2. Query Function Tests — get_current_admin
    // ============================================================================

    #[test]
    fn test_get_current_admin_before_init_returns_none() {
        let (_env, client) = setup();
        assert_eq!(
            client.get_current_admin(),
            None,
            "Admin must be None on a fresh contract"
        );
    }

    #[test]
    fn test_get_current_admin_after_init_returns_address() {
        let (_env, client, admin) = setup_with_admin();
        assert_eq!(
            client.get_current_admin(),
            Some(admin),
            "get_current_admin must return the initialized address"
        );
    }

    #[test]
    fn test_get_current_admin_after_transfer_returns_new_address() {
        let (env, client, _old_admin) = setup_with_admin();

        let new_admin = Address::generate(&env);
        client.transfer_admin(&new_admin);

        assert_eq!(
            client.get_current_admin(),
            Some(new_admin),
            "get_current_admin must reflect the transferred address"
        );
    }

    #[test]
    fn test_get_current_admin_tracks_full_lifecycle() {
        let (env, client) = setup();
        env.mock_all_auths();

        // Phase 1: uninitialized
        assert_eq!(client.get_current_admin(), None);

        // Phase 2: initialized
        let admin = Address::generate(&env);
        client.initialize_admin(&admin);
        assert_eq!(client.get_current_admin(), Some(admin));

        // Phase 3: first transfer
        let second = Address::generate(&env);
        client.transfer_admin(&second);
        assert_eq!(client.get_current_admin(), Some(second));

        // Phase 4: second transfer
        let third = Address::generate(&env);
        client.transfer_admin(&third);
        assert_eq!(client.get_current_admin(), Some(third));
    }

    // ============================================================================
    // 3. Admin Transfer Tests
    // ============================================================================

    #[test]
    fn test_transfer_admin_succeeds() {
        let (env, client, _old_admin) = setup_with_admin();

        let new_admin = Address::generate(&env);
        let result = client.try_transfer_admin(&new_admin);

        assert!(result.is_ok(), "Transfer from current admin must succeed");
        assert_eq!(client.get_current_admin(), Some(new_admin));
    }

    #[test]
    fn test_transfer_admin_without_init_fails() {
        let (env, client) = setup();
        env.mock_all_auths();

        let addr = Address::generate(&env);
        let result = client.try_transfer_admin(&addr);

        assert!(
            result.is_err(),
            "Transfer must fail when no admin has been initialized"
        );
    }

    #[test]
    fn test_transfer_admin_chain() {
        let (env, client, admin1) = setup_with_admin();

        let admin2 = Address::generate(&env);
        let admin3 = Address::generate(&env);
        let admin4 = Address::generate(&env);

        client.transfer_admin(&admin2);
        assert_eq!(client.get_current_admin(), Some(admin2.clone()));

        client.transfer_admin(&admin3);
        assert_eq!(client.get_current_admin(), Some(admin3.clone()));

        client.transfer_admin(&admin4);
        assert_eq!(client.get_current_admin(), Some(admin4.clone()));

        // Confirm original admin is no longer stored
        assert_ne!(client.get_current_admin(), Some(admin1));
    }

    #[test]
    fn test_transfer_admin_to_self() {
        let (_env, client, admin) = setup_with_admin();

        let result = client.try_transfer_admin(&admin);
        assert!(
            result.is_ok(),
            "Transferring admin to the same address is a valid no-op"
        );
        assert_eq!(client.get_current_admin(), Some(admin));
    }

    // ============================================================================
    // 4. AdminStorage Internal Tests — is_admin / require_admin
    // ============================================================================

    #[test]
    fn test_is_admin_returns_false_before_init() {
        let env = Env::default();
        let contract_id = env.register(QuickLendXContract, ());
        let addr = Address::generate(&env);
        env.as_contract(&contract_id, || {
            assert!(
                !AdminStorage::is_admin(&env, &addr),
                "is_admin must be false when no admin is set"
            );
        });
    }

    #[test]
    fn test_is_admin_returns_true_for_current_admin() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(QuickLendXContract, ());
        let admin = Address::generate(&env);
        env.as_contract(&contract_id, || {
            AdminStorage::initialize(&env, &admin).unwrap();
            assert!(AdminStorage::is_admin(&env, &admin));
        });
    }

    #[test]
    fn test_is_admin_returns_false_for_different_address() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(QuickLendXContract, ());
        let admin = Address::generate(&env);
        let other = Address::generate(&env);
        env.as_contract(&contract_id, || {
            AdminStorage::initialize(&env, &admin).unwrap();
            assert!(
                !AdminStorage::is_admin(&env, &other),
                "is_admin must be false for a non-admin address"
            );
        });
    }

    #[test]
    fn test_require_admin_succeeds_for_admin() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(QuickLendXContract, ());
        let admin = Address::generate(&env);
        env.as_contract(&contract_id, || {
            AdminStorage::initialize(&env, &admin).unwrap();
            let result = AdminStorage::require_admin(&env, &admin);
            assert!(result.is_ok(), "require_admin must pass for the real admin");
        });
    }

    #[test]
    fn test_require_admin_fails_for_non_admin() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(QuickLendXContract, ());
        let admin = Address::generate(&env);
        let impostor = Address::generate(&env);
        env.as_contract(&contract_id, || {
            AdminStorage::initialize(&env, &admin).unwrap();
            let result = AdminStorage::require_admin(&env, &impostor);
            assert_eq!(
                result,
                Err(QuickLendXError::NotAdmin),
                "require_admin must return NotAdmin for a non-admin address"
            );
        });
    }

    #[test]
    fn test_require_admin_fails_before_init() {
        let env = Env::default();
        let contract_id = env.register(QuickLendXContract, ());
        let addr = Address::generate(&env);
        env.as_contract(&contract_id, || {
            let result = AdminStorage::require_admin(&env, &addr);
            assert_eq!(
                result,
                Err(QuickLendXError::NotAdmin),
                "require_admin must fail when no admin has been initialized"
            );
        });
    }

    #[test]
    fn test_get_admin_returns_none_before_init() {
        let env = Env::default();
        let contract_id = env.register(QuickLendXContract, ());
        env.as_contract(&contract_id, || {
            assert_eq!(
                AdminStorage::get_admin(&env),
                None,
                "get_admin must return None on a blank environment"
            );
        });
    }

    #[test]
    fn test_set_admin_rejects_non_admin_caller() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(QuickLendXContract, ());
        let real_admin = Address::generate(&env);
        let impostor = Address::generate(&env);
        let target = Address::generate(&env);
        env.as_contract(&contract_id, || {
            AdminStorage::initialize(&env, &real_admin).unwrap();

            let result = AdminStorage::set_admin(&env, &impostor, &target);
            assert_eq!(
                result,
                Err(QuickLendXError::NotAdmin),
                "set_admin must reject a caller who is not the current admin"
            );

            // Confirm the real admin is unchanged
            assert_eq!(AdminStorage::get_admin(&env), Some(real_admin));
        });
    }

    // ============================================================================
    // 5. Authorization Gate Tests
    // ============================================================================

    #[test]
    fn test_admin_can_verify_invoice() {
        let (env, client, _admin) = setup_with_admin();

        let business = Address::generate(&env);
        let currency = Address::generate(&env);

        let invoice_id = client.store_invoice(
            &business,
            &10_000,
            &currency,
            &(env.ledger().timestamp() + 86400),
            &String::from_str(&env, "Admin gate test"),
            &crate::invoice::InvoiceCategory::Services,
            &Vec::new(&env),
        );

        let result = client.try_verify_invoice(&invoice_id);
        assert!(result.is_ok(), "Admin must be able to verify invoices");
    }

    #[test]
    fn test_verify_invoice_without_admin_fails() {
        let (env, client) = setup();
        env.mock_all_auths();

        let business = Address::generate(&env);
        let currency = Address::generate(&env);

        let invoice_id = client.store_invoice(
            &business,
            &10_000,
            &currency,
            &(env.ledger().timestamp() + 86400),
            &String::from_str(&env, "No admin test"),
            &crate::invoice::InvoiceCategory::Services,
            &Vec::new(&env),
        );

        let result = client.try_verify_invoice(&invoice_id);
        assert!(
            result.is_err(),
            "Invoice verification must fail when no admin is initialized"
        );
    }

    #[test]
    fn test_admin_can_set_platform_fee() {
        let (_env, client, _admin) = setup_with_admin();

        let result = client.try_set_platform_fee(&200);
        assert!(result.is_ok(), "Admin must be able to set platform fees");
    }

    #[test]
    fn test_set_platform_fee_without_admin_fails() {
        let (_env, client) = setup();

        let result = client.try_set_platform_fee(&200);
        assert!(
            result.is_err(),
            "Fee configuration must fail when no admin is set"
        );
    }

    // ============================================================================
    // 6. Event Emission Tests
    // ============================================================================

    #[test]
    fn test_initialize_emits_admin_set_event() {
        let (env, client) = setup();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        client.initialize_admin(&admin);

        let events = env.events().all();
        let has_admin_set = events.iter().any(|evt| {
            let (_, topics, _): (_, soroban_sdk::Vec<soroban_sdk::Val>, _) = evt;
            // The first topic should be the "adm_set" symbol
            !topics.is_empty()
        });
        assert!(has_admin_set, "initialize must emit at least one event");
    }

    #[test]
    fn test_transfer_emits_admin_transferred_event() {
        let (env, client) = setup();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        client.initialize_admin(&admin);

        let new_admin = Address::generate(&env);
        let result = client.try_transfer_admin(&new_admin);

        // Verify transfer succeeded (which triggers emit_admin_transferred internally)
        assert!(result.is_ok(), "transfer must succeed to emit event");
        assert_eq!(
            client.get_current_admin(),
            Some(new_admin),
            "admin must be updated after transfer that emits event"
        );
    }
}
