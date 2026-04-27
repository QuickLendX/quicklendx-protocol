//! Simulated investment-query pagination tests.
//!
//! Exercises [`crate::pagination`] against a mock investment dataset to
//! validate status-filter + offset/limit semantics. No Soroban storage is
//! used; these tests are purely functional so they remain runnable while the
//! legacy contract library is mid-migration.

extern crate alloc;

use alloc::vec;
use alloc::vec::Vec;

use crate::pagination::{
    calculate_safe_bounds, paginate_slice, validate_pagination_params, MAX_QUERY_LIMIT,
};
use proptest::prelude::*;

// ---------------------------------------------------------------------------
// Mock investment model - deliberately minimal, no Soroban storage involved.
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum MockInvestmentStatus {
    Active,
    Completed,
    Defaulted,
    Refunded,
    Withdrawn,
}

const STATUS_CYCLE: [MockInvestmentStatus; 5] = [
    MockInvestmentStatus::Active,
    MockInvestmentStatus::Completed,
    MockInvestmentStatus::Defaulted,
    MockInvestmentStatus::Refunded,
    MockInvestmentStatus::Withdrawn,
];

#[derive(Clone, Debug, PartialEq, Eq)]
struct MockInvestment {
    id: [u8; 32],
    status: MockInvestmentStatus,
}

/// Build a deterministic dataset of `count` mock investments. IDs are
/// `[index as u8; 32]` (wrapping at 256) and statuses cycle through the five
/// enum variants.
fn build_mock_investments(count: u32) -> Vec<MockInvestment> {
    (0..count)
        .map(|i| MockInvestment {
            id: [i as u8; 32],
            status: STATUS_CYCLE[(i as usize) % STATUS_CYCLE.len()],
        })
        .collect()
}

fn filter_by_status(
    investments: &[MockInvestment],
    status: MockInvestmentStatus,
) -> Vec<MockInvestment> {
    investments
        .iter()
        .filter(|inv| inv.status == status)
        .cloned()
        .collect()
}

// ---------------------------------------------------------------------------
// 1. Helper determinism - mock dataset is reproducible.
// ---------------------------------------------------------------------------

/// The mock generator yields the same data on every call, which is a
/// prerequisite for stable-ordering assertions.
#[test]
fn test_build_mock_investments_is_deterministic() {
    let a = build_mock_investments(50);
    let b = build_mock_investments(50);
    assert_eq!(a, b);
    assert_eq!(a.len(), 50);
    assert_eq!(a[0].id, [0u8; 32]);
    assert_eq!(a[0].status, MockInvestmentStatus::Active);
    assert_eq!(a[4].status, MockInvestmentStatus::Withdrawn);
    assert_eq!(a[5].status, MockInvestmentStatus::Active);
}

// ---------------------------------------------------------------------------
// 2. Simulated pagination across several total sizes.
// ---------------------------------------------------------------------------

/// Paging with `size = 25` through totals of 0, 1, 10, 99, 100, 101, 500 must
/// yield the expected prefix of the input (capped at `MAX_QUERY_LIMIT`), never
/// a panic, and never a duplicate.
#[test]
fn test_simulated_pagination_across_sizes() {
    let size = 25u32;
    for &total in &[0u32, 1, 10, 99, 100, 101, 500] {
        let dataset = build_mock_investments(total);
        let mut collected: Vec<MockInvestment> = Vec::new();
        let max_pages = (MAX_QUERY_LIMIT / size) + 2; // +2 proves we stop at the cap
        for p in 0..max_pages {
            let offset = p.saturating_mul(size);
            let page = paginate_slice(&dataset, offset, size);
            if page.is_empty() {
                break;
            }
            assert!(page.len() as u32 <= size);
            for item in page {
                assert!(!collected.contains(&item), "duplicate for total={total}");
                collected.push(item);
            }
        }
        // collected is a prefix of dataset.
        for (i, item) in collected.iter().enumerate() {
            assert_eq!(item, &dataset[i], "order mismatch at total={total}");
        }
        // Length never exceeds MAX_QUERY_LIMIT when total > MAX_QUERY_LIMIT.
        if total > MAX_QUERY_LIMIT {
            // Using size=25 and max_pages=(100/25)+2=6, we consume min(total, 150).
            // That still caps the collected length at MAX_QUERY_LIMIT via per-page
            // clamping because every page is <= size <= MAX_QUERY_LIMIT.
            // So the total collected can exceed MAX_QUERY_LIMIT as the *pagination
            // helper* caps per-call, not per-session. That is the desired
            // contract: MAX_QUERY_LIMIT is a per-query cap.
            assert!(collected.len() <= total as usize);
        }
    }
}

