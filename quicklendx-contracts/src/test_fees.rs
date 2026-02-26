use super::*;
use crate::{errors::QuickLendXError, fees::FeeType};
use soroban_sdk::{testutils::Address as _, Address, Env, Map, String};

/// Helper function to set up admin for testing
fn setup_admin(env: &Env, client: &QuickLendXContractClient) -> Address {
    let admin = Address::generate(&env);
    client.set_admin(&admin);
    admin
}

/// Helper function to create and verify a business
fn setup_business(env: &Env, client: &QuickLendXContractClient, admin: &Address) -> Address {
    let business = Address::generate(&env);
    client.submit_kyc_application(&business, &String::from_str(env, "Business KYC"));
    client.verify_business(admin, &business);
    business
}

/// Helper function to create and verify an investor
fn setup_investor(env: &Env, client: &QuickLendXContractClient, admin: &Address) -> Address {
    let investor = Address::generate(&env);
    client.submit_investor_kyc(&investor, &String::from_str(env, "Investor KYC"));
    client.verify_investor(&investor, &1_000_000); // 1000 XLM limit
    investor
}

/// Simple test to verify the module is loaded
#[test]
fn test_module_loaded() {
    assert_eq!(2 + 2, 4);
}

/// Test default platform fee configuration (2%)
#[test]
fn test_default_platform_fee() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    // Get default platform fee config
    let fee_config = client.get_platform_fee();
    assert_eq!(fee_config.fee_bps, 200); // 2%
    assert_eq!(fee_config.updated_at, 0); // Not updated yet
    assert_eq!(fee_config.updated_by, contract_id); // Defaults to current contract address
}

/// FeeManager getter should fail before fee system initialization
#[test]
fn test_get_platform_fee_config_before_init_returns_storage_key_not_found() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let result = client.try_get_platform_fee_config();
    assert!(result.is_err());

    let err = result.err().expect("expected error");
    let contract_error = err.expect("expected contract invoke error");
    assert_eq!(contract_error, QuickLendXError::StorageKeyNotFound);
}

/// FeeManager getter returns defaults after initialization
#[test]
fn test_get_platform_fee_config_after_init_has_defaults() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);

    client.initialize_fee_system(&admin);

    let fee_config = client.get_platform_fee_config();
    assert_eq!(fee_config.fee_bps, 200);
    assert_eq!(fee_config.treasury_address, None);
    assert_eq!(fee_config.updated_by, admin);
    assert_eq!(fee_config.updated_at, env.ledger().timestamp());
}

/// FeeManager getter reflects updates from update_platform_fee_bps
#[test]
fn test_get_platform_fee_config_after_update_platform_fee_bps() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);

    client.initialize_fee_system(&admin);
    client.update_platform_fee_bps(&450);

    let fee_config = client.get_platform_fee_config();
    assert_eq!(fee_config.fee_bps, 450);
    assert_eq!(fee_config.treasury_address, None);
    assert_eq!(fee_config.updated_by, admin);
    assert_eq!(fee_config.updated_at, env.ledger().timestamp());
}

/// FeeManager getter should include treasury address when configured
#[test]
fn test_get_platform_fee_config_includes_treasury_when_set() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);
    let treasury = Address::generate(&env);

    client.initialize_fee_system(&admin);
    client.configure_treasury(&treasury);

    let fee_config = client.get_platform_fee_config();
    assert_eq!(fee_config.fee_bps, 200);
    assert_eq!(fee_config.treasury_address, Some(treasury.clone()));
    assert_eq!(fee_config.updated_by, admin);
    assert_eq!(client.get_treasury_address(), Some(treasury));
}

/// Test custom platform fee BPS configuration
#[test]
fn test_custom_platform_fee_bps() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);

    // Test setting custom fee BPS
    let new_fee_bps = 500; // 5%
    client.set_platform_fee(&new_fee_bps);

    let updated_config = client.get_platform_fee();
    assert_eq!(updated_config.fee_bps, new_fee_bps);
    assert_eq!(updated_config.updated_by, admin);
}

/// Test that only admin can update platform fee configuration
#[test]
fn test_only_admin_can_update_platform_fee() {
    let env = Env::default();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let attacker = Address::generate(&env);

    client.mock_all_auths().set_admin(&admin);

    // Non-admin cannot authorize admin-only platform fee update.
    let unauthorized_auth = MockAuth {
        address: &attacker,
        invoke: &MockAuthInvoke {
            contract: &contract_id,
            fn_name: "set_platform_fee",
            args: (300i128,).into_val(&env),
            sub_invokes: &[],
        },
    };
    let unauthorized_result = client
        .mock_auths(&[unauthorized_auth])
        .try_set_platform_fee(&300);
    let unauthorized_err = unauthorized_result
        .err()
        .expect("non-admin platform fee update must fail");
    let invoke_err = unauthorized_err
        .err()
        .expect("non-admin platform fee update should abort at auth");
    assert_eq!(invoke_err, soroban_sdk::InvokeError::Abort);

    // Stored fee stays unchanged after unauthorized attempt.
    let fee_after_reject = client.get_platform_fee();
    assert_eq!(fee_after_reject.fee_bps, 200);

    // Admin can authorize the same update.
    let admin_auth = MockAuth {
        address: &admin,
        invoke: &MockAuthInvoke {
            contract: &contract_id,
            fn_name: "set_platform_fee",
            args: (300i128,).into_val(&env),
            sub_invokes: &[],
        },
    };
    let admin_result = client.mock_auths(&[admin_auth]).try_set_platform_fee(&300);
    assert!(admin_result.is_ok());
    assert_eq!(client.get_platform_fee().fee_bps, 300);
}

/// Test platform fee calculation accuracy
#[test]
fn test_platform_fee_calculation() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    // Test default 2% fee calculation
    let investment_amount = 1000; // 1000 units
    let payment_amount = 1100; // 1100 units (100 profit)

    let (investor_return, platform_fee) =
        client.calculate_profit(&investment_amount, &payment_amount);

    // Expected: 2% of profit (100) = 2 units
    assert_eq!(platform_fee, 2);
    assert_eq!(investor_return, 1098); // 1100 - 2

    // Test with custom fee
    let admin = setup_admin(&env, &client);
    client.set_platform_fee(&500); // 5%

    let (investor_return, platform_fee) =
        client.calculate_profit(&investment_amount, &payment_amount);

    // Expected: 5% of profit (100) = 5 units
    assert_eq!(platform_fee, 5);
    assert_eq!(investor_return, 1095); // 1100 - 5
}

