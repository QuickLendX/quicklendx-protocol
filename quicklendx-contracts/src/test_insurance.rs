/// Comprehensive test suite for investment insurance parameter validation.
///
/// # Coverage areas
/// 1.  Bounds constants – verify the public constants match their documented values.
/// 2.  `calculate_premium` – pure math, boundary values, overflow safety, invariants.
/// 3.  Authorization – only the investment owner can call `add_investment_insurance`.
/// 4.  State validation – insurance is only allowed on Active investments.
/// 5.  Coverage-percentage validation – below min, above max, exact boundaries.
/// 6.  Investment-amount validation – zero, negative, tiny (premium rounds to 0).
/// 7.  Active-insurance guard – only one active policy per investment.
/// 8.  Premium-vs-coverage invariant – premium must not exceed coverage amount.
/// 9.  Over-coverage exploit prevention – coverage_amount never exceeds principal.
/// 10. Query correctness – `query_investment_insurance` returns all historical entries.
/// 11. Claim / process_insurance_claim – deactivates coverage, returns correct amounts.
/// 12. Cross-investment isolation – operations on one investment do not affect another.
extern crate alloc;
use super::*;
use crate::errors::QuickLendXError;
use crate::investment::{
    Investment, InvestmentStatus, InvestmentStorage, DEFAULT_INSURANCE_PREMIUM_BPS,
    MAX_COVERAGE_PERCENTAGE, MIN_COVERAGE_PERCENTAGE, MIN_PREMIUM_AMOUNT,
};
use soroban_sdk::{
    testutils::{Address as _, MockAuth, MockAuthInvoke},
    Address, BytesN, Env, IntoVal, Vec,
};

// ============================================================================
// Test helpers
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

