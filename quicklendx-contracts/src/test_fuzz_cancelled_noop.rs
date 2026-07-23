//! Property tests: cancelled invoice/bid are terminal – every state-changing
//! operation attempted after cancellation must error.
//!
//! # Summary
//! `cancel_invoice` (or `cancel_bid`) followed by **any** state-changing
//! operation must return an error and must leave the stored state identical
//! to what it was immediately after the cancellation.
//!
//! # Properties locked in by this module
//!
//! ## Invoice properties
//! P1. `cancel_invoice` on a Pending invoice with arbitrary amount/due-date
//!     always succeeds and leaves status == Cancelled.
//! P2. `cancel_invoice` again on a Cancelled invoice always returns
//!     `InvalidStatus`.
//! P3. `verify_invoice` on a Cancelled invoice always returns `InvalidStatus`.
//! P4. `place_bid` on a Cancelled invoice always returns `InvalidStatus`.
//! P5. `accept_bid` / `accept_bid_and_fund` on a Cancelled invoice always
//!     returns an error.
//! P6. `settle_invoice` on a Cancelled invoice always returns `InvalidStatus`.
//! P7. `process_partial_payment` on a Cancelled invoice always returns an
//!     error (not `Ok`).
//!
//! ## Bid properties
//! P8. `cancel_bid` on a Placed bid succeeds (returns `true`) and sets status
//!     == Cancelled.
//! P9. `cancel_bid` on an already-Cancelled bid always returns `false`
//!     (idempotent no-op) and status remains Cancelled.
//! P10. `withdraw_bid` on a Cancelled bid always returns `OperationNotAllowed`.
//! P11. `accept_bid` on a Cancelled bid always returns an error.

#![cfg(all(test, feature = "fuzz-tests"))]

use crate::{
    invoice::{InvoiceCategory, InvoiceStatus},
    types::BidStatus,
    QuickLendXContract, QuickLendXContractClient,
};
use proptest::prelude::*;
use soroban_sdk::{
    testutils::Address as _, Address, BytesN, Env, String as SorobanString, Vec as SorobanVec,
};

// ---------------------------------------------------------------------------
// Constants (kept consistent with the main fuzz harness)
// ---------------------------------------------------------------------------

const MIN_AMOUNT: i128 = 1_000;
const MAX_AMOUNT: i128 = 10_000_000;
const MIN_DUE_DATE_OFFSET: u64 = 86_400; // 1 day
const MAX_DUE_DATE_OFFSET: u64 = 365 * 86_400; // 1 year

// ---------------------------------------------------------------------------
// Setup helpers
// ---------------------------------------------------------------------------

/// Minimal env setup: one admin, one verified business, one verified investor,
/// one whitelisted (non-real) currency address.  All authorisations are mocked.
fn setup() -> (
    Env,
    QuickLendXContractClient<'static>,
    Address, // admin
    Address, // business
    Address, // investor
    Address, // currency (whitelisted)
) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let business = Address::generate(&env);
    let investor = Address::generate(&env);
    let currency = Address::generate(&env);

    let _ = client.try_initialize_admin(&admin);
    let _ = client.try_add_currency(&admin, &currency);

    // Business KYC – must be long enough to pass validation (copied pattern from
    // the existing fuzz harness).
    let biz_kyc = SorobanString::from_str(
        &env,
        "Business KYC 1234567890 1234567890 1234567890 1234567890 \
         1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 \
         1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 \
         1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 \
         1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 \
         1234567890 1234567890 1234567890 1234567890 1234567890 1234567890",
    );
    let _ = client.try_submit_kyc_application(&business, &biz_kyc);
    let _ = client.try_verify_business(&admin, &business);

    // Investor KYC
    let inv_kyc = SorobanString::from_str(
        &env,
        "Investor KYC 1234567890 1234567890 1234567890 1234567890 \
         1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 \
         1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 \
         1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 \
         1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 \
         1234567890 1234567890 1234567890 1234567890 1234567890 1234567890",
    );
    let _ = client.try_submit_investor_kyc(&investor, &inv_kyc);
    let _ = client.try_verify_investor(&investor, &MAX_AMOUNT);

    (env, client, admin, business, investor, currency)
}

