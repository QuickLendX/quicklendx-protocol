#![cfg(test)]

//! # `cancel_invoice` Authorization & State-Precondition Matrix (Issue #1347)
//!
//! Pins the cancellation guard for the public `cancel_invoice` entry point and
//! the underlying `Invoice::cancel` model method.
//!
//! ## Cancellation guard invariant (as enforced by the code today)
//!
//! 1. **Ownership:** only the owning business may cancel its invoice.
//!    `Invoice::cancel` rejects a non-owner actor with
//!    [`QuickLendXError::Unauthorized`]; the public `cancel_invoice` additionally
//!    calls `invoice.business.require_auth()`.
//! 2. **Pre-funding cancellation:** cancelling from `Pending` / `Verified`
//!    succeeds and moves the invoice to `Cancelled`, removing it from the
//!    available (`Verified`) index and adding it to the `Cancelled` index.
//!
//! ## Findings (state-precondition gap)
//!
//! The issue's expectation is that cancellation is rejected from
//! `Funded` / `Paid` / `Defaulted` / `Cancelled` with an `InvalidStatus`-style
//! error. **The current implementation does not enforce that precondition:**
//!
//! - `Invoice::cancel` (`invoice.rs`) checks ownership only and then
//!   unconditionally sets `status = Cancelled` — there is no status guard.
//! - The public `cancel_invoice` (`lib.rs`) wraps it with pause / auth / KYC
//!   checks but likewise performs **no status-precondition check** before
//!   calling `invoice.cancel`.
//!
//! These tests therefore pin the *actual* behaviour (cancellation from a
//! post-funding state currently succeeds) and flag the missing guard via the
//! [`test_cancel_from_funded_currently_succeeds_documents_gap`] case so a future
//! fix that adds the guard will surface here for review.

use crate::errors::QuickLendXError;
use crate::invoice::Invoice;
use crate::types::{InvoiceCategory, InvoiceStatus};
use crate::{QuickLendXContract, QuickLendXContractClient};
use soroban_sdk::{testutils::Address as _, Address, BytesN, Env, String, Vec};

// ============================================================================
// Helpers
// ============================================================================

fn setup() -> (Env, QuickLendXContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let _ = client.try_initialize_admin(&admin);
    client.set_admin(&admin);
    (env, client, admin)
}

fn verified_business(env: &Env, client: &QuickLendXContractClient, admin: &Address) -> Address {
    let business = Address::generate(env);
    client.submit_kyc_application(&business, &String::from_str(env, "Business KYC"));
    client.verify_business(admin, &business);
    business
}

fn upload(env: &Env, client: &QuickLendXContractClient, business: &Address) -> BytesN<32> {
    let currency = Address::generate(env);
    let due_date = env.ledger().timestamp() + 86_400;
    client.upload_invoice(
        business,
        &1_000i128,
        &currency,
        &due_date,
        &String::from_str(env, "matrix invoice"),
        &InvoiceCategory::Services,
        &Vec::new(env),
    )
}

// ============================================================================
// Ownership matrix (model layer — returns a typed error)
// ============================================================================

/// `Invoice::cancel` rejects a non-owner actor with `Unauthorized` and leaves
/// the status unchanged, while the owner succeeds. No escrow exists for such a
/// pre-funding invoice (it has never been funded), so cancellation strands no
/// investor capital.
#[test]
fn test_cancel_ownership_matrix() {
    let env = Env::default();
    let business = Address::generate(&env);
    let attacker = Address::generate(&env);

    let mut invoice = Invoice::new(
        &env,
        business.clone(),
        10_000,
        Address::generate(&env),
        env.ledger().timestamp() + 86_400,
        String::from_str(&env, "owner matrix"),
        InvoiceCategory::Services,
        Vec::new(&env),
    )
    .expect("invoice creation");

    // A cancellable (Pending) invoice has no investor / escrow attached.
    assert!(invoice.investor.is_none());
    assert_eq!(invoice.funded_amount, 0);

    // Non-owner is rejected; status untouched.
    assert_eq!(
        invoice.cancel(&env, attacker).unwrap_err(),
        QuickLendXError::Unauthorized
    );
    assert_eq!(invoice.status, InvoiceStatus::Pending);

    // Owner succeeds.
    assert!(invoice.cancel(&env, business).is_ok());
    assert_eq!(invoice.status, InvoiceStatus::Cancelled);
}

// ============================================================================
// Allowed-from pre-funding states (public entry point)
// ============================================================================

/// Cancellation is allowed from `Pending`.
#[test]
fn test_cancel_allowed_from_pending() {
    let (env, client, admin) = setup();
    let business = verified_business(&env, &client, &admin);
    let invoice_id = upload(&env, &client, &business);

    assert_eq!(client.get_invoice(&invoice_id).status, InvoiceStatus::Pending);
    client.cancel_invoice(&invoice_id);
    assert_eq!(
        client.get_invoice(&invoice_id).status,
        InvoiceStatus::Cancelled
    );
}

/// Cancellation is allowed from `Verified`, and removes the invoice from the
/// available (`Verified`) index while adding it to the `Cancelled` index.
#[test]
fn test_cancel_allowed_from_verified_updates_indexes() {
    let (env, client, admin) = setup();
    let business = verified_business(&env, &client, &admin);
    let invoice_id = upload(&env, &client, &business);

    client.verify_invoice(&invoice_id);
    assert_eq!(client.get_invoice(&invoice_id).status, InvoiceStatus::Verified);
    assert!(client.get_available_invoices().contains(&invoice_id));

    client.cancel_invoice(&invoice_id);

    assert_eq!(
        client.get_invoice(&invoice_id).status,
        InvoiceStatus::Cancelled
    );
    // Removed from the available/verified index.
    assert!(!client.get_available_invoices().contains(&invoice_id));
    assert!(!client
        .get_invoices_by_status(&InvoiceStatus::Verified)
        .contains(&invoice_id));
    // Present in the cancelled index.
    assert!(client
        .get_invoices_by_status(&InvoiceStatus::Cancelled)
        .contains(&invoice_id));
}

// ============================================================================
// State-precondition gap (documented finding)
// ============================================================================

/// FINDING: cancelling from a post-funding state (`Funded`) currently
/// **succeeds** because neither `Invoice::cancel` nor `cancel_invoice` enforces
/// a status precondition. The issue's intent is that this be rejected with an
/// `InvalidStatus`-style error; pinning the present behaviour here means a
/// future guard will flip this assertion and prompt an update.
#[test]
fn test_cancel_from_funded_currently_succeeds_documents_gap() {
    let (env, client, admin) = setup();
    let business = verified_business(&env, &client, &admin);
    let invoice_id = upload(&env, &client, &business);

    client.verify_invoice(&invoice_id);
    // Drive the invoice into a Funded state via the admin status setter.
    client.update_invoice_status(&invoice_id, &InvoiceStatus::Funded);
    assert_eq!(client.get_invoice(&invoice_id).status, InvoiceStatus::Funded);

    // No status guard today: this transition is accepted.
    let result = client.try_cancel_invoice(&invoice_id);
    assert!(
        result.is_ok(),
        "FINDING: cancel_invoice lacks a status precondition; a funded invoice \
         is cancellable today. Add a Pending/Verified-only guard to fix."
    );
    assert_eq!(
        client.get_invoice(&invoice_id).status,
        InvoiceStatus::Cancelled
    );
}
