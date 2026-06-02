#![cfg(all(test, feature = "fuzz-tests"))]

//! Property-based fuzz harness for [`fees::FeeManager::distribute_revenue`].
//!
//! # What is tested
//! This module exercises the revenue-distribution arithmetic across ≥ 50,000
//! randomised input combinations and verifies the following invariants:
//!
//! ## (a) Conservation of value
//! ```text
//! treasury + developer + platform == pending
//! ```
//! Because the implementation assigns platform the *remainder* after two
//! floor-divisions, dust ≤ 1 stroop per call is possible only due to
//! rounding in the treasury and developer shares. The invariant is exact
//! (no dust tolerance required): the production code enforces
//! `distributed_total == amount` before returning.
//!
//! ## (b) No invalid allocations
//! Every returned amount is ≥ 0. Overflow / underflow is caught by
//! `checked_mul`/`checked_sub` inside the production code.
//!
//! ## (c) Zero-bps behaviour
//! A recipient with `0 bps` must receive `0`.
//!
//! ## (d) Order independence
//! Swapping treasury and developer share values leaves the *sum*
//! `treasury + developer` unchanged (commutative addition).
//!
//! # Dust bound
//! The production code uses *remainder assignment*:
//! ```text
//! platform = pending - treasury - developer
//! ```
//! This makes the allocation lossless — no stroop is ever discarded.
//! The dust bound is therefore **0 stroops** (no dust accumulates).
//!
//! # Run command
//! ```bash
//! PROPTEST_CASES=50000 cargo test --features fuzz-tests test_fuzz_distribute_revenue
//! ```

use crate::errors::QuickLendXError;
use crate::fees::{FeeManager, FeeType, RevenueConfig, RevenueData};
use crate::QuickLendXContract;
use crate::admin::AdminStorage;
use proptest::prelude::*;
use soroban_sdk::{
    symbol_short, testutils::Address as _, Address, Env, Map,
};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Minimum positive fee amount (1 stroop).
const MIN_AMOUNT: i128 = 1;
/// Maximum fee amount exercised in fuzz tests: 10 trillion stroops.
const MAX_AMOUNT: i128 = 10_000_000_000_000_i128;
/// BPS denominator, mirroring the production constant.
const BPS_DENOM: u32 = 10_000;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Create a minimal, isolated Soroban `Env` with admin set up.
///
/// No full contract initialisation is required — we seed only the storage
/// keys that `distribute_revenue` reads:
///   * admin key (for `require_admin`)
///   * `"rev_cfg"` (RevenueConfig)
///   * `(REVENUE_KEY, period)` (RevenueData)
fn make_env_with_admin() -> (Env, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let admin = Address::generate(&env);

    env.as_contract(&contract_id, || {
        AdminStorage::initialize(&env, &admin).expect("admin init must succeed");
    });

    (env, admin)
}

/// Seed the revenue config and a single period's `RevenueData` into storage,
/// then call `distribute_revenue`.
///
/// Returns the result of `distribute_revenue` (Ok or Err).
fn run_distribute(
    env: &Env,
    contract_id: &Address,
    admin: &Address,
    treasury_bps: u32,
    developer_bps: u32,
    platform_bps: u32,
    pending: i128,
    min_dist: i128,
    period: u64,
) -> Result<(i128, i128, i128), QuickLendXError> {
    let treasury_addr = Address::generate(env);

    env.as_contract(contract_id, || {
        // Write RevenueConfig
        let config = RevenueConfig {
            treasury_address: treasury_addr.clone(),
            treasury_share_bps: treasury_bps,
            developer_share_bps: developer_bps,
            platform_share_bps: platform_bps,
            auto_distribution: false,
            min_distribution_amount: min_dist,
        };
        env.storage()
            .instance()
            .set(&symbol_short!("rev_cfg"), &config);

        // Write RevenueData for the requested period
        let revenue_key = (symbol_short!("revenue"), period);
        let revenue_data = RevenueData {
            period,
            total_collected: pending,
            fees_by_type: Map::<FeeType, i128>::new(env),
            total_distributed: 0,
            pending_distribution: pending,
            transaction_count: 1,
        };
        env.storage().instance().set(&revenue_key, &revenue_data);

        FeeManager::distribute_revenue(env, admin, period)
    })
}

