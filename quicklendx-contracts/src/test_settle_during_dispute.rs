//! Negative regression tests: settlement is blocked while a dispute is active.
//!
//! # Security gap being closed (defence-in-depth)
//!
//! `settle_invoice_internal()` previously called only `ensure_payable_status()`,
//! which checks `invoice.status == Funded`.  Disputes do **not** change that
//! field â€” the invoice stays `Funded` throughout its dispute lifecycle â€” so the
//! old code let a business finalize settlement while a dispute was open.
//!
//! ## Threat model
//!
//! An attacker (bad-faith business) with an open dispute could:
//!
//! 1. Open a dispute (`create_dispute`) to signal a contested state and
//!    pre-occupy the investor's and admin's attention.
//! 2. Immediately call `settle_invoice` (or `process_partial_payment` with
//!    the full remaining amount), which â€” without the fix â€” succeeds because
//!    `ensure_payable_status()` only checks `invoice.status == Funded`.
//! 3. This triggers `settle_invoice_internal`, which:
//!    a. Releases the escrowed investor funds to the business (or investor
//!       return path) **before** the admin has ruled on the dispute.
//!    b. Marks the invoice `Paid` â€” a terminal state â€” permanently closing
//!       the `refund_escrow` pathway the investor would need if the dispute
//!       resolved in their favour.
//! 4. Result: the investor cannot recover their principal even if the dispute
//!    ruling later finds in their favour, because the settlement is final.
//!
//! ## Fix
//!
//! `settle_invoice_internal` now runs a second guard after
//! `ensure_payable_status()`:
//!
//! ```text
//! // Block only the two open states; Resolved means admin has ruled.
//! if matches!(dispute_status, Disputed | UnderReview) {
//!     return Err(QuickLendXError::DisputeActive);  // error 2204
//! }
//! ```
//!
//! `Resolved` is intentionally allowed: once the admin has issued a ruling
//! the dispute is concluded, so a business-favourable resolution can proceed
//! to settlement normally.
//!
//! ## Test matrix
//!
//! | Test                                                        | State         | Expected         |
//! |-------------------------------------------------------------|---------------|------------------|
//! | `test_settle_blocked_while_disputed`                        | `Disputed`    | `DisputeActive`  |
//! | `test_settle_blocked_while_under_review`                    | `UnderReview` | `DisputeActive`  |
//! | `test_settle_allowed_after_dispute_resolved`                | `Resolved`    | not DisputeActive|
//! | `test_partial_payment_finalization_blocked_while_disputed`  | `Disputed`    | `DisputeActive`  |
//!
//! The first two (and last) tests would **fail before the fix** â€” settlement
//! would succeed when it must not â€” and **pass after** it.

#![cfg(test)]

use super::*; // brings QuickLendXContract + QuickLendXContractClient into scope

use crate::errors::QuickLendXError;
use crate::types::{DisputeStatus, InvoiceCategory, InvoiceStatus};
use soroban_sdk::{testutils::Address as _, token, Address, BytesN, Env, String, Vec};

// â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
// Shared setup helper
// â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

/// Boot a full contract instance and return a `Funded` invoice ready for
/// settlement or dispute operations.
///
/// Returns `(client, invoice_id, business, investor, contract_id)`.
///
/// Mirrors the setup used in `test_settlement_accounting_identity.rs` so both
/// test suites exercise the same initialization path.
fn setup_funded_invoice(
    env: &Env,
) -> (
    QuickLendXContractClient,
    BytesN<32>,
    Address,
    Address,
    Address,
) {
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(env, &contract_id);

    let admin = Address::generate(env);
    let business = Address::generate(env);
    let investor = Address::generate(env);

    // Initialize fee subsystem (matches existing test pattern).
    client.set_admin(&admin);
    client.initialize_fee_system(&admin);
    client.update_platform_fee_bps(&200u32); // 2 %

    // Create a real Stellar Asset Contract so token transfers succeed.
    let token_admin = Address::generate(env);
    let currency = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token_client = token::Client::new(env, &currency);
    let sac = token::StellarAssetClient::new(env, &currency);

    let balance: i128 = 500_000;
    sac.mint(&business, &balance);
    sac.mint(&investor, &balance);

    // Approve the contract to spend on behalf of both parties.
    let expiry = env.ledger().sequence() + 10_000;
    token_client.approve(&business, &contract_id, &balance, &expiry);
    token_client.approve(&investor, &contract_id, &balance, &expiry);

    // KYC â€” both parties must be verified before transacting.
    client.submit_kyc_application(&business, &String::from_str(env, "business-kyc"));
    client.verify_business(&admin, &business);
    client.submit_investor_kyc(&investor, &String::from_str(env, "investor-kyc"));
    client.verify_investor(&investor, &balance);

    // Create invoice â†’ verify â†’ bid â†’ accept (funds escrowed, invoice = Funded).
    let amount: i128 = 100_000;
    let due_date = env.ledger().timestamp() + 86_400;
    let invoice_id = client.store_invoice(
        &business,
        &amount,
        &currency,
        &due_date,
        &String::from_str(env, "Dispute-settlement interaction test invoice"),
        &InvoiceCategory::Services,
        &Vec::new(env),
        &None);
    client.verify_invoice(&invoice_id);
    let bid_id = client.place_bid(&investor, &invoice_id, &amount, &amount, &BytesN::from_array(&env, &[0u8; 32]));
    client.accept_bid(&invoice_id, &bid_id);

    (client, invoice_id, business, investor, contract_id)
}

