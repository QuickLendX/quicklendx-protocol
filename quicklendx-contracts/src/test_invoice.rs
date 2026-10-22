#![cfg(test)]

//! Invoice creation and lifecycle entrypoints (#2926).
//!
//! Contract-level tests go through `QuickLendXContractClient`. KYC has no
//! contract entrypoint yet, so business verification is set up by calling
//! the verification module inside the contract context.

use crate::errors::QuickLendXError;
use crate::init::InitializationParams;
use crate::invoice::Invoice;
use crate::protocol_limits::{MAX_INPUT_STATUS_BATCH_SIZE, MAX_INPUT_TAGS};
use crate::types::{InvoiceCategory, InvoiceMetadata, InvoiceStatus, LineItemRecord};
use crate::{QuickLendXContract, QuickLendXContractClient};
use soroban_sdk::{
    testutils::{Address as _, MockAuth, MockAuthInvoke},
    Address, BytesN, Env, IntoVal, String, Vec,
};

// ============================================================================
// HELPERS
// ============================================================================

fn params(
    env: &Env,
    admin: &Address,
    min_invoice_amount: i128,
    currencies: Vec<Address>,
) -> InitializationParams {
    InitializationParams {
        admin: admin.clone(),
        treasury: Address::generate(env),
        fee_bps: 200,
        min_invoice_amount,
        max_due_date_days: 365,
        grace_period_seconds: 604_800,
        initial_currencies: currencies,
        corridors: Vec::new(env),
        backfill_max_batch_size: 100,
    }
}

/// Registered and initialized contract with all auths mocked.
fn setup() -> (Env, QuickLendXContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&params(&env, &admin, 1_000, Vec::new(&env)));
    (env, client, admin)
}

/// Submit KYC for a new business; verify it when `verify` is true.
fn kyc_business(
    env: &Env,
    client: &QuickLendXContractClient,
    admin: &Address,
    verify: bool,
) -> Address {
    let business = Address::generate(env);
    env.as_contract(&client.address, || {
        crate::verification::submit_kyc_application(
            env,
            &business,
            String::from_str(env, "Business KYC data"),
        )
        .unwrap();
        if verify {
            crate::verification::verify_business(env, admin, &business).unwrap();
        }
    });
    business
}

fn setup_verified_business(
    env: &Env,
    client: &QuickLendXContractClient,
    admin: &Address,
) -> Address {
    kyc_business(env, client, admin, true)
}

fn nonce(env: &Env, n: u8) -> BytesN<32> {
    BytesN::from_array(env, &[n; 32])
}

fn try_store(
    env: &Env,
    client: &QuickLendXContractClient,
    business: &Address,
    amount: i128,
    currency: &Address,
    tags: &Vec<String>,
    n: u8,
) -> Result<BytesN<32>, QuickLendXError> {
    match client.try_store_invoice(
        business,
        &amount,
        currency,
        &(env.ledger().timestamp() + 86_400),
        &String::from_str(env, "Test invoice"),
        &InvoiceCategory::Services,
        tags,
        &None,
        &None,
        &None,
        &nonce(env, n),
    ) {
        Ok(Ok(id)) => Ok(id),
        Err(Ok(e)) => Err(e),
        other => panic!("unexpected invocation result: {:?}", other),
    }
}

/// Create an invoice for `business` with a fresh currency and no tags.
fn create_test_invoice(
    env: &Env,
    client: &QuickLendXContractClient,
    business: &Address,
    amount: i128,
    n: u8,
) -> BytesN<32> {
    try_store(
        env,
        client,
        business,
        amount,
        &Address::generate(env),
        &Vec::new(env),
        n,
    )
    .unwrap()
}

fn metadata(env: &Env, customer: &str, tax_id: &str) -> InvoiceMetadata {
    let mut line_items = Vec::new(env);
    line_items.push_back(LineItemRecord(
        String::from_str(env, "Item 1"),
        1,
        100_000,
        100_000,
    ));
    InvoiceMetadata {
        customer_name: String::from_str(env, customer),
        customer_address: String::from_str(env, "Address A"),
        tax_id: String::from_str(env, tax_id),
        line_items,
        notes: String::from_str(env, "Notes A"),
    }
}

