#![cfg(test)]

//! Tests for the `invoice_batch_cancel` entrypoint.
//!
//! Coverage:
//!  - Happy path: 1 invoice, N invoices, exactly MAX_BATCH_INVOICES invoices.
//!  - Batch-size guard: empty vec and oversized vec both return `BatchSizeExceeded`.
//!  - KYC gate: unverified and pending businesses cannot use the batch cancel endpoint.
//!  - Atomicity: an error on any single item in the batch (not found, unauthorized, frozen)
//!    aborts the entire batch with zero mutations.

use super::*;
use crate::errors::QuickLendXError;
use crate::protocol_limits::MAX_BATCH_INVOICES;
use crate::types::{BusinessFreezeReason, InvoiceCategory, InvoiceInput, InvoiceStatus};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, BytesN, Env, String, Vec,
};

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn setup() -> (Env, QuickLendXContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.set_admin(&admin);
    client.initialize_fee_system(&admin);
    (env, client, admin)
}

fn verified_business(env: &Env, client: &QuickLendXContractClient<'_>, admin: &Address) -> Address {
    let business = Address::generate(env);
    client.submit_kyc_application(&business, &String::from_str(env, "KYC data"));
    client.verify_business(admin, &business);
    business
}

fn make_inputs(env: &Env, currency: &Address, n: u32) -> Vec<InvoiceInput> {
    let mut inputs: Vec<InvoiceInput> = Vec::new(env);
    for i in 0..n {
        inputs.push_back(InvoiceInput {
            amount: 10_000,
            currency: currency.clone(),
            due_date: env.ledger().timestamp() + 86_400 + u64::from(i) * 60,
            description: String::from_str(env, "Batch invoice"),
            category: InvoiceCategory::Services,
            tags: Vec::new(env),

            early_payment_discount_bps: None,
        });
    }
    inputs
}

// ─── Happy-path tests ─────────────────────────────────────────────────────────

#[test]
fn test_invoice_batch_cancel_single_success() {
    let (env, client, admin) = setup();
    let business = verified_business(&env, &client, &admin);
    let currency = Address::generate(&env);

    let inputs = make_inputs(&env, &currency, 1);
    let ids = client.store_invoices_batch(&business, &inputs);
    assert_eq!(ids.len(), 1);

    let res = client.try_invoice_batch_cancel(&business, &ids);
    assert!(res.is_ok());

    let inv = client.get_invoice(&ids.get(0).unwrap());
    assert_eq!(inv.status, InvoiceStatus::Cancelled);
}

#[test]
fn test_invoice_batch_cancel_multiple_success() {
    let (env, client, admin) = setup();
    let business = verified_business(&env, &client, &admin);
    let currency = Address::generate(&env);

    let inputs = make_inputs(&env, &currency, 3);
    let ids = client.store_invoices_batch(&business, &inputs);
    assert_eq!(ids.len(), 3);

    let res = client.try_invoice_batch_cancel(&business, &ids);
    assert!(res.is_ok());

    for i in 0..3 {
        let inv = client.get_invoice(&ids.get(i).unwrap());
        assert_eq!(inv.status, InvoiceStatus::Cancelled);
    }
}

#[test]
fn test_invoice_batch_cancel_max_size_success() {
    let (env, client, admin) = setup();
    let business = verified_business(&env, &client, &admin);
    let currency = Address::generate(&env);

    let inputs = make_inputs(&env, &currency, MAX_BATCH_INVOICES);
    let ids = client.store_invoices_batch(&business, &inputs);
    assert_eq!(ids.len(), MAX_BATCH_INVOICES);

    let res = client.try_invoice_batch_cancel(&business, &ids);
    assert!(res.is_ok());

    for i in 0..MAX_BATCH_INVOICES {
        let inv = client.get_invoice(&ids.get(i).unwrap());
        assert_eq!(inv.status, InvoiceStatus::Cancelled);
    }
}

// ─── Batch size guards ────────────────────────────────────────────────────────

#[test]
fn test_invoice_batch_cancel_empty_rejected() {
    let (env, client, admin) = setup();
    let business = verified_business(&env, &client, &admin);
    let ids: Vec<BytesN<32>> = Vec::new(&env);

    let res = client.try_invoice_batch_cancel(&business, &ids);
    assert_eq!(res, Err(Ok(QuickLendXError::BatchSizeExceeded)));
}