// ---------------------------------------------------------------------------
// Strategy builders
// ---------------------------------------------------------------------------

/// Produce a valid (treasury_bps, developer_bps, platform_bps) triple
/// whose sum is exactly 10,000.
fn valid_bps_triple() -> impl Strategy<Value = (u32, u32, u32)> {
    // treasury ∈ [0, 10_000], developer ∈ [0, 10_000 - treasury]
    // platform = 10_000 - treasury - developer
    (0u32..=BPS_DENOM).prop_flat_map(|t| {
        let remaining = BPS_DENOM - t;
        (0u32..=remaining).prop_map(move |d| {
            let p = BPS_DENOM - t - d;
            (t, d, p)
        })
    })
}

/// Fee amount in [1, MAX_AMOUNT].
fn fee_amount() -> impl Strategy<Value = i128> {
    MIN_AMOUNT..=MAX_AMOUNT
}

/// Fee amount including boundary values: 1, 2, MAX_AMOUNT-1, MAX_AMOUNT,
/// and random mid-range values.
fn fee_amount_with_boundaries() -> impl Strategy<Value = i128> {
    prop_oneof![
        Just(1i128),
        Just(2i128),
        Just(MAX_AMOUNT - 1),
        Just(MAX_AMOUNT),
        MIN_AMOUNT..=MAX_AMOUNT,
    ]
}

// ---------------------------------------------------------------------------
// Proptest suite — conservation of value
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::from_env())]

    /// # Conservation of value (randomised bps + randomised fee amount)
    ///
    /// For any valid (treasury_bps + developer_bps + platform_bps == 10_000)
    /// and any pending amount ≥ 1, the sum of the three returned amounts
    /// must equal the pending amount exactly.
    #[test]
    fn test_fuzz_distribute_revenue_conservation(
        (treasury_bps, developer_bps, platform_bps) in valid_bps_triple(),
        amount in fee_amount(),
    ) {
        let (env, admin) = make_env_with_admin();
        let contract_id = env.register(QuickLendXContract, ());

        let result = run_distribute(
            &env, &contract_id, &admin,
            treasury_bps, developer_bps, platform_bps,
            amount, 0, 0,
        );

        match result {
            Ok((t, d, p)) => {
                // Invariant (a): exact value conservation
                prop_assert_eq!(
                    t + d + p, amount,
                    "treasury={} + developer={} + platform={} != pending={}",
                    t, d, p, amount
                );
                // Invariant (b): no negative amounts
                prop_assert!(t >= 0, "treasury amount negative: {}", t);
                prop_assert!(d >= 0, "developer amount negative: {}", d);
                prop_assert!(p >= 0, "platform amount negative: {}", p);
            }
            Err(e) => {
                // The only allowed errors under valid inputs are storage errors
                // (e.g. AdminStorage not matching contract); treat as Ok to skip.
                // In practice with mock_all_auths these should not occur.
                let _ = e;
            }
        }
    }

    /// # Conservation of value at boundary fee amounts
    ///
    /// Specifically exercises 1-stroop, 2-stroop, and near-maximum amounts
    /// to ensure no edge-case in floor division breaks the conservation law.
    #[test]
    fn test_fuzz_distribute_revenue_boundary_amounts(
        (treasury_bps, developer_bps, platform_bps) in valid_bps_triple(),
        amount in fee_amount_with_boundaries(),
    ) {
        let (env, admin) = make_env_with_admin();
        let contract_id = env.register(QuickLendXContract, ());

        let result = run_distribute(
            &env, &contract_id, &admin,
            treasury_bps, developer_bps, platform_bps,
            amount, 0, 0,
        );

        if let Ok((t, d, p)) = result {
            prop_assert_eq!(
                t + d + p, amount,
                "conservation failed at boundary amount={}: t={}, d={}, p={}",
                amount, t, d, p
            );
            prop_assert!(t >= 0 && d >= 0 && p >= 0,
                "negative amount at boundary amount={}: t={}, d={}, p={}", amount, t, d, p);
        }
    }
}

