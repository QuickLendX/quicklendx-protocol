use crate::fees::FeeType;
use crate::QuickLendXContract;
use crate::QuickLendXContractClient;
#[cfg(test)]
use soroban_sdk::{testutils::Address as _, Address, Env, Map};

fn setup_admin(env: &Env, client: &QuickLendXContractClient) -> Address {
    let admin = Address::generate(&env);
    client.initialize_admin(&admin);
    admin
}

#[test]
fn test_50_50_split() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);
    let treasury = Address::generate(&env);
    let user = Address::generate(&env);

    // Initialize fee system
    client.initialize_fee_system(&admin);

    // Configure revenue distribution: 50% Treasury, 50% Platform, 0% Developer
    client.configure_revenue_distribution(
        &admin, &treasury, &5000, // 50% Treasury
        &0,    // 0% Developer
        &5000, // 50% Platform
        &false, &100,
    );

    // Collect fees
    let mut fees_by_type = Map::new(&env);
    fees_by_type.set(FeeType::Platform, 1000);
    client.collect_transaction_fees(&user, &fees_by_type, &1000);

    let current_period = env.ledger().timestamp() / 2_592_000;

    let (treasury_amount, developer_amount, platform_amount) =
        client.distribute_revenue(&admin, &current_period);

    assert_eq!(treasury_amount, 500);
    assert_eq!(platform_amount, 500);
    assert_eq!(developer_amount, 0);
}

#[test]
fn test_60_20_20_split() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);
    let treasury = Address::generate(&env);
    let user = Address::generate(&env);

    client.initialize_fee_system(&admin);

    // 60% Treasury, 20% Developer, 20% Platform
    client.configure_revenue_distribution(&admin, &treasury, &6000, &2000, &2000, &false, &100);

    let mut fees_by_type = Map::new(&env);
    fees_by_type.set(FeeType::Platform, 1000);
    client.collect_transaction_fees(&user, &fees_by_type, &1000);

    let current_period = env.ledger().timestamp() / 2_592_000;

    let (treasury_amount, developer_amount, platform_amount) =
        client.distribute_revenue(&admin, &current_period);

    assert_eq!(treasury_amount, 600);
    assert_eq!(developer_amount, 200);
    assert_eq!(platform_amount, 200);
}

#[test]
fn test_rounding() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);
    let treasury = Address::generate(&env);
    let user = Address::generate(&env);

    client.initialize_fee_system(&admin);

    // 33% Treasury, 33% Developer, 34% Platform (Sum=100%)
    client.configure_revenue_distribution(&admin, &treasury, &3300, &3300, &3400, &false, &1);

    // Collect 100 units key
    let mut fees_by_type = Map::new(&env);
    fees_by_type.set(FeeType::Platform, 100);
    client.collect_transaction_fees(&user, &fees_by_type, &100);

    let current_period = env.ledger().timestamp() / 2_592_000;

    // Debug print
    // std::println!("Distributing revenue for period: {}", current_period);

    let (treasury_amount, developer_amount, platform_amount) =
        client.distribute_revenue(&admin, &current_period);

    // std::println!("Amounts: T={}, D={}, P={}", treasury_amount, developer_amount, platform_amount);

    // 33% of 100 = 33
    // 33% of 100 = 33
    // Remaining = 100 - 33 - 33 = 34

    assert_eq!(
        treasury_amount, 33,
        "Treasury amount incorrect: expected 33, got {}",
        treasury_amount
    );
    assert_eq!(
        developer_amount, 33,
        "Developer amount incorrect: expected 33, got {}",
        developer_amount
    );
    assert_eq!(
        platform_amount, 34,
        "Platform amount incorrect: expected 34, got {}",
        platform_amount
    );

    assert_eq!(
        treasury_amount + developer_amount + platform_amount,
        100,
        "Total sum incorrect: got {}",
        treasury_amount + developer_amount + platform_amount
    );
}

#[test]
fn test_only_admin_can_update_config() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);
    let treasury = Address::generate(&env);
    let non_admin = Address::generate(&env);

    client.initialize_fee_system(&admin);

    let result = client
        .try_configure_revenue_distribution(&non_admin, &treasury, &5000, &0, &5000, &false, &100);

    assert!(result.is_err(), "Should fail for non-admin");

    // Verify admin can do it
    let result_admin = client
        .try_configure_revenue_distribution(&admin, &treasury, &5000, &0, &5000, &false, &100);
    assert!(result_admin.is_ok(), "Should succeed for admin");
}

