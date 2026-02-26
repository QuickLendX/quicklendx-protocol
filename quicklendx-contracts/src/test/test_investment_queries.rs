/// Comprehensive test suite for investment query functions
///
/// Coverage:
/// 1. get_invoice_investment - query by invoice ID
/// 2. get_investment - query by investment ID
/// 3. get_investments_by_investor - query all investments for an investor
/// 4. Empty queries do not panic
/// 5. Non-existent IDs return appropriate errors
/// 6. Multiple investments per investor
extern crate alloc;
use crate::errors::QuickLendXError;
use crate::investment::{Investment, InvestmentStatus, InvestmentStorage};
use soroban_sdk::{
    testutils::{Address as _},
    Address, BytesN, Env, Vec,
};

// ============================================================================
// Test Helpers
// ============================================================================

fn setup() -> (Env, crate::QuickLendXContractClient<'static>, Address) {
    let env = Env::default();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = crate::QuickLendXContractClient::new(&env, &contract_id);
    (env, client, contract_id)
}

fn create_test_investment(
    env: &Env,
    contract_id: &Address,
    investor: &Address,
    amount: i128,
    status: InvestmentStatus,
    seed: u8,
) -> (BytesN<32>, BytesN<32>) {
    env.as_contract(contract_id, || {
        let investment_id = InvestmentStorage::generate_unique_investment_id(env);
        let mut invoice_bytes = [seed; 32];
        invoice_bytes[0] = 0xFE;
        let invoice_id = BytesN::from_array(env, &invoice_bytes);
        
        let investment = Investment {
            investment_id: investment_id.clone(),
            invoice_id: invoice_id.clone(),
            investor: investor.clone(),
            amount,
            funded_at: env.ledger().timestamp(),
            status,
            insurance: Vec::new(env),
        };
        InvestmentStorage::store_investment(env, &investment);
        (investment_id, invoice_id)
    })
}

// ============================================================================
// Empty Query Tests
// ============================================================================

#[test]
fn test_empty_investment_queries_do_not_panic() {
    let (env, client, _) = setup();
    env.mock_all_auths();

    let investor = Address::generate(&env);
    let result = client.get_investments_by_investor(&investor);
    assert_eq!(result.len(), 0);
}

#[test]
fn test_get_investment_nonexistent_returns_error() {
    let (env, client, _) = setup();
    env.mock_all_auths();

    let nonexistent_id = BytesN::from_array(&env, &[0u8; 32]);
    let result = client.try_get_investment(&nonexistent_id);
    
    let err = result.err().expect("expected error for nonexistent investment");
    let contract_error = err.expect("expected contract error");
    assert_eq!(contract_error, QuickLendXError::StorageKeyNotFound);
}

#[test]
fn test_get_invoice_investment_nonexistent_returns_error() {
    let (env, client, _) = setup();
    env.mock_all_auths();

    let nonexistent_invoice_id = BytesN::from_array(&env, &[0u8; 32]);
    let result = client.try_get_invoice_investment(&nonexistent_invoice_id);
    
    let err = result.err().expect("expected error for nonexistent invoice");
    let contract_error = err.expect("expected contract error");
    assert_eq!(contract_error, QuickLendXError::StorageKeyNotFound);
}

// ============================================================================
// get_investment Tests
// ============================================================================

#[test]
fn test_get_investment_by_id_success() {
    let (env, client, contract_id) = setup();
    env.mock_all_auths();

    let investor = Address::generate(&env);
    let (investment_id, invoice_id) = create_test_investment(
        &env,
        &contract_id,
        &investor,
        5_000,
        InvestmentStatus::Active,
        1,
    );

    let result = client.get_investment(&investment_id);
    assert_eq!(result.investment_id, investment_id);
    assert_eq!(result.invoice_id, invoice_id);
    assert_eq!(result.investor, investor);
    assert_eq!(result.amount, 5_000);
    assert_eq!(result.status, InvestmentStatus::Active);
    assert_eq!(result.insurance.len(), 0);
}

