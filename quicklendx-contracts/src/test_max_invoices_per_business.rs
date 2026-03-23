#![cfg(test)]

use crate::{
    invoice::{Invoice, InvoiceCategory, InvoiceStatus, InvoiceStorage},
    protocol_limits::ProtocolLimitsContract,
    verification::{BusinessVerificationStatus, BusinessVerificationStorage},
    QuickLendXContract, QuickLendXContractClient, QuickLendXError,
    init::InitializationParams,
};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Env, String, Vec, vec,
};

fn setup(env: &Env) -> (QuickLendXContractClient, Address, Address, Address, Address) {
    env.mock_all_auths();

    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    let treasury = Address::generate(&env);

    // Initialize contract
    let params = InitializationParams {
        admin: admin.clone(),
        treasury: treasury.clone(),
        fee_bps: 200,
        min_invoice_amount: 10,
        max_due_date_days: 365,
        grace_period_seconds: 7 * 24 * 60 * 60,
        initial_currencies: vec![&env, currency.clone()],
    };
    client.initialize(&params);

    // Initialize protocol limits
    client.initialize_protocol_limits(&admin, &10, &365, &(7 * 24 * 60 * 60));

    // Verify business
    env.as_contract(&contract_id, || {
        BusinessVerificationStorage::set_verification_status(
            &env,
            &business,
            BusinessVerificationStatus::Verified,
        );
    });

    (client, admin, business, currency, contract_id)
}

fn create_invoice_params(env: &Env) -> (i128, u64, String, InvoiceCategory, Vec<String>) {
    let amount = 1000i128;
    let due_date = env.ledger().timestamp() + 86400;
    let description = String::from_str(&env, "Test invoice");
    let category = InvoiceCategory::Services;
    let tags = Vec::new(&env);
    (amount, due_date, description, category, tags)
}

// ============================================================================
// TEST 1: Create invoices up to limit (succeed)
// ============================================================================

#[test]
fn test_create_invoices_up_to_limit_succeeds() {
    let env = Env::default();
    let (client, admin, business, currency, contract_id) = setup(&env);

    // Set limit to 5 invoices per business
    client.update_limits_max_invoices(&admin, &10, &365, &86400, &5);

    let (amount, due_date, description, category, tags) = create_invoice_params(&env);

    // Create 5 invoices - all should succeed
    for i in 0..5 {
        let desc = String::from_str(&env, "Invoice");
        let result = client.try_upload_invoice(
            &business, &amount, &currency, &due_date, &desc, &category, &tags,
        );
        assert!(result.is_ok(), "Invoice {} should succeed", i);
    }

    // Verify active count
    let active_count = env.as_contract(&contract_id, || {
        InvoiceStorage::count_active_business_invoices(&env, &business)
    });
    assert_eq!(active_count, 5, "Should have 5 active invoices");
}

// ============================================================================
// TEST 2: Next invoice fails with clear error
// ============================================================================

#[test]
fn test_next_invoice_after_limit_fails_with_clear_error() {
    let env = Env::default();
    let (client, admin, business, currency, _contract_id) = setup(&env);

    // Set limit to 3 invoices per business
    client.update_limits_max_invoices(&admin, &10, &365, &86400, &3);

    let (amount, due_date, description, category, tags) = create_invoice_params(&env);

    // Create 3 invoices successfully
    for _i in 0..3 {
        let desc = String::from_str(&env, "Invoice");
        client.upload_invoice(
            &business, &amount, &currency, &due_date, &desc, &category, &tags,
        );
    }

    // 4th invoice should fail with MaxInvoicesPerBusinessExceeded error
    let result = client.try_upload_invoice(
        &business,
        &amount,
        &currency,
        &due_date,
        &description,
        &category,
        &tags,
    );
    
    assert!(result.is_err(), "4th invoice should fail");
    let err = result.err().expect("Expected error result");
    if let Ok(contract_err) = err {
        assert_eq!(
            contract_err,
            QuickLendXError::MaxInvoicesPerBusinessExceeded,
            "Should return MaxInvoicesPerBusinessExceeded error"
        );
    } else {
        panic!("Expected contract error, got {:?}", err);
    }
}

// ============================================================================
// TEST 3: Cancelled invoices free up slots
// ============================================================================

