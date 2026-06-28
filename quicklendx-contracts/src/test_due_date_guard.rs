//! Regression tests for the due_date lower-bound guard.
//!
//! Both `Invoice::new` and `ProtocolLimitsContract::validate_invoice` must
//! reject a `due_date` that is already in the past (≤ current ledger timestamp).
//!
//! Threat model: without this check at the model and validation layers a caller
//! could supply a `due_date` that is already elapsed, creating an invoice that
//! is overdue on arrival.  Even though the top-level entry points (`store_invoice`,
//! `verify_invoice_data`) already reject past dates, the absence of the guard
//! deeper in the call stack leaves the invariant unenforceable for any code path
//! that bypasses those entry points.
//!
//! **Negative test** (`invoice_new_rejects_due_date_in_past`): fails on the
//! unpatched codebase because `Invoice::new` returned `Ok` for past due dates,
//! causing the `unwrap_err()` call below to panic.  After the fix it returns
//! `Err(InvoiceDueDateInvalid)` and the test passes.

use super::*;
use crate::errors::QuickLendXError;
use crate::invoice::{Invoice, InvoiceCategory};
use crate::protocol_limits::ProtocolLimitsContract;
use soroban_sdk::{testutils::Address as _, testutils::Ledger as _, Address, Env, String, Vec};

fn setup() -> (Env, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    (env, contract_id)
}

// ---------------------------------------------------------------------------
// Invoice::new — lower-bound guard
// ---------------------------------------------------------------------------

/// NEGATIVE TEST — fails on current main before fix, passes after.
///
/// `Invoice::new` previously had no `due_date <= current_timestamp` check.
/// A caller could construct an invoice whose due date had already elapsed,
/// bypassing the guards in the top-level entry points.
#[test]
fn invoice_new_rejects_due_date_in_past() {
    let (env, contract_id) = setup();
    env.ledger().set_timestamp(1_000_000);

    let business = Address::generate(&env);
    let currency = Address::generate(&env);

    let result = env.as_contract(&contract_id, || {
        Invoice::new(
            &env,
            business,
            100i128,
            currency,
            999_999u64,
            String::from_str(&env, "past-due invoice"),
            InvoiceCategory::Services,
            Vec::new(&env),
        )
    });

    assert_eq!(result.unwrap_err(), QuickLendXError::InvoiceDueDateInvalid);
}

#[test]
fn invoice_new_rejects_due_date_equal_to_current_timestamp() {
    let (env, contract_id) = setup();
    env.ledger().set_timestamp(1_000_000);

    let business = Address::generate(&env);
    let currency = Address::generate(&env);

    let result = env.as_contract(&contract_id, || {
        Invoice::new(
            &env,
            business,
            100i128,
            currency,
            1_000_000u64,
            String::from_str(&env, "due-now invoice"),
            InvoiceCategory::Services,
            Vec::new(&env),
        )
    });

    assert_eq!(result.unwrap_err(), QuickLendXError::InvoiceDueDateInvalid);
}

#[test]
fn invoice_new_accepts_due_date_strictly_in_future() {
    let (env, contract_id) = setup();
    env.ledger().set_timestamp(1_000_000);

    let business = Address::generate(&env);
    let currency = Address::generate(&env);

    let result = env.as_contract(&contract_id, || {
        Invoice::new(
            &env,
            business,
            100i128,
            currency,
            1_000_001u64,
            String::from_str(&env, "future invoice"),
            InvoiceCategory::Services,
            Vec::new(&env),
        )
    });

    assert!(result.is_ok(), "Invoice::new must accept a due_date strictly in the future");
}

// ---------------------------------------------------------------------------
// ProtocolLimitsContract::validate_invoice — lower-bound guard
// ---------------------------------------------------------------------------

#[test]
fn validate_invoice_rejects_past_due_date() {
    let (env, contract_id) = setup();
    env.ledger().set_timestamp(1_000_000);

    let result = env.as_contract(&contract_id, || {
        ProtocolLimitsContract::validate_invoice(env.clone(), 100i128, 999_999u64)
    });

    assert_eq!(result, Err(QuickLendXError::InvoiceDueDateInvalid));
}

#[test]
fn validate_invoice_rejects_due_date_equal_to_current_timestamp() {
    let (env, contract_id) = setup();
    env.ledger().set_timestamp(1_000_000);

    let result = env.as_contract(&contract_id, || {
        ProtocolLimitsContract::validate_invoice(env.clone(), 100i128, 1_000_000u64)
    });

    assert_eq!(result, Err(QuickLendXError::InvoiceDueDateInvalid));
}

#[test]
fn validate_invoice_accepts_future_due_date() {
    let (env, contract_id) = setup();
    env.ledger().set_timestamp(1_000_000);

    let result = env.as_contract(&contract_id, || {
        // amount=100 > DEFAULT_MIN_AMOUNT (10 in test builds)
        // due_date=1_000_001 is within max_due_date_days=365 window
        ProtocolLimitsContract::validate_invoice(env.clone(), 100i128, 1_000_001u64)
    });

    assert_eq!(result, Ok(()));
}