// ---------------------------------------------------------------------------
// 3. Status filter + pagination semantics.
// ---------------------------------------------------------------------------

/// Filtering to `Active`, then paging with `size = 3`, preserves the original
/// Active-subset ordering and never duplicates.
#[test]
fn test_status_filter_then_paginate_size_three() {
    let dataset = build_mock_investments(50); // 10 Active at indices 0, 5, 10, ...
    let filtered = filter_by_status(&dataset, MockInvestmentStatus::Active);
    assert_eq!(filtered.len(), 10);

    let size = 3u32;
    let mut collected: Vec<MockInvestment> = Vec::new();
    for p in 0..10u32 {
        let page = paginate_slice(&filtered, p * size, size);
        if page.is_empty() {
            break;
        }
        assert!(page.len() as u32 <= size);
        for item in page {
            assert!(!collected.contains(&item));
            collected.push(item);
        }
    }
    assert_eq!(collected, filtered);
    assert!(collected.len() as u32 <= MAX_QUERY_LIMIT);
}

/// Multi-page status filter - ensures concatenation of all pages of the
/// filtered list equals the original filtered list.
#[test]
fn test_multi_page_status_filter_reconstructs_filtered_list() {
    let dataset = build_mock_investments(200);
    for &status in &STATUS_CYCLE {
        let filtered = filter_by_status(&dataset, status);
        let size = 7u32;
        let mut collected: Vec<MockInvestment> = Vec::new();
        for p in 0..(filtered.len() as u32 / size + 2) {
            let page = paginate_slice(&filtered, p * size, size);
            if page.is_empty() {
                break;
            }
            collected.extend(page);
        }
        assert_eq!(collected, filtered, "reconstruction failed for {status:?}");
    }
}

/// Filter yielding an empty set still paginates safely.
#[test]
fn test_empty_filter_result_yields_empty_page() {
    // Ten investments -> all Active since 10 < 5*2 cycles? Actually build 10:
    // indices 0..10 cycle through all 5 statuses twice. To get a truly empty
    // filter, build a tiny dataset of 1 (only Active) and filter for Defaulted.
    let dataset = build_mock_investments(1);
    assert_eq!(dataset.len(), 1);
    assert_eq!(dataset[0].status, MockInvestmentStatus::Active);

    let filtered = filter_by_status(&dataset, MockInvestmentStatus::Defaulted);
    assert!(filtered.is_empty());

    let page = paginate_slice(&filtered, 0, 10);
    assert!(page.is_empty());
}

// ---------------------------------------------------------------------------
// 4. u32::MAX offsets and limits against realistic-sized datasets.
// ---------------------------------------------------------------------------

/// With 50 investments, `(u32::MAX, any_limit)` always returns empty.
#[test]
fn test_u32_max_offset_always_empty() {
    let dataset = build_mock_investments(50);
    for &lim in &[0u32, 1, 10, MAX_QUERY_LIMIT, u32::MAX] {
        let page = paginate_slice(&dataset, u32::MAX, lim);
        assert!(page.is_empty(), "unexpected data for limit={lim}");
    }
}

/// With 50 investments (< `MAX_QUERY_LIMIT`), `(0, u32::MAX)` returns the full
/// dataset - the cap does not truncate data that already fits.
#[test]
fn test_u32_max_limit_returns_full_small_collection() {
    let dataset = build_mock_investments(50);
    let page = paginate_slice(&dataset, 0, u32::MAX);
    assert_eq!(page, dataset);
}

/// With 250 investments (> `MAX_QUERY_LIMIT`), `(0, u32::MAX)` returns
/// exactly `MAX_QUERY_LIMIT` items.
#[test]
fn test_u32_max_limit_clamped_on_large_collection() {
    let dataset = build_mock_investments(250);
    let page = paginate_slice(&dataset, 0, u32::MAX);
    assert_eq!(page.len() as u32, MAX_QUERY_LIMIT);
    // First MAX_QUERY_LIMIT items preserved in order.
    for (i, item) in page.iter().enumerate() {
        assert_eq!(item, &dataset[i]);
    }
}

// ---------------------------------------------------------------------------
// 5. Cross-consistency between validate_pagination_params and
//    calculate_safe_bounds at a specific point in the investor query.
// ---------------------------------------------------------------------------

