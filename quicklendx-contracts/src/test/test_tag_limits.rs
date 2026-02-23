use super::*;
use crate::invoice::InvoiceCategory;
use crate::errors::QuickLendXError;
use soroban_sdk::{testutils::Address as _, Address, Env, String, Vec};

#[test]
fn test_create_invoice_with_max_tags_allowed() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;

    // Create exactly 10 tags
    let mut tags = Vec::new(&env);
    let list = ["t1", "t2", "t3", "t4", "t5", "t6", "t7", "t8", "t9", "t10"];
    for s in list.iter() {
        tags.push_back(String::from_str(&env, s));
    }

    // Should succeed when creating invoice with exactly 10 tags
    let invoice_id = client.store_invoice(
        &business,
        &1000,
        &currency,
        &due_date,
        &String::from_str(&env, "Invoice with 10 tags"),
        &InvoiceCategory::Other,
        &tags,
    );

    // Verify stored invoice has 10 tags
    let inv = client.get_invoice(&invoice_id);
    assert_eq!(inv.get_tags().len(), 10);
}

#[test]
fn test_create_invoice_over_limit_returns_tag_limit_error() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;

    // Create 11 tags (over the limit)
    let mut tags = Vec::new(&env);
    let list = [
        "t1", "t2", "t3", "t4", "t5", "t6", "t7", "t8", "t9", "t10", "t11",
    ];
    for s in list.iter() {
        tags.push_back(String::from_str(&env, s));
    }

    // Should return TagLimitExceeded
    let res = client.try_store_invoice(
        &business,
        &1000,
        &currency,
        &due_date,
        &String::from_str(&env, "Invoice with 11 tags"),
        &InvoiceCategory::Other,
        &tags,
    );

    // Accept either an Err outer result or an inner Err result as a failure signal
    let failed = match res {
        Ok(inner) => inner.is_err(),
        Err(_) => true,
    };
    assert!(failed, "Expected TagLimitExceeded but store_invoice succeeded");
}

#[test]
fn test_add_invoice_tag_at_limit_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;

    // Start with 9 tags
    let mut tags = Vec::new(&env);
    let list = ["a1", "a2", "a3", "a4", "a5", "a6", "a7", "a8", "a9"];
    for s in list.iter() {
        tags.push_back(String::from_str(&env, s));
    }

    let invoice_id = client.store_invoice(
        &business,
        &1000,
        &currency,
        &due_date,
        &String::from_str(&env, "Invoice with 9 tags"),
        &InvoiceCategory::Other,
        &tags,
    );

    // Add 10th tag via entrypoint; should succeed
    client.add_invoice_tag(&invoice_id, &String::from_str(&env, "a10"));
    let inv = client.get_invoice(&invoice_id);
    assert_eq!(inv.get_tags().len(), 10);
}

#[test]
fn test_add_invoice_tag_over_limit_returns_tag_limit_error() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;

    // Create exactly 10 tags
    let mut tags = Vec::new(&env);
    let list = ["x1", "x2", "x3", "x4", "x5", "x6", "x7", "x8", "x9", "x10"];
    for s in list.iter() {
        tags.push_back(String::from_str(&env, s));
    }

    let invoice_id = client.store_invoice(
        &business,
        &1000,
        &currency,
        &due_date,
        &String::from_str(&env, "Invoice with 10 tags"),
        &InvoiceCategory::Other,
        &tags,
    );

    // Attempt to add 11th tag via entrypoint; should return TagLimitExceeded
    let res = client.try_add_invoice_tag(&invoice_id, &String::from_str(&env, "x11"));

    // Accept either an Err outer result or an inner Err result as a failure signal
    let failed = match res {
        Ok(inner) => inner.is_err(),
        Err(_) => true,
    };
    assert!(failed, "Expected TagLimitExceeded but add_invoice_tag succeeded");
}
