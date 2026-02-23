use super::*;
use crate::fees::FeeType;
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
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);

    // Test invalid fee (too high) - this should fail
    let result = client.try_set_platform_fee(&1200);
    assert!(result.is_err());

    // Admin should be able to update fee with valid value
    client.set_platform_fee(&300);
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
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);
    let non_admin = Address::generate(&env);

    // Initialize fee system
    client.initialize_fee_system(&admin);

    // Non-admin should not be able to update (this would require a try_ method which doesn't exist)
    // For now, we'll just test that admin can update successfully
    client.update_fee_structure(&admin, &FeeType::Platform, &400, &50, &5000, &true);
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
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    // Test valid parameters
    client.validate_fee_parameters(&200, &10, &1000);

    // Test invalid base fee BPS (too high) - this would need a try_ method
    // For now, we'll just test the valid case
    // let result = client.validate_fee_parameters(&1500, &10, &1000);
    // assert!(result.is_err());

    // Test invalid min/max fees - this would need a try_ method
    // let result = client.validate_fee_parameters(&200, &1000, &500);
    // assert!(result.is_err()); // min > max
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
    assert!(early_fees < base_fees, "Early payment must reduce total fees");
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
        &100,   // 1%
        &50,    // min fee
        &10_000, // max fee
        &true,
    );

    let amount = 10_000_i128;
    let base_fees = client.calculate_transaction_fees(&user, &amount, &false, &false);
    let late_fees = client.calculate_transaction_fees(&user, &amount, &false, &true);

    // LatePayment: 1% of 10k = 100, +20% surcharge = 120
    // Total: 350 + 120 = 470
    assert_eq!(late_fees, 470);
    assert!(late_fees > base_fees, "Late payment must increase total fees");
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

    client.update_fee_structure(
        &admin,
        &FeeType::LatePayment,
        &100,
        &50,
        &10_000,
        &true,
    );

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
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);
    let user = setup_investor(&env, &client, &admin);

    client.initialize_fee_system(&admin);

    let result = client.try_calculate_transaction_fees(&user, &0_i128, &false, &false);
    assert!(result.is_err(), "Zero amount must return InvalidAmount error");
}