// â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
// Negative tests (these failed BEFORE the fix)
// â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

/// `settle_invoice` MUST return `DisputeActive` (2204) when `dispute_status == Disputed`.
///
/// Before the fix: `settle_invoice` returned `Ok(())` and marked the invoice
/// `Paid` while a dispute was unresolved.
/// After the fix: `settle_invoice_internal` checks `dispute_status` before
/// touching funds and returns `Err(DisputeActive)`.
#[test]
fn test_settle_blocked_while_disputed() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, invoice_id, _business, investor, _contract_id) = setup_funded_invoice(&env);

    // Open a dispute â€” investor claims breach.
    client.create_dispute(
        &invoice_id,
        &investor,
        &String::from_str(&env, "Business failed to deliver goods per contract"),
        &String::from_str(&env, "Signed delivery receipt shows non-conforming goods"),
    );

    // Verify the pre-condition: dispute is now open.
    let inv = client.get_invoice(&invoice_id);
    assert_eq!(
        inv.dispute_status,
        DisputeStatus::Disputed,
        "pre-condition: invoice must have an open dispute"
    );

    // Attempt full settlement while the dispute is active â€” MUST FAIL.
    let result = client.try_settle_invoice(&invoice_id, &100_000i128, &client.get_investment(&invoice_id).unwrap());

    assert_eq!(
        result.unwrap_err().unwrap(),
        QuickLendXError::DisputeActive,
        "settle_invoice must return DisputeActive (2204) when dispute_status == Disputed"
    );

    // Invoice must still be Funded â€” no funds moved, no terminal state reached.
    let inv_after = client.get_invoice(&invoice_id);
    assert_eq!(
        inv_after.status,
        InvoiceStatus::Funded,
        "invoice must remain Funded after a blocked settlement attempt"
    );
}

/// `settle_invoice` MUST return `DisputeActive` (2204) when `dispute_status == UnderReview`.
///
/// `UnderReview` is a distinct step (admin has acknowledged the dispute but not
/// yet resolved it).  Settlement must be blocked in this state just as much as
/// in `Disputed`.
#[test]
fn test_settle_blocked_while_under_review() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, invoice_id, business, _investor, _contract_id) = setup_funded_invoice(&env);

    let admin = client
        .get_current_admin()
        .expect("admin must be set after initialization");

    // Business opens a dispute, then admin advances it to UnderReview.
    client.create_dispute(
        &invoice_id,
        &business,
        &String::from_str(&env, "Investor delayed release of payment"),
        &String::from_str(&env, "Ledger timestamp shows payment overdue by 3 days"),
    );
    // Note: put_dispute_under_review(invoice_id, admin) â€” invoice_id is first.
    client.put_dispute_under_review(&invoice_id, &admin);

    let inv = client.get_invoice(&invoice_id);
    assert_eq!(
        inv.dispute_status,
        DisputeStatus::UnderReview,
        "pre-condition: dispute must be UnderReview"
    );

    // Settlement attempt while admin review is in progress â€” MUST FAIL.
    let result = client.try_settle_invoice(&invoice_id, &100_000i128, &client.get_investment(&invoice_id).unwrap());

    assert_eq!(
        result.unwrap_err().unwrap(),
        QuickLendXError::DisputeActive,
        "settle_invoice must return DisputeActive (2204) when dispute_status == UnderReview"
    );

    let inv_after = client.get_invoice(&invoice_id);
    assert_eq!(
        inv_after.status,
        InvoiceStatus::Funded,
        "invoice must remain Funded after blocked settlement during UnderReview"
    );
}