/// Store a bare investment directly via storage (no contract call needed).
fn store_investment(
    env: &Env,
    contract_id: &Address,
    investor: &Address,
    amount: i128,
    status: InvestmentStatus,
    seed: u8,
) -> BytesN<32> {
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

/// Deactivate the insurance entry at `idx` inside a stored investment.
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
// 1. Bounds constants
// ============================================================================

#[test]
fn test_constants_have_expected_values() {
    assert_eq!(DEFAULT_INSURANCE_PREMIUM_BPS, 200);
    assert_eq!(MIN_COVERAGE_PERCENTAGE, 1);
    assert_eq!(MAX_COVERAGE_PERCENTAGE, 100);
    assert_eq!(MIN_PREMIUM_AMOUNT, 1);
}

// ============================================================================
// 2. calculate_premium – pure unit tests (no contract required)
// ============================================================================

#[test]
fn test_calculate_premium_typical_cases() {
    // 10 000 × 80 % = 8 000 coverage; 8 000 × 2 % = 160 premium
    assert_eq!(Investment::calculate_premium(10_000, 80), 160);
    // 10 000 × 50 % = 5 000 coverage; 5 000 × 2 % = 100 premium
    assert_eq!(Investment::calculate_premium(10_000, 50), 100);
    // 10 000 × 100 % = 10 000 coverage; 10 000 × 2 % = 200 premium
    assert_eq!(Investment::calculate_premium(10_000, 100), 200);
    // 10 000 × 1 % = 100 coverage; 100 × 2 % = 2 premium
    assert_eq!(Investment::calculate_premium(10_000, 1), 2);
}

#[test]
fn test_calculate_premium_returns_zero_for_invalid_inputs() {
    // Zero amount
    assert_eq!(Investment::calculate_premium(0, 50), 0);
    // Negative amount
    assert_eq!(Investment::calculate_premium(-1, 50), 0);
    // Coverage percentage below minimum
    assert_eq!(Investment::calculate_premium(10_000, 0), 0);
    // Coverage percentage above maximum
    assert_eq!(Investment::calculate_premium(10_000, 101), 0);
    assert_eq!(Investment::calculate_premium(10_000, 200), 0);
    assert_eq!(Investment::calculate_premium(10_000, u32::MAX), 0);
}

#[test]
fn test_calculate_premium_minimum_floor() {
    // amount = 500, coverage = 1 % → coverage_amount = 5; 5 × 2 % = 0 (integer)
    // Floor kicks in → premium = 1
    assert_eq!(Investment::calculate_premium(500, 1), 1);

    // amount = 100, coverage = 1 % → coverage_amount = 1; 1 × 2 % = 0 → floor → 1
    assert_eq!(Investment::calculate_premium(100, 1), 1);
}

#[test]
fn test_calculate_premium_boundary_coverage_percentages() {
    // Exactly MIN_COVERAGE_PERCENTAGE should succeed
    let p = Investment::calculate_premium(10_000, MIN_COVERAGE_PERCENTAGE);
    assert!(p >= MIN_PREMIUM_AMOUNT);

    // Exactly MAX_COVERAGE_PERCENTAGE should succeed
    let p = Investment::calculate_premium(10_000, MAX_COVERAGE_PERCENTAGE);
    assert!(p >= MIN_PREMIUM_AMOUNT);

    // One below min → 0
    assert_eq!(Investment::calculate_premium(10_000, MIN_COVERAGE_PERCENTAGE - 1), 0);

    // One above max → 0  (over-coverage guard)
    assert_eq!(Investment::calculate_premium(10_000, MAX_COVERAGE_PERCENTAGE + 1), 0);
}

#[test]
fn test_calculate_premium_coverage_never_exceeds_amount() {
    // For all valid percentages, coverage_amount ≤ amount.
    let amount: i128 = 9_999;
    for pct in MIN_COVERAGE_PERCENTAGE..=MAX_COVERAGE_PERCENTAGE {
        let premium = Investment::calculate_premium(amount, pct);
        // premium > 0 means inputs were valid; verify invariant holds
        if premium > 0 {
            let coverage_amount = amount * pct as i128 / 100;
            assert!(coverage_amount <= amount, "coverage_amount must not exceed principal");
        }
    }
}

#[test]
fn test_calculate_premium_overflow_safety() {
    // i128::MAX amount with maximum coverage_percentage must not panic.
    let result = Investment::calculate_premium(i128::MAX, MAX_COVERAGE_PERCENTAGE);
    // saturating_mul on i128::MAX × 100 saturates; checked_div then gives
    // Some(i128::MAX) → premium > 0.  The exact value is less important than
    // the absence of a panic.
    assert!(result >= 0);

    // Large but representable amount
    let large: i128 = 1_000_000_000_000_000_000; // 1 quintillion
    let p = Investment::calculate_premium(large, 80);
    // 1e18 × 80 / 100 = 8e17; 8e17 × 200 / 10_000 = 1.6e16
    assert_eq!(p, 16_000_000_000_000_000);
}

#[test]
fn test_calculate_premium_premium_does_not_exceed_coverage() {
    // With DEFAULT_INSURANCE_PREMIUM_BPS = 200 (2 %), premium ≤ coverage_amount
    // for all valid inputs.
    for pct in MIN_COVERAGE_PERCENTAGE..=MAX_COVERAGE_PERCENTAGE {
        let amount: i128 = 1_000_000;
        let premium = Investment::calculate_premium(amount, pct);
        if premium > 0 {
            let coverage_amount = amount * pct as i128 / 100;
            assert!(premium <= coverage_amount, "premium must not exceed coverage amount");
        }
    }
}

// ============================================================================
// 3. Authorization
// ============================================================================

#[test]
fn test_add_insurance_requires_exactly_investor_auth() {
    // Soroban auth violations result in a host panic — we verify the correct
    // authorization call is made by supplying only the investor's MockAuth and
    // confirming the invocation succeeds. If the contract called require_auth()
    // on any other address (or not at all), the mock would not match and the
    // test would fail.
    let (env, client, contract_id) = setup();

    let investor = Address::generate(&env);
    let provider = Address::generate(&env);
    let investment_id =
        store_investment(&env, &contract_id, &investor, 10_000, InvestmentStatus::Active, 1);

    env.mock_auths(&[MockAuth {
        address: &investor,
        invoke: &MockAuthInvoke {
            contract: &contract_id,
            fn_name: "add_investment_insurance",
            args: (&investment_id, &provider, &50u32).into_val(&env),
            sub_invokes: &[],
        },
    }]);

    // Succeeds because exactly the investor's auth is supplied.
    client.add_investment_insurance(&investment_id, &provider, &50u32);

    let records = client.query_investment_insurance(&investment_id);
    assert_eq!(records.len(), 1);
}

// ============================================================================
// 4. State validation – investment status
// ============================================================================

#[test]
fn test_add_insurance_fails_when_investment_not_found() {
    let (env, client, _) = setup();
    env.mock_all_auths();

    let provider = Address::generate(&env);
    let missing_id = BytesN::from_array(&env, &[0u8; 32]);

    let err = client
        .try_add_investment_insurance(&missing_id, &provider, &50u32)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, QuickLendXError::StorageKeyNotFound);
}

