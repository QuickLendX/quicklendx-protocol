//! Query consistency tests: pagination invariants and escrow query surfaces.
//!
//! Section 1 (lines below) covers pagination.
//! Section 2 covers escrow query consistency across lifecycle transitions
//! (Issue #831): `get_escrow_details` / `get_escrow_status` must agree at
//! every state, immutable fields must be stable, and missing-record errors
//! must be deterministic.
//!
//! Pagination section:
//!
//! Covers the invariants demanded by the `query-pagination-consistency-tests`
//! issue: `limit=0` returns empty, offset beyond length returns empty, limits
//! are clamped to [`MAX_QUERY_LIMIT`], ordering is stable across repeated
//! calls and consecutive pages, and `u32::MAX` edge cases never panic.
//!
//! The tests use only the self-contained `pagination` module plus `alloc`
//! types - no Soroban storage, no contract client, no legacy modules.

extern crate alloc;

use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use crate::pagination::{
    calculate_safe_bounds, cap_query_limit, paginate_slice, validate_pagination_params,
    MAX_QUERY_LIMIT,
};
use proptest::prelude::*;

// ---------------------------------------------------------------------------
// Helper: newtype around `String` to confirm `paginate_slice` only requires
// `T: Clone`.
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
struct NamedId(String);

fn build_u64_items(n: u32) -> Vec<u64> {
    (0u64..n as u64).collect()
}

fn build_bytes_items(n: u32) -> Vec<[u8; 32]> {
    (0..n).map(|i| [i as u8; 32]).collect()
}

fn build_named_items(n: u32) -> Vec<NamedId> {
    (0..n).map(|i| NamedId(i.to_string())).collect()
}

// ---------------------------------------------------------------------------
// 1. limit = 0 on non-empty collection returns empty
// ---------------------------------------------------------------------------

/// `limit = 0` on a non-empty collection returns an empty Vec.
#[test]
fn test_paginate_slice_limit_zero_returns_empty() {
    let items = build_u64_items(50);
    let page = paginate_slice(&items, 0, 0);
    assert!(page.is_empty(), "limit=0 must return empty");
}

/// `validate_pagination_params` with `limit = 0` reports zero effective limit
/// and `has_more = true` whenever the collection is non-empty.
#[test]
fn test_validate_limit_zero_yields_zero_effective_limit() {
    let (safe_off, eff_lim, has_more) = validate_pagination_params(0, 0, 50);
    assert_eq!(safe_off, 0);
    assert_eq!(eff_lim, 0);
    assert!(has_more, "zero-size page still leaves items past the cursor");
}

/// `limit = 0` at the end of the collection: `has_more` must be `false`.
#[test]
fn test_validate_limit_zero_at_end_of_collection() {
    let (safe_off, eff_lim, has_more) = validate_pagination_params(50, 0, 50);
    assert_eq!(safe_off, 50);
    assert_eq!(eff_lim, 0);
    assert!(!has_more);
}

// ---------------------------------------------------------------------------
// 2. offset >= total returns empty
// ---------------------------------------------------------------------------

/// `offset == total` on a non-empty collection returns an empty page.
#[test]
fn test_paginate_slice_offset_equals_total_returns_empty() {
    let items = build_u64_items(10);
    let page = paginate_slice(&items, 10, 5);
    assert!(page.is_empty());
}

/// `offset > total` returns an empty page.
#[test]
fn test_paginate_slice_offset_past_total_returns_empty() {
    let items = build_u64_items(10);
    let page = paginate_slice(&items, 11, 5);
    assert!(page.is_empty());
}

/// `offset = u32::MAX` with a small collection returns an empty page and
/// does not panic.
#[test]
fn test_paginate_slice_u32_max_offset_returns_empty() {
    let items = build_u64_items(5);
    let page = paginate_slice(&items, u32::MAX, 10);
    assert!(page.is_empty());
}

