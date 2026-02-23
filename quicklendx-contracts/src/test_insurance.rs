/// Comprehensive test suite for investment insurance
///
/// Coverage:
/// 1. Authorization - only investment owner can add insurance
/// 2. State validation - insurance only for active investments
/// 3. Multiple entries - historical entries persist, no cross-investment leakage
/// 4. Coverage/premium math - exact rounding and overflow boundaries
/// 5. Query correctness - insurance list and ordering
/// 6. Security edges - duplicates, invalid inputs, and non-mutation on failures
extern crate alloc;
use super::*;
use crate::errors::QuickLendXError;
use crate::investment::{
    Investment, InvestmentStatus, InvestmentStorage, DEFAULT_INSURANCE_PREMIUM_BPS,
};
use soroban_sdk::{
    testutils::{Address as _, MockAuth, MockAuthInvoke},
    Address, BytesN, Env, IntoVal, Vec,
};

// ============================================================================
// Helpers
// ============================================================================

fn setup() -> (Env, QuickLendXContractClient<'static>, Address) {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    (env, client, contract_id)
}
fn invoice_id_from_seed(env: &Env, seed: u8) -> BytesN<32> {
    let mut bytes = [seed; 32];
    bytes[0] = 0xAB;
    BytesN::from_array(env, &bytes)
}
fn store_investment(
    env: &Env,
    contract_id: &Address,
    investor: &Address,
    amount: i128,
    status: InvestmentStatus,
    seed: u8,
) -> BytesN<32> {
    // run storage operations within contract context
    env.as_contract(contract_id, || {
        let investment_id = InvestmentStorage::generate_unique_investment_id(env);
        let investment = Investment {
            investment_id: investment_id.clone(),
            invoice_id: invoice_id_from_seed(env, seed),
            investor: investor.clone(),
            amount,
            funded_at: env.ledger().timestamp(),
            status,
            insurance: Vec::new(env),
        };
        InvestmentStorage::store_investment(env, &investment);
        investment_id
    })
}

fn set_insurance_inactive(env: &Env, contract_id: &Address, investment_id: &BytesN<32>, idx: u32) {
    env.as_contract(contract_id, || {
        let mut investment =
            InvestmentStorage::get_investment(env, investment_id).expect("investment must exist");
        let mut coverage = investment
            .insurance
            .get(idx)
            .expect("insurance entry must exist");
        coverage.active = false;
        investment.insurance.set(idx, coverage);
        InvestmentStorage::update_investment(env, &investment);
    });
}

// ============================================================================
// Authorization Tests
// ============================================================================

#[test]
fn test_add_insurance_requires_investor_auth() {
    let (env, client, contract_id) = setup();
    let investor = Address::generate(&env);
    let attacker = Address::generate(&env);
    let provider = Address::generate(&env);

    let investment_id = store_investment(&env, &contract_id, &investor, 10_000, InvestmentStatus::Active, 1);

    let auth = MockAuth {
        address: &attacker,
        invoke: &MockAuthInvoke {
            contract: &contract_id,
            fn_name: "add_investment_insurance",
            args: (investment_id.clone(), provider.clone(), 60u32).into_val(&env),
            sub_invokes: &[],
        },
    };

    let result =
        client
            .mock_auths(&[auth])
            .try_add_investment_insurance(&investment_id, &provider, &60u32);

    let err = result.err().expect("expected auth error");
    let invoke_err = err.err().expect("expected invoke error");
    assert_eq!(invoke_err, soroban_sdk::InvokeError::Abort);

    let stored = client.get_investment(&investment_id);
    assert_eq!(stored.insurance.len(), 0);
    let err_debug = alloc::format!("{:?}", invoke_err);
    assert!(!err_debug.contains("ed25519"));
}

// ============================================================================
// State Validation Tests
// ============================================================================

