#![cfg(test)]

use crate::errors::QuickLendXError;
use crate::types::{InvoiceCategory, InvoiceMetadata, LineItemRecord};
use crate::verification::BusinessVerificationStorage;
use crate::{QuickLendXContract, QuickLendXContractClient};
use soroban_sdk::{
    testutils::Address as _,
    vec, Address, BytesN, Env, String, Vec,
};

fn setup() -> (Env, QuickLendXContractClient<'static>, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.set_admin(&admin);
    (env, client, admin, contract_id)
}

fn verified_business(env: &Env, client: &QuickLendXContractClient, admin: &Address) -> Address {
    let business = Address::generate(env);
    client.submit_kyc_application(&business, &String::from_str(env, "KYC data"));
    client.verify_business(admin, &business);
    business
}

fn upload_invoice(env: &Env, client: &QuickLendXContractClient, business: &Address) -> BytesN<32> {
    let currency = Address::generate(env);
    let due_date = env.ledger().timestamp() + 86_400;
    client.upload_invoice(
        business,
        &1_000i128,
        &currency,
        &due_date,
        &String::from_str(env, "test invoice"),
        &InvoiceCategory::Services,
        &Vec::new(env),
    )
}

fn delete_business(env: &Env, contract_id: &Address, business: &Address) {
    // Access storage through the contract context to satisfy Soroban's storage access rules.
    env.as_contract(contract_id, || {
        BusinessVerificationStorage::delete_business(env, business)
            .expect("delete_business should succeed");
    });
}

fn restore_business(env: &Env, contract_id: &Address, business: &Address) {
    env.as_contract(contract_id, || {
        BusinessVerificationStorage::restore_business(env, business)
            .expect("restore_business should succeed");
    });
}

/// Deleted/frozen business cannot cancel their own invoice.
#[test]
fn test_deleted_business_cannot_cancel_invoice() {
    let (env, client, admin, contract_id) = setup();
    let business = verified_business(&env, &client, &admin);
    let invoice_id = upload_invoice(&env, &client, &business);

    delete_business(&env, &contract_id, &business);

    let result = client.try_cancel_invoice(&invoice_id);
    assert!(result.is_err(), "Deleted business must not cancel invoice");
    assert_eq!(
        result.unwrap_err().unwrap(),
        QuickLendXError::BusinessDeleted,
        "Expected BusinessDeleted for deleted business"
    );
}

/// Deleted/frozen business cannot update invoice metadata.
#[test]
fn test_deleted_business_cannot_update_metadata() {
    let (env, client, admin, contract_id) = setup();
    let business = verified_business(&env, &client, &admin);
    let invoice_id = upload_invoice(&env, &client, &business);

    delete_business(&env, &contract_id, &business);

    let metadata = InvoiceMetadata {
        customer_name: String::from_str(&env, "Customer"),
        customer_address: String::from_str(&env, "123 Street"),
        tax_id: String::from_str(&env, "TAX-001"),
        notes: String::from_str(&env, "Note"),
        line_items: vec![
            &env,
            LineItemRecord(String::from_str(&env, "Item"), 1, 1_000, 1_000),
        ],
    };

    let result = client.try_update_invoice_metadata(&invoice_id, &metadata);
    assert!(result.is_err(), "Deleted business must not update metadata");
    assert_eq!(
        result.unwrap_err().unwrap(),
        QuickLendXError::BusinessDeleted,
        "Expected BusinessDeleted for deleted business"
    );
}

/// Deleted/frozen business cannot clear invoice metadata.
#[test]
fn test_deleted_business_cannot_clear_metadata() {
    let (env, client, admin, contract_id) = setup();
    let business = verified_business(&env, &client, &admin);
    let invoice_id = upload_invoice(&env, &client, &business);

    delete_business(&env, &contract_id, &business);

    let result = client.try_clear_invoice_metadata(&invoice_id);
    assert!(result.is_err(), "Deleted business must not clear metadata");
    assert_eq!(
        result.unwrap_err().unwrap(),
        QuickLendXError::BusinessDeleted,
        "Expected BusinessDeleted for deleted business"
    );
}

/// Deleted/frozen business cannot update invoice category.
#[test]
fn test_deleted_business_cannot_update_category() {
    let (env, client, admin, contract_id) = setup();
    let business = verified_business(&env, &client, &admin);
    let invoice_id = upload_invoice(&env, &client, &business);

    delete_business(&env, &contract_id, &business);

    let result = client.try_update_invoice_category(&invoice_id, &InvoiceCategory::Goods);
    assert!(result.is_err(), "Deleted business must not update category");
    assert_eq!(
        result.unwrap_err().unwrap(),
        QuickLendXError::BusinessDeleted,
        "Expected BusinessDeleted for deleted business"
    );
}