/// Test fee calculation edge cases
#[test]
fn test_platform_fee_edge_cases() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    // Test case: no profit (payment <= investment)
    let investment_amount = 1000;
    let payment_amount = 900; // Loss

    let (investor_return, platform_fee) =
        client.calculate_profit(&investment_amount, &payment_amount);

    assert_eq!(platform_fee, 0);
    assert_eq!(investor_return, 900);

    // Test case: zero payment
    let (investor_return, platform_fee) = client.calculate_profit(&investment_amount, &0);

    assert_eq!(platform_fee, 0);
    assert_eq!(investor_return, 0);
}

/// Test fee initialization
#[test]
fn test_fee_system_initialization() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);

    // Initialize fee system
    client.initialize_fee_system(&admin);

    // Verify fee structures are initialized
    let platform_fee = client.get_fee_structure(&FeeType::Platform);
    assert_eq!(platform_fee.base_fee_bps, 200); // 2%
    assert!(platform_fee.is_active);
}

/// Test fee structure updates
#[test]
fn test_fee_structure_updates() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);

    // Initialize fee system
    client.initialize_fee_system(&admin);

    // Update platform fee structure
    client.update_fee_structure(
        &admin,
        &FeeType::Platform,
        &300,  // 3%
        &50,   // min fee
        &5000, // max fee
        &true, // active
    );

    // Verify update
    let updated_fee = client.get_fee_structure(&FeeType::Platform);
    assert_eq!(updated_fee.base_fee_bps, 300);
    assert_eq!(updated_fee.min_fee, 50);
    assert_eq!(updated_fee.max_fee, 5000);
    assert!(updated_fee.is_active);
}

/// Test only admin can update fee structures
#[test]
fn test_only_admin_can_update_fee_structure() {
    let env = Env::default();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let attacker = Address::generate(&env);

    client.mock_all_auths().set_admin(&admin);

    let init_auth = MockAuth {
        address: &admin,
        invoke: &MockAuthInvoke {
            contract: &contract_id,
            fn_name: "initialize_fee_system",
            args: (admin.clone(),).into_val(&env),
            sub_invokes: &[],
        },
    };
    client
        .mock_auths(&[init_auth])
        .initialize_fee_system(&admin);

    // Non-admin cannot authorize fee structure update for admin identity.
    let unauthorized_auth = MockAuth {
        address: &attacker,
        invoke: &MockAuthInvoke {
            contract: &contract_id,
            fn_name: "update_fee_structure",
            args: (
                admin.clone(),
                FeeType::Platform,
                400u32,
                50i128,
                5_000i128,
                true,
            )
                .into_val(&env),
            sub_invokes: &[],
        },
    };
    let unauthorized_result = client
        .mock_auths(&[unauthorized_auth])
        .try_update_fee_structure(&admin, &FeeType::Platform, &400, &50, &5_000, &true);
    let unauthorized_err = unauthorized_result
        .err()
        .expect("non-admin fee structure update must fail");
    let invoke_err = unauthorized_err
        .err()
        .expect("non-admin fee structure update should abort at auth");
    assert_eq!(invoke_err, soroban_sdk::InvokeError::Abort);

    // Admin can update fee structure successfully.
    let admin_auth = MockAuth {
        address: &admin,
        invoke: &MockAuthInvoke {
            contract: &contract_id,
            fn_name: "update_fee_structure",
            args: (
                admin.clone(),
                FeeType::Platform,
                400u32,
                50i128,
                5_000i128,
                true,
            )
                .into_val(&env),
            sub_invokes: &[],
        },
    };
    let admin_result = client.mock_auths(&[admin_auth]).try_update_fee_structure(
        &admin,
        &FeeType::Platform,
        &400,
        &50,
        &5_000,
        &true,
    );
    assert!(admin_result.is_ok());

    let updated = client.get_fee_structure(&FeeType::Platform);
    assert_eq!(updated.base_fee_bps, 400);
    assert_eq!(updated.min_fee, 50);
    assert_eq!(updated.max_fee, 5_000);
}

/// Test transaction fee calculation
#[test]
fn test_transaction_fee_calculation() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);
    let user = setup_investor(&env, &client, &admin);

    // Initialize fee system
    client.initialize_fee_system(&admin);

    let transaction_amount = 10_000; // 10,000 units

    // Calculate fees for standard transaction
    let total_fees = client.calculate_transaction_fees(
        &user,
        &transaction_amount,
        &false, // not early payment
        &false, // not late payment
    );

    // Platform fee should be 2% of 10,000 = 200
    // Processing fee should be 0.5% of 10,000 = 50
    // Verification fee should be 1% of 10,000 = 100
    // Total: 350
    assert_eq!(total_fees, 350);
}

/// Test volume tier discounts
#[test]
fn test_volume_tier_discounts() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);
    let user = setup_investor(&env, &client, &admin);

    // Initialize fee system
    client.initialize_fee_system(&admin);

    // Get initial volume data (should be Standard tier)
    let volume_data = client.get_user_volume_data(&user);
    assert_eq!(volume_data.current_tier, crate::fees::VolumeTier::Standard);

    // Simulate high volume to reach Gold tier
    for _ in 0..6 {
        client.update_user_transaction_volume(&user, &100_000_000_000); // 100k XLM
    }

    let volume_data = client.get_user_volume_data(&user);
    assert_eq!(volume_data.current_tier, crate::fees::VolumeTier::Gold);

    // Calculate fees - should get 10% discount
    let transaction_amount = 10_000;
    let total_fees = client.calculate_transaction_fees(&user, &transaction_amount, &false, &false);

    // Standard fee would be 350, Gold tier gets 10% discount = 315
    assert_eq!(total_fees, 315);
}

/// Test early payment discounts
#[test]
fn test_early_payment_discounts() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);
    let user = setup_investor(&env, &client, &admin);

    // Initialize fee system
    client.initialize_fee_system(&admin);

    let transaction_amount = 10_000;

    // Calculate fees for early payment
    let early_payment_fees = client.calculate_transaction_fees(
        &user,
        &transaction_amount,
        &true, // early payment
        &false,
    );

    // Calculate fees for regular payment
    let regular_fees =
        client.calculate_transaction_fees(&user, &transaction_amount, &false, &false);

    // Early payment should have lower fees
    assert!(early_payment_fees < regular_fees);
}