#[test]
fn test_add_insurance_requires_active_investment() {
    let (env, client, contract_id) = setup();
    env.mock_all_auths();

    let investor = Address::generate(&env);
    let provider = Address::generate(&env);

    let statuses = [
        InvestmentStatus::Withdrawn,
        InvestmentStatus::Completed,
        InvestmentStatus::Defaulted,
    ];

    for (idx, status) in statuses.iter().enumerate() {
        let investment_id =
            store_investment(&env, &contract_id, &investor, 5_000, status.clone(), (idx + 2) as u8);

        let result = client.try_add_investment_insurance(&investment_id, &provider, &50u32);
        let err = result.err().expect("expected invalid status error");
        let contract_error = err.expect("expected contract error");
        assert_eq!(contract_error, QuickLendXError::InvalidStatus);

        let stored = client.get_investment(&investment_id);
        assert_eq!(stored.insurance.len(), 0);
    }
}

#[test]
fn test_add_insurance_storage_key_not_found() {
    let (env, client, contract_id) = setup();
    env.mock_all_auths();

    let provider = Address::generate(&env);
    let missing_id = BytesN::from_array(&env, &[0u8; 32]);

    let result = client.try_add_investment_insurance(&missing_id, &provider, &45u32);
    let err = result.err().expect("expected storage error");
    let contract_error = err.expect("expected contract error");
    assert_eq!(contract_error, QuickLendXError::StorageKeyNotFound);
}

#[test]
fn test_state_transition_before_add_rejected() {
    let (env, client, contract_id) = setup();
    env.mock_all_auths();

    let investor = Address::generate(&env);
    let provider = Address::generate(&env);

    let investment_id = store_investment(&env, &contract_id, &investor, 7_500, InvestmentStatus::Active, 9);

    env.as_contract(&contract_id, || {
        let mut investment = InvestmentStorage::get_investment(&env, &investment_id).unwrap();
        investment.status = InvestmentStatus::Completed;
        InvestmentStorage::update_investment(&env, &investment);
    });

    let result = client.try_add_investment_insurance(&investment_id, &provider, &35u32);
    let err = result.err().expect("expected invalid status error");
    let contract_error = err.expect("expected contract error");
    assert_eq!(contract_error, QuickLendXError::InvalidStatus);

    let stored = client.get_investment(&investment_id);
    assert_eq!(stored.insurance.len(), 0);
}

// ============================================================================
// Coverage / Premium Math Tests
// ============================================================================

#[test]
fn test_premium_and_coverage_math_exact() {
    let (env, client, contract_id) = setup();
    env.mock_all_auths();

    let investor = Address::generate(&env);
    let provider = Address::generate(&env);

    let investment_id = store_investment(&env, &contract_id, &investor, 10_000, InvestmentStatus::Active, 4);

    client.add_investment_insurance(&investment_id, &provider, &80u32);

    let stored = client.get_investment(&investment_id);
    let insurance = stored.insurance.get(0).unwrap();
    assert_eq!(insurance.coverage_amount, 8_000);
    assert_eq!(insurance.premium_amount, 160);
    assert_eq!(
        insurance.premium_amount,
        Investment::calculate_premium(10_000, 80)
    );

    let investment_id_small = store_investment(&env, &contract_id, &investor, 500, InvestmentStatus::Active, 5);
    client.add_investment_insurance(&investment_id_small, &provider, &1u32);

    let stored_small = client.get_investment(&investment_id_small);
    let insurance_small = stored_small.insurance.get(0).unwrap();
    assert_eq!(insurance_small.coverage_amount, 5);
    assert_eq!(insurance_small.premium_amount, 1);
}