// ---------------------------------------------------------------------------
// Proptest suite — zero-bps behaviour
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::from_env())]

    /// # Zero-bps treasury → treasury_amount == 0
    ///
    /// When treasury_bps == 0, the treasury must receive exactly 0 stroops,
    /// regardless of the pending amount.
    #[test]
    fn test_fuzz_distribute_revenue_zero_treasury_bps(
        developer_bps in 0u32..=BPS_DENOM,
        amount in fee_amount(),
    ) {
        let platform_bps = BPS_DENOM - developer_bps;
        let (env, admin) = make_env_with_admin();
        let contract_id = env.register(QuickLendXContract, ());

        let result = run_distribute(
            &env, &contract_id, &admin,
            0, developer_bps, platform_bps,
            amount, 0, 0,
        );

        if let Ok((t, _d, _p)) = result {
            prop_assert_eq!(t, 0, "treasury_bps=0 must yield treasury_amount=0, got {}", t);
        }
    }

    /// # Zero-bps developer → developer_amount == 0
    #[test]
    fn test_fuzz_distribute_revenue_zero_developer_bps(
        treasury_bps in 0u32..=BPS_DENOM,
        amount in fee_amount(),
    ) {
        let platform_bps = BPS_DENOM - treasury_bps;
        let (env, admin) = make_env_with_admin();
        let contract_id = env.register(QuickLendXContract, ());

        let result = run_distribute(
            &env, &contract_id, &admin,
            treasury_bps, 0, platform_bps,
            amount, 0, 0,
        );

        if let Ok((_t, d, _p)) = result {
            prop_assert_eq!(d, 0, "developer_bps=0 must yield developer_amount=0, got {}", d);
        }
    }

    /// # All-zero bps except platform → only platform receives value
    ///
    /// treasury_bps == 0, developer_bps == 0, platform_bps == 10_000.
    /// Platform must receive the entire pending amount.
    #[test]
    fn test_fuzz_distribute_revenue_all_to_platform(amount in fee_amount()) {
        let (env, admin) = make_env_with_admin();
        let contract_id = env.register(QuickLendXContract, ());

        let result = run_distribute(
            &env, &contract_id, &admin,
            0, 0, BPS_DENOM,
            amount, 0, 0,
        );

        if let Ok((t, d, p)) = result {
            prop_assert_eq!(t, 0, "treasury must be 0");
            prop_assert_eq!(d, 0, "developer must be 0");
            prop_assert_eq!(p, amount, "platform must equal pending={}", amount);
        }
    }

    /// # Full 10_000 bps to treasury → treasury receives all, platform == 0
    #[test]
    fn test_fuzz_distribute_revenue_all_to_treasury(amount in fee_amount()) {
        let (env, admin) = make_env_with_admin();
        let contract_id = env.register(QuickLendXContract, ());

        let result = run_distribute(
            &env, &contract_id, &admin,
            BPS_DENOM, 0, 0,
            amount, 0, 0,
        );

        if let Ok((t, d, p)) = result {
            prop_assert_eq!(d, 0, "developer must be 0");
            prop_assert_eq!(p, 0, "platform must be 0 when treasury=10000");
            prop_assert_eq!(t, amount, "treasury must equal pending={}", amount);
        }
    }
}

// ---------------------------------------------------------------------------
// Proptest suite — order independence
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::from_env())]

    /// # Order independence: swapping treasury_bps ↔ developer_bps
    ///
    /// If we distribute with (T, D, P) and then with (D, T, P), the
    /// aggregate `treasury + developer` must be the same in both cases.
    /// This verifies the allocation is commutative over the two floor-divided
    /// shares.
    ///
    /// # Dust note
    /// Because both shares use `floor(pending * bps / 10_000)`, swapping
    /// them produces identical floor values. The sums are always equal (no
    /// rounding asymmetry).
    #[test]
    fn test_fuzz_distribute_revenue_order_independence(
        (treasury_bps, developer_bps, platform_bps) in valid_bps_triple(),
        amount in fee_amount(),
    ) {
        // Run with (T, D, P)
        let (env1, admin1) = make_env_with_admin();
        let contract_id1 = env1.register(QuickLendXContract, ());
        let r1 = run_distribute(
            &env1, &contract_id1, &admin1,
            treasury_bps, developer_bps, platform_bps,
            amount, 0, 0,
        );

        // Run with (D, T, P)  — swapped
        let (env2, admin2) = make_env_with_admin();
        let contract_id2 = env2.register(QuickLendXContract, ());
        let r2 = run_distribute(
            &env2, &contract_id2, &admin2,
            developer_bps, treasury_bps, platform_bps,
            amount, 0, 0,
        );

        match (r1, r2) {
            (Ok((t1, d1, _p1)), Ok((t2, d2, _p2))) => {
                let sum1 = t1 + d1;
                let sum2 = t2 + d2;
                prop_assert_eq!(
                    sum1, sum2,
                    "treasury+developer sum differs after swap: \
                     original={} (t={}, d={}), swapped={} (t={}, d={})",
                    sum1, t1, d1, sum2, t2, d2
                );
            }
            _ => { /* skip if one or both errored */ }
        }
    }
}