fn distinct_tags(env: &Env, n: u32) -> Vec<String> {
    let names = [
        "t00", "t01", "t02", "t03", "t04", "t05", "t06", "t07", "t08", "t09", "t10", "t11", "t12",
        "t13", "t14", "t15", "t16", "t17", "t18", "t19", "t20", "t21", "t22", "t23", "t24", "t25",
        "t26", "t27", "t28", "t29", "t30", "t31", "t32", "t33", "t34", "t35", "t36", "t37", "t38",
        "t39", "t40", "t41", "t42", "t43", "t44", "t45", "t46", "t47", "t48", "t49", "t50", "t51",
    ];
    let mut v = Vec::new(env);
    for name in names.iter().take(n as usize) {
        v.push_back(String::from_str(env, name));
    }
    v
}

// ============================================================================
// ACCEPTANCE TESTS (#2926)
// ============================================================================

#[test]
fn same_ledger_invoices_get_distinct_ids() {
    let (env, client, admin) = setup();
    let business = setup_verified_business(&env, &client, &admin);

    // No ledger advance between the two calls.
    let id1 = create_test_invoice(&env, &client, &business, 1_000, 1);
    let id2 = create_test_invoice(&env, &client, &business, 2_000, 2);

    assert_ne!(id1, id2);
    assert_eq!(client.get_invoice(&id1).amount, 1_000);
    assert_eq!(client.get_invoice(&id2).amount, 2_000);
    assert_eq!(client.get_total_invoice_count(), 2);
    assert_eq!(client.get_business_invoices(&business).len(), 2);
}

#[test]
fn idempotent_store_is_bound_to_business() {
    let (env, client, admin) = setup();
    let a = setup_verified_business(&env, &client, &admin);
    let b = setup_verified_business(&env, &client, &admin);
    let currency = Address::generate(&env);
    let no_tags = Vec::new(&env);

    let id1 = try_store(&env, &client, &a, 10_000, &currency, &no_tags, 9).unwrap();
    let again = try_store(&env, &client, &a, 10_000, &currency, &no_tags, 9).unwrap();
    assert_eq!(again, id1);
    assert_eq!(client.get_total_invoice_count(), 1);

    let id2 = try_store(&env, &client, &b, 10_000, &currency, &no_tags, 9).unwrap();
    assert_ne!(id2, id1);
    assert_eq!(client.get_total_invoice_count(), 2);
    assert_eq!(client.get_invoice(&id1).business, a);
    assert_eq!(client.get_invoice(&id2).business, b);
}

#[test]
fn store_rejects_non_whitelisted_currency() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let usdc = Address::generate(&env);
    let mut currencies = Vec::new(&env);
    currencies.push_back(usdc.clone());
    client.initialize(&params(&env, &admin, 1_000, currencies));
    let business = setup_verified_business(&env, &client, &admin);
    let no_tags = Vec::new(&env);

    let other = Address::generate(&env);
    assert_eq!(
        try_store(&env, &client, &business, 10_000, &other, &no_tags, 1),
        Err(QuickLendXError::InvalidCurrency)
    );
    assert_eq!(client.get_total_invoice_count(), 0);

    // The whitelisted currency is accepted.
    assert!(try_store(&env, &client, &business, 10_000, &usdc, &no_tags, 2).is_ok());
}

#[test]
fn store_enforces_protocol_min_amount() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&params(&env, &admin, 1_000_000, Vec::new(&env)));
    let business = setup_verified_business(&env, &client, &admin);
    let currency = Address::generate(&env);
    let no_tags = Vec::new(&env);

    assert_eq!(
        try_store(&env, &client, &business, 999_999, &currency, &no_tags, 1),
        Err(QuickLendXError::InvalidAmount)
    );
    assert!(try_store(&env, &client, &business, 1_000_000, &currency, &no_tags, 2).is_ok());
}