/// Deleted/frozen business cannot add a tag to an invoice.
#[test]
fn test_deleted_business_cannot_add_tag() {
    let (env, client, admin, contract_id) = setup();
    let business = verified_business(&env, &client, &admin);
    let invoice_id = upload_invoice(&env, &client, &business);

    delete_business(&env, &contract_id, &business);

    let tag = String::from_str(&env, "urgent");
    let result = client.try_add_invoice_tag(&invoice_id, &tag);
    assert!(result.is_err(), "Deleted business must not add tag");
    assert_eq!(
        result.unwrap_err().unwrap(),
        QuickLendXError::BusinessDeleted,
        "Expected BusinessDeleted for deleted business"
    );
}

/// Deleted/frozen business cannot remove a tag from an invoice.
#[test]
fn test_deleted_business_cannot_remove_tag() {
    let (env, client, admin, contract_id) = setup();
    let business = verified_business(&env, &client, &admin);
    let invoice_id = upload_invoice(&env, &client, &business);

    delete_business(&env, &contract_id, &business);

    let tag = String::from_str(&env, "test");
    let result = client.try_remove_invoice_tag(&invoice_id, &tag);
    assert!(result.is_err(), "Deleted business must not remove tag");
    assert_eq!(
        result.unwrap_err().unwrap(),
        QuickLendXError::BusinessDeleted,
        "Expected BusinessDeleted for deleted business"
    );
}

/// Deleted/frozen business can still view their own invoice (read is not gated).
#[test]
fn test_deleted_business_can_view_invoice() {
    let (env, client, admin, contract_id) = setup();
    let business = verified_business(&env, &client, &admin);
    let invoice_id = upload_invoice(&env, &client, &business);

    delete_business(&env, &contract_id, &business);

    let result = client.try_get_invoice(&invoice_id);
    assert!(result.is_ok(), "Deleted business must still view invoice");
}

/// Verified (active) business can still mutate — happy path.
#[test]
fn test_active_business_can_cancel_invoice() {
    let (env, client, admin, _contract_id) = setup();
    let business = verified_business(&env, &client, &admin);
    let invoice_id = upload_invoice(&env, &client, &business);

    let result = client.try_cancel_invoice(&invoice_id);
    assert!(result.is_ok(), "Active business must cancel invoice");
}

// =============================================================================
// Freeze / unfreeze cycle tests
// =============================================================================

/// Full cycle: active → freeze → blocked → unfreeze → active again.
#[test]
fn test_cancel_invoice_succeeds_after_unfreeze() {
    let (env, client, admin, contract_id) = setup();
    let business = verified_business(&env, &client, &admin);
    let invoice_id = upload_invoice(&env, &client, &business);

    delete_business(&env, &contract_id, &business);

    let blocked = client.try_cancel_invoice(&invoice_id);
    assert_eq!(
        blocked.unwrap_err().unwrap(),
        QuickLendXError::BusinessDeleted,
        "must be blocked while frozen"
    );

    restore_business(&env, &contract_id, &business);

    let result = client.try_cancel_invoice(&invoice_id);
    assert!(result.is_ok(), "cancel must succeed after unfreeze");
}

/// Full cycle: active → freeze → blocked → unfreeze → upload new invoice succeeds.
#[test]
fn test_upload_invoice_succeeds_after_unfreeze() {
    let (env, client, admin, contract_id) = setup();
    let business = verified_business(&env, &client, &admin);
    let _first = upload_invoice(&env, &client, &business);

    delete_business(&env, &contract_id, &business);

    let blocked = upload_invoice_fallible(&env, &client, &business);
    assert!(blocked.is_err(), "upload must be blocked while frozen");

    restore_business(&env, &contract_id, &business);

    let second = upload_invoice(&env, &client, &business);
    let invoice = client.try_get_invoice(&second);
    assert!(invoice.is_ok(), "newly uploaded invoice must be retrievable");
}

/// Unfreezing a business that was never frozen must fail.
#[test]
fn test_restore_non_deleted_business_fails() {
    let (env, client, admin, contract_id) = setup();
    let business = verified_business(&env, &client, &admin);

    env.as_contract(&contract_id, || {
        let result = BusinessVerificationStorage::restore_business(&env, &business);
        assert_eq!(
            result,
            Err(QuickLendXError::BusinessDeleted),
            "restoring a non-deleted business must return BusinessDeleted"
        );
    });
}

/// Double-freeze is idempotent; single unfreeze restores.
#[test]
fn test_double_freeze_single_unfreeze_restores() {
    let (env, client, admin, contract_id) = setup();
    let business = verified_business(&env, &client, &admin);
    let invoice_id = upload_invoice(&env, &client, &business);

    delete_business(&env, &contract_id, &business);
    delete_business(&env, &contract_id, &business);

    let blocked = client.try_cancel_invoice(&invoice_id);
    assert_eq!(
        blocked.unwrap_err().unwrap(),
        QuickLendXError::BusinessDeleted,
        "must be blocked after double freeze"
    );

    restore_business(&env, &contract_id, &business);

    let result = client.try_cancel_invoice(&invoice_id);
    assert!(
        result.is_ok(),
        "cancel must succeed after single unfreeze from double freeze"
    );
}