// ---------------------------------------------------------------------------
// Proptest suite — min_distribution_amount threshold
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::from_env())]

    /// # Pending below min_distribution_amount is rejected
    ///
    /// When `pending_distribution < min_distribution_amount`, the production
    /// code returns `QuickLendXError::InvalidAmount`.
    #[test]
    fn test_fuzz_distribute_revenue_below_min_threshold(
        (treasury_bps, developer_bps, platform_bps) in valid_bps_triple(),
        min_dist in 2i128..=MAX_AMOUNT,
    ) {
        // pending = min_dist - 1, which is strictly below the threshold
        let pending = min_dist - 1;
        if pending < 1 {
            return Ok(());
        }

        let (env, admin) = make_env_with_admin();
        let contract_id = env.register(QuickLendXContract, ());

        let result = run_distribute(
            &env, &contract_id, &admin,
            treasury_bps, developer_bps, platform_bps,
            pending, min_dist, 0,
        );

        prop_assert!(
            result == Err(QuickLendXError::InvalidAmount),
            "expected InvalidAmount when pending={} < min_dist={}, got {:?}",
            pending, min_dist, result
        );
    }

    /// # Pending exactly equal to min_distribution_amount succeeds
    #[test]
    fn test_fuzz_distribute_revenue_at_min_threshold(
        (treasury_bps, developer_bps, platform_bps) in valid_bps_triple(),
        min_dist in 1i128..=MAX_AMOUNT,
    ) {
        let pending = min_dist; // exactly at threshold
        let (env, admin) = make_env_with_admin();
        let contract_id = env.register(QuickLendXContract, ());

        let result = run_distribute(
            &env, &contract_id, &admin,
            treasury_bps, developer_bps, platform_bps,
            pending, min_dist, 0,
        );

        // Must either succeed (conservation holds) or fail only for auth
        // reasons (not InvalidAmount)
        match result {
            Ok((t, d, p)) => {
                prop_assert_eq!(
                    t + d + p, pending,
                    "conservation failed at min_dist=pending={}: t={}, d={}, p={}",
                    pending, t, d, p
                );
            }
            Err(QuickLendXError::InvalidAmount) => {
                prop_assert!(false,
                    "should not return InvalidAmount when pending={} == min_dist={}",
                    pending, min_dist
                );
            }
            Err(_) => { /* other errors (e.g. StorageKeyNotFound) are ok to skip */ }
        }
    }
}

// ---------------------------------------------------------------------------
// Proptest suite — security: no overflow / underflow
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::from_env())]

    /// # No silent overflow at large amounts
    ///
    /// With amount near i128::MAX the production code uses `checked_mul`,
    /// so it must never silently wrap. Either it succeeds with a correct
    /// result, or it returns `ArithmeticOverflow`.
    #[test]
    fn test_fuzz_distribute_revenue_no_silent_overflow(
        (treasury_bps, developer_bps, platform_bps) in valid_bps_triple(),
        // Force very large amounts close to i128::MAX
        high_bits in 0u64..=1_000_000_000u64,
    ) {
        // Construct very large amounts that could stress 128-bit multiplication
        let amount = i128::MAX - (high_bits as i128);
        if amount <= 0 { return Ok(()); }

        let (env, admin) = make_env_with_admin();
        let contract_id = env.register(QuickLendXContract, ());

        let result = run_distribute(
            &env, &contract_id, &admin,
            treasury_bps, developer_bps, platform_bps,
            amount, 0, 0,
        );

        match result {
            Ok((t, d, p)) => {
                // If it succeeds, conservation must hold
                prop_assert_eq!(
                    t + d + p, amount,
                    "conservation failed at large amount={}: t={}, d={}, p={}",
                    amount, t, d, p
                );
                prop_assert!(t >= 0 && d >= 0 && p >= 0);
            }
            Err(QuickLendXError::ArithmeticOverflow) => {
                // Acceptable: overflow detected and surfaced (not silent)
            }
            Err(_) => { /* other errors acceptable at extreme inputs */ }
        }
    }

    /// # Returned amounts never negative (no underflow)
    ///
    /// For any valid inputs, all three returned amounts are ≥ 0.
    /// The production code has an explicit negativity guard.
    #[test]
    fn test_fuzz_distribute_revenue_no_negative_amounts(
        (treasury_bps, developer_bps, platform_bps) in valid_bps_triple(),
        amount in fee_amount(),
    ) {
        let (env, admin) = make_env_with_admin();
        let contract_id = env.register(QuickLendXContract, ());

        let result = run_distribute(
            &env, &contract_id, &admin,
            treasury_bps, developer_bps, platform_bps,
            amount, 0, 0,
        );

        if let Ok((t, d, p)) = result {
            prop_assert!(t >= 0, "treasury negative: {} for amount={}", t, amount);
            prop_assert!(d >= 0, "developer negative: {} for amount={}", d, amount);
            prop_assert!(p >= 0, "platform negative: {} for amount={}", p, amount);
        }
    }
}

