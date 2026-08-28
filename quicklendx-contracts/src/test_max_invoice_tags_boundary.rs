#![cfg(test)]

//! # Max-Invoice-Tags Helper Boundary Tests
//!
//! Locks in the cap behaviour enforced by every helper that bounds the size of
//! an invoice's tag vector at `MAX_INVOICE_TAGS` (= 10). The cap is enforced at
//! three distinct entry points using two separate constants:
//!
//! - `MAX_INVOICE_TAGS` (in `crate::invoice`) — used by [`Invoice::new`] (bulk,
//!   post-normalization, distinct tags) and [`Invoice::add_tag`] (incremental
//!   mutator on an existing invoice).
//! - `MAX_INVOICE_TAG_COUNT` (in `crate::verification`) — used by
//!   [`crate::verification::validate_invoice_tags`] (pure bulk validator).
//!
//! Each helper is exercised at three boundary points:
//!
//! - **below cap** — strictly fewer than the cap: must succeed.
//! - **at cap**    — exactly the cap: must succeed (the declared bound is
//!                   inclusive).
//! - **over cap**  — one past the cap: must fail with the stable error
//!                   [`crate::errors::QuickLendXError::TagLimitExceeded`].
//!
//! No feature gate is used (`#[cfg(test)]` only) so these tests run on every
//! CI matrix entry — they are not skipped by the off-by-default
//! `legacy-tests` or `fuzz-tests` features that gate the existing property
//! tests for these surfaces.
//!
//! Test names are assertive (not interrogative). All inputs are deterministic —
//! no `Date.now()` / `Math.random()` — and tag bodies are built with
//! `alloc::format!` so test names map directly to the assertion they protect.

extern crate alloc;

use crate::errors::QuickLendXError;
use crate::invoice::{Invoice, InvoiceCategory, MAX_INVOICE_TAGS};
use crate::verification::{validate_invoice_tags, MAX_INVOICE_TAG_COUNT};
use crate::QuickLendXContract;

use alloc::format;
use soroban_sdk::{testutils::Address as _, Address, Env, String, Vec};

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

fn fresh_env_with_contract() -> (Env, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    (env, contract_id)
}

/// Build a baseline invoice owned by a freshly generated business. The tag
/// vector is empty so the per-helper boundary tests start from a known state.
fn make_empty_invoice(env: &Env, contract_id: &Address) -> Invoice {
    let business = Address::generate(env);
    let currency = Address::generate(env);
    env.as_contract(contract_id, || {
        Invoice::new(
env,
business,
1_000,
currency,
env.ledger().timestamp() + 86_400,
String::from_str(env, "tag-boundary invoice"),
InvoiceCategory::Services,
Vec::new(env),
None,
        None, /* early_payment_discount_bps */
        
)
        .expect("baseline invoice creation must succeed")
    })
}

/// Build a `Vec<String>` of `count` *distinct*, normalized tag values
/// (`"t0"`, `"t1"`, …). All tags stay well below the per-tag length cap and
/// avoid the trim/lower-case equivalence classes for adjacent indices so the
/// dedup logic never collapses them.
fn distinct_tags_vec(env: &Env, count: u32) -> Vec<String> {
    let mut v = Vec::new(env);
    for i in 0..count {
        v.push_back(String::from_str(env, &format!("t{}", i)));
    }
    v
}

// ═══════════════════════════════════════════════════════════════════════════
// 1. Invoice::add_tag boundary — the named "add_tag" helper
// ═══════════════════════════════════════════════════════════════════════════

/// `add_tag` succeeds for every addition while the tag vector is strictly
/// below the cap. Locks in the inclusive lower bound of the helper's accept
/// range.
#[test]
fn add_tag_accepts_every_addition_below_max_invoice_tags_cap() {
    let (env, contract_id) = fresh_env_with_contract();
    let mut inv = make_empty_invoice(&env, &contract_id);

    // Strictly below the cap: MAX_INVOICE_TAGS - 1 distinct adds must succeed.
    let below = MAX_INVOICE_TAGS.saturating_sub(1);
    for i in 0..below {
        inv.add_tag(&env, String::from_str(&env, &format!("t{}", i)))
            .unwrap_or_else(|e| panic!("add_tag failed below cap at i={}: {:?}", i, e));
    }
    assert_eq!(inv.tags.len(), below);
    assert!(inv.tags.len() < MAX_INVOICE_TAGS);
}

/// `add_tag` succeeds for the full `MAX_INVOICE_TAGS` invocations, growing the
/// tag vector to exactly the declared bound.
#[test]
fn add_tag_accepts_exactly_max_invoice_tags_distinct_additions() {
    let (env, contract_id) = fresh_env_with_contract();
    let mut inv = make_empty_invoice(&env, &contract_id);

    for i in 0..MAX_INVOICE_TAGS {
        inv.add_tag(&env, String::from_str(&env, &format!("t{}", i)))
            .unwrap_or_else(|e| panic!("add_tag failed at cap i={}: {:?}", i, e));
    }
    assert_eq!(inv.tags.len(), MAX_INVOICE_TAGS);
}

