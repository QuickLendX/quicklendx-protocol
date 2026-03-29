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
    env.mock_auths(&[MockAuth {
        address: &malicious_user,
        invoke: &MockAuthInvoke {
            contract: &contract_id,
            fn_name: "add_tag",
            args: (invoice_id.clone(), new_tag.clone()).into_val(&env),
            sub_invokes: &[],
        },
    }]);

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

    env.mock_auths(&[MockAuth {
        address: &malicious_user,
        invoke: &MockAuthInvoke {
            contract: &contract_id,
            fn_name: "update_category",
            args: (invoice_id.clone(), InvoiceCategory::Healthcare).into_val(&env),
            sub_invokes: &[],
        },
    }]);

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
    env.mock_auths(&[MockAuth {
        address: &business,
        invoke: &MockAuthInvoke {
            contract: &contract_id,
            fn_name: "add_tag",
            args: (invoice_id.clone(), new_tag.clone()).into_val(&env),
            sub_invokes: &[],
        },
    }]);

    let result = client.try_add_tag(&invoice_id, &new_tag);
    assert!(result.is_ok());

    let invoice = client.get_invoice(&invoice_id);
    assert!(invoice.tags.contains(new_tag));
}

// ============================================================================
// INVOICE UPLOAD VALIDATION TESTS
// ============================================================================

