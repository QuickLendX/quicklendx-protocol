/// Tag normalization and duplicate prevention tests — Issue #527
///
/// These tests verify that:
/// - Tags are stored in lowercase, trimmed form regardless of input casing/padding.
/// - Normalized duplicates are rejected at creation time.
/// - `add_invoice_tag`, `remove_invoice_tag`, `invoice_has_tag`, `get_invoices_by_tag`,
///   and `get_invoice_count_by_tag` all operate on the normalized form.
#![cfg(test)]
extern crate std;

use crate::errors::QuickLendXError;
use crate::invoice::InvoiceCategory;
use crate::{QuickLendXContract, QuickLendXContractClient};
use soroban_sdk::{testutils::Address as _, Address, Env, String, Vec};

fn setup(env: &Env) -> (QuickLendXContractClient<'static>, Address, Address) {
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(env, &contract_id);
    let business = Address::generate(env);
    let currency = Address::generate(env);
    (client, business, currency)
}

fn make_invoice(
    env: &Env,
    client: &QuickLendXContractClient,
    business: &Address,
    currency: &Address,
    tags: Vec<String>,
) -> soroban_sdk::BytesN<32> {
    env.mock_all_auths();
    let due_date = env.ledger().timestamp() + 86400;
    client.store_invoice(
        business,
        &1000,
        currency,
        &due_date,
        &String::from_str(env, "Test invoice"),
        &InvoiceCategory::Services,
        &tags,
    )
}

// ----------------------------------------------------------------------------
// Normalization at creation time
// ----------------------------------------------------------------------------

/// Tags submitted in uppercase are stored as lowercase.
#[test]
fn test_tag_stored_as_lowercase() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, business, currency) = setup(&env);
    let mut tags = Vec::new(&env);
    tags.push_back(String::from_str(&env, "Technology"));

    let id = make_invoice(&env, &client, &business, &currency, tags);
    let stored = client.get_invoice_tags(&id).unwrap();

    assert_eq!(stored.len(), 1);
    assert_eq!(stored.get(0).unwrap(), String::from_str(&env, "technology"));
}

/// Tags with surrounding whitespace are trimmed.
#[test]
fn test_tag_whitespace_trimmed() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, business, currency) = setup(&env);
    let mut tags = Vec::new(&env);
    tags.push_back(String::from_str(&env, "  tech  "));

    let id = make_invoice(&env, &client, &business, &currency, tags);
    let stored = client.get_invoice_tags(&id).unwrap();

    assert_eq!(stored.len(), 1);
    assert_eq!(stored.get(0).unwrap(), String::from_str(&env, "tech"));
}

/// A tag consisting entirely of spaces normalizes to empty and is rejected.
#[test]
fn test_whitespace_only_tag_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, business, currency) = setup(&env);
    let mut tags = Vec::new(&env);
    tags.push_back(String::from_str(&env, "   "));
    let due_date = env.ledger().timestamp() + 86400;

    let result = client.try_store_invoice(
        &business,
        &1000,
        &currency,
        &due_date,
        &String::from_str(&env, "Spaces test"),
        &InvoiceCategory::Services,
        &tags,
    );
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().unwrap(), QuickLendXError::InvalidTag);
}

/// "Tech" and "tech" in the same tag list are normalized duplicates — rejected.
#[test]
fn test_case_duplicate_tags_rejected_at_creation() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, business, currency) = setup(&env);
    let mut tags = Vec::new(&env);
    tags.push_back(String::from_str(&env, "tech"));
    tags.push_back(String::from_str(&env, "Tech"));
    let due_date = env.ledger().timestamp() + 86400;

    let result = client.try_store_invoice(
        &business,
        &1000,
        &currency,
        &due_date,
        &String::from_str(&env, "Dup test"),
        &InvoiceCategory::Services,
        &tags,
    );
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().unwrap(), QuickLendXError::InvalidTag);
}

/// " tech " and "tech" in the same tag list are normalized duplicates — rejected.
#[test]
fn test_whitespace_duplicate_tags_rejected_at_creation() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, business, currency) = setup(&env);
    let mut tags = Vec::new(&env);
    tags.push_back(String::from_str(&env, "tech"));
    tags.push_back(String::from_str(&env, " tech "));
    let due_date = env.ledger().timestamp() + 86400;

    let result = client.try_store_invoice(
        &business,
        &1000,
        &currency,
        &due_date,
        &String::from_str(&env, "Pad dup test"),
        &InvoiceCategory::Services,
        &tags,
    );
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().unwrap(), QuickLendXError::InvalidTag);
}

// ----------------------------------------------------------------------------
// add_invoice_tag normalization
// ----------------------------------------------------------------------------

