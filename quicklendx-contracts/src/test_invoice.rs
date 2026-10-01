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
fn setup_verified_business(
    env: &Env,
    client: &QuickLendXContractClient,
    admin: &Address,
) -> Address {
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
        &None,
        &None,
        &None,
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

    env.mock_auths(&[MockAuth {
        address: &malicious_user,
        invoke: &MockAuthInvoke {
            contract: &contract_id,
            fn_name: "add_invoice_tag",
            args: (invoice_id.clone(), new_tag.clone()).into_val(&env),
            sub_invokes: &[],
        },
    }]);

    // This should fail because the contract expects 'business' to sign, not 'malicious_user'
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

    env.mock_auths(&[MockAuth {
        address: &malicious_user,
        invoke: &MockAuthInvoke {
            contract: &contract_id,
            fn_name: "update_invoice_category",
            args: (invoice_id.clone(), InvoiceCategory::Healthcare).into_val(&env),
            sub_invokes: &[],
        },
    }]);

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

    env.mock_auths(&[MockAuth {
        address: &business,
        invoke: &MockAuthInvoke {
            contract: &contract_id,
            fn_name: "add_invoice_tag",
            args: (invoice_id.clone(), new_tag.clone()).into_val(&env),
            sub_invokes: &[],
        },
    }]);

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

    // Create an invoice owned by business
    let mut invoice = env
        .as_contract(&contract_id, || {
            Invoice::new(
                &env,
                business.clone(),
                10_000,
                currency,
                due_date,
                description,
                category,
                tags,
                None,
                None,
                None,
            )
        })
        .expect("Invoice creation should succeed");

    // Attempt to cancel as attacker (not business owner) - should fail in contract context
    let result = env.as_contract(&contract_id, || invoice.cancel(&env, attacker));
    assert_eq!(
        result.unwrap_err(),
        QuickLendXError::Unauthorized,
        "Non-owner cannot cancel invoice"
    );

    assert_eq!(invoice.status, InvoiceStatus::Pending);

    // Cancel as business owner - should succeed in contract context
    let result = env.as_contract(&contract_id, || invoice.cancel(&env, business.clone()));
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

    // Test cancellation from various states
    let test_states = soroban_sdk::vec![
        &env,
        InvoiceStatus::Pending,
        InvoiceStatus::Verified,
        InvoiceStatus::Funded,
        InvoiceStatus::Paid,
        InvoiceStatus::Defaulted,
    ];

    for status in test_states {
        let mut invoice = env
            .as_contract(&contract_id, || {
                Invoice::new(
                    &env,
                    business.clone(),
                    10_000,
                    currency.clone(),
                    due_date,
                    description.clone(),
                    category,
                    tags.clone(),
                    None,
                    None,
                    None,
                )
            })
            .expect("Invoice creation should succeed");

        invoice.status = status.clone();

        // Cancel should succeed regardless of state (only authorization matters)
        let result = env.as_contract(&contract_id, || invoice.cancel(&env, business.clone()));
        assert!(result.is_ok(), "Cancel should succeed");

        // Status should be Cancelled
        assert_eq!(invoice.status, InvoiceStatus::Cancelled);
    }
}

// ============================================================================
// MIXED-OWNER ATTACK SECURITY TESTS (Issue #1981)
// ============================================================================

/// Sad path: Verification that different Business B cannot update the metadata
/// of Business A's invoice, and fails auth checks.
#[test]
fn test_rejects_invoice_metadata_update_by_different_business() {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);

    env.mock_all_auths();
    let _ = client.try_initialize_admin(&admin);
    client.set_admin(&admin);

    let business_a = setup_verified_business(&env, &client, &admin);
    let business_b = setup_verified_business(&env, &client, &admin);
    let invoice_id = create_test_invoice(&env, &client, &business_a, 100_000);

    let mut line_items = Vec::new(&env);
    line_items.push_back(LineItemRecord(
        String::from_str(&env, "Item 1"),
        1,
        100_000,
        100_000,
    ));

    let metadata = InvoiceMetadata {
        customer_name: String::from_str(&env, "Customer A"),
        customer_address: String::from_str(&env, "Address A"),
        tax_id: String::from_str(&env, "Tax A"),
        line_items,
        notes: String::from_str(&env, "Notes A"),
    };

    // targeted auth mocking: only Business B signs
    env.mock_auths(&[MockAuth {
        address: &business_b,
        invoke: &MockAuthInvoke {
            contract: &contract_id,
            fn_name: "update_invoice_metadata",
            args: (invoice_id.clone(), metadata.clone()).into_val(&env),
            sub_invokes: &[],
        },
    }]);

    let result = client.try_update_invoice_metadata(&invoice_id, &metadata);
    assert!(
        result.is_err(),
        "Different business B must not be allowed to update metadata"
    );
}

/// Happy path: Verification that Business A can successfully update the
/// metadata of their own invoice.
#[test]
fn test_allows_invoice_metadata_update_by_owner_business() {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);

    env.mock_all_auths();
    let _ = client.try_initialize_admin(&admin);
    client.set_admin(&admin);

    let business_a = setup_verified_business(&env, &client, &admin);
    let invoice_id = create_test_invoice(&env, &client, &business_a, 100_000);

    let mut line_items = Vec::new(&env);
    line_items.push_back(LineItemRecord(
        String::from_str(&env, "Item 1"),
        1,
        100_000,
        100_000,
    ));

    let metadata = InvoiceMetadata {
        customer_name: String::from_str(&env, "Customer A"),
        customer_address: String::from_str(&env, "Address A"),
        tax_id: String::from_str(&env, "Tax A"),
        line_items,
        notes: String::from_str(&env, "Notes A"),
    };

    // targeted auth mocking: Business A signs
    env.mock_auths(&[MockAuth {
        address: &business_a,
        invoke: &MockAuthInvoke {
            contract: &contract_id,
            fn_name: "update_invoice_metadata",
            args: (invoice_id.clone(), metadata.clone()).into_val(&env),
            sub_invokes: &[],
        },
    }]);

    let result = client.try_update_invoice_metadata(&invoice_id, &metadata);
    assert!(
        result.is_ok(),
        "Business owner must succeed updating own metadata"
    );

    let invoice = client.get_invoice(&invoice_id);
    let updated = invoice.metadata().unwrap();
    assert_eq!(updated.customer_name, String::from_str(&env, "Customer A"));
}

