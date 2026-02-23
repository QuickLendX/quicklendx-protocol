#![cfg(test)]
extern crate std;

use crate::fees::FeeManager;
use crate::QuickLendXContract;
use soroban_sdk::{testutils::Address as _, Address, Env};

#[test]
fn test_calculate_platform_fee_full_payment() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(QuickLendXContract, ());
    let admin = Address::generate(&env);

    env.as_contract(&contract_id, || {
        FeeManager::initialize(&env, &admin).unwrap();

        // Default fee is 2% (200 BPS)
        // Investment: 1000, Payment: 1100 => Profit: 100
        // Fee: 100 * 2% = 2
        // Investor gets: 1100 - 2 = 1098

        let investment_amount = 1000i128;
        let payment_amount = 1100i128;

        let (investor_return, platform_fee) =
            FeeManager::calculate_platform_fee(&env, investment_amount, payment_amount).unwrap();

        assert_eq!(platform_fee, 2);
        assert_eq!(investor_return, 1098);
    });
}

#[test]
fn test_calculate_platform_fee_no_profit() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(QuickLendXContract, ());
    let admin = Address::generate(&env);

    env.as_contract(&contract_id, || {
        FeeManager::initialize(&env, &admin).unwrap();

        // Investment: 1000, Payment: 1000 => Profit: 0
        // Fee: 0
        // Investor gets: 1000

        let investment_amount = 1000i128;
        let payment_amount = 1000i128;

        let (investor_return, platform_fee) =
            FeeManager::calculate_platform_fee(&env, investment_amount, payment_amount).unwrap();

        assert_eq!(platform_fee, 0);
        assert_eq!(investor_return, 1000);
    });
}

#[test]
fn test_calculate_platform_fee_partial_loss() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(QuickLendXContract, ());
    let admin = Address::generate(&env);

    env.as_contract(&contract_id, || {
        FeeManager::initialize(&env, &admin).unwrap();

        // Investment: 1000, Payment: 800 => Profit: -200 (0)
        // Fee: 0
        // Investor gets: 800

        let investment_amount = 1000i128;
        let payment_amount = 800i128;

        let (investor_return, platform_fee) =
            FeeManager::calculate_platform_fee(&env, investment_amount, payment_amount).unwrap();

        assert_eq!(platform_fee, 0);
        assert_eq!(investor_return, 800);
    });
}

#[test]
fn test_calculate_platform_fee_rounding() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(QuickLendXContract, ());
    let admin = Address::generate(&env);

    env.as_contract(&contract_id, || {
        FeeManager::initialize(&env, &admin).unwrap();

        // Investment: 1000, Payment: 1001 => Profit: 1
        // Fee: 1 * 200 / 10000 = 0.02 => 0 (integer division)
        // Investor gets: 1001

        let investment_amount = 1000i128;
        let payment_amount = 1001i128;

        let (investor_return, platform_fee) =
            FeeManager::calculate_platform_fee(&env, investment_amount, payment_amount).unwrap();

        assert_eq!(platform_fee, 0);
        assert_eq!(investor_return, 1001);
    });
}

#[test]
fn test_calculate_platform_fee_small_fee() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(QuickLendXContract, ());
    let admin = Address::generate(&env);

    env.as_contract(&contract_id, || {
        FeeManager::initialize(&env, &admin).unwrap();

        // Investment: 1000, Payment: 1050 => Profit: 50
        // Fee: 50 * 200 / 10000 = 10000 / 10000 = 1
        // Investor gets: 1049

        let investment_amount = 1000i128;
        let payment_amount = 1050i128;

        let (investor_return, platform_fee) =
            FeeManager::calculate_platform_fee(&env, investment_amount, payment_amount).unwrap();

        assert_eq!(platform_fee, 1);
        assert_eq!(investor_return, 1049);
    });
}

#[test]
fn test_calculate_platform_fee_updated_bps() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(QuickLendXContract, ());
    let admin = Address::generate(&env);

    env.as_contract(&contract_id, || {
        FeeManager::initialize(&env, &admin).unwrap();
    });

    env.as_contract(&contract_id, || {
        // Update fee to 10% (1000 BPS)
        FeeManager::update_platform_fee(&env, &admin, 1000).unwrap();
    });

    env.as_contract(&contract_id, || {
        // Investment: 1000, Payment: 1100 => Profit: 100
        // Fee: 100 * 10% = 10
        // Investor gets: 1090

        let investment_amount = 1000i128;
        let payment_amount = 1100i128;

        let (investor_return, platform_fee) =
            FeeManager::calculate_platform_fee(&env, investment_amount, payment_amount).unwrap();

        assert_eq!(platform_fee, 10);
        assert_eq!(investor_return, 1090);
    });
}

#[test]
fn test_calculate_platform_fee_large_numbers() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(QuickLendXContract, ());
    let admin = Address::generate(&env);

    env.as_contract(&contract_id, || {
        FeeManager::initialize(&env, &admin).unwrap();

        // Investment: 1M, Payment: 2M => Profit: 1M
        // Fee: 1M * 2% = 20,000

        let investment_amount = 1_000_000i128;
        let payment_amount = 2_000_000i128;

        let (investor_return, platform_fee) =
            FeeManager::calculate_platform_fee(&env, investment_amount, payment_amount).unwrap();

        assert_eq!(platform_fee, 20_000);
        assert_eq!(investor_return, 1_980_000);
    });
}
