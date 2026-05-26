#![cfg(test)]

//! # Invoice Metadata Owner Authorization Tests
//!
//! Validates that only the invoice business owner can update, clear, or
//! modify metadata fields and their derived indexes.
//!
//! ## Security invariants tested
//!
//! 1. `update_metadata` rejects non-owner callers with `Unauthorized`
//! 2. `clear_metadata` rejects non-owner callers with `Unauthorized`
//! 3. No partial writes occur on authorization failure (atomicity)
//! 4. Owner operations succeed and correctly update storage + indexes
//! 5. Metadata validation runs *after* authorization (no info leak)
//! 6. Derived indexes (customer, tax_id) only change on owner calls

use soroban_sdk::{testutils::Address as _, Address, BytesN, Env, String, Vec};

use crate::errors::QuickLendXError;
use crate::invoice::{
    Invoice, InvoiceCategory, InvoiceMetadata, LineItemRecord, MAX_INVOICE_TAGS,
    MAX_RATINGS_PER_INVOICE,
};
use crate::storage::{Indexes, InvoiceStorage};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Create a fresh invoice owned by `business`.
fn make_invoice(env: &Env, business: &Address) -> Invoice {
    let currency = Address::generate(env);
    let tags = Vec::new(env);
    Invoice::new(
        env,
        business.clone(),
        1000,
        currency,
        env.ledger().timestamp() + 86400,
        String::from_str(env, "Test invoice"),
        InvoiceCategory::Services,
        tags,
    )
}

/// Build valid metadata with sensible defaults.
fn make_metadata(env: &Env) -> InvoiceMetadata {
    InvoiceMetadata {
        customer_name: String::from_str(env, "Acme Corp"),
        customer_address: String::from_str(env, "42 Blockchain Ave"),
        tax_id: String::from_str(env, "TAX-999"),
        line_items: Vec::from_array(
            env,
            [LineItemRecord(
                String::from_str(env, "Consulting"),
                1,
                1000,
                1000,
            )],
        ),
        notes: String::from_str(env, "Net 30"),
    }
}

/// Build alternative metadata for update/index-change tests.
fn make_alt_metadata(env: &Env) -> InvoiceMetadata {
    InvoiceMetadata {
        customer_name: String::from_str(env, "Beta Inc"),
        customer_address: String::from_str(env, "99 Stellar Blvd"),
        tax_id: String::from_str(env, "TAX-888"),
        line_items: Vec::from_array(
            env,
            [LineItemRecord(
                String::from_str(env, "Development"),
                2,
                500,
                1000,
            )],
        ),
        notes: String::from_str(env, "Urgent"),
    }
}

// ---------------------------------------------------------------------------
// 1. Owner can update metadata
// ---------------------------------------------------------------------------

#[test]
fn test_owner_can_update_metadata() {
    let env = Env::default();
    env.mock_all_auths();

    let business = Address::generate(&env);
    let mut invoice = make_invoice(&env, &business);
    let metadata = make_metadata(&env);

    let result = invoice.update_metadata(&env, &business, metadata.clone());

    assert!(result.is_ok());
    assert_eq!(
        invoice.metadata_customer_name,
        Some(metadata.customer_name)
    );
    assert_eq!(
        invoice.metadata_customer_address,
        Some(metadata.customer_address)
    );
    assert_eq!(invoice.metadata_tax_id, Some(metadata.tax_id));
    assert_eq!(invoice.metadata_notes, Some(metadata.notes));
    assert_eq!(invoice.metadata_line_items.len(), 1);
}

// ---------------------------------------------------------------------------
// 2. Non-owner cannot update metadata
// ---------------------------------------------------------------------------

#[test]
fn test_non_owner_update_metadata_returns_unauthorized() {
    let env = Env::default();
    env.mock_all_auths();

    let business = Address::generate(&env);
    let attacker = Address::generate(&env);
    let mut invoice = make_invoice(&env, &business);
    let metadata = make_metadata(&env);

    let result = invoice.update_metadata(&env, &attacker, metadata);

    assert_eq!(result, Err(QuickLendXError::Unauthorized));
}

// ---------------------------------------------------------------------------
// 3. Non-owner cannot clear metadata
// ---------------------------------------------------------------------------

#[test]
fn test_non_owner_clear_metadata_returns_unauthorized() {
    let env = Env::default();
    env.mock_all_auths();

    let business = Address::generate(&env);
    let attacker = Address::generate(&env);
    let mut invoice = make_invoice(&env, &business);

    // First set metadata as owner
    let metadata = make_metadata(&env);
    invoice
        .update_metadata(&env, &business, metadata)
        .unwrap();

    // Attacker tries to clear
    let result = invoice.clear_metadata(&env, &attacker);

    assert_eq!(result, Err(QuickLendXError::Unauthorized));
}