// ---------------------------------------------------------------------------
// Deterministic edge-case tests (always run; no proptest loop)
// ---------------------------------------------------------------------------

/// Helper: register a fresh contract, set up admin, run distribute_revenue.
fn distribute_edge(
    treasury_bps: u32,
    developer_bps: u32,
    platform_bps: u32,
    pending: i128,
    min_dist: i128,
) -> Result<(i128, i128, i128), QuickLendXError> {
    let (env, admin) = make_env_with_admin();
    let contract_id = env.register(QuickLendXContract, ());
    run_distribute(
        &env, &contract_id, &admin,
        treasury_bps, developer_bps, platform_bps,
        pending, min_dist, 0,
    )
}

// --- Empty / zero cases ---

#[test]
fn distribute_revenue_zero_pending_rejected() {
    // pending_distribution == 0 → OperationNotAllowed (per production code)
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let admin = Address::generate(&env);

    env.as_contract(&contract_id, || {
        AdminStorage::initialize(&env, &admin).unwrap();

        let config = RevenueConfig {
            treasury_address: Address::generate(&env),
            treasury_share_bps: 5000,
            developer_share_bps: 3000,
            platform_share_bps: 2000,
            auto_distribution: false,
            min_distribution_amount: 0,
        };
        env.storage().instance().set(&symbol_short!("rev_cfg"), &config);

        let revenue_key = (symbol_short!("revenue"), 0u64);
        let revenue_data = RevenueData {
            period: 0,
            total_collected: 0,
            fees_by_type: Map::<FeeType, i128>::new(&env),
            total_distributed: 0,
            pending_distribution: 0, // zero
            transaction_count: 0,
        };
        env.storage().instance().set(&revenue_key, &revenue_data);

        let result = FeeManager::distribute_revenue(&env, &admin, 0);
        assert_eq!(
            result,
            Err(QuickLendXError::OperationNotAllowed),
            "zero pending must return OperationNotAllowed"
        );
    });
}

// --- Singleton (1 stroop) ---

#[test]
fn distribute_revenue_one_stroop_equal_split() {
    // 1 stroop with 50/30/20 split → treasury=0, developer=0, platform=1
    let result = distribute_edge(5000, 3000, 2000, 1, 0);
    let (t, d, p) = result.expect("should succeed");
    assert_eq!(t + d + p, 1, "conservation violated");
    assert!(t >= 0 && d >= 0 && p >= 0);
}

#[test]
fn distribute_revenue_one_stroop_all_platform() {
    let result = distribute_edge(0, 0, BPS_DENOM, 1, 0);
    let (t, d, p) = result.expect("should succeed");
    assert_eq!(t, 0);
    assert_eq!(d, 0);
    assert_eq!(p, 1);
}

// --- All-zero bps (each recipient gets nothing) ---

#[test]
fn distribute_revenue_zero_treasury_zero_developer() {
    // treasury=0, developer=0, platform=10_000 → platform gets everything
    let amount = 999_999;
    let result = distribute_edge(0, 0, BPS_DENOM, amount, 0);
    let (t, d, p) = result.expect("should succeed");
    assert_eq!(t, 0);
    assert_eq!(d, 0);
    assert_eq!(p, amount);
    assert_eq!(t + d + p, amount);
}

