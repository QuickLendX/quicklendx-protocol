//! Tests for `get_business_invoices_paged` ordering.
//!
//! Acceptance: returned invoices must be sorted by `created_at` descending
//! (most-recently-created first).
//!
//! These tests lock in the expected sort order and will fail on any build
//! where the sort is absent or inverted.

use super::*;
use crate::invoice::{InvoiceCategory, InvoiceStatus};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Env, String, Vec,
};

fn setup() -> (Env, QuickLendXContractClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    (env, client)
}

/// Create one invoice whose `created_at` equals `timestamp`.
/// A fresh currency address is generated each call so hash-collisions are avoided.
fn create_invoice_at(
    env: &Env,
    client: &QuickLendXContractClient,
    business: &Address,
    timestamp: u64,
) -> BytesN<32> {
    env.ledger().set_timestamp(timestamp);
    client.store_invoice(
        business,
        &1_000i128,
        &Address::generate(env),
        &(timestamp + 86_400),
        &String::from_str(env, "invoice"),
        &InvoiceCategory::Services,
        &Vec::new(env),
    )
}

// ---------------------------------------------------------------------------
// Happy path
// ---------------------------------------------------------------------------

/// Three invoices created at ascending timestamps must be returned newest-first.
#[test]
fn returns_invoices_sorted_by_created_at_descending() {
    let (env, client) = setup();
    let business = Address::generate(&env);

    let id_oldest = create_invoice_at(&env, &client, &business, 1_000);
    let id_middle = create_invoice_at(&env, &client, &business, 2_000);
    let id_newest = create_invoice_at(&env, &client, &business, 3_000);

    let page = client.get_business_invoices_paged(
        &business,
        &Option::<InvoiceStatus>::None,
        &0u32,
        &10u32,
    );

    assert_eq!(page.len(), 3);
    assert_eq!(page.get(0).unwrap(), id_newest, "index 0 must be newest");
    assert_eq!(page.get(1).unwrap(), id_middle, "index 1 must be middle");
    assert_eq!(page.get(2).unwrap(), id_oldest, "index 2 must be oldest");
}

// ---------------------------------------------------------------------------
// Sad path / boundary
// ---------------------------------------------------------------------------

/// A single-invoice list must satisfy the ordering invariant without panicking.
#[test]
fn single_invoice_returned_in_correct_position() {
    let (env, client) = setup();
    let business = Address::generate(&env);

    let id = create_invoice_at(&env, &client, &business, 5_000);

    let page = client.get_business_invoices_paged(
        &business,
        &Option::<InvoiceStatus>::None,
        &0u32,
        &10u32,
    );

    assert_eq!(page.len(), 1);
    assert_eq!(page.get(0).unwrap(), id);
}

// ---------------------------------------------------------------------------
// Status-filter path
// ---------------------------------------------------------------------------

/// Ordering must hold when a status filter is applied (Pending-only subset).
#[test]
fn returns_status_filtered_invoices_sorted_by_created_at_descending() {
    let (env, client) = setup();
    let business = Address::generate(&env);

    let id_early = create_invoice_at(&env, &client, &business, 1_000);
    let id_late = create_invoice_at(&env, &client, &business, 9_000);

    let page =
        client.get_business_invoices_paged(&business, &Some(InvoiceStatus::Pending), &0u32, &10u32);

    assert_eq!(page.len(), 2);
    assert_eq!(
        page.get(0).unwrap(),
        id_late,
        "newer Pending invoice must be first"
    );
    assert_eq!(
        page.get(1).unwrap(),
        id_early,
        "older Pending invoice must be second"
    );
}

// ---------------------------------------------------------------------------
// Pagination path
// ---------------------------------------------------------------------------

/// Descending order must be preserved across page boundaries.
/// Given invoices at t=1000, 2000, 3000, 4000 the full sorted list is
/// [t4, t3, t2, t1].  Page1 (offset=0, limit=2) → [t4, t3];
/// Page2 (offset=2, limit=2) → [t2, t1].
#[test]
fn ordering_preserved_across_page_boundaries() {
    let (env, client) = setup();
    let business = Address::generate(&env);

    let id_t1 = create_invoice_at(&env, &client, &business, 1_000);
    let id_t2 = create_invoice_at(&env, &client, &business, 2_000);
    let id_t3 = create_invoice_at(&env, &client, &business, 3_000);
    let id_t4 = create_invoice_at(&env, &client, &business, 4_000);

    let page1 =
        client.get_business_invoices_paged(&business, &Option::<InvoiceStatus>::None, &0u32, &2u32);
    let page2 =
        client.get_business_invoices_paged(&business, &Option::<InvoiceStatus>::None, &2u32, &2u32);

    assert_eq!(page1.len(), 2);
    assert_eq!(page1.get(0).unwrap(), id_t4, "page1[0] must be newest");
    assert_eq!(
        page1.get(1).unwrap(),
        id_t3,
        "page1[1] must be second-newest"
    );

    assert_eq!(page2.len(), 2);
    assert_eq!(
        page2.get(0).unwrap(),
        id_t2,
        "page2[0] must be third-newest"
    );
    assert_eq!(page2.get(1).unwrap(), id_t1, "page2[1] must be oldest");
}