#[test]
fn test_cancelled_invoices_free_slot() {
    let env = Env::default();
    let (client, admin, business, currency, contract_id) = setup(&env);

    // Set limit to 2 invoices per business
    client.update_limits_max_invoices(&admin, &10, &365, &86400, &2);

    let (amount, due_date, description, category, tags) = create_invoice_params(&env);

    // Create 2 invoices
    let invoice1_id = client.upload_invoice(
        &business,
        &amount,
        &currency,
        &due_date,
        &description,
        &category,
        &tags,
    );
    let _invoice2_id = client.upload_invoice(
        &business,
        &amount,
        &currency,
        &due_date,
        &description,
        &category,
        &tags,
    );

    // Verify limit is reached
    let result = client.try_upload_invoice(
        &business,
        &amount,
        &currency,
        &due_date,
        &description,
        &category,
        &tags,
    );
    assert!(result.is_err(), "3rd invoice should fail");

    // Cancel first invoice
    client.cancel_invoice(&invoice1_id);

    // Verify invoice is cancelled
    let invoice1 = env.as_contract(&contract_id, || {
        InvoiceStorage::get_invoice(&env, &invoice1_id).unwrap()
    });
    assert_eq!(invoice1.status, InvoiceStatus::Cancelled);

    // Now should be able to create a new invoice
    let result = client.try_upload_invoice(
        &business,
        &amount,
        &currency,
        &due_date,
        &description,
        &category,
        &tags,
    );
    assert!(result.is_ok(), "Should be able to create invoice after cancellation");

    // Verify active count is 2
    let active_count = env.as_contract(&contract_id, || {
        InvoiceStorage::count_active_business_invoices(&env, &business)
    });
    assert_eq!(active_count, 2, "Should have 2 active invoices");
}

// ============================================================================
// TEST 4: Paid invoices free up slots
// ============================================================================

#[test]
fn test_paid_invoices_free_slot() {
    let env = Env::default();
    let (client, admin, business, currency, contract_id) = setup(&env);

    // Set limit to 2 invoices per business
    client.update_limits_max_invoices(&admin, &10, &365, &86400, &2);

    let (amount, due_date, description, category, tags) = create_invoice_params(&env);

    // Create 2 invoices
    let invoice1_id = client.upload_invoice(
        &business,
        &amount,
        &currency,
        &due_date,
        &description,
        &category,
        &tags,
    );
    client.upload_invoice(
        &business,
        &amount,
        &currency,
        &due_date,
        &description,
        &category,
        &tags,
    );

    // Mark first invoice as paid (simulate payment flow)
    env.as_contract(&contract_id, || {
        let mut invoice1 = InvoiceStorage::get_invoice(&env, &invoice1_id).unwrap();
        invoice1.mark_as_paid(&env, business.clone(), env.ledger().timestamp());
        InvoiceStorage::update_invoice(&env, &invoice1);
    });

    // Verify invoice is paid
    let invoice1 = env.as_contract(&contract_id, || {
        InvoiceStorage::get_invoice(&env, &invoice1_id).unwrap()
    });
    assert_eq!(invoice1.status, InvoiceStatus::Paid);

    // Now should be able to create a new invoice
    let result = client.try_upload_invoice(
        &business,
        &amount,
        &currency,
        &due_date,
        &description,
        &category,
        &tags,
    );
    assert!(
        result.is_ok(),
        "Should be able to create invoice after one is paid"
    );

    // Verify active count is 2
    let active_count = env.as_contract(&contract_id, || {
        InvoiceStorage::count_active_business_invoices(&env, &business)
    });
    assert_eq!(active_count, 2, "Should have 2 active invoices");
}

// ============================================================================
// TEST 5: Config update changes limit
// ============================================================================

#[test]
fn test_config_update_changes_limit() {
    let env = Env::default();
    let (client, admin, business, currency, _contract_id) = setup(&env);

    // Start with limit of 2
    client.update_limits_max_invoices(&admin, &10, &365, &86400, &2);

    let (amount, due_date, description, category, tags) = create_invoice_params(&env);

    // Create 2 invoices
    client.upload_invoice(
        &business,
        &amount,
        &currency,
        &due_date,
        &description,
        &category,
        &tags,
    );
    client.upload_invoice(
        &business,
        &amount,
        &currency,
        &due_date,
        &description,
        &category,
        &tags,
    );

    // 3rd should fail
    let result = client.try_upload_invoice(
        &business,
        &amount,
        &currency,
        &due_date,
        &description,
        &category,
        &tags,
    );
    assert!(result.is_err(), "3rd invoice should fail with limit of 2");

    // Update limit to 5
    client.update_limits_max_invoices(&admin, &10, &365, &86400, &5);

    // Verify limit was updated
    let limits = client.get_protocol_limits();
    assert_eq!(limits.max_invoices_per_business, 5);

    // Now 3rd invoice should succeed
    let result = client.try_upload_invoice(
        &business,
        &amount,
        &currency,
        &due_date,
        &description,
        &category,
        &tags,
    );
    assert!(result.is_ok(), "3rd invoice should succeed with limit of 5");

    // Create 2 more to reach new limit
    client.upload_invoice(
        &business,
        &amount,
        &currency,
        &due_date,
        &description,
        &category,
        &tags,
    );
    client.upload_invoice(
        &business,
        &amount,
        &currency,
        &due_date,
        &description,
        &category,
        &tags,
    );

    // 6th should fail
    let result = client.try_upload_invoice(
        &business,
        &amount,
        &currency,
        &due_date,
        &description,
        &category,
        &tags,
    );
    assert!(result.is_err(), "6th invoice should fail with limit of 5");
}

