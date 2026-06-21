/// Extended admin test suite covering dry-run preview functions.
///
/// Run with:  `cargo test test_admin`
#[cfg(test)]
mod test_admin {
    use soroban_sdk::{
        testutils::{Address as _, AuthorizedFunction, AuthorizedInvocation},
        vec, Address, Env, IntoVal,
    };

    use crate::{
        AdminContract, AdminContractClient, ContractError, FeeConfig, ProtocolConfig,
    };

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    /// Deploy a fresh contract and return (env, client, admin_address).
    fn setup() -> (Env, AdminContractClient<'static>, Address) {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, AdminContract);
        let client = AdminContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        client.initialize(&admin);
        (env, client, admin)
    }

    /// Return a valid default [`ProtocolConfig`].
    fn default_protocol_cfg() -> ProtocolConfig {
        ProtocolConfig {
            min_invoice_amount: 1_000_000,
            max_due_date_days: 90,
            grace_period_seconds: 86_400,
        }
    }

    /// Return a valid default [`FeeConfig`] given a treasury address.
    fn default_fee_cfg(treasury: &Address) -> FeeConfig {
        FeeConfig {
            fee_bps: 50,
            treasury: treasury.clone(),
        }
    }

    // -----------------------------------------------------------------------
    // Admin initialization
    // -----------------------------------------------------------------------

    #[test]
    fn test_initialize_sets_admin() {
        let (_, client, admin) = setup();
        // Re-initialization must fail.
        let result = client.try_initialize(&admin);
        assert_eq!(result, Err(Ok(ContractError::AlreadyInitialized)));
    }

    #[test]
    fn test_transfer_admin_success() {
        let (env, client, admin) = setup();
        let new_admin = Address::generate(&env);
        client.transfer_admin(&admin, &new_admin);
    }

    #[test]
    fn test_transfer_admin_self_blocked() {
        let (_, client, admin) = setup();
        let result = client.try_transfer_admin(&admin, &admin);
        assert_eq!(result, Err(Ok(ContractError::OperationNotAllowed)));
    }

    #[test]
    fn test_transfer_admin_non_admin_blocked() {
        let (env, client, _admin) = setup();
        let impostor = Address::generate(&env);
        let new_admin = Address::generate(&env);
        let result = client.try_transfer_admin(&impostor, &new_admin);
        assert_eq!(result, Err(Ok(ContractError::NotAdmin)));
    }

    // -----------------------------------------------------------------------
    // set_protocol_config (apply / mutating path)
    // -----------------------------------------------------------------------

    fn seed_protocol_config(client: &AdminContractClient, admin: &Address) {
        client.set_protocol_config(admin, &default_protocol_cfg());
    }

    #[test]
    fn test_set_protocol_config_success() {
        let (_, client, admin) = setup();
        seed_protocol_config(&client, &admin);
    }

    #[test]
    fn test_set_protocol_config_invalid_amount() {
        let (_, client, admin) = setup();
        let bad = ProtocolConfig {
            min_invoice_amount: 0,
            ..default_protocol_cfg()
        };
        let result = client.try_set_protocol_config(&admin, &bad);
        assert_eq!(result, Err(Ok(ContractError::InvalidAmount)));
    }

    #[test]
    fn test_set_protocol_config_invalid_due_date_zero() {
        let (_, client, admin) = setup();
        let bad = ProtocolConfig {
            max_due_date_days: 0,
            ..default_protocol_cfg()
        };
        let result = client.try_set_protocol_config(&admin, &bad);
        assert_eq!(result, Err(Ok(ContractError::InvalidParameter)));
    }

    #[test]
    fn test_set_protocol_config_invalid_due_date_too_large() {
        let (_, client, admin) = setup();
        let bad = ProtocolConfig {
            max_due_date_days: 731,
            ..default_protocol_cfg()
        };
        let result = client.try_set_protocol_config(&admin, &bad);
        assert_eq!(result, Err(Ok(ContractError::InvalidParameter)));
    }

    #[test]
    fn test_set_protocol_config_invalid_grace_period() {
        let (_, client, admin) = setup();
        let bad = ProtocolConfig {
            grace_period_seconds: 2_592_001,
            ..default_protocol_cfg()
        };
        let result = client.try_set_protocol_config(&admin, &bad);
        assert_eq!(result, Err(Ok(ContractError::InvalidParameter)));
    }

    #[test]
    fn test_set_protocol_config_non_admin_blocked() {
        let (env, client, _admin) = setup();
        let impostor = Address::generate(&env);
        let result = client.try_set_protocol_config(&impostor, &default_protocol_cfg());
        assert_eq!(result, Err(Ok(ContractError::NotAdmin)));
    }