#[test]
fn verify_invoice_requires_admin_signature() {
    let (env, client, admin) = setup();
    let business = setup_verified_business(&env, &client, &admin);
    let id = create_test_invoice(&env, &client, &business, 10_000, 1);

    // Nothing signed: auth error, status unchanged.
    env.set_auths(&[]);
    let r = client.try_verify_invoice(&admin, &id);
    assert!(
        matches!(r, Err(Err(_))),
        "unsigned verify_invoice must fail with an auth error, got {:?}",
        r
    );
    assert_eq!(client.get_invoice(&id).status, InvoiceStatus::Pending);

    // A non-admin who signs is rejected as NotAdmin.
    env.mock_all_auths();
    let other = Address::generate(&env);
    assert_eq!(
        client.try_verify_invoice(&other, &id),
        Err(Ok(QuickLendXError::NotAdmin))
    );

    // The admin's signature verifies it and moves it between status indexes.
    client
        .mock_auths(&[MockAuth {
            address: &admin,
            invoke: &MockAuthInvoke {
                contract: &client.address,
                fn_name: "verify_invoice",
                args: (admin.clone(), id.clone()).into_val(&env),
                sub_invokes: &[],
            },
        }])
        .verify_invoice(&admin, &id);
    assert_eq!(client.get_invoice(&id).status, InvoiceStatus::Verified);
    assert_eq!(
        client.get_invoice_count_by_status(&InvoiceStatus::Pending),
        0
    );
    assert_eq!(
        client.get_invoice_count_by_status(&InvoiceStatus::Verified),
        1
    );
}

#[test]
fn unverified_business_cannot_store() {
    let (env, client, admin) = setup();
    let currency = Address::generate(&env);
    let no_tags = Vec::new(&env);

    let no_record = Address::generate(&env);
    assert_eq!(
        try_store(&env, &client, &no_record, 10_000, &currency, &no_tags, 1),
        Err(QuickLendXError::BusinessNotVerified)
    );

    let pending = kyc_business(&env, &client, &admin, false);
    assert_eq!(
        try_store(&env, &client, &pending, 10_000, &currency, &no_tags, 2),
        Err(QuickLendXError::KYCAlreadyPending)
    );
    assert_eq!(client.get_total_invoice_count(), 0);
}

// ============================================================================
// store_invoice: auth, bounds, validation
// ============================================================================