#[test]
fn test_get_revenue_split_config() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);
    let treasury = Address::generate(&env);

    client.initialize_fee_system(&admin);

    // Configure revenue distribution
    client.configure_revenue_distribution(
        &admin, &treasury, &6000, // 60% Treasury
        &2500, // 25% Developer
        &1500, // 15% Platform
        &true, &500,
    );

    // Query and verify configuration
    let config = client.get_revenue_split_config();
    assert_eq!(config.treasury_share_bps, 6000);
    assert_eq!(config.developer_share_bps, 2500);
    assert_eq!(config.platform_share_bps, 1500);
    assert_eq!(config.auto_distribution, true);
    assert_eq!(config.min_distribution_amount, 500);
}

// ============================================================================
// Treasury and Revenue Config – Additional Tests
// ============================================================================

#[test]
fn test_distribute_revenue_requires_config() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);
    let user = Address::generate(&env);

    client.initialize_fee_system(&admin);

    // Collect fees without configuring revenue distribution
    let mut fees_by_type = Map::new(&env);
    fees_by_type.set(FeeType::Platform, 500);
    client.collect_transaction_fees(&user, &fees_by_type, &500);

    let current_period = env.ledger().timestamp() / 2_592_000;

    // Should fail — no revenue config set
    let result = client.try_distribute_revenue(&admin, &current_period);
    assert!(result.is_err(), "Should fail without revenue config");

    // Now configure and verify it works
    let treasury = Address::generate(&env);
    client.configure_revenue_distribution(&admin, &treasury, &5000, &2500, &2500, &false, &100);

    let result = client.try_distribute_revenue(&admin, &current_period);
    assert!(result.is_ok(), "Should succeed after revenue config is set");
}

#[test]
fn test_invalid_shares_not_summing_to_10000() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);
    let treasury = Address::generate(&env);

    // Sum = 8000 (not 10000)
    let result = client
        .try_configure_revenue_distribution(&admin, &treasury, &3000, &3000, &2000, &false, &100);
    assert!(result.is_err(), "Shares not summing to 10000 should fail");

    // Sum = 12000
    let result = client
        .try_configure_revenue_distribution(&admin, &treasury, &5000, &4000, &3000, &false, &100);
    assert!(result.is_err(), "Shares exceeding 10000 should fail");
}

#[test]
fn test_100_percent_treasury_split() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);
    let treasury = Address::generate(&env);
    let user = Address::generate(&env);

    client.initialize_fee_system(&admin);

    // 100% to treasury
    client.configure_revenue_distribution(&admin, &treasury, &10000, &0, &0, &false, &1);

    let mut fees_by_type = Map::new(&env);
    fees_by_type.set(FeeType::Platform, 777);
    client.collect_transaction_fees(&user, &fees_by_type, &777);

    let current_period = env.ledger().timestamp() / 2_592_000;

    let (treasury_amount, developer_amount, platform_amount) =
        client.distribute_revenue(&admin, &current_period);

    assert_eq!(treasury_amount, 777);
    assert_eq!(developer_amount, 0);
    assert_eq!(platform_amount, 0);
}

#[test]
fn test_100_percent_developer_split() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);
    let treasury = Address::generate(&env);
    let user = Address::generate(&env);

    client.initialize_fee_system(&admin);

    // 100% to developer
    client.configure_revenue_distribution(&admin, &treasury, &0, &10000, &0, &false, &1);

    let mut fees_by_type = Map::new(&env);
    fees_by_type.set(FeeType::Platform, 500);
    client.collect_transaction_fees(&user, &fees_by_type, &500);

    let current_period = env.ledger().timestamp() / 2_592_000;

    let (treasury_amount, developer_amount, platform_amount) =
        client.distribute_revenue(&admin, &current_period);

    assert_eq!(treasury_amount, 0);
    assert_eq!(developer_amount, 500);
    assert_eq!(platform_amount, 0);
}

