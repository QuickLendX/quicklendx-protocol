#![cfg(test)]

use crate::fees::FeeManager;
use crate::{QuickLendXContract, QuickLendXError};
use soroban_sdk::{testutils::Address as _, Address, Env};

#[test]
fn test_initialize_fee_system() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, QuickLendXContract);
    let admin = Address::generate(&env);

    // Initialize fee system
    let result = env.as_contract(&contract_id, || {
        FeeManager::initialize(&env, &admin)
    });
    assert!(result.is_ok());

    // Verify platform fee config was created
    let config = env.as_contract(&contract_id, || {
        FeeManager::get_platform_fee_config(&env)
    });
    assert!(config.is_ok());
    
    let config = config.unwrap();
    assert_eq!(config.fee_bps, 200); // Default 2%
    assert!(config.treasury_address.is_none());
    assert_eq!(config.updated_by, admin);
}

#[test]
fn test_configure_treasury() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, QuickLendXContract);
    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);

    // Initialize first
    env.as_contract(&contract_id, || {
        FeeManager::initialize(&env, &admin)
    }).unwrap();

    // Configure treasury
    let result = env.as_contract(&contract_id, || {
        FeeManager::configure_treasury(&env, &admin, treasury.clone())
    });
    assert!(result.is_ok());

    // Verify treasury address is retrievable
    let retrieved_treasury = env.as_contract(&contract_id, || {
        FeeManager::get_treasury_address(&env)
    });
    assert!(retrieved_treasury.is_some());
    assert_eq!(retrieved_treasury.unwrap(), treasury);
}

#[test]
fn test_update_platform_fee() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, QuickLendXContract);
    let admin = Address::generate(&env);

    // Initialize first
    env.as_contract(&contract_id, || {
        FeeManager::initialize(&env, &admin)
    }).unwrap();

    // Update fee to 3%
    let result = env.as_contract(&contract_id, || {
        FeeManager::update_platform_fee(&env, &admin, 300)
    });
    assert!(result.is_ok());

    let config = result.unwrap();
    assert_eq!(config.fee_bps, 300);
    assert_eq!(config.updated_by, admin);

    // Verify the change persisted
    let retrieved_config = env.as_contract(&contract_id, || {
        FeeManager::get_platform_fee_config(&env)
    }).unwrap();
    assert_eq!(retrieved_config.fee_bps, 300);
}

#[test]
fn test_update_platform_fee_invalid_amount() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, QuickLendXContract);
    let admin = Address::generate(&env);

    // Initialize first
    env.as_contract(&contract_id, || {
        FeeManager::initialize(&env, &admin)
    }).unwrap();

    // Try to set fee above maximum (10%)
    let result = env.as_contract(&contract_id, || {
        FeeManager::update_platform_fee(&env, &admin, 1500)
    });
    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), QuickLendXError::InvalidAmount);

    // Verify original fee is unchanged
    let config = env.as_contract(&contract_id, || {
        FeeManager::get_platform_fee_config(&env)
    }).unwrap();
    assert_eq!(config.fee_bps, 200); // Still default 2%
}

#[test]
fn test_calculate_platform_fee() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, QuickLendXContract);
    let admin = Address::generate(&env);

    // Initialize with default 2% fee
    env.as_contract(&contract_id, || {
        FeeManager::initialize(&env, &admin)
    }).unwrap();

    // Test profitable transaction
    let investment_amount = 1000;
    let payment_amount = 1200; // 200 profit
    
    let result = env.as_contract(&contract_id, || {
        FeeManager::calculate_platform_fee(&env, investment_amount, payment_amount)
    });
    assert!(result.is_ok());

    let (investor_return, platform_fee) = result.unwrap();
    assert_eq!(platform_fee, 4); // 2% of 200 profit = 4
    assert_eq!(investor_return, 1196); // 1200 - 4 = 1196

    // Test break-even transaction
    let result = env.as_contract(&contract_id, || {
        FeeManager::calculate_platform_fee(&env, 1000, 1000)
    });
    assert!(result.is_ok());

    let (investor_return, platform_fee) = result.unwrap();
    assert_eq!(platform_fee, 0); // No profit, no fee
    assert_eq!(investor_return, 1000);

    // Test loss transaction
    let result = env.as_contract(&contract_id, || {
        FeeManager::calculate_platform_fee(&env, 1000, 800)
    });
    assert!(result.is_ok());

    let (investor_return, platform_fee) = result.unwrap();
    assert_eq!(platform_fee, 0); // No profit, no fee
    assert_eq!(investor_return, 800);
}