// ---------------------------------------------------------------------------
// 3. limit > MAX_QUERY_LIMIT is clamped
// ---------------------------------------------------------------------------

/// `cap_query_limit` is a no-op at or below `MAX_QUERY_LIMIT`.
#[test]
fn test_cap_query_limit_below_max_is_identity() {
    assert_eq!(cap_query_limit(0), 0);
    assert_eq!(cap_query_limit(1), 1);
    assert_eq!(cap_query_limit(50), 50);
    assert_eq!(cap_query_limit(MAX_QUERY_LIMIT), MAX_QUERY_LIMIT);
}

/// `cap_query_limit` clamps anything above `MAX_QUERY_LIMIT`.
#[test]
fn test_cap_query_limit_above_max_is_clamped() {
    assert_eq!(cap_query_limit(MAX_QUERY_LIMIT + 1), MAX_QUERY_LIMIT);
    assert_eq!(cap_query_limit(500), MAX_QUERY_LIMIT);
    assert_eq!(cap_query_limit(u32::MAX), MAX_QUERY_LIMIT);
}

/// `paginate_slice` clamps `limit = 101` on a collection larger than
/// `MAX_QUERY_LIMIT` to exactly `MAX_QUERY_LIMIT`.
#[test]
fn test_paginate_slice_clamps_limit_101() {
    let items = build_u64_items(MAX_QUERY_LIMIT + 50);
    let page = paginate_slice(&items, 0, MAX_QUERY_LIMIT + 1);
    assert_eq!(page.len() as u32, MAX_QUERY_LIMIT);
}

/// `paginate_slice` clamps `limit = 500` on a collection larger than
/// `MAX_QUERY_LIMIT`.
#[test]
fn test_paginate_slice_clamps_limit_500() {
    let items = build_u64_items(MAX_QUERY_LIMIT + 50);
    let page = paginate_slice(&items, 0, 500);
    assert_eq!(page.len() as u32, MAX_QUERY_LIMIT);
}

/// `paginate_slice` clamps `limit = u32::MAX` on a collection larger than
/// `MAX_QUERY_LIMIT`.
#[test]
fn test_paginate_slice_clamps_limit_u32_max_large_collection() {
    let items = build_u64_items(MAX_QUERY_LIMIT + 50);
    let page = paginate_slice(&items, 0, u32::MAX);
    assert_eq!(page.len() as u32, MAX_QUERY_LIMIT);
}

/// `paginate_slice` with `limit = u32::MAX` on a small collection returns
/// exactly the whole collection (clamped by available, not by the cap).
#[test]
fn test_paginate_slice_limit_u32_max_small_collection() {
    let items = build_u64_items(7);
    let page = paginate_slice(&items, 0, u32::MAX);
    assert_eq!(page, items);
}

// ---------------------------------------------------------------------------
// 4. Stable ordering - repeated calls are identical
// ---------------------------------------------------------------------------

/// Repeated calls with identical args produce `==` results.
#[test]
fn test_paginate_slice_is_stable_across_repeated_calls() {
    let items = build_u64_items(50);
    let first = paginate_slice(&items, 5, 7);
    let second = paginate_slice(&items, 5, 7);
    assert_eq!(first, second);
}

/// Returned items match the slice `items[start..end]` verbatim.
#[test]
fn test_paginate_slice_equals_underlying_slice() {
    let items = build_u64_items(100);
    for &(offset, limit) in &[(0u32, 10u32), (10, 20), (33, 17), (90, 10), (95, 50)] {
        let page = paginate_slice(&items, offset, limit);
        let (start, end) =
            calculate_safe_bounds(offset, limit, items.len() as u32);
        let expected: Vec<u64> = items[(start as usize)..(end as usize)].to_vec();
        assert_eq!(page, expected, "mismatch for offset={offset} limit={limit}");
    }
}

// ---------------------------------------------------------------------------
// 5. No duplicates across consecutive pages, concatenation equals capped prefix
// ---------------------------------------------------------------------------