// ---------------------------------------------------------------------------
// 4. No partial writes on auth failure - update
// ---------------------------------------------------------------------------

#[test]
fn test_no_partial_write_on_unauthorized_update() {
    let env = Env::default();
    env.mock_all_auths();

    let business = Address::generate(&env);
    let attacker = Address::generate(&env);
    let mut invoice = make_invoice(&env, &business);

    // Set initial metadata as owner
    let original = make_metadata(&env);
    invoice
        .update_metadata(&env, &business, original.clone())
        .unwrap();

    // Snapshot pre-attack state
    let pre_name = invoice.metadata_customer_name.clone();
    let pre_addr = invoice.metadata_customer_address.clone();
    let pre_tax = invoice.metadata_tax_id.clone();
    let pre_notes = invoice.metadata_notes.clone();
    let pre_items_len = invoice.metadata_line_items.len();

    // Attacker tries to overwrite with different metadata
    let evil_metadata = make_alt_metadata(&env);
    let result = invoice.update_metadata(&env, &attacker, evil_metadata);
    assert!(result.is_err());

    // ALL fields must remain unchanged
    assert_eq!(invoice.metadata_customer_name, pre_name);
    assert_eq!(invoice.metadata_customer_address, pre_addr);
    assert_eq!(invoice.metadata_tax_id, pre_tax);
    assert_eq!(invoice.metadata_notes, pre_notes);
    assert_eq!(invoice.metadata_line_items.len(), pre_items_len);
}

// ---------------------------------------------------------------------------
// 5. No partial writes on auth failure - clear
// ---------------------------------------------------------------------------

#[test]
fn test_no_partial_write_on_unauthorized_clear() {
    let env = Env::default();
    env.mock_all_auths();

    let business = Address::generate(&env);
    let attacker = Address::generate(&env);
    let mut invoice = make_invoice(&env, &business);

    // Set metadata as owner
    let metadata = make_metadata(&env);
    invoice
        .update_metadata(&env, &business, metadata.clone())
        .unwrap();

    // Attacker tries to clear
    let result = invoice.clear_metadata(&env, &attacker);
    assert!(result.is_err());

    // Metadata must still be present
    assert!(invoice.metadata_customer_name.is_some());
    assert!(invoice.metadata_customer_address.is_some());
    assert!(invoice.metadata_tax_id.is_some());
    assert!(invoice.metadata_notes.is_some());
    assert_eq!(invoice.metadata_line_items.len(), 1);
}

// ---------------------------------------------------------------------------
// 6. Owner can clear metadata
// ---------------------------------------------------------------------------

#[test]
fn test_owner_can_clear_metadata() {
    let env = Env::default();
    env.mock_all_auths();

    let business = Address::generate(&env);
    let mut invoice = make_invoice(&env, &business);

    // Set then clear
    let metadata = make_metadata(&env);
    invoice
        .update_metadata(&env, &business, metadata)
        .unwrap();
    let result = invoice.clear_metadata(&env, &business);

    assert!(result.is_ok());
    assert!(invoice.metadata_customer_name.is_none());
    assert!(invoice.metadata_customer_address.is_none());
    assert!(invoice.metadata_tax_id.is_none());
    assert!(invoice.metadata_notes.is_none());
    assert_eq!(invoice.metadata_line_items.len(), 0);
}

// ---------------------------------------------------------------------------
// 7. Clearing already-empty metadata is idempotent
// ---------------------------------------------------------------------------

#[test]
fn test_clear_empty_metadata_is_idempotent() {
    let env = Env::default();
    env.mock_all_auths();

    let business = Address::generate(&env);
    let mut invoice = make_invoice(&env, &business);

    // Clear without ever setting metadata
    let result = invoice.clear_metadata(&env, &business);
    assert!(result.is_ok());
    assert!(invoice.metadata_customer_name.is_none());
}

// ---------------------------------------------------------------------------
// 8. Derived indexes update only on owner operations
// ---------------------------------------------------------------------------

#[test]
fn test_indexes_created_on_owner_update() {
    let env = Env::default();
    env.mock_all_auths();

    let business = Address::generate(&env);
    let mut invoice = make_invoice(&env, &business);
    let metadata = make_metadata(&env);

    invoice
        .update_metadata(&env, &business, metadata.clone())
        .unwrap();
    InvoiceStorage::store(&env, &invoice);

    // Customer index should contain invoice ID
    let key = Indexes::invoices_by_customer(&metadata.customer_name);
    let ids: Vec<BytesN<32>> = env.storage().persistent().get(&key).unwrap();
    assert!(ids.iter().any(|id| id == invoice.id));

    // Tax ID index should contain invoice ID
    let key = Indexes::invoices_by_tax_id(&metadata.tax_id);
    let ids: Vec<BytesN<32>> = env.storage().persistent().get(&key).unwrap();
    assert!(ids.iter().any(|id| id == invoice.id));
}

