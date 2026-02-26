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