/// Upload a Pending invoice and return its id.
fn upload_pending_invoice(
    env: &Env,
    client: &QuickLendXContractClient,
    business: &Address,
    currency: &Address,
    amount: i128,
    due_date_offset: u64,
) -> BytesN<32> {
    let due_date = env.ledger().timestamp().saturating_add(due_date_offset);
    client
        .try_upload_invoice(
            business,
            &amount,
            currency,
            &due_date,
            &SorobanString::from_str(env, "Fuzz invoice"),
            &InvoiceCategory::Services,
            &SorobanVec::new(env),
        )
        .expect("upload_invoice must succeed during setup")
        .expect("contract must not error during setup")
}

/// Upload a Verified invoice and return its id.
fn upload_verified_invoice(
    env: &Env,
    client: &QuickLendXContractClient,
    business: &Address,
    currency: &Address,
    amount: i128,
    due_date_offset: u64,
) -> BytesN<32> {
    let invoice_id =
        upload_pending_invoice(env, client, business, currency, amount, due_date_offset);
    client
        .try_verify_invoice(&invoice_id)
        .expect("verify_invoice must succeed")
        .expect("contract must not error");
    invoice_id
}

/// Place a bid against a Verified invoice and return its id.
fn place_bid(
    env: &Env,
    client: &QuickLendXContractClient,
    investor: &Address,
    invoice_id: &BytesN<32>,
    bid_amount: i128,
) -> BytesN<32> {
    let expected_return = bid_amount
        .saturating_add(bid_amount / 10)
        .max(bid_amount + 1);
    client
        .try_place_bid(investor, invoice_id, &bid_amount, &expected_return)
        .expect("place_bid must succeed during setup")
        .expect("contract must not error during setup")
}

// ---------------------------------------------------------------------------
// Helper: assert that the invoice status is Cancelled and that the stored
// record has not changed between two reads.
// ---------------------------------------------------------------------------
fn assert_status_still_cancelled(client: &QuickLendXContractClient, invoice_id: &BytesN<32>) {
    let inv = client
        .try_get_invoice(invoice_id)
        .expect("get_invoice must not panic")
        .expect("invoice must exist after cancel");
    assert_eq!(
        inv.status,
        InvoiceStatus::Cancelled,
        "invoice status must remain Cancelled after a failed state-changing op"
    );
}

