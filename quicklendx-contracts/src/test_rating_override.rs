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
        None, /* early_payment_discount_bps */,
        None
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

#[test]
fn test_rating_override_revert_restores_pre_override_rating_and_logs_audit_trail() {
    let (env, client, admin) = setup();
    let invoice_id = seed_invoice(&env, &client.address);
    let rater = Address::generate(&env);

    // 1. Initial rating added (pre-override value = 5)
    client.add_invoice_rating(
        &invoice_id,
        &5u32,
        &String::from_str(&env, "Great service"),
        &rater,
    );
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.average_rating, Some(5));

    // 2. Admin overrides rating to 2
    let override_reason = String::from_str(
        &env,
        "Temporary rating override due to pending dispute investigation.",
    );
    client.rating_override(&admin, &invoice_id, &2u32, &override_reason);

    let invoice_overridden = client.get_invoice(&invoice_id);
    assert_eq!(invoice_overridden.average_rating, Some(2));

    // 3. Admin reverts override back to pre-override rating (5)
    let revert_reason = String::from_str(
        &env,
        "Dispute resolved in favor of seller; reverting rating override to original value.",
    );
    client.rating_override(&admin, &invoice_id, &5u32, &revert_reason);

    let invoice_reverted = client.get_invoice(&invoice_id);
    assert_eq!(invoice_reverted.average_rating, Some(5));

    // 4. Verify audit trail logs both the initial override and the revert
    let trail = client.get_invoice_audit_trail(&invoice_id);
    assert_eq!(trail.len(), 2);

    let entry1 = client.get_audit_entry(&trail.get(0).unwrap()).unwrap();
    assert_eq!(entry1.operation, AuditOperation::RatingOverridden);
    assert_eq!(entry1.actor, admin);
    assert_eq!(entry1.invoice_id, invoice_id);
    assert_eq!(entry1.old_value, Some(String::from_str(&env, "5")));
    assert_eq!(entry1.new_value, Some(String::from_str(&env, "2")));
    assert_eq!(entry1.additional_data, Some(override_reason));

    let entry2 = client.get_audit_entry(&trail.get(1).unwrap()).unwrap();
    assert_eq!(entry2.operation, AuditOperation::RatingOverridden);
    assert_eq!(entry2.actor, admin);
    assert_eq!(entry2.invoice_id, invoice_id);
    assert_eq!(entry2.old_value, Some(String::from_str(&env, "2")));
    assert_eq!(entry2.new_value, Some(String::from_str(&env, "5")));
    assert_eq!(entry2.additional_data, Some(revert_reason));
}

#[test]
fn test_rating_override_revert_on_invoice_with_multiple_ratings() {
    let (env, client, admin) = setup();
    let invoice_id = seed_invoice(&env, &client.address);
    let rater1 = Address::generate(&env);
    let rater2 = Address::generate(&env);

    // Add multiple ratings: 5 and 3 -> computed average rating is 4
    client.add_invoice_rating(
        &invoice_id,
        &5u32,
        &String::from_str(&env, "Rating 1"),
        &rater1,
    );
    client.add_invoice_rating(
        &invoice_id,
        &3u32,
        &String::from_str(&env, "Rating 2"),
        &rater2,
    );

    let pre_override_invoice = client.get_invoice(&invoice_id);
    let pre_override_rating = pre_override_invoice.average_rating;
    assert_eq!(pre_override_rating, Some(4));

    // Admin overrides average rating to 1
    let override_reason = String::from_str(&env, "Flagged rating override");
    client.rating_override(&admin, &invoice_id, &1u32, &override_reason);
    assert_eq!(client.get_invoice(&invoice_id).average_rating, Some(1));

    // Admin reverts rating back to pre-override rating (4)
    let revert_reason = String::from_str(&env, "Reverting rating override to pre-override average");
    client.rating_override(
        &admin,
        &invoice_id,
        &pre_override_rating.unwrap(),
        &revert_reason,
    );

    let final_invoice = client.get_invoice(&invoice_id);
    assert_eq!(final_invoice.average_rating, pre_override_rating);
    assert_eq!(final_invoice.total_ratings, 2);
}

#[test]
fn test_rating_override_revert_failed_attempt_preserves_current_override_rating() {
    let (env, client, admin) = setup();
    let invoice_id = seed_invoice(&env, &client.address);
    let rater = Address::generate(&env);

    client.add_invoice_rating(
        &invoice_id,
        &4u32,
        &String::from_str(&env, "Good"),
        &rater,
    );

    // Override to 2
    let override_reason = String::from_str(&env, "Override to 2");
    client.rating_override(&admin, &invoice_id, &2u32, &override_reason);
    assert_eq!(client.get_invoice(&invoice_id).average_rating, Some(2));

    // Revert attempt with invalid out-of-range rating (6) fails
    let invalid_rating_res = client.try_rating_override(
        &admin,
        &invoice_id,
        &6u32,
        &String::from_str(&env, "Revert attempt with invalid rating"),
    );
    assert_eq!(invalid_rating_res, Err(Ok(QuickLendXError::InvalidRating)));

    // Revert attempt with empty reason fails
    let empty_reason_res = client.try_rating_override(
        &admin,
        &invoice_id,
        &4u32,
        &String::from_str(&env, ""),
    );
    assert_eq!(
        empty_reason_res,
        Err(Ok(QuickLendXError::InvalidRatingOverrideReason))
    );

    // Revert attempt by non-admin fails
    let attacker = Address::generate(&env);
    let non_admin_res = client.try_rating_override(
        &attacker,
        &invoice_id,
        &4u32,
        &String::from_str(&env, "Unauthorized revert"),
    );
    assert!(non_admin_res.is_err());

    // Rating must remain at current override value (2) after failed revert attempts
    let invoice_after_failed_reverts = client.get_invoice(&invoice_id);
    assert_eq!(invoice_after_failed_reverts.average_rating, Some(2));

    // Audit trail must only contain the single successful override entry
    let trail = client.get_invoice_audit_trail(&invoice_id);
    assert_eq!(trail.len(), 1);
}

#[test]
fn test_rating_override_revert_on_frozen_invoice_fails() {
    let (env, client, admin) = setup();
    let invoice_id = seed_invoice(&env, &client.address);
    let rater = Address::generate(&env);

    client.add_invoice_rating(
        &invoice_id,
        &5u32,
        &String::from_str(&env, "Superb"),
        &rater,
    );

    // Override to 1
    client.rating_override(
        &admin,
        &invoice_id,
        &1u32,
        &String::from_str(&env, "Initial override"),
    );
    assert_eq!(client.get_invoice(&invoice_id).average_rating, Some(1));

    // Freeze invoice
    env.as_contract(&client.address, || {
        InvoiceStorage::set_frozen(
            &env,
            &invoice_id,
            true,
            Some(crate::types::BusinessFreezeReason::AdminAction),
        );
    });

    // Revert attempt on frozen invoice must fail
    let revert_res = client.try_rating_override(
        &admin,
        &invoice_id,
        &5u32,
        &String::from_str(&env, "Revert override on frozen invoice"),
    );
    assert_eq!(revert_res, Err(Ok(QuickLendXError::InvoiceFrozen)));

    // Rating remains at 1
    assert_eq!(client.get_invoice(&invoice_id).average_rating, Some(1));
}