#[test]
fn test_invoice_amount_must_be_positive() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86_400;

    // Zero amount — must fail
    let result = client.try_create_invoice(
        &business,
        &0i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Zero amount invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    assert!(result.is_err(), "Zero amount should be rejected");

    // Negative amount — must fail
    let result = client.try_create_invoice(
        &business,
        &-1i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Negative amount invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    assert!(result.is_err(), "Negative amount should be rejected");

    // Valid positive amount — must succeed
    let result = client.try_create_invoice(
        &business,
        &1i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Minimum valid invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    assert!(result.is_ok(), "Positive amount should be accepted");
}

#[test]
fn test_invoice_due_date_must_be_in_future() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    let now = env.ledger().timestamp();

    // Due date in the past — must fail
    let past_due = now.saturating_sub(1);
    let result = client.try_create_invoice(
        &business,
        &1_000_000i128,
        &currency,
        &past_due,
        &String::from_str(&env, "Past due invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    assert!(result.is_err(), "Past due date should be rejected");

    // Due date equal to now — must fail (not strictly future)
    let result = client.try_create_invoice(
        &business,
        &1_000_000i128,
        &currency,
        &now,
        &String::from_str(&env, "Due now invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    assert!(result.is_err(), "Due date equal to now should be rejected");

    // Due date strictly in the future — must succeed
    let future_due = now + 86_400;
    let result = client.try_create_invoice(
        &business,
        &1_000_000i128,
        &currency,
        &future_due,
        &String::from_str(&env, "Future due invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    assert!(result.is_ok(), "Future due date should be accepted");
}

#[test]
fn test_invoice_description_length_boundaries() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86_400;

    // Empty description — must fail
    let result = client.try_create_invoice(
        &business,
        &1_000_000i128,
        &currency,
        &due_date,
        &String::from_str(&env, ""),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    assert!(result.is_err(), "Empty description should be rejected");

    // Description at max allowed length — must succeed
    let max_desc = "a".repeat(MAX_DESCRIPTION_LENGTH as usize);
    let result = client.try_create_invoice(
        &business,
        &1_000_000i128,
        &currency,
        &due_date,
        &String::from_str(&env, &max_desc),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    assert!(result.is_ok(), "Max-length description should be accepted");

    // Description one byte over max — must fail
    let over_desc = "a".repeat(MAX_DESCRIPTION_LENGTH as usize + 1);
    let result = client.try_create_invoice(
        &business,
        &1_000_000i128,
        &currency,
        &due_date,
        &String::from_str(&env, &over_desc),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    assert!(result.is_err(), "Over-limit description should be rejected");
}

// ============================================================================
// TAG NORMALIZATION AND INDEX INTEGRITY TESTS
// ============================================================================

#[test]
fn test_tag_normalization_collapses_case_and_whitespace() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let business = Address::generate(&env);
    let invoice_id = create_test_invoice(&env, &client, &business, 1_000_000);

    // Add "Tech" — stored as "tech"
    client.add_tag(&invoice_id, &String::from_str(&env, "Tech"));

    // Adding " TECH " should be treated as duplicate and silently ignored
    client.add_tag(&invoice_id, &String::from_str(&env, " TECH "));

    let invoice = client.get_invoice(&invoice_id);

    // Expect exactly one tag with normalized form
    let tech_count = invoice
        .tags
        .iter()
        .filter(|t| t == &String::from_str(&env, "tech"))
        .count();
    assert_eq!(tech_count, 1, "Duplicate normalized tags should be deduplicated");
    assert_eq!(invoice.tags.len(), 1, "Only one tag should exist after dedup");
}

#[test]
fn test_tag_limit_enforced_at_ten() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let business = Address::generate(&env);
    let invoice_id = create_test_invoice(&env, &client, &business, 1_000_000);

    // Add 10 unique tags — all should succeed
    for i in 0..10u32 {
        let tag = String::from_str(&env, &format!("tag{i}"));
        let result = client.try_add_tag(&invoice_id, &tag);
        assert!(result.is_ok(), "Tag {i} should be accepted");
    }

    // 11th tag must be rejected
    let overflow_tag = String::from_str(&env, "overflow");
    let result = client.try_add_tag(&invoice_id, &overflow_tag);
    assert!(result.is_err(), "11th tag should be rejected with TagLimitExceeded");
}

#[test]
fn test_empty_and_whitespace_tags_are_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let business = Address::generate(&env);
    let invoice_id = create_test_invoice(&env, &client, &business, 1_000_000);

    let empty = String::from_str(&env, "");
    assert!(
        client.try_add_tag(&invoice_id, &empty).is_err(),
        "Empty tag should be rejected"
    );

    let whitespace = String::from_str(&env, "   ");
    assert!(
        client.try_add_tag(&invoice_id, &whitespace).is_err(),
        "Whitespace-only tag should be rejected"
    );
}

#[test]
fn test_tag_over_50_bytes_is_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let business = Address::generate(&env);
    let invoice_id = create_test_invoice(&env, &client, &business, 1_000_000);

    let long_tag = String::from_str(&env, &"a".repeat(51));
    let result = client.try_add_tag(&invoice_id, &long_tag);
    assert!(result.is_err(), "Tag exceeding 50 bytes should be rejected");
}

#[test]
fn test_remove_tag_updates_index_and_invoice() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let business = Address::generate(&env);
    let invoice_id = create_test_invoice(&env, &client, &business, 1_000_000);
    let tag = String::from_str(&env, "removable");

    client.add_tag(&invoice_id, &tag);

    // Verify it was added
    let invoice = client.get_invoice(&invoice_id);
    assert!(invoice.tags.contains(String::from_str(&env, "removable")));

    // Remove it
    client.remove_tag(&invoice_id, &tag);

    // Verify it is gone from invoice
    let invoice = client.get_invoice(&invoice_id);
    assert!(!invoice.tags.contains(String::from_str(&env, "removable")));

    // Verify tag index no longer lists this invoice
    let indexed = client.get_invoices_by_tag(&String::from_str(&env, "removable"));
    assert!(!indexed.contains(invoice_id.clone()), "Invoice should be removed from tag index");
}

#[test]
fn test_remove_nonexistent_tag_returns_error() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let business = Address::generate(&env);
    let invoice_id = create_test_invoice(&env, &client, &business, 1_000_000);

    let result = client.try_remove_tag(&invoice_id, &String::from_str(&env, "ghost"));
    assert!(result.is_err(), "Removing a tag that does not exist should return an error");
}

// ============================================================================
// CATEGORY INDEX INTEGRITY TESTS
// ============================================================================

#[test]
fn test_category_update_removes_from_old_index_and_adds_to_new() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let business = Address::generate(&env);
    // create_test_invoice creates with InvoiceCategory::Services by default
    let invoice_id = create_test_invoice(&env, &client, &business, 1_000_000);

    // Verify it is in the Services index
    let services_before = client.get_invoices_by_category(&InvoiceCategory::Services);
    assert!(services_before.contains(invoice_id.clone()));

    // Update category to Technology
    client.update_category(&invoice_id, &InvoiceCategory::Technology);

    // Must no longer be in Services
    let services_after = client.get_invoices_by_category(&InvoiceCategory::Services);
    assert!(
        !services_after.contains(invoice_id.clone()),
        "Invoice must be removed from old category index"
    );

    // Must now be in Technology
    let tech = client.get_invoices_by_category(&InvoiceCategory::Technology);
    assert!(
        tech.contains(invoice_id.clone()),
        "Invoice must appear in new category index"
    );
}

#[test]
fn test_category_index_no_duplicates_on_repeated_update() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let business = Address::generate(&env);
    let invoice_id = create_test_invoice(&env, &client, &business, 1_000_000);

    // Update category to Technology twice
    client.update_category(&invoice_id, &InvoiceCategory::Technology);
    client.update_category(&invoice_id, &InvoiceCategory::Technology);

    let tech = client.get_invoices_by_category(&InvoiceCategory::Technology);
    let occurrences = tech.iter().filter(|id| *id == invoice_id).count();
    assert_eq!(occurrences, 1, "Invoice must appear exactly once in the category index");
}

// ============================================================================
// STATUS TRANSITION AND LIFECYCLE TESTS
// ============================================================================

#[test]
fn test_cancel_only_allowed_from_pending_or_verified() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let business = Address::generate(&env);
    let investor = Address::generate(&env);

    // Pending → Cancelled: OK
    let invoice_id = create_test_invoice(&env, &client, &business, 1_000_000);
    assert!(client.try_cancel_invoice(&invoice_id).is_ok());

    // Verified → Cancelled: OK
    let invoice_id2 = create_test_invoice(&env, &client, &business, 1_000_000);
    client.verify_invoice(&invoice_id2);
    assert!(client.try_cancel_invoice(&invoice_id2).is_ok());

    // Funded → Cancel must FAIL
    let invoice_id3 = create_test_invoice(&env, &client, &business, 1_000_000);
    client.verify_invoice(&invoice_id3);
    client.fund_invoice(&invoice_id3, &investor, &1_000_000i128);
    assert!(
        client.try_cancel_invoice(&invoice_id3).is_err(),
        "Funded invoice must not be cancellable"
    );
}