/// After a dispute is `Resolved`, `settle_invoice` must NOT return `DisputeActive`.
///
/// `Resolved` means the admin has issued a ruling; the dispute is concluded.
/// A business-favourable resolution must re-enable the settlement path.
/// This positive counterpart prevents the guard from being over-broad.
#[test]
fn test_settle_allowed_after_dispute_resolved() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, invoice_id, _business, investor, _contract_id) = setup_funded_invoice(&env);

    let admin = client
        .get_current_admin()
        .expect("admin must be set after initialization");

    // Full dispute lifecycle: open â†’ under_review â†’ resolved.
    client.create_dispute(
        &invoice_id,
        &investor,
        &String::from_str(&env, "Quality dispute over delivered goods"),
        &String::from_str(&env, "Independent inspection report attached"),
    );
    // put_dispute_under_review(invoice_id, admin)
    client.put_dispute_under_review(&invoice_id, &admin);
    // resolve_dispute(invoice_id, admin, resolution)
    client.resolve_dispute(
        &invoice_id,
        &admin,
        &String::from_str(
            &env,
            "Ruling: goods conform to spec. Dispute dismissed. Settlement may proceed.",
        ),
    );

    let inv = client.get_invoice(&invoice_id);
    assert_eq!(
        inv.dispute_status,
        DisputeStatus::Resolved,
        "pre-condition: dispute must be Resolved before testing settlement unblock"
    );

    // Settlement must NOT be blocked by the dispute-active guard now.
    let result = client.try_settle_invoice(&invoice_id, &100_000i128, &client.get_investment(&invoice_id).unwrap());
    assert_ne!(
        result.err().and_then(|e| e.ok()),
        Some(QuickLendXError::DisputeActive),
        "settle_invoice must NOT return DisputeActive after dispute is Resolved"
    );
}

/// `process_partial_payment` that brings `total_paid` to `invoice.amount`
/// internally calls `settle_invoice_internal` â€” that finalization path must
/// also be blocked while a dispute is open.
///
/// Call chain: `process_partial_payment` â†’ `record_payment` â†’ `settle_invoice_internal`
#[test]
fn test_partial_payment_finalization_blocked_while_disputed() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, invoice_id, business, _investor, _contract_id) = setup_funded_invoice(&env);

    // Record a first partial payment (50 %) before any dispute â€” must succeed.
    client
        .try_process_partial_payment(
            &invoice_id,
            &50_000i128,
            &String::from_str(&env, "payment-001"),
        )
        .expect("first partial payment must succeed before any dispute is open");

    // Open a dispute mid-payment sequence.
    client.create_dispute(
        &invoice_id,
        &business,
        &String::from_str(&env, "Investor disputes partial payment allocation"),
        &String::from_str(&env, "Bank statement shows discrepancy"),
    );

    // Second payment brings total to 100 % â€” would normally trigger finalization.
    // With the guard in place this MUST be blocked.
    let result = client.try_process_partial_payment(
        &invoice_id,
        &50_000i128,
        &String::from_str(&env, "payment-002"),
    );

    assert_eq!(
        result.unwrap_err().unwrap(),
        QuickLendXError::DisputeActive,
        "process_partial_payment finalization must be blocked while dispute_status == Disputed"
    );

    // Invoice must still be Funded, not Paid.
    let inv = client.get_invoice(&invoice_id);
    assert_eq!(
        inv.status,
        InvoiceStatus::Funded,
        "invoice must remain Funded after the blocked finalization"
    );
    // Only the pre-dispute payment must be reflected.
    assert_eq!(
        inv.total_paid, 50_000,
        "total_paid must reflect only the payment recorded before the dispute"
    );
}

/// Happy Path (Cleared): Full settlement succeeds when `dispute_status == None`.
///
/// "Cleared" means no investigation was ever opened on the invoice.  The
/// active-investigation guard is a no-op in this state and normal settlement
/// must proceed without any dispute-related error.
#[test]
fn test_settle_succeeds_when_dispute_status_is_none() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, invoice_id, _business, _investor, _contract_id) = setup_funded_invoice(&env);

    let inv = client.get_invoice(&invoice_id);
    assert_eq!(
        inv.dispute_status,
        DisputeStatus::None,
        "pre-condition: invoice must start with dispute_status == None (cleared)"
    );

    let result = client.try_settle_invoice(&invoice_id, &100_000i128, &client.get_investment(&invoice_id).unwrap());
    assert!(
        result.is_ok(),
        "settle_invoice must succeed when dispute_status == None (cleared), got {:?}",
        result.err()
    );

    let inv_after = client.get_invoice(&invoice_id);
    assert_eq!(
        inv_after.status,
        InvoiceStatus::Paid,
        "invoice must reach Paid after a successful cleared-state settlement"
    );
}