/// Adding the same tag in different cases is idempotent: only one tag is stored.
#[test]
fn test_add_tag_case_idempotent() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, business, currency) = setup(&env);
    let id = make_invoice(&env, &client, &business, &currency, Vec::new(&env));

    client.add_invoice_tag(&id, &String::from_str(&env, "tech"));
    client.add_invoice_tag(&id, &String::from_str(&env, "Tech"));
    client.add_invoice_tag(&id, &String::from_str(&env, "TECH"));

    let tags = client.get_invoice_tags(&id).unwrap();
    assert_eq!(tags.len(), 1);
    assert_eq!(tags.get(0).unwrap(), String::from_str(&env, "tech"));
}

/// Tags added with whitespace padding are normalized before storage.
#[test]
fn test_add_tag_with_padding_normalized() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, business, currency) = setup(&env);
    let id = make_invoice(&env, &client, &business, &currency, Vec::new(&env));

    client.add_invoice_tag(&id, &String::from_str(&env, "  urgent  "));

    let tags = client.get_invoice_tags(&id).unwrap();
    assert_eq!(tags.len(), 1);
    assert_eq!(tags.get(0).unwrap(), String::from_str(&env, "urgent"));
}

// ----------------------------------------------------------------------------
// remove_invoice_tag normalization
// ----------------------------------------------------------------------------

/// Removing a tag using a different casing removes the stored lowercase entry.
#[test]
fn test_remove_tag_case_insensitive() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, business, currency) = setup(&env);
    let mut tags = Vec::new(&env);
    tags.push_back(String::from_str(&env, "tech"));
    let id = make_invoice(&env, &client, &business, &currency, tags);

    client.remove_invoice_tag(&id, &String::from_str(&env, "TECH"));

    let remaining = client.get_invoice_tags(&id).unwrap();
    assert_eq!(remaining.len(), 0);
}

// ----------------------------------------------------------------------------
// invoice_has_tag normalization
// ----------------------------------------------------------------------------

/// has_tag is case-insensitive.
#[test]
fn test_has_tag_case_insensitive() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, business, currency) = setup(&env);
    let mut tags = Vec::new(&env);
    tags.push_back(String::from_str(&env, "tech"));
    let id = make_invoice(&env, &client, &business, &currency, tags);

    assert!(client.invoice_has_tag(&id, &String::from_str(&env, "Tech")).unwrap());
    assert!(client.invoice_has_tag(&id, &String::from_str(&env, "TECH")).unwrap());
    assert!(client.invoice_has_tag(&id, &String::from_str(&env, " tech ")).unwrap());
}

// ----------------------------------------------------------------------------
// Index query normalization
// ----------------------------------------------------------------------------

/// get_invoices_by_tag normalizes the query: "Technology" finds "technology".
#[test]
fn test_tag_query_case_insensitive() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, business, currency) = setup(&env);
    let mut tags = Vec::new(&env);
    tags.push_back(String::from_str(&env, "technology"));
    let id = make_invoice(&env, &client, &business, &currency, tags);

    let results = client.get_invoices_by_tag(&String::from_str(&env, "Technology"));
    assert!(results.contains(&id));

    let results2 = client.get_invoices_by_tag(&String::from_str(&env, "TECHNOLOGY"));
    assert!(results2.contains(&id));
}

/// get_invoice_count_by_tag is case-insensitive.
#[test]
fn test_tag_count_case_insensitive() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, business, currency) = setup(&env);
    let mut tags = Vec::new(&env);
    tags.push_back(String::from_str(&env, "urgent"));
    make_invoice(&env, &client, &business, &currency, tags);

    assert_eq!(client.get_invoice_count_by_tag(&String::from_str(&env, "URGENT")), 1);
    assert_eq!(client.get_invoice_count_by_tag(&String::from_str(&env, "Urgent")), 1);
    assert_eq!(client.get_invoice_count_by_tag(&String::from_str(&env, "urgent")), 1);
}

/// get_invoices_by_tags (AND logic) works case-insensitively.
#[test]
fn test_multi_tag_query_case_insensitive() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, business, currency) = setup(&env);
    let mut tags = Vec::new(&env);
    tags.push_back(String::from_str(&env, "tech"));
    tags.push_back(String::from_str(&env, "urgent"));
    let id = make_invoice(&env, &client, &business, &currency, tags);

    let mut query_tags = Vec::new(&env);
    query_tags.push_back(String::from_str(&env, "TECH"));
    query_tags.push_back(String::from_str(&env, "Urgent"));

    let results = client.get_invoices_by_tags(&query_tags);
    assert!(results.contains(&id));
}