#[test]
fn test_zero_coverage_and_invalid_inputs() {
    let (env, client, contract_id) = setup();
    env.mock_all_auths();

    let investor = Address::generate(&env);
    let provider = Address::generate(&env);

    let investment_id = store_investment(&env, &contract_id, &investor, 1_000, InvestmentStatus::Active, 6);

    let result = client.try_add_investment_insurance(&investment_id, &provider, &0u32);
    let err = result.err().expect("expected invalid amount error");
    let contract_error = err.expect("expected contract error");
    assert_eq!(contract_error, QuickLendXError::InvalidAmount);

    let result = client.try_add_investment_insurance(&investment_id, &provider, &150u32);
    let err = result.err().expect("expected invalid coverage error");
    let contract_error = err.expect("expected contract error");
    assert_eq!(contract_error, QuickLendXError::InvalidCoveragePercentage);

    let small_amount_id = store_investment(&env, &contract_id, &investor, 50, InvestmentStatus::Active, 7);
    let result = client.try_add_investment_insurance(&small_amount_id, &provider, &1u32);
    let err = result.err().expect("expected invalid amount error");
    let contract_error = err.expect("expected contract error");
    assert_eq!(contract_error, QuickLendXError::InvalidAmount);

    let negative_amount_id = store_investment(&env, &contract_id, &investor, -10, InvestmentStatus::Active, 8);
    let result = client.try_add_investment_insurance(&negative_amount_id, &provider, &10u32);
    let err = result.err().expect("expected invalid amount error");
    let contract_error = err.expect("expected contract error");
    assert_eq!(contract_error, QuickLendXError::InvalidAmount);
}

#[test]
fn test_large_values_handle_saturation() {
    let (env, client, contract_id) = setup();
    env.mock_all_auths();

    let investor = Address::generate(&env);
    let provider = Address::generate(&env);

    let amount = i128::MAX;
    let investment_id = store_investment(&env, &contract_id, &investor, amount, InvestmentStatus::Active, 10);

    client.add_investment_insurance(&investment_id, &provider, &100u32);

    let stored = client.get_investment(&investment_id);
    let insurance = stored.insurance.get(0).unwrap();

    let expected_coverage = amount.saturating_mul(100).checked_div(100).unwrap_or(0);
    let expected_premium = expected_coverage
        .saturating_mul(DEFAULT_INSURANCE_PREMIUM_BPS)
        .checked_div(10_000)
        .unwrap_or(0);

    assert_eq!(insurance.coverage_amount, expected_coverage);
    assert_eq!(insurance.premium_amount, expected_premium);
}

// ============================================================================
// Multiple Entries + Query Correctness
// ============================================================================

#[test]
fn test_multiple_entries_and_no_cross_investment_leakage() {
    let (env, client, contract_id) = setup();
    env.mock_all_auths();

    let investor = Address::generate(&env);
    let provider_one = Address::generate(&env);
    let provider_two = Address::generate(&env);
    let provider_three = Address::generate(&env);

    let investment_a = store_investment(&env, &contract_id, &investor, 12_000, InvestmentStatus::Active, 11);
    let investment_b = store_investment(&env, &contract_id, &investor, 8_000, InvestmentStatus::Active, 12);

    client.add_investment_insurance(&investment_a, &provider_one, &60u32);

    set_insurance_inactive(&env, &contract_id, &investment_a, 0);
    client.add_investment_insurance(&investment_a, &provider_two, &40u32);

    let stored_a = client.get_investment(&investment_a);
    assert_eq!(stored_a.insurance.len(), 2);
    let first = stored_a.insurance.get(0).unwrap();
    let second = stored_a.insurance.get(1).unwrap();
    assert_eq!(first.provider, provider_one);
    assert!(!first.active);
    assert_eq!(second.provider, provider_two);
    assert!(second.active);

    let stored_b = client.get_investment(&investment_b);
    assert_eq!(stored_b.insurance.len(), 0);

    client.add_investment_insurance(&investment_b, &provider_three, &50u32);

    let stored_a_after = client.get_investment(&investment_a);
    let stored_b_after = client.get_investment(&investment_b);

    assert_eq!(stored_a_after.insurance.len(), 2);
    assert_eq!(stored_b_after.insurance.len(), 1);
    assert_eq!(
        stored_b_after.insurance.get(0).unwrap().provider,
        provider_three
    );
}

// ============================================================================
// query_investment_insurance Tests
// ============================================================================