#[test]
fn test_indexes_unchanged_after_unauthorized_update() {
    let env = Env::default();
    env.mock_all_auths();

    let business = Address::generate(&env);
    let attacker = Address::generate(&env);
    let mut invoice = make_invoice(&env, &business);

    // Owner sets initial metadata and stores
    let original = make_metadata(&env);
    invoice
        .update_metadata(&env, &business, original.clone())
        .unwrap();
    InvoiceStorage::store(&env, &invoice);

    // Attacker tries to change metadata (fails at Invoice level)
    let alt = make_alt_metadata(&env);
    let _ = invoice.update_metadata(&env, &attacker, alt.clone());
    // Even if someone re-stores, the invoice fields are unchanged
    InvoiceStorage::update(&env, &invoice);

    // Original indexes still present
    let key = Indexes::invoices_by_customer(&original.customer_name);
    let ids: Vec<BytesN<32>> = env.storage().persistent().get(&key).unwrap();
    assert!(ids.iter().any(|id| id == invoice.id));

    // Attacker's indexes should NOT exist
    let alt_key = Indexes::invoices_by_customer(&alt.customer_name);
    let alt_ids: Option<Vec<BytesN<32>>> = env.storage().persistent().get(&alt_key);
    assert!(
        alt_ids.is_none() || !alt_ids.unwrap().iter().any(|id| id == invoice.id)
    );
}

// ---------------------------------------------------------------------------
// 9. Indexes removed on owner clear
// ---------------------------------------------------------------------------

#[test]
fn test_indexes_removed_on_owner_clear() {
    let env = Env::default();
    env.mock_all_auths();

    let business = Address::generate(&env);
    let mut invoice = make_invoice(&env, &business);
    let metadata = make_metadata(&env);

    // Set, store, clear, update
    invoice
        .update_metadata(&env, &business, metadata.clone())
        .unwrap();
    InvoiceStorage::store(&env, &invoice);
    invoice.clear_metadata(&env, &business).unwrap();
    InvoiceStorage::update(&env, &invoice);

    // Customer index should no longer contain invoice ID
    let key = Indexes::invoices_by_customer(&metadata.customer_name);
    let ids: Option<Vec<BytesN<32>>> = env.storage().persistent().get(&key);
    assert!(
        ids.is_none() || !ids.unwrap().iter().any(|id| id == invoice.id)
    );

    // Tax ID index should no longer contain invoice ID
    let key = Indexes::invoices_by_tax_id(&metadata.tax_id);
    let ids: Option<Vec<BytesN<32>>> = env.storage().persistent().get(&key);
    assert!(
        ids.is_none() || !ids.unwrap().iter().any(|id| id == invoice.id)
    );
}

// ---------------------------------------------------------------------------
// 10. Indexes swap correctly on owner metadata change
// ---------------------------------------------------------------------------

#[test]
fn test_indexes_swap_on_owner_metadata_change() {
    let env = Env::default();
    env.mock_all_auths();

    let business = Address::generate(&env);
    let mut invoice = make_invoice(&env, &business);

    // Set original metadata
    let original = make_metadata(&env);
    invoice
        .update_metadata(&env, &business, original.clone())
        .unwrap();
    InvoiceStorage::store(&env, &invoice);

    // Owner changes metadata to new customer
    let alt = make_alt_metadata(&env);
    invoice
        .update_metadata(&env, &business, alt.clone())
        .unwrap();
    InvoiceStorage::update(&env, &invoice);

    // Old customer index should NOT contain invoice ID
    let old_key = Indexes::invoices_by_customer(&original.customer_name);
    let old_ids: Option<Vec<BytesN<32>>> = env.storage().persistent().get(&old_key);
    assert!(
        old_ids.is_none() || !old_ids.unwrap().iter().any(|id| id == invoice.id)
    );

    // New customer index should contain invoice ID
    let new_key = Indexes::invoices_by_customer(&alt.customer_name);
    let new_ids: Vec<BytesN<32>> = env.storage().persistent().get(&new_key).unwrap();
    assert!(new_ids.iter().any(|id| id == invoice.id));
}

// ---------------------------------------------------------------------------
// 11. Validation failure does not leak partial state
// ---------------------------------------------------------------------------

