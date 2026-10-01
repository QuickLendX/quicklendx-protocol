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
use soroban_sdk::{testutils::Address as _, Address, BytesN, Env, Vec};

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

    let err = result
        .err()
        .expect("expected error for nonexistent investment");
    let contract_error = err.expect("expected contract error");
    assert_eq!(contract_error, QuickLendXError::StorageKeyNotFound);
}

#[test]
fn test_get_invoice_investment_nonexistent_returns_error() {
    let (env, client, _) = setup();
    env.mock_all_auths();

    let nonexistent_invoice_id = BytesN::from_array(&env, &[0u8; 32]);
    let result = client.try_get_invoice_investment(&nonexistent_invoice_id);

    let err = result
        .err()
        .expect("expected error for nonexistent invoice");
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

// ============================================================================
// get_investor_portfolio_summary Tests
// ============================================================================

/// Edge case: investor with zero investments → all fields are 0.
#[test]
fn test_portfolio_summary_zero_investments() {
    let (env, client, _) = setup();
    env.mock_all_auths();

    let investor = Address::generate(&env);
    let summary = client.get_investor_portfolio_summary(&investor);

    assert_eq!(summary.active_principal, 0);
    assert_eq!(summary.completed_count, 0);
    assert_eq!(summary.completed_returns, 0);
    assert_eq!(summary.defaulted_count, 0);
    assert_eq!(summary.refunded_count, 0);
    assert_eq!(summary.total_positions, 0);
}

/// Edge case: investor with only Defaulted positions.
#[test]
fn test_portfolio_summary_only_defaulted() {
    let (env, client, contract_id) = setup();
    env.mock_all_auths();

    let investor = Address::generate(&env);
    create_test_investment(
        &env,
        &contract_id,
        &investor,
        1_000,
        InvestmentStatus::Defaulted,
        90,
    );
    create_test_investment(
        &env,
        &contract_id,
        &investor,
        2_000,
        InvestmentStatus::Defaulted,
        91,
    );

    let summary = client.get_investor_portfolio_summary(&investor);

    assert_eq!(summary.active_principal, 0);
    assert_eq!(summary.completed_count, 0);
    assert_eq!(summary.completed_returns, 0);
    assert_eq!(summary.defaulted_count, 2);
    assert_eq!(summary.refunded_count, 0);
    assert_eq!(summary.total_positions, 2);
}

/// Mixed-status portfolio: all buckets populated.
#[test]
fn test_portfolio_summary_mixed_statuses() {
    let (env, client, contract_id) = setup();
    env.mock_all_auths();

    let investor = Address::generate(&env);
    // Active: 3_000 + 5_000 = 8_000 principal
    create_test_investment(
        &env,
        &contract_id,
        &investor,
        3_000,
        InvestmentStatus::Active,
        100,
    );
    create_test_investment(
        &env,
        &contract_id,
        &investor,
        5_000,
        InvestmentStatus::Active,
        101,
    );
    // Completed: 2 positions, 4_000 + 6_000 = 10_000 returns
    create_test_investment(
        &env,
        &contract_id,
        &investor,
        4_000,
        InvestmentStatus::Completed,
        102,
    );
    create_test_investment(
        &env,
        &contract_id,
        &investor,
        6_000,
        InvestmentStatus::Completed,
        103,
    );
    // Defaulted: 1 position
    create_test_investment(
        &env,
        &contract_id,
        &investor,
        1_500,
        InvestmentStatus::Defaulted,
        104,
    );
    // Refunded: 1 position
    create_test_investment(
        &env,
        &contract_id,
        &investor,
        2_500,
        InvestmentStatus::Refunded,
        105,
    );
    // Withdrawn: counted in total only
    create_test_investment(
        &env,
        &contract_id,
        &investor,
        7_000,
        InvestmentStatus::Withdrawn,
        106,
    );

    let summary = client.get_investor_portfolio_summary(&investor);

    assert_eq!(summary.active_principal, 8_000);
    assert_eq!(summary.completed_count, 2);
    assert_eq!(summary.completed_returns, 10_000);
    assert_eq!(summary.defaulted_count, 1);
    assert_eq!(summary.refunded_count, 1);
    assert_eq!(summary.total_positions, 7);
}

/// Aggregate reconciles with individually queried records:
/// sum of amounts for each status must match the summary fields.
#[test]
fn test_portfolio_summary_reconciles_with_individual_records() {
    let (env, client, contract_id) = setup();
    env.mock_all_auths();

    let investor = Address::generate(&env);
    let amounts: [(i128, InvestmentStatus, u8); 6] = [
        (1_000, InvestmentStatus::Active, 110),
        (2_000, InvestmentStatus::Active, 111),
        (3_000, InvestmentStatus::Completed, 112),
        (4_000, InvestmentStatus::Defaulted, 113),
        (5_000, InvestmentStatus::Refunded, 114),
        (6_000, InvestmentStatus::Withdrawn, 115),
    ];

    let mut expected_active: i128 = 0;
    let mut expected_completed_returns: i128 = 0;
    let mut expected_completed_count: u32 = 0;
    let mut expected_defaulted: u32 = 0;
    let mut expected_refunded: u32 = 0;

    for (amount, status, seed) in amounts.iter() {
        create_test_investment(&env, &contract_id, &investor, *amount, *status, *seed);
        match status {
            InvestmentStatus::Active => expected_active += amount,
            InvestmentStatus::Completed => {
                expected_completed_returns += amount;
                expected_completed_count += 1;
            }
            InvestmentStatus::Defaulted => expected_defaulted += 1,
            InvestmentStatus::Refunded => expected_refunded += 1,
            InvestmentStatus::Withdrawn => {}
        }
    }

    let summary = client.get_investor_portfolio_summary(&investor);

    assert_eq!(summary.active_principal, expected_active);
    assert_eq!(summary.completed_count, expected_completed_count);
    assert_eq!(summary.completed_returns, expected_completed_returns);
    assert_eq!(summary.defaulted_count, expected_defaulted);
    assert_eq!(summary.refunded_count, expected_refunded);
    assert_eq!(summary.total_positions, amounts.len() as u32);
}

/// Portfolio summary is isolated between investors.
#[test]
fn test_portfolio_summary_isolated_per_investor() {
    let (env, client, contract_id) = setup();
    env.mock_all_auths();

    let investor1 = Address::generate(&env);
    let investor2 = Address::generate(&env);

    create_test_investment(
        &env,
        &contract_id,
        &investor1,
        9_000,
        InvestmentStatus::Active,
        120,
    );
    create_test_investment(
        &env,
        &contract_id,
        &investor2,
        1_000,
        InvestmentStatus::Completed,
        121,
    );

    let s1 = client.get_investor_portfolio_summary(&investor1);
    assert_eq!(s1.active_principal, 9_000);
    assert_eq!(s1.total_positions, 1);

    let s2 = client.get_investor_portfolio_summary(&investor2);
    assert_eq!(s2.completed_returns, 1_000);
    assert_eq!(s2.completed_count, 1);
    assert_eq!(s2.total_positions, 1);
}

// ============================================================================
// get_investor_investments_cursor (#2456)
//
// Exercises the cursor-stable pagination entrypoint at the actual contract
// boundary (via `client`, not the bare `InvestmentQueries` impl functions),
// covering every case the issue's "Required validation" list names: empty,
// single-page, boundary, concurrent-insert, invalid-cursor, and
// large-result.
// ============================================================================

#[test]
fn test_cursored_investments_empty_investor_returns_generation_zero() {
    let (env, client, _) = setup();
    env.mock_all_auths();

    let investor = Address::generate(&env);
    let page = client.get_investor_investments_cursor(
        &investor, &None, &0u32, &10u32, &None,
    );

    assert_eq!(page.items.len(), 0);
    assert_eq!(page.total_count, 0);
    assert!(!page.has_more);
    assert_eq!(page.generation, 0);
}

#[test]
fn test_cursored_investments_single_page_returns_all_and_no_more() {
    let (env, client, contract_id) = setup();
    env.mock_all_auths();

    let investor = Address::generate(&env);
    for seed in 0u8..5 {
        create_test_investment(
            &env,
            &contract_id,
            &investor,
            1_000,
            InvestmentStatus::Active,
            seed,
        );
    }

    let page = client.get_investor_investments_cursor(
        &investor, &None, &0u32, &10u32, &None,
    );

    assert_eq!(page.items.len(), 5);
    assert_eq!(page.total_count, 5);
    assert!(!page.has_more);
    assert_eq!(page.generation, 5); // one bump per newly-appended investment
}

#[test]
fn test_cursored_investments_boundary_offset_at_and_past_total_count() {
    let (env, client, contract_id) = setup();
    env.mock_all_auths();

    let investor = Address::generate(&env);
    for seed in 0u8..3 {
        create_test_investment(
            &env,
            &contract_id,
            &investor,
            1_000,
            InvestmentStatus::Active,
            seed,
        );
    }

    // offset == total_count: empty page, not an error, has_more is false.
    let at_boundary = client.get_investor_investments_cursor(
        &investor, &None, &3u32, &10u32, &None,
    );
    assert_eq!(at_boundary.items.len(), 0);
    assert!(!at_boundary.has_more);
    assert_eq!(at_boundary.total_count, 3);

    // offset > total_count: still empty, still not an error (saturating, no panic).
    let past_boundary = client.get_investor_investments_cursor(
        &investor,
        &None,
        &u32::MAX,
        &10u32,
        &None,
    );
    assert_eq!(past_boundary.items.len(), 0);
    assert!(!past_boundary.has_more);
}

#[test]
fn test_cursored_investments_concurrent_insert_is_detected_as_unstable() {
    let (env, client, contract_id) = setup();
    env.mock_all_auths();

    let investor = Address::generate(&env);
    for seed in 0u8..3 {
        create_test_investment(
            &env,
            &contract_id,
            &investor,
            1_000,
            InvestmentStatus::Active,
            seed,
        );
    }

    // Page 1: capture the generation the caller observed.
    let page1 = client.get_investor_investments_cursor(
        &investor, &None, &0u32, &2u32, &None,
    );
    assert_eq!(page1.items.len(), 2);
    assert!(page1.has_more);
    let stale_generation = page1.generation;

    // Simulate a concurrent insert: a new investment lands for this investor
    // between the caller's page 1 and page 2 requests.
    create_test_investment(
        &env,
        &contract_id,
        &investor,
        5_000,
        InvestmentStatus::Active,
        99,
    );

    // Page 2, using the now-stale generation from page 1, must fail closed
    // rather than silently returning a page computed against the new,
    // longer list (which could skip or duplicate relative to page 1).
    let result = client.try_get_investor_investments_cursor(
        &investor,
        &None,
        &2u32,
        &2u32,
        &Some(stale_generation),
    );
    let err = result.err().expect("expected UnstableCursor error");
    let contract_error = err.expect("expected contract error");
    assert_eq!(contract_error, QuickLendXError::UnstableCursor);
}

#[test]
fn test_cursored_investments_retry_with_fresh_generation_succeeds() {
    let (env, client, contract_id) = setup();
    env.mock_all_auths();

    let investor = Address::generate(&env);
    for seed in 0u8..3 {
        create_test_investment(
            &env,
            &contract_id,
            &investor,
            1_000,
            InvestmentStatus::Active,
            seed,
        );
    }

    let page1 = client.get_investor_investments_cursor(
        &investor, &None, &0u32, &2u32, &None,
    );
    let stale_generation = page1.generation;

    create_test_investment(
        &env,
        &contract_id,
        &investor,
        5_000,
        InvestmentStatus::Active,
        99,
    );

    // The stale generation is rejected...
    let stale_result = client.try_get_investor_investments_cursor(
        &investor,
        &None,
        &2u32,
        &2u32,
        &Some(stale_generation),
    );
    assert!(stale_result.is_err());

    // ...but restarting pagination from offset 0 with the *current*
    // generation (from a fresh first-page call) succeeds and sees all 4
    // investments, including the one inserted concurrently.
    let restarted = client.get_investor_investments_cursor(
        &investor, &None, &0u32, &2u32, &None,
    );
    assert_eq!(restarted.total_count, 4);
    assert_ne!(restarted.generation, stale_generation);

    let page2 = client.get_investor_investments_cursor(
        &investor,
        &None,
        &2u32,
        &2u32,
        &Some(restarted.generation),
    );
    assert_eq!(page2.items.len(), 2);
    assert!(!page2.has_more);
}

#[test]
fn test_cursored_investments_invalid_cursor_generation_is_rejected() {
    let (env, client, contract_id) = setup();
    env.mock_all_auths();

    let investor = Address::generate(&env);
    create_test_investment(
        &env,
        &contract_id,
        &investor,
        1_000,
        InvestmentStatus::Active,
        0,
    );

    // No investment was ever added after the first, so generation 1 (or any
    // value other than the true current generation) is simply wrong — not
    // just stale — and must be rejected the same way a stale one is.
    let result = client.try_get_investor_investments_cursor(
        &investor,
        &None,
        &0u32,
        &10u32,
        &Some(999u64),
    );
    let err = result.err().expect("expected UnstableCursor error");
    let contract_error = err.expect("expected contract error");
    assert_eq!(contract_error, QuickLendXError::UnstableCursor);
}

#[test]
fn test_cursored_investments_large_result_capped_to_max_query_limit() {
    let (env, client, contract_id) = setup();
    env.mock_all_auths();

    let investor = Address::generate(&env);
    let count = crate::MAX_QUERY_LIMIT + 10;
    for seed in 0u8..(count as u8) {
        create_test_investment(
            &env,
            &contract_id,
            &investor,
            1_000,
            InvestmentStatus::Active,
            seed,
        );
    }

    // Ask for far more than MAX_QUERY_LIMIT in one page.
    let page = client.get_investor_investments_cursor(
        &investor,
        &None,
        &0u32,
        &(count * 10),
        &None,
    );

    assert_eq!(page.items.len() as u32, crate::MAX_QUERY_LIMIT);
    assert_eq!(page.total_count, count);
    assert!(page.has_more);
}

#[test]
fn test_cursored_investments_repeated_identical_calls_are_idempotent() {
    let (env, client, contract_id) = setup();
    env.mock_all_auths();

    let investor = Address::generate(&env);
    for seed in 0u8..5 {
        create_test_investment(
            &env,
            &contract_id,
            &investor,
            1_000,
            InvestmentStatus::Active,
            seed,
        );
    }

    let first = client.get_investor_investments_cursor(
        &investor, &None, &1u32, &2u32, &None,
    );
    // Repeating the exact same read (same offset/limit/generation) must be a
    // pure, side-effect-free operation: identical results every time, no
    // drift, no partial state accumulated by the read itself.
    for _ in 0..3 {
        let repeat = client.get_investor_investments_cursor(
            &investor,
            &None,
            &1u32,
            &2u32,
            &Some(first.generation),
        );
        assert_eq!(repeat, first);
    }
}