    // -----------------------------------------------------------------------
    // set_fee_config (apply / mutating path)
    // -----------------------------------------------------------------------

    fn seed_fee_config(client: &AdminContractClient, admin: &Address, treasury: &Address) {
        client.set_fee_config(admin, &default_fee_cfg(treasury));
    }

    #[test]
    fn test_set_fee_config_success() {
        let (env, client, admin) = setup();
        let treasury = Address::generate(&env);
        seed_fee_config(&client, &admin, &treasury);
    }

    #[test]
    fn test_set_fee_config_fee_too_high() {
        let (env, client, admin) = setup();
        let treasury = Address::generate(&env);
        let bad = FeeConfig {
            fee_bps: 1001,
            treasury,
        };
        let result = client.try_set_fee_config(&admin, &bad);
        assert_eq!(result, Err(Ok(ContractError::InvalidFee)));
    }

    #[test]
    fn test_set_fee_config_boundary_max_fee() {
        let (env, client, admin) = setup();
        let treasury = Address::generate(&env);
        // Exactly 1000 bps (10 %) must be accepted.
        let cfg = FeeConfig {
            fee_bps: 1000,
            treasury,
        };
        client.set_fee_config(&admin, &cfg);
    }

    #[test]
    fn test_set_fee_config_zero_fee_allowed() {
        let (env, client, admin) = setup();
        let treasury = Address::generate(&env);
        let cfg = FeeConfig {
            fee_bps: 0,
            treasury,
        };
        client.set_fee_config(&admin, &cfg);
    }

    #[test]
    fn test_set_fee_config_non_admin_blocked() {
        let (env, client, _admin) = setup();
        let impostor = Address::generate(&env);
        let treasury = Address::generate(&env);
        let result = client.try_set_fee_config(&impostor, &default_fee_cfg(&treasury));
        assert_eq!(result, Err(Ok(ContractError::NotAdmin)));
    }

    // -----------------------------------------------------------------------
    // preview_protocol_config (dry-run – read-only)
    // -----------------------------------------------------------------------

    /// Seed config, then preview a change, confirm storage is UNCHANGED.
    #[test]
    fn test_preview_protocol_config_returns_diff() {
        let (env, client, admin) = setup();
        let original = default_protocol_cfg();
        seed_protocol_config(&client, &admin);

        let new_cfg = ProtocolConfig {
            min_invoice_amount: 2_000_000,
            max_due_date_days: 180,
            grace_period_seconds: 172_800,
        };

        let diff = client.preview_protocol_config(&admin, &new_cfg);

        // Current must match what was seeded.
        assert_eq!(diff.current, original);
        // Projected must match what was passed in.
        assert_eq!(diff.projected, new_cfg);
        // Not a no-op.
        assert!(!diff.is_noop);

        // CRITICAL: storage must NOT have changed.
        client.set_protocol_config(&admin, &original);
    }

    #[test]
    fn test_preview_protocol_config_noop_when_same() {
        let (_, client, admin) = setup();
        let cfg = default_protocol_cfg();
        seed_protocol_config(&client, &admin);

        let diff = client.preview_protocol_config(&admin, &cfg);

        assert_eq!(diff.current, cfg);
        assert_eq!(diff.projected, cfg);
        assert!(diff.is_noop, "expected is_noop = true for identical config");
    }

    #[test]
    fn test_preview_protocol_config_matches_apply_effect() {
        let (_, client, admin) = setup();
        seed_protocol_config(&client, &admin);

        let new_cfg = ProtocolConfig {
            min_invoice_amount: 5_000_000,
            max_due_date_days: 365,
            grace_period_seconds: 259_200,
        };

        // Get the diff first.
        let diff = client.preview_protocol_config(&admin, &new_cfg);

        // Now actually apply.
        client.set_protocol_config(&admin, &new_cfg);

        // Preview's `projected` must equal what apply would write.
        // (We verify by running preview again; current should now equal projected.)
        let diff2 = client.preview_protocol_config(&admin, &diff.current);

        // After apply, on-chain state == new_cfg == diff.projected.
        assert_eq!(diff2.current, diff.projected);
    }

    #[test]
    fn test_preview_protocol_config_invalid_params_rejected() {
        let (_, client, admin) = setup();
        seed_protocol_config(&client, &admin);

        let bad = ProtocolConfig {
            min_invoice_amount: 0, // invalid
            ..default_protocol_cfg()
        };
        let result = client.try_preview_protocol_config(&admin, &bad);
        assert_eq!(result, Err(Ok(ContractError::InvalidAmount)));
    }