#[test]
fn test_revenue_config_reconfiguration() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);
    let treasury = Address::generate(&env);

    client.initialize_fee_system(&admin);

    // First config
    client.configure_revenue_distribution(&admin, &treasury, &5000, &3000, &2000, &false, &100);
    let config = client.get_revenue_split_config();
    assert_eq!(config.treasury_share_bps, 5000);

    // Reconfigure
    client.configure_revenue_distribution(&admin, &treasury, &8000, &1000, &1000, &true, &50);
    let config = client.get_revenue_split_config();
    assert_eq!(config.treasury_share_bps, 8000);
    assert_eq!(config.developer_share_bps, 1000);
    assert_eq!(config.platform_share_bps, 1000);
    assert_eq!(config.auto_distribution, true);
    assert_eq!(config.min_distribution_amount, 50);
}

#[test]
fn test_revenue_config_treasury_address_stored() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);
    let treasury = Address::generate(&env);

    client.initialize_fee_system(&admin);

    client.configure_revenue_distribution(&admin, &treasury, &5000, &3000, &2000, &false, &100);

    let config = client.get_revenue_split_config();
    assert_eq!(config.treasury_address, treasury);
}

#[test]
fn test_accumulated_fees_distribution() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);
    let treasury = Address::generate(&env);
    let user = Address::generate(&env);

    client.initialize_fee_system(&admin);

    client.configure_revenue_distribution(&admin, &treasury, &5000, &3000, &2000, &false, &1);

    // Collect fees in multiple transactions
    for _ in 0..5 {
        let mut fees_by_type = Map::new(&env);
        fees_by_type.set(FeeType::Platform, 200);
        client.collect_transaction_fees(&user, &fees_by_type, &200);
    }

    let current_period = env.ledger().timestamp() / 2_592_000;

    let (treasury_amount, developer_amount, platform_amount) =
        client.distribute_revenue(&admin, &current_period);

    // Total collected = 5 * 200 = 1000
    assert_eq!(treasury_amount, 500); // 50%
    assert_eq!(developer_amount, 300); // 30%
    assert_eq!(platform_amount, 200); // 20%
    assert_eq!(treasury_amount + developer_amount + platform_amount, 1000);
}

#[test]
fn test_distribute_revenue_no_revenue_data_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);
    let treasury = Address::generate(&env);

    client.initialize_fee_system(&admin);

    client.configure_revenue_distribution(&admin, &treasury, &5000, &3000, &2000, &false, &100);

    // No fees collected — period has no data
    let result = client.try_distribute_revenue(&admin, &9999);
    assert!(
        result.is_err(),
        "Should fail when no revenue data exists for period"
    );
}

#[test]
fn test_double_distribution_same_period_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);
    let treasury = Address::generate(&env);
    let user = Address::generate(&env);

    client.initialize_fee_system(&admin);

    client.configure_revenue_distribution(&admin, &treasury, &5000, &3000, &2000, &false, &100);

    let mut fees_by_type = Map::new(&env);
    fees_by_type.set(FeeType::Platform, 1000);
    client.collect_transaction_fees(&user, &fees_by_type, &1000);

    let current_period = env.ledger().timestamp() / 2_592_000;

    // First distribution succeeds
    let result = client.try_distribute_revenue(&admin, &current_period);
    assert!(result.is_ok());

    // Second distribution fails — pending is now 0
    let result = client.try_distribute_revenue(&admin, &current_period);
    assert!(result.is_err(), "Double distribution should fail");
}

// ============================================================================
// Revenue Split Safety – Accounting Invariant Tests
// ============================================================================

/// Helper: collect fees and distribute, then assert the sum invariant holds.
fn assert_distribution_sum_invariant(
    env: &Env,
    client: &QuickLendXContractClient,
    admin: &Address,
    treasury: &Address,
    treasury_bps: u32,
    developer_bps: u32,
    platform_bps: u32,
    fee_amount: i128,
) {
    let user = Address::generate(env);
    client.configure_revenue_distribution(
        admin,
        treasury,
        &treasury_bps,
        &developer_bps,
        &platform_bps,
        &false,
        &1,
    );
    let mut fees_by_type = Map::new(env);
    fees_by_type.set(FeeType::Platform, fee_amount);
    client.collect_transaction_fees(&user, &fees_by_type, &fee_amount);

    let current_period = env.ledger().timestamp() / 2_592_000;
    let (t, d, p) = client.distribute_revenue(admin, &current_period);

    assert!(t >= 0, "Treasury amount must be non-negative");
    assert!(d >= 0, "Developer amount must be non-negative");
    assert!(p >= 0, "Platform amount must be non-negative");
    assert_eq!(
        t + d + p,
        fee_amount,
        "Sum invariant violated: {} + {} + {} != {}",
        t,
        d,
        p,
        fee_amount
    );
}