/// Sad path: Verification that different Business B cannot clear the metadata
/// of Business A's invoice.
#[test]
fn test_rejects_invoice_metadata_clear_by_different_business() {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);

    env.mock_all_auths();
    let _ = client.try_initialize_admin(&admin);
    client.set_admin(&admin);

    let business_a = setup_verified_business(&env, &client, &admin);
    let business_b = setup_verified_business(&env, &client, &admin);
    let invoice_id = create_test_invoice(&env, &client, &business_a, 100_000);

    let mut line_items = Vec::new(&env);
    line_items.push_back(LineItemRecord(
        String::from_str(&env, "Item 1"),
        1,
        100_000,
        100_000,
    ));

    let metadata = InvoiceMetadata {
        customer_name: String::from_str(&env, "Customer A"),
        customer_address: String::from_str(&env, "Address A"),
        tax_id: String::from_str(&env, "Tax A"),
        line_items,
        notes: String::from_str(&env, "Notes A"),
    };
    client.update_invoice_metadata(&invoice_id, &metadata);

    // targeted auth mocking: only Business B signs
    env.mock_auths(&[MockAuth {
        address: &business_b,
        invoke: &MockAuthInvoke {
            contract: &contract_id,
            fn_name: "clear_invoice_metadata",
            args: (invoice_id.clone(),).into_val(&env),
            sub_invokes: &[],
        },
    }]);

    let result = client.try_clear_invoice_metadata(&invoice_id);
    assert!(
        result.is_err(),
        "Different business B must not clear metadata"
    );
}

/// Happy path: Verification that Business A can successfully clear the
/// metadata of their own invoice.
#[test]
fn test_allows_invoice_metadata_clear_by_owner_business() {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);

    env.mock_all_auths();
    let _ = client.try_initialize_admin(&admin);
    client.set_admin(&admin);

    let business_a = setup_verified_business(&env, &client, &admin);
    let invoice_id = create_test_invoice(&env, &client, &business_a, 100_000);

    let mut line_items = Vec::new(&env);
    line_items.push_back(LineItemRecord(
        String::from_str(&env, "Item 1"),
        1,
        100_000,
        100_000,
    ));

    let metadata = InvoiceMetadata {
        customer_name: String::from_str(&env, "Customer A"),
        customer_address: String::from_str(&env, "Address A"),
        tax_id: String::from_str(&env, "Tax A"),
        line_items,
        notes: String::from_str(&env, "Notes A"),
    };
    client.update_invoice_metadata(&invoice_id, &metadata);

    // targeted auth mocking: Business A signs
    env.mock_auths(&[MockAuth {
        address: &business_a,
        invoke: &MockAuthInvoke {
            contract: &contract_id,
            fn_name: "clear_invoice_metadata",
            args: (invoice_id.clone(),).into_val(&env),
            sub_invokes: &[],
        },
    }]);

    let result = client.try_clear_invoice_metadata(&invoice_id);
    assert!(
        result.is_ok(),
        "Business owner must succeed clearing own metadata"
    );

    let invoice = client.get_invoice(&invoice_id);
    assert!(invoice.metadata().is_none());
}

/// Sad path: Verification that different Business B cannot cancel
/// Business A's invoice.
#[test]
fn test_rejects_invoice_cancellation_by_different_business() {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);

    env.mock_all_auths();
    let _ = client.try_initialize_admin(&admin);
    client.set_admin(&admin);

    let business_a = setup_verified_business(&env, &client, &admin);
    let business_b = setup_verified_business(&env, &client, &admin);
    let invoice_id = create_test_invoice(&env, &client, &business_a, 100_000);

    // targeted auth mocking: only Business B signs
    env.mock_auths(&[MockAuth {
        address: &business_b,
        invoke: &MockAuthInvoke {
            contract: &contract_id,
            fn_name: "cancel_invoice",
            args: (invoice_id.clone(),).into_val(&env),
            sub_invokes: &[],
        },
    }]);

    let result = client.try_cancel_invoice(&invoice_id);
    assert!(
        result.is_err(),
        "Different business B must not cancel invoice"
    );
}

/// Happy path: Verification that Business A can successfully cancel their
/// own invoice.
#[test]
fn test_allows_invoice_cancellation_by_owner_business() {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);

    env.mock_all_auths();
    let _ = client.try_initialize_admin(&admin);
    client.set_admin(&admin);

    let business_a = setup_verified_business(&env, &client, &admin);
    let invoice_id = create_test_invoice(&env, &client, &business_a, 100_000);

    // targeted auth mocking: Business A signs
    env.mock_auths(&[MockAuth {
        address: &business_a,
        invoke: &MockAuthInvoke {
            contract: &contract_id,
            fn_name: "cancel_invoice",
            args: (invoice_id.clone(),).into_val(&env),
            sub_invokes: &[],
        },
    }]);

    let result = client.try_cancel_invoice(&invoice_id);
    assert!(
        result.is_ok(),
        "Business owner must succeed cancelling own invoice"
    );

    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Cancelled);
}
