//! Simple test to verify hardened admin implementation works

#[cfg(test)]
mod test_admin_simple {
    use crate::admin::AdminStorage;
    use crate::errors::QuickLendXError;
    use soroban_sdk::{testutils::Address as _, Address, Env};

    #[test]
    fn test_admin_initialization() {
        let env = Env::default();
        env.mock_all_auths();
        
        let admin = Address::generate(&env);
        
        // Test initialization
        let result = AdminStorage::initialize(&env, &admin);
        assert!(result.is_ok(), "Admin initialization should succeed");
        
        // Test that admin is set
        assert_eq!(AdminStorage::get_admin(&env), Some(admin.clone()));
        assert!(AdminStorage::is_admin(&env, &admin));
        assert!(AdminStorage::is_initialized(&env));
    }

    #[test]
    fn test_admin_double_initialization_fails() {
        let env = Env::default();
        env.mock_all_auths();
        
        let admin1 = Address::generate(&env);
        let admin2 = Address::generate(&env);
        
        // First initialization succeeds
        AdminStorage::initialize(&env, &admin1).unwrap();
        
        // Second initialization fails
        let result = AdminStorage::initialize(&env, &admin2);
        assert_eq!(result, Err(QuickLendXError::OperationNotAllowed));
        
        // Original admin remains
        assert_eq!(AdminStorage::get_admin(&env), Some(admin1));
    }

    #[test]
    fn test_admin_transfer() {
        let env = Env::default();
        env.mock_all_auths();
        
        let admin1 = Address::generate(&env);
        let admin2 = Address::generate(&env);
        
        // Initialize with first admin
        AdminStorage::initialize(&env, &admin1).unwrap();
        
        // Transfer to second admin
        let result = AdminStorage::transfer_admin(&env, &admin1, &admin2);
        assert!(result.is_ok(), "Admin transfer should succeed");
        
        // Verify transfer
        assert_eq!(AdminStorage::get_admin(&env), Some(admin2.clone()));
        assert!(AdminStorage::is_admin(&env, &admin2));
        assert!(!AdminStorage::is_admin(&env, &admin1));
    }

    #[test]
    fn test_admin_require_functions() {
        let env = Env::default();
        env.mock_all_auths();
        
        let admin = Address::generate(&env);
        let non_admin = Address::generate(&env);
        
        // Before initialization
        assert_eq!(
            AdminStorage::require_admin(&env, &admin),
            Err(QuickLendXError::OperationNotAllowed)
        );
        
        // After initialization
        AdminStorage::initialize(&env, &admin).unwrap();
        
        assert!(AdminStorage::require_admin(&env, &admin).is_ok());
        assert_eq!(
            AdminStorage::require_admin(&env, &non_admin),
            Err(QuickLendXError::NotAdmin)
        );
    }
}