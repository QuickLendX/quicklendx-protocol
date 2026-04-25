//! Comprehensive test suite for hardened protocol initialization.
//!
//! Test Coverage:
//! 1. Successful Initialization — all parameters, admin setup, storage
//! 2. Re-initialization Protection — double init, state preservation
//! 3. Parameter Validation — fees, amounts, dates, grace periods
//! 4. Authorization — admin auth requirement, unauthorized attempts
//! 5. Configuration Updates — protocol config, fees, treasury
//! 6. Query Functions — all getters, defaults, edge cases
//! 7. Events — initialization, updates, audit trail
//! 8. Security — atomic operations, locks, validation
//! 9. Edge Cases — boundary values, concurrent operations
//! 10. Integration — full workflow, admin integration
//!
//! Target: 95%+ test coverage for init.rs

#[cfg(test)]
mod test_init {
    use crate::admin::AdminStorage;
    use crate::errors::QuickLendXError;
    use crate::init::{InitializationParams, ProtocolConfig, ProtocolInitializer};
    use crate::{QuickLendXContract, QuickLendXContractClient};
    use soroban_sdk::{
        testutils::{Address as _, Events},
        Address, Env, Vec,
    };

    fn setup() -> (Env, QuickLendXContractClient<'static>) {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(QuickLendXContract, ());
        let client = QuickLendXContractClient::new(&env, &contract_id);
        (env, client)
    }

    fn create_valid_params(env: &Env) -> InitializationParams {
        InitializationParams {
            admin: Address::generate(env),
            treasury: Address::generate(env),
            fee_bps: 200,
            min_invoice_amount: 1_000_000,
            max_due_date_days: 365,
            grace_period_seconds: 604800, // 7 days
            initial_currencies: Vec::new(env),
        }
    }

    fn setup_initialized() -> (Env, QuickLendXContractClient<'static>, InitializationParams) {
        let (env, client) = setup();
        let params = create_valid_params(&env);
        client.initialize(&params);
        (env, client, params)
    }

    // ============================================================================
    // 1. Successful Initialization Tests
    // ============================================================================

    #[test]
    fn test_successful_initialization_with_valid_params() {
        let (env, client) = setup();
        let params = create_valid_params(&env);

        let result = client.try_initialize(&params);
        assert!(
            result.is_ok(),
            "Initialization with valid params must succeed"
        );

        assert!(
            client.is_initialized(),
            "Protocol must be marked as initialized"
        );
        assert!(
            AdminStorage::is_initialized(&env),
            "Admin must be initialized"
        );
    }

    #[test]
    fn test_initialization_stores_admin_correctly() {
        let (env, client) = setup();
        let params = create_valid_params(&env);
        let admin = params.admin.clone();

        client.initialize(&params);

        assert_eq!(
            client.get_current_admin(),
            Some(admin),
            "Admin must be stored correctly"
        );
        assert!(
            AdminStorage::is_admin(&env, &params.admin),
            "Admin must be recognized by AdminStorage"
        );
    }

    #[test]
    fn test_initialization_stores_treasury_correctly() {
        let (env, client) = setup();
        let params = create_valid_params(&env);
        let treasury = params.treasury.clone();

        client.initialize(&params);

        assert_eq!(
            client.get_treasury(),
            Some(treasury),
            "Treasury must be stored correctly"
        );
    }

    #[test]
    fn test_initialization_stores_fee_bps_correctly() {
        let (env, client) = setup();
        let params = create_valid_params(&env);
        let fee_bps = params.fee_bps;

        client.initialize(&params);

        assert_eq!(
            client.get_fee_bps(),
            fee_bps,
            "Fee BPS must be stored correctly"
        );
    }

    #[test]
    fn test_initialization_stores_protocol_config_correctly() {
        let (env, client) = setup();
        let params = create_valid_params(&env);

        client.initialize(&params);

        let config = ProtocolInitializer::get_protocol_config(&env);
        assert!(config.is_some(), "Protocol config must be stored");

        let config = config.unwrap();
        assert_eq!(config.min_invoice_amount, params.min_invoice_amount);
        assert_eq!(config.max_due_date_days, params.max_due_date_days);
        assert_eq!(config.grace_period_seconds, params.grace_period_seconds);
        assert_eq!(config.updated_by, params.admin);
    }

