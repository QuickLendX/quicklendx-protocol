/// Comprehensive test suite for investment insurance
///
/// Coverage:
/// 1. Authorization - only investment owner can add insurance
/// 2. State validation - insurance only for active investments
/// 3. Multiple entries - historical entries persist, no cross-investment leakage
/// 4. Coverage/premium math - exact rounding and overflow boundaries
/// 5. Query correctness - insurance list and ordering
/// 6. Security edges - duplicates, invalid inputs, and non-mutation on failures
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
    investor: &Address,
    amount: i128,
    status: InvestmentStatus,
    seed: u8,
) -> BytesN<32> {
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
}

fn set_insurance_inactive(env: &Env, investment_id: &BytesN<32>, idx: u32) {
    let mut investment =
        InvestmentStorage::get_investment(env, investment_id).expect("investment must exist");
    let mut coverage = investment
        .insurance
        .get(idx)
        .expect("insurance entry must exist");
    coverage.active = false;
    investment.insurance.set(idx, coverage);
    InvestmentStorage::update_investment(env, &investment);
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

    let investment_id = store_investment(&env, &investor, 10_000, InvestmentStatus::Active, 1);

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
}

// ============================================================================
// State Validation Tests
// ============================================================================

#[test]
fn test_add_insurance_requires_active_investment() {
    let (env, client, _contract_id) = setup();
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
            store_investment(&env, &investor, 5_000, status.clone(), (idx + 2) as u8);

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
    let (env, client, _contract_id) = setup();
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
    let (env, client, _contract_id) = setup();
    env.mock_all_auths();

    let investor = Address::generate(&env);
    let provider = Address::generate(&env);

    let investment_id = store_investment(&env, &investor, 7_500, InvestmentStatus::Active, 9);

    let mut investment = InvestmentStorage::get_investment(&env, &investment_id).unwrap();
    investment.status = InvestmentStatus::Completed;
    InvestmentStorage::update_investment(&env, &investment);

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
    let (env, client, _contract_id) = setup();
    env.mock_all_auths();

    let investor = Address::generate(&env);
    let provider = Address::generate(&env);

    let investment_id = store_investment(&env, &investor, 10_000, InvestmentStatus::Active, 4);

    client.add_investment_insurance(&investment_id, &provider, &80u32);

    let stored = client.get_investment(&investment_id);
    let insurance = stored.insurance.get(0).unwrap();
    assert_eq!(insurance.coverage_amount, 8_000);
    assert_eq!(insurance.premium_amount, 160);
    assert_eq!(
        insurance.premium_amount,
        Investment::calculate_premium(10_000, 80)
    );

    let investment_id_small = store_investment(&env, &investor, 500, InvestmentStatus::Active, 5);
    client.add_investment_insurance(&investment_id_small, &provider, &1u32);

    let stored_small = client.get_investment(&investment_id_small);
    let insurance_small = stored_small.insurance.get(0).unwrap();
    assert_eq!(insurance_small.coverage_amount, 5);
    assert_eq!(insurance_small.premium_amount, 1);
}

#[test]
fn test_zero_coverage_and_invalid_inputs() {
    let (env, client, _contract_id) = setup();
    env.mock_all_auths();

    let investor = Address::generate(&env);
    let provider = Address::generate(&env);

    let investment_id = store_investment(&env, &investor, 1_000, InvestmentStatus::Active, 6);

    let result = client.try_add_investment_insurance(&investment_id, &provider, &0u32);
    let err = result.err().expect("expected invalid amount error");
    let contract_error = err.expect("expected contract error");
    assert_eq!(contract_error, QuickLendXError::InvalidAmount);

    let result = client.try_add_investment_insurance(&investment_id, &provider, &150u32);
    let err = result.err().expect("expected invalid coverage error");
    let contract_error = err.expect("expected contract error");
    assert_eq!(contract_error, QuickLendXError::InvalidCoveragePercentage);

    let small_amount_id = store_investment(&env, &investor, 50, InvestmentStatus::Active, 7);
    let result = client.try_add_investment_insurance(&small_amount_id, &provider, &1u32);
    let err = result.err().expect("expected invalid amount error");
    let contract_error = err.expect("expected contract error");
    assert_eq!(contract_error, QuickLendXError::InvalidAmount);

    let negative_amount_id = store_investment(&env, &investor, -10, InvestmentStatus::Active, 8);
    let result = client.try_add_investment_insurance(&negative_amount_id, &provider, &10u32);
    let err = result.err().expect("expected invalid amount error");
    let contract_error = err.expect("expected contract error");
    assert_eq!(contract_error, QuickLendXError::InvalidAmount);
}

#[test]
fn test_large_values_handle_saturation() {
    let (env, client, _contract_id) = setup();
    env.mock_all_auths();

    let investor = Address::generate(&env);
    let provider = Address::generate(&env);

    let amount = i128::MAX;
    let investment_id = store_investment(&env, &investor, amount, InvestmentStatus::Active, 10);

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
    let (env, client, _contract_id) = setup();
    env.mock_all_auths();

    let investor = Address::generate(&env);
    let provider_one = Address::generate(&env);
    let provider_two = Address::generate(&env);
    let provider_three = Address::generate(&env);

    let investment_a = store_investment(&env, &investor, 12_000, InvestmentStatus::Active, 11);
    let investment_b = store_investment(&env, &investor, 8_000, InvestmentStatus::Active, 12);

    client.add_investment_insurance(&investment_a, &provider_one, &60u32);

    set_insurance_inactive(&env, &investment_a, 0);
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
// Security / Edge Scenarios
// ============================================================================

#[test]
fn test_duplicate_submission_rejected_and_state_unchanged() {
    let (env, client, _contract_id) = setup();
    env.mock_all_auths();

    let investor = Address::generate(&env);
    let provider = Address::generate(&env);
    let provider_two = Address::generate(&env);

    let investment_id = store_investment(&env, &investor, 9_000, InvestmentStatus::Active, 13);
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
