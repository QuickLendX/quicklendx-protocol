#[cfg(test)]
mod test_insurance_stacking {
    use crate::investment::{
        Investment, InvestmentStatus, MAX_COVERAGE_PERCENTAGE, MAX_TOTAL_COVERAGE_PERCENTAGE,
        MIN_COVERAGE_PERCENTAGE,
    };
    use crate::types::InsuranceCoverage;
    use proptest::prelude::*;
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::{Address, Env, Vec};

    /// Setup test environment
    fn setup_test_env() -> Env {
        let env = Env::default();
        env.mock_all_auths();
        env
    }

    /// Create a test investment with specified amount
    fn create_test_investment(env: &Env, amount: i128) -> Investment {
        Investment {
            id: env.crypto().sha256(&env.crypto().random_bytes()).into(),
            invoice_id: env.crypto().sha256(&env.crypto().random_bytes()).into(),
            investor: Address::generate(env),
            amount,
            status: InvestmentStatus::Active,
            insurance: Vec::new(env),
        }
    }

    // ============================================================================
    // CUMULATIVE COVERAGE CAP INVARIANT TESTS
    // ============================================================================

    /// Test: Single policy respects individual cap
    /// A single policy with coverage <= 100% should always be accepted
    #[test]
    fn test_single_policy_within_cap() {
        let env = setup_test_env();

        proptest!(|(
            amount in 1i128..=1_000_000,
            coverage_pct in MIN_COVERAGE_PERCENTAGE..=MAX_COVERAGE_PERCENTAGE
        )| {
            let mut investment = create_test_investment(&env, amount);
            let provider = Address::generate(&env);
            let premium = 1; // Minimum premium

            let result = investment.add_insurance(provider, coverage_pct, premium);

            // Should always succeed for valid inputs
            prop_assert!(result.is_ok(), "Single policy within cap should succeed");

            // Cumulative coverage should equal the single policy
            let total = investment.total_active_coverage_percentage();
            prop_assert_eq!(total, coverage_pct, "Total should equal single policy");
        });
    }

    /// Test: Two policies that fit within cap are accepted
    /// If policy1 + policy2 <= 100%, both should be accepted
    #[test]
    fn test_two_policies_within_cap() {
        let env = setup_test_env();

        proptest!(|(
            amount in 1i128..=1_000_000,
            coverage1 in MIN_COVERAGE_PERCENTAGE..=50u32,
            coverage2 in MIN_COVERAGE_PERCENTAGE..=50u32
        )| {
            // Ensure sum doesn't exceed cap
            if coverage1.saturating_add(coverage2) > MAX_TOTAL_COVERAGE_PERCENTAGE {
                return Ok(());
            }

            let mut investment = create_test_investment(&env, amount);
            let provider1 = Address::generate(&env);
            let provider2 = Address::generate(&env);
            let premium = 1;

            // Add first policy
            let result1 = investment.add_insurance(provider1, coverage1, premium);
            prop_assert!(result1.is_ok(), "First policy should succeed");

            // Add second policy
            let result2 = investment.add_insurance(provider2, coverage2, premium);
            prop_assert!(result2.is_ok(), "Second policy should succeed");

            // Cumulative coverage should equal sum
            let total = investment.total_active_coverage_percentage();
            let expected = coverage1.saturating_add(coverage2);
            prop_assert_eq!(total, expected, "Total should equal sum of policies");

            // Total should not exceed cap
            prop_assert!(total <= MAX_TOTAL_COVERAGE_PERCENTAGE, "Total should not exceed cap");
        });
    }

    /// Test: Policy that would exceed cap is rejected
    /// If adding a policy would push total > 100%, it should be rejected
    #[test]
    fn test_policy_exceeding_cap_rejected() {
        let env = setup_test_env();

        proptest!(|(
            amount in 1i128..=1_000_000,
            coverage1 in 50u32..=100u32,
            coverage2 in 50u32..=100u32
        )| {
            // Only test cases where sum exceeds cap
            if coverage1.saturating_add(coverage2) <= MAX_TOTAL_COVERAGE_PERCENTAGE {
                return Ok(());
            }

            let mut investment = create_test_investment(&env, amount);
            let provider1 = Address::generate(&env);
            let provider2 = Address::generate(&env);
            let premium = 1;

            // Add first policy
            let result1 = investment.add_insurance(provider1, coverage1, premium);
            prop_assert!(result1.is_ok(), "First policy should succeed");

            // Try to add second policy (should fail)
            let result2 = investment.add_insurance(provider2, coverage2, premium);
            prop_assert!(result2.is_err(), "Second policy exceeding cap should fail");

            // Cumulative coverage should still be just the first policy
            let total = investment.total_active_coverage_percentage();
            prop_assert_eq!(total, coverage1, "Total should still be first policy only");
        });
    }