#[test]
fn test_query_investment_insurance_empty() {
    let (env, client, contract_id) = setup();
    env.mock_all_auths();

    let investor = Address::generate(&env);
    let investment_id = store_investment(&env, &contract_id, &investor, 5_000, InvestmentStatus::Active, 20);

    let result = client.query_investment_insurance(&investment_id);
    assert_eq!(result.len(), 0);
}

#[test]
fn test_query_investment_insurance_single_active() {
    let (env, client, contract_id) = setup();
    env.mock_all_auths();

    let investor = Address::generate(&env);
    let provider = Address::generate(&env);
    let investment_id = store_investment(&env, &contract_id, &investor, 10_000, InvestmentStatus::Active, 21);

    client.add_investment_insurance(&investment_id, &provider, &60u32);

    let result = client.query_investment_insurance(&investment_id);
    assert_eq!(result.len(), 1);
    
    let coverage = result.get(0).unwrap();
    assert_eq!(coverage.provider, provider);
    assert_eq!(coverage.coverage_percentage, 60);
    assert_eq!(coverage.coverage_amount, 6_000);
    assert_eq!(coverage.premium_amount, 120);
    assert!(coverage.active);
}

#[test]
fn test_query_investment_insurance_multiple_entries() {
    let (env, client, contract_id) = setup();
    env.mock_all_auths();

    let investor = Address::generate(&env);
    let provider1 = Address::generate(&env);
    let provider2 = Address::generate(&env);
    let investment_id = store_investment(&env, &contract_id, &investor, 20_000, InvestmentStatus::Active, 22);

    client.add_investment_insurance(&investment_id, &provider1, &50u32);
    set_insurance_inactive(&env, &contract_id, &investment_id, 0);
    client.add_investment_insurance(&investment_id, &provider2, &75u32);

    let result = client.query_investment_insurance(&investment_id);
    assert_eq!(result.len(), 2);

    let first = result.get(0).unwrap();
    assert_eq!(first.provider, provider1);
    assert!(!first.active);
    assert_eq!(first.coverage_percentage, 50);

    let second = result.get(1).unwrap();
    assert_eq!(second.provider, provider2);
    assert!(second.active);
    assert_eq!(second.coverage_percentage, 75);
}

#[test]
fn test_query_investment_insurance_nonexistent_investment() {
    let (env, client, _) = setup();
    env.mock_all_auths();

    let nonexistent_id = BytesN::from_array(&env, &[0u8; 32]);
    let result = client.try_query_investment_insurance(&nonexistent_id);
    
    let err = result.err().expect("expected error for nonexistent investment");
    let contract_error = err.expect("expected contract error");
    assert_eq!(contract_error, QuickLendXError::StorageKeyNotFound);
}

#[test]
fn test_query_investment_insurance_no_auth_required() {
    let (env, client, contract_id) = setup();
    env.mock_all_auths();
    
    let investor = Address::generate(&env);
    let provider = Address::generate(&env);
    
    let investment_id = store_investment(&env, &contract_id, &investor, 8_000, InvestmentStatus::Active, 23);
    client.add_investment_insurance(&investment_id, &provider, &40u32);

    // Query without any special auth setup should work (queries are public)
    let result = client.query_investment_insurance(&investment_id);
    assert_eq!(result.len(), 1);
    assert_eq!(result.get(0).unwrap().provider, provider);
    assert_eq!(result.get(0).unwrap().coverage_percentage, 40);
}