#[test]
fn test_sum_invariant_even_split() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);
    let treasury = Address::generate(&env);
    client.initialize_fee_system(&admin);

    // 33.33% / 33.33% / 33.34% with amount that causes rounding
    assert_distribution_sum_invariant(&env, &client, &admin, &treasury, 3333, 3333, 3334, 999);
}

#[test]
fn test_sum_invariant_skewed_split() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);
    let treasury = Address::generate(&env);
    client.initialize_fee_system(&admin);

    // 90% / 7% / 3%
    assert_distribution_sum_invariant(&env, &client, &admin, &treasury, 9000, 700, 300, 10001);
}

#[test]
fn test_sum_invariant_with_one_unit() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);
    let treasury = Address::generate(&env);
    client.initialize_fee_system(&admin);

    // 1 unit with 33/33/34 split — only platform should get the 1 unit (remainder)
    assert_distribution_sum_invariant(&env, &client, &admin, &treasury, 3300, 3300, 3400, 1);
}

#[test]
fn test_sum_invariant_large_amount() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);
    let treasury = Address::generate(&env);
    client.initialize_fee_system(&admin);

    // Large amount: 1 trillion units
    assert_distribution_sum_invariant(
        &env,
        &client,
        &admin,
        &treasury,
        5000,
        3000,
        2000,
        1_000_000_000_000,
    );
}

#[test]
fn test_sum_invariant_prime_amount() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);
    let treasury = Address::generate(&env);
    client.initialize_fee_system(&admin);

    // Prime number amount to stress rounding: 7919
    assert_distribution_sum_invariant(&env, &client, &admin, &treasury, 4111, 2789, 3100, 7919);
}

// ============================================================================
// Revenue Split Safety – Invalid Configuration Rejection Tests
// ============================================================================

#[test]
fn test_individual_share_exceeds_10000_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);
    let treasury = Address::generate(&env);

    client.initialize_fee_system(&admin);

    // Treasury share exceeds 10_000 even if sum equals 10_000 (impossible, but test bounds)
    let result =
        client.try_configure_revenue_distribution(&admin, &treasury, &10001, &0, &0, &false, &100);
    assert!(
        result.is_err(),
        "Individual share > 10000 should be rejected"
    );
}

#[test]
fn test_negative_min_distribution_amount_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);
    let treasury = Address::generate(&env);

    client.initialize_fee_system(&admin);

    let result = client.try_configure_revenue_distribution(
        &admin,
        &treasury,
        &5000,
        &2500,
        &2500,
        &false,
        &-1, // negative
    );
    assert!(
        result.is_err(),
        "Negative min_distribution_amount should be rejected"
    );
}

#[test]
fn test_shares_sum_over_10000_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);
    let treasury = Address::generate(&env);

    client.initialize_fee_system(&admin);

    // Sum = 10001
    let result = client.try_configure_revenue_distribution(
        &admin, &treasury, &5000, &3000, &2001, &false, &100,
    );
    assert!(result.is_err(), "Shares summing to > 10000 should fail");
}

#[test]
fn test_shares_sum_under_10000_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);
    let treasury = Address::generate(&env);

    client.initialize_fee_system(&admin);

    // Sum = 9999
    let result = client.try_configure_revenue_distribution(
        &admin, &treasury, &5000, &3000, &1999, &false, &100,
    );
    assert!(result.is_err(), "Shares summing to < 10000 should fail");
}

#[test]
fn test_all_zero_shares_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);
    let treasury = Address::generate(&env);

    client.initialize_fee_system(&admin);

    let result =
        client.try_configure_revenue_distribution(&admin, &treasury, &0, &0, &0, &false, &100);
    assert!(result.is_err(), "All-zero shares should be rejected");
}

// ============================================================================
// Revenue Split Safety – Edge Case Distribution Tests
// ============================================================================

#[test]
fn test_100_percent_platform_split() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);
    let treasury = Address::generate(&env);
    let user = Address::generate(&env);

    client.initialize_fee_system(&admin);

    // 100% to platform
    client.configure_revenue_distribution(&admin, &treasury, &0, &0, &10000, &false, &1);

    let mut fees_by_type = Map::new(&env);
    fees_by_type.set(FeeType::Platform, 12345);
    client.collect_transaction_fees(&user, &fees_by_type, &12345);

    let current_period = env.ledger().timestamp() / 2_592_000;
    let (treasury_amount, developer_amount, platform_amount) =
        client.distribute_revenue(&admin, &current_period);

    assert_eq!(treasury_amount, 0);
    assert_eq!(developer_amount, 0);
    assert_eq!(platform_amount, 12345);
}