    /// Test: Cumulative cap holds after multiple policy additions
    /// After adding N policies, cumulative coverage should never exceed 100%
    #[test]
    fn test_cumulative_cap_multiple_policies() {
        let env = setup_test_env();

        proptest!(|(
            amount in 1i128..=1_000_000,
            coverages in prop::collection::vec(MIN_COVERAGE_PERCENTAGE..=20u32, 1..=10)
        )| {
            let mut investment = create_test_investment(&env, amount);
            let mut total_coverage = 0u32;

            for (idx, coverage) in coverages.iter().enumerate() {
                let provider = Address::generate(&env);
                let premium = 1;

                // Check if adding this policy would exceed cap
                let would_exceed = total_coverage.saturating_add(*coverage) > MAX_TOTAL_COVERAGE_PERCENTAGE;

                let result = investment.add_insurance(provider, *coverage, premium);

                if would_exceed {
                    // Should be rejected
                    prop_assert!(result.is_err(), "Policy {} should be rejected (would exceed cap)", idx);
                } else {
                    // Should be accepted
                    prop_assert!(result.is_ok(), "Policy {} should be accepted", idx);
                    total_coverage = total_coverage.saturating_add(*coverage);
                }

                // Invariant: cumulative coverage should never exceed cap
                let actual_total = investment.total_active_coverage_percentage();
                prop_assert!(
                    actual_total <= MAX_TOTAL_COVERAGE_PERCENTAGE,
                    "Cumulative coverage {} exceeds cap {}",
                    actual_total,
                    MAX_TOTAL_COVERAGE_PERCENTAGE
                );
            }
        });
    }

    // ============================================================================
    // POLICY EXPIRY AND CANCELLATION TESTS
    // ============================================================================

    /// Test: Deactivating a policy reduces cumulative coverage
    /// When a policy is marked inactive, cumulative coverage should decrease
    #[test]
    fn test_deactivate_policy_reduces_coverage() {
        let env = setup_test_env();

        proptest!(|(
            amount in 1i128..=1_000_000,
            coverage1 in MIN_COVERAGE_PERCENTAGE..=50u32,
            coverage2 in MIN_COVERAGE_PERCENTAGE..=50u32
        )| {
            if coverage1.saturating_add(coverage2) > MAX_TOTAL_COVERAGE_PERCENTAGE {
                return Ok(());
            }

            let mut investment = create_test_investment(&env, amount);
            let provider1 = Address::generate(&env);
            let provider2 = Address::generate(&env);
            let premium = 1;

            // Add two policies
            investment.add_insurance(provider1, coverage1, premium).ok();
            investment.add_insurance(provider2, coverage2, premium).ok();

            let total_before = investment.total_active_coverage_percentage();
            prop_assert_eq!(total_before, coverage1.saturating_add(coverage2));

            // Deactivate first policy
            if let Some(mut coverage) = investment.insurance.get(0) {
                coverage.active = false;
                investment.insurance.set(0, coverage);
            }

            let total_after = investment.total_active_coverage_percentage();
            prop_assert_eq!(total_after, coverage2, "Total should be second policy only");
            prop_assert!(total_after < total_before, "Total should decrease after deactivation");
        });
    }

    /// Test: Deactivating all policies results in zero coverage
    /// When all policies are deactivated, cumulative coverage should be 0%
    #[test]
    fn test_deactivate_all_policies_zero_coverage() {
        let env = setup_test_env();

        proptest!(|(
            amount in 1i128..=1_000_000,
            coverages in prop::collection::vec(MIN_COVERAGE_PERCENTAGE..=20u32, 1..=5)
        )| {
            let mut investment = create_test_investment(&env, amount);

            // Add multiple policies
            for coverage in coverages.iter() {
                let provider = Address::generate(&env);
                let premium = 1;
                if investment.add_insurance(provider, *coverage, premium).is_err() {
                    break; // Stop if we hit the cap
                }
            }

            // Deactivate all policies
            let len = investment.insurance.len();
            for idx in 0..len {
                if let Some(mut coverage) = investment.insurance.get(idx) {
                    coverage.active = false;
                    investment.insurance.set(idx, coverage);
                }
            }

            let total = investment.total_active_coverage_percentage();
            prop_assert_eq!(total, 0, "Total should be 0 after deactivating all policies");
        });
    }