#[test]
fn store_invoice_requires_business_signature() {
    let (env, client, admin) = setup();
    let business = setup_verified_business(&env, &client, &admin);
    env.set_auths(&[]);
    let r = client.try_store_invoice(
        &business,
        &10_000,
        &Address::generate(&env),
        &(env.ledger().timestamp() + 86_400),
        &String::from_str(&env, "x"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
        &None,
        &None,
        &None,
        &nonce(&env, 1),
    );
    assert!(
        matches!(r, Err(Err(_))),
        "unsigned store_invoice must fail with an auth error, got {:?}",
        r
    );
    env.mock_all_auths();
    assert_eq!(client.get_total_invoice_count(), 0);
}

#[test]
fn store_tag_limits_keep_both_error_codes() {
    let (env, client, admin) = setup();
    let business = setup_verified_business(&env, &client, &admin);
    let currency = Address::generate(&env);

    // 11 distinct tags: rejected by Invoice::new.
    assert_eq!(
        try_store(
            &env,
            &client,
            &business,
            10_000,
            &currency,
            &distinct_tags(&env, 11),
            1
        ),
        Err(QuickLendXError::TagLimitExceeded)
    );
    // MAX_INPUT_TAGS + 1 tags: rejected earlier by the input-size bound.
    assert_eq!(
        try_store(
            &env,
            &client,
            &business,
            10_000,
            &currency,
            &distinct_tags(&env, MAX_INPUT_TAGS + 1),
            2
        ),
        Err(QuickLendXError::InputTooLarge)
    );
    // 10 distinct tags are fine.
    let id = try_store(
        &env,
        &client,
        &business,
        10_000,
        &currency,
        &distinct_tags(&env, 10),
        3,
    )
    .unwrap();
    assert_eq!(client.get_invoice(&id).tags.len(), 10);
}

#[test]
fn store_rejects_oversized_description_and_bad_due_date() {
    let (env, client, admin) = setup();
    let business = setup_verified_business(&env, &client, &admin);
    let currency = Address::generate(&env);

    let big = String::from_bytes(&env, &[b'a'; 4_097]);
    let r = client.try_store_invoice(
        &business,
        &10_000,
        &currency,
        &(env.ledger().timestamp() + 86_400),
        &big,
        &InvoiceCategory::Services,
        &Vec::new(&env),
        &None,
        &None,
        &None,
        &nonce(&env, 1),
    );
    assert_eq!(r, Err(Ok(QuickLendXError::InputTooLarge)));

    let r = client.try_store_invoice(
        &business,
        &10_000,
        &currency,
        &env.ledger().timestamp(),
        &String::from_str(&env, "x"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
        &None,
        &None,
        &None,
        &nonce(&env, 2),
    );
    assert_eq!(r, Err(Ok(QuickLendXError::InvoiceDueDateInvalid)));

    // Penalty above 5000 bps is rejected by Invoice::new.
    let r = client.try_store_invoice(
        &business,
        &10_000,
        &currency,
        &(env.ledger().timestamp() + 86_400),
        &String::from_str(&env, "x"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
        &None,
        &Some(5_001u32),
        &None,
        &nonce(&env, 3),
    );
    assert_eq!(r, Err(Ok(QuickLendXError::InvalidFeeBasisPoints)));
    assert_eq!(client.get_total_invoice_count(), 0);
}

// ============================================================================
// QUERIES
// ============================================================================

#[test]
fn queries_follow_the_indexes() {
    let (env, client, admin) = setup();
    let business = setup_verified_business(&env, &client, &admin);
    let id1 = create_test_invoice(&env, &client, &business, 100_000, 1);
    let id2 = create_test_invoice(&env, &client, &business, 100_000, 2);

    assert_eq!(
        client.get_invoices_by_status(&InvoiceStatus::Pending).len(),
        2
    );
    assert_eq!(
        client.get_invoice_count_by_status(&InvoiceStatus::Pending),
        2
    );

    client.update_invoice_metadata(&business, &id1, &metadata(&env, "Customer A", "TAX-1"));
    assert_eq!(
        client.get_invoices_by_customer(&String::from_str(&env, "Customer A")),
        soroban_sdk::vec![&env, id1.clone()]
    );
    assert_eq!(
        client.get_invoices_by_tax_id(&String::from_str(&env, "TAX-1")),
        soroban_sdk::vec![&env, id1.clone()]
    );

    let unknown = BytesN::from_array(&env, &[0xEE; 32]);
    let statuses = client.get_invoices_by_status_batch(&soroban_sdk::vec![
        &env,
        id1.clone(),
        unknown,
        id2.clone()
    ]);
    assert_eq!(
        statuses,
        soroban_sdk::vec![
            &env,
            Some(InvoiceStatus::Pending),
            None,
            Some(InvoiceStatus::Pending)
        ]
    );

    let mut too_many = Vec::new(&env);
    for _ in 0..=MAX_INPUT_STATUS_BATCH_SIZE {
        too_many.push_back(id1.clone());
    }
    assert_eq!(
        client.try_get_invoices_by_status_batch(&too_many),
        Err(Ok(QuickLendXError::InputTooLarge))
    );

    assert_eq!(
        client.try_get_invoice(&BytesN::from_array(&env, &[0xAB; 32])),
        Err(Ok(QuickLendXError::InvoiceNotFound))
    );
}

// ============================================================================
// AUTHORIZATION AND SECURITY ENFORCEMENT TESTS
// ============================================================================

#[test]
fn test_unauthorized_tag_addition_fails() {
    let (env, client, admin) = setup();
    let business = setup_verified_business(&env, &client, &admin);
    let malicious_user = Address::generate(&env);
    let invoice_id = create_test_invoice(&env, &client, &business, 1_000_000, 1);
    let new_tag = String::from_str(&env, "stolen_invoice");

    // The attacker names the real owner and signs as themselves: auth fails.
    env.mock_auths(&[MockAuth {
        address: &malicious_user,
        invoke: &MockAuthInvoke {
            contract: &client.address,
            fn_name: "add_invoice_tag",
            args: (business.clone(), invoice_id.clone(), new_tag.clone()).into_val(&env),
            sub_invokes: &[],
        },
    }]);
    assert!(client
        .try_add_invoice_tag(&business, &invoice_id, &new_tag)
        .is_err());

    // The attacker names themselves: ownership check fails.
    env.mock_all_auths();
    assert_eq!(
        client.try_add_invoice_tag(&malicious_user, &invoice_id, &new_tag),
        Err(Ok(QuickLendXError::Unauthorized))
    );
    assert!(!client.get_invoice(&invoice_id).tags.contains(new_tag));
}

#[test]
fn test_unauthorized_category_update_fails() {
    let (env, client, admin) = setup();
    let business = setup_verified_business(&env, &client, &admin);
    let malicious_user = Address::generate(&env);
    let invoice_id = create_test_invoice(&env, &client, &business, 1_000_000, 1);

    env.mock_auths(&[MockAuth {
        address: &malicious_user,
        invoke: &MockAuthInvoke {
            contract: &client.address,
            fn_name: "update_invoice_category",
            args: (
                business.clone(),
                invoice_id.clone(),
                InvoiceCategory::Healthcare,
            )
                .into_val(&env),
            sub_invokes: &[],
        },
    }]);
    assert!(client
        .try_update_invoice_category(&business, &invoice_id, &InvoiceCategory::Healthcare)
        .is_err());

    env.mock_all_auths();
    assert_eq!(
        client.try_update_invoice_category(
            &malicious_user,
            &invoice_id,
            &InvoiceCategory::Healthcare
        ),
        Err(Ok(QuickLendXError::Unauthorized))
    );
    assert_eq!(
        client.get_invoice(&invoice_id).category,
        InvoiceCategory::Services
    );
}

#[test]
fn test_authorized_mutation_succeeds() {
    let (env, client, admin) = setup();
    let business = setup_verified_business(&env, &client, &admin);
    let invoice_id = create_test_invoice(&env, &client, &business, 1_000_000, 1);
    let new_tag = String::from_str(&env, "verified_v2");

    env.mock_auths(&[MockAuth {
        address: &business,
        invoke: &MockAuthInvoke {
            contract: &client.address,
            fn_name: "add_invoice_tag",
            args: (business.clone(), invoice_id.clone(), new_tag.clone()).into_val(&env),
            sub_invokes: &[],
        },
    }]);
    assert!(client
        .try_add_invoice_tag(&business, &invoice_id, &new_tag)
        .is_ok());
    assert!(client
        .get_invoice(&invoice_id)
        .tags
        .contains(new_tag.clone()));

    env.mock_all_auths();
    client.update_invoice_category(&business, &invoice_id, &InvoiceCategory::Healthcare);
    assert_eq!(
        client.get_invoice(&invoice_id).category,
        InvoiceCategory::Healthcare
    );

    client.remove_invoice_tag(&business, &invoice_id, &new_tag);
    assert!(!client.get_invoice(&invoice_id).tags.contains(new_tag));
}

// ============================================================================
// INVOICE CANCELLATION AUTHORIZATION (module level; `cancel_invoice` is not
// a contract entrypoint yet)
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

    let result = env.as_contract(&contract_id, || invoice.cancel(&env, attacker));
    assert_eq!(
        result.unwrap_err(),
        QuickLendXError::Unauthorized,
        "Non-owner cannot cancel invoice"
    );
    assert_eq!(invoice.status, InvoiceStatus::Pending);

    let result = env.as_contract(&contract_id, || invoice.cancel(&env, business.clone()));
    assert!(result.is_ok(), "Business owner can cancel invoice");
    assert_eq!(invoice.status, InvoiceStatus::Cancelled);
}

// ============================================================================
// MIXED-OWNER ATTACK SECURITY TESTS (Issue #1981)
// ============================================================================

/// Business B cannot update the metadata of Business A's invoice.
#[test]
fn test_rejects_invoice_metadata_update_by_different_business() {
    let (env, client, admin) = setup();
    let business_a = setup_verified_business(&env, &client, &admin);
    let business_b = setup_verified_business(&env, &client, &admin);
    let invoice_id = create_test_invoice(&env, &client, &business_a, 100_000, 1);
    let md = metadata(&env, "Customer A", "Tax A");

    // B signs but names A as the business: A's signature is missing.
    env.mock_auths(&[MockAuth {
        address: &business_b,
        invoke: &MockAuthInvoke {
            contract: &client.address,
            fn_name: "update_invoice_metadata",
            args: (business_a.clone(), invoice_id.clone(), md.clone()).into_val(&env),
            sub_invokes: &[],
        },
    }]);
    assert!(client
        .try_update_invoice_metadata(&business_a, &invoice_id, &md)
        .is_err());

    // B names itself: ownership check rejects it.
    env.mock_all_auths();
    assert_eq!(
        client.try_update_invoice_metadata(&business_b, &invoice_id, &md),
        Err(Ok(QuickLendXError::Unauthorized))
    );
    assert!(client.get_invoice(&invoice_id).metadata().is_none());
}

/// Business A can update the metadata of its own invoice.
#[test]
fn test_allows_invoice_metadata_update_by_owner_business() {
    let (env, client, admin) = setup();
    let business_a = setup_verified_business(&env, &client, &admin);
    let invoice_id = create_test_invoice(&env, &client, &business_a, 100_000, 1);
    let md = metadata(&env, "Customer A", "Tax A");

    env.mock_auths(&[MockAuth {
        address: &business_a,
        invoke: &MockAuthInvoke {
            contract: &client.address,
            fn_name: "update_invoice_metadata",
            args: (business_a.clone(), invoice_id.clone(), md.clone()).into_val(&env),
            sub_invokes: &[],
        },
    }]);
    assert!(client
        .try_update_invoice_metadata(&business_a, &invoice_id, &md)
        .is_ok());

    let updated = client.get_invoice(&invoice_id).metadata().unwrap();
    assert_eq!(updated.customer_name, String::from_str(&env, "Customer A"));
}

/// Business B cannot clear the metadata of Business A's invoice.
#[test]
fn test_rejects_invoice_metadata_clear_by_different_business() {
    let (env, client, admin) = setup();
    let business_a = setup_verified_business(&env, &client, &admin);
    let business_b = setup_verified_business(&env, &client, &admin);
    let invoice_id = create_test_invoice(&env, &client, &business_a, 100_000, 1);
    client.update_invoice_metadata(
        &business_a,
        &invoice_id,
        &metadata(&env, "Customer A", "Tax A"),
    );

    env.mock_auths(&[MockAuth {
        address: &business_b,
        invoke: &MockAuthInvoke {
            contract: &client.address,
            fn_name: "clear_invoice_metadata",
            args: (business_a.clone(), invoice_id.clone()).into_val(&env),
            sub_invokes: &[],
        },
    }]);
    assert!(client
        .try_clear_invoice_metadata(&business_a, &invoice_id)
        .is_err());

    env.mock_all_auths();
    assert_eq!(
        client.try_clear_invoice_metadata(&business_b, &invoice_id),
        Err(Ok(QuickLendXError::Unauthorized))
    );
    assert!(client.get_invoice(&invoice_id).metadata().is_some());
}

/// Business A can clear the metadata of its own invoice.
#[test]
fn test_allows_invoice_metadata_clear_by_owner_business() {
    let (env, client, admin) = setup();
    let business_a = setup_verified_business(&env, &client, &admin);
    let invoice_id = create_test_invoice(&env, &client, &business_a, 100_000, 1);
    client.update_invoice_metadata(
        &business_a,
        &invoice_id,
        &metadata(&env, "Customer A", "Tax A"),
    );

    env.mock_auths(&[MockAuth {
        address: &business_a,
        invoke: &MockAuthInvoke {
            contract: &client.address,
            fn_name: "clear_invoice_metadata",
            args: (business_a.clone(), invoice_id.clone()).into_val(&env),
            sub_invokes: &[],
        },
    }]);
    assert!(client
        .try_clear_invoice_metadata(&business_a, &invoice_id)
        .is_ok());
    assert!(client.get_invoice(&invoice_id).metadata().is_none());
    assert_eq!(
        client
            .get_invoices_by_customer(&String::from_str(&env, "Customer A"))
            .len(),
        0
    );
}