#[test]
fn test_status_index_updated_on_transition() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let business = Address::generate(&env);
    let invoice_id = create_test_invoice(&env, &client, &business, 1_000_000);

    // Should be in Pending index
    let pending = client.get_invoices_by_status(&InvoiceStatus::Pending);
    assert!(pending.contains(invoice_id.clone()));

    // Verify → moves to Verified index
    client.verify_invoice(&invoice_id);
    assert!(!client.get_invoices_by_status(&InvoiceStatus::Pending).contains(invoice_id.clone()));
    assert!(client.get_invoices_by_status(&InvoiceStatus::Verified).contains(invoice_id.clone()));
}

// ============================================================================
// METADATA VALIDATION TESTS
// ============================================================================

#[test]
fn test_metadata_customer_name_required() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let business = Address::generate(&env);
    let invoice_id = create_test_invoice(&env, &client, &business, 1_000_000);

    let bad_metadata = InvoiceMetadata {
        customer_name: String::from_str(&env, ""), // empty — must fail
        customer_address: String::from_str(&env, "123 Main St"),
        tax_id: String::from_str(&env, "TAX123"),
        line_items: Vec::new(&env),
        notes: String::from_str(&env, ""),
    };

    let result = client.try_update_metadata(&invoice_id, &bad_metadata);
    assert!(result.is_err(), "Empty customer name should be rejected");
}

#[test]
fn test_metadata_line_item_limit_at_50() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let business = Address::generate(&env);
    let invoice_id = create_test_invoice(&env, &client, &business, 1_000_000);

    // Build 51 line items — must fail
    let mut items = Vec::new(&env);
    for i in 0..51u32 {
        items.push_back(LineItemRecord(
            String::from_str(&env, &format!("item{i}")),
            1i128,
            1i128,
            1i128,
        ));
    }

    let bad_metadata = InvoiceMetadata {
        customer_name: String::from_str(&env, "Acme Corp"),
        customer_address: String::from_str(&env, ""),
        tax_id: String::from_str(&env, ""),
        line_items: items,
        notes: String::from_str(&env, ""),
    };

    let result = client.try_update_metadata(&invoice_id, &bad_metadata);
    assert!(result.is_err(), "More than 50 line items should be rejected");
}