    #[test]
    fn test_initialization_with_currencies() {
        let (env, client) = setup();
        let currency1 = Address::generate(&env);
        let currency2 = Address::generate(&env);
        let currencies = Vec::from_array(&env, [currency1.clone(), currency2.clone()]);

        let mut params = create_valid_params(&env);
        params.initial_currencies = currencies.clone();

        client.initialize(&params);

        // Note: This test assumes there's a way to query currencies
        // The actual implementation may need a get_currencies function
        assert!(
            client.is_initialized(),
            "Initialization with currencies must succeed"
        );
    }

    #[test]
    fn test_initialization_emits_event() {
        let (env, client) = setup();
        let params = create_valid_params(&env);

        client.initialize(&params);

        let events = env.events().all();
        let init_events: Vec<_> = events
            .iter()
            .filter(|e| e.0 == (soroban_sdk::symbol_short!("proto_in"),))
            .collect();

        assert!(!init_events.is_empty(), "Initialization must emit event");
    }

    // ============================================================================
    // 2. Re-initialization Protection Tests
    // ============================================================================

    #[test]
    fn test_double_initialization_fails() {
        let (env, client) = setup();
        let params1 = create_valid_params(&env);
        let params2 = create_valid_params(&env);

        // First initialization succeeds
        client.initialize(&params1);
        assert!(client.is_initialized());

        // Second initialization fails
        let result = client.try_initialize(&params2);
        assert_eq!(
            result,
            Err(Ok(QuickLendXError::OperationNotAllowed)),
            "Double initialization must fail"
        );
    }

    #[test]
    fn test_state_preservation_after_failed_reinit() {
        let (env, client, original_params) = setup_initialized();
        let new_params = create_valid_params(&env);

        // Attempt re-initialization
        let _result = client.try_initialize(&new_params);

        // Original state must be preserved
        assert_eq!(
            client.get_current_admin(),
            Some(original_params.admin),
            "Original admin must be preserved"
        );
        assert_eq!(
            client.get_treasury(),
            Some(original_params.treasury),
            "Original treasury must be preserved"
        );
        assert_eq!(
            client.get_fee_bps(),
            original_params.fee_bps,
            "Original fee must be preserved"
        );
    }

    #[test]
    fn test_is_initialized_returns_correct_values() {
        let (env, client) = setup();

        // Before initialization
        assert!(!client.is_initialized(), "Must return false before init");
        assert!(
            !ProtocolInitializer::is_initialized(&env),
            "Direct call must also return false"
        );

        // After initialization
        let params = create_valid_params(&env);
        client.initialize(&params);

        assert!(client.is_initialized(), "Must return true after init");
        assert!(
            ProtocolInitializer::is_initialized(&env),
            "Direct call must also return true"
        );
    }

    // ============================================================================
    // 3. Parameter Validation Tests - Fee BPS
    // ============================================================================

    #[test]
    fn test_fee_bps_too_high_fails() {
        let (env, client) = setup();
        let mut params = create_valid_params(&env);
        params.fee_bps = 1001; // 10.01% > 10%

        let result = client.try_initialize(&params);
        assert_eq!(
            result,
            Err(Ok(QuickLendXError::InvalidFeeBasisPoints)),
            "Fee BPS too high must fail"
        );
    }

    #[test]
    fn test_fee_bps_max_value_succeeds() {
        let (env, client) = setup();
        let mut params = create_valid_params(&env);
        params.fee_bps = 1000; // 10% exactly

        let result = client.try_initialize(&params);
        assert!(result.is_ok(), "Max fee BPS must succeed");
    }

    #[test]
    fn test_fee_bps_zero_succeeds() {
        let (env, client) = setup();
        let mut params = create_valid_params(&env);
        params.fee_bps = 0; // 0%

        let result = client.try_initialize(&params);
        assert!(result.is_ok(), "Zero fee BPS must succeed");
    }

    // ============================================================================
    // 4. Parameter Validation Tests - Min Invoice Amount
    // ============================================================================

    #[test]
    fn test_min_invoice_amount_zero_fails() {
        let (env, client) = setup();
        let mut params = create_valid_params(&env);
        params.min_invoice_amount = 0;

        let result = client.try_initialize(&params);
        assert_eq!(
            result,
            Err(Ok(QuickLendXError::InvalidAmount)),
            "Zero min invoice amount must fail"
        );
    }