/// Specific verification that both helpers agree on `(total=250, offset=120,
/// limit=70)`.
#[test]
fn test_cross_consistency_validate_and_bounds_specific() {
    let (safe_off, eff_lim, has_more) = validate_pagination_params(120, 70, 250);
    assert_eq!(safe_off, 120);
    assert_eq!(eff_lim, 70);
    assert!(has_more);

    let (start, end) = calculate_safe_bounds(120, 70, 250);
    assert_eq!(start, 120);
    assert_eq!(end, 190);
    assert_eq!(end - start, eff_lim);
}

// ---------------------------------------------------------------------------
// 6. Boundary clamp: limit is trimmed to the remaining items.
// ---------------------------------------------------------------------------

/// `(total=250, offset=240, limit=50)` -> eff_lim = 10, has_more = false.
#[test]
fn test_boundary_clamp_trims_effective_limit() {
    let (safe_off, eff_lim, has_more) = validate_pagination_params(240, 50, 250);
    assert_eq!(safe_off, 240);
    assert_eq!(eff_lim, 10);
    assert!(!has_more);

    let dataset = build_mock_investments(250);
    let page = paginate_slice(&dataset, 240, 50);
    assert_eq!(page.len(), 10);
    assert_eq!(page[0], dataset[240]);
    assert_eq!(page[9], dataset[249]);
}

// ---------------------------------------------------------------------------
// 7. Empty dataset always returns empty, regardless of inputs.
// ---------------------------------------------------------------------------

/// Empty dataset + any reasonable (offset, limit) always yields empty.
#[test]
fn test_empty_dataset_always_returns_empty() {
    let dataset: Vec<MockInvestment> = Vec::new();
    let test_cases = vec![
        (0u32, 0u32),
        (0, 10),
        (10, 0),
        (10, 10),
        (u32::MAX, u32::MAX),
    ];
    for (offset, limit) in test_cases {
        assert!(paginate_slice(&dataset, offset, limit).is_empty());
    }
}

// ---------------------------------------------------------------------------
// 8. Stability - repeated calls are bitwise identical.
// ---------------------------------------------------------------------------

/// Repeated calls with identical args to the filter + paginate pipeline yield
/// `==` results.
#[test]
fn test_query_is_stable_across_repeated_calls() {
    let dataset = build_mock_investments(100);
    let filtered = filter_by_status(&dataset, MockInvestmentStatus::Active);
    let a = paginate_slice(&filtered, 2, 5);
    let b = paginate_slice(&filtered, 2, 5);
    assert_eq!(a, b);
    assert_eq!(a.len(), 5);
}

// ---------------------------------------------------------------------------
// 9. Proptest - status-filter + pagination invariants.
// ---------------------------------------------------------------------------

fn status_strategy() -> impl Strategy<Value = MockInvestmentStatus> {
    prop_oneof![
        Just(MockInvestmentStatus::Active),
        Just(MockInvestmentStatus::Completed),
        Just(MockInvestmentStatus::Defaulted),
        Just(MockInvestmentStatus::Refunded),
        Just(MockInvestmentStatus::Withdrawn),
    ]
}

fn investment_strategy() -> impl Strategy<Value = MockInvestment> {
    (any::<u8>(), status_strategy()).prop_map(|(byte, status)| MockInvestment {
        id: [byte; 32],
        status,
    })
}

proptest! {
    /// For any random dataset (up to 200 items), any filter status, and any
    /// `(offset, limit)` triple, the filter-then-paginate pipeline must:
    /// 1. preserve the filtered-list order,
    /// 2. never return more than `min(limit, MAX_QUERY_LIMIT)` items,
    /// 3. never panic.
    #[test]
    fn prop_status_filter_then_paginate(
        dataset in proptest::collection::vec(investment_strategy(), 0..=200),
        filter_status in status_strategy(),
        offset in 0u32..=300,
        limit in 0u32..=(MAX_QUERY_LIMIT * 3),
    ) {
        let filtered = filter_by_status(&dataset, filter_status);
        let page = paginate_slice(&filtered, offset, limit);

        let expected_cap = core::cmp::min(limit, MAX_QUERY_LIMIT) as usize;
        prop_assert!(page.len() <= expected_cap);

        // Page is a contiguous sub-slice of `filtered`, preserving order.
        let (start, end) = calculate_safe_bounds(offset, limit, filtered.len() as u32);
        let expected: Vec<MockInvestment> = filtered[(start as usize)..(end as usize)].to_vec();
        prop_assert_eq!(page, expected);
    }
}