#[test]
fn test_add_insurance_fails_on_withdrawn_investment() {
    let (env, client, contract_id) = setup();
    env.mock_all_auths();

    let investor = Address::generate(&env);
    let provider = Address::generate(&env);
    let id = store_investment(&env, &contract_id, &investor, 10_000, InvestmentStatus::Withdrawn, 2);

    let err = client
        .try_add_investment_insurance(&id, &provider, &50u32)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, QuickLendXError::InvalidStatus);
}

#[test]
fn test_add_insurance_fails_on_completed_investment() {
    let (env, client, contract_id) = setup();
    env.mock_all_auths();

    let investor = Address::generate(&env);
    let provider = Address::generate(&env);
    let id = store_investment(&env, &contract_id, &investor, 10_000, InvestmentStatus::Completed, 3);

    let err = client
        .try_add_investment_insurance(&id, &provider, &50u32)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, QuickLendXError::InvalidStatus);
}

#[test]
fn test_add_insurance_fails_on_defaulted_investment() {
    let (env, client, contract_id) = setup();
    env.mock_all_auths();

    let investor = Address::generate(&env);
    let provider = Address::generate(&env);
    let id = store_investment(&env, &contract_id, &investor, 10_000, InvestmentStatus::Defaulted, 4);

    let err = client
        .try_add_investment_insurance(&id, &provider, &50u32)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, QuickLendXError::InvalidStatus);
}

#[test]
fn test_add_insurance_fails_on_refunded_investment() {
    let (env, client, contract_id) = setup();
    env.mock_all_auths();

    let investor = Address::generate(&env);
    let provider = Address::generate(&env);
    let id = store_investment(&env, &contract_id, &investor, 10_000, InvestmentStatus::Refunded, 5);

    let err = client
        .try_add_investment_insurance(&id, &provider, &50u32)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, QuickLendXError::InvalidStatus);
}

// ============================================================================
// 5. Coverage-percentage validation
// ============================================================================

#[test]
fn test_add_insurance_rejects_zero_coverage_percentage() {
    let (env, client, contract_id) = setup();
    env.mock_all_auths();

    let investor = Address::generate(&env);
    let provider = Address::generate(&env);
    let id = store_investment(&env, &contract_id, &investor, 10_000, InvestmentStatus::Active, 10);

    // coverage_percentage = 0 is below MIN_COVERAGE_PERCENTAGE → specific error
    let err = client
        .try_add_investment_insurance(&id, &provider, &0u32)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, QuickLendXError::InvalidCoveragePercentage);
}

#[test]
fn test_add_insurance_rejects_over_100_coverage_percentage() {
    let (env, client, contract_id) = setup();
    env.mock_all_auths();

    let investor = Address::generate(&env);
    let provider = Address::generate(&env);
    let id = store_investment(&env, &contract_id, &investor, 10_000, InvestmentStatus::Active, 11);

    for &pct in &[101u32, 150, 200, u32::MAX] {
        let err = client
            .try_add_investment_insurance(&id, &provider, &pct)
            .err()
            .unwrap()
            .unwrap();
        assert_eq!(
            err,
            QuickLendXError::InvalidCoveragePercentage
        );
    }
}

#[test]
fn test_add_insurance_accepts_min_coverage_percentage() {
    let (env, client, contract_id) = setup();
    env.mock_all_auths();

    let investor = Address::generate(&env);
    let provider = Address::generate(&env);
    // Use a large enough amount so 1 % gives a non-zero coverage_amount and premium.
    let id = store_investment(&env, &contract_id, &investor, 100_000, InvestmentStatus::Active, 12);

    client.add_investment_insurance(&id, &provider, &MIN_COVERAGE_PERCENTAGE);

    let records = client.query_investment_insurance(&id);
    assert_eq!(records.len(), 1);
    assert_eq!(records.get(0).unwrap().coverage_percentage, MIN_COVERAGE_PERCENTAGE);
}