/// Test late payment penalties
#[test]
fn test_late_payment_penalties() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);
    let user = setup_investor(&env, &client, &admin);

    // Initialize fee system
    client.initialize_fee_system(&admin);

    // Add LatePayment fee structure for testing penalties
    client.update_fee_structure(
        &admin,
        &FeeType::LatePayment,
        &100,  // 1%
        &50,   // min fee
        &1000, // max fee
        &true, // active
    );

    let transaction_amount = 10_000;

    // Calculate fees for late payment
    let late_payment_fees = client.calculate_transaction_fees(
        &user,
        &transaction_amount,
        &false,
        &true, // late payment
    );

    // Calculate fees for regular payment
    let regular_fees =
        client.calculate_transaction_fees(&user, &transaction_amount, &false, &false);

    // Late payment should have higher fees
    assert!(late_payment_fees > regular_fees);
}

/// Test revenue distribution configuration
#[test]
fn test_revenue_distribution_config() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);
    let treasury = Address::generate(&env);

    // Configure revenue distribution
    client.configure_revenue_distribution(
        &admin, &treasury, &5000, // 50% treasury
        &3000, // 30% developer
        &2000, // 20% platform
        &true, // auto distribution
        &1000, // min distribution amount
    );
}

/// Test revenue distribution execution
#[test]
fn test_revenue_distribution_execution() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);
    let user = setup_investor(&env, &client, &admin);
    let treasury = Address::generate(&env);

    // Initialize fee system
    client.initialize_fee_system(&admin);

    // Configure revenue distribution
    client.configure_revenue_distribution(
        &admin, &treasury, &6000,  // 60% treasury
        &2000,  // 20% developer
        &2000,  // 20% platform
        &false, // manual distribution
        &100,   // min distribution amount
    );

    // Collect some fees
    let mut fees_by_type = Map::new(&env);
    fees_by_type.set(FeeType::Platform, 200);
    fees_by_type.set(FeeType::Processing, 50);

    client.collect_transaction_fees(&user, &fees_by_type, &250);

    // Get current period
    let current_period = env.ledger().timestamp() / 2_592_000; // Weeks

    // Distribute revenue
    let (treasury_amount, developer_amount, platform_amount) =
        client.distribute_revenue(&admin, &current_period);

    // Verify distribution: 250 total * 60% = 150 treasury
    assert_eq!(treasury_amount, 150);
    assert_eq!(developer_amount, 50); // 250 * 20%
    assert_eq!(platform_amount, 50); // 250 * 20%
}

/// Test fee analytics
#[test]
fn test_fee_analytics() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);
    let user = setup_investor(&env, &client, &admin);

    // Initialize fee system
    client.initialize_fee_system(&admin);

    // Collect some fees
    let mut fees_by_type = Map::new(&env);
    fees_by_type.set(FeeType::Platform, 200);

    client.collect_transaction_fees(&user, &fees_by_type, &200);

    // Get current period
    let current_period = env.ledger().timestamp() / 2_592_000;

    // Get analytics
    let analytics = client.get_fee_analytics(&current_period);
    assert_eq!(analytics.total_fees, 200);
    assert_eq!(analytics.total_transactions, 1);
}

/// Test fee parameter validation
#[test]
fn test_fee_parameter_validation() {
    let env = Env::default();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    // Valid parameters should pass.
    client.validate_fee_parameters(&200, &10, &1000);

    // Invalid base fee BPS (over max 1000).
    let invalid_bps = client.try_validate_fee_parameters(&1001, &10, &1000);
    let invalid_bps_err = invalid_bps
        .err()
        .expect("base_fee_bps > 1000 must return contract error");
    let invalid_bps_contract_error = invalid_bps_err.expect("expected contract invoke error");
    assert_eq!(invalid_bps_contract_error, QuickLendXError::InvalidAmount);

    // Invalid range: min_fee > max_fee.
    let min_gt_max = client.try_validate_fee_parameters(&200, &1001, &1000);
    let min_gt_max_err = min_gt_max
        .err()
        .expect("min_fee > max_fee must return contract error");
    let min_gt_max_contract_error = min_gt_max_err.expect("expected contract invoke error");
    assert_eq!(min_gt_max_contract_error, QuickLendXError::InvalidAmount);

    // Invalid negative min_fee.
    let negative_min = client.try_validate_fee_parameters(&200, &-1, &1000);
    let negative_min_err = negative_min
        .err()
        .expect("negative min_fee must return contract error");
    let negative_min_contract_error = negative_min_err.expect("expected contract invoke error");
    assert_eq!(negative_min_contract_error, QuickLendXError::InvalidAmount);

    // Invalid negative max_fee.
    let negative_max = client.try_validate_fee_parameters(&200, &0, &-1);
    let negative_max_err = negative_max
        .err()
        .expect("negative max_fee must return contract error");
    let negative_max_contract_error = negative_max_err.expect("expected contract invoke error");
    assert_eq!(negative_max_contract_error, QuickLendXError::InvalidAmount);
}

/// Test fee config update validation rejects invalid fee parameters
#[test]
fn test_update_fee_structure_rejects_invalid_values() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);

    client.initialize_fee_system(&admin);

    let invalid_bps =
        client.try_update_fee_structure(&admin, &FeeType::Platform, &1001, &50, &5_000, &true);
    let invalid_bps_err = invalid_bps
        .err()
        .expect("base_fee_bps > 1000 must be rejected");
    let invalid_bps_contract_error = invalid_bps_err.expect("expected contract invoke error");
    assert_eq!(invalid_bps_contract_error, QuickLendXError::InvalidAmount);

    let min_gt_max =
        client.try_update_fee_structure(&admin, &FeeType::Platform, &400, &5_001, &5_000, &true);
    let min_gt_max_err = min_gt_max
        .err()
        .expect("min_fee > max_fee must be rejected");
    let min_gt_max_contract_error = min_gt_max_err.expect("expected contract invoke error");
    assert_eq!(min_gt_max_contract_error, QuickLendXError::InvalidAmount);

    let negative_min =
        client.try_update_fee_structure(&admin, &FeeType::Platform, &400, &-1, &5_000, &true);
    let negative_min_err = negative_min
        .err()
        .expect("negative min_fee must be rejected");
    let negative_min_contract_error = negative_min_err.expect("expected contract invoke error");
    assert_eq!(negative_min_contract_error, QuickLendXError::InvalidAmount);

    let negative_max =
        client.try_update_fee_structure(&admin, &FeeType::Platform, &400, &0, &-1, &true);
    let negative_max_err = negative_max
        .err()
        .expect("negative max_fee must be rejected");
    let negative_max_contract_error = negative_max_err.expect("expected contract invoke error");
    assert_eq!(negative_max_contract_error, QuickLendXError::InvalidAmount);
}

