#![cfg(test)]

use crate::{
    invoice::{Invoice, InvoiceCategory, InvoiceStatus, InvoiceStorage},
    protocol_limits::ProtocolLimitsContract,
    verification::{BusinessVerificationStatus, BusinessVerificationStorage},
    QuickLendXContract, QuickLendXContractClient, QuickLendXError,
};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Env, String, Vec,
};

fn setup() -> (Env, QuickLendXContractClient<'static>, Address, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let business = Address::generate(&env);
    let currency = Address::generate(&env);

    // Initialize contract
    client.initialize(&admin);

    // Add currency to whitelist
    client.add_currency(&admin, &currency);

    // Verify business
    BusinessVerificationStorage::set_verification_status(
        &env,
        &business,
        BusinessVerificationStatus::Verified,
    );

    (env, client, admin, business, currency)
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
    let (env, client, admin, business, currency) = setup();

    // Set limit to 5 invoices per business
    client
        .update_limits_max_invoices(&admin, &10, &365, &86400, &5);

    let (amount, due_date, description, category, tags) = create_invoice_params(&env);

    // Create 5 invoices - all should succeed
    for i in 0..5 {
        let desc = String::from_str(&env, "Invoice");
        client.upload_invoice(
            &business, &amount, &currency, &due_date, &desc, &category, &tags,
        );
    }

    // Verify all 5 invoices were created
    let business_invoices = InvoiceStorage::get_business_invoices(&env, &business);
    assert_eq!(business_invoices.len(), 5, "Should have 5 invoices");

    // Verify active count
    let active_count = InvoiceStorage::count_active_business_invoices(&env, &business);
    assert_eq!(active_count, 5, "Should have 5 active invoices");
}

// ============================================================================
// TEST 2: Next invoice fails with clear error
// ============================================================================

