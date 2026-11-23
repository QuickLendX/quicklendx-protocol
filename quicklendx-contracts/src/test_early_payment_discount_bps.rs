//! Boundary tests for the `early_payment_discount_bps` per-invoice config
//! (Issues #1820 / #1821).
//!
//! Threat model / rationale:
//!
//! - **Invoice creation must reject `>5000 bps`**: anything above 50% cannot
//!   represent a real-world discount and only shows up from a misconfigured
//!   business or a hostile caller probing for overflow / rounding abuse. The
//!   constructor must surface a typed error rather than silently truncating
//!   or wrapping.
//! - **`None` is the no-regression anchor**: pre-issue invoices store no
//!   discount whatsoever. The constructor continues to accept `None` and the
//!   stored field accepts `None` on read without breaking existing fixtures.
//! - **Boundary cases (`0`, `5000`) must both succeed**: `0` represents
//!   "advertise but waive"; `5000` represents the legal ceiling. Anything in
//!   between (e.g. identity cases like `1`, `100`, `1000`) must also succeed
//!   so the implementation isn't accidentally `>0` only.
//!
//! Tests live at the unit level so they exercise only the typed-error path,
//! not the full client wiring (which the existing batch / store tests cover).

#![cfg(test)]

use crate::errors::QuickLendXError;
use crate::invoice::Invoice;
use crate::types::{InvoiceCategory, InvoiceStatus};
use soroban_sdk::{testutils::Address as _, Address, Env, String, Vec};

fn make_invoice(
    env: &Env,
    business: Address,
    due_offset_secs: u64,
    early_payment_discount_bps: Option<u32>,
) -> Result<Invoice, QuickLendXError> {
    Invoice::new(
        env,
        business,
        1_000_000,
        Address::generate(env),
        env.ledger().timestamp() + due_offset_secs,
        String::from_str(env, "Boundary test invoice"),
        InvoiceCategory::Services,
        Vec::new(env),
        None,
        None,
        early_payment_discount_bps,
    )
}

fn verified_business(env: &Env) -> Address {
    Address::generate(env)
}

// ─── Same-day boundary (T-0): `due_date` is the current ledger timestamp + 0 ─

/// `due_date == now`: the qualifier "settles on or before the due_date"
/// should still include same-day settlement. We don't lose data: a discount
/// negotiation that targets "due today" must continue to be representable.
#[test]
fn early_payment_discount_bps_accepted_at_same_day_boundary() {
    let env = Env::default();
    let business = verified_business(&env);
    // due_date strictly > now per contract rules, so bump by 1 second.
    let invoice = make_invoice(&env, business, 1, Some(100)).unwrap();
    assert_eq!(invoice.early_payment_discount_bps, Some(100));
    assert_eq!(invoice.status, InvoiceStatus::Pending);
}

// ─── T-1 boundary: 1-day forward due date ───────────────────────────────────

/// Due date 24h in the future — the most common invoice-to-investor timing.
#[test]
fn early_payment_discount_bps_accepted_at_t_minus_1_boundary() {
    let env = Env::default();
    let business = verified_business(&env);
    let invoice = make_invoice(&env, business, 86_400, Some(2_500)).unwrap();
    assert_eq!(invoice.early_payment_discount_bps, Some(2_500));
}

// ─── T-30 boundary: 30-day forward due date ─────────────────────────────────

/// 30 days out — the long-tail of invoice maturities used in B2B trade.
/// Confirms the upper end of the practical time horizon is not gated.
#[test]
fn early_payment_discount_bps_accepted_at_t_minus_30_boundary() {
    let env = Env::default();
    let business = verified_business(&env);
    let invoice = make_invoice(&env, business, 86_400 * 30, Some(500)).unwrap();
    assert_eq!(invoice.early_payment_discount_bps, Some(500));
}

// ─── Bps-value boundaries ───────────────────────────────────────────────────

/// `0 bps`: "advertise but waive" — the discount is *named* on the invoice
/// for transparency but does not move money. Must succeed so callers can
/// publish the policy without paying for it.
#[test]
fn early_payment_discount_bps_zero_is_accepted() {
    let env = Env::default();
    let business = verified_business(&env);
    let invoice = make_invoice(&env, business, 86_400, Some(0)).unwrap();
    assert_eq!(invoice.early_payment_discount_bps, Some(0));
}

/// `5000 bps` (50%): the legal ceiling. Anything above would not represent
/// a real-world discount; the constructor must accept exactly the boundary.
#[test]
fn early_payment_discount_bps_max_is_accepted() {
    let env = Env::default();
    let business = verified_business(&env);
    let invoice = make_invoice(&env, business, 86_400, Some(5_000)).unwrap();
    assert_eq!(invoice.early_payment_discount_bps, Some(5_000));
}

/// `5001 bps`: one basis point above the ceiling. Must fail with a typed
/// error rather than wrap, saturate, or truncate.
#[test]
fn early_payment_discount_bps_above_max_rejected() {
    let env = Env::default();
    let business = verified_business(&env);
    let err = make_invoice(&env, business, 86_400, Some(5_001)).unwrap_err();
    assert_eq!(err, QuickLendXError::InvalidFeeBasisPoints);
}

/// `u32::MAX` (~4.29B bps): an obviously bogus value. Must fail the same
/// way the one-bps-over-max case does — failing loudly and consistently is
/// the contract.
#[test]
fn early_payment_discount_bps_u32_max_rejected() {
    let env = Env::default();
    let business = verified_business(&env);
    let err = make_invoice(&env, business, 86_400, Some(u32::MAX)).unwrap_err();
    assert_eq!(err, QuickLendXError::InvalidFeeBasisPoints);
}

// ─── None anchor: backwards-compat — must continue to round-trip ─────────────

/// `None` must succeed and round-trip exactly. This is the regression
/// anchor: pre-#1820 invoices stored no discount field at all, and the
/// new field's `None` default preserves that exact behaviour.
#[test]
fn early_payment_discount_bps_none_round_trips() {
    let env = Env::default();
    let business = verified_business(&env);
    let invoice = make_invoice(&env, business, 86_400, None).unwrap();
    assert_eq!(invoice.early_payment_discount_bps, None);
}
