#![cfg(test)]

//! Tests for the admin `rating_override` entrypoint (issue #1876).
//!
//! ## Threat model
//! Without a mandatory, logged reason, an admin could silently rewrite an
//! invoice's displayed rating — e.g. to bury a legitimate bad-faith
//! complaint or inflate a business's track record — leaving investors who
//! rely on that score with no way to detect or attribute the change after
//! the fact. These tests confirm:
//! - a missing/empty reason is rejected with a typed error (not a panic),
//! - non-admin callers cannot invoke the override,
//! - out-of-range ratings are rejected,
//! - every successful override mutates the invoice AND produces exactly one
//!   audit-trail entry recording the actor and reason.

use soroban_sdk::{testutils::Address as _, Address, BytesN, Env, String, Vec};

use crate::admin::AdminStorage;
use crate::audit::AuditOperation;
use crate::errors::QuickLendXError;
use crate::invoice::Invoice;
use crate::storage::InvoiceStorage;
use crate::types::InvoiceCategory;
use crate::{QuickLendXContract, QuickLendXContractClient};

fn setup() -> (Env, QuickLendXContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    env.as_contract(&contract_id, || {
        AdminStorage::initialize(&env, &admin).unwrap();
    });
    (env, client, admin)
}

/// Store a bare invoice directly, bypassing `store_invoice`'s currency
/// whitelist / KYC checks (irrelevant to rating-override logic), and
/// return its id.
fn seed_invoice(env: &Env, contract_id: &Address) -> BytesN<32> {
    env.as_contract(contract_id, || {
        let business = Address::generate(env);
        let currency = Address::generate(env);
        let invoice = Invoice::new(
            env,
            business,
            1_000i128,
            currency,
            env.ledger().timestamp() + 86_400,
            String::from_str(env, "Test invoice"),
            InvoiceCategory::Services,
            Vec::new(env),
            None,
            None,
        )
        .unwrap();
        let id = invoice.id.clone();
        InvoiceStorage::store_invoice(env, &invoice);
        id
    })
}

#[test]
fn test_rating_override_requires_non_empty_reason() {
    let (env, client, admin) = setup();
    let invoice_id = seed_invoice(&env, &client.address);

    let result =
        client.try_rating_override(&admin, &invoice_id, &4u32, &String::from_str(&env, ""));

    assert_eq!(
        result,
        Err(Ok(QuickLendXError::InvalidRatingOverrideReason))
    );

    // Rejected calls must not mutate state or leave an audit trace.
    let trail = client.get_invoice_audit_trail(&invoice_id);
    assert_eq!(trail.len(), 0);
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.average_rating, None);
}

#[test]
fn test_rating_override_rejects_non_admin_caller() {
    let (env, client, _admin) = setup();
    let invoice_id = seed_invoice(&env, &client.address);
    let attacker = Address::generate(&env);

    let result = client.try_rating_override(
        &attacker,
        &invoice_id,
        &4u32,
        &String::from_str(&env, "correcting a fraudulent rating"),
    );

    assert!(result.is_err());

    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.average_rating, None);
}

#[test]
fn test_rating_override_rejects_out_of_range_rating() {
    let (env, client, admin) = setup();
    let invoice_id = seed_invoice(&env, &client.address);

    let result = client.try_rating_override(
        &admin,
        &invoice_id,
        &6u32,
        &String::from_str(&env, "correcting a fraudulent rating"),
    );

    assert_eq!(result, Err(Ok(QuickLendXError::InvalidRating)));
}

#[test]
fn test_rating_override_unknown_invoice_returns_not_found() {
    let (env, client, admin) = setup();
    let bogus_id = BytesN::from_array(&env, &[7u8; 32]);

    let result = client.try_rating_override(
        &admin,
        &bogus_id,
        &4u32,
        &String::from_str(&env, "correcting a fraudulent rating"),
    );

    assert_eq!(result, Err(Ok(QuickLendXError::InvoiceNotFound)));
}

#[test]
fn test_rating_override_success_updates_rating_and_logs_audit_entry() {
    let (env, client, admin) = setup();
    let invoice_id = seed_invoice(&env, &client.address);
    let reason = String::from_str(
        &env,
        "Investor rating found to be retaliatory; correcting to neutral score.",
    );

    client.rating_override(&admin, &invoice_id, &3u32, &reason);

    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.average_rating, Some(3));

    let trail = client.get_invoice_audit_trail(&invoice_id);
    assert_eq!(trail.len(), 1);

    let entry = client.get_audit_entry(&trail.get(0).unwrap()).unwrap();
    assert_eq!(entry.operation, AuditOperation::RatingOverridden);
    assert_eq!(entry.actor, admin);
    assert_eq!(entry.invoice_id, invoice_id);
    assert_eq!(entry.old_value, None);
    assert_eq!(entry.additional_data, Some(reason));
}