/// Happy Path (Cleared): Non-finalizing partial payment succeeds with
/// `dispute_status == None`.
///
/// Documents that the guard never interferes with progress payments when the
/// invoice has never entered a dispute lifecycle.
#[test]
fn test_partial_payment_succeeds_when_dispute_status_is_none() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, invoice_id, _business, _investor, _contract_id) = setup_funded_invoice(&env);

    let res = client.try_process_partial_payment(
        &invoice_id,
        &30_000i128,
        &String::from_str(&env, "deposit-1"),
    );
    assert!(
        res.is_ok(),
        "partial payment must succeed on cleared invoice, got {:?}",
        res.err()
    );

    let inv = client.get_invoice(&invoice_id);
    assert_eq!(inv.total_paid, 30_000, "total_paid must reflect the partial payment");
    assert_eq!(
        inv.status,
        InvoiceStatus::Funded,
        "invoice must remain Funded after a non-final partial payment"
    );
}

/// Sad Path (Active): `process_partial_payment` finalization is blocked when
/// `dispute_status == UnderReview`.
///
/// Mirror of `test_partial_payment_finalization_blocked_while_disputed` for the
/// `UnderReview` state â€” admin has acknowledged the dispute (investigation is
/// active and ongoing), so final settlement must remain blocked.
#[test]
fn test_partial_payment_finalization_blocked_while_under_review() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, invoice_id, business, _investor, _contract_id) = setup_funded_invoice(&env);
    let admin = client
        .get_current_admin()
        .expect("admin must be set after initialization");

    // 50 % payment before any dispute â€” allowed (cleared state).
    client
        .try_process_partial_payment(
            &invoice_id,
            &50_000i128,
            &String::from_str(&env, "payment-001"),
        )
        .expect("first partial payment must succeed while cleared");

    // Open dispute and advance to UnderReview (active investigation).
    client.create_dispute(
        &invoice_id,
        &business,
        &String::from_str(&env, "Partial payment amount disputed by business"),
        &String::from_str(&env, "Wire transfer confirmation attached"),
    );
    client.put_dispute_under_review(&invoice_id, &admin);

    let inv = client.get_invoice(&invoice_id);
    assert_eq!(
        inv.dispute_status,
        DisputeStatus::UnderReview,
        "pre-condition: dispute must be UnderReview (active investigation)"
    );

    // Finalising payment â€” MUST be rejected by the active-investigation guard.
    let result = client.try_process_partial_payment(
        &invoice_id,
        &50_000i128,
        &String::from_str(&env, "payment-002"),
    );

    assert_eq!(
        result.unwrap_err().unwrap(),
        QuickLendXError::DisputeActive,
        "process_partial_payment finalization must return DisputeActive while dispute_status == UnderReview"
    );

    let inv_after = client.get_invoice(&invoice_id);
    assert_eq!(
        inv_after.status,
        InvoiceStatus::Funded,
        "invoice must stay Funded after blocked UnderReview finalization"
    );
    assert_eq!(
        inv_after.total_paid, 50_000,
        "total_paid must NOT include the rejected finalising payment"
    );
}

/// Happy Path (Resolved): `process_partial_payment` finalization succeeds once
/// the dispute reaches `Resolved`.
///
/// Once the admin issues a ruling the investigation is closed and normal
/// settlement paths must be re-opened so a business-favourable resolution can
/// complete normally.
#[test]
fn test_partial_payment_finalization_succeeds_after_dispute_resolved() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, invoice_id, business, _investor, _contract_id) = setup_funded_invoice(&env);
    let admin = client
        .get_current_admin()
        .expect("admin must be set after initialization");

    // 50 % payment â†’ open dispute â†’ review â†’ resolve.
    client
        .try_process_partial_payment(
            &invoice_id,
            &50_000i128,
            &String::from_str(&env, "payment-001"),
        )
        .expect("first partial payment must succeed while cleared");

    client.create_dispute(
        &invoice_id,
        &business,
        &String::from_str(&env, "Counterparty raised quality concern"),
        &String::from_str(&env, "Photo evidence attached"),
    );
    client.put_dispute_under_review(&invoice_id, &admin);
    client.resolve_dispute(
        &invoice_id,
        &admin,
        &String::from_str(
            &env,
            "Ruling: Business position upheld. Investor provided no contrary evidence.",
        ),
    );

    let inv = client.get_invoice(&invoice_id);
    assert_eq!(
        inv.dispute_status,
        DisputeStatus::Resolved,
        "pre-condition: dispute must be Resolved before attempting finalization"
    );

    // Now the remaining 50 % â€” must succeed, investigation is closed.
    let res = client.try_process_partial_payment(
        &invoice_id,
        &50_000i128,
        &String::from_str(&env, "payment-002"),
    );
    assert!(
        res.is_ok(),
        "finalising partial payment must succeed after dispute is Resolved, got {:?}",
        res.err()
    );

    let inv_after = client.get_invoice(&invoice_id);
    assert_eq!(
        inv_after.status,
        InvoiceStatus::Paid,
        "invoice must reach Paid once finalization succeeds in Resolved state"
    );
}