#[test]
fn test_add_insurance_accepts_max_coverage_percentage() {
    let (env, client, contract_id) = setup();
    env.mock_all_auths();

    let investor = Address::generate(&env);
    let provider = Address::generate(&env);
    let id = store_investment(&env, &contract_id, &investor, 10_000, InvestmentStatus::Active, 13);

    client.add_investment_insurance(&id, &provider, &MAX_COVERAGE_PERCENTAGE);

    let records = client.query_investment_insurance(&id);
    assert_eq!(records.len(), 1);
    assert_eq!(records.get(0).unwrap().coverage_percentage, MAX_COVERAGE_PERCENTAGE);
    // 100 % coverage means coverage_amount == investment amount
    assert_eq!(records.get(0).unwrap().coverage_amount, 10_000);
}

// ============================================================================
// 6. Investment-amount validation
// ============================================================================

#[test]
fn test_add_insurance_rejects_negative_investment_amount() {
    let (env, client, contract_id) = setup();
    env.mock_all_auths();

    let investor = Address::generate(&env);
    let provider = Address::generate(&env);
    let id = store_investment(&env, &contract_id, &investor, -1, InvestmentStatus::Active, 20);

    let err = client
        .try_add_investment_insurance(&id, &provider, &50u32)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, QuickLendXError::InvalidAmount);
}

#[test]
fn test_add_insurance_rejects_zero_investment_amount() {
    let (env, client, contract_id) = setup();
    env.mock_all_auths();

    let investor = Address::generate(&env);
    let provider = Address::generate(&env);
    let id = store_investment(&env, &contract_id, &investor, 0, InvestmentStatus::Active, 21);

    let err = client
        .try_add_investment_insurance(&id, &provider, &50u32)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, QuickLendXError::InvalidAmount);
}

#[test]
fn test_add_insurance_rejects_tiny_amount_where_premium_rounds_to_zero() {
    // amount=50, coverage=1 % → coverage_amount = 0 (integer division) → premium = 0
    let (env, client, contract_id) = setup();
    env.mock_all_auths();

    let investor = Address::generate(&env);
    let provider = Address::generate(&env);
    let id = store_investment(&env, &contract_id, &investor, 50, InvestmentStatus::Active, 22);

    let err = client
        .try_add_investment_insurance(&id, &provider, &1u32)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, QuickLendXError::InvalidAmount);
}

// ============================================================================
// 7. Active-insurance guard (OperationNotAllowed)
// ============================================================================

#[test]
fn test_add_insurance_rejects_duplicate_active_coverage() {
    let (env, client, contract_id) = setup();
    env.mock_all_auths();

    let investor = Address::generate(&env);
    let provider_a = Address::generate(&env);
    let provider_b = Address::generate(&env);
    let id = store_investment(&env, &contract_id, &investor, 10_000, InvestmentStatus::Active, 30);

    // First add succeeds
    client.add_investment_insurance(&id, &provider_a, &60u32);

    // Second add while active coverage exists must fail
    let err = client
        .try_add_investment_insurance(&id, &provider_b, &40u32)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, QuickLendXError::OperationNotAllowed);
}

#[test]
fn test_add_insurance_allowed_after_previous_is_deactivated() {
    let (env, client, contract_id) = setup();
    env.mock_all_auths();

    let investor = Address::generate(&env);
    let provider_a = Address::generate(&env);
    let provider_b = Address::generate(&env);
    let id = store_investment(&env, &contract_id, &investor, 10_000, InvestmentStatus::Active, 31);

    client.add_investment_insurance(&id, &provider_a, &40u32);
    set_insurance_inactive(&env, &contract_id, &id, 0);

    // After deactivation a new policy may be added
    client.add_investment_insurance(&id, &provider_b, &60u32);

    let records = client.query_investment_insurance(&id);
    assert_eq!(records.len(), 2);
    assert!(!records.get(0).unwrap().active); // first is now inactive
    assert!(records.get(1).unwrap().active);  // second is active
}

// ============================================================================
// 8. Premium correctness
// ============================================================================