/// Consecutive pages with size in {1, 3, 7, 25, MAX_QUERY_LIMIT} never
/// duplicate items and concatenate to the original prefix.
#[test]
fn test_no_duplicates_across_pages_various_sizes() {
    let items = build_u64_items(250);
    for size in [1u32, 3, 7, 25, MAX_QUERY_LIMIT] {
        let mut collected: Vec<u64> = Vec::new();
        // Bound total iterations: MAX_QUERY_LIMIT pages at size 1 covers 100 items.
        let max_pages = (MAX_QUERY_LIMIT / size).max(1) + 1;
        for page_idx in 0..max_pages {
            let offset = page_idx.saturating_mul(size);
            let page = paginate_slice(&items, offset, size);
            if page.is_empty() {
                break;
            }
            assert!(
                page.len() as u32 <= size.min(MAX_QUERY_LIMIT),
                "page exceeded size for size={size}"
            );
            for item in page {
                assert!(
                    !collected.contains(&item),
                    "duplicate item {item} detected at size={size}"
                );
                collected.push(item);
            }
        }
        assert_eq!(
            collected,
            items
                .iter()
                .take(collected.len())
                .cloned()
                .collect::<Vec<u64>>(),
            "concatenation is not a prefix of the input at size={size}"
        );
        assert!(
            collected.len() as u32 <= MAX_QUERY_LIMIT || size <= MAX_QUERY_LIMIT,
            "unexpected collected length for size={size}"
        );
    }
}

// ---------------------------------------------------------------------------
// 6. u32::MAX edge cases - no panic for offset or limit at the extreme
// ---------------------------------------------------------------------------

/// `(u32::MAX, u32::MAX)` against total=0, 1, MAX_QUERY_LIMIT, MAX+10 is a no-op.
#[test]
fn test_validate_params_u32_max_extremes_do_not_panic() {
    for total in [0u32, 1, MAX_QUERY_LIMIT, MAX_QUERY_LIMIT + 10] {
        let (safe_off, eff_lim, has_more) =
            validate_pagination_params(u32::MAX, u32::MAX, total);
        assert_eq!(safe_off, total, "safe_off must clamp to total for total={total}");
        assert_eq!(eff_lim, 0, "no items available past the end for total={total}");
        assert!(!has_more, "has_more must be false when past the end");
    }
}

/// `calculate_safe_bounds` with `u32::MAX` inputs never yields `start > end`.
#[test]
fn test_calculate_safe_bounds_u32_max_inputs_are_safe() {
    for collection_size in [0u32, 1, MAX_QUERY_LIMIT, MAX_QUERY_LIMIT + 10, 5_000] {
        let (start, end) =
            calculate_safe_bounds(u32::MAX, u32::MAX, collection_size);
        assert!(start <= end);
        assert!(end <= collection_size);
        assert!(end.saturating_sub(start) <= MAX_QUERY_LIMIT);
    }
}

// ---------------------------------------------------------------------------
// 7. Empty collection - empty result for any (offset, limit)
// ---------------------------------------------------------------------------

/// `paginate_slice(&[], offset, limit)` is empty for any input.
#[test]
fn test_empty_collection_always_returns_empty() {
    let empty: Vec<u64> = Vec::new();
    for &(offset, limit) in &[
        (0u32, 0u32),
        (0, 10),
        (10, 0),
        (10, 10),
        (u32::MAX, 0),
        (u32::MAX, u32::MAX),
    ] {
        assert!(paginate_slice(&empty, offset, limit).is_empty());
    }
}

// ---------------------------------------------------------------------------
// 8. Single-element collection is paginated correctly
// ---------------------------------------------------------------------------

/// Single-element behaviour: `(0, 10)` -> [42]; `(1, 10)` -> []; `(0, 0)` -> [].
#[test]
fn test_single_element_collection_paginates_correctly() {
    let items: Vec<u64> = vec![42];
    assert_eq!(paginate_slice(&items, 0, 10), vec![42]);
    assert!(paginate_slice(&items, 1, 10).is_empty());
    assert!(paginate_slice(&items, 0, 0).is_empty());
    assert_eq!(paginate_slice(&items, 0, 1), vec![42]);
}