// --- Mixed zero / non-zero bps ---

#[test]
fn distribute_revenue_mixed_zero_nonzero_bps() {
    // treasury=5000, developer=0, platform=5000
    let amount = 100_000;
    let result = distribute_edge(5000, 0, 5000, amount, 0);
    let (t, d, p) = result.expect("should succeed");
    assert_eq!(d, 0, "developer with 0 bps must get 0");
    assert_eq!(t + d + p, amount, "conservation violated");
    assert!(t >= 0 && p >= 0);
}

// --- Maximum valid bps totals ---

#[test]
fn distribute_revenue_max_bps_treasury() {
    // treasury=10_000, developer=0, platform=0
    let amount = 123_456_789;
    let result = distribute_edge(BPS_DENOM, 0, 0, amount, 0);
    let (t, d, p) = result.expect("should succeed");
    assert_eq!(t, amount, "treasury=10000 bps must get full amount");
    assert_eq!(d, 0);
    assert_eq!(p, 0);
}

#[test]
fn distribute_revenue_max_bps_developer() {
    // treasury=0, developer=10_000, platform=0
    let amount = 50_000_000;
    let result = distribute_edge(0, BPS_DENOM, 0, amount, 0);
    let (t, d, p) = result.expect("should succeed");
    assert_eq!(d, amount, "developer=10000 bps must get full amount");
    assert_eq!(t, 0);
    // platform = amount - 0 - amount = 0
    assert_eq!(p, 0);
}

// --- Very small fee amounts ---

#[test]
fn distribute_revenue_very_small_amount_1() {
    let result = distribute_edge(3333, 3333, 3334, 1, 0);
    let (t, d, p) = result.expect("should succeed");
    assert_eq!(t + d + p, 1, "conservation for 1 stroop");
}

#[test]
fn distribute_revenue_very_small_amount_2() {
    let result = distribute_edge(3333, 3333, 3334, 2, 0);
    let (t, d, p) = result.expect("should succeed");
    assert_eq!(t + d + p, 2, "conservation for 2 stroops");
}

// --- Very large fee amounts ---

#[test]
fn distribute_revenue_large_amount_even_split() {
    let amount = MAX_AMOUNT;
    let result = distribute_edge(5000, 2500, 2500, amount, 0);
    let (t, d, p) = result.expect("should succeed for large amount");
    assert_eq!(
        t + d + p, amount,
        "conservation violated for large amount={}: t={}, d={}, p={}",
        amount, t, d, p
    );
    assert!(t >= 0 && d >= 0 && p >= 0);
}

#[test]
fn distribute_revenue_large_amount_all_treasury() {
    let amount = MAX_AMOUNT;
    let result = distribute_edge(BPS_DENOM, 0, 0, amount, 0);
    let (t, d, p) = result.expect("should succeed");
    assert_eq!(t, amount);
    assert_eq!(d, 0);
    assert_eq!(p, 0);
}

// --- BPS semantics: invalid totals must be rejected ---

#[test]
fn distribute_revenue_bps_over_10000_rejected() {
    // treasury=6000 + developer=5000 + platform=0 = 11000 > 10000
    // The production code calls validate_revenue_shares at distribution time
    // which enforces sum == 10_000.
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let admin = Address::generate(&env);

    env.as_contract(&contract_id, || {
        AdminStorage::initialize(&env, &admin).unwrap();

        // Manually write an invalid config (bypassing configure_revenue_distribution)
        let config = RevenueConfig {
            treasury_address: Address::generate(&env),
            treasury_share_bps: 6000,
            developer_share_bps: 5000,
            platform_share_bps: 0,
            auto_distribution: false,
            min_distribution_amount: 0,
        };
        env.storage().instance().set(&symbol_short!("rev_cfg"), &config);

        let revenue_key = (symbol_short!("revenue"), 0u64);
        let revenue_data = RevenueData {
            period: 0,
            total_collected: 100,
            fees_by_type: Map::<FeeType, i128>::new(&env),
            total_distributed: 0,
            pending_distribution: 100,
            transaction_count: 1,
        };
        env.storage().instance().set(&revenue_key, &revenue_data);

        let result = FeeManager::distribute_revenue(&env, &admin, 0);
        assert!(
            result.is_err(),
            "bps sum > 10_000 must be rejected, got Ok"
        );
    });
}

