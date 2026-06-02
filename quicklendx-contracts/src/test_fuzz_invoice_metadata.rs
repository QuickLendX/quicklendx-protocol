#![cfg(all(test, feature = "fuzz-tests"))]

//! Proptest harness for invoice metadata vector bounds.
//!
//! Sweeps each bounded vector (tags, line items, ratings) from 0 to its
//! declared maximum plus one and asserts:
//!   - exactly-at-max: operation succeeds
//!   - at-max-plus-one: operation fails with a stable, specific error variant
//!
//! Also verifies that the tag→invoice secondary index stays consistent under
//! arbitrary add/remove sequences (no orphan entries).
//!
//! Run with:
//!   PROPTEST_CASES=20000 cargo test --features fuzz-tests test_fuzz_invoice_metadata

use crate::errors::QuickLendXError;
use crate::invoice::{
    Invoice, InvoiceCategory, InvoiceMetadata, InvoiceStatus, MAX_INVOICE_TAGS,
    MAX_RATINGS_PER_INVOICE,
};
use crate::storage::{Indexes, InvoiceStorage};
use crate::types::LineItemRecord;
use crate::verification::{validate_invoice_metadata, MAX_METADATA_LINE_ITEMS};
use crate::QuickLendXContract;
use proptest::prelude::*;
use soroban_sdk::{testutils::Address as _, Address, BytesN, Env, String as SStr, Vec as SVec};

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

fn make_env() -> (Env, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    (env, contract_id)
}

fn make_invoice_unit(env: &Env) -> (Invoice, Address) {
    let business = Address::generate(env);
    let currency = Address::generate(env);
    let inv = Invoice::new(
        env,
        business.clone(),
        1000,
        currency,
        env.ledger().timestamp() + 86400,
        SStr::from_str(env, "fuzz invoice"),
        InvoiceCategory::Services,
        SVec::new(env),
    )
    .expect("baseline invoice creation must succeed");
    (inv, business)
}

/// Build valid metadata whose line-item totals sum to exactly `item_count`.
///
/// Each item: description="item", qty=1, unit_price=1, line_total=1.
/// Invoice amount must be set to `item_count` for `validate_invoice_metadata`
/// to pass the total-consistency check.
fn make_metadata_with_items(env: &Env, item_count: u32) -> (InvoiceMetadata, i128) {
    let mut items = SVec::new(env);
    for _ in 0..item_count {
        items.push_back(LineItemRecord(
            SStr::from_str(env, "item"),
            1,
            1,
            1,
        ));
    }
    let invoice_amount = item_count as i128;
    let metadata = InvoiceMetadata {
        customer_name: SStr::from_str(env, "Acme Corp"),
        customer_address: SStr::from_str(env, "42 Blockchain Ave"),
        tax_id: SStr::from_str(env, "TAX-001"),
        line_items: items,
        notes: SStr::from_str(env, ""),
    };
    (metadata, invoice_amount)
}

// ---------------------------------------------------------------------------
// Tag vector boundary proptest
//
// Invariant:
//   count <= MAX_INVOICE_TAGS  → Invoice::new succeeds, tags.len() == count
//   count >  MAX_INVOICE_TAGS  → Invoice::new returns TagLimitExceeded
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config({
        let mut config = ProptestConfig::from_env();
        if let Some(seed_array) = crate::test_seed::seed() {
            config.rng_algorithm = proptest::test_runner::RngAlgorithm::ChaCha;
        }
        config
    })]

    #[test]
    fn fuzz_tags_at_or_below_max_accepts(count in 0u32..=MAX_INVOICE_TAGS) {
        let (env, contract_id) = make_env();
        env.as_contract(&contract_id, || {
            let business = Address::generate(&env);
            let currency = Address::generate(&env);
            let mut tags = SVec::new(&env);
            for i in 0..count {
                tags.push_back(SStr::from_str(&env, &alloc::format!("tag{}", i)));
            }
            let result = Invoice::new(
                &env,
                business,
                1000,
                currency,
                env.ledger().timestamp() + 86400,
                SStr::from_str(&env, "t"),
                InvoiceCategory::Services,
                tags,
            );
            prop_assert!(result.is_ok(), "count={} should succeed", count);
            prop_assert_eq!(result.unwrap().tags.len(), count);
        });
    }

    #[test]
    fn fuzz_tags_above_max_rejects(extra in 1u32..=10u32) {
        let (env, contract_id) = make_env();
        env.as_contract(&contract_id, || {
            let business = Address::generate(&env);
            let currency = Address::generate(&env);
            let count = MAX_INVOICE_TAGS + extra;
            let mut tags = SVec::new(&env);
            for i in 0..count {
                tags.push_back(SStr::from_str(&env, &alloc::format!("tag{}", i)));
            }
            let result = Invoice::new(
                &env,
                business,
                1000,
                currency,
                env.ledger().timestamp() + 86400,
                SStr::from_str(&env, "t"),
                InvoiceCategory::Services,
                tags,
            );
            prop_assert_eq!(
                result,
                Err(QuickLendXError::TagLimitExceeded),
                "count={} must return TagLimitExceeded",
                count
            );
        });
    }

    /// add_tag path: sweep add operations on an existing invoice.
    #[test]
    fn fuzz_add_tag_boundary(count in 0u32..=(MAX_INVOICE_TAGS + 5)) {
        let (env, contract_id) = make_env();
        env.as_contract(&contract_id, || {
            let (mut inv, _) = make_invoice_unit(&env);
            let mut accepted = 0u32;
            for i in 0..count {
                let tag = SStr::from_str(&env, &alloc::format!("t{}", i));
                match inv.add_tag(&env, tag) {
                    Ok(()) => accepted += 1,
                    Err(QuickLendXError::TagLimitExceeded) => {
                        // All subsequent adds must also fail once capacity is reached
                        prop_assert!(
                            accepted >= MAX_INVOICE_TAGS,
                            "TagLimitExceeded fired too early at accepted={}",
                            accepted
                        );
                    }
                    Err(e) => prop_assert!(false, "unexpected error: {:?}", e),
                }
            }
            prop_assert!(
                inv.tags.len() <= MAX_INVOICE_TAGS,
                "tags.len()={} exceeded MAX_INVOICE_TAGS={}",
                inv.tags.len(),
                MAX_INVOICE_TAGS
            );
        });
    }
}