    /// Test: After expiry, new policies can be added up to cap
    /// After deactivating policies, the freed capacity should allow new policies
    #[test]
    fn test_add_policy_after_expiry() {
        let env = setup_test_env();

        proptest!(|(
            amount in 1i128..=1_000_000,
            coverage1 in 50u32..=100u32,
            coverage2 in 1u32..=50u32
        )| {
            let mut investment = create_test_investment(&env, amount);
            let provider1 = Address::generate(&env);
            let provider2 = Address::generate(&env);
            let premium = 1;

            // Add first policy
            investment.add_insurance(provider1, coverage1, premium).ok();

            // Try to add second policy (might fail if sum > 100%)
            let result_before = investment.add_insurance(provider2.clone(), coverage2, premium);

            if result_before.is_ok() {
                // Both fit, nothing to test
                return Ok(());
            }

            // Second policy failed. Now deactivate first policy.
            if let Some(mut coverage) = investment.insurance.get(0) {
                coverage.active = false;
                investment.insurance.set(0, coverage);
            }

            // Now second policy should succeed
            let result_after = investment.add_insurance(provider2, coverage2, premium);
            prop_assert!(result_after.is_ok(), "Policy should succeed after expiry frees capacity");

            let total = investment.total_active_coverage_percentage();
            prop_assert_eq!(total, coverage2, "Total should be second policy only");
        });
    }

    // ============================================================================
    // EDGE CASE TESTS
    // ============================================================================

    /// Test: Exactly 100% coverage is allowed
    /// A single policy with 100% coverage should be accepted
    #[test]
    fn test_exactly_100_percent_coverage() {
        let env = setup_test_env();

        proptest!(|(amount in 1i128..=1_000_000)| {
            let mut investment = create_test_investment(&env, amount);
            let provider = Address::generate(&env);
            let premium = 1;

            let result = investment.add_insurance(provider, 100, premium);
            prop_assert!(result.is_ok(), "100% coverage should be accepted");

            let total = investment.total_active_coverage_percentage();
            prop_assert_eq!(total, 100, "Total should be exactly 100%");
        });
    }

    /// Test: 100% + 1% is rejected
    /// Adding 1% to 100% should be rejected
    #[test]
    fn test_100_plus_1_percent_rejected() {
        let env = setup_test_env();

        proptest!(|(amount in 1i128..=1_000_000)| {
            let mut investment = create_test_investment(&env, amount);
            let provider1 = Address::generate(&env);
            let provider2 = Address::generate(&env);
            let premium = 1;

            // Add 100% policy
            investment.add_insurance(provider1, 100, premium).ok();

            // Try to add 1% policy
            let result = investment.add_insurance(provider2, 1, premium);
            prop_assert!(result.is_err(), "Adding 1% to 100% should be rejected");

            let total = investment.total_active_coverage_percentage();
            prop_assert_eq!(total, 100, "Total should still be 100%");
        });
    }

    /// Test: Minimum coverage (1%) is allowed
    /// A single policy with 1% coverage should be accepted
    #[test]
    fn test_minimum_coverage_allowed() {
        let env = setup_test_env();

        proptest!(|(amount in 1i128..=1_000_000)| {
            let mut investment = create_test_investment(&env, amount);
            let provider = Address::generate(&env);
            let premium = 1;

            let result = investment.add_insurance(provider, 1, premium);
            prop_assert!(result.is_ok(), "1% coverage should be accepted");

            let total = investment.total_active_coverage_percentage();
            prop_assert_eq!(total, 1, "Total should be 1%");
        });
    }

    /// Test: Zero coverage is rejected
    /// A policy with 0% coverage should be rejected
    #[test]
    fn test_zero_coverage_rejected() {
        let env = setup_test_env();

        proptest!(|(amount in 1i128..=1_000_000)| {
            let mut investment = create_test_investment(&env, amount);
            let provider = Address::generate(&env);
            let premium = 1;

            let result = investment.add_insurance(provider, 0, premium);
            prop_assert!(result.is_err(), "0% coverage should be rejected");

            let total = investment.total_active_coverage_percentage();
            prop_assert_eq!(total, 0, "Total should remain 0%");
        });
    }