/// Test treasury receives exact amount in distribution
#[test]
fn test_treasury_receives_exact_amount() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);
    let user = setup_investor(&env, &client, &admin);
    let treasury = Address::generate(&env);

    // Initialize fee system
    client.initialize_fee_system(&admin);

    // Configure revenue distribution with 70% to treasury
    client.configure_revenue_distribution(
        &admin, &treasury, &7000, // 70% treasury
        &2000, // 20% developer
        &1000, // 10% platform
        &false, &100,
    );

    // Collect fees
    let mut fees_by_type = Map::new(&env);
    fees_by_type.set(FeeType::Platform, 1000);

    client.collect_transaction_fees(&user, &fees_by_type, &1000);

    // Get current period
    let current_period = env.ledger().timestamp() / 2_592_000;

    // Distribute revenue
    let (treasury_amount, _, _) = client.distribute_revenue(&admin, &current_period);

    // Treasury should receive exactly 70% of 1000 = 700
    assert_eq!(treasury_amount, 700);
}

/// Test comprehensive fee calculation with all factors
#[test]
fn test_comprehensive_fee_calculation() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);
    let user = setup_investor(&env, &client, &admin);

    // Initialize fee system
    client.initialize_fee_system(&admin);

    // Build up user volume to Platinum tier (15% discount)
    for _ in 0..20 {
        client.update_user_transaction_volume(&user, &500_000_000_000); // 500k XLM
    }

    let volume_data = client.get_user_volume_data(&user);
    assert_eq!(volume_data.current_tier, crate::fees::VolumeTier::Platinum);

    let transaction_amount = 50_000;

    // Test early payment with Platinum tier discount
    let fees = client.calculate_transaction_fees(
        &user,
        &transaction_amount,
        &true, // early payment
        &false,
    );

    // Calculate expected fees:
    // Platform: 2% of 50k = 1000, minus tier discount (15%) = 850, minus early payment (10%) = 765
    // Processing: 0.5% = 250, minus tier discount (15%) = 212.5 -> 213
    // Verification: 1% = 500, minus tier discount (15%) = 425
    // Total: 765 + 213 + 425 = 1403

    assert_eq!(fees, 1403);
}

// ─── calculate_transaction_fees: all flag combinations ───────────────────────

/// Base case: no flags set, Standard tier — verifies raw fee with no modifiers
#[test]
fn test_calculate_transaction_fees_base_case() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);
    let user = setup_investor(&env, &client, &admin);

    client.initialize_fee_system(&admin);

    let amount = 10_000_i128;
    let fees = client.calculate_transaction_fees(&user, &amount, &false, &false);

    // Platform 2% = 200, Processing 0.5% = 50, Verification 1% = 100 → total 350
    assert_eq!(fees, 350);
}

// ============================================================================
// Treasury Configuration Tests
// ============================================================================

/// Test configure_treasury sets treasury address correctly
#[test]
fn test_configure_treasury() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin_init(&env, &client);
    let treasury = Address::generate(&env);

    // Initialize fee system (creates platform fee config needed by configure_treasury)
    client.initialize_fee_system(&admin);

    // Configure treasury
    client.configure_treasury(&treasury);

    // Verify treasury address is set
    let treasury_addr = client.get_treasury_address();
    assert!(treasury_addr.is_some());
    assert_eq!(treasury_addr.unwrap(), treasury);
}

/// is_early_payment = true: Platform fee gets an extra 10% reduction
#[test]
fn test_calculate_transaction_fees_early_payment_flag() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);
    let user = setup_investor(&env, &client, &admin);

    client.initialize_fee_system(&admin);

    let amount = 10_000_i128;
    let base_fees = client.calculate_transaction_fees(&user, &amount, &false, &false);
    let early_fees = client.calculate_transaction_fees(&user, &amount, &true, &false);

    // Early payment applies 10% discount on Platform fee (200 → 180)
    // Total: 180 + 50 + 100 = 330
    assert_eq!(early_fees, 330);
    assert!(
        early_fees < base_fees,
        "Early payment must reduce total fees"
    );
}

/// Test get_treasury_address returns None before configuration
#[test]
fn test_get_treasury_address_before_config() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin_init(&env, &client);

    client.initialize_fee_system(&admin);

    // Treasury address should be None before configuration
    let treasury_addr = client.get_treasury_address();
    assert!(treasury_addr.is_none());
}

/// is_late_payment = true: LatePayment fee is added with 20% surcharge on top
#[test]
fn test_calculate_transaction_fees_late_payment_flag() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);
    let user = setup_investor(&env, &client, &admin);

    client.initialize_fee_system(&admin);

    // Add an active LatePayment fee structure
    client.update_fee_structure(
        &admin,
        &FeeType::LatePayment,
        &100,    // 1%
        &50,     // min fee
        &10_000, // max fee
        &true,
    );

    let amount = 10_000_i128;
    let base_fees = client.calculate_transaction_fees(&user, &amount, &false, &false);
    let late_fees = client.calculate_transaction_fees(&user, &amount, &false, &true);

    // LatePayment: 1% of 10k = 100, +20% surcharge = 120
    // Total: 350 + 120 = 470
    assert_eq!(late_fees, 470);
    assert!(
        late_fees > base_fees,
        "Late payment must increase total fees"
    );
}

/// Test treasury address is reflected in platform fee config
#[test]
fn test_treasury_address_in_platform_fee_config() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin_init(&env, &client);
    let treasury = Address::generate(&env);

    // Initialize fee system first
    client.initialize_fee_system(&admin);

    // Before treasury config, platform fee config should have no treasury
    let config_before = client.get_platform_fee_config();
    assert!(config_before.treasury_address.is_none());

    // Configure treasury
    client.configure_treasury(&treasury);

    // After treasury config, platform fee config should have treasury address
    let config_after = client.get_platform_fee_config();
    assert!(config_after.treasury_address.is_some());
    assert_eq!(config_after.treasury_address.unwrap(), treasury);
}

/// Both flags true: early payment discount AND late payment penalty applied together
#[test]
fn test_calculate_transaction_fees_both_flags() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);
    let user = setup_investor(&env, &client, &admin);

    client.initialize_fee_system(&admin);

    client.update_fee_structure(&admin, &FeeType::LatePayment, &100, &50, &10_000, &true);

    let amount = 10_000_i128;
    let both_flags_fees = client.calculate_transaction_fees(&user, &amount, &true, &true);
    let base_fees = client.calculate_transaction_fees(&user, &amount, &false, &false);

    // Both flags: early discount reduces platform fee AND late fee adds surcharge
    assert!(
        both_flags_fees != base_fees,
        "Both flags must change total fees"
    );
}