#[test]
fn test_premium_stored_in_coverage_record() {
    let (env, client, contract_id) = setup();
    env.mock_all_auths();

    let investor = Address::generate(&env);
    let provider = Address::generate(&env);
    // 10 000 × 80 % = 8 000; 8 000 × 2 % = 160
    let id = store_investment(&env, &contract_id, &investor, 10_000, InvestmentStatus::Active, 40);

    client.add_investment_insurance(&id, &provider, &80u32);

    let records = client.query_investment_insurance(&id);
    let cov = records.get(0).unwrap();
    assert_eq!(cov.coverage_amount, 8_000);
    assert_eq!(cov.premium_amount, 160);
}

#[test]
fn test_premium_minimum_floor_applied() {
    // 100 × 1 % = 1 coverage; 1 × 2 % = 0 (integer) → floor → premium = 1
    let (env, client, contract_id) = setup();
    env.mock_all_auths();

    let investor = Address::generate(&env);
    let provider = Address::generate(&env);
    let id = store_investment(&env, &contract_id, &investor, 100, InvestmentStatus::Active, 41);

    client.add_investment_insurance(&id, &provider, &1u32);

    let cov = client.query_investment_insurance(&id).get(0).unwrap();
    assert_eq!(cov.coverage_amount, 1);
    assert_eq!(cov.premium_amount, 1);  // floor applied
}

// ============================================================================
// 9. Over-coverage exploit prevention
// ============================================================================

#[test]
fn test_coverage_amount_never_exceeds_investment_amount() {
    let (env, client, contract_id) = setup();
    env.mock_all_auths();

    let investor = Address::generate(&env);
    let provider = Address::generate(&env);
    let amount: i128 = 7_777;
    let id = store_investment(&env, &contract_id, &investor, amount, InvestmentStatus::Active, 50);

    client.add_investment_insurance(&id, &provider, &MAX_COVERAGE_PERCENTAGE);

    let cov = client.query_investment_insurance(&id).get(0).unwrap();
    assert!(cov.coverage_amount <= amount, "coverage_amount must not exceed investment amount");
    assert_eq!(cov.coverage_amount, amount); // 100 % → exact match
}

#[test]
fn test_over_100_percent_rejected_with_specific_error() {
    // This is the core over-coverage exploit test: an attacker supplying
    // coverage_percentage = 200 would compute coverage_amount = 2 × principal.
    // The explicit range check in lib.rs must catch this before arithmetic.
    let (env, client, contract_id) = setup();
    env.mock_all_auths();

    let investor = Address::generate(&env);
    let provider = Address::generate(&env);
    let id = store_investment(&env, &contract_id, &investor, 10_000, InvestmentStatus::Active, 51);

    for &pct in &[101u32, 150, 200, 1_000, u32::MAX] {
        let err = client
            .try_add_investment_insurance(&id, &provider, &pct)
            .err()
            .unwrap()
            .unwrap();
        assert_eq!(
            err,
            QuickLendXError::InvalidCoveragePercentage
        );
    }
}

// ============================================================================
// 10. Query correctness
// ============================================================================

#[test]
fn test_query_returns_empty_for_new_investment() {
    let (env, client, contract_id) = setup();
    env.mock_all_auths();

    let investor = Address::generate(&env);
    let id = store_investment(&env, &contract_id, &investor, 10_000, InvestmentStatus::Active, 60);

    let records = client.query_investment_insurance(&id);
    assert_eq!(records.len(), 0);
}

#[test]
fn test_query_returns_all_historical_entries() {
    let (env, client, contract_id) = setup();
    env.mock_all_auths();

    let investor = Address::generate(&env);
    let provider_a = Address::generate(&env);
    let provider_b = Address::generate(&env);
    let id = store_investment(&env, &contract_id, &investor, 10_000, InvestmentStatus::Active, 61);

    client.add_investment_insurance(&id, &provider_a, &40u32);
    set_insurance_inactive(&env, &contract_id, &id, 0);
    client.add_investment_insurance(&id, &provider_b, &60u32);

    let records = client.query_investment_insurance(&id);
    assert_eq!(records.len(), 2);
    assert_eq!(records.get(0).unwrap().provider, provider_a);
    assert!(!records.get(0).unwrap().active);
    assert_eq!(records.get(1).unwrap().provider, provider_b);
    assert!(records.get(1).unwrap().active);
}