// ============================================================================
// TEST 6: Limit of 0 means unlimited
// ============================================================================

#[test]
fn test_limit_zero_means_unlimited() {
    let env = Env::default();
    let (client, admin, business, currency, contract_id) = setup(&env);

    // Set limit to 0 (unlimited)
    client.update_limits_max_invoices(&admin, &10, &365, &86400, &0);

    let (amount, due_date, description, category, tags) = create_invoice_params(&env);

    // Create 10 invoices - all should succeed
    for i in 0..10 {
        let desc = String::from_str(&env, "Invoice");
        let result = client.try_upload_invoice(
            &business, &amount, &currency, &due_date, &desc, &category, &tags,
        );
        assert!(
            result.is_ok(),
            "Invoice {} should succeed with unlimited limit",
            i
        );
    }

    let active_count = env.as_contract(&contract_id, || {
        InvoiceStorage::count_active_business_invoices(&env, &business)
    });
    assert_eq!(active_count, 10, "Should have 10 active invoices");
}

// ============================================================================
// TEST 7: Multiple businesses have independent limits
// ============================================================================

#[test]
fn test_multiple_businesses_independent_limits() {
    let env = Env::default();
    let (client, admin, business1, currency, contract_id) = setup(&env);
    let business2 = Address::generate(&env);

    // Verify business2
    env.as_contract(&contract_id, || {
        BusinessVerificationStorage::set_verification_status(
            &env,
            &business2,
            BusinessVerificationStatus::Verified,
        );
    });

    // Set limit to 2 invoices per business
    client.update_limits_max_invoices(&admin, &10, &365, &86400, &2);

    let (amount, due_date, description, category, tags) = create_invoice_params(&env);

    // Business1 creates 2 invoices
    client.upload_invoice(
        &business1,
        &amount,
        &currency,
        &due_date,
        &description,
        &category,
        &tags,
    );
    client.upload_invoice(
        &business1,
        &amount,
        &currency,
        &due_date,
        &description,
        &category,
        &tags,
    );

    // Business1's 3rd invoice should fail
    let result = client.try_upload_invoice(
        &business1,
        &amount,
        &currency,
        &due_date,
        &description,
        &category,
        &tags,
    );
    assert!(result.is_err(), "Business1's 3rd invoice should fail");

    // Business2 should still be able to create 2 invoices
    client.upload_invoice(
        &business2,
        &amount,
        &currency,
        &due_date,
        &description,
        &category,
        &tags,
    );
    client.upload_invoice(
        &business2,
        &amount,
        &currency,
        &due_date,
        &description,
        &category,
        &tags,
    );

    // Verify counts
    let (business1_count, business2_count) = env.as_contract(&contract_id, || {
        (
            InvoiceStorage::count_active_business_invoices(&env, &business1),
            InvoiceStorage::count_active_business_invoices(&env, &business2),
        )
    });
    assert_eq!(business1_count, 2);
    assert_eq!(business2_count, 2);
}

// ============================================================================
// TEST 8: Only active invoices count toward limit
// ============================================================================

#[test]
fn test_only_active_invoices_count_toward_limit() {
    let env = Env::default();
    let (client, admin, business, currency, contract_id) = setup(&env);

    // Set limit to 3
    client.update_limits_max_invoices(&admin, &10, &365, &86400, &3);

    let (amount, due_date, description, category, tags) = create_invoice_params(&env);

    // Create 3 invoices
    let invoice1_id = client.upload_invoice(
        &business,
        &amount,
        &currency,
        &due_date,
        &description,
        &category,
        &tags,
    );
    let invoice2_id = client.upload_invoice(
        &business,
        &amount,
        &currency,
        &due_date,
        &description,
        &category,
        &tags,
    );
    let _invoice3_id = client.upload_invoice(
        &business,
        &amount,
        &currency,
        &due_date,
        &description,
        &category,
        &tags,
    );

    // Cancel one and mark one as paid
    client.cancel_invoice(&invoice1_id);
    env.as_contract(&contract_id, || {
        let mut invoice2 = InvoiceStorage::get_invoice(&env, &invoice2_id).unwrap();
        invoice2.mark_as_paid(&env, business.clone(), env.ledger().timestamp());
        InvoiceStorage::update_invoice(&env, &invoice2);
    });

    // Active count should be 1 (only invoice3)
    let active_count = env.as_contract(&contract_id, || {
        InvoiceStorage::count_active_business_invoices(&env, &business)
    });
    assert_eq!(active_count, 1, "Should have 1 active invoice");

    // Should be able to create 2 more invoices
    client.upload_invoice(
        &business,
        &amount,
        &currency,
        &due_date,
        &description,
        &category,
        &tags,
    );
    client.upload_invoice(
        &business,
        &amount,
        &currency,
        &due_date,
        &description,
        &category,
        &tags,
    );

    // Active count should now be 3
    let active_count = env.as_contract(&contract_id, || {
        InvoiceStorage::count_active_business_invoices(&env, &business)
    });
    assert_eq!(active_count, 3, "Should have 3 active invoices");

    // 4th should fail
    let result = client.try_upload_invoice(
        &business,
        &amount,
        &currency,
        &due_date,
        &description,
        &category,
        &tags,
    );
    assert!(result.is_err(), "4th active invoice should fail");
}

// ============================================================================
// TEST 9: Verified, Funded, Defaulted, Refunded invoices count as active
// ============================================================================

#[test]
fn test_various_statuses_count_as_active() {
    let env = Env::default();
    let (client, admin, business, currency, contract_id) = setup(&env);

    // Set limit to 5
    client.update_limits_max_invoices(&admin, &10, &365, &86400, &5);

    let (amount, due_date, description, category, tags) = create_invoice_params(&env);

    // Create 5 invoices
    let mut ids = Vec::new(&env);
    for _ in 0..5 {
        ids.push_back(client.upload_invoice(
            &business,
            &amount,
            &currency,
            &due_date,
            &description,
            &category,
            &tags,
        ));
    }

    // Set different statuses (all should count as active except Cancelled and Paid)
    env.as_contract(&contract_id, || {
        // Invoice 0: Pending (default)
        // Invoice 1: Verified
        let mut invoice1 = InvoiceStorage::get_invoice(&env, &ids.get(1).unwrap()).unwrap();
        invoice1.verify(&env, admin.clone());
        InvoiceStorage::update_invoice(&env, &invoice1);

        // Invoice 2: Funded
        let mut invoice2 = InvoiceStorage::get_invoice(&env, &ids.get(2).unwrap()).unwrap();
        invoice2.mark_as_funded(&env, Address::generate(&env), amount, env.ledger().timestamp());
        InvoiceStorage::update_invoice(&env, &invoice2);

        // Invoice 3: Defaulted
        let mut invoice3 = InvoiceStorage::get_invoice(&env, &ids.get(3).unwrap()).unwrap();
        invoice3.mark_as_defaulted();
        InvoiceStorage::update_invoice(&env, &invoice3);

        // Invoice 4: Refunded
        let mut invoice4 = InvoiceStorage::get_invoice(&env, &ids.get(4).unwrap()).unwrap();
        invoice4.mark_as_refunded(&env, admin.clone());
        InvoiceStorage::update_invoice(&env, &invoice4);
    });

    // All 5 should count as active
    let active_count = env.as_contract(&contract_id, || {
        InvoiceStorage::count_active_business_invoices(&env, &business)
    });
    assert_eq!(active_count, 5, "All 5 invoices should count as active");

    // Cannot create another invoice
    let result = client.try_upload_invoice(
        &business,
        &amount,
        &currency,
        &due_date,
        &description,
        &category,
        &tags,
    );
    assert!(result.is_err(), "6th invoice should fail");
}

// ============================================================================
// TEST 10: Edge case - limit of 1
// ============================================================================

#[test]
fn test_limit_of_one() {
    let env = Env::default();
    let (client, admin, business, currency, _contract_id) = setup(&env);

    // Set limit to 1
    client.update_limits_max_invoices(&admin, &10, &365, &86400, &1);

    let (amount, due_date, description, category, tags) = create_invoice_params(&env);

    // First invoice succeeds
    let invoice1_id = client.upload_invoice(
        &business,
        &amount,
        &currency,
        &due_date,
        &description,
        &category,
        &tags,
    );

    // Second invoice fails
    let result = client.try_upload_invoice(
        &business,
        &amount,
        &currency,
        &due_date,
        &description,
        &category,
        &tags,
    );
    assert!(result.is_err(), "2nd invoice should fail with limit of 1");

    // Cancel first invoice
    client.cancel_invoice(&invoice1_id);

    // Now can create another
    let result = client.try_upload_invoice(
        &business,
        &amount,
        &currency,
        &due_date,
        &description,
        &category,
        &tags,
    );
    assert!(
        result.is_ok(),
        "Should be able to create invoice after cancellation"
    );
}