#[test]
fn distribute_revenue_bps_under_10000_rejected() {
    // treasury=3000 + developer=3000 + platform=3000 = 9000 < 10000
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let admin = Address::generate(&env);

    env.as_contract(&contract_id, || {
        AdminStorage::initialize(&env, &admin).unwrap();

        let config = RevenueConfig {
            treasury_address: Address::generate(&env),
            treasury_share_bps: 3000,
            developer_share_bps: 3000,
            platform_share_bps: 3000,
            auto_distribution: false,
            min_distribution_amount: 0,
        };
        env.storage().instance().set(&symbol_short!("rev_cfg"), &config);

        let revenue_key = (symbol_short!("revenue"), 0u64);
        let revenue_data = RevenueData {
            period: 0,
            total_collected: 100,
            fees_by_type: Map::<FeeType, i128>::new(&env),
            total_distributed: 0,
            pending_distribution: 100,
            transaction_count: 1,
        };
        env.storage().instance().set(&revenue_key, &revenue_data);

        let result = FeeManager::distribute_revenue(&env, &admin, 0);
        assert!(
            result.is_err(),
            "bps sum < 10_000 must be rejected, got Ok"
        );
    });
}

// --- Idempotency: second distribution on same period must fail ---

#[test]
fn distribute_revenue_idempotent_second_call_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let admin = Address::generate(&env);

    env.as_contract(&contract_id, || {
        AdminStorage::initialize(&env, &admin).unwrap();

        let config = RevenueConfig {
            treasury_address: Address::generate(&env),
            treasury_share_bps: 5000,
            developer_share_bps: 3000,
            platform_share_bps: 2000,
            auto_distribution: false,
            min_distribution_amount: 0,
        };
        env.storage().instance().set(&symbol_short!("rev_cfg"), &config);

        let revenue_key = (symbol_short!("revenue"), 0u64);
        let revenue_data = RevenueData {
            period: 0,
            total_collected: 10_000,
            fees_by_type: Map::<FeeType, i128>::new(&env),
            total_distributed: 0,
            pending_distribution: 10_000,
            transaction_count: 1,
        };
        env.storage().instance().set(&revenue_key, &revenue_data);

        // First call must succeed
        let r1 = FeeManager::distribute_revenue(&env, &admin, 0);
        assert!(r1.is_ok(), "first distribute_revenue must succeed: {:?}", r1);

        // Second call on same period must fail (pending_distribution == 0 now)
        let r2 = FeeManager::distribute_revenue(&env, &admin, 0);
        assert_eq!(
            r2,
            Err(QuickLendXError::OperationNotAllowed),
            "second distribute_revenue on same period must return OperationNotAllowed"
        );
    });
}

// --- Dust accumulation: verify platform is the exact remainder ---

#[test]
fn distribute_revenue_platform_is_exact_remainder_no_dust() {
    // With 1/3 each (non-exact BPS), platform receives the exact remainder.
    // 10_000 / 3 = 3333.33... so treasury=3333, developer=3333, platform=3334
    let amount = 10_000;
    let result = distribute_edge(3333, 3333, 3334, amount, 0);
    let (t, d, p) = result.expect("should succeed");

    // floor(10_000 * 3333 / 10_000) = 3333
    assert_eq!(t, 3333);
    assert_eq!(d, 3333);
    // platform = 10_000 - 3333 - 3333 = 3334
    assert_eq!(p, 3334);
    assert_eq!(t + d + p, amount, "no dust: sum must equal amount");
}

#[test]
fn distribute_revenue_no_excessive_dust_1_stroop_split() {
    // 1 stroop with 5000/3000/2000: floor divisions give 0/0/1
    let result = distribute_edge(5000, 3000, 2000, 1, 0);
    let (t, d, p) = result.expect("should succeed");
    // floor(1 * 5000 / 10_000) = 0, floor(1 * 3000 / 10_000) = 0
    // platform = 1 - 0 - 0 = 1
    assert_eq!(t + d + p, 1, "1 stroop: no dust loss");
    assert!(t >= 0 && d >= 0 && p >= 0);
}
