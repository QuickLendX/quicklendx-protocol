#![cfg(test)]
extern crate std;

use crate::fees::FeeManager;
use crate::{QuickLendXContract, QuickLendXContractClient};
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

#[test]
fn test_calculate_profit_no_dust_rounding_boundary() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(QuickLendXContract, ());
    let admin = Address::generate(&env);
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let _ = client.initialize_admin(&admin);
    // 2% fee, profit=49 -> fee rounds down to 0.
    let (investor_return, platform_fee) = client.calculate_profit(&1000, &1049);
    assert_eq!(platform_fee, 0);
    assert_eq!(investor_return, 1049);
    assert_eq!(investor_return + platform_fee, 1049);
}

#[test]
fn test_calculate_profit_large_amount_no_overflow() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(QuickLendXContract, ());
    let admin = Address::generate(&env);
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let _ = client.initialize_admin(&admin);
    let investment_amount: i128 = 1_000_000_000_000_000_000_000_000_000_000_000_000;
    let payment_amount: i128 = 1_100_000_000_000_000_000_000_000_000_000_000_000;
    let (investor_return, platform_fee) =
        client.calculate_profit(&investment_amount, &payment_amount);

    assert_eq!(platform_fee, 2_000_000_000_000_000_000_000_000_000_000_000);
    assert_eq!(
        investor_return,
        1_098_000_000_000_000_000_000_000_000_000_000_000
    );
    assert_eq!(investor_return + platform_fee, payment_amount);
}

// ============================================================================
// PROFIT + FEE INTEGRATION TESTS
// ============================================================================

/// Fee config update is immediately reflected in the next calculate_profit call.
#[test]
fn test_fee_config_change_reflected_immediately() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let admin = Address::generate(&env);
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let _ = client.initialize_admin(&admin);

    // Default 2%: profit=1000 => fee=20
    let (_, fee_2pct) = client.calculate_profit(&0, &1000);
    assert_eq!(fee_2pct, 20);

    // Switch to 5%: profit=1000 => fee=50
    client.set_platform_fee(&500);
    let (_, fee_5pct) = client.calculate_profit(&0, &1000);
    assert_eq!(fee_5pct, 50);

    // Switch to 10%: profit=1000 => fee=100
    client.set_platform_fee(&1000);
    let (_, fee_10pct) = client.calculate_profit(&0, &1000);
    assert_eq!(fee_10pct, 100);
}

/// Invariant holds across a sequence of fee-rate changes.
#[test]
fn test_invariant_across_fee_rate_sequence() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let admin = Address::generate(&env);
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let _ = client.initialize_admin(&admin);

    let investment = 5_000i128;
    let payment = 6_000i128;

    for bps in [0u32, 50, 200, 500, 750, 1000] {
        client.set_platform_fee(&bps);
        let (investor_return, platform_fee) = client.calculate_profit(&investment, &payment);
        assert_eq!(
            investor_return + platform_fee,
            payment,
            "invariant broken at bps={bps}"
        );
        assert!(investor_return >= investment, "principal lost at bps={bps}");
        assert!(platform_fee >= 0, "negative fee at bps={bps}");
    }
}

/// Fee is only taken from profit, never from principal.
#[test]
fn test_fee_only_from_profit_not_principal() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let admin = Address::generate(&env);
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let _ = client.initialize_admin(&admin);

    client.set_platform_fee(&1000); // 10% - worst case
    let investment = 10_000i128;
    let payment = 10_500i128; // profit=500
    let gross_profit = payment - investment;

    let (investor_return, platform_fee) = client.calculate_profit(&investment, &payment);

    // Fee must not exceed gross profit
    assert!(platform_fee <= gross_profit);
    // Investor must recover at least their principal
    assert!(investor_return >= investment);
    assert_eq!(investor_return + platform_fee, payment);
}

/// Zero-profit settlement: investor recovers exactly the payment, fee is zero.
#[test]
fn test_zero_profit_investor_recovers_full_payment() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let admin = Address::generate(&env);
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let _ = client.initialize_admin(&admin);

    client.set_platform_fee(&1000);
    let amount = 7_777i128;
    let (investor_return, platform_fee) = client.calculate_profit(&amount, &amount);
    assert_eq!(platform_fee, 0);
    assert_eq!(investor_return, amount);
}

/// Loss settlement: investor receives payment_amount, fee is zero.
#[test]
fn test_loss_settlement_no_fee_investor_gets_payment() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let admin = Address::generate(&env);
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let _ = client.initialize_admin(&admin);

    client.set_platform_fee(&1000);
    let (investor_return, platform_fee) = client.calculate_profit(&10_000, &8_000);
    assert_eq!(platform_fee, 0);
    assert_eq!(investor_return, 8_000);
    assert_eq!(investor_return + platform_fee, 8_000);
}

/// Rounding boundary: profit=49 at 2% rounds to fee=0; profit=50 rounds to fee=1.
#[test]
fn test_rounding_boundary_2pct_profit_49_vs_50() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let admin = Address::generate(&env);
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let _ = client.initialize_admin(&admin);

    // Default 2%
    let (_, fee_49) = client.calculate_profit(&1000, &1049);
    assert_eq!(fee_49, 0);

    let (_, fee_50) = client.calculate_profit(&1000, &1050);
    assert_eq!(fee_50, 1);
}

/// Overflow safety: near-max i128 values satisfy the invariant without panic.
#[test]
fn test_overflow_safety_near_max_i128() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let admin = Address::generate(&env);
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let _ = client.initialize_admin(&admin);

    let investment: i128 = i128::MAX / 2;
    let payment: i128 = i128::MAX / 2 + 1_000_000;

    let (investor_return, platform_fee) = client.calculate_profit(&investment, &payment);
    assert_eq!(investor_return + platform_fee, payment);
    assert!(platform_fee >= 0);
    assert!(investor_return >= investment);
}
