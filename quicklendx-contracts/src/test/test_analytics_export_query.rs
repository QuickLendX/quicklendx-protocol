use super::*;
use soroban_sdk::{
    testutils::{Address as _, Events, MockAuth, MockAuthInvoke},
    Address, Env, String, Vec, IntoVal,
};

fn setup_test(env: &Env) -> (QuickLendXContractClient, Address, Address) {
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    
    // Initializing admin requires admin's auth
    env.mock_auths(&[
        MockAuth {
            address: &admin,
            invoke: &MockAuthInvoke {
                contract: &contract_id,
                fn_name: "set_admin",
                args: (admin.clone(),).into_val(env),
                sub_invokes: &[],
            },
        }
    ]);
    client.set_admin(&admin);
    
    (client, admin, contract_id)
}

#[test]
fn test_export_analytics_data_success() {
    let env = Env::default();
    let (client, admin, contract_id) = setup_test(&env);
    
    let export_type = String::from_str(&env, "CSV");
    let mut filters: Vec<String> = Vec::new(&env);
    filters.push_back(String::from_str(&env, "active_only"));
    
    // Authorize export_analytics_data
    env.mock_auths(&[
        MockAuth {
            address: &admin,
            invoke: &MockAuthInvoke {
                contract: &contract_id,
                fn_name: "export_analytics_data",
                args: (export_type.clone(), filters.clone()).into_val(&env),
                sub_invokes: &[],
            },
        }
    ]);
    
    let result = client.export_analytics_data(&export_type, &filters);
    
    assert_eq!(result, String::from_str(&env, "Analytics data exported"));
    
    // Check event emission
    let events = env.events().all();
    assert!(events.events().len() > 0, "Expected at least one event");
}

#[test]
fn test_export_analytics_data_fails_non_admin() {
    let env = Env::default();
    let (client, _admin, _contract_id) = setup_test(&env);
    
    let export_type = String::from_str(&env, "CSV");
    let filters: Vec<String> = Vec::new(&env);
    
    // Call without any auth setup for admin. 
    // It should fail because it calls admin.require_auth() but admin didn't sign.
    let result = client.try_export_analytics_data(&export_type, &filters);
    
    assert!(result.is_err());
}

#[test]
fn test_query_analytics_data_success() {
    let env = Env::default();
    let (client, _admin, _contract_id) = setup_test(&env);
    
    let query_type = String::from_str(&env, "performance");
    let mut filters: Vec<String> = Vec::new(&env);
    filters.push_back(String::from_str(&env, "period:daily"));
    let limit = 10;
    
    let result = client.query_analytics_data(&query_type, &filters, &limit);
    
    assert_eq!(result.len(), 1);
    assert_eq!(result.get(0).unwrap(), String::from_str(&env, "Analytics query completed"));
}

#[test]
fn test_query_analytics_data_limit_capping() {
    let env = Env::default();
    let (client, _admin, _contract_id) = setup_test(&env);
    
    let query_type = String::from_str(&env, "volume");
    let filters: Vec<String> = Vec::new(&env);
    let large_limit = 1000;
    
    let result = client.query_analytics_data(&query_type, &filters, &large_limit);
    assert_eq!(result.len(), 1);
}

#[test]
fn test_export_analytics_data_empty_filters() {
    let env = Env::default();
    let (client, admin, contract_id) = setup_test(&env);
    
    let export_type = String::from_str(&env, "JSON");
    let filters: Vec<String> = Vec::new(&env);
    
    env.mock_auths(&[
        MockAuth {
            address: &admin,
            invoke: &MockAuthInvoke {
                contract: &contract_id,
                fn_name: "export_analytics_data",
                args: (export_type.clone(), filters.clone()).into_val(&env),
                sub_invokes: &[],
            },
        }
    ]);
    
    let result = client.export_analytics_data(&export_type, &filters);
    assert_eq!(result, String::from_str(&env, "Analytics data exported"));
}