// ---------------------------------------------------------------------------
// 9. Boundary: total=100, offset=99 -> exactly 1 element; offset=100 -> empty
// ---------------------------------------------------------------------------

/// Boundary around `total_count = 100`.
#[test]
fn test_boundary_offset_equals_total_minus_one_yields_one() {
    let items = build_u64_items(100);
    let page = paginate_slice(&items, 99, 10);
    assert_eq!(page.len(), 1);
    assert_eq!(page[0], 99);
}

/// `offset == total_count` yields zero elements.
#[test]
fn test_boundary_offset_equals_total_yields_empty() {
    let items = build_u64_items(100);
    let page = paginate_slice(&items, 100, 10);
    assert!(page.is_empty());
}

// ---------------------------------------------------------------------------
// 10. Cross-type coverage - u64, [u8; 32], NamedId(String)
// ---------------------------------------------------------------------------

/// Generic `paginate_slice` works for `u64`.
#[test]
fn test_generic_paginate_on_u64() {
    let items = build_u64_items(20);
    let page = paginate_slice(&items, 5, 3);
    assert_eq!(page, vec![5u64, 6, 7]);
}

/// Generic `paginate_slice` works for `[u8; 32]` - the dominant ID type in
/// Soroban contracts.
#[test]
fn test_generic_paginate_on_bytes_32() {
    let items = build_bytes_items(10);
    let page = paginate_slice(&items, 7, 2);
    assert_eq!(page.len(), 2);
    assert_eq!(page[0], [7u8; 32]);
    assert_eq!(page[1], [8u8; 32]);
}

/// Generic `paginate_slice` works for a custom `Clone` newtype around `String`.
#[test]
fn test_generic_paginate_on_named_id_string_newtype() {
    let items = build_named_items(5);
    let page = paginate_slice(&items, 1, 3);
    assert_eq!(page.len(), 3);
    assert_eq!(page[0].0, "1");
    assert_eq!(page[1].0, "2");
    assert_eq!(page[2].0, "3");
}

// ---------------------------------------------------------------------------
// 11. Cross-consistency: validate_pagination_params and calculate_safe_bounds
//     must agree on the effective window.
// ---------------------------------------------------------------------------

/// For any `(offset, limit, total)`, the two helpers return consistent bounds.
#[test]
fn test_cross_consistency_validate_vs_bounds() {
    let cases: &[(u32, u32, u32)] = &[
        (0, 0, 0),
        (0, 10, 0),
        (0, 10, 1),
        (0, 10, 50),
        (25, 10, 50),
        (49, 10, 50),
        (50, 10, 50),
        (51, 10, 50),
        (0, MAX_QUERY_LIMIT + 50, 1_000),
        (0, u32::MAX, 1_000),
        (u32::MAX, u32::MAX, 1_000),
        (100, 50, 250),
    ];
    for &(offset, limit, total) in cases {
        let (safe_off, eff_lim, _has_more) =
            validate_pagination_params(offset, limit, total);
        let (start, end) = calculate_safe_bounds(offset, limit, total);
        assert_eq!(start, safe_off, "start != safe_off for case {:?}", (offset, limit, total));
        assert_eq!(
            end.saturating_sub(start),
            eff_lim,
            "window size != effective_limit for case {:?}",
            (offset, limit, total)
        );
    }
}

// ---------------------------------------------------------------------------
// 12. has_more correctness matrix
// ---------------------------------------------------------------------------

/// `has_more` is true iff items remain past the current page.
#[test]
fn test_has_more_within_window_is_true() {
    // total=50, offset=0, limit=10 -> page ends at 10 < 50
    let (_, _, has_more) = validate_pagination_params(0, 10, 50);
    assert!(has_more);
}

/// `has_more` is false when the page exactly exhausts the collection.
#[test]
fn test_has_more_exhausting_page_is_false() {
    // total=50, offset=40, limit=10 -> page ends at 50 == 50
    let (_, _, has_more) = validate_pagination_params(40, 10, 50);
    assert!(!has_more);
}

/// `has_more` is false when past the end.
#[test]
fn test_has_more_past_end_is_false() {
    let (_, _, has_more) = validate_pagination_params(60, 10, 50);
    assert!(!has_more);
}

/// Over-cap `limit` still clamps and may flip `has_more` off when
/// `MAX_QUERY_LIMIT` covers the remainder.
#[test]
fn test_has_more_over_cap_limit_clamps_correctly() {
    // total=50, limit=500 -> clamped to 100 -> remainder=50 -> eff_lim=50, has_more=false
    let (_, eff_lim, has_more) = validate_pagination_params(0, 500, 50);
    assert_eq!(eff_lim, 50);
    assert!(!has_more);

    // total=250, limit=500 -> clamped to 100 -> eff_lim=100, has_more=true (150 remaining)
    let (_, eff_lim2, has_more2) = validate_pagination_params(0, 500, 250);
    assert_eq!(eff_lim2, MAX_QUERY_LIMIT);
    assert!(has_more2);
}

// ---------------------------------------------------------------------------
// 13. Proptest - invariants
// ---------------------------------------------------------------------------

proptest! {
    /// `validate_pagination_params` never yields `effective_limit > MAX_QUERY_LIMIT`.
    #[test]
    fn prop_paginate_never_exceeds_cap(
        offset in 0u32..=10_000,
        limit in 0u32..=u32::MAX,
        total in 0u32..=10_000,
    ) {
        let (_, eff_lim, _) = validate_pagination_params(offset, limit, total);
        prop_assert!(eff_lim <= MAX_QUERY_LIMIT);
        prop_assert!(eff_lim <= total);
    }

    /// `paginate_slice` returns exactly `items[start..end]` where
    /// `(start, end) = calculate_safe_bounds(offset, limit, items.len() as u32)`.
    #[test]
    fn prop_paginate_preserves_order(
        v in proptest::collection::vec(any::<u64>(), 0..=300),
        offset in 0u32..=400,
        limit in 0u32..=400,
    ) {
        let page = paginate_slice(&v, offset, limit);
        let (start, end) = calculate_safe_bounds(offset, limit, v.len() as u32);
        let expected: Vec<u64> = v[(start as usize)..(end as usize)].to_vec();
        prop_assert_eq!(page, expected);
    }

    /// Concatenation of all pages equals a contiguous prefix of the input and
    /// never contains duplicates.
    #[test]
    fn prop_paginate_all_pages_cover_capped_prefix(
        v in proptest::collection::vec(any::<u64>(), 0..=300),
        size in 1u32..=MAX_QUERY_LIMIT,
    ) {
        let mut collected: Vec<u64> = Vec::new();
        // MAX_QUERY_LIMIT pages at smallest size = 1 covers 100 items; one extra
        // page proves we hit the hard cap.
        let max_pages = (MAX_QUERY_LIMIT / size).max(1) + 2;
        for p in 0..max_pages {
            let offset = p.saturating_mul(size);
            let page = paginate_slice(&v, offset, size);
            if page.is_empty() {
                break;
            }
            prop_assert!(page.len() as u32 <= size);
            for item in page {
                prop_assert!(!collected.contains(&item));
                collected.push(item);
            }
        }
        // Must be a prefix of v.
        for (i, item) in collected.iter().enumerate() {
            prop_assert_eq!(*item, v[i]);
        }
    }

    /// `validate_pagination_params` and `calculate_safe_bounds` never panic on
    /// any `u32` triple and always satisfy `start <= end <= total`.
    #[test]
    fn prop_no_panic_on_u32_extremes(
        offset in any::<u32>(),
        limit in any::<u32>(),
        total in any::<u32>(),
    ) {
        let (safe_off, eff_lim, has_more) = validate_pagination_params(offset, limit, total);
        prop_assert!(safe_off <= total);
        prop_assert!(eff_lim <= MAX_QUERY_LIMIT);
        prop_assert!(safe_off.saturating_add(eff_lim) <= total);
        if has_more {
            prop_assert!(safe_off.saturating_add(eff_lim) < total);
        }

        let (start, end) = calculate_safe_bounds(offset, limit, total);
        prop_assert!(start <= end);
        prop_assert!(end <= total);
        prop_assert!(end.saturating_sub(start) <= MAX_QUERY_LIMIT);
    }
}

// ===========================================================================
// Section 2 — Escrow query consistency (Issue #831)
//
// All tests below exercise the two public escrow query surfaces:
//   • get_escrow_details  → returns the full Escrow struct
//   • get_escrow_status   → returns just the EscrowStatus enum
//
// Invariants verified:
//   1. details.status == get_escrow_status() at every lifecycle point
//   2. Immutable fields do not change across transitions
//   3. Missing-record errors are identical and deterministic on both surfaces
// ===========================================================================

// The tests below need the full contract; they live in a sub-module so they
// can access `super::*` from lib.rs (identical pattern to other test_* files).
#[cfg(test)]
mod escrow_query_consistency {
    use crate::errors::QuickLendXError;
    use crate::invoice::InvoiceCategory;
    use crate::payments::EscrowStatus;
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        token, Address, BytesN, Env, String, Vec,
    };

    fn setup_contract() -> (Env, crate::QuickLendXContractClient<'static>, Address) {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(crate::QuickLendXContract, ());
        let client = crate::QuickLendXContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let _ = client.try_initialize_admin(&admin);
        client.set_admin(&admin);
        (env, client, admin)
    }

    fn setup_token(
        env: &Env,
        business: &Address,
        investor: &Address,
        contract_id: &Address,
    ) -> Address {
        let token_admin = Address::generate(env);
        let currency = env
            .register_stellar_asset_contract_v2(token_admin)
            .address();
        let tc = token::Client::new(env, &currency);
        let sac = token::StellarAssetClient::new(env, &currency);
        sac.mint(business, &100_000i128);
        sac.mint(investor, &100_000i128);
        let exp = env.ledger().sequence() + 10_000;
        tc.approve(business, contract_id, &100_000i128, &exp);
        tc.approve(investor, contract_id, &100_000i128, &exp);
        currency
    }

    fn setup_funded_invoice(
        env: &Env,
        client: &crate::QuickLendXContractClient<'static>,
        admin: &Address,
        amount: i128,
    ) -> (Address, Address, Address, BytesN<32>, BytesN<32>) {
        let contract_id = client.address.clone();
        let business = Address::generate(env);
        let investor = Address::generate(env);

        client.submit_kyc_application(&business, &String::from_str(env, "B"));
        client.verify_business(admin, &business);
        client.submit_investor_kyc(&investor, &String::from_str(env, "I"));
        client.verify_investor(&investor, &(amount * 10));

        let currency = setup_token(env, &business, &investor, &contract_id);
        let due = env.ledger().timestamp() + 86_400;
        let invoice_id = client.store_invoice(
            &business,
            &amount,
            &currency,
            &due,
            &String::from_str(env, "Invoice"),
            &InvoiceCategory::Services,
            &Vec::new(env),
        );
        client.verify_invoice(&invoice_id);

        let bid_id = client.place_bid(&investor, &invoice_id, &amount, &(amount + 500));
        client.accept_bid(&invoice_id, &bid_id);

        (business, investor, currency, invoice_id, bid_id)
    }

    // -----------------------------------------------------------------------
    // 1. Status agreement at each lifecycle point
    // -----------------------------------------------------------------------

    /// After funding: details.status == Held and get_escrow_status() == Held.
    #[test]
    fn test_status_match_held() {
        let (env, client, admin) = setup_contract();
        let amount = 5_000i128;
        let (_, _, _, invoice_id, _) = setup_funded_invoice(&env, &client, &admin, amount);

        let details = client.get_escrow_details(&invoice_id);
        let status = client.get_escrow_status(&invoice_id);

        assert_eq!(details.status, EscrowStatus::Held);
        assert_eq!(status, EscrowStatus::Held);
        assert_eq!(details.status, status);
    }

    /// After release: details.status == Released and get_escrow_status() == Released.
    #[test]
    fn test_status_match_released() {
        let (env, client, admin) = setup_contract();
        let amount = 5_000i128;
        let (_, _, _, invoice_id, _) = setup_funded_invoice(&env, &client, &admin, amount);

        client.release_escrow_funds(&invoice_id);

        let details = client.get_escrow_details(&invoice_id);
        let status = client.get_escrow_status(&invoice_id);

        assert_eq!(details.status, EscrowStatus::Released);
        assert_eq!(status, EscrowStatus::Released);
        assert_eq!(details.status, status);
    }

    /// After refund: details.status == Refunded and get_escrow_status() == Refunded.
    #[test]
    fn test_status_match_refunded() {
        let (env, client, admin) = setup_contract();
        let amount = 5_000i128;
        let (_, _, _, invoice_id, _) = setup_funded_invoice(&env, &client, &admin, amount);

        client.refund_escrow_funds(&invoice_id, &admin);

        let details = client.get_escrow_details(&invoice_id);
        let status = client.get_escrow_status(&invoice_id);

        assert_eq!(details.status, EscrowStatus::Refunded);
        assert_eq!(status, EscrowStatus::Refunded);
        assert_eq!(details.status, status);
    }

    // -----------------------------------------------------------------------
    // 2. Immutable fields stable across transitions
    // -----------------------------------------------------------------------

    /// Snapshot taken before and after Held → Released; only status changes.
    #[test]
    fn test_immutable_fields_stable_held_to_released() {
        let (env, client, admin) = setup_contract();
        let amount = 7_000i128;
        let (business, investor, currency, invoice_id, _) =
            setup_funded_invoice(&env, &client, &admin, amount);

        let before = client.get_escrow_details(&invoice_id);
        client.release_escrow_funds(&invoice_id);
        let after = client.get_escrow_details(&invoice_id);

        assert_eq!(before.escrow_id, after.escrow_id);
        assert_eq!(before.invoice_id, after.invoice_id);
        assert_eq!(before.investor, after.investor);
        assert_eq!(before.business, after.business);
        assert_eq!(before.amount, after.amount);
        assert_eq!(before.currency, after.currency);
        assert_eq!(before.created_at, after.created_at);

        // Sanity: fields match what was set up
        assert_eq!(before.investor, investor);
        assert_eq!(before.business, business);
        assert_eq!(before.amount, amount);
        assert_eq!(before.currency, currency);
    }

    /// Snapshot taken before and after Held → Refunded; only status changes.
    #[test]
    fn test_immutable_fields_stable_held_to_refunded() {
        let (env, client, admin) = setup_contract();
        let amount = 7_000i128;
        let (_, _, _, invoice_id, _) = setup_funded_invoice(&env, &client, &admin, amount);

        let before = client.get_escrow_details(&invoice_id);
        client.refund_escrow_funds(&invoice_id, &admin);
        let after = client.get_escrow_details(&invoice_id);

        assert_eq!(before.escrow_id, after.escrow_id);
        assert_eq!(before.invoice_id, after.invoice_id);
        assert_eq!(before.investor, after.investor);
        assert_eq!(before.business, after.business);
        assert_eq!(before.amount, after.amount);
        assert_eq!(before.currency, after.currency);
        assert_eq!(before.created_at, after.created_at);
        assert_ne!(before.status, after.status);
    }

    // -----------------------------------------------------------------------
    // 3. Missing-record errors — deterministic and identical on both surfaces
    // -----------------------------------------------------------------------

    /// A random ID with no escrow returns `StorageKeyNotFound` on both surfaces.
    #[test]
    fn test_missing_record_both_surfaces_same_error() {
        let (env, client, _admin) = setup_contract();
        let ghost = BytesN::from_array(&env, &[0xDE; 32]);

        let de = client.try_get_escrow_details(&ghost).unwrap_err().unwrap();
        let se = client.try_get_escrow_status(&ghost).unwrap_err().unwrap();

        assert_eq!(de, QuickLendXError::StorageKeyNotFound);
        assert_eq!(se, QuickLendXError::StorageKeyNotFound);
        assert_eq!(de, se);
    }

    /// A verified invoice that was never funded returns `StorageKeyNotFound`
    /// (not `InvoiceNotFound`).
    #[test]
    fn test_verified_invoice_no_escrow_returns_storage_key_not_found() {
        let (env, client, admin) = setup_contract();
        let contract_id = client.address.clone();

        let business = Address::generate(&env);
        client.submit_kyc_application(&business, &String::from_str(&env, "B"));
        client.verify_business(&admin, &business);

        let investor = Address::generate(&env);
        let currency = setup_token(&env, &business, &investor, &contract_id);

        let due = env.ledger().timestamp() + 86_400;
        let invoice_id = client.store_invoice(
            &business,
            &1_000i128,
            &currency,
            &due,
            &String::from_str(&env, "Invoice"),
            &InvoiceCategory::Services,
            &Vec::new(&env),
        );
        client.verify_invoice(&invoice_id);

        let de = client.try_get_escrow_details(&invoice_id).unwrap_err().unwrap();
        let se = client.try_get_escrow_status(&invoice_id).unwrap_err().unwrap();

        assert_eq!(de, QuickLendXError::StorageKeyNotFound);
        assert_eq!(se, QuickLendXError::StorageKeyNotFound);
    }

    /// Error is deterministic: repeated calls return the same variant.
    #[test]
    fn test_missing_record_error_is_stable_across_repeated_calls() {
        let (env, client, _admin) = setup_contract();
        let ghost = BytesN::from_array(&env, &[0xCC; 32]);

        for _ in 0..3 {
            let de = client.try_get_escrow_details(&ghost).unwrap_err().unwrap();
            let se = client.try_get_escrow_status(&ghost).unwrap_err().unwrap();
            assert_eq!(de, QuickLendXError::StorageKeyNotFound);
            assert_eq!(se, QuickLendXError::StorageKeyNotFound);
        }
    }

    // -----------------------------------------------------------------------
    // 4. Cross-invoice isolation
    // -----------------------------------------------------------------------

    /// Querying one escrow never changes another. Transitions on A do not
    /// affect B's query results.
    #[test]
    fn test_cross_invoice_queries_are_isolated() {
        let (env, client, admin) = setup_contract();
        let (_, _, _, invoice_a, _) = setup_funded_invoice(&env, &client, &admin, 4_000);
        let (_, _, _, invoice_b, _) = setup_funded_invoice(&env, &client, &admin, 6_000);

        // Release A; B must remain Held.
        client.release_escrow_funds(&invoice_a);

        assert_eq!(client.get_escrow_status(&invoice_a), EscrowStatus::Released);
        assert_eq!(client.get_escrow_status(&invoice_b), EscrowStatus::Held);
        assert_eq!(
            client.get_escrow_details(&invoice_b).status,
            EscrowStatus::Held
        );

        // Refund B; A must remain Released.
        client.refund_escrow_funds(&invoice_b, &admin);

        assert_eq!(client.get_escrow_status(&invoice_a), EscrowStatus::Released);
        assert_eq!(client.get_escrow_status(&invoice_b), EscrowStatus::Refunded);
    }
}