#[test]
fn test_invalid_metadata_does_not_modify_state() {
    let env = Env::default();
    env.mock_all_auths();

    let business = Address::generate(&env);
    let mut invoice = make_invoice(&env, &business);

    // Set valid metadata first
    let valid = make_metadata(&env);
    invoice
        .update_metadata(&env, &business, valid.clone())
        .unwrap();

    // Try to update with invalid metadata (empty customer_name)
    let invalid = InvoiceMetadata {
        customer_name: String::from_str(&env, ""),
        customer_address: String::from_str(&env, "Addr"),
        tax_id: String::from_str(&env, "T"),
        line_items: Vec::new(&env),
        notes: String::from_str(&env, ""),
    };
    let result = invoice.update_metadata(&env, &business, invalid);
    assert!(result.is_err());

    // Original metadata must be intact
    assert_eq!(
        invoice.metadata_customer_name,
        Some(valid.customer_name)
    );
}

// ---------------------------------------------------------------------------
// 12. Multiple non-owner attempts all fail consistently
// ---------------------------------------------------------------------------

#[test]
fn test_multiple_attackers_all_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let business = Address::generate(&env);
    let mut invoice = make_invoice(&env, &business);
    let metadata = make_metadata(&env);

    // Try 3 different non-owners
    for _ in 0..3 {
        let attacker = Address::generate(&env);
        assert_eq!(
            invoice.update_metadata(&env, &attacker, metadata.clone()),
            Err(QuickLendXError::Unauthorized)
        );
        assert_eq!(
            invoice.clear_metadata(&env, &attacker),
            Err(QuickLendXError::Unauthorized)
        );
    }

    // Invoice still has no metadata (never set by owner)
    assert!(invoice.metadata_customer_name.is_none());
}

// ---------------------------------------------------------------------------
// 13. Owner update after failed non-owner attempt succeeds
// ---------------------------------------------------------------------------

#[test]
fn test_owner_succeeds_after_attacker_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let business = Address::generate(&env);
    let attacker = Address::generate(&env);
    let mut invoice = make_invoice(&env, &business);
    let metadata = make_metadata(&env);

    // Attacker fails
    let _ = invoice.update_metadata(&env, &attacker, metadata.clone());

    // Owner succeeds
    let result = invoice.update_metadata(&env, &business, metadata.clone());
    assert!(result.is_ok());
    assert_eq!(
        invoice.metadata_customer_name,
        Some(metadata.customer_name)
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// 14. Tag normalization deduplicates at invoice construction
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_invoice_new_deduplicates_trimmed_casefolded_tags() {
    let env = Env::default();
    let business = Address::generate(&env);
    let currency = Address::generate(&env);

    let mut tags = Vec::new(&env);
    tags.push_back(String::from_str(&env, "  Tech  "));
    tags.push_back(String::from_str(&env, "tech"));
    tags.push_back(String::from_str(&env, "TECH"));

    let invoice = Invoice::new(
        &env,
        business,
        1000,
        currency,
        env.ledger().timestamp() + 86400,
        String::from_str(&env, "Normalized tags"),
        InvoiceCategory::Services,
        tags,
    )
    .expect("invoice creation should normalize tags");

    assert_eq!(
        invoice.tags.len(),
        1,
        "trim/case-equivalent tags must collapse to one canonical value"
    );
    assert_eq!(invoice.tags.get(0).unwrap(), String::from_str(&env, "tech"));
}

// ═══════════════════════════════════════════════════════════════════════════
// 15. Tag vector cap blocks unbounded growth
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_add_tag_rejects_when_max_tag_count_reached() {
    let env = Env::default();
    env.mock_all_auths();

    let business = Address::generate(&env);
    let mut invoice = make_invoice(&env, &business);

    for i in 0..MAX_INVOICE_TAGS {
        let tag = String::from_str(&env, &format!("tag{}", i));
        invoice.add_tag(&env, tag).expect("tag add should succeed");
    }
    assert_eq!(invoice.tags.len(), MAX_INVOICE_TAGS);

    let err = invoice
        .add_tag(&env, String::from_str(&env, "overflow-tag"))
        .unwrap_err();
    assert_eq!(err, QuickLendXError::TagLimitExceeded);
}

// ═══════════════════════════════════════════════════════════════════════════
// 16. Ratings vector cap blocks unbounded growth
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_add_rating_rejects_when_max_rating_count_reached() {
    let env = Env::default();
    let business = Address::generate(&env);
    let mut invoice = make_invoice(&env, &business);

    // add_rating is only valid when invoice is funded/paid.
    invoice.status = crate::invoice::InvoiceStatus::Funded;

    for i in 0..MAX_RATINGS_PER_INVOICE {
        let rater = Address::generate(&env);
        invoice
            .add_rating(5, String::from_str(&env, "ok"), rater, i as u64 + 1)
            .expect("rating add should succeed until max bound");
    }

    let err = invoice
        .add_rating(
            4,
            String::from_str(&env, "excess"),
            Address::generate(&env),
            9999,
        )
        .unwrap_err();
    assert_eq!(err, QuickLendXError::OperationNotAllowed);
}
