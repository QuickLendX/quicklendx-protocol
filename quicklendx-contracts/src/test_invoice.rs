#![cfg(test)]

use crate::errors::QuickLendXError;
use crate::invoice::Invoice;
use crate::types::{InvoiceCategory, InvoiceMetadata, InvoiceStatus, LineItemRecord};
use crate::{QuickLendXContract, QuickLendXContractClient};
use soroban_sdk::{
    testutils::{Address as _, MockAuth, MockAuthInvoke},
    Address, BytesN, Env, IntoVal, String, Vec,
};

// ============================================================================
// HELPER FUNCTIONS
// ============================================================================

/// Helper to set up a verified business for testing
fn setup_verified_business(env: &Env, client: &QuickLendXContractClient, admin: &Address) -> Address {
    let business = Address::generate(env);
    let kyc_data = String::from_str(env, "Business KYC data");

    client.submit_kyc_application(&business, &kyc_data);
    client.verify_business(admin, &business);

    business
}

/// Helper to create a test invoice
fn create_test_invoice(
    env: &Env,
    client: &QuickLendXContractClient,
    business: &Address,
    amount: i128,
) -> BytesN<32> {
    let currency = Address::generate(env);
    let due_date = env.ledger().timestamp() + 86400; // 1 day from now

    client.store_invoice(
        business,
        &amount,
        &currency,
        &due_date,
        &String::from_str(env, "Test invoice"),
        &InvoiceCategory::Services,
        &Vec::new(env),
    )
}

// ============================================================================
// AUTHORIZATION AND SECURITY ENFORCEMENT TESTS
// ============================================================================

#[test]
fn test_unauthorized_tag_addition_fails() {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let business = Address::generate(&env);
    let malicious_user = Address::generate(&env);
    let invoice_id = create_test_invoice(&env, &client, &business, 1_000_000);

    let new_tag = String::from_str(&env, "stolen_invoice");

    env.mock_auths(&[
        MockAuth {
            address: &malicious_user,
            invoke: &MockAuthInvoke {
                contract: &contract_id,
                fn_name: "add_invoice_tag",
                args: (invoice_id.clone(), new_tag.clone()).into_val(&env),
                sub_invokes: &[],
            },
        },
    ]);

    let result = client.try_add_invoice_tag(&invoice_id, &new_tag);
    assert!(result.is_err());
}

#[test]
fn test_unauthorized_category_update_fails() {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let business = Address::generate(&env);
    let malicious_user = Address::generate(&env);
    let invoice_id = create_test_invoice(&env, &client, &business, 1_000_000);

    env.mock_auths(&[
        MockAuth {
            address: &malicious_user,
            invoke: &MockAuthInvoke {
                contract: &contract_id,
                fn_name: "update_invoice_category",
                args: (invoice_id.clone(), InvoiceCategory::Healthcare).into_val(&env),
                sub_invokes: &[],
            },
        },
    ]);

    let result = client.try_update_invoice_category(&invoice_id, &InvoiceCategory::Healthcare);
    assert!(result.is_err());
}

#[test]
fn test_authorized_mutation_succeeds() {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let business = Address::generate(&env);
    let invoice_id = create_test_invoice(&env, &client, &business, 1_000_000);
    let new_tag = String::from_str(&env, "verified_v2");

    env.mock_auths(&[
        MockAuth {
            address: &business,
            invoke: &MockAuthInvoke {
                contract: &contract_id,
                fn_name: "add_invoice_tag",
                args: (invoice_id.clone(), new_tag.clone()).into_val(&env),
                sub_invokes: &[],
            },
        },
    ]);

    let result = client.try_add_invoice_tag(&invoice_id, &new_tag);
    assert!(result.is_ok());

    let invoice = client.get_invoice(&invoice_id);
    assert!(invoice.tags.contains(new_tag));
}

// ============================================================================
// INVOICE CANCELLATION AUTHORIZATION AND STATE-PRECONDITION TESTS
// ============================================================================

#[test]
fn test_invoice_cancel_authorization() {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());
    let business = Address::generate(&env);
    let attacker = Address::generate(&env);
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;
    let description = String::from_str(&env, "Test invoice");
    let category = InvoiceCategory::Services;
    let tags = Vec::new(&env);

    let mut invoice = env.as_contract(&contract_id, || {
        Invoice::new(
            &env,
            business.clone(),
            10_000,
            currency,
            due_date,
            description,
            category,
            tags,
        )
    }).expect("Invoice creation should succeed");

    let result = env.as_contract(&contract_id, || {
        invoice.cancel(&env, attacker)
    });
    assert_eq!(
        result.unwrap_err(),
        QuickLendXError::Unauthorized,
        "Non-owner cannot cancel invoice"
    );

    assert_eq!(invoice.status, InvoiceStatus::Pending);

    let result = env.as_contract(&contract_id, || {
        invoice.cancel(&env, business.clone())
    });
    assert!(result.is_ok(), "Business owner can cancel invoice");
    assert_eq!(invoice.status, InvoiceStatus::Cancelled);
}

#[test]
fn test_invoice_cancel_no_state_preconditions() {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());
    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;
    let description = String::from_str(&env, "Test invoice");
    let category = InvoiceCategory::Services;
    let tags = Vec::new(&env);

    let test_states = soroban_sdk::vec![
        &env,
        InvoiceStatus::Pending,
        InvoiceStatus::Verified,
        InvoiceStatus::Funded,
        InvoiceStatus::Paid,
        InvoiceStatus::Defaulted,
    ];

    for status in test_states {
        let mut invoice = env.as_contract(&contract_id, || {
            Invoice::new(
                &env,
                business.clone(),
                10_000,
                currency.clone(),
                due_date,
                description.clone(),
                category,
                tags.clone(),
            )
        }).expect("Invoice creation should succeed");

        invoice.status = status.clone();

        let result = env.as_contract(&contract_id, || {
            invoice.cancel(&env, business.clone())
        });
        assert!(result.is_ok(), "Cancel should succeed");
        assert_eq!(invoice.status, InvoiceStatus::Cancelled);
    }
}

// ============================================================================
// DIRECT OWNERSHIP GUARD UNIT TESTS (Issue #1980)
// ============================================================================

#[test]
fn test_ownership_guard_fails_for_mismatched_business() {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());
    let business_a = Address::generate(&env);
    let business_b = Address::generate(&env);

    let invoice = env.as_contract(&contract_id, || {
        Invoice::new(
            &env,
            business_a.clone(),
            10_000,
            Address::generate(&env),
            env.ledger().timestamp() + 86400,
            String::from_str(&env, "Test invoice"),
            InvoiceCategory::Services,
            Vec::new(&env),
        )
    }).expect("Invoice creation should succeed");

    // Call the guard helper directly with mismatched business_b
    let result = crate::invoice::require_matching_business_invoice_ownership(&env, &business_b, &invoice);
    assert_eq!(result.unwrap_err(), QuickLendXError::Unauthorized);
}

#[test]
fn test_ownership_guard_succeeds_for_matching_business() {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());
    let business_a = Address::generate(&env);

    let invoice = env.as_contract(&contract_id, || {
        Invoice::new(
            &env,
            business_a.clone(),
            10_000,
            Address::generate(&env),
            env.ledger().timestamp() + 86400,
            String::from_str(&env, "Test invoice"),
            InvoiceCategory::Services,
            Vec::new(&env),
        )
    }).expect("Invoice creation should succeed");

    // Call the guard helper directly with matching business_a
    let result = crate::invoice::require_matching_business_invoice_ownership(&env, &business_a, &invoice);
    assert!(result.is_ok());
}