/// Freeze → unfreeze → re-freeze → blocked again (two full cycles).
#[test]
fn test_two_freeze_unfreeze_cycles() {
    let (env, client, admin, contract_id) = setup();
    let business = verified_business(&env, &client, &admin);
    let invoice_id = upload_invoice(&env, &client, &business);

    // Cycle 1
    delete_business(&env, &contract_id, &business);
    assert_eq!(
        client
            .try_cancel_invoice(&invoice_id)
            .unwrap_err()
            .unwrap(),
        QuickLendXError::BusinessDeleted,
    );
    restore_business(&env, &contract_id, &business);
    assert!(client.try_cancel_invoice(&invoice_id).is_ok());

    // Cycle 2
    let invoice_id_2 = upload_invoice(&env, &client, &business);
    delete_business(&env, &contract_id, &business);
    assert_eq!(
        client
            .try_cancel_invoice(&invoice_id_2)
            .unwrap_err()
            .unwrap(),
        QuickLendXError::BusinessDeleted,
    );
    restore_business(&env, &contract_id, &business);
    assert!(client.try_cancel_invoice(&invoice_id_2).is_ok());
}

/// Metadata mutation blocked while frozen, succeeds after unfreeze.
#[test]
fn test_update_metadata_blocked_then_succeeds_after_unfreeze() {
    let (env, client, admin, contract_id) = setup();
    let business = verified_business(&env, &client, &admin);
    let invoice_id = upload_invoice(&env, &client, &business);

    delete_business(&env, &contract_id, &business);

    let metadata = InvoiceMetadata {
        customer_name: String::from_str(&env, "Customer"),
        customer_address: String::from_str(&env, "123 Street"),
        tax_id: String::from_str(&env, "TAX-001"),
        notes: String::from_str(&env, "Note"),
        line_items: vec![
            &env,
            LineItemRecord(String::from_str(&env, "Item"), 1, 1_000, 1_000),
        ],
    };

    let blocked = client.try_update_invoice_metadata(&invoice_id, &metadata);
    assert_eq!(
        blocked.unwrap_err().unwrap(),
        QuickLendXError::BusinessDeleted,
    );

    restore_business(&env, &contract_id, &business);

    let result = client.try_update_invoice_metadata(&invoice_id, &metadata);
    assert!(result.is_ok(), "metadata update must succeed after unfreeze");
}

/// Tag operations blocked while frozen, succeed after unfreeze.
#[test]
fn test_add_tag_blocked_then_succeeds_after_unfreeze() {
    let (env, client, admin, contract_id) = setup();
    let business = verified_business(&env, &client, &admin);
    let invoice_id = upload_invoice(&env, &client, &business);

    delete_business(&env, &contract_id, &business);

    let tag = String::from_str(&env, "urgent");
    let blocked = client.try_add_invoice_tag(&invoice_id, &tag);
    assert_eq!(
        blocked.unwrap_err().unwrap(),
        QuickLendXError::BusinessDeleted,
    );

    restore_business(&env, &contract_id, &business);

    let result = client.try_add_invoice_tag(&invoice_id, &tag);
    assert!(result.is_ok(), "add tag must succeed after unfreeze");
}

/// Category update blocked while frozen, succeeds after unfreeze.
#[test]
fn test_update_category_blocked_then_succeeds_after_unfreeze() {
    let (env, client, admin, contract_id) = setup();
    let business = verified_business(&env, &client, &admin);
    let invoice_id = upload_invoice(&env, &client, &business);

    delete_business(&env, &contract_id, &business);

    let blocked = client.try_update_invoice_category(&invoice_id, &InvoiceCategory::Goods);
    assert_eq!(
        blocked.unwrap_err().unwrap(),
        QuickLendXError::BusinessDeleted,
    );

    restore_business(&env, &contract_id, &business);

    let result = client.try_update_invoice_category(&invoice_id, &InvoiceCategory::Goods);
    assert!(result.is_ok(), "category update must succeed after unfreeze");
}

/// View operations (read-only) are never blocked by freeze or unfreeze.
#[test]
fn test_view_always_allowed_through_freeze_unfreeze_cycle() {
    let (env, client, admin, contract_id) = setup();
    let business = verified_business(&env, &client, &admin);
    let invoice_id = upload_invoice(&env, &client, &business);

    // Before freeze
    assert!(client.try_get_invoice(&invoice_id).is_ok());

    // During freeze
    delete_business(&env, &contract_id, &business);
    assert!(client.try_get_invoice(&invoice_id).is_ok());

    // After unfreeze
    restore_business(&env, &contract_id, &business);
    assert!(client.try_get_invoice(&invoice_id).is_ok());
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn upload_invoice_fallible(
    env: &Env,
    client: &QuickLendXContractClient,
    business: &Address,
) -> Result<BytesN<32>, QuickLendXError> {
    let currency = Address::generate(env);
    let due_date = env.ledger().timestamp() + 86_400;
    client
        .try_upload_invoice(
            business,
            &1_000i128,
            &currency,
            &due_date,
            &String::from_str(env, "test invoice"),
            &InvoiceCategory::Services,
            &Vec::new(env),
        )
        .map_err(|_| QuickLendXError::BusinessDeleted)?
}