#[test]
fn test_query_investment_insurance_historical_tracking() {
    let (env, client, contract_id) = setup();
    env.mock_all_auths();

    let investor = Address::generate(&env);
    let provider1 = Address::generate(&env);
    let provider2 = Address::generate(&env);
    let provider3 = Address::generate(&env);
    
    let investment_id = store_investment(&env, &contract_id, &investor, 15_000, InvestmentStatus::Active, 24);

    // Add first insurance
    client.add_investment_insurance(&investment_id, &provider1, &30u32);
    let after_first = client.query_investment_insurance(&investment_id);
    assert_eq!(after_first.len(), 1);

    // Deactivate and add second
    set_insurance_inactive(&env, &contract_id, &investment_id, 0);
    client.add_investment_insurance(&investment_id, &provider2, &50u32);
    let after_second = client.query_investment_insurance(&investment_id);
    assert_eq!(after_second.len(), 2);

    // Deactivate and add third
    set_insurance_inactive(&env, &contract_id, &investment_id, 1);
    client.add_investment_insurance(&investment_id, &provider3, &70u32);
    let after_third = client.query_investment_insurance(&investment_id);
    assert_eq!(after_third.len(), 3);

    // Verify all historical entries are preserved
    assert_eq!(after_third.get(0).unwrap().provider, provider1);
    assert!(!after_third.get(0).unwrap().active);
    assert_eq!(after_third.get(1).unwrap().provider, provider2);
    assert!(!after_third.get(1).unwrap().active);
    assert_eq!(after_third.get(2).unwrap().provider, provider3);
    assert!(after_third.get(2).unwrap().active);
}

// ============================================================================
// Security / Edge Scenarios
// ============================================================================

#[test]
fn test_duplicate_submission_rejected_and_state_unchanged() {
    let (env, client, contract_id) = setup();
    env.mock_all_auths();

    let investor = Address::generate(&env);
    let provider = Address::generate(&env);
    let provider_two = Address::generate(&env);

    let investment_id = store_investment(&env, &contract_id, &investor, 9_000, InvestmentStatus::Active, 13);
    client.add_investment_insurance(&investment_id, &provider, &70u32);

    let before = client.get_investment(&investment_id);
    assert_eq!(before.insurance.len(), 1);

    let result = client.try_add_investment_insurance(&investment_id, &provider_two, &30u32);
    let err = result.err().expect("expected duplicate rejection");
    let contract_error = err.expect("expected contract error");
    assert_eq!(contract_error, QuickLendXError::OperationNotAllowed);

    let after = client.get_investment(&investment_id);
    assert_eq!(after.insurance.len(), 1);
    assert_eq!(after.insurance.get(0).unwrap().provider, provider);
}

#[test]
fn test_investment_helpers_cover_branches() {
    let env = Env::default();
    let investor = Address::generate(&env);
    let provider = Address::generate(&env);

    let mut investment = Investment {
        investment_id: BytesN::from_array(&env, &[1u8; 32]),
        invoice_id: BytesN::from_array(&env, &[2u8; 32]),
        investor: investor.clone(),
        amount: 1_000,
        funded_at: env.ledger().timestamp(),
        status: InvestmentStatus::Active,
        insurance: Vec::new(&env),
    };

    assert_eq!(Investment::calculate_premium(0, 50), 0);
    assert_eq!(Investment::calculate_premium(1_000, 0), 0);

    let premium = Investment::calculate_premium(1_000, 50);
    let coverage_amount = investment
        .add_insurance(provider.clone(), 50, premium)
        .expect("insurance should be added");
    assert_eq!(coverage_amount, 500);
    assert!(investment.has_active_insurance());

    let duplicate = investment.add_insurance(provider.clone(), 40, premium);
    assert_eq!(duplicate, Err(QuickLendXError::OperationNotAllowed));

    let mut empty_investment = investment.clone();
    empty_investment.insurance = Vec::new(&env);
    let invalid = empty_investment.add_insurance(provider.clone(), 150, premium);
    assert_eq!(invalid, Err(QuickLendXError::InvalidCoveragePercentage));

    let invalid_premium = empty_investment.add_insurance(provider.clone(), 50, 0);
    assert_eq!(invalid_premium, Err(QuickLendXError::InvalidAmount));

    let claim = investment
        .process_insurance_claim()
        .expect("claim should succeed");
    assert_eq!(claim.0, provider);
    assert_eq!(claim.1, 500);
    assert!(!investment.has_active_insurance());

    let no_claim = investment.process_insurance_claim();
    assert!(no_claim.is_none());
}