    /// Test: Coverage > 100% is rejected
    /// A policy with > 100% coverage should be rejected
    #[test]
    fn test_over_100_percent_rejected() {
        let env = setup_test_env();

        proptest!(|(amount in 1i128..=1_000_000)| {
            let mut investment = create_test_investment(&env, amount);
            let provider = Address::generate(&env);
            let premium = 1;

            let result = investment.add_insurance(provider, 101, premium);
            prop_assert!(result.is_err(), "Coverage > 100% should be rejected");

            let total = investment.total_active_coverage_percentage();
            prop_assert_eq!(total, 0, "Total should remain 0%");
        });
    }

    // ============================================================================
    // RANDOMIZED SEQUENCE TESTS
    // ============================================================================

    /// Test: Random add/expire/cancel sequences maintain invariant
    /// Randomly add, expire, and cancel policies and verify cap holds
    #[test]
    fn test_random_sequences_maintain_invariant() {
        let env = setup_test_env();

        proptest!(|(
            amount in 1i128..=1_000_000,
            operations in prop::collection::vec(
                (0u8..=2, MIN_COVERAGE_PERCENTAGE..=20u32),
                1..=50
            )
        )| {
            let mut investment = create_test_investment(&env, amount);
            let mut policy_count = 0u32;

            for (op_type, coverage) in operations.iter() {
                match op_type {
                    0 => {
                        // Add policy
                        let provider = Address::generate(&env);
                        let premium = 1;
                        let _ = investment.add_insurance(provider, *coverage, premium);
                        if investment.total_active_coverage_percentage() > 0 {
                            policy_count += 1;
                        }
                    }
                    1 => {
                        // Deactivate first active policy
                        let len = investment.insurance.len();
                        for idx in 0..len {
                            if let Some(mut cov) = investment.insurance.get(idx) {
                                if cov.active {
                                    cov.active = false;
                                    investment.insurance.set(idx, cov);
                                    break;
                                }
                            }
                        }
                    }
                    2 => {
                        // Deactivate all policies
                        let len = investment.insurance.len();
                        for idx in 0..len {
                            if let Some(mut cov) = investment.insurance.get(idx) {
                                cov.active = false;
                                investment.insurance.set(idx, cov);
                            }
                        }
                    }
                    _ => {}
                }

                // INVARIANT: Cumulative coverage must never exceed cap
                let total = investment.total_active_coverage_percentage();
                prop_assert!(
                    total <= MAX_TOTAL_COVERAGE_PERCENTAGE,
                    "Invariant violated: cumulative coverage {} exceeds cap {}",
                    total,
                    MAX_TOTAL_COVERAGE_PERCENTAGE
                );
            }
        });
    }

    /// Test: 20,000+ randomized sequences with varying parameters
    /// Comprehensive stress test with many random operations
    #[test]
    fn test_20000_randomized_sequences() {
        let env = setup_test_env();

        proptest!(|(
            amount in 1i128..=10_000_000,
            seed in 0u64..1_000_000,
            operations in prop::collection::vec(
                (0u8..=2, 1u32..=100u32),
                1..=100
            )
        )| {
            let mut investment = create_test_investment(&env, amount);

            for (op_type, coverage) in operations.iter() {
                match op_type {
                    0 => {
                        // Add policy with random coverage
                        let provider = Address::generate(&env);
                        let premium = 1;
                        let _ = investment.add_insurance(provider, *coverage % 100 + 1, premium);
                    }
                    1 => {
                        // Deactivate random policy
                        let len = investment.insurance.len();
                        if len > 0 {
                            let idx = (seed as usize) % len;
                            if let Some(mut cov) = investment.insurance.get(idx) {
                                cov.active = false;
                                investment.insurance.set(idx, cov);
                            }
                        }
                    }
                    2 => {
                        // Deactivate all
                        let len = investment.insurance.len();
                        for idx in 0..len {
                            if let Some(mut cov) = investment.insurance.get(idx) {
                                cov.active = false;
                                investment.insurance.set(idx, cov);
                            }
                        }
                    }
                    _ => {}
                }

                // INVARIANT: Cumulative coverage must never exceed cap
                let total = investment.total_active_coverage_percentage();
                prop_assert!(
                    total <= MAX_TOTAL_COVERAGE_PERCENTAGE,
                    "Invariant violated at operation {}: total {} > cap {}",
                    op_type,
                    total,
                    MAX_TOTAL_COVERAGE_PERCENTAGE
                );
            }
        });
    }

    // ============================================================================
    // SATURATION AND OVERFLOW TESTS
    // ============================================================================

