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
                fn_name: "update_category",
                args: (invoice_id.clone(), InvoiceCategory::Healthcare).into_val(&env),
                sub_invokes: &[],
            },
        },
    ]);

    let result = client.try_update_category(&invoice_id, &InvoiceCategory::Healthcare);
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
                fn_name: "add_tag",
                args: (invoice_id.clone(), new_tag.clone()).into_val(&env),
                sub_invokes: &[],
            },
        },
    ]);

    let result = client.try_add_tag(&invoice_id, &new_tag);
    assert!(result.is_ok());
    
    let invoice = client.get_invoice(&invoice_id);
    assert!(invoice.tags.contains(new_tag));
}