/// Test treasury address can be updated
#[test]
fn test_treasury_address_update() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin_init(&env, &client);
    let treasury1 = Address::generate(&env);
    let treasury2 = Address::generate(&env);

    // Initialize fee system first
    client.initialize_fee_system(&admin);

    // Set first treasury
    client.configure_treasury(&treasury1);
    assert_eq!(client.get_treasury_address().unwrap(), treasury1);

    // Update to second treasury
    client.configure_treasury(&treasury2);
    assert_eq!(client.get_treasury_address().unwrap(), treasury2);
}

/// Test configure_treasury fails without admin set
#[test]
fn test_configure_treasury_fails_without_admin() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let treasury = Address::generate(&env);

    // No admin set — should fail
    let result = client.try_configure_treasury(&treasury);
    assert!(result.is_err());
}

// ============================================================================
// Revenue Distribution Config Validation Tests
// ============================================================================

/// Helper: set up admin using initialize_admin (avoids double-auth issues)
fn setup_admin_init(env: &Env, client: &QuickLendXContractClient) -> Address {
    let admin = Address::generate(&env);
    client.initialize_admin(&admin);
    admin
}

/// Test revenue distribution config rejects shares not summing to 10000
#[test]
fn test_revenue_config_invalid_shares_sum() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin_init(&env, &client);
    let treasury = Address::generate(&env);

    // Shares sum to 9000 (not 10000) — should fail
    let result = client
        .try_configure_revenue_distribution(&admin, &treasury, &4000, &3000, &2000, &false, &100);
    assert!(result.is_err());
}

/// Test revenue distribution config rejects shares exceeding 10000
#[test]
fn test_revenue_config_shares_exceed_10000() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin_init(&env, &client);
    let treasury = Address::generate(&env);

    // Shares sum to 11000 — should fail
    let result = client
        .try_configure_revenue_distribution(&admin, &treasury, &5000, &3000, &3000, &false, &100);
    assert!(result.is_err());
}

/// Test get_revenue_split_config fails when not configured
#[test]
fn test_get_revenue_split_config_before_configuration() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    // No revenue config set — should fail
    let result = client.try_get_revenue_split_config();
    assert!(result.is_err());
}

/// Test revenue config can be reconfigured by admin
#[test]
fn test_revenue_config_reconfiguration() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin_init(&env, &client);
    let treasury = Address::generate(&env);

    // First configuration
    client.configure_revenue_distribution(&admin, &treasury, &5000, &3000, &2000, &false, &100);
    let config1 = client.get_revenue_split_config();
    assert_eq!(config1.treasury_share_bps, 5000);

    // Reconfigure with different shares
    client.configure_revenue_distribution(&admin, &treasury, &7000, &2000, &1000, &true, &500);
    let config2 = client.get_revenue_split_config();
    assert_eq!(config2.treasury_share_bps, 7000);
    assert_eq!(config2.developer_share_bps, 2000);
    assert_eq!(config2.platform_share_bps, 1000);
    assert_eq!(config2.auto_distribution, true);
    assert_eq!(config2.min_distribution_amount, 500);
}

// ============================================================================
// Revenue Distribution Execution Edge Cases
// ============================================================================

/// Test distribute_revenue fails when pending amount is below minimum
#[test]
fn test_distribute_revenue_below_minimum() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin_init(&env, &client);
    let user = Address::generate(&env);
    let treasury = Address::generate(&env);

    client.initialize_fee_system(&admin);

    // Configure with high minimum distribution amount
    client.configure_revenue_distribution(&admin, &treasury, &5000, &3000, &2000, &false, &10000);

    // Collect small amount of fees (below minimum)
    let mut fees_by_type = Map::new(&env);
    fees_by_type.set(FeeType::Platform, 50);
    client.collect_transaction_fees(&user, &fees_by_type, &50);

    let current_period = env.ledger().timestamp() / 2_592_000;

    // Distribution should fail — pending (50) < min_distribution_amount (10000)
    let result = client.try_distribute_revenue(&admin, &current_period);
    assert!(result.is_err());
}

/// Test distribute_revenue fails when revenue config is not set
#[test]
fn test_distribute_revenue_without_revenue_config() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin_init(&env, &client);
    let user = Address::generate(&env);

    client.initialize_fee_system(&admin);

    // Collect fees but don't configure revenue distribution
    let mut fees_by_type = Map::new(&env);
    fees_by_type.set(FeeType::Platform, 500);
    client.collect_transaction_fees(&user, &fees_by_type, &500);

    let current_period = env.ledger().timestamp() / 2_592_000;

    // Should fail — no revenue config set
    let result = client.try_distribute_revenue(&admin, &current_period);
    assert!(result.is_err());
}

/// Test distribute_revenue clears pending amount after distribution
#[test]
fn test_distribute_revenue_clears_pending() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin_init(&env, &client);
    let user = Address::generate(&env);
    let treasury = Address::generate(&env);

    client.initialize_fee_system(&admin);

    client.configure_revenue_distribution(&admin, &treasury, &5000, &3000, &2000, &false, &100);

    let mut fees_by_type = Map::new(&env);
    fees_by_type.set(FeeType::Platform, 1000);
    client.collect_transaction_fees(&user, &fees_by_type, &1000);

    let current_period = env.ledger().timestamp() / 2_592_000;

    // First distribution should succeed
    let (t, d, p) = client.distribute_revenue(&admin, &current_period);
    assert_eq!(t + d + p, 1000);

    // Second distribution should fail — pending is now 0, below min (100)
    let result = client.try_distribute_revenue(&admin, &current_period);
    assert!(result.is_err());
}

/// Test distribute_revenue fails for non-existent period
#[test]
fn test_distribute_revenue_nonexistent_period() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin_init(&env, &client);
    let treasury = Address::generate(&env);

    client.initialize_fee_system(&admin);

    client.configure_revenue_distribution(&admin, &treasury, &5000, &3000, &2000, &false, &100);

    // Try to distribute for a period with no revenue data
    let result = client.try_distribute_revenue(&admin, &9999);
    assert!(result.is_err());
}