    #[test]
    fn test_min_invoice_amount_negative_fails() {
        let (env, client) = setup();
        let mut params = create_valid_params(&env);
        params.min_invoice_amount = -1000;

        let result = client.try_initialize(&params);
        assert_eq!(
            result,
            Err(Ok(QuickLendXError::InvalidAmount)),
            "Negative min invoice amount must fail"
        );
    }

    #[test]
    fn test_min_invoice_amount_small_positive_succeeds() {
        let (env, client) = setup();
        let mut params = create_valid_params(&env);
        params.min_invoice_amount = 1;

        let result = client.try_initialize(&params);
        assert!(
            result.is_ok(),
            "Small positive min invoice amount must succeed"
        );
    }

    #[test]
    fn test_min_invoice_amount_large_succeeds() {
        let (env, client) = setup();
        let mut params = create_valid_params(&env);
        params.min_invoice_amount = 1_000_000_000_000; // 1 trillion

        let result = client.try_initialize(&params);
        assert!(result.is_ok(), "Large min invoice amount must succeed");
    }

    // ============================================================================
    // 5. Parameter Validation Tests - Due Date Days
    // ============================================================================

    #[test]
    fn test_max_due_date_days_zero_fails() {
        let (env, client) = setup();
        let mut params = create_valid_params(&env);
        params.max_due_date_days = 0;

        let result = client.try_initialize(&params);
        assert_eq!(
            result,
            Err(Ok(QuickLendXError::InvoiceDueDateInvalid)),
            "Zero max due date days must fail"
        );
    }

    #[test]
    fn test_max_due_date_days_too_high_fails() {
        let (env, client) = setup();
        let mut params = create_valid_params(&env);
        params.max_due_date_days = 731; // > 730

        let result = client.try_initialize(&params);
        assert_eq!(
            result,
            Err(Ok(QuickLendXError::InvoiceDueDateInvalid)),
            "Too high max due date days must fail"
        );
    }

    #[test]
    fn test_max_due_date_days_max_value_succeeds() {
        let (env, client) = setup();
        let mut params = create_valid_params(&env);
        params.max_due_date_days = 730; // 2 years exactly

        let result = client.try_initialize(&params);
        assert!(result.is_ok(), "Max due date days at limit must succeed");
    }

    #[test]
    fn test_max_due_date_days_one_succeeds() {
        let (env, client) = setup();
        let mut params = create_valid_params(&env);
        params.max_due_date_days = 1;

        let result = client.try_initialize(&params);
        assert!(result.is_ok(), "One day max due date must succeed");
    }

    // ============================================================================
    // 6. Parameter Validation Tests - Grace Period
    // ============================================================================

    #[test]
    fn test_grace_period_too_long_fails() {
        let (env, client) = setup();
        let mut params = create_valid_params(&env);
        params.grace_period_seconds = 2_592_001; // > 30 days

        let result = client.try_initialize(&params);
        assert_eq!(
            result,
            Err(Ok(QuickLendXError::InvalidTimestamp)),
            "Too long grace period must fail"
        );
    }

    #[test]
    fn test_grace_period_max_value_succeeds() {
        let (env, client) = setup();
        let mut params = create_valid_params(&env);
        params.grace_period_seconds = 2_592_000; // 30 days exactly

        let result = client.try_initialize(&params);
        assert!(result.is_ok(), "Max grace period must succeed");
    }

    #[test]
    fn test_grace_period_zero_succeeds() {
        let (env, client) = setup();
        let mut params = create_valid_params(&env);
        params.grace_period_seconds = 0;

        let result = client.try_initialize(&params);
        assert!(result.is_ok(), "Zero grace period must succeed");
    }

    // ============================================================================
    // 7. Address Validation Tests
    // ============================================================================

    #[test]
    fn test_treasury_same_as_admin_fails() {
        let (env, client) = setup();
        let mut params = create_valid_params(&env);
        params.treasury = params.admin.clone(); // Same as admin

        let result = client.try_initialize(&params);
        assert_eq!(
            result,
            Err(Ok(QuickLendXError::InvalidAddress)),
            "Treasury same as admin must fail"
        );
    }

    // ============================================================================
    // 8. Authorization Tests
    // ============================================================================