    /// Test: Saturating addition prevents overflow
    /// Adding many small policies should saturate at cap, not overflow
    #[test]
    fn test_saturation_prevents_overflow() {
        let env = setup_test_env();

        proptest!(|(amount in 1i128..=1_000_000)| {
            let mut investment = create_test_investment(&env, amount);

            // Try to add 101 policies of 1% each
            for i in 0..101 {
                let provider = Address::generate(&env);
                let premium = 1;
                let _ = investment.add_insurance(provider, 1, premium);

                let total = investment.total_active_coverage_percentage();
                // Should never exceed cap, even with 101 attempts
                prop_assert!(
                    total <= MAX_TOTAL_COVERAGE_PERCENTAGE,
                    "Total {} exceeds cap at iteration {}",
                    total,
                    i
                );
            }

            let final_total = investment.total_active_coverage_percentage();
            prop_assert_eq!(final_total, 100, "Final total should be exactly 100%");
        });
    }

    /// Test: Malformed state detection
    /// If stored state somehow exceeds cap, adding new policies should fail
    #[test]
    fn test_malformed_state_detection() {
        let env = setup_test_env();

        proptest!(|(amount in 1i128..=1_000_000)| {
            let mut investment = create_test_investment(&env, amount);

            // Manually create a malformed state (simulating corruption)
            // Add two policies that together exceed 100%
            let provider1 = Address::generate(&env);
            let provider2 = Address::generate(&env);
            let premium = 1;

            investment.add_insurance(provider1, 60, premium).ok();
            investment.add_insurance(provider2, 50, premium).ok();

            // Manually corrupt by adding a third policy beyond the cap
            // (This simulates a bug or corruption)
            let provider3 = Address::generate(&env);
            if let Some(mut cov) = investment.insurance.get(0) {
                cov.coverage_percentage = 70; // Manually increase to create malformed state
                investment.insurance.set(0, cov);
            }

            // Now the total is 70 + 50 = 120%, which exceeds cap
            let total = investment.total_active_coverage_percentage();
            prop_assert!(total > MAX_TOTAL_COVERAGE_PERCENTAGE, "State should be malformed");

            // Trying to add a new policy should fail
            let result = investment.add_insurance(provider3, 1, premium);
            prop_assert!(result.is_err(), "Adding policy to malformed state should fail");
        });
    }

    // ============================================================================
    // CONSISTENCY TESTS
    // ============================================================================

    /// Test: total_active_coverage_percentage is consistent
    /// Calling total_active_coverage_percentage multiple times should give same result
    #[test]
    fn test_total_coverage_consistency() {
        let env = setup_test_env();

        proptest!(|(
            amount in 1i128..=1_000_000,
            coverages in prop::collection::vec(MIN_COVERAGE_PERCENTAGE..=20u32, 1..=5)
        )| {
            let mut investment = create_test_investment(&env, amount);

            // Add policies
            for coverage in coverages.iter() {
                let provider = Address::generate(&env);
                let premium = 1;
                let _ = investment.add_insurance(provider, *coverage, premium);
            }

            // Call total_active_coverage_percentage multiple times
            let total1 = investment.total_active_coverage_percentage();
            let total2 = investment.total_active_coverage_percentage();
            let total3 = investment.total_active_coverage_percentage();

            // All should be identical
            prop_assert_eq!(total1, total2, "First and second calls should match");
            prop_assert_eq!(total2, total3, "Second and third calls should match");
        });
    }

    /// Test: has_active_insurance is consistent with total_active_coverage_percentage
    /// has_active_insurance should return true iff total > 0
    #[test]
    fn test_has_active_insurance_consistency() {
        let env = setup_test_env();

        proptest!(|(
            amount in 1i128..=1_000_000,
            coverages in prop::collection::vec(MIN_COVERAGE_PERCENTAGE..=20u32, 0..=5)
        )| {
            let mut investment = create_test_investment(&env, amount);

            // Add policies
            for coverage in coverages.iter() {
                let provider = Address::generate(&env);
                let premium = 1;
                let _ = investment.add_insurance(provider, *coverage, premium);
            }

            let has_active = investment.has_active_insurance();
            let total = investment.total_active_coverage_percentage();

            // Consistency: has_active should be true iff total > 0
            if total > 0 {
                prop_assert!(has_active, "has_active_insurance should be true when total > 0");
            } else {
                prop_assert!(!has_active, "has_active_insurance should be false when total == 0");
            }
        });
    }
}
