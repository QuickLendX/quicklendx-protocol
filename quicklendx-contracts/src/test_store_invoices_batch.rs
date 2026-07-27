//! Tests for the `store_invoices_batch` entrypoint.
//!
//! Coverage:
//!  - Happy path: 1 invoice, N invoices, exactly MAX_BATCH_INVOICES invoices.
//!  - Batch-size guard: empty vec and oversized vec both return `BatchSizeExceeded`.
//!  - Per-business active-invoice cap: batch is rejected when the total would
//!    exceed the configured limit.
//!  - KYC gate: unverified and pending businesses cannot use the batch endpoint.
//!  - Atomicity: a single bad input aborts the whole batch (no partial writes).
//!  - Return value: IDs are distinct and correspond to retrievable invoices.

use super::*;
use crate::errors::QuickLendXError;
use crate::protocol_limits::MAX_BATCH_INVOICES;
use crate::types::{InvoiceCategory, InvoiceInput, InvoiceStatus};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Env, String, Vec,
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

fn verified_business(
    env: &Env,
    client: &QuickLendXContractClient<'_>,
    admin: &Address,
) -> Address {
    let business = Address::generate(env);
    client.submit_kyc_application(&business, &String::from_str(env, "KYC data"));
    client.verify_business(admin, &business);
    business
}

/// Build a single valid `InvoiceInput` for tests.
fn make_input(env: &Env, currency: &Address, offset_secs: u64) -> InvoiceInput {
    InvoiceInput {
        amount: 10_000,
        currency: currency.clone(),
        due_date: env.ledger().timestamp() + offset_secs,
        description: String::from_str(env, "Batch invoice"),
        category: InvoiceCategory::Services,
        tags: Vec::new(env),
        late_payment_penalty_bps: None,
        early_payment_discount_bps: None,
    }
}

/// Build a `Vec<InvoiceInput>` with `n` valid entries.
fn make_inputs(env: &Env, currency: &Address, n: u32) -> Vec<InvoiceInput> {
    let mut inputs: Vec<InvoiceInput> = Vec::new(env);
    for i in 0..n {
        // Stagger due dates so allocate_id cannot collide on timestamp alone.
        inputs.push_back(make_input(env, currency, 86_400 + u64::from(i) * 60));
    }
    inputs
}

// ─── Happy-path tests ─────────────────────────────────────────────────────────

#[test]
fn test_batch_single_invoice() {
    let (env, client, admin) = setup();
    let business = verified_business(&env, &client, &admin);
    let currency = Address::generate(&env);

    let inputs = make_inputs(&env, &currency, 1);
    let ids = client.store_invoices_batch(&business, &inputs);

    assert_eq!(ids.len(), 1, "Expected 1 ID returned");
    let invoice = client.get_invoice(&ids.get(0).unwrap());
    assert_eq!(invoice.status, InvoiceStatus::Pending);
    assert_eq!(invoice.business, business);
}

#[test]
fn test_batch_multiple_invoices() {
    let (env, client, admin) = setup();
    let business = verified_business(&env, &client, &admin);
    let currency = Address::generate(&env);

    let n = 5u32;
    let inputs = make_inputs(&env, &currency, n);
    let ids = client.store_invoices_batch(&business, &inputs);

    assert_eq!(ids.len(), n, "Expected {} IDs returned", n);

    // All IDs must be distinct.
    for i in 0..n {
        for j in (i + 1)..n {
            assert_ne!(
                ids.get(i).unwrap(),
                ids.get(j).unwrap(),
                "Invoice IDs must be unique"
            );
        }
    }

    // All invoices must be retrievable and in Pending state.
    for i in 0..n {
        let invoice = client.get_invoice(&ids.get(i).unwrap());
        assert_eq!(invoice.status, InvoiceStatus::Pending);
    }
}

#[test]
fn test_batch_max_size_succeeds() {
    let (env, client, admin) = setup();
    let business = verified_business(&env, &client, &admin);
    let currency = Address::generate(&env);

    let inputs = make_inputs(&env, &currency, MAX_BATCH_INVOICES);
    let ids = client.store_invoices_batch(&business, &inputs);

    assert_eq!(ids.len(), MAX_BATCH_INVOICES, "Max batch should be accepted");
}

// ─── Batch-size guard ─────────────────────────────────────────────────────────

#[test]
fn test_batch_empty_rejected() {
    let (env, client, admin) = setup();
    let business = verified_business(&env, &client, &admin);

    let empty: Vec<InvoiceInput> = Vec::new(&env);
    let result = client.try_store_invoices_batch(&business, &empty);
    assert_eq!(
        result.unwrap_err().unwrap(),
        QuickLendXError::BatchSizeExceeded,
        "Empty batch must be rejected"
    );
}