/// Test revenue distribution amounts sum correctly for large values
#[test]
fn test_distribute_revenue_large_amounts() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin_init(&env, &client);
    let user = Address::generate(&env);
    let treasury = Address::generate(&env);

    client.initialize_fee_system(&admin);

    client.configure_revenue_distribution(&admin, &treasury, &5000, &3000, &2000, &false, &1);

    // Collect large fee amount
    let mut fees_by_type = Map::new(&env);
    fees_by_type.set(FeeType::Platform, 1_000_000);
    client.collect_transaction_fees(&user, &fees_by_type, &1_000_000);

    let current_period = env.ledger().timestamp() / 2_592_000;

    let (treasury_amount, developer_amount, platform_amount) =
        client.distribute_revenue(&admin, &current_period);

    // 50% of 1M = 500K
    assert_eq!(treasury_amount, 500_000);
    // 30% of 1M = 300K
    assert_eq!(developer_amount, 300_000);
    // Remainder = 200K
    assert_eq!(platform_amount, 200_000);
    // Total must equal original amount
    assert_eq!(
        treasury_amount + developer_amount + platform_amount,
        1_000_000
    );
}

// ============================================================================
// update_fee_structure Tests - Comprehensive Coverage
// ============================================================================

/// Test update_fee_structure with admin authorization
#[test]
fn test_update_fee_structure_with_admin() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin_init(&env, &client);

    client.initialize_fee_system(&admin);

    // Update Platform fee structure
    let updated = client.update_fee_structure(
        &admin,
        &FeeType::Platform,
        &350,   // 3.5% base fee
        &75,    // min fee
        &10000, // max fee
        &true,  // active
    );

    assert_eq!(updated.fee_type, FeeType::Platform);
    assert_eq!(updated.base_fee_bps, 350);
    assert_eq!(updated.min_fee, 75);
    assert_eq!(updated.max_fee, 10000);
    assert!(updated.is_active);
    assert_eq!(updated.updated_by, admin);
}

/// Test update_fee_structure for each FeeType
#[test]
fn test_update_fee_structure_all_fee_types() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin_init(&env, &client);

    client.initialize_fee_system(&admin);

    // Test Platform fee type
    let platform_fee =
        client.update_fee_structure(&admin, &FeeType::Platform, &250, &50, &5000, &true);
    assert_eq!(platform_fee.fee_type, FeeType::Platform);

    // Test Processing fee type
    let processing_fee =
        client.update_fee_structure(&admin, &FeeType::Processing, &75, &25, &2500, &true);
    assert_eq!(processing_fee.fee_type, FeeType::Processing);

    // Test Verification fee type
    let verification_fee =
        client.update_fee_structure(&admin, &FeeType::Verification, &150, &100, &3000, &true);
    assert_eq!(verification_fee.fee_type, FeeType::Verification);

    // Test EarlyPayment fee type
    let early_payment_fee =
        client.update_fee_structure(&admin, &FeeType::EarlyPayment, &50, &10, &1000, &true);
    assert_eq!(early_payment_fee.fee_type, FeeType::EarlyPayment);

    // Test LatePayment fee type
    let late_payment_fee =
        client.update_fee_structure(&admin, &FeeType::LatePayment, &200, &100, &5000, &true);
    assert_eq!(late_payment_fee.fee_type, FeeType::LatePayment);
}

/// Test update_fee_structure with various base_fee_bps values
#[test]
fn test_update_fee_structure_base_fee_bps_variations() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin_init(&env, &client);

    client.initialize_fee_system(&admin);

    // Test minimum valid base_fee_bps (0)
    let fee_zero = client.update_fee_structure(&admin, &FeeType::Platform, &0, &10, &1000, &true);
    assert_eq!(fee_zero.base_fee_bps, 0);

    // Test mid-range base_fee_bps
    let fee_mid = client.update_fee_structure(&admin, &FeeType::Platform, &500, &10, &1000, &true);
    assert_eq!(fee_mid.base_fee_bps, 500);

    // Test maximum valid base_fee_bps (1000 = 10%)
    let fee_max = client.update_fee_structure(&admin, &FeeType::Platform, &1000, &10, &1000, &true);
    assert_eq!(fee_max.base_fee_bps, 1000);
}

/// Test update_fee_structure rejects base_fee_bps exceeding MAX_FEE_BPS
#[test]
fn test_update_fee_structure_base_fee_bps_exceeds_max() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);
    let user = setup_investor(&env, &client, &admin);

    client.initialize_fee_system(&admin);

    client.update_fee_structure(&admin, &FeeType::LatePayment, &100, &50, &10_000, &true);

    let amount = 10_000_i128;
    let early_fees = client.calculate_transaction_fees(&user, &amount, &true, &false);
    let late_fees = client.calculate_transaction_fees(&user, &amount, &false, &true);
    let both_fees = client.calculate_transaction_fees(&user, &amount, &true, &true);

    // Platform 200 → early discount 10% → 180
    // Processing 50, Verification 100 unchanged
    // LatePayment 100 + 20% surcharge = 120
    // Total: 180 + 50 + 100 + 120 = 450
    assert_eq!(both_fees, 450);
    // Both flags should produce result between the two extremes
    assert!(both_fees > early_fees);
    assert!(both_fees < late_fees + early_fees); // sanity: not additive of both penalties
}

/// Volume tier discount applied correctly for Silver, Gold, and Platinum
#[test]
fn test_calculate_transaction_fees_volume_tier_discounts() {
    let admin = setup_admin_init(&env, &client);

    client.initialize_fee_system(&admin);

    // Test base_fee_bps > 1000 (MAX_FEE_BPS)
    let result =
        client.try_update_fee_structure(&admin, &FeeType::Platform, &1001, &10, &1000, &true);
    assert!(result.is_err());
    let err = result.err().unwrap();
    let contract_error = err.unwrap();
    assert_eq!(contract_error, QuickLendXError::InvalidAmount);
}

/// Test update_fee_structure with various min_fee values
#[test]
fn test_update_fee_structure_min_fee_variations() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin_init(&env, &client);

    client.initialize_fee_system(&admin);

    // Test min_fee = 0
    let fee_zero = client.update_fee_structure(&admin, &FeeType::Platform, &200, &0, &1000, &true);
    assert_eq!(fee_zero.min_fee, 0);

    // Test min_fee = 1
    let fee_one = client.update_fee_structure(&admin, &FeeType::Platform, &200, &1, &1000, &true);
    assert_eq!(fee_one.min_fee, 1);

    // Test large min_fee
    let fee_large =
        client.update_fee_structure(&admin, &FeeType::Platform, &200, &50000, &100000, &true);
    assert_eq!(fee_large.min_fee, 50000);
}

/// Test update_fee_structure rejects negative min_fee
#[test]
fn test_update_fee_structure_negative_min_fee() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin_init(&env, &client);

    client.initialize_fee_system(&admin);

    // Test negative min_fee
    let result =
        client.try_update_fee_structure(&admin, &FeeType::Platform, &200, &-1, &1000, &true);
    assert!(result.is_err());
    let err = result.err().unwrap();
    let contract_error = err.unwrap();
    assert_eq!(contract_error, QuickLendXError::InvalidAmount);
}

