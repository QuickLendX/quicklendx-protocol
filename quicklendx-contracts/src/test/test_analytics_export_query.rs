use crate::analytics::{AnalyticsCalculator, AnalyticsStorage, ANALYTICS_SCHEMA_VERSION};
use crate::invoice::InvoiceCategory;
use crate::{QuickLendXContract, QuickLendXContractClient};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Env, String, Vec,
};

fn setup_test(env: &Env) -> (QuickLendXContractClient<'_>, Address, Address, Address) {
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let business = Address::generate(env);
    env.mock_all_auths();
    client.set_admin(&admin);
    (client, contract_id, admin, business)
}

#[test]
fn test_export_analytics_snapshot_reflects_uploaded_invoices() {
    let env = Env::default();
    let ts = 1_500_000u64;
    env.ledger().set_timestamp(ts);
    let (client, contract_id, _admin, business) = setup_test(&env);

    let currency = Address::generate(&env);
    client.store_invoice(
        &business,
        &4_000,
        &currency,
        &(ts + 86_400),
        &String::from_str(&env, "export-inv"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
        &None,
    );

    let snapshot = env
        .as_contract(&contract_id, || {
            AnalyticsCalculator::export_analytics_snapshot(&env)
        })
        .unwrap();

    assert_eq!(snapshot.schema_version, ANALYTICS_SCHEMA_VERSION);
    assert_eq!(snapshot.ledger_timestamp, ts);
    assert_eq!(snapshot.platform_metrics.total_invoices, 1);
    assert_eq!(snapshot.platform_metrics.total_volume, 4_000);
}

#[test]
fn test_analytics_storage_platform_metrics_roundtrip() {
    let env = Env::default();
    env.ledger().set_timestamp(2_000_000);
    let (_client, contract_id, _admin, _business) = setup_test(&env);

    env.as_contract(&contract_id, || {
        assert!(AnalyticsStorage::get_platform_metrics(&env).is_none());

        let metrics = AnalyticsCalculator::calculate_platform_metrics(&env).unwrap();
        AnalyticsStorage::store_platform_metrics(&env, &metrics);

        let loaded = AnalyticsStorage::get_platform_metrics(&env).unwrap();
        assert_eq!(loaded, metrics);
    });
}

#[test]
fn test_analytics_storage_performance_metrics_roundtrip() {
    let env = Env::default();
    env.ledger().set_timestamp(2_500_000);
    let (_client, contract_id, _admin, _business) = setup_test(&env);

    env.as_contract(&contract_id, || {
        assert!(AnalyticsStorage::get_performance_metrics(&env).is_none());

        let metrics = AnalyticsCalculator::calculate_performance_metrics(&env).unwrap();
        AnalyticsStorage::store_performance_metrics(&env, &metrics);

        let loaded = AnalyticsStorage::get_performance_metrics(&env).unwrap();
        assert_eq!(loaded, metrics);
    });
}

#[test]
fn test_analytics_storage_user_behavior_roundtrip() {
    let env = Env::default();
    env.ledger().set_timestamp(3_000_000);
    let (_client, contract_id, _admin, business) = setup_test(&env);

    env.as_contract(&contract_id, || {
        assert!(AnalyticsStorage::get_user_behavior(&env, &business).is_none());

        let behavior =
            AnalyticsCalculator::calculate_user_behavior_metrics(&env, &business).unwrap();
        AnalyticsStorage::store_user_behavior(&env, &business, &behavior);

        let loaded = AnalyticsStorage::get_user_behavior(&env, &business).unwrap();
        assert_eq!(loaded.user_address, business);
        assert_eq!(loaded.total_invoices_uploaded, behavior.total_invoices_uploaded);
    });
}