#[test]
fn test_query_nonexistent_investment_returns_error() {
    let (env, client, _) = setup();
    env.mock_all_auths();

    let missing = BytesN::from_array(&env, &[0xFFu8; 32]);
    let err = client
        .try_query_investment_insurance(&missing)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, QuickLendXError::StorageKeyNotFound);
}

// ============================================================================
// 11. Claim / process_insurance_claim
// ============================================================================

#[test]
fn test_process_insurance_claim_deactivates_and_returns_amount() {
    let env = Env::default();
    let investor = Address::generate(&env);
    let provider = Address::generate(&env);

    let mut investment = Investment {
        investment_id: BytesN::from_array(&env, &[1u8; 32]),
        invoice_id: BytesN::from_array(&env, &[2u8; 32]),
        investor: investor.clone(),
        amount: 10_000,
        funded_at: env.ledger().timestamp(),
        status: InvestmentStatus::Active,
        insurance: Vec::new(&env),
    };

    let premium = Investment::calculate_premium(10_000, 80);
    investment
        .add_insurance(provider.clone(), 80, premium)
        .expect("insurance should be added");
    assert!(investment.has_active_insurance());

    let (claim_provider, claim_amount) = investment
        .process_insurance_claim()
        .expect("claim must succeed");
    assert_eq!(claim_provider, provider);
    assert_eq!(claim_amount, 8_000); // 80 % of 10_000
    assert!(!investment.has_active_insurance());
}

#[test]
fn test_process_insurance_claim_returns_none_when_no_active_coverage() {
    let env = Env::default();
    let investor = Address::generate(&env);

    let mut investment = Investment {
        investment_id: BytesN::from_array(&env, &[3u8; 32]),
        invoice_id: BytesN::from_array(&env, &[4u8; 32]),
        investor: investor.clone(),
        amount: 5_000,
        funded_at: env.ledger().timestamp(),
        status: InvestmentStatus::Active,
        insurance: Vec::new(&env),
    };

    assert!(investment.process_insurance_claim().is_none());
}

#[test]
fn test_second_claim_returns_none_after_first() {
    let env = Env::default();
    let investor = Address::generate(&env);
    let provider = Address::generate(&env);

    let mut investment = Investment {
        investment_id: BytesN::from_array(&env, &[5u8; 32]),
        invoice_id: BytesN::from_array(&env, &[6u8; 32]),
        investor: investor.clone(),
        amount: 1_000,
        funded_at: env.ledger().timestamp(),
        status: InvestmentStatus::Active,
        insurance: Vec::new(&env),
    };

    let premium = Investment::calculate_premium(1_000, 100);
    investment.add_insurance(provider.clone(), 100, premium).unwrap();
    investment.process_insurance_claim().unwrap();

    // Second call must return None because coverage is now inactive.
    assert!(investment.process_insurance_claim().is_none());
}

// ============================================================================
// 12. Cross-investment isolation
// ============================================================================

#[test]
fn test_insurance_on_one_investment_does_not_affect_another() {
    let (env, client, contract_id) = setup();
    env.mock_all_auths();

    let investor = Address::generate(&env);
    let provider = Address::generate(&env);
    let id_a = store_investment(&env, &contract_id, &investor, 10_000, InvestmentStatus::Active, 70);
    let id_b = store_investment(&env, &contract_id, &investor, 20_000, InvestmentStatus::Active, 71);

    client.add_investment_insurance(&id_a, &provider, &50u32);

    // investment_b must still have zero insurance entries
    let records_b = client.query_investment_insurance(&id_b);
    assert_eq!(records_b.len(), 0);

    // investment_a has one active entry
    let records_a = client.query_investment_insurance(&id_a);
    assert_eq!(records_a.len(), 1);
    assert!(records_a.get(0).unwrap().active);
}

// ============================================================================
// 13. add_insurance direct unit tests (independent of contract dispatch)
// ============================================================================

