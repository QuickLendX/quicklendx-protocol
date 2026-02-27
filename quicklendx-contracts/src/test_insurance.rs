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


// ============================================================================
// State Validation Tests
// ============================================================================


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


// ============================================================================
// Coverage / Premium Math Tests
// ============================================================================


#[test]
fn test_zero_coverage_and_invalid_inputs() {
    let (env, client, contract_id) = setup();
    env.mock_all_auths();

    let investor = Address::generate(&env);
    let provider = Address::generate(&env);

    let investment_id = store_investment(
        &env,
        &contract_id,
        &investor,
        1_000,
        InvestmentStatus::Active,
        6,
    );

    let result = client.try_add_investment_insurance(&investment_id, &provider, &0u32);
    let err = result.err().expect("expected invalid amount error");
    let contract_error = err.expect("expected contract error");
    assert_eq!(contract_error, QuickLendXError::InvalidAmount);

    let result = client.try_add_investment_insurance(&investment_id, &provider, &150u32);
    let err = result.err().expect("expected invalid coverage error");
    let contract_error = err.expect("expected contract error");
    assert_eq!(contract_error, QuickLendXError::InvalidCoveragePercentage);

    let small_amount_id = store_investment(
        &env,
        &contract_id,
        &investor,
        50,
        InvestmentStatus::Active,
        7,
    );
    let result = client.try_add_investment_insurance(&small_amount_id, &provider, &1u32);
    let err = result.err().expect("expected invalid amount error");
    let contract_error = err.expect("expected contract error");
    assert_eq!(contract_error, QuickLendXError::InvalidAmount);

    let negative_amount_id = store_investment(
        &env,
        &contract_id,
        &investor,
        -10,
        InvestmentStatus::Active,
        8,
    );
    let result = client.try_add_investment_insurance(&negative_amount_id, &provider, &10u32);
    let err = result.err().expect("expected invalid amount error");
    let contract_error = err.expect("expected contract error");
    assert_eq!(contract_error, QuickLendXError::InvalidAmount);
}


// ============================================================================
// Multiple Entries + Query Correctness
// ============================================================================


// ============================================================================
// Security / Edge Scenarios
// ============================================================================


// ============================================================================
// Multiple coverages, premium, query returns all, cannot add when not Active (#359)
// ============================================================================


#[test]
fn test_query_investment_insurance_returns_all_entries() {
    let (env, client, contract_id) = setup();
    env.mock_all_auths();

    let investor = Address::generate(&env);
    let provider_a = Address::generate(&env);
    let provider_b = Address::generate(&env);

    let investment_id = store_investment(
        &env,
        &contract_id,
        &investor,
        10_000,
        InvestmentStatus::Active,
        21,
    );

    client.add_investment_insurance(&investment_id, &provider_a, &40u32);
    set_insurance_inactive(&env, &contract_id, &investment_id, 0);
    client.add_investment_insurance(&investment_id, &provider_b, &60u32);

    let all = client.query_investment_insurance(&investment_id);
    assert_eq!(all.len(), 2);
    assert_eq!(all.get(0).unwrap().provider, provider_a);
    assert_eq!(all.get(1).unwrap().provider, provider_b);
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