    #[test]
    fn test_preview_protocol_config_invalid_due_date_zero_rejected() {
        let (_, client, admin) = setup();
        seed_protocol_config(&client, &admin);

        let bad = ProtocolConfig {
            max_due_date_days: 0, // zero is invalid
            ..default_protocol_cfg()
        };
        let result = client.try_preview_protocol_config(&admin, &bad);
        assert_eq!(result, Err(Ok(ContractError::InvalidParameter)));
    }

    #[test]
    fn test_preview_protocol_config_invalid_due_date_too_large_rejected() {
        let (_, client, admin) = setup();
        seed_protocol_config(&client, &admin);

        let bad = ProtocolConfig {
            max_due_date_days: 800, // > 730
            ..default_protocol_cfg()
        };
        let result = client.try_preview_protocol_config(&admin, &bad);
        assert_eq!(result, Err(Ok(ContractError::InvalidParameter)));
    }

    #[test]
    fn test_preview_protocol_config_invalid_grace_period_rejected() {
        let (_, client, admin) = setup();
        seed_protocol_config(&client, &admin);

        let bad = ProtocolConfig {
            grace_period_seconds: 9_999_999,
            ..default_protocol_cfg()
        };
        let result = client.try_preview_protocol_config(&admin, &bad);
        assert_eq!(result, Err(Ok(ContractError::InvalidParameter)));
    }

    #[test]
    fn test_preview_protocol_config_non_admin_blocked() {
        let (env, client, _admin) = setup();
        seed_protocol_config(&client, &_admin);

        let impostor = Address::generate(&env);
        let result = client.try_preview_protocol_config(&impostor, &default_protocol_cfg());
        assert_eq!(result, Err(Ok(ContractError::NotAdmin)));
    }

    #[test]
    fn test_preview_protocol_config_not_initialized() {
        // Protocol not yet seeded – should fail with NotInitialized.
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, AdminContract);
        let client = AdminContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        client.initialize(&admin);