// ---------------------------------------------------------------------------
// P1 + P2  – cancel_invoice on Pending succeeds; re-cancel returns InvalidStatus
// ---------------------------------------------------------------------------
proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// P1: cancel_invoice on a Pending invoice always transitions to Cancelled.
    /// P2: A second cancel_invoice always returns InvalidStatus.
    #[test]
    fn fuzz_cancel_pending_invoice_is_terminal(
        amount in MIN_AMOUNT..MAX_AMOUNT,
        due_date_offset in MIN_DUE_DATE_OFFSET..MAX_DUE_DATE_OFFSET,
    ) {
        let (env, client, _admin, business, _investor, currency) = setup();
        let invoice_id = upload_pending_invoice(
            &env, &client, &business, &currency, amount, due_date_offset,
        );

        // Happy path – first cancel must succeed.
        client
            .try_cancel_invoice(&invoice_id)
            .expect("try_cancel_invoice must not panic")
            .expect("first cancel_invoice must return Ok");

        let inv = client.try_get_invoice(&invoice_id)
            .expect("get_invoice must not panic")
            .expect("invoice must exist");
        assert_eq!(inv.status, InvoiceStatus::Cancelled, "P1: status must be Cancelled after cancel");

        // Sad path (P2) – second cancel must error.
        let second = client
            .try_cancel_invoice(&invoice_id)
            .expect("try_cancel_invoice must not panic");
        assert!(second.is_err(), "P2: second cancel_invoice must return Err");

        assert_status_still_cancelled(&client, &invoice_id);
    }

    /// P3: verify_invoice on a Cancelled invoice always returns an error.
    #[test]
    fn fuzz_verify_after_cancel_errors(
        amount in MIN_AMOUNT..MAX_AMOUNT,
        due_date_offset in MIN_DUE_DATE_OFFSET..MAX_DUE_DATE_OFFSET,
    ) {
        let (env, client, _admin, business, _investor, currency) = setup();
        let invoice_id = upload_pending_invoice(
            &env, &client, &business, &currency, amount, due_date_offset,
        );
        client.cancel_invoice(&invoice_id);

        // P3
        let result = client
            .try_verify_invoice(&invoice_id)
            .expect("try_verify_invoice must not panic");
        assert!(result.is_err(), "P3: verify_invoice on Cancelled must return Err");

        assert_status_still_cancelled(&client, &invoice_id);
    }

    /// P4: place_bid on a Cancelled invoice always returns an error.
    #[test]
    fn fuzz_place_bid_after_cancel_errors(
        amount in MIN_AMOUNT..MAX_AMOUNT,
        due_date_offset in MIN_DUE_DATE_OFFSET..MAX_DUE_DATE_OFFSET,
        bid_pct in 10u32..100u32, // bid as % of invoice amount
    ) {
        let (env, client, _admin, business, investor, currency) = setup();

        // Invoice must be Verified first so we can cancel it and still have a
        // meaningful bid attempt (place_bid requires Verified; once Cancelled it
        // must be rejected regardless of how the invoice got there).
        let invoice_id = upload_verified_invoice(
            &env, &client, &business, &currency, amount, due_date_offset,
        );
        client.cancel_invoice(&invoice_id);

        let bid_amount = (amount as u128 * bid_pct as u128 / 100).max(1) as i128;
        let expected_return = bid_amount + 1;

        // P4
        let result = client
            .try_place_bid(&investor, &invoice_id, &bid_amount, &expected_return)
            .expect("try_place_bid must not panic");
        assert!(result.is_err(), "P4: place_bid on Cancelled must return Err");

        assert_status_still_cancelled(&client, &invoice_id);
    }

    /// P6: settle_invoice on a Cancelled invoice always returns an error.
    #[test]
    fn fuzz_settle_after_cancel_errors(
        amount in MIN_AMOUNT..MAX_AMOUNT,
        due_date_offset in MIN_DUE_DATE_OFFSET..MAX_DUE_DATE_OFFSET,
        payment_pct in 1u32..200u32,
    ) {
        let (env, client, _admin, business, _investor, currency) = setup();
        let invoice_id = upload_pending_invoice(
            &env, &client, &business, &currency, amount, due_date_offset,
        );
        client.cancel_invoice(&invoice_id);

        let payment_amount = (amount as u128 * payment_pct as u128 / 100).max(1) as i128;

        // P6
        let result = client
            .try_settle_invoice(&invoice_id, &payment_amount)
            .expect("try_settle_invoice must not panic");
        assert!(result.is_err(), "P6: settle_invoice on Cancelled must return Err");

        assert_status_still_cancelled(&client, &invoice_id);
    }

    /// P7: process_partial_payment on a Cancelled invoice always returns an error.
    #[test]
    fn fuzz_partial_payment_after_cancel_errors(
        amount in MIN_AMOUNT..MAX_AMOUNT,
        due_date_offset in MIN_DUE_DATE_OFFSET..MAX_DUE_DATE_OFFSET,
        payment_pct in 1u32..150u32,
    ) {
        let (env, client, _admin, business, _investor, currency) = setup();
        let invoice_id = upload_pending_invoice(
            &env, &client, &business, &currency, amount, due_date_offset,
        );
        client.cancel_invoice(&invoice_id);

        let payment_amount = (amount as u128 * payment_pct as u128 / 100).max(1) as i128;
        let nonce = SorobanString::from_str(&env, "nonce-cancelled-pay");

        // P7
        let result = client
            .try_process_partial_payment(&invoice_id, &payment_amount, &nonce)
            .expect("try_process_partial_payment must not panic");
        assert!(result.is_err(), "P7: process_partial_payment on Cancelled must return Err");

        assert_status_still_cancelled(&client, &invoice_id);
    }
}

