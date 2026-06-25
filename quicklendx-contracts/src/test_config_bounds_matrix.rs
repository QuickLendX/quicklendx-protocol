//! Bounds-enforcement matrix for admin protocol and fee configuration.
//!
//! These tests pin the exact accepted boundary values and first rejected values
//! for `set_fee_config` and `set_protocol_config`, and assert failed admin or
//! validation checks leave the readable on-chain config unchanged.

use super::*;
use crate::admin::AdminStorage;
use crate::errors::QuickLendXError;
use soroban_sdk::{testutils::Address as _, Address, Env};

const VALID_MIN_INVOICE_AMOUNT: i128 = 1_000_000;
const VALID_MAX_DUE_DATE_DAYS: u64 = 365;
const VALID_GRACE_PERIOD_SECONDS: u64 = 604_800;
const MAX_FEE_BPS: u32 = 1_000;
const MAX_DUE_DATE_DAYS: u64 = 730;
const MAX_GRACE_PERIOD_SECONDS: u64 = 2_592_000;

fn setup_initialized() -> (Env, QuickLendXContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);

    env.as_contract(&contract_id, || {
        AdminStorage::initialize(&env, &admin).unwrap();
    });
    client.set_fee_config(&admin, &200);
    client.set_protocol_config(
        &admin,
        &VALID_MIN_INVOICE_AMOUNT,
        &VALID_MAX_DUE_DATE_DAYS,
        &VALID_GRACE_PERIOD_SECONDS,
    );

    (env, client, admin)
}

fn assert_contract_err<T: core::fmt::Debug, C: core::fmt::Debug, E: core::fmt::Debug>(
    result: Result<Result<T, C>, Result<QuickLendXError, E>>,
    expected: QuickLendXError,
) {
    match result {
        Err(Ok(err)) => assert_eq!(err, expected),
        other => panic!("expected contract error {:?}, got {:?}", expected, other),
    }
}

#[test]
fn test_set_fee_config_bounds_matrix() {
    let (_env, client, admin) = setup_initialized();

    for accepted_fee_bps in [0, MAX_FEE_BPS] {
        assert!(
            client.try_set_fee_config(&admin, &accepted_fee_bps).is_ok(),
            "fee_bps={} must be accepted",
            accepted_fee_bps
        );
        assert_eq!(client.get_fee_bps(), accepted_fee_bps);
    }

    assert_contract_err(
        client.try_set_fee_config(&admin, &(MAX_FEE_BPS + 1)),
        QuickLendXError::InvalidFeeBasisPoints,
    );
    assert_eq!(client.get_fee_bps(), MAX_FEE_BPS);
}

#[test]
fn test_set_protocol_config_min_invoice_amount_bounds_matrix() {
    let (_env, client, admin) = setup_initialized();

    assert!(
        client
            .try_set_protocol_config(
                &admin,
                &1i128,
                &VALID_MAX_DUE_DATE_DAYS,
                &VALID_GRACE_PERIOD_SECONDS,
            )
            .is_ok(),
        "min_invoice_amount=1 must be accepted"
    );
    assert_eq!(client.get_min_invoice_amount(), 1);

    for rejected_min_invoice_amount in [0, -1] {
        assert_contract_err(
            client.try_set_protocol_config(
                &admin,
                &rejected_min_invoice_amount,
                &VALID_MAX_DUE_DATE_DAYS,
                &VALID_GRACE_PERIOD_SECONDS,
            ),
            QuickLendXError::InvalidAmount,
        );
        assert_eq!(client.get_min_invoice_amount(), 1);
    }
}

#[test]
fn test_set_protocol_config_due_date_bounds_matrix() {
    let (_env, client, admin) = setup_initialized();

    for accepted_max_due_date_days in [1, MAX_DUE_DATE_DAYS] {
        assert!(
            client
                .try_set_protocol_config(
                    &admin,
                    &VALID_MIN_INVOICE_AMOUNT,
                    &accepted_max_due_date_days,
                    &VALID_GRACE_PERIOD_SECONDS,
                )
                .is_ok(),
            "max_due_date_days={} must be accepted",
            accepted_max_due_date_days
        );
        assert_eq!(client.get_max_due_date_days(), accepted_max_due_date_days);
        assert_eq!(
            client.get_grace_period_seconds(),
            VALID_GRACE_PERIOD_SECONDS
        );
    }

    for rejected_max_due_date_days in [0, MAX_DUE_DATE_DAYS + 1] {
        assert_contract_err(
            client.try_set_protocol_config(
                &admin,
                &VALID_MIN_INVOICE_AMOUNT,
                &rejected_max_due_date_days,
                &VALID_GRACE_PERIOD_SECONDS,
            ),
            QuickLendXError::InvoiceDueDateInvalid,
        );
        assert_eq!(client.get_max_due_date_days(), MAX_DUE_DATE_DAYS);
    }
}

#[test]
fn test_set_protocol_config_grace_period_bounds_matrix() {
    let (_env, client, admin) = setup_initialized();

    for accepted_grace_period_seconds in [0, MAX_GRACE_PERIOD_SECONDS] {
        assert!(
            client
                .try_set_protocol_config(
                    &admin,
                    &VALID_MIN_INVOICE_AMOUNT,
                    &VALID_MAX_DUE_DATE_DAYS,
                    &accepted_grace_period_seconds,
                )
                .is_ok(),
            "grace_period_seconds={} must be accepted",
            accepted_grace_period_seconds
        );
        assert_eq!(client.get_max_due_date_days(), VALID_MAX_DUE_DATE_DAYS);
        assert_eq!(
            client.get_grace_period_seconds(),
            accepted_grace_period_seconds
        );
    }

    assert_contract_err(
        client.try_set_protocol_config(
            &admin,
            &VALID_MIN_INVOICE_AMOUNT,
            &VALID_MAX_DUE_DATE_DAYS,
            &(MAX_GRACE_PERIOD_SECONDS + 1),
        ),
        QuickLendXError::InvalidTimestamp,
    );
    assert_eq!(client.get_grace_period_seconds(), MAX_GRACE_PERIOD_SECONDS);
}

#[test]
fn test_config_bounds_reject_non_admin_without_mutation() {
    let (env, client, _admin) = setup_initialized();
    let non_admin = Address::generate(&env);
    let fee_before = client.get_fee_bps();
    let min_invoice_amount_before = client.get_min_invoice_amount();
    let max_due_date_days_before = client.get_max_due_date_days();
    let grace_period_seconds_before = client.get_grace_period_seconds();

    assert_contract_err(
        client.try_set_fee_config(&non_admin, &MAX_FEE_BPS),
        QuickLendXError::NotAdmin,
    );
    assert_eq!(client.get_fee_bps(), fee_before);

    assert_contract_err(
        client.try_set_protocol_config(
            &non_admin,
            &1i128,
            &MAX_DUE_DATE_DAYS,
            &MAX_GRACE_PERIOD_SECONDS,
        ),
        QuickLendXError::NotAdmin,
    );

    assert_eq!(client.get_min_invoice_amount(), min_invoice_amount_before);
    assert_eq!(client.get_max_due_date_days(), max_due_date_days_before);
    assert_eq!(
        client.get_grace_period_seconds(),
        grace_period_seconds_before
    );
}