/// `add_tag` returns `TagLimitExceeded` on the *first* attempt that would push
/// the tag vector past the cap, and the rejected call must NOT mutate the
/// vector.
#[test]
fn add_tag_rejects_first_addition_past_max_invoice_tags_cap() {
    let (env, contract_id) = fresh_env_with_contract();
    let mut inv = make_empty_invoice(&env, &contract_id);

    // Fill to the cap.
    for i in 0..MAX_INVOICE_TAGS {
        inv.add_tag(&env, String::from_str(&env, &format!("t{}", i)))
            .unwrap();
    }
    assert_eq!(inv.tags.len(), MAX_INVOICE_TAGS);

    // One more must fail with the stable error and must not grow the vector.
    let err = inv
        .add_tag(&env, String::from_str(&env, "overflow"))
        .unwrap_err();
    assert_eq!(err, QuickLendXError::TagLimitExceeded);
    assert_eq!(inv.tags.len(), MAX_INVOICE_TAGS);
}

/// Once the cap is reached, every subsequent `add_tag` call fails with
/// `TagLimitExceeded` and leaves the vector untouched. Guards against a
/// regression where a transient capacity error might allow a later write to
/// slip through.
#[test]
fn add_tag_rejects_every_addition_after_max_invoice_tags_cap_is_reached() {
    let (env, contract_id) = fresh_env_with_contract();
    let mut inv = make_empty_invoice(&env, &contract_id);

    for i in 0..MAX_INVOICE_TAGS {
        inv.add_tag(&env, String::from_str(&env, &format!("t{}", i)))
            .unwrap();
    }

    // Five additional attempts past the cap must all fail with the same
    // stable error and must not mutate the vector.
    for attempt in 0..5u32 {
        let err = inv
            .add_tag(&env, String::from_str(&env, &format!("extra{}", attempt)))
            .unwrap_err();
        assert_eq!(
            err,
            QuickLendXError::TagLimitExceeded,
            "expected TagLimitExceeded on attempt {} past the cap",
            attempt + 1
        );
    }
    assert_eq!(inv.tags.len(), MAX_INVOICE_TAGS);
}

// ═══════════════════════════════════════════════════════════════════════════
// 2. Invoice::new (bulk) boundary — distinct-tag path post-normalization
// ═══════════════════════════════════════════════════════════════════════════

/// `Invoice::new` succeeds when supplied with strictly fewer than
/// `MAX_INVOICE_TAGS` distinct, well-formed tags, and the stored vector
/// contains exactly that many entries.
#[test]
fn invoice_new_accepts_distinct_tags_below_max_invoice_tags_cap() {
    let (env, contract_id) = fresh_env_with_contract();
    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    let below = MAX_INVOICE_TAGS.saturating_sub(1);
    let tags = distinct_tags_vec(&env, below);

    let invoice = env
        .as_contract(&contract_id, || {
            Invoice::new(
&env,
business,
1_000,
currency,
env.ledger().timestamp() + 86_400,
String::from_str(&env, "below-cap invoice"),
InvoiceCategory::Services,
tags,
None,
            None, /* early_payment_discount_bps */
            
)
        })
        .expect("Invoice::new must succeed below the tag cap");
    assert_eq!(invoice.tags.len(), below);
}

/// `Invoice::new` succeeds when supplied with exactly `MAX_INVOICE_TAGS`
/// distinct tags. This is the inclusive-upper-bound acceptance case for the
/// bulk ctor.
#[test]
fn invoice_new_accepts_distinct_tags_exactly_at_max_invoice_tags_cap() {
    let (env, contract_id) = fresh_env_with_contract();
    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    let tags = distinct_tags_vec(&env, MAX_INVOICE_TAGS);

    let invoice = env
        .as_contract(&contract_id, || {
            Invoice::new(
&env,
business,
1_000,
currency,
env.ledger().timestamp() + 86_400,
String::from_str(&env, "at-cap invoice"),
InvoiceCategory::Services,
tags,
None,
            None, /* early_payment_discount_bps */
            
)
        })
        .expect("Invoice::new must succeed at exactly the tag cap");
    assert_eq!(invoice.tags.len(), MAX_INVOICE_TAGS);
}

/// `Invoice::new` returns `TagLimitExceeded` when supplied with
/// `MAX_INVOICE_TAGS + 1` distinct tags. The cap is inclusive — the ctor must
/// stop appending and surface the stable error variant.
#[test]
fn invoice_new_rejects_distinct_tags_one_over_max_invoice_tags_cap() {
    let (env, contract_id) = fresh_env_with_contract();
    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    let tags = distinct_tags_vec(&env, MAX_INVOICE_TAGS + 1);

    let err = env
        .as_contract(&contract_id, || {
            Invoice::new(
&env,
business,
1_000,
currency,
env.ledger().timestamp() + 86_400,
String::from_str(&env, "over-cap invoice"),
InvoiceCategory::Services,
tags,
None,
            None, /* early_payment_discount_bps */
            
)
        })
        .unwrap_err();
    assert_eq!(err, QuickLendXError::TagLimitExceeded);
}