/// Test update_fee_structure with various max_fee values
#[test]
fn test_update_fee_structure_max_fee_variations() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin_init(&env, &client);

    client.initialize_fee_system(&admin);

    // Test max_fee equal to min_fee
    let fee_equal =
        client.update_fee_structure(&admin, &FeeType::Platform, &200, &100, &100, &true);
    assert_eq!(fee_equal.max_fee, 100);
    assert_eq!(fee_equal.min_fee, 100);

    // Test max_fee > min_fee
    let fee_greater =
        client.update_fee_structure(&admin, &FeeType::Platform, &200, &100, &5000, &true);
    assert_eq!(fee_greater.max_fee, 5000);

    // Test very large max_fee
    let fee_large =
        client.update_fee_structure(&admin, &FeeType::Platform, &200, &100, &10_000_000, &true);
    assert_eq!(fee_large.max_fee, 10_000_000);
}

/// Test update_fee_structure rejects max_fee < min_fee
#[test]
fn test_update_fee_structure_max_fee_less_than_min_fee() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin_init(&env, &client);

    client.initialize_fee_system(&admin);

    // Test max_fee < min_fee
    let result =
        client.try_update_fee_structure(&admin, &FeeType::Platform, &200, &1000, &500, &true);
    assert!(result.is_err());
    let err = result.err().unwrap();
    let contract_error = err.unwrap();
    assert_eq!(contract_error, QuickLendXError::InvalidAmount);
}

/// Test update_fee_structure with is_active true
#[test]
fn test_update_fee_structure_is_active_true() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin_init(&env, &client);

    client.initialize_fee_system(&admin);

    let fee = client.update_fee_structure(&admin, &FeeType::Platform, &200, &50, &1000, &true);
    assert!(fee.is_active);
}

/// Test update_fee_structure with is_active false (deactivate fee)
#[test]
fn test_update_fee_structure_is_active_false() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin_init(&env, &client);

    client.initialize_fee_system(&admin);

    // Deactivate Platform fee
    let fee = client.update_fee_structure(&admin, &FeeType::Platform, &200, &50, &1000, &false);
    assert!(!fee.is_active);
}

/// Test update_fee_structure can toggle is_active
#[test]
fn test_update_fee_structure_toggle_is_active() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);
    let user = setup_investor(&env, &client, &admin);

    client.initialize_fee_system(&admin);

    let amount = 10_000_i128;

    // Standard tier (no discount) — baseline
    let standard_fees = client.calculate_transaction_fees(&user, &amount, &false, &false);
    assert_eq!(standard_fees, 350);

    // Elevate to Silver tier (5% discount, total_volume >= 100_000_000_000)
    client.update_user_transaction_volume(&user, &100_000_000_000_i128);
    let silver_fees = client.calculate_transaction_fees(&user, &amount, &false, &false);
    // Each non-LatePayment fee reduced by 5%: 200*0.95=190, 50*0.95=47, 100*0.95=95 → 332
    assert_eq!(silver_fees, 332);
    assert!(silver_fees < standard_fees);

    // Elevate to Gold tier (10% discount, total_volume >= 500_000_000_000)
    client.update_user_transaction_volume(&user, &400_000_000_000_i128);
    let gold_fees = client.calculate_transaction_fees(&user, &amount, &false, &false);
    // 200*0.90=180, 50*0.90=45, 100*0.90=90 → 315
    assert_eq!(gold_fees, 315);
    assert!(gold_fees < silver_fees);

    // Elevate to Platinum tier (15% discount, total_volume >= 1_000_000_000_000)
    client.update_user_transaction_volume(&user, &500_000_000_000_i128);
    let platinum_fees = client.calculate_transaction_fees(&user, &amount, &false, &false);
    // 200*0.85=170, 50*0.85=42, 100*0.85=85 → 297
    assert_eq!(platinum_fees, 297);
    assert!(platinum_fees < gold_fees);
}

/// Zero amount must return an error
#[test]
fn test_calculate_transaction_fees_zero_amount() {
    let admin = setup_admin_init(&env, &client);

    client.initialize_fee_system(&admin);

    // Activate
    let fee_active =
        client.update_fee_structure(&admin, &FeeType::Platform, &200, &50, &1000, &true);
    assert!(fee_active.is_active);

    // Deactivate
    let fee_inactive =
        client.update_fee_structure(&admin, &FeeType::Platform, &200, &50, &1000, &false);
    assert!(!fee_inactive.is_active);
}

/// Test update_fee_structure creates new fee type if not exists
#[test]
fn test_update_fee_structure_creates_new_fee_type() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin_init(&env, &client);

    client.initialize_fee_system(&admin);

    // EarlyPayment fee type doesn't exist by default
    let result = client.try_get_fee_structure(&FeeType::EarlyPayment);
    assert!(result.is_err());

    // Create it via update_fee_structure
    let early_payment_fee =
        client.update_fee_structure(&admin, &FeeType::EarlyPayment, &50, &10, &500, &true);
    assert_eq!(early_payment_fee.fee_type, FeeType::EarlyPayment);

    // Now it should exist
    let retrieved = client.get_fee_structure(&FeeType::EarlyPayment);
    assert_eq!(retrieved.base_fee_bps, 50);
}

/// Test update_fee_structure updates existing fee type
#[test]
fn test_update_fee_structure_updates_existing() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin_init(&env, &client);

    client.initialize_fee_system(&admin);

    // Get initial Platform fee
    let initial = client.get_fee_structure(&FeeType::Platform);
    assert_eq!(initial.base_fee_bps, 200);

    // Update it
    client.update_fee_structure(&admin, &FeeType::Platform, &350, &75, &7500, &true);

    // Verify update
    let updated = client.get_fee_structure(&FeeType::Platform);
    assert_eq!(updated.base_fee_bps, 350);
    assert_eq!(updated.min_fee, 75);
    assert_eq!(updated.max_fee, 7500);
}

/// Test update_fee_structure sets updated_at timestamp
#[test]
fn test_update_fee_structure_sets_updated_at() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin_init(&env, &client);

    client.initialize_fee_system(&admin);

    let fee = client.update_fee_structure(&admin, &FeeType::Platform, &200, &50, &1000, &true);

    // updated_at should be set to current ledger timestamp
    assert_eq!(fee.updated_at, env.ledger().timestamp());
}