#[test]
fn test_calculate_platform_fee_with_updated_rate() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, QuickLendXContract);
    let admin = Address::generate(&env);

    // Initialize and update fee to 5%
    env.as_contract(&contract_id, || {
        FeeManager::initialize(&env, &admin)
    }).unwrap();
    env.as_contract(&contract_id, || {
        FeeManager::update_platform_fee(&env, &admin, 500)
    }).unwrap();

    // Test with new rate
    let investment_amount = 1000;
    let payment_amount = 1200; // 200 profit
    
    let result = env.as_contract(&contract_id, || {
        FeeManager::calculate_platform_fee(&env, investment_amount, payment_amount)
    });
    assert!(result.is_ok());

    let (investor_return, platform_fee) = result.unwrap();
    assert_eq!(platform_fee, 10); // 5% of 200 profit = 10
    assert_eq!(investor_return, 1190); // 1200 - 10 = 1190
}

#[test]
fn test_treasury_not_configured() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, QuickLendXContract);
    let admin = Address::generate(&env);

    // Initialize without configuring treasury
    env.as_contract(&contract_id, || {
        FeeManager::initialize(&env, &admin)
    }).unwrap();

    // Treasury address should be None
    let treasury = env.as_contract(&contract_id, || {
        FeeManager::get_treasury_address(&env)
    });
    assert!(treasury.is_none());
}

#[test]
fn test_fee_system_not_initialized() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, QuickLendXContract);

    // Try to get config without initialization
    let result = env.as_contract(&contract_id, || {
        FeeManager::get_platform_fee_config(&env)
    });
    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), QuickLendXError::StorageKeyNotFound);
}

#[test]
fn test_large_amounts() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, QuickLendXContract);
    let admin = Address::generate(&env);

    // Initialize with default 2% fee
    env.as_contract(&contract_id, || {
        FeeManager::initialize(&env, &admin)
    }).unwrap();

    // Test with large amounts
    let investment_amount = 1_000_000_000; // 1 billion
    let payment_amount = 1_100_000_000;   // 1.1 billion (100M profit)
    
    let result = env.as_contract(&contract_id, || {
        FeeManager::calculate_platform_fee(&env, investment_amount, payment_amount)
    });
    assert!(result.is_ok());

    let (investor_return, platform_fee) = result.unwrap();
    assert_eq!(platform_fee, 2_000_000); // 2% of 100M profit = 2M
    assert_eq!(investor_return, 1_098_000_000); // 1.1B - 2M = 1.098B
}

#[test]
fn test_zero_amounts() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, QuickLendXContract);
    let admin = Address::generate(&env);

    // Initialize with default 2% fee
    env.as_contract(&contract_id, || {
        FeeManager::initialize(&env, &admin)
    }).unwrap();

    // Test with zero amounts
    let result = env.as_contract(&contract_id, || {
        FeeManager::calculate_platform_fee(&env, 0, 0)
    });
    assert!(result.is_ok());

    let (investor_return, platform_fee) = result.unwrap();
    assert_eq!(platform_fee, 0);
    assert_eq!(investor_return, 0);
}

#[test]
fn test_fee_precision() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, QuickLendXContract);
    let admin = Address::generate(&env);

    // Initialize with 1% fee (100 bps)
    env.as_contract(&contract_id, || {
        FeeManager::initialize(&env, &admin)
    }).unwrap();
    env.as_contract(&contract_id, || {
        FeeManager::update_platform_fee(&env, &admin, 100)
    }).unwrap();

    // Test with amount that results in fractional fee
    let investment_amount = 1000;
    let payment_amount = 1003; // 3 profit
    
    let result = env.as_contract(&contract_id, || {
        FeeManager::calculate_platform_fee(&env, investment_amount, payment_amount)
    });
    assert!(result.is_ok());

    let (investor_return, platform_fee) = result.unwrap();
    // 1% of 3 = 0.03, should round down to 0 in integer math
    assert_eq!(platform_fee, 0);
    assert_eq!(investor_return, 1003);

    // Test with larger profit for visible fee
    let payment_amount = 1100; // 100 profit
    let result = env.as_contract(&contract_id, || {
        FeeManager::calculate_platform_fee(&env, investment_amount, payment_amount)
    });
    assert!(result.is_ok());

    let (investor_return, platform_fee) = result.unwrap();
    assert_eq!(platform_fee, 1); // 1% of 100 = 1
    assert_eq!(investor_return, 1099);
}