#[test]
fn test_get_investment_multiple_statuses() {
    let (env, client, contract_id) = setup();
    env.mock_all_auths();

    let investor = Address::generate(&env);
    
    // Create investments with different statuses
    let (id1, _) = create_test_investment(
        &env,
        &contract_id,
        &investor,
        1_000,
        InvestmentStatus::Active,
        2,
    );
    let (id2, _) = create_test_investment(
        &env,
        &contract_id,
        &investor,
        2_000,
        InvestmentStatus::Completed,
        3,
    );
    let (id3, _) = create_test_investment(
        &env,
        &contract_id,
        &investor,
        3_000,
        InvestmentStatus::Withdrawn,
        4,
    );
    let (id4, _) = create_test_investment(
        &env,
        &contract_id,
        &investor,
        4_000,
        InvestmentStatus::Defaulted,
        5,
    );
    let (id5, _) = create_test_investment(
        &env,
        &contract_id,
        &investor,
        5_000,
        InvestmentStatus::Refunded,
        6,
    );

    let result1 = client.get_investment(&id1);
    assert_eq!(result1.status, InvestmentStatus::Active);
    assert_eq!(result1.amount, 1_000);

    let result2 = client.get_investment(&id2);
    assert_eq!(result2.status, InvestmentStatus::Completed);
    assert_eq!(result2.amount, 2_000);

    let result3 = client.get_investment(&id3);
    assert_eq!(result3.status, InvestmentStatus::Withdrawn);
    assert_eq!(result3.amount, 3_000);

    let result4 = client.get_investment(&id4);
    assert_eq!(result4.status, InvestmentStatus::Defaulted);
    assert_eq!(result4.amount, 4_000);

    let result5 = client.get_investment(&id5);
    assert_eq!(result5.status, InvestmentStatus::Refunded);
    assert_eq!(result5.amount, 5_000);
}

// ============================================================================
// get_invoice_investment Tests
// ============================================================================

#[test]
fn test_get_invoice_investment_success() {
    let (env, client, contract_id) = setup();
    env.mock_all_auths();

    let investor = Address::generate(&env);
    let (investment_id, invoice_id) = create_test_investment(
        &env,
        &contract_id,
        &investor,
        10_000,
        InvestmentStatus::Active,
        10,
    );

    let result = client.get_invoice_investment(&invoice_id);
    assert_eq!(result.investment_id, investment_id);
    assert_eq!(result.invoice_id, invoice_id);
    assert_eq!(result.investor, investor);
    assert_eq!(result.amount, 10_000);
}

#[test]
fn test_get_invoice_investment_unique_mapping() {
    let (env, client, contract_id) = setup();
    env.mock_all_auths();

    let investor1 = Address::generate(&env);
    let investor2 = Address::generate(&env);

    let (investment_id1, invoice_id1) = create_test_investment(
        &env,
        &contract_id,
        &investor1,
        7_500,
        InvestmentStatus::Active,
        20,
    );

    let (investment_id2, invoice_id2) = create_test_investment(
        &env,
        &contract_id,
        &investor2,
        12_000,
        InvestmentStatus::Completed,
        21,
    );

    let result1 = client.get_invoice_investment(&invoice_id1);
    assert_eq!(result1.investment_id, investment_id1);
    assert_eq!(result1.investor, investor1);

    let result2 = client.get_invoice_investment(&invoice_id2);
    assert_eq!(result2.investment_id, investment_id2);
    assert_eq!(result2.investor, investor2);
}

// ============================================================================
// get_investments_by_investor Tests
// ============================================================================

#[test]
fn test_get_investments_by_investor_single() {
    let (env, client, contract_id) = setup();
    env.mock_all_auths();

    let investor = Address::generate(&env);
    let (investment_id, _) = create_test_investment(
        &env,
        &contract_id,
        &investor,
        3_000,
        InvestmentStatus::Active,
        30,
    );

    let result = client.get_investments_by_investor(&investor);
    assert_eq!(result.len(), 1);
    assert_eq!(result.get(0).unwrap(), investment_id);
}

#[test]
fn test_get_investments_by_investor_multiple() {
    let (env, client, contract_id) = setup();
    env.mock_all_auths();

    let investor = Address::generate(&env);
    let mut expected_ids = Vec::new(&env);

    for i in 0..5 {
        let (investment_id, _) = create_test_investment(
            &env,
            &contract_id,
            &investor,
            1_000 * (i + 1),
            InvestmentStatus::Active,
            (40 + i) as u8,
        );
        expected_ids.push_back(investment_id);
    }

    let result = client.get_investments_by_investor(&investor);
    assert_eq!(result.len(), 5);

    for (idx, expected_id) in expected_ids.iter().enumerate() {
        assert_eq!(result.get(idx as u32).unwrap(), expected_id);
    }
}

#[test]
fn test_get_investments_by_investor_isolation() {
    let (env, client, contract_id) = setup();
    env.mock_all_auths();

    let investor1 = Address::generate(&env);
    let investor2 = Address::generate(&env);

    let (inv1_id, _) = create_test_investment(
        &env,
        &contract_id,
        &investor1,
        5_000,
        InvestmentStatus::Active,
        50,
    );

    let (inv2_id, _) = create_test_investment(
        &env,
        &contract_id,
        &investor2,
        8_000,
        InvestmentStatus::Completed,
        51,
    );

    let (inv3_id, _) = create_test_investment(
        &env,
        &contract_id,
        &investor1,
        3_000,
        InvestmentStatus::Withdrawn,
        52,
    );

    let result1 = client.get_investments_by_investor(&investor1);
    assert_eq!(result1.len(), 2);
    assert_eq!(result1.get(0).unwrap(), inv1_id);
    assert_eq!(result1.get(1).unwrap(), inv3_id);

    let result2 = client.get_investments_by_investor(&investor2);
    assert_eq!(result2.len(), 1);
    assert_eq!(result2.get(0).unwrap(), inv2_id);
}

#[test]
fn test_get_investments_by_investor_mixed_statuses() {
    let (env, client, contract_id) = setup();
    env.mock_all_auths();

    let investor = Address::generate(&env);
    
    let mut expected_ids = Vec::new(&env);
    
    let (id1, _) = create_test_investment(
        &env,
        &contract_id,
        &investor,
        2_000,
        InvestmentStatus::Active,
        60,
    );
    expected_ids.push_back(id1);
    
    let (id2, _) = create_test_investment(
        &env,
        &contract_id,
        &investor,
        2_000,
        InvestmentStatus::Completed,
        61,
    );
    expected_ids.push_back(id2);
    
    let (id3, _) = create_test_investment(
        &env,
        &contract_id,
        &investor,
        2_000,
        InvestmentStatus::Withdrawn,
        62,
    );
    expected_ids.push_back(id3);
    
    let (id4, _) = create_test_investment(
        &env,
        &contract_id,
        &investor,
        2_000,
        InvestmentStatus::Defaulted,
        63,
    );
    expected_ids.push_back(id4);

    let result = client.get_investments_by_investor(&investor);
    assert_eq!(result.len(), 4);

    for (idx, expected_id) in expected_ids.iter().enumerate() {
        assert_eq!(result.get(idx as u32).unwrap(), expected_id);
    }
}

// ============================================================================
// Integration Tests
// ============================================================================

#[test]
fn test_query_investment_with_insurance() {
    let (env, client, contract_id) = setup();
    env.mock_all_auths();

    let investor = Address::generate(&env);
    let provider = Address::generate(&env);
    
    let (investment_id, _) = create_test_investment(
        &env,
        &contract_id,
        &investor,
        10_000,
        InvestmentStatus::Active,
        70,
    );

    client.add_investment_insurance(&investment_id, &provider, &50u32);

    let result = client.get_investment(&investment_id);
    assert_eq!(result.insurance.len(), 1);
    
    let insurance = result.insurance.get(0).unwrap();
    assert_eq!(insurance.provider, provider);
    assert_eq!(insurance.coverage_percentage, 50);
    assert_eq!(insurance.coverage_amount, 5_000);
    assert!(insurance.active);
}

#[test]
fn test_complete_investment_query_workflow() {
    let (env, client, contract_id) = setup();
    env.mock_all_auths();

    let investor = Address::generate(&env);
    let (investment_id, invoice_id) = create_test_investment(
        &env,
        &contract_id,
        &investor,
        15_000,
        InvestmentStatus::Active,
        80,
    );

    // Query by investment ID
    let by_id = client.get_investment(&investment_id);
    assert_eq!(by_id.amount, 15_000);

    // Query by invoice ID
    let by_invoice = client.get_invoice_investment(&invoice_id);
    assert_eq!(by_invoice.investment_id, investment_id);

    // Query by investor
    let by_investor = client.get_investments_by_investor(&investor);
    assert_eq!(by_investor.len(), 1);
    assert_eq!(by_investor.get(0).unwrap(), investment_id);
}