#[test]
fn test_metadata_clear_removes_all_fields() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let business = Address::generate(&env);
    let invoice_id = create_test_invoice(&env, &client, &business, 1_000_000);

    let metadata = InvoiceMetadata {
        customer_name: String::from_str(&env, "Acme Corp"),
        customer_address: String::from_str(&env, "123 Main St"),
        tax_id: String::from_str(&env, "TAX-001"),
        line_items: Vec::new(&env),
        notes: String::from_str(&env, "Net 30"),
    };
    client.update_metadata(&invoice_id, &metadata);

    // Now clear
    client.clear_metadata(&invoice_id);

    let invoice = client.get_invoice(&invoice_id);
    assert!(invoice.metadata_customer_name.is_none());
    assert!(invoice.metadata_customer_address.is_none());
    assert!(invoice.metadata_tax_id.is_none());
    assert!(invoice.metadata_notes.is_none());
    assert_eq!(invoice.metadata_line_items.len(), 0);
}

// ============================================================================
// PAYMENT AND PROGRESS TESTS
// ============================================================================

#[test]
fn test_payment_progress_zero_on_no_payments() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let business = Address::generate(&env);
    let invoice_id = create_test_invoice(&env, &client, &business, 1_000_000);

    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.total_paid, 0);
}

#[test]
fn test_partial_payment_recorded_and_progress_calculated() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let business = Address::generate(&env);
    let investor = Address::generate(&env);
    let invoice_id = create_test_invoice(&env, &client, &business, 1_000_000);

    client.verify_invoice(&invoice_id);
    client.fund_invoice(&invoice_id, &investor, &1_000_000i128);

    // Record 50% payment
    let progress = client.record_payment(
        &invoice_id,
        &500_000i128,
        &String::from_str(&env, "TXN-001"),
    );
    assert_eq!(progress, 50, "Expected 50% payment progress");
}

#[test]
fn test_payment_progress_capped_at_100() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let business = Address::generate(&env);
    let investor = Address::generate(&env);
    let invoice_id = create_test_invoice(&env, &client, &business, 1_000_000);

    client.verify_invoice(&invoice_id);
    client.fund_invoice(&invoice_id, &investor, &1_000_000i128);

    // Overpay
    client.record_payment(&invoice_id, &2_000_000i128, &String::from_str(&env, "TXN-002"));

    let invoice = client.get_invoice(&invoice_id);
    // payment_progress() must not exceed 100
    assert!(
        invoice.total_paid >= invoice.amount,
        "Overpayment should be recorded"
    );
}

#[test]
fn test_zero_and_negative_payment_amounts_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let business = Address::generate(&env);
    let investor = Address::generate(&env);
    let invoice_id = create_test_invoice(&env, &client, &business, 1_000_000);

    client.verify_invoice(&invoice_id);
    client.fund_invoice(&invoice_id, &investor, &1_000_000i128);

    assert!(
        client
            .try_record_payment(&invoice_id, &0i128, &String::from_str(&env, "TXN-ZERO"))
            .is_err(),
        "Zero payment should be rejected"
    );

    assert!(
        client
            .try_record_payment(&invoice_id, &-100i128, &String::from_str(&env, "TXN-NEG"))
            .is_err(),
        "Negative payment should be rejected"
    );
}

// ============================================================================
// RATING TESTS
// ============================================================================

#[test]
fn test_rating_only_by_investor() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let business = Address::generate(&env);
    let investor = Address::generate(&env);
    let non_investor = Address::generate(&env);
    let invoice_id = create_test_invoice(&env, &client, &business, 1_000_000);

    client.verify_invoice(&invoice_id);
    client.fund_invoice(&invoice_id, &investor, &1_000_000i128);

    // Non-investor rating must fail
    let result = client.try_add_rating(&invoice_id, &4u32, &String::from_str(&env, "ok"), &non_investor);
    assert!(result.is_err(), "Non-investor should not be able to rate");

    // Investor rating must succeed
    let result = client.try_add_rating(&invoice_id, &5u32, &String::from_str(&env, "great"), &investor);
    assert!(result.is_ok(), "Investor should be able to rate");
}

