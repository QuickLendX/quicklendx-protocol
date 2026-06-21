#![cfg(test)]

//! # Line Item Consistency Tests
//!
//! Validates the LineItemRecord mathematical consistency rules, overflow safety,
//! line-item count limits, and sum-to-amount alignment policy.

use soroban_sdk::{testutils::Address as _, Address, Env, String, Vec};

use crate::errors::QuickLendXError;
use crate::invoice::{Invoice, InvoiceCategory, InvoiceMetadata};
use crate::types::LineItemRecord;
use crate::QuickLendXContract;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_invoice_with_amount(env: &Env, business: &Address, amount: i128) -> Invoice {
    let currency = Address::generate(env);
    let tags = Vec::new(env);
    Invoice::new(
        env,
        business.clone(),
        amount,
        currency,
        env.ledger().timestamp() + 86400,
        String::from_str(env, "Test invoice"),
        InvoiceCategory::Services,
        tags,
    )
    .unwrap()
}

fn make_valid_metadata_with_items(
    env: &Env,
    items: Vec<LineItemRecord>,
) -> InvoiceMetadata {
    InvoiceMetadata {
        customer_name: String::from_str(env, "Acme Corp"),
        customer_address: String::from_str(env, "123 Main St"),
        tax_id: String::from_str(env, "TAX-123"),
        line_items: items,
        notes: String::from_str(env, "None"),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn test_valid_line_item_totals() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(QuickLendXContract, ());
    env.as_contract(&contract_id, || {
        let business = Address::generate(&env);
        let mut invoice = make_invoice_with_amount(&env, &business, 1000);

        let items = Vec::from_array(
            &env,
            [
                LineItemRecord(String::from_str(&env, "Consulting"), 2, 300, 600),
                LineItemRecord(String::from_str(&env, "Support"), 4, 100, 400),
            ],
        );
        let metadata = make_valid_metadata_with_items(&env, items);

        let result = invoice.update_metadata(&env, &business, metadata);
        assert!(result.is_ok());
        assert_eq!(invoice.metadata_line_items.len(), 2);
    });
}

#[test]
fn test_mismatched_line_item_total_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(QuickLendXContract, ());
    env.as_contract(&contract_id, || {
        let business = Address::generate(&env);
        let mut invoice = make_invoice_with_amount(&env, &business, 1000);

        // quantity * unit_price (2 * 300 = 600) != total (599)
        let items = Vec::from_array(
            &env,
            [LineItemRecord(String::from_str(&env, "Consulting"), 2, 300, 599)],
        );
        let metadata = make_valid_metadata_with_items(&env, items);

        let result = invoice.update_metadata(&env, &business, metadata);
        assert_eq!(result, Err(QuickLendXError::InvalidAmount));
    });
}

#[test]
fn test_overflowing_line_item_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(QuickLendXContract, ());
    env.as_contract(&contract_id, || {
        let business = Address::generate(&env);
        let mut invoice = make_invoice_with_amount(&env, &business, 1000);

        // quantity * unit_price overflows i128
        let items = Vec::from_array(
            &env,
            [LineItemRecord(
                String::from_str(&env, "Overflow Item"),
                2,
                i128::MAX,
                i128::MAX,
            )],
        );
        let metadata = make_valid_metadata_with_items(&env, items);

        let result = invoice.update_metadata(&env, &business, metadata);
        assert_eq!(result, Err(QuickLendXError::InvalidAmount));
    });
}

#[test]
fn test_max_line_items_cap_enforced() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(QuickLendXContract, ());
    env.as_contract(&contract_id, || {
        let business = Address::generate(&env);
        let mut invoice = make_invoice_with_amount(&env, &business, 101);

        let mut items = Vec::new(&env);
        for _ in 0..101 {
            items.push_back(LineItemRecord(String::from_str(&env, "Item"), 1, 1, 1));
        }
        let metadata = make_valid_metadata_with_items(&env, items);

        let result = invoice.update_metadata(&env, &business, metadata);
        // Exceeds MAX_METADATA_LINE_ITEMS (100)
        assert_eq!(result, Err(QuickLendXError::InvalidDescription));
    });
}

#[test]
fn test_sum_mismatch_to_invoice_amount_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(QuickLendXContract, ());
    env.as_contract(&contract_id, || {
        let business = Address::generate(&env);
        // Invoice amount is 1000
        let mut invoice = make_invoice_with_amount(&env, &business, 1000);

        // Sum of line items totals is 900 (not 1000)
        let items = Vec::from_array(
            &env,
            [LineItemRecord(String::from_str(&env, "Consulting"), 3, 300, 900)],
        );
        let metadata = make_valid_metadata_with_items(&env, items);

        let result = invoice.update_metadata(&env, &business, metadata);
        assert_eq!(result, Err(QuickLendXError::InvoiceAmountInvalid));
    });
}

#[test]
fn test_zero_quantity_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(QuickLendXContract, ());
    env.as_contract(&contract_id, || {
        let business = Address::generate(&env);
        let mut invoice = make_invoice_with_amount(&env, &business, 1000);

        // quantity is 0
        let items = Vec::from_array(
            &env,
            [LineItemRecord(String::from_str(&env, "Consulting"), 0, 1000, 0)],
        );
        let metadata = make_valid_metadata_with_items(&env, items);

        let result = invoice.update_metadata(&env, &business, metadata);
        assert_eq!(result, Err(QuickLendXError::InvalidAmount));
    });
}

#[test]
fn test_zero_unit_price_allowed_if_matching() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(QuickLendXContract, ());
    env.as_contract(&contract_id, || {
        let business = Address::generate(&env);
        // Invoice amount is 1000
        let mut invoice = make_invoice_with_amount(&env, &business, 1000);

        // One item has unit price 0, the other has 1000
        let items = Vec::from_array(
            &env,
            [
                LineItemRecord(String::from_str(&env, "Free item"), 10, 0, 0),
                LineItemRecord(String::from_str(&env, "Charged item"), 1, 1000, 1000),
            ],
        );
        let metadata = make_valid_metadata_with_items(&env, items);

        let result = invoice.update_metadata(&env, &business, metadata);
        assert!(result.is_ok());
        assert_eq!(invoice.metadata_line_items.len(), 2);
    });
}