    #[test]
    fn test_initialization_requires_admin_auth() {
        let env = Env::default();
        let contract_id = env.register(QuickLendXContract, ());
        let client = QuickLendXContractClient::new(&env, &contract_id);

        let params = create_valid_params(&env);

        // Should panic without authorization
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client.initialize(&params);
        }));
        assert!(result.is_err(), "Initialization without auth must fail");
    }

    // ============================================================================
    // 9. Configuration Update Tests
    // ============================================================================

    #[test]
    fn test_set_protocol_config_succeeds() {
        let (env, client, params) = setup_initialized();

        let result = client.try_set_protocol_config(
            &params.admin,
            2_000_000, // new min amount
            180,       // new max days
            86400,     // new grace period (1 day)
        );

        assert!(result.is_ok(), "Protocol config update must succeed");

        let config = ProtocolInitializer::get_protocol_config(&env).unwrap();
        assert_eq!(config.min_invoice_amount, 2_000_000);
        assert_eq!(config.max_due_date_days, 180);
        assert_eq!(config.grace_period_seconds, 86400);
    }

    #[test]
    fn test_set_protocol_config_non_admin_fails() {
        let (env, client, _params) = setup_initialized();
        let non_admin = Address::generate(&env);

        let result = client.try_set_protocol_config(&non_admin, &1_000_000i128, &365u64, &604800u64);
        assert_eq!(
            result,
            Err(Ok(QuickLendXError::NotAdmin)),
            "Non-admin config update must fail"
        );
    }

    #[test]
    fn test_set_protocol_config_validates_parameters() {
        let (env, client, params) = setup_initialized();

        // Test invalid min amount
        let result = client.try_set_protocol_config(&params.admin, &0i128, &365u64, &604800u64);
        assert_eq!(
            result,
            Err(Ok(QuickLendXError::InvalidAmount)),
            "Invalid min amount must fail"
        );

        // Test invalid max days
        let result = client.try_set_protocol_config(&params.admin, &1_000_000i128, &0u64, &604800u64);
        assert_eq!(
            result,
            Err(Ok(QuickLendXError::InvoiceDueDateInvalid)),
            "Invalid max days must fail"
        );

        // Test invalid grace period
        let result = client.try_set_protocol_config(&params.admin, &1_000_000i128, &365u64, &3_000_000u64);
        assert_eq!(
            result,
            Err(Ok(QuickLendXError::InvalidTimestamp)),
            "Invalid grace period must fail"
        );
    }

    #[test]
    fn test_set_protocol_config_emits_event() {
        let (env, client, params) = setup_initialized();

        client.set_protocol_config(&params.admin, &2_000_000i128, &180u64, &86400u64);

        let events = env.events().all();
        let config_events: Vec<_> = events
            .iter()
            .filter(|e| e.0 == (soroban_sdk::symbol_short!("proto_cfg"),))
            .collect();

        assert!(!config_events.is_empty(), "Config update must emit event");
    }

    #[test]
    fn test_set_fee_config_succeeds() {
        let (env, client, params) = setup_initialized();

        let result = client.try_set_fee_config(&params.admin, &300u32); // 3%
        assert!(result.is_ok(), "Fee config update must succeed");

        assert_eq!(client.get_fee_bps(), 300, "Fee must be updated");
    }

    #[test]
    fn test_set_fee_config_validates_fee() {
        let (env, client, params) = setup_initialized();

        let result = client.try_set_fee_config(&params.admin, &1001u32); // > 10%
        assert_eq!(
            result,
            Err(Ok(QuickLendXError::InvalidFeeBasisPoints)),
            "Invalid fee must fail"
        );
    }

    #[test]
    fn test_set_fee_config_zero_allowed() {
        let (env, client, params) = setup_initialized();

        let result = client.try_set_fee_config(&params.admin, &0u32);
        assert!(result.is_ok(), "Zero fee must be allowed");
        assert_eq!(client.get_fee_bps(), 0);
    }

    #[test]
    fn test_set_treasury_succeeds() {
        let (env, client, params) = setup_initialized();
        let new_treasury = Address::generate(&env);

        let result = client.try_set_treasury(&params.admin, &new_treasury);
        assert!(result.is_ok(), "Treasury update must succeed");

        assert_eq!(
            client.get_treasury(),
            Some(new_treasury),
            "Treasury must be updated"
        );
    }

    #[test]
    fn test_set_treasury_same_as_admin_fails() {
        let (env, client, params) = setup_initialized();

        let result = client.try_set_treasury(&params.admin, &params.admin);
        assert_eq!(
            result,
            Err(Ok(QuickLendXError::InvalidAddress)),
            "Treasury same as admin must fail"
        );
    }

    // ============================================================================
    // 10. Query Function Tests
    // ============================================================================

    #[test]
    fn test_query_functions_before_initialization() {
        let (env, client) = setup();

        assert_eq!(
            client.get_treasury(),
            None,
            "Treasury must be None before init"
        );
        assert_eq!(
            client.get_fee_bps(),
            200,
            "Fee must return default before init"
        );
        assert_eq!(
            client.get_min_invoice_amount(),
            10, // Test default
            "Min amount must return default before init"
        );
        assert_eq!(
            client.get_max_due_date_days(),
            365,
            "Max days must return default before init"
        );
        assert_eq!(
            client.get_grace_period_seconds(),
            604800,
            "Grace period must return default before init"
        );
    }

    #[test]
    fn test_query_functions_after_initialization() {
        let (env, client, params) = setup_initialized();

        assert_eq!(
            client.get_treasury(),
            Some(params.treasury),
            "Treasury must return stored value"
        );
        assert_eq!(
            client.get_fee_bps(),
            params.fee_bps,
            "Fee must return stored value"
        );
        assert_eq!(
            client.get_min_invoice_amount(),
            params.min_invoice_amount,
            "Min amount must return stored value"
        );
        assert_eq!(
            client.get_max_due_date_days(),
            params.max_due_date_days,
            "Max days must return stored value"
        );
        assert_eq!(
            client.get_grace_period_seconds(),
            params.grace_period_seconds,
            "Grace period must return stored value"
        );
    }

    #[test]
    fn test_get_protocol_config_returns_none_before_init() {
        let (env, _client) = setup();

        let config = ProtocolInitializer::get_protocol_config(&env);
        assert!(
            config.is_none(),
            "Config must be None before initialization"
        );
    }

    #[test]
    fn test_get_protocol_config_returns_config_after_init() {
        let (env, _client, params) = setup_initialized();

        let config = ProtocolInitializer::get_protocol_config(&env);
        assert!(config.is_some(), "Config must exist after initialization");

        let config = config.unwrap();
        assert_eq!(config.min_invoice_amount, params.min_invoice_amount);
        assert_eq!(config.max_due_date_days, params.max_due_date_days);
        assert_eq!(config.grace_period_seconds, params.grace_period_seconds);
        assert_eq!(config.updated_by, params.admin);
    }

    // ============================================================================
    // 11. Edge Cases and Boundary Values
    // ============================================================================

    #[test]
    fn test_boundary_values_succeed() {
        let (env, client) = setup();
        let mut params = create_valid_params(&env);

        // Test all boundary values
        params.fee_bps = 1000; // Max fee
        params.min_invoice_amount = 1; // Min positive amount
        params.max_due_date_days = 730; // Max days
        params.grace_period_seconds = 2_592_000; // Max grace period

        let result = client.try_initialize(&params);
        assert!(result.is_ok(), "Boundary values must succeed");
    }

    #[test]
    fn test_multiple_config_updates() {
        let (env, client, params) = setup_initialized();

        // Update protocol config
        client.set_protocol_config(&params.admin, 2_000_000, 180, 86400);

        // Update fee config
        client.set_fee_config(&params.admin, 300);

        // Update treasury
        let new_treasury = Address::generate(&env);
        client.set_treasury(&params.admin, &new_treasury);

        // Verify all updates
        let config = ProtocolInitializer::get_protocol_config(&env).unwrap();
        assert_eq!(config.min_invoice_amount, 2_000_000);
        assert_eq!(config.max_due_date_days, 180);
        assert_eq!(config.grace_period_seconds, 86400);
        assert_eq!(client.get_fee_bps(), 300);
        assert_eq!(client.get_treasury(), Some(new_treasury));
    }

    // ============================================================================
    // 12. Integration Tests
    // ============================================================================

    #[test]
    fn test_full_initialization_workflow() {
        let (env, client) = setup();

        // 1. Initial state
        assert!(!client.is_initialized());
        assert!(!AdminStorage::is_initialized(&env));

        // 2. Initialize protocol
        let params = create_valid_params(&env);
        client.initialize(&params);

        // 3. Verify initialization
        assert!(client.is_initialized());
        assert!(AdminStorage::is_initialized(&env));
        assert_eq!(client.get_current_admin(), Some(params.admin.clone()));

        // 4. Update configurations
        client.set_protocol_config(&params.admin, &2_000_000i128, &180u64, &86400u64);
        client.set_fee_config(&params.admin, 300);

        let new_treasury = Address::generate(&env);
        client.set_treasury(&params.admin, &new_treasury);

        // 5. Verify final state
        let config = ProtocolInitializer::get_protocol_config(&env).unwrap();
        assert_eq!(config.min_invoice_amount, 2_000_000);
        assert_eq!(client.get_fee_bps(), 300);
        assert_eq!(client.get_treasury(), Some(new_treasury));
    }

    #[test]
    fn test_admin_integration() {
        let (env, client) = setup();
        let params = create_valid_params(&env);

        // Initialize protocol
        client.initialize(&params);

        // Verify admin integration
        assert!(AdminStorage::is_admin(&env, &params.admin));
        assert_eq!(AdminStorage::get_admin(&env), Some(params.admin.clone()));

        // Transfer admin
        let new_admin = Address::generate(&env);
        client.transfer_admin(&params.admin, &new_admin);

        // Verify new admin can update config
        let result = client.try_set_fee_config(&new_admin, 400);
        assert!(result.is_ok(), "New admin must be able to update config");

        // Verify old admin cannot update config
        let result = client.try_set_fee_config(&params.admin, 500);
        assert_eq!(
            result,
            Err(Ok(QuickLendXError::NotAdmin)),
            "Old admin must not be able to update config"
        );
    }

    #[test]
    fn test_event_emission_comprehensive() {
        let (env, client) = setup();
        let params = create_valid_params(&env);

        // Initialize
        client.initialize(&params);

        // Update configs
        client.set_protocol_config(&params.admin, &2_000_000i128, &180u64, &86400u64);
        client.set_fee_config(&params.admin, 300);

        let new_treasury = Address::generate(&env);
        client.set_treasury(&params.admin, &new_treasury);

        let events = env.events().all();

        // Check for all expected events
        let init_events: Vec<_> = events
            .iter()
            .filter(|e| e.0 == (soroban_sdk::symbol_short!("proto_in"),))
            .collect();
        let config_events: Vec<_> = events
            .iter()
            .filter(|e| e.0 == (soroban_sdk::symbol_short!("proto_cfg"),))
            .collect();
        let fee_events: Vec<_> = events
            .iter()
            .filter(|e| e.0 == (soroban_sdk::symbol_short!("fee_cfg"),))
            .collect();
        let treasury_events: Vec<_> = events
            .iter()
            .filter(|e| e.0 == (soroban_sdk::symbol_short!("trsr_upd"),))
            .collect();

        assert_eq!(init_events.len(), 1, "Must have one init event");
        assert_eq!(config_events.len(), 1, "Must have one config event");
        assert_eq!(fee_events.len(), 1, "Must have one fee event");
        assert_eq!(treasury_events.len(), 1, "Must have one treasury event");
    }

    // ============================================================================
    // 13. Version Consistency Tests
    // ============================================================================

    #[test]
    fn test_get_version_before_init_returns_constant() {
        let (_env, client) = setup();
        assert_eq!(
            client.get_version(),
            crate::init::PROTOCOL_VERSION,
            "get_version must return PROTOCOL_VERSION constant before init"
        );
    }

    #[test]
    fn test_get_version_after_init_matches_constant() {
        let (_env, client, _params) = setup_initialized();
        assert_eq!(
            client.get_version(),
            crate::init::PROTOCOL_VERSION,
            "get_version must equal PROTOCOL_VERSION after init"
        );
    }

    #[test]
    fn test_get_version_stored_in_instance_storage() {
        let (env, _client, _params) = setup_initialized();
        assert_eq!(
            crate::init::ProtocolInitializer::get_version(&env),
            crate::init::PROTOCOL_VERSION,
            "Stored version must match PROTOCOL_VERSION"
        );
    }

    #[test]
    fn test_get_version_consistent_before_and_after_init() {
        let (env, client) = setup();
        let before = client.get_version();
        let params = create_valid_params(&env);
        client.initialize(&params);
        let after = client.get_version();
        assert_eq!(
            before, after,
            "Version must be the same before and after init"
        );
    }

    #[test]
    fn test_reinit_does_not_change_version() {
        let (_env, client, params) = setup_initialized();
        let v1 = client.get_version();
        // Idempotent re-init with same params must not alter version
        let _ = client.try_initialize(&params);
        assert_eq!(
            client.get_version(),
            v1,
            "Version must not change on idempotent re-init"
        );
    }
}
