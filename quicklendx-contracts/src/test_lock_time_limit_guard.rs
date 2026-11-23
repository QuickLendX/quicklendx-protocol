//! Lock-time-limit guard — "Within, at limit, past".
//!
//! Pins the time-window validation in
//! `ProtocolLimitsContract::validate_invoice` across three zones:
//!
//! | Zone       | Lower bound                        | Upper bound                        |
//! |------------|------------------------------------|------------------------------------|
//! | **Within** | `due_date > current_time`          | `due_date < max_due_date`          |
//! | **At limit** | `due_date == current_time`       | `due_date == max_due_date`         |
//! | **Past**   | `due_date < current_time`          | `due_date > max_due_date`          |
//!
//! Every test is gated only by `#[cfg(test)]` so it runs on every CI matrix
//! entry, not just when the `legacy-tests` feature is enabled.

#![cfg(test)]

use crate::errors::QuickLendXError;
use crate::protocol_limits::ProtocolLimitsContract;
use crate::QuickLendXContract;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Env,
};

const SECONDS_PER_DAY: u64 = 86_400;

fn setup_env() -> (Env, Address) {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000_000);
    let contract_id = env.register(QuickLendXContract, ());
    (env, contract_id)
}

fn valid_amount() -> i128 {
    100i128 // > DEFAULT_MIN_AMOUNT (10 in test builds)
}

// ---------------------------------------------------------------------------
// Lower bound: due_date <= current_time → InvoiceDueDateInvalid
// ---------------------------------------------------------------------------

#[test]
fn validate_invoice_rejects_due_date_past_lower_bound() {
    let (env, contract_id) = setup_env();
    let result = env.as_contract(&contract_id, || {
        ProtocolLimitsContract::validate_invoice(env.clone(), valid_amount(), 999_999u64)
    });
    assert_eq!(result, Err(QuickLendXError::InvoiceDueDateInvalid));
}

#[test]
fn validate_invoice_rejects_due_date_at_lower_bound() {
    let (env, contract_id) = setup_env();
    let result = env.as_contract(&contract_id, || {
        ProtocolLimitsContract::validate_invoice(env.clone(), valid_amount(), 1_000_000u64)
    });
    assert_eq!(result, Err(QuickLendXError::InvoiceDueDateInvalid));
}

#[test]
fn validate_invoice_accepts_due_date_within_lower_bound() {
    let (env, contract_id) = setup_env();
    let result = env.as_contract(&contract_id, || {
        // due_date = current_time + 1, the first valid second
        ProtocolLimitsContract::validate_invoice(env.clone(), valid_amount(), 1_000_001u64)
    });
    assert_eq!(result, Ok(()));
}

// ---------------------------------------------------------------------------
// Upper bound: due_date > max_due_date → InvoiceDueDateInvalid
// ---------------------------------------------------------------------------

#[test]
fn validate_invoice_accepts_due_date_within_upper_bound() {
    let (env, contract_id) = setup_env();
    let one_day_before_max = env.ledger().timestamp() + 365 * SECONDS_PER_DAY - 1;
    let result = env.as_contract(&contract_id, || {
        ProtocolLimitsContract::validate_invoice(env.clone(), valid_amount(), one_day_before_max)
    });
    assert_eq!(result, Ok(()));
}

#[test]
fn validate_invoice_accepts_due_date_at_upper_bound() {
    let (env, contract_id) = setup_env();
    let max_due_date = env.ledger().timestamp() + 365 * SECONDS_PER_DAY;
    let result = env.as_contract(&contract_id, || {
        ProtocolLimitsContract::validate_invoice(env.clone(), valid_amount(), max_due_date)
    });
    assert_eq!(result, Ok(()));
}

#[test]
fn validate_invoice_rejects_due_date_past_upper_bound() {
    let (env, contract_id) = setup_env();
    let past_max = env.ledger().timestamp() + 365 * SECONDS_PER_DAY + 1;
    let result = env.as_contract(&contract_id, || {
        ProtocolLimitsContract::validate_invoice(env.clone(), valid_amount(), past_max)
    });
    assert_eq!(result, Err(QuickLendXError::InvoiceDueDateInvalid));
}
