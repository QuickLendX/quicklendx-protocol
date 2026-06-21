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

    // We specifically DO NOT use mock_all_auths() here to test real enforcement.
    // Instead, we mock auth for the WRONG user.
    env.mock_auths(&[
        MockAuth {
            address: &malicious_user,
            invoke: &MockAuthInvoke {
                contract: &contract_id,
                fn_name: "add_tag",
                args: (invoice_id.clone(), new_tag.clone()).into_val(&env),
                sub_invokes: &[],
            },
        },
    ]);

    // This should fail because the contract expects 'business' to sign, not 'malicious_user'
    let result = client.try_add_tag(&invoice_id, &new_tag);
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

    // Mock auth for the CORRECT user
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

/// Test that the Invoice::cancel method enforces authorization:
/// only the business owner can cancel their own invoice.
#[test]
fn test_invoice_cancel_authorization() {
    use crate::invoice::Invoice;
    use crate::errors::QuickLendXError;

    let env = Env::default();
    let business = Address::generate(&env);
    let attacker = Address::generate(&env);
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;
    let description = String::from_str(&env, "Test invoice");
    let category = InvoiceCategory::Services;
    let tags = Vec::new(&env);

    // Create an invoice owned by business
    let mut invoice = Invoice::new(
        &env,
        business.clone(),
        10_000,
        currency,
        due_date,
        description,
        category,
        tags,
    ).expect("Invoice creation should succeed");

    // Attempt to cancel as attacker (not business owner) - should fail
    let result = invoice.cancel(&env, attacker);
    assert_eq!(
        result.unwrap_err(),
        QuickLendXError::Unauthorized,
        "Non-owner cannot cancel invoice"
    );

    // Invoice status should remain unchanged
    assert_eq!(invoice.status, InvoiceStatus::Pending);

    // Cancel as business owner - should succeed
    let result = invoice.cancel(&env, business.clone());
    assert!(result.is_ok(), "Business owner can cancel invoice");

    // Invoice status should be Cancelled
    assert_eq!(invoice.status, InvoiceStatus::Cancelled);
}

/// Test that the Invoice::cancel method has no state preconditions:
/// it cancels regardless of current status (authorization is the only check).
///
/// Note: The public cancel_invoice function in lib.rs has additional state
/// preconditions (only Pending or Verified can be cancelled), but the internal
/// Invoice::cancel method only checks authorization.
#[test]
fn test_invoice_cancel_no_state_preconditions() {
    use crate::invoice::Invoice;

    let env = Env::default();
    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;
    let description = String::from_str(&env, "Test invoice");
    let category = InvoiceCategory::Services;
    let tags = Vec::new(&env);

    // Test cancellation from various states
    let test_states = vec![
        InvoiceStatus::Pending,
        InvoiceStatus::Verified,
        InvoiceStatus::Funded,
        InvoiceStatus::Paid,
        InvoiceStatus::Defaulted,
    ];

    for status in test_states {
        let mut invoice = Invoice::new(
            &env,
            business.clone(),
            10_000,
            currency,
            due_date,
            description.clone(),
            category,
            tags.clone(),
        ).expect("Invoice creation should succeed");

        // Set the invoice to the test state
        invoice.status = status.clone();

        // Cancel should succeed regardless of state (only authorization matters)
        let result = invoice.cancel(&env, business.clone());
        assert!(result.is_ok(), "Cancel should succeed from {} state", format!("{:?}", status));

        // Status should be Cancelled
        assert_eq!(invoice.status, InvoiceStatus::Cancelled);
    }
}