#[test]
fn test_invoice_batch_cancel_oversized_rejected() {
    let (env, client, admin) = setup();
    let business = verified_business(&env, &client, &admin);

    let mut ids: Vec<BytesN<32>> = Vec::new(&env);
    for _ in 0..=MAX_BATCH_INVOICES {
        ids.push_back(BytesN::from_array(&env, &[1u8; 32]));
    }

    let res = client.try_invoice_batch_cancel(&business, &ids);
    assert_eq!(res, Err(Ok(QuickLendXError::BatchSizeExceeded)));
}

// ─── KYC guards ───────────────────────────────────────────────────────────────

#[test]
fn test_invoice_batch_cancel_unverified_business_rejected() {
    let (env, client, _admin) = setup();
    let unverified_business = Address::generate(&env);
    let mut ids: Vec<BytesN<32>> = Vec::new(&env);
    ids.push_back(BytesN::from_array(&env, &[1u8; 32]));

    let res = client.try_invoice_batch_cancel(&unverified_business, &ids);
    assert_eq!(res, Err(Ok(QuickLendXError::BusinessNotVerified)));
}

#[test]
fn test_invoice_batch_cancel_pending_business_rejected() {
    let (env, client, _admin) = setup();
    let pending_business = Address::generate(&env);
    client.submit_kyc_application(&pending_business, &String::from_str(&env, "KYC data"));

    let mut ids: Vec<BytesN<32>> = Vec::new(&env);
    ids.push_back(BytesN::from_array(&env, &[1u8; 32]));

    let res = client.try_invoice_batch_cancel(&pending_business, &ids);
    assert_eq!(res, Err(Ok(QuickLendXError::KYCAlreadyPending)));
}

// ─── Atomicity & item validation tests ────────────────────────────────────────

#[test]
fn test_invoice_batch_cancel_nonexistent_invoice_aborts() {
    let (env, client, admin) = setup();
    let business = verified_business(&env, &client, &admin);
    let currency = Address::generate(&env);

    let inputs = make_inputs(&env, &currency, 2);
    let valid_ids = client.store_invoices_batch(&business, &inputs);

    let mut ids: Vec<BytesN<32>> = Vec::new(&env);
    ids.push_back(valid_ids.get(0).unwrap());
    ids.push_back(BytesN::from_array(&env, &[9u8; 32])); // Fake ID

    let res = client.try_invoice_batch_cancel(&business, &ids);
    assert_eq!(res, Err(Ok(QuickLendXError::InvoiceNotFound)));

    // Verify first invoice remains in original status (atomicity)
    let inv0 = client.get_invoice(&valid_ids.get(0).unwrap());
    assert_eq!(inv0.status, InvoiceStatus::Pending);
}

#[test]
fn test_invoice_batch_cancel_unauthorized_business_aborts() {
    let (env, client, admin) = setup();
    let business_a = verified_business(&env, &client, &admin);
    let business_b = verified_business(&env, &client, &admin);
    let currency = Address::generate(&env);

    let inputs_a = make_inputs(&env, &currency, 1);
    let ids_a = client.store_invoices_batch(&business_a, &inputs_a);

    let inputs_b = make_inputs(&env, &currency, 1);
    let ids_b = client.store_invoices_batch(&business_b, &inputs_b);

    let mut mixed_ids: Vec<BytesN<32>> = Vec::new(&env);
    mixed_ids.push_back(ids_a.get(0).unwrap());
    mixed_ids.push_back(ids_b.get(0).unwrap()); // Belongs to business_b

    let res = client.try_invoice_batch_cancel(&business_a, &mixed_ids);
    assert_eq!(res, Err(Ok(QuickLendXError::Unauthorized)));

    // Verify invoice A was not cancelled
    let inv_a = client.get_invoice(&ids_a.get(0).unwrap());
    assert_eq!(inv_a.status, InvoiceStatus::Pending);
}

#[test]
fn test_invoice_batch_cancel_frozen_invoice_aborts() {
    let (env, client, admin) = setup();
    let business = verified_business(&env, &client, &admin);
    let currency = Address::generate(&env);

    let inputs = make_inputs(&env, &currency, 2);
    let ids = client.store_invoices_batch(&business, &inputs);

    // Freeze second invoice
    client.freeze_invoice(
        &admin,
        &ids.get(1).unwrap(),
        &BusinessFreezeReason::Administrative,
    );

    let res = client.try_invoice_batch_cancel(&business, &ids);
    assert_eq!(res, Err(Ok(QuickLendXError::InvoiceFrozen)));

    // Verify invoice 0 was not cancelled due to atomic rollback
    let inv0 = client.get_invoice(&ids.get(0).unwrap());
    assert_eq!(inv0.status, InvoiceStatus::Pending);
}