#[test]
fn test_minimum_distribution_threshold_enforced() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);
    let treasury = Address::generate(&env);
    let user = Address::generate(&env);

    client.initialize_fee_system(&admin);

    // Set min_distribution_amount = 1000
    client.configure_revenue_distribution(&admin, &treasury, &5000, &2500, &2500, &false, &1000);

    // Collect only 500 (below threshold)
    let mut fees_by_type = Map::new(&env);
    fees_by_type.set(FeeType::Platform, 500);
    client.collect_transaction_fees(&user, &fees_by_type, &500);

    let current_period = env.ledger().timestamp() / 2_592_000;
    let result = client.try_distribute_revenue(&admin, &current_period);
    assert!(
        result.is_err(),
        "Distribution below min threshold should fail"
    );

    // Collect more to exceed threshold
    let mut fees_by_type2 = Map::new(&env);
    fees_by_type2.set(FeeType::Platform, 600);
    client.collect_transaction_fees(&user, &fees_by_type2, &600);

    // Now should succeed (pending = 1100 >= 1000)
    let result = client.try_distribute_revenue(&admin, &current_period);
    assert!(
        result.is_ok(),
        "Distribution above min threshold should succeed"
    );
}

#[test]
fn test_validate_revenue_shares_directly() {
    // Test the validate_revenue_shares function directly via the contract
    // Valid cases
    assert!(crate::fees::FeeManager::validate_revenue_shares(10000, 0, 0).is_ok());
    assert!(crate::fees::FeeManager::validate_revenue_shares(0, 10000, 0).is_ok());
    assert!(crate::fees::FeeManager::validate_revenue_shares(0, 0, 10000).is_ok());
    assert!(crate::fees::FeeManager::validate_revenue_shares(3333, 3333, 3334).is_ok());
    assert!(crate::fees::FeeManager::validate_revenue_shares(5000, 3000, 2000).is_ok());

    // Invalid: sum != 10000
    assert!(crate::fees::FeeManager::validate_revenue_shares(5000, 3000, 1999).is_err());
    assert!(crate::fees::FeeManager::validate_revenue_shares(5000, 3000, 2001).is_err());
    assert!(crate::fees::FeeManager::validate_revenue_shares(0, 0, 0).is_err());

    // Invalid: individual share > 10000
    assert!(crate::fees::FeeManager::validate_revenue_shares(10001, 0, 0).is_err());
    assert!(crate::fees::FeeManager::validate_revenue_shares(0, 10001, 0).is_err());
    assert!(crate::fees::FeeManager::validate_revenue_shares(0, 0, 10001).is_err());
}

#[test]
fn test_distribution_non_negative_amounts() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);
    let treasury = Address::generate(&env);
    let user = Address::generate(&env);

    client.initialize_fee_system(&admin);

    // Edge case: very small amount with extreme split ratios
    client.configure_revenue_distribution(&admin, &treasury, &1, &1, &9998, &false, &1);

    let mut fees_by_type = Map::new(&env);
    fees_by_type.set(FeeType::Platform, 3);
    client.collect_transaction_fees(&user, &fees_by_type, &3);

    let current_period = env.ledger().timestamp() / 2_592_000;
    let (t, d, p) = client.distribute_revenue(&admin, &current_period);

    assert!(t >= 0, "Treasury must be >= 0");
    assert!(d >= 0, "Developer must be >= 0");
    assert!(p >= 0, "Platform must be >= 0");
    assert_eq!(t + d + p, 3, "Sum must equal original amount");
}

#[test]
fn test_zero_min_distribution_amount_allowed() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = setup_admin(&env, &client);
    let treasury = Address::generate(&env);

    client.initialize_fee_system(&admin);

    // min_distribution_amount = 0 should be valid
    let result = client.try_configure_revenue_distribution(
        &admin, &treasury, &5000, &2500, &2500, &false, &0,
    );
    assert!(
        result.is_ok(),
        "Zero min_distribution_amount should be allowed"
    );
}
