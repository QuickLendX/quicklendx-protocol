use crate::analytics::{AnalyticsCalculator, TimePeriod};
use soroban_sdk::{Env, Address};
use soroban_sdk::testutils::Address as _;

// ✅ FIX: return Address (NOT BytesN)
fn setup_contract(env: &Env) -> Address {
    env.register_contract(None, crate::QuickLendXContract)
}

#[test]
fn test_platform_metrics_empty_defaults() {
    let env = Env::default();
    let contract_id = setup_contract(&env);

    env.as_contract(&contract_id, || {
        let result = AnalyticsCalculator::calculate_platform_metrics(&env);
        assert!(result.is_ok());

        let m = result.unwrap();
        assert_eq!(m.total_invoices, 0);
        assert_eq!(m.total_investments, 0);
        assert_eq!(m.total_volume, 0);
    });
}

#[test]
fn test_financial_metrics_empty_defaults() {
    let env = Env::default();
    let contract_id = setup_contract(&env);

    env.as_contract(&contract_id, || {
        let result =
            AnalyticsCalculator::calculate_financial_metrics(&env, TimePeriod::AllTime);

        assert!(result.is_ok());

        let m = result.unwrap();
        assert_eq!(m.total_volume, 0);
    });
}

#[test]
fn test_user_behavior_empty_defaults() {
    let env = Env::default();
    let contract_id = setup_contract(&env);
    let user = Address::generate(&env);

    env.as_contract(&contract_id, || {
        let result =
            AnalyticsCalculator::calculate_user_behavior_metrics(&env, &user);

        assert!(result.is_ok());

        let m = result.unwrap();
        assert_eq!(m.total_invoices_uploaded, 0);
    });
}

#[test]
fn test_deterministic_empty_results() {
    let env = Env::default();
    let contract_id = setup_contract(&env);

    env.as_contract(&contract_id, || {
        let r1 = AnalyticsCalculator::calculate_platform_metrics(&env).unwrap();
        let r2 = AnalyticsCalculator::calculate_platform_metrics(&env).unwrap();

        assert_eq!(r1.total_invoices, r2.total_invoices);
        assert_eq!(r1.total_volume, r2.total_volume);
    });
}

#[test]
fn test_performance_metrics_empty_defaults() {
    let env = Env::default();
    let contract_id = setup_contract(&env);

    env.as_contract(&contract_id, || {
        // ✅ FIX: removed TimePeriod argument
        let result = AnalyticsCalculator::calculate_performance_metrics(&env);

        assert!(result.is_ok());
    });
}