/// Happy Path (Resolved via structured): `settle_invoice` succeeds after a
/// `FavorBusiness` ruling issued through `resolve_dispute_structured`.
///
/// Guards the explicit policy: `DisputeStatus::Resolved` unblocks settlement
/// regardless of which resolution variant (text-only vs structured) was used
/// by the admin.
#[test]
fn test_settle_succeeds_after_structured_resolution_favor_business() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, invoice_id, _business, investor, _contract_id) = setup_funded_invoice(&env);
    let admin = client
        .get_current_admin()
        .expect("admin must be set after initialization");

    client.create_dispute(
        &invoice_id,
        &investor,
        &String::from_str(&env, "Non-delivery claim"),
        &String::from_str(&env, "Customer support ticket attached"),
    );
    client.put_dispute_under_review(&invoice_id, &admin);
    client.resolve_dispute_structured(
        &invoice_id,
        &admin,
        &crate::types::DisputeResolution::FavorBusiness,
        &String::from_str(
            &env,
            "Structured ruling: delivery confirmed on blockchain, FavorBusiness.",
        ),
    );

    let inv = client.get_invoice(&invoice_id);
    assert_eq!(
        inv.dispute_status,
        DisputeStatus::Resolved,
        "pre-condition: structured resolution must transition status to Resolved"
    );

    let result = client.try_settle_invoice(&invoice_id, &100_000i128, &client.get_investment(&invoice_id).unwrap());
    assert_ne!(
        result.as_ref().err().and_then(|e| e.as_ref().ok()).copied(),
        Some(QuickLendXError::DisputeActive),
        "settle_invoice must NOT be blocked after structured FavorBusiness resolution"
    );
    assert!(
        result.is_ok(),
        "settle_invoice must succeed entirely after structured FavorBusiness, got {:?}",
        result.as_ref().err()
    );

    let inv_after = client.get_invoice(&invoice_id);
    assert_eq!(
        inv_after.status,
        InvoiceStatus::Paid,
        "invoice must be Paid after successful Resolved-state settlement"
    );
}

/// Sad Path (Active): `settle_invoice` during `UnderReview` returns
/// `DisputeActive` specifically, never `InvalidStatus`.
///
/// Locks in the explicit error contract: the active-investigation guard fires
/// *before* downstream payable-status checks, so clients see the dedicated
/// dispute-error code and can surface "unresolved dispute" in the UI instead
/// of a generic "wrong lifecycle state".
#[test]
fn test_settle_under_review_returns_dispute_active_not_invalid_status() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, invoice_id, business, _investor, _contract_id) = setup_funded_invoice(&env);
    let admin = client
        .get_current_admin()
        .expect("admin must be set after initialization");

    client.create_dispute(
        &invoice_id,
        &business,
        &String::from_str(&env, "Investigation guard error-code boundary test"),
        &String::from_str(&env, "Placeholder evidence"),
    );
    client.put_dispute_under_review(&invoice_id, &admin);

    // Invoice is still Funded (dispute doesn't change status), so if the
    // guard were missing or reordered we'd see InvalidStatus instead of
    // DisputeActive.  Pin the correct ordering here.
    let err = client
        .try_settle_invoice(&invoice_id, &100_000i128)
        .unwrap_err()
        .unwrap();

    assert_eq!(
        err,
        QuickLendXError::DisputeActive,
        "settle_invoice on UnderReview must return DisputeActive (investigation guard fires before other status checks); got {:?}",
        err
    );
    assert_ne!(
        err,
        QuickLendXError::InvalidStatus,
        "DisputeActive must be strictly distinct from InvalidStatus for off-chain clients"
    );
}