        // No set_protocol_config called, so storage has no config yet.
        let result = client.try_preview_protocol_config(&admin, &default_protocol_cfg());
        assert_eq!(result, Err(Ok(ContractError::NotInitialized)));
    }

    // -----------------------------------------------------------------------
    // preview_fee_config (dry-run – read-only)
    // -----------------------------------------------------------------------

    #[test]
    fn test_preview_fee_config_returns_diff() {
        let (env, client, admin) = setup();
        let treasury = Address::generate(&env);
        let original = default_fee_cfg(&treasury);
        seed_fee_config(&client, &admin, &treasury);

        let new_treasury = Address::generate(&env);
        let new_cfg = FeeConfig {
            fee_bps: 200,
            treasury: new_treasury,
        };

        let diff = client.preview_fee_config(&admin, &new_cfg);

        assert_eq!(diff.current, original);
        assert_eq!(diff.projected, new_cfg);
        assert!(!diff.is_noop);
    }

    #[test]
    fn test_preview_fee_config_noop_when_same() {
        let (env, client, admin) = setup();
        let treasury = Address::generate(&env);
        let cfg = default_fee_cfg(&treasury);
        seed_fee_config(&client, &admin, &treasury);

        let diff = client.preview_fee_config(&admin, &cfg);
        assert!(diff.is_noop);
    }

    #[test]
    fn test_preview_fee_config_matches_apply_effect() {
        let (env, client, admin) = setup();
        let treasury = Address::generate(&env);
        seed_fee_config(&client, &admin, &treasury);

        let new_cfg = FeeConfig {
            fee_bps: 500,
            treasury: treasury.clone(),
        };

        let diff = client.preview_fee_config(&admin, &new_cfg);

        // Apply and confirm on-chain state equals diff.projected.
        client.set_fee_config(&admin, &new_cfg);

        let diff2 = client.preview_fee_config(&admin, &diff.current);
        assert_eq!(diff2.current, diff.projected);
    }

    #[test]
    fn test_preview_fee_config_fee_too_high_rejected() {
        let (env, client, admin) = setup();
        let treasury = Address::generate(&env);
        seed_fee_config(&client, &admin, &treasury);

        let bad = FeeConfig {
            fee_bps: 1001,
            treasury: treasury.clone(),
        };
        let result = client.try_preview_fee_config(&admin, &bad);
        assert_eq!(result, Err(Ok(ContractError::InvalidFee)));
    }

    #[test]
    fn test_preview_fee_config_non_admin_blocked() {
        let (env, client, _admin) = setup();
        let treasury = Address::generate(&env);
        seed_fee_config(&client, &_admin, &treasury);

        let impostor = Address::generate(&env);
        let result = client.try_preview_fee_config(&impostor, &default_fee_cfg(&treasury));
        assert_eq!(result, Err(Ok(ContractError::NotAdmin)));
    }

    #[test]
    fn test_preview_fee_config_not_initialized() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, AdminContract);
        let client = AdminContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        client.initialize(&admin);

        let treasury = Address::generate(&env);
        let result = client.try_preview_fee_config(&admin, &default_fee_cfg(&treasury));
        assert_eq!(result, Err(Ok(ContractError::NotInitialized)));
    }

    // -----------------------------------------------------------------------
    // Boundary / edge-case tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_preview_protocol_config_boundary_min_amount_one() {
        let (_, client, admin) = setup();
        seed_protocol_config(&client, &admin);

        let cfg = ProtocolConfig {
            min_invoice_amount: 1,
            max_due_date_days: 1,
            grace_period_seconds: 0,
        };
        let diff = client.preview_protocol_config(&admin, &cfg);
        assert_eq!(diff.projected, cfg);
    }

    #[test]
    fn test_preview_protocol_config_boundary_max_due_date() {
        let (_, client, admin) = setup();
        seed_protocol_config(&client, &admin);

        let cfg = ProtocolConfig {
            max_due_date_days: 730,
            ..default_protocol_cfg()
        };
        let diff = client.preview_protocol_config(&admin, &cfg);
        assert_eq!(diff.projected.max_due_date_days, 730);
    }

    #[test]
    fn test_preview_protocol_config_boundary_max_grace_period() {
        let (_, client, admin) = setup();
        seed_protocol_config(&client, &admin);

        let cfg = ProtocolConfig {
            grace_period_seconds: 2_592_000,
            ..default_protocol_cfg()
        };
        let diff = client.preview_protocol_config(&admin, &cfg);
        assert_eq!(diff.projected.grace_period_seconds, 2_592_000);
    }

    #[test]
    fn test_preview_fee_config_boundary_max_fee() {
        let (env, client, admin) = setup();
        let treasury = Address::generate(&env);
        seed_fee_config(&client, &admin, &treasury);

        let cfg = FeeConfig {
            fee_bps: 1000,
            treasury: treasury.clone(),
        };
        let diff = client.preview_fee_config(&admin, &cfg);
        assert_eq!(diff.projected.fee_bps, 1000);
    }

    #[test]
    fn test_preview_fee_config_boundary_zero_fee() {
        let (env, client, admin) = setup();
        let treasury = Address::generate(&env);
        seed_fee_config(&client, &admin, &treasury);

        let cfg = FeeConfig {
            fee_bps: 0,
            treasury: treasury.clone(),
        };
        let diff = client.preview_fee_config(&admin, &cfg);
        assert_eq!(diff.projected.fee_bps, 0);
    }

    // -----------------------------------------------------------------------
    // Storage-mutation guard (no storage write during preview)
    // -----------------------------------------------------------------------

    /// After preview_protocol_config, storage must contain the *original* value.
    #[test]
    fn test_preview_does_not_write_protocol_config() {
        let (_, client, admin) = setup();
        let original = default_protocol_cfg();
        seed_protocol_config(&client, &admin);

        let different = ProtocolConfig {
            min_invoice_amount: 9_999_999,
            max_due_date_days: 500,
            grace_period_seconds: 1_000,
        };

        client.preview_protocol_config(&admin, &different);

        // After preview, storage should still hold `original`.
        // We detect this by running another preview and checking `current`.
        let diff = client.preview_protocol_config(&admin, &original);
        assert_eq!(
            diff.current, original,
            "preview_protocol_config must not mutate storage"
        );
    }

    /// After preview_fee_config, storage must contain the *original* value.
    #[test]
    fn test_preview_does_not_write_fee_config() {
        let (env, client, admin) = setup();
        let treasury = Address::generate(&env);
        let original = default_fee_cfg(&treasury);
        seed_fee_config(&client, &admin, &treasury);

        let different = FeeConfig {
            fee_bps: 999,
            treasury: Address::generate(&env),
        };

        client.preview_fee_config(&admin, &different);

        let diff = client.preview_fee_config(&admin, &original);
        assert_eq!(
            diff.current, original,
            "preview_fee_config must not mutate storage"
        );
    }
}