#[test]
fn test_batch_oversized_rejected() {
    let (env, client, admin) = setup();
    let business = verified_business(&env, &client, &admin);
    let currency = Address::generate(&env);

    let inputs = make_inputs(&env, &currency, MAX_BATCH_INVOICES + 1);
    let result = client.try_store_invoices_batch(&business, &inputs);
    assert_eq!(
        result.unwrap_err().unwrap(),
        QuickLendXError::BatchSizeExceeded,
        "Batch exceeding MAX_BATCH_INVOICES must be rejected"
    );
}

// ─── Per-business active-invoice cap ─────────────────────────────────────────

#[test]
fn test_batch_respects_active_invoice_cap() {
    let (env, client, admin) = setup();
    let business = verified_business(&env, &client, &admin);
    let currency = Address::generate(&env);

    // Lower the per-business limit to something small.
    // Default is 100; set it to 3 so we can test the cap easily.
    client.set_protocol_limits_full(
        &admin,
        &10,    // min_invoice_amount
        &10,    // min_bid_amount
        &100,   // min_bid_bps
        &365,   // max_due_date_days
        &604800, // grace_period_seconds (7 days)
        &3,     // max_invoices_per_business
        &crate::verification::InvestorTier::Basic, // min_investor_tier
    );

    // First batch: 2 invoices — should succeed (2 < 3).
    let first = make_inputs(&env, &currency, 2);
    let ids = client.store_invoices_batch(&business, &first);
    assert_eq!(ids.len(), 2);

    // Second batch: 2 invoices — should fail (2 + 2 = 4 > 3).
    let second = make_inputs(&env, &currency, 2);
    let result = client.try_store_invoices_batch(&business, &second);
    assert_eq!(
        result.unwrap_err().unwrap(),
        QuickLendXError::MaxInvoicesPerBusinessExceeded,
        "Batch that would exceed the cap must be rejected"
    );

    // Exactly 1 more invoice fits (limit = 3, active = 2, remaining = 1).
    let third = make_inputs(&env, &currency, 1);
    let ids3 = client.store_invoices_batch(&business, &third);
    assert_eq!(ids3.len(), 1);
}

// ─── KYC gate ────────────────────────────────────────────────────────────────

#[test]
fn test_batch_unverified_business_rejected() {
    let (env, client, admin) = setup();
    let _ = admin;

    // Business that has never submitted KYC.
    let unverified = Address::generate(&env);
    let currency = Address::generate(&env);
    let inputs = make_inputs(&env, &currency, 1);

    let result = client.try_store_invoices_batch(&unverified, &inputs);
    assert!(
        result.is_err(),
        "Unverified business must be rejected from batch endpoint"
    );
}

#[test]
fn test_batch_pending_business_rejected() {
    let (env, client, admin) = setup();
    let _ = admin;

    // Business that submitted KYC but is not yet approved.
    let pending_biz = Address::generate(&env);
    client.submit_kyc_application(&pending_biz, &String::from_str(&env, "KYC data"));

    let currency = Address::generate(&env);
    let inputs = make_inputs(&env, &currency, 1);

    let result = client.try_store_invoices_batch(&pending_biz, &inputs);
    assert_eq!(
        result.unwrap_err().unwrap(),
        QuickLendXError::KYCAlreadyPending,
        "Pending business must receive KYCAlreadyPending"
    );
}

// ─── Atomicity ───────────────────────────────────────────────────────────────

#[test]
fn test_batch_bad_input_aborts_entirely() {
    let (env, client, admin) = setup();
    let business = verified_business(&env, &client, &admin);
    let currency = Address::generate(&env);

    // Build a batch where the second entry has an invalid amount (0).
    let mut inputs: Vec<InvoiceInput> = Vec::new(&env);
    inputs.push_back(make_input(&env, &currency, 86_400));
    inputs.push_back(InvoiceInput {
        amount: 0, // invalid
        currency: currency.clone(),
        due_date: env.ledger().timestamp() + 86_400,
        description: String::from_str(&env, "Bad invoice"),
        category: InvoiceCategory::Services,
        tags: Vec::new(&env),
        late_payment_penalty_bps: None,
        early_payment_discount_bps: None,
    });

    let result = client.try_store_invoices_batch(&business, &inputs);
    assert!(
        result.is_err(),
        "Batch with invalid entry must fail entirely"
    );

    // No invoices should have been stored (atomicity).
    assert_eq!(
        client.get_total_invoice_count(),
        0,
        "No invoices must be stored when batch fails"
    );
}