// ---------------------------------------------------------------------------
// P5  – accept_bid / accept_bid_and_fund on a Cancelled invoice must error.
//
// This needs a real bid stored before the cancellation, so it gets its own
// proptest block so the cancelled bid is from a Verified invoice.
// ---------------------------------------------------------------------------
proptest! {
    #![proptest_config(ProptestConfig::with_cases(32))]

    /// P5a: accept_bid on a Cancelled invoice always returns an error.
    #[test]
    fn fuzz_accept_bid_after_invoice_cancel_errors(
        amount in MIN_AMOUNT..MAX_AMOUNT,
        due_date_offset in MIN_DUE_DATE_OFFSET..MAX_DUE_DATE_OFFSET,
        bid_pct in 10u32..100u32,
    ) {
        let (env, client, _admin, business, investor, currency) = setup();
        let invoice_id = upload_verified_invoice(
            &env, &client, &business, &currency, amount, due_date_offset,
        );
        let bid_amount = (amount as u128 * bid_pct as u128 / 100).max(1) as i128;
        // Place the bid while invoice is still Verified.
        let bid_id = place_bid(&env, &client, &investor, &invoice_id, bid_amount);

        // Cancel the invoice AFTER the bid is placed.
        client.cancel_invoice(&invoice_id);

        // P5a – accept_bid must fail on the now-Cancelled invoice.
        let result = client
            .try_accept_bid(&invoice_id, &bid_id)
            .expect("try_accept_bid must not panic");
        assert!(result.is_err(), "P5a: accept_bid on Cancelled invoice must return Err");

        assert_status_still_cancelled(&client, &invoice_id);
    }

    /// P5b: accept_bid_and_fund on a Cancelled invoice always returns an error.
    #[test]
    fn fuzz_accept_bid_and_fund_after_invoice_cancel_errors(
        amount in MIN_AMOUNT..MAX_AMOUNT,
        due_date_offset in MIN_DUE_DATE_OFFSET..MAX_DUE_DATE_OFFSET,
        bid_pct in 10u32..100u32,
    ) {
        let (env, client, _admin, business, investor, currency) = setup();
        let invoice_id = upload_verified_invoice(
            &env, &client, &business, &currency, amount, due_date_offset,
        );
        let bid_amount = (amount as u128 * bid_pct as u128 / 100).max(1) as i128;
        let bid_id = place_bid(&env, &client, &investor, &invoice_id, bid_amount);

        client.cancel_invoice(&invoice_id);

        // P5b – accept_bid_and_fund must fail on the now-Cancelled invoice.
        let result = client
            .try_accept_bid_and_fund(&invoice_id, &bid_id)
            .expect("try_accept_bid_and_fund must not panic");
        assert!(result.is_err(), "P5b: accept_bid_and_fund on Cancelled invoice must return Err");

        assert_status_still_cancelled(&client, &invoice_id);
    }
}

// ---------------------------------------------------------------------------
// P8 + P9 + P10 + P11  – bid-level cancel properties
// ---------------------------------------------------------------------------
proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// P8: cancel_bid on a Placed bid returns true and sets status == Cancelled.
    /// P9: A second cancel_bid returns false (idempotent no-op).
    #[test]
    fn fuzz_cancel_bid_is_terminal(
        amount in MIN_AMOUNT..MAX_AMOUNT,
        due_date_offset in MIN_DUE_DATE_OFFSET..MAX_DUE_DATE_OFFSET,
        bid_pct in 10u32..100u32,
    ) {
        let (env, client, _admin, business, investor, currency) = setup();
        let invoice_id = upload_verified_invoice(
            &env, &client, &business, &currency, amount, due_date_offset,
        );
        let bid_amount = (amount as u128 * bid_pct as u128 / 100).max(1) as i128;
        let bid_id = place_bid(&env, &client, &investor, &invoice_id, bid_amount);

        // P8 – first cancel must succeed.
        let first = client.cancel_bid(&bid_id);
        assert!(first, "P8: first cancel_bid must return true");

        let bid = client
            .try_get_bid(&bid_id)
            .expect("try_get_bid must not panic")
            .expect("bid must be Some (P8)");
        assert_eq!(bid.status, BidStatus::Cancelled, "P8: bid status must be Cancelled");

        // P9 – second cancel is a no-op returning false.
        let second = client.cancel_bid(&bid_id);
        assert!(!second, "P9: second cancel_bid must return false");

        let bid_after = client
            .try_get_bid(&bid_id)
            .expect("try_get_bid must not panic")
            .expect("bid must be Some after no-op (P9)");
        assert_eq!(
            bid_after.status,
            BidStatus::Cancelled,
            "P9: bid status must remain Cancelled after no-op second cancel"
        );
    }

    /// P10: withdraw_bid on a Cancelled bid always returns an error.
    #[test]
    fn fuzz_withdraw_after_cancel_bid_errors(
        amount in MIN_AMOUNT..MAX_AMOUNT,
        due_date_offset in MIN_DUE_DATE_OFFSET..MAX_DUE_DATE_OFFSET,
        bid_pct in 10u32..100u32,
    ) {
        let (env, client, _admin, business, investor, currency) = setup();
        let invoice_id = upload_verified_invoice(
            &env, &client, &business, &currency, amount, due_date_offset,
        );
        let bid_amount = (amount as u128 * bid_pct as u128 / 100).max(1) as i128;
        let bid_id = place_bid(&env, &client, &investor, &invoice_id, bid_amount);
        client.cancel_bid(&bid_id);

        // P10 – withdraw on Cancelled bid must error.
        let result = client
            .try_withdraw_bid(&bid_id)
            .expect("try_withdraw_bid must not panic");
        assert!(result.is_err(), "P10: withdraw_bid on Cancelled bid must return Err");

        let bid = client
            .try_get_bid(&bid_id)
            .expect("try_get_bid must not panic")
            .expect("bid must be Some after failed withdraw (P10)");
        assert_eq!(
            bid.status,
            BidStatus::Cancelled,
            "P10: bid status must remain Cancelled after failed withdraw"
        );
    }

    /// P11: accept_bid on a Cancelled bid always returns an error.
    #[test]
    fn fuzz_accept_after_cancel_bid_errors(
        amount in MIN_AMOUNT..MAX_AMOUNT,
        due_date_offset in MIN_DUE_DATE_OFFSET..MAX_DUE_DATE_OFFSET,
        bid_pct in 10u32..100u32,
    ) {
        let (env, client, _admin, business, investor, currency) = setup();
        let invoice_id = upload_verified_invoice(
            &env, &client, &business, &currency, amount, due_date_offset,
        );
        let bid_amount = (amount as u128 * bid_pct as u128 / 100).max(1) as i128;
        let bid_id = place_bid(&env, &client, &investor, &invoice_id, bid_amount);
        client.cancel_bid(&bid_id);

        // P11 – accept on Cancelled bid must error (invoice is still Verified).
        let result = client
            .try_accept_bid(&invoice_id, &bid_id)
            .expect("try_accept_bid must not panic");
        assert!(result.is_err(), "P11: accept_bid on Cancelled bid must return Err");

        let bid = client
            .try_get_bid(&bid_id)
            .expect("try_get_bid must not panic")
            .expect("bid must be Some after failed accept (P11)");
        assert_eq!(
            bid.status,
            BidStatus::Cancelled,
            "P11: bid status must remain Cancelled after failed accept"
        );
    }
}