#[test]
fn test_next_invoice_after_limit_fails_with_clear_error() {
    let (env, client, admin, business, currency) = setup();

    // Set limit to 3 invoices per business
    client
        .update_limits_max_invoices(&admin, &10, &365, &86400, &3);

    let (amount, due_date, description, category, tags) = create_invoice_params(&env);

    // Create 3 invoices successfully
    for _ in 0..3 {
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
}

// ============================================================================
// TEST 3: Cancelled invoices free up slots
// ============================================================================

#[test]
fn test_cancelled_invoices_free_slot() {
    let (env, client, admin, business, currency) = setup();

    // Set limit to 2 invoices per business
    client
        .update_limits_max_invoices(&admin, &10, &365, &86400, &2)
        .unwrap();

    let (amount, due_date, description, category, tags) = create_invoice_params(&env);

    // Create 2 invoices
    let invoice1_id = client
        .upload_invoice(
            &business,
            &amount,
            &currency,
            &due_date,
            &description,
            &category,
            &tags,
        )
        .unwrap();
    let invoice2_id = client
        .upload_invoice(
            &business,
            &amount,
            &currency,
            &due_date,
            &description,
            &category,
            &tags,
        )
        .unwrap();

    // Verify limit is reached
    let result = client.upload_invoice(
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
    client.cancel_invoice(&business, &invoice1_id).unwrap();

    // Verify invoice is cancelled
    let invoice1 = InvoiceStorage::get_invoice(&env, &invoice1_id).unwrap();
    assert_eq!(invoice1.status, InvoiceStatus::Cancelled);

    // Now should be able to create a new invoice
    let invoice3_id = client
        .upload_invoice(
            &business,
            &amount,
            &currency,
            &due_date,
            &description,
            &category,
            &tags,
        )
        .unwrap();

    assert!(invoice3_id != invoice1_id && invoice3_id != invoice2_id);

    // Verify active count is 2 (invoice2 and invoice3)
    let active_count = InvoiceStorage::count_active_business_invoices(&env, &business);
    assert_eq!(active_count, 2, "Should have 2 active invoices");
}

// ============================================================================
// TEST 4: Paid invoices free up slots
// ============================================================================

#[test]
fn test_paid_invoices_free_slot() {
    let (env, client, admin, business, currency) = setup();

    // Set limit to 2 invoices per business
    client
        .update_limits_max_invoices(&admin, &10, &365, &86400, &2)
        .unwrap();

    let (amount, due_date, description, category, tags) = create_invoice_params(&env);

    // Create 2 invoices
    let invoice1_id = client
        .upload_invoice(
            &business,
            &amount,
            &currency,
            &due_date,
            &description,
            &category,
            &tags,
        )
        .unwrap();
    client
        .upload_invoice(
            &business,
            &amount,
            &currency,
            &due_date,
            &description,
            &category,
            &tags,
        )
        .unwrap();

    // Mark first invoice as paid (simulate payment flow)
    let mut invoice1 = InvoiceStorage::get_invoice(&env, &invoice1_id).unwrap();
    invoice1.mark_as_paid(&env, business.clone(), env.ledger().timestamp());
    InvoiceStorage::update_invoice(&env, &invoice1);

    // Verify invoice is paid
    let invoice1 = InvoiceStorage::get_invoice(&env, &invoice1_id).unwrap();
    assert_eq!(invoice1.status, InvoiceStatus::Paid);

    // Now should be able to create a new invoice
    let result = client.upload_invoice(
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
    let active_count = InvoiceStorage::count_active_business_invoices(&env, &business);
    assert_eq!(active_count, 2, "Should have 2 active invoices");
}

// ============================================================================
// TEST 5: Config update changes limit
// ============================================================================

#[test]
fn test_config_update_changes_limit() {
    let (env, client, admin, business, currency) = setup();

    // Start with limit of 2
    client
        .update_limits_max_invoices(&admin, &10, &365, &86400, &2)
        .unwrap();

    let (amount, due_date, description, category, tags) = create_invoice_params(&env);

    // Create 2 invoices
    client
        .upload_invoice(
            &business,
            &amount,
            &currency,
            &due_date,
            &description,
            &category,
            &tags,
        )
        .unwrap();
    client
        .upload_invoice(
            &business,
            &amount,
            &currency,
            &due_date,
            &description,
            &category,
            &tags,
        )
        .unwrap();

    // 3rd should fail
    let result = client.upload_invoice(
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
    client
        .update_limits_max_invoices(&admin, &10, &365, &86400, &5)
        .unwrap();

    // Verify limit was updated
    let limits = client.get_protocol_limits();
    assert_eq!(limits.max_invoices_per_business, 5);

    // Now 3rd invoice should succeed
    let result = client.upload_invoice(
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
    client
        .upload_invoice(
            &business,
            &amount,
            &currency,
            &due_date,
            &description,
            &category,
            &tags,
        )
        .unwrap();
    client
        .upload_invoice(
            &business,
            &amount,
            &currency,
            &due_date,
            &description,
            &category,
            &tags,
        )
        .unwrap();

    // 6th should fail
    let result = client.upload_invoice(
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
    let (env, client, admin, business, currency) = setup();

    // Set limit to 0 (unlimited)
    client
        .update_limits_max_invoices(&admin, &10, &365, &86400, &0)
        .unwrap();

    let (amount, due_date, description, category, tags) = create_invoice_params(&env);

    // Create 10 invoices - all should succeed
    for i in 0..10 {
        let desc = String::from_str(&env, "Invoice");
        let result = client.upload_invoice(
            &business, &amount, &currency, &due_date, &desc, &category, &tags,
        );
        assert!(
            result.is_ok(),
            "Invoice {} should succeed with unlimited limit",
            i
        );
    }

    let active_count = InvoiceStorage::count_active_business_invoices(&env, &business);
    assert_eq!(active_count, 10, "Should have 10 active invoices");
}

// ============================================================================
// TEST 7: Multiple businesses have independent limits
// ============================================================================

#[test]
fn test_multiple_businesses_independent_limits() {
    let (env, client, admin, business1, currency) = setup();
    let business2 = Address::generate(&env);

    // Verify business2
    BusinessVerificationStorage::set_verification_status(
        &env,
        &business2,
        BusinessVerificationStatus::Verified,
    );

    // Set limit to 2 invoices per business
    client
        .update_limits_max_invoices(&admin, &10, &365, &86400, &2)
        .unwrap();

    let (amount, due_date, description, category, tags) = create_invoice_params(&env);

    // Business1 creates 2 invoices
    client
        .upload_invoice(
            &business1,
            &amount,
            &currency,
            &due_date,
            &description,
            &category,
            &tags,
        )
        .unwrap();
    client
        .upload_invoice(
            &business1,
            &amount,
            &currency,
            &due_date,
            &description,
            &category,
            &tags,
        )
        .unwrap();

    // Business1's 3rd invoice should fail
    let result = client.upload_invoice(
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
    client
        .upload_invoice(
            &business2,
            &amount,
            &currency,
            &due_date,
            &description,
            &category,
            &tags,
        )
        .unwrap();
    client
        .upload_invoice(
            &business2,
            &amount,
            &currency,
            &due_date,
            &description,
            &category,
            &tags,
        )
        .unwrap();

    // Verify counts
    let business1_count = InvoiceStorage::count_active_business_invoices(&env, &business1);
    let business2_count = InvoiceStorage::count_active_business_invoices(&env, &business2);
    assert_eq!(business1_count, 2);
    assert_eq!(business2_count, 2);
}

// ============================================================================
// TEST 8: Only active invoices count toward limit
// ============================================================================

#[test]
fn test_only_active_invoices_count_toward_limit() {
    let (env, client, admin, business, currency) = setup();

    // Set limit to 3
    client
        .update_limits_max_invoices(&admin, &10, &365, &86400, &3)
        .unwrap();

    let (amount, due_date, description, category, tags) = create_invoice_params(&env);

    // Create 3 invoices
    let invoice1_id = client
        .upload_invoice(
            &business,
            &amount,
            &currency,
            &due_date,
            &description,
            &category,
            &tags,
        )
        .unwrap();
    let invoice2_id = client
        .upload_invoice(
            &business,
            &amount,
            &currency,
            &due_date,
            &description,
            &category,
            &tags,
        )
        .unwrap();
    let invoice3_id = client
        .upload_invoice(
            &business,
            &amount,
            &currency,
            &due_date,
            &description,
            &category,
            &tags,
        )
        .unwrap();

    // Cancel one and mark one as paid
    client.cancel_invoice(&business, &invoice1_id).unwrap();
    let mut invoice2 = InvoiceStorage::get_invoice(&env, &invoice2_id).unwrap();
    invoice2.mark_as_paid(&env, business.clone(), env.ledger().timestamp());
    InvoiceStorage::update_invoice(&env, &invoice2);

    // Active count should be 1 (only invoice3)
    let active_count = InvoiceStorage::count_active_business_invoices(&env, &business);
    assert_eq!(active_count, 1, "Should have 1 active invoice");

    // Should be able to create 2 more invoices
    client
        .upload_invoice(
            &business,
            &amount,
            &currency,
            &due_date,
            &description,
            &category,
            &tags,
        )
        .unwrap();
    client
        .upload_invoice(
            &business,
            &amount,
            &currency,
            &due_date,
            &description,
            &category,
            &tags,
        )
        .unwrap();

    // Active count should now be 3
    let active_count = InvoiceStorage::count_active_business_invoices(&env, &business);
    assert_eq!(active_count, 3, "Should have 3 active invoices");

    // 4th should fail
    let result = client.upload_invoice(
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
    let (env, client, admin, business, currency) = setup();

    // Set limit to 5
    client
        .update_limits_max_invoices(&admin, &10, &365, &86400, &5)
        .unwrap();

    let (amount, due_date, description, category, tags) = create_invoice_params(&env);

    // Create 5 invoices
    let ids: Vec<_> = (0..5)
        .map(|_| {
            client
                .upload_invoice(
                    &business,
                    &amount,
                    &currency,
                    &due_date,
                    &description,
                    &category,
                    &tags,
                )
                .unwrap()
        })
        .collect();

    // Set different statuses (all should count as active except Cancelled and Paid)
    // Invoice 0: Pending (default)
    // Invoice 1: Verified
    let mut invoice1 = InvoiceStorage::get_invoice(&env, &ids[1]).unwrap();
    invoice1.verify(&env, admin.clone());
    InvoiceStorage::update_invoice(&env, &invoice1);

    // Invoice 2: Funded
    let mut invoice2 = InvoiceStorage::get_invoice(&env, &ids[2]).unwrap();
    invoice2.mark_as_funded(&env, Address::generate(&env), amount);
    InvoiceStorage::update_invoice(&env, &invoice2);

    // Invoice 3: Defaulted
    let mut invoice3 = InvoiceStorage::get_invoice(&env, &ids[3]).unwrap();
    invoice3.mark_as_defaulted();
    InvoiceStorage::update_invoice(&env, &invoice3);

    // Invoice 4: Refunded
    let mut invoice4 = InvoiceStorage::get_invoice(&env, &ids[4]).unwrap();
    invoice4.mark_as_refunded(&env, admin.clone());
    InvoiceStorage::update_invoice(&env, &invoice4);

    // All 5 should count as active
    let active_count = InvoiceStorage::count_active_business_invoices(&env, &business);
    assert_eq!(active_count, 5, "All 5 invoices should count as active");

    // Cannot create another invoice
    let result = client.upload_invoice(
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
    let (env, client, admin, business, currency) = setup();

    // Set limit to 1
    client
        .update_limits_max_invoices(&admin, &10, &365, &86400, &1)
        .unwrap();

    let (amount, due_date, description, category, tags) = create_invoice_params(&env);

    // First invoice succeeds
    let invoice1_id = client
        .upload_invoice(
            &business,
            &amount,
            &currency,
            &due_date,
            &description,
            &category,
            &tags,
        )
        .unwrap();

    // Second invoice fails
    let result = client.upload_invoice(
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
    client.cancel_invoice(&business, &invoice1_id).unwrap();

    // Now can create another
    let result = client.upload_invoice(
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