/// Test update_fee_structure sets updated_by to admin
#[test]
fn test_update_fee_structure_sets_updated_by() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin_init(&env, &client);

    client.initialize_fee_system(&admin);

    let fee = client.update_fee_structure(&admin, &FeeType::Platform, &200, &50, &1000, &true);

    assert_eq!(fee.updated_by, admin);
}

// ============================================================================
// validate_fee_parameters Tests - Comprehensive Coverage
// ============================================================================

/// Test validate_fee_parameters with valid parameters
#[test]
fn test_validate_fee_parameters_valid() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    // Valid parameters: base_fee_bps=200, min_fee=10, max_fee=1000
    client.validate_fee_parameters(&200, &10, &1000);
}

/// Test validate_fee_parameters with base_fee_bps at minimum (0)
#[test]
fn test_validate_fee_parameters_base_fee_bps_zero() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    // base_fee_bps = 0 is valid
    client.validate_fee_parameters(&0, &10, &1000);
}

/// Test validate_fee_parameters with base_fee_bps at maximum (1000)
#[test]
fn test_validate_fee_parameters_base_fee_bps_max() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    // base_fee_bps = 1000 (MAX_FEE_BPS) is valid
    client.validate_fee_parameters(&1000, &10, &1000);
}

/// Test validate_fee_parameters rejects base_fee_bps exceeding MAX_FEE_BPS
#[test]
fn test_validate_fee_parameters_base_fee_bps_exceeds_max() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    // base_fee_bps = 1001 exceeds MAX_FEE_BPS (1000)
    let result = client.try_validate_fee_parameters(&1001, &10, &1000);
    assert!(result.is_err());
    let err = result.err().unwrap();
    let contract_error = err.unwrap();
    assert_eq!(contract_error, QuickLendXError::InvalidAmount);
}

/// Test validate_fee_parameters rejects base_fee_bps far exceeding MAX_FEE_BPS
#[test]
fn test_validate_fee_parameters_base_fee_bps_far_exceeds_max() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    // base_fee_bps = 10000 far exceeds MAX_FEE_BPS
    let result = client.try_validate_fee_parameters(&10000, &10, &1000);
    assert!(result.is_err());
    let err = result.err().unwrap();
    let contract_error = err.unwrap();
    assert_eq!(contract_error, QuickLendXError::InvalidAmount);
}

/// Test validate_fee_parameters with min_fee = 0
#[test]
fn test_validate_fee_parameters_min_fee_zero() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    // min_fee = 0 is valid
    client.validate_fee_parameters(&200, &0, &1000);
}

/// Test validate_fee_parameters rejects negative min_fee
#[test]
fn test_validate_fee_parameters_negative_min_fee() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    // min_fee = -1 is invalid
    let result = client.try_validate_fee_parameters(&200, &-1, &1000);
    assert!(result.is_err());
    let err = result.err().unwrap();
    let contract_error = err.unwrap();
    assert_eq!(contract_error, QuickLendXError::InvalidAmount);
}

/// Test validate_fee_parameters rejects large negative min_fee
#[test]
fn test_validate_fee_parameters_large_negative_min_fee() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    // min_fee = -1000 is invalid
    let result = client.try_validate_fee_parameters(&200, &-1000, &1000);
    assert!(result.is_err());
    let err = result.err().unwrap();
    let contract_error = err.unwrap();
    assert_eq!(contract_error, QuickLendXError::InvalidAmount);
}

/// Test validate_fee_parameters with max_fee = 0
#[test]
fn test_validate_fee_parameters_max_fee_zero() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    // max_fee = 0 is valid if min_fee = 0
    client.validate_fee_parameters(&200, &0, &0);
}

/// Test validate_fee_parameters rejects negative max_fee
#[test]
fn test_validate_fee_parameters_negative_max_fee() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    // max_fee = -1 is invalid
    let result = client.try_validate_fee_parameters(&200, &10, &-1);
    assert!(result.is_err());
    let err = result.err().unwrap();
    let contract_error = err.unwrap();
    assert_eq!(contract_error, QuickLendXError::InvalidAmount);
}

/// Test validate_fee_parameters rejects min_fee > max_fee
#[test]
fn test_validate_fee_parameters_min_greater_than_max() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    // min_fee (1000) > max_fee (500) is invalid
    let result = client.try_validate_fee_parameters(&200, &1000, &500);
    assert!(result.is_err());
    let err = result.err().unwrap();
    let contract_error = err.unwrap();
    assert_eq!(contract_error, QuickLendXError::InvalidAmount);
}

/// Test validate_fee_parameters with min_fee = max_fee (edge case)
#[test]
fn test_validate_fee_parameters_min_equals_max() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    // min_fee = max_fee is valid
    client.validate_fee_parameters(&200, &500, &500);
}

/// Test validate_fee_parameters with large valid values
#[test]
fn test_validate_fee_parameters_large_valid_values() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    // Large but valid values
    client.validate_fee_parameters(&999, &1_000_000, &100_000_000);
}

/// Test validate_fee_parameters rejects multiple invalid conditions
#[test]
fn test_validate_fee_parameters_multiple_invalid_conditions() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    // base_fee_bps exceeds max AND min_fee > max_fee
    let result = client.try_validate_fee_parameters(&1500, &1000, &500);
    assert!(result.is_err());
    let err = result.err().unwrap();
    let contract_error = err.unwrap();
    assert_eq!(contract_error, QuickLendXError::InvalidAmount);
}

/// Test validate_fee_parameters with boundary values
#[test]
fn test_validate_fee_parameters_boundary_values() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    // All boundary values: base_fee_bps=1000, min_fee=0, max_fee=i128::MAX
    client.validate_fee_parameters(&1000, &0, &i128::MAX);
}

/// Test validate_fee_parameters rejects both negative min and max fees
#[test]
fn test_validate_fee_parameters_both_negative() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    // Both min_fee and max_fee negative
    let result = client.try_validate_fee_parameters(&200, &-10, &-5);
    assert!(result.is_err());
    let err = result.err().unwrap();
    let contract_error = err.unwrap();
    assert_eq!(contract_error, QuickLendXError::InvalidAmount);
}

/// Test validate_fee_parameters with realistic production values
#[test]
fn test_validate_fee_parameters_realistic_values() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    // Realistic production values
    client.validate_fee_parameters(&250, &100, &50000); // 2.5%, min 100, max 50000
    client.validate_fee_parameters(&50, &25, &10000); // 0.5%, min 25, max 10000
    client.validate_fee_parameters(&100, &50, &25000); // 1%, min 50, max 25000
}
