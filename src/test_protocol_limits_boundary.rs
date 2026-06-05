#[cfg(test)]
mod test_protocol_limits_boundary {
    use soroban_sdk::{testutils::Address as _, Address, Env};
    use crate::admin::{AdminContract, AdminContractClient, validate_protocol_config};
    use crate::storage_types::ProtocolConfig;
    use crate::errors::ContractError;

    // Default valid protocol configuration for testing
    fn default_protocol_cfg() -> ProtocolConfig {
        ProtocolConfig {
            min_invoice_amount: 1_000_000,
            max_due_date_days: 90,
            grace_period_seconds: 86_400,
        }
    }

    #[test]
    fn test_set_protocol_config_atomic_application() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let contract_id = env.register_contract(None, AdminContract);
        let client = AdminContractClient::new(&env, &contract_id);
        
        client.initialize(&admin).unwrap();

        let original_config = default_protocol_cfg();
        client.set_protocol_config(&admin, &original_config).unwrap();
        
        // Attempt to apply an invalid config (e.g. min_invoice_amount = 0)
        let mut invalid_config = original_config.clone();
        invalid_config.min_invoice_amount = 0;
        
        let result = client.set_protocol_config(&admin, &invalid_config);
        assert_eq!(result, Err(Ok(ContractError::InvalidAmount)));
        
        // Assert that the config was NOT partially applied (atomic application)
        let diff = client.preview_protocol_config(&admin, &original_config).unwrap();
        assert_eq!(diff.current, original_config, "Configuration should remain unchanged after a failed update");
    }

    #[test]
    fn test_min_invoice_amount_exact_boundary() {
        let mut cfg = default_protocol_cfg();
        
        // Boundary: exactly 1 is allowed
        cfg.min_invoice_amount = 1;
        assert!(validate_protocol_config(&cfg).is_ok());
        
        // Boundary: 0 is rejected
        cfg.min_invoice_amount = 0;
        assert_eq!(validate_protocol_config(&cfg), Err(ContractError::InvalidAmount));
        
        // Boundary: -1 is rejected
        cfg.min_invoice_amount = -1;
        assert_eq!(validate_protocol_config(&cfg), Err(ContractError::InvalidAmount));
    }

    #[test]
    fn test_max_due_date_days_exact_boundary() {
        let mut cfg = default_protocol_cfg();
        
        // Boundary: 730 is allowed
        cfg.max_due_date_days = 730;
        assert!(validate_protocol_config(&cfg).is_ok());
        
        // Boundary: 731 is rejected (off-by-one beyond max)
        cfg.max_due_date_days = 731;
        assert_eq!(validate_protocol_config(&cfg), Err(ContractError::InvalidParameter));
        
        // Boundary: 1 is allowed
        cfg.max_due_date_days = 1;
        assert!(validate_protocol_config(&cfg).is_ok());
        
        // Boundary: 0 is rejected (off-by-one below min)
        cfg.max_due_date_days = 0;
        assert_eq!(validate_protocol_config(&cfg), Err(ContractError::InvalidParameter));
    }
}