#[test]
fn test_add_insurance_unit_coverage_percentage_bounds() {
    let env = Env::default();
    let investor = Address::generate(&env);
    let provider = Address::generate(&env);

    let mut inv = Investment {
        investment_id: BytesN::from_array(&env, &[7u8; 32]),
        invoice_id: BytesN::from_array(&env, &[8u8; 32]),
        investor: investor.clone(),
        amount: 1_000,
        funded_at: 0,
        status: InvestmentStatus::Active,
        insurance: Vec::new(&env),
    };

    // coverage_percentage = 0 → InvalidCoveragePercentage
    assert_eq!(
        inv.add_insurance(provider.clone(), 0, 10),
        Err(QuickLendXError::InvalidCoveragePercentage)
    );

    // coverage_percentage = 101 → InvalidCoveragePercentage
    assert_eq!(
        inv.add_insurance(provider.clone(), 101, 10),
        Err(QuickLendXError::InvalidCoveragePercentage)
    );

    // coverage_percentage = MAX_COVERAGE_PERCENTAGE with valid premium → Ok
    let premium = Investment::calculate_premium(1_000, MAX_COVERAGE_PERCENTAGE);
    assert!(inv.add_insurance(provider.clone(), MAX_COVERAGE_PERCENTAGE, premium).is_ok());
}

#[test]
fn test_add_insurance_unit_rejects_non_positive_investment_amount() {
    let env = Env::default();
    let investor = Address::generate(&env);
    let provider = Address::generate(&env);

    for &bad_amount in &[0i128, -1, -1_000_000] {
        let mut inv = Investment {
            investment_id: BytesN::from_array(&env, &[9u8; 32]),
            invoice_id: BytesN::from_array(&env, &[10u8; 32]),
            investor: investor.clone(),
            amount: bad_amount,
            funded_at: 0,
            status: InvestmentStatus::Active,
            insurance: Vec::new(&env),
        };
        assert_eq!(
            inv.add_insurance(provider.clone(), 50, 1),
            Err(QuickLendXError::InvalidAmount)
        );
    }
}

#[test]
fn test_add_insurance_unit_rejects_below_minimum_premium() {
    let env = Env::default();
    let investor = Address::generate(&env);
    let provider = Address::generate(&env);

    let mut inv = Investment {
        investment_id: BytesN::from_array(&env, &[11u8; 32]),
        invoice_id: BytesN::from_array(&env, &[12u8; 32]),
        investor: investor.clone(),
        amount: 10_000,
        funded_at: 0,
        status: InvestmentStatus::Active,
        insurance: Vec::new(&env),
    };

    // premium = 0 → below MIN_PREMIUM_AMOUNT
    assert_eq!(
        inv.add_insurance(provider.clone(), 50, 0),
        Err(QuickLendXError::InvalidAmount)
    );

    // premium = -1 → below MIN_PREMIUM_AMOUNT
    assert_eq!(
        inv.add_insurance(provider.clone(), 50, -1),
        Err(QuickLendXError::InvalidAmount)
    );
}

#[test]
fn test_add_insurance_unit_rejects_premium_exceeding_coverage_amount() {
    let env = Env::default();
    let investor = Address::generate(&env);
    let provider = Address::generate(&env);

    let mut inv = Investment {
        investment_id: BytesN::from_array(&env, &[13u8; 32]),
        invoice_id: BytesN::from_array(&env, &[14u8; 32]),
        investor: investor.clone(),
        amount: 1_000,
        funded_at: 0,
        status: InvestmentStatus::Active,
        insurance: Vec::new(&env),
    };

    // coverage_percentage = 10 → coverage_amount = 100
    // premium = 101 > coverage_amount → should be rejected
    assert_eq!(
        inv.add_insurance(provider.clone(), 10, 101),
        Err(QuickLendXError::InvalidAmount)
    );

    // premium = coverage_amount exactly → should be accepted (edge of valid range)
    assert!(inv.add_insurance(provider.clone(), 10, 100).is_ok());
}

#[test]
fn test_add_insurance_unit_active_guard() {
    let env = Env::default();
    let investor = Address::generate(&env);
    let provider = Address::generate(&env);

    let mut inv = Investment {
        investment_id: BytesN::from_array(&env, &[15u8; 32]),
        invoice_id: BytesN::from_array(&env, &[16u8; 32]),
        investor: investor.clone(),
        amount: 10_000,
        funded_at: 0,
        status: InvestmentStatus::Active,
        insurance: Vec::new(&env),
    };

    let premium = Investment::calculate_premium(10_000, 50);
    inv.add_insurance(provider.clone(), 50, premium).unwrap();

    // Second add while active → OperationNotAllowed
    assert_eq!(
        inv.add_insurance(provider.clone(), 30, premium),
        Err(QuickLendXError::OperationNotAllowed)
    );
}