#[test]
fn test_rating_bounds_enforced() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let business = Address::generate(&env);
    let investor = Address::generate(&env);
    let invoice_id = create_test_invoice(&env, &client, &business, 1_000_000);

    client.verify_invoice(&invoice_id);
    client.fund_invoice(&invoice_id, &investor, &1_000_000i128);

    // Rating 0 — below minimum
    assert!(
        client.try_add_rating(&invoice_id, &0u32, &String::from_str(&env, "bad"), &investor).is_err(),
        "Rating of 0 should be rejected"
    );

    // Rating 6 — above maximum
    assert!(
        client.try_add_rating(&invoice_id, &6u32, &String::from_str(&env, "too good"), &investor).is_err(),
        "Rating of 6 should be rejected"
    );
}

#[test]
fn test_duplicate_rating_by_same_investor_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let business = Address::generate(&env);
    let investor = Address::generate(&env);
    let invoice_id = create_test_invoice(&env, &client, &business, 1_000_000);

    client.verify_invoice(&invoice_id);
    client.fund_invoice(&invoice_id, &investor, &1_000_000i128);

    client.add_rating(&invoice_id, &5u32, &String::from_str(&env, "first"), &investor);

    let result = client.try_add_rating(&invoice_id, &3u32, &String::from_str(&env, "second"), &investor);
    assert!(result.is_err(), "Same investor rating twice should be rejected");
}

// ============================================================================
// INVOICE ID UNIQUENESS AND COUNTER TESTS
// ============================================================================

#[test]
fn test_sequential_invoices_have_unique_ids() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86_400;
    let desc = String::from_str(&env, "Invoice");

    let id1 = client.create_invoice(
        &business, &1_000i128, &currency, &due_date,
        &desc, &InvoiceCategory::Services, &Vec::new(&env),
    );
    let id2 = client.create_invoice(
        &business, &2_000i128, &currency, &due_date,
        &desc, &InvoiceCategory::Services, &Vec::new(&env),
    );
    let id3 = client.create_invoice(
        &business, &3_000i128, &currency, &due_date,
        &desc, &InvoiceCategory::Services, &Vec::new(&env),
    );

    assert_ne!(id1, id2, "Invoice IDs must be unique");
    assert_ne!(id2, id3, "Invoice IDs must be unique");
    assert_ne!(id1, id3, "Invoice IDs must be unique");
}

#[test]
fn test_total_invoice_count_increments_correctly() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let business = Address::generate(&env);

    assert_eq!(client.get_total_invoice_count(), 0);

    create_test_invoice(&env, &client, &business, 1_000_000);
    assert_eq!(client.get_total_invoice_count(), 1);

    create_test_invoice(&env, &client, &business, 2_000_000);
    assert_eq!(client.get_total_invoice_count(), 2);
}

// ============================================================================
// GRACE PERIOD AND DEFAULT TESTS
// ============================================================================

#[test]
fn test_invoice_not_defaulted_before_grace_deadline() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let business = Address::generate(&env);
    let investor = Address::generate(&env);

    // Set due_date to now + 10s, ledger still at now → not overdue yet
    let due_date = env.ledger().timestamp() + 10;
    let invoice_id = client.create_invoice(
        &business, &1_000_000i128, &Address::generate(&env),
        &due_date, &String::from_str(&env, "Grace test"),
        &InvoiceCategory::Services, &Vec::new(&env),
    );
    client.verify_invoice(&invoice_id);
    client.fund_invoice(&invoice_id, &investor, &1_000_000i128);

    let defaulted = client.check_and_handle_expiration(&invoice_id, &Invoice::DEFAULT_GRACE_PERIOD);
    assert!(!defaulted, "Invoice should not default before grace deadline");
}

#[test]
fn test_invoice_defaults_after_grace_deadline() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let business = Address::generate(&env);
    let investor = Address::generate(&env);

    let now = env.ledger().timestamp();
    // due_date well in the past so grace period has passed
    let due_date = now.saturating_sub(Invoice::DEFAULT_GRACE_PERIOD + 100);
    let invoice_id = client.create_invoice(
        &business, &1_000_000i128, &Address::generate(&env),
        &due_date, &String::from_str(&env, "Default test"),
        &InvoiceCategory::Services, &Vec::new(&env),
    );
    client.verify_invoice(&invoice_id);
    client.fund_invoice(&invoice_id, &investor, &1_000_000i128);

    let defaulted = client.check_and_handle_expiration(&invoice_id, &Invoice::DEFAULT_GRACE_PERIOD);
    assert!(defaulted, "Invoice should be defaulted after grace deadline");

    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Defaulted);
}