// ---------------------------------------------------------------------------
// Line-items vector boundary proptest
//
// Invariant:
//   1 <= count <= MAX_METADATA_LINE_ITEMS → validate_invoice_metadata succeeds
//   count > MAX_METADATA_LINE_ITEMS       → returns InvalidDescription
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config({
        let mut config = ProptestConfig::from_env();
        if let Some(seed_array) = crate::test_seed::seed() {
            config.rng_algorithm = proptest::test_runner::RngAlgorithm::ChaCha;
        }
        config
    })]

    #[test]
    fn fuzz_line_items_at_or_below_max_accepts(count in 1u32..=MAX_METADATA_LINE_ITEMS) {
        let (env, contract_id) = make_env();
        env.as_contract(&contract_id, || {
            let (metadata, invoice_amount) = make_metadata_with_items(&env, count);
            let result = validate_invoice_metadata(&metadata, invoice_amount);
            prop_assert!(
                result.is_ok(),
                "count={} should pass validation, got {:?}",
                count,
                result
            );
        });
    }

    #[test]
    fn fuzz_line_items_above_max_rejects(extra in 1u32..=10u32) {
        let (env, contract_id) = make_env();
        env.as_contract(&contract_id, || {
            let count = MAX_METADATA_LINE_ITEMS + extra;
            let (metadata, invoice_amount) = make_metadata_with_items(&env, count);
            let result = validate_invoice_metadata(&metadata, invoice_amount);
            prop_assert_eq!(
                result,
                Err(QuickLendXError::InvalidDescription),
                "count={} must return InvalidDescription",
                count
            );
        });
    }
}

// ---------------------------------------------------------------------------
// Ratings vector boundary proptest
//
// Invariant:
//   count <= MAX_RATINGS_PER_INVOICE → add_rating succeeds for each unique rater
//   next add after reaching max      → returns OperationNotAllowed
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config({
        let mut config = ProptestConfig::from_env();
        if let Some(seed_array) = crate::test_seed::seed() {
            config.rng_algorithm = proptest::test_runner::RngAlgorithm::ChaCha;
        }
        config
    })]

    #[test]
    fn fuzz_ratings_at_or_below_max_accepts(count in 0u32..=MAX_RATINGS_PER_INVOICE) {
        let (env, contract_id) = make_env();
        env.as_contract(&contract_id, || {
            let (mut inv, _) = make_invoice_unit(&env);
            inv.status = InvoiceStatus::Funded;
            for i in 0..count {
                let rater = Address::generate(&env);
                let result = inv.add_rating(
                    5,
                    SStr::from_str(&env, "ok"),
                    rater,
                    i as u64 + 1,
                );
                prop_assert!(
                    result.is_ok(),
                    "rating {} of {} should succeed",
                    i + 1,
                    count
                );
            }
            prop_assert_eq!(inv.ratings.len(), count);
        });
    }

    #[test]
    fn fuzz_ratings_above_max_rejects(extra in 1u32..=5u32) {
        let (env, contract_id) = make_env();
        env.as_contract(&contract_id, || {
            let (mut inv, _) = make_invoice_unit(&env);
            inv.status = InvoiceStatus::Funded;
            for i in 0..MAX_RATINGS_PER_INVOICE {
                let rater = Address::generate(&env);
                inv.add_rating(5, SStr::from_str(&env, "ok"), rater, i as u64 + 1)
                    .expect("pre-fill ratings must succeed");
            }
            // Any further add must fail, regardless of `extra` count
            let overflow_rater = Address::generate(&env);
            let result = inv.add_rating(
                4,
                SStr::from_str(&env, "overflow"),
                overflow_rater,
                9999,
            );
            prop_assert_eq!(
                result,
                Err(QuickLendXError::OperationNotAllowed),
                "rating {} must return OperationNotAllowed",
                MAX_RATINGS_PER_INVOICE + extra
            );
        });
    }
}

// ---------------------------------------------------------------------------
// Tag→invoice index consistency under random add/remove sequences
//
// Invariant: after each operation the set of tags present in `invoice.tags`
// and the set of invoice IDs stored in each tag's secondary index must agree.
// No orphan entries (tag index references a removed tag) may remain.
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config({
        let mut config = ProptestConfig::from_env();
        if let Some(seed_array) = crate::test_seed::seed() {
            config.rng_algorithm = proptest::test_runner::RngAlgorithm::ChaCha;
        }
        config
    })]

    /// Randomly add and remove distinct tags, checking index consistency.
    ///
    /// `ops` encodes a sequence of operations: even index = add tag `op % 5`,
    /// odd index = remove tag `op % 5`. This keeps the tag pool small enough
    /// that removals actually hit existing tags rather than always missing.
    #[test]
    fn fuzz_tag_index_consistency(ops in proptest::collection::vec(0u32..10u32, 1..20)) {
        let (env, contract_id) = make_env();

        // Pre-build a small pool of normalized tag names.
        let tag_pool: [&str; 5] = ["alpha", "beta", "gamma", "delta", "epsilon"];

        env.as_contract(&contract_id, || {
            let (mut inv, _) = make_invoice_unit(&env);
            // Store the invoice so the index machinery has a persistent record.
            InvoiceStorage::store(&env, &inv);

            for op in &ops {
                let tag_str = tag_pool[(*op % 5) as usize];
                let tag = SStr::from_str(&env, tag_str);

                if op % 2 == 0 {
                    // Add: tolerate TagLimitExceeded (normal at capacity)
                    let _ = inv.add_tag(&env, tag.clone());
                    if inv.has_tag(tag.clone()) {
                        InvoiceStorage::add_tag_index(&env, &tag, &inv.id);
                    }
                } else {
                    // Remove
                    let _ = inv.remove_tag(tag.clone());
                    InvoiceStorage::remove_tag_index(&env, &tag, &inv.id);
                }

                // Persist updated invoice state
                InvoiceStorage::update(&env, &inv);

                // --- Consistency check ---
                // For every tag that IS in the invoice, its index must list this invoice.
                for present_tag in inv.tags.iter() {
                    let indexed: SVec<BytesN<32>> = env
                        .storage()
                        .persistent()
                        .get(&Indexes::invoices_by_tag(&present_tag))
                        .unwrap_or_else(|| SVec::new(&env));
                    prop_assert!(
                        indexed.iter().any(|id| id == inv.id),
                        "tag '{}' is in invoice.tags but not in the tag index",
                        present_tag.len()
                    );
                }

                // For every tag NOT in the invoice, this invoice must not appear in its index.
                for absent_str in tag_pool.iter() {
                    let absent_tag = SStr::from_str(&env, absent_str);
                    if !inv.has_tag(absent_tag.clone()) {
                        let indexed: Option<SVec<BytesN<32>>> = env
                            .storage()
                            .persistent()
                            .get(&Indexes::invoices_by_tag(&absent_tag));
                        if let Some(ids) = indexed {
                            prop_assert!(
                                !ids.iter().any(|id| id == inv.id),
                                "tag '{}' is NOT in invoice.tags but invoice still appears in its index (orphan)",
                                absent_str
                            );
                        }
                    }
                }
            }
        });
    }
}

// ---------------------------------------------------------------------------
// Deterministic edge-case smoke tests (no proptest; always run with fuzz-tests)
// ---------------------------------------------------------------------------

#[test]
fn tags_exactly_at_max_accepts() {
    let (env, contract_id) = make_env();
    env.as_contract(&contract_id, || {
        let (mut inv, _) = make_invoice_unit(&env);
        for i in 0..MAX_INVOICE_TAGS {
            let tag = SStr::from_str(&env, &alloc::format!("tag{}", i));
            assert!(
                inv.add_tag(&env, tag).is_ok(),
                "tag {} of {} should succeed",
                i + 1,
                MAX_INVOICE_TAGS
            );
        }
        assert_eq!(inv.tags.len(), MAX_INVOICE_TAGS);
    });
}

#[test]
fn tags_one_over_max_rejects_with_stable_error() {
    let (env, contract_id) = make_env();
    env.as_contract(&contract_id, || {
        let (mut inv, _) = make_invoice_unit(&env);
        for i in 0..MAX_INVOICE_TAGS {
            inv.add_tag(&env, SStr::from_str(&env, &alloc::format!("tag{}", i)))
                .unwrap();
        }
        let err = inv
            .add_tag(&env, SStr::from_str(&env, "overflow"))
            .unwrap_err();
        assert_eq!(err, QuickLendXError::TagLimitExceeded);
    });
}

#[test]
fn line_items_exactly_at_max_accepts() {
    let (env, contract_id) = make_env();
    env.as_contract(&contract_id, || {
        let (metadata, invoice_amount) = make_metadata_with_items(&env, MAX_METADATA_LINE_ITEMS);
        assert!(
            validate_invoice_metadata(&metadata, invoice_amount).is_ok(),
            "exactly {} line items should pass",
            MAX_METADATA_LINE_ITEMS
        );
    });
}

#[test]
fn line_items_one_over_max_rejects_with_stable_error() {
    let (env, contract_id) = make_env();
    env.as_contract(&contract_id, || {
        let (metadata, invoice_amount) =
            make_metadata_with_items(&env, MAX_METADATA_LINE_ITEMS + 1);
        let err = validate_invoice_metadata(&metadata, invoice_amount).unwrap_err();
        assert_eq!(err, QuickLendXError::InvalidDescription);
    });
}

#[test]
fn ratings_exactly_at_max_accepts() {
    let (env, contract_id) = make_env();
    env.as_contract(&contract_id, || {
        let (mut inv, _) = make_invoice_unit(&env);
        inv.status = InvoiceStatus::Funded;
        for i in 0..MAX_RATINGS_PER_INVOICE {
            let rater = Address::generate(&env);
            assert!(
                inv.add_rating(5, SStr::from_str(&env, "ok"), rater, i as u64 + 1)
                    .is_ok(),
                "rating {} of {} should succeed",
                i + 1,
                MAX_RATINGS_PER_INVOICE
            );
        }
        assert_eq!(inv.ratings.len(), MAX_RATINGS_PER_INVOICE);
    });
}

#[test]
fn ratings_one_over_max_rejects_with_stable_error() {
    let (env, contract_id) = make_env();
    env.as_contract(&contract_id, || {
        let (mut inv, _) = make_invoice_unit(&env);
        inv.status = InvoiceStatus::Funded;
        for i in 0..MAX_RATINGS_PER_INVOICE {
            let rater = Address::generate(&env);
            inv.add_rating(5, SStr::from_str(&env, "ok"), rater, i as u64 + 1)
                .unwrap();
        }
        let err = inv
            .add_rating(
                4,
                SStr::from_str(&env, "overflow"),
                Address::generate(&env),
                9999,
            )
            .unwrap_err();
        assert_eq!(err, QuickLendXError::OperationNotAllowed);
    });
}

#[test]
fn tag_index_no_orphan_after_remove() {
    let (env, contract_id) = make_env();
    env.as_contract(&contract_id, || {
        let (mut inv, _) = make_invoice_unit(&env);
        InvoiceStorage::store(&env, &inv);

        let tag = SStr::from_str(&env, "rust");
        inv.add_tag(&env, tag.clone()).unwrap();
        InvoiceStorage::add_tag_index(&env, &tag, &inv.id);
        InvoiceStorage::update(&env, &inv);

        // Verify present
        let ids: SVec<BytesN<32>> = env
            .storage()
            .persistent()
            .get(&Indexes::invoices_by_tag(&tag))
            .unwrap();
        assert!(ids.iter().any(|id| id == inv.id));

        // Remove
        inv.remove_tag(tag.clone()).unwrap();
        InvoiceStorage::remove_tag_index(&env, &tag, &inv.id);
        InvoiceStorage::update(&env, &inv);

        // No orphan
        let ids_after: Option<SVec<BytesN<32>>> = env
            .storage()
            .persistent()
            .get(&Indexes::invoices_by_tag(&tag));
        assert!(
            ids_after
                .map_or(true, |v| !v.iter().any(|id| id == inv.id)),
            "orphan: invoice still in tag index after removal"
        );
    });
}