/// Cross-helper invariant: invalid tag content (empty after normalization)
/// keeps the bulk ctor from ever reaching the cap check. Locks in the input-
/// order documented by `Invoice::new`: duplicate collapse, then length
/// enforcement. Supplying many *invalid* tags must not by itself trip the cap
/// guard — it must trip `InvalidTag` instead, leaving the cap boundary
/// unchanged.
#[test]
fn invoice_new_invalid_tags_fail_with_invalid_tag_not_tag_limit_exceeded() {
    let (env, contract_id) = fresh_env_with_contract();
    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    // Build a vec whose length is over the cap, but every entry is
    // whitespace-only so each one fails normalization before the cap is
    // checked. The first one to fail must produce `InvalidTag`.
    let mut tags = Vec::new(&env);
    for _ in 0..12u32 {
        tags.push_back(String::from_str(&env, "   "));
    }

    let err = env
        .as_contract(&contract_id, || {
            Invoice::new(
&env,
business,
1_000,
currency,
env.ledger().timestamp() + 86_400,
String::from_str(&env, "invalid-tag invoice"),
InvoiceCategory::Services,
tags,
None,
            None, /* early_payment_discount_bps */
            
)
        })
        .unwrap_err();
    assert_eq!(err, QuickLendXError::InvalidTag);
}

// ═══════════════════════════════════════════════════════════════════════════
// 3. validation::validate_invoice_tags (bulk validator) boundary
// ═══════════════════════════════════════════════════════════════════════════

/// `validate_invoice_tags` accepts a vector with `MAX_INVOICE_TAG_COUNT - 1`
/// distinct, well-formed tags.
#[test]
fn validate_invoice_tags_accepts_distinct_tags_below_max_invoice_tag_count() {
    let (env, contract_id) = fresh_env_with_contract();
    env.as_contract(&contract_id, || {
        let tags = distinct_tags_vec(&env, MAX_INVOICE_TAG_COUNT.saturating_sub(1));
        let result = validate_invoice_tags(&env, &tags);
        assert!(
            result.is_ok(),
            "expected Ok below cap, got {:?}",
            result.unwrap_err()
        );
    });
}

/// `validate_invoice_tags` accepts a vector with exactly
/// `MAX_INVOICE_TAG_COUNT` distinct, well-formed tags. The cap check uses a
/// strict `>` so the inclusive-at-cap edge is an *accept*.
#[test]
fn validate_invoice_tags_accepts_distinct_tags_exactly_at_max_invoice_tag_count() {
    let (env, contract_id) = fresh_env_with_contract();
    env.as_contract(&contract_id, || {
        let tags = distinct_tags_vec(&env, MAX_INVOICE_TAG_COUNT);
        let result = validate_invoice_tags(&env, &tags);
        assert!(
            result.is_ok(),
            "expected Ok at exact cap, got {:?}",
            result.unwrap_err()
        );
    });
}

/// `validate_invoice_tags` rejects a vector with `MAX_INVOICE_TAG_COUNT + 1`
/// distinct, well-formed tags, returning the stable `TagLimitExceeded` error.
#[test]
fn validate_invoice_tags_rejects_distinct_tags_one_over_max_invoice_tag_count() {
    let (env, contract_id) = fresh_env_with_contract();
    env.as_contract(&contract_id, || {
        let tags = distinct_tags_vec(&env, MAX_INVOICE_TAG_COUNT + 1);
        let err = validate_invoice_tags(&env, &tags).unwrap_err();
        assert_eq!(err, QuickLendXError::TagLimitExceeded);
    });
}

/// `validate_invoice_tags` continues to return `TagLimitExceeded` for every
/// further increase past the cap. Locks in the strict-greater-than behaviour:
/// doubling the overflow does not change the error variant.
#[test]
fn validate_invoice_tags_rejects_distinct_tags_far_over_max_invoice_tag_count() {
    let (env, contract_id) = fresh_env_with_contract();
    env.as_contract(&contract_id, || {
        let tags = distinct_tags_vec(&env, MAX_INVOICE_TAG_COUNT + 5);
        let err = validate_invoice_tags(&env, &tags).unwrap_err();
        assert_eq!(err, QuickLendXError::TagLimitExceeded);
    });
}

/// `validate_invoice_tags` accepts the empty vector. The empty-input path is
/// the deepest "below cap" case and documents that the validator never
/// rejects on count alone when `count == 0`.
#[test]
fn validate_invoice_tags_accepts_empty_vector_below_max_invoice_tag_count() {
    let (env, contract_id) = fresh_env_with_contract();
    env.as_contract(&contract_id, || {
        let tags: Vec<String> = Vec::new(&env);
        assert!(validate_invoice_tags(&env, &tags).is_ok());
    });
}
