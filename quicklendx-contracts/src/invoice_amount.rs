//! Invoice amount precision and overflow validation (Issue #2432).
//!
//! # Scope
//!
//! This module is the single, documented enforcement point for the **exact
//! integer rules** that every invoice amount must satisfy before any state
//! change in the invoice lifecycle (creation, amendment, cancellation,
//! completion). Amounts are denominated in the smallest unit of the invoice
//! currency (integers only — there is no fractional representation on-chain),
//! so "precision" here means:
//!
//! 1. **Sign** — zero and negative amounts are invalid (`InvalidAmount`).
//! 2. **Ceiling (overflow safety)** — amounts above `MAX_INVOICE_AMOUNT` are
//!    invalid (`InvalidAmount`). The ceiling is deliberately
//!    `i128::MAX / 10_000` so that every downstream bps computation
//!    (`amount * bps / 10_000`, `amount * bps / BPS_DENOMINATOR`) is
//!    overflow-free for any `bps <= 10_000`. See [`MAX_INVOICE_AMOUNT`].
//! 3. **Minimum** — amounts below the configured `min_invoice_amount` are
//!    invalid (`InvalidAmount`); the boundary is inclusive (`amount >= min`).
//! 4. **Scale** — the invoice currency must not report more than
//!    [`MAX_CURRENCY_DECIMALS`] (18) decimals, otherwise the helper rejects
//!    with `InvalidCurrency` so that amounts can never be silently
//!    re-scaled/truncated against the token's internal precision.
//!
//! Every helper in this module is a **pure, side-effect-free function**: it
//! performs no storage access and no arithmetic beyond the documented checks.
//! Callers (contract entrypoints) invoke these helpers **before** writing any
//! state, so a rejected call can never leave a partial or unauthorized
//! invoice record behind.
//!
//! # Relationship to the legacy tree
//!
//! The pre-existing checks in the invoice lifecycle entrypoints
//! (`contract.rs::store_invoice` and `invoice.rs::Invoice::new`) applied the
//! sign/ceiling rule inline:
//!
//! ```text
//! if amount <= 0 || amount > MAX_INVOICE_AMOUNT { return Err(InvalidAmount); }
//! ```
//!
//! [`validate_invoice_amount_ceiling`] reproduces that exact predicate (same
//! error, same boundaries — inclusive at `MAX_INVOICE_AMOUNT`) so public
//! behavior is preserved byte-for-byte; the entrypoints now route through this
//! module instead of duplicating the predicate. The min-bound and scale rules
//! mirror the semantics of `protocol_limits::validate_invoice` (min
//! `min_invoice_amount`, inclusive) and `payments::require_matching_currency_precision`
//! (`decimals <= 18`) respectively, so the rules are also consistent with the
//! funding/payment paths that consume invoice amounts later in the lifecycle.
//!
//! # Compatibility, migration, and rollback
//!
//! * **Public behavior**: unchanged. `store_invoice` / `Invoice::new` return
//!   the same `QuickLendXError::InvalidAmount` for the same inputs; no error
//!   codes, response shapes, or ABI types are added or removed.
//! * **Migration**: none required. No stored state is touched; existing
//!   invoices are unaffected.
//! * **Rollback**: reverting this change restores the inline predicates; the
//!   validation semantics are identical in both forms.
//!
//! # Operational limitations and security assumptions
//!
//! * Amounts are `i128` smallest-units. "Fractional" values (e.g. `1.5` tokens
//!   at 6 decimals) are represented exactly as integers (`1_500_000`); the
//!   integer division in bps math floors toward zero, matching the existing
//!   `floor(amount * bps / 10_000)` fee formula (see `fees.rs` /
//!   `profits.rs`). [`checked_fee_amount`] pins that formula with checked
//!   arithmetic.
//! * The scale rule trusts the currency contract's `decimals()` report,
//!   exactly as the existing funding-path guard does; it is a ceiling on
//!   precision, not a registry check. Token allowlisting, if desired, remains
//!   a separate compliance layer.
//! * `MAX_INVOICE_AMOUNT` assumes `BPS_DENOMINATOR == 10_000` (basis points).
//!   If the fee denominator ever changes, this ceiling must be re-derived.
//!
//! # Invariants (enforced by this module)
//!
//! * `0 < amount <= MAX_INVOICE_AMOUNT` for every accepted invoice amount.
//! * `amount >= min_invoice_amount` when a positive floor is configured.
//! * `decimals(currency) <= 18` for every accepted currency.
//! * For any accepted `amount` and any `bps <= 10_000`:
//!   `amount * bps` never overflows `i128`; the fee is
//!   `floor(amount * bps / 10_000)`.
//! * Validation is deterministic and pure: the same rejected input yields the
//!   same error on every call, and no call mutates state.

use crate::errors::QuickLendXError;

/// Hard upper bound for invoice amounts (smallest units).
///
/// `i128::MAX / 10_000` guarantees that `amount * bps` cannot overflow `i128`
/// for any `bps <= BPS_DENOMINATOR` (10_000), which covers every fee,
/// discount, penalty, and split computation in the protocol. This mirrors the
/// legacy `protocol_limits::MAX_INVOICE_AMOUNT` constant and is locked to it
/// by [`test_max_invoice_amount_matches_documented_formula`] in the test
/// module.
pub const MAX_INVOICE_AMOUNT: i128 = i128::MAX / 10_000;

/// Basis-point denominator used by every fee/split formula.
///
/// 100 % == 10_000 bps. Mirrors `profits::BPS_DENOMINATOR`.
pub const BPS_DENOMINATOR: i128 = 10_000;

/// Maximum number of decimals the invoice currency may report.
///
/// Mirrors the ceiling enforced by `payments::require_matching_currency_precision`
/// on the funding path. Tokens reporting more than 18 decimals are rejected at
/// every amount boundary so that internal math can never silently truncate or
/// re-scale a stored amount.
pub const MAX_CURRENCY_DECIMALS: u32 = 18;

/// Validate an invoice amount against the sign and overflow-ceiling rules.
///
/// This is the exact predicate historically applied inline by
/// `contract.rs::store_invoice` and `invoice.rs::Invoice::new`:
///
/// ```text
/// amount <= 0 || amount > MAX_INVOICE_AMOUNT  →  Err(InvalidAmount)
/// ```
///
/// The boundary is **inclusive at the top**: `amount == MAX_INVOICE_AMOUNT`
/// is accepted (and is exactly the largest value for which every downstream
/// bps computation remains overflow-free).
///
/// # Errors
/// * [`QuickLendXError::InvalidAmount`] — `amount <= 0` or
///   `amount > MAX_INVOICE_AMOUNT`.
pub fn validate_invoice_amount_ceiling(amount: i128) -> Result<(), QuickLendXError> {
    if amount <= 0 || amount > MAX_INVOICE_AMOUNT {
        return Err(QuickLendXError::InvalidAmount);
    }
    Ok(())
}

/// Validate an invoice amount against the sign, overflow-ceiling, and
/// configured minimum rules.
///
/// Equivalent to [`validate_invoice_amount_ceiling`] plus the minimum floor:
/// `amount < min_amount` (with `min_amount > 0`) is rejected with
/// [`QuickLendXError::InvalidAmount`]. The minimum boundary is **inclusive**:
/// `amount == min_amount` is accepted, matching
/// `protocol_limits::validate_invoice`.
///
/// A `min_amount <= 0` disables the floor (protocol configuration rejects
/// non-positive minimums at `set_protocol_limits` time, so this only guards
/// against direct misuse of the helper).
///
/// # Errors
/// * [`QuickLendXError::InvalidAmount`] — `amount <= 0`,
///   `amount > MAX_INVOICE_AMOUNT`, or (`min_amount > 0` and
///   `amount < min_amount`).
pub fn validate_invoice_amount(amount: i128, min_amount: i128) -> Result<(), QuickLendXError> {
    validate_invoice_amount_ceiling(amount)?;
    if min_amount > 0 && amount < min_amount {
        return Err(QuickLendXError::InvalidAmount);
    }
    Ok(())
}

/// Validate the scale of an invoice currency's `decimals()` report.
///
/// Accepts `decimals` in `[0, MAX_CURRENCY_DECIMALS]` (both inclusive) and
/// rejects anything larger so the protocol never accepts a token whose
/// internal scaling could overflow or truncate internal math.
///
/// # Errors
/// * [`QuickLendXError::InvalidCurrency`] — `decimals > MAX_CURRENCY_DECIMALS`.
pub fn check_currency_scale(decimals: u32) -> Result<(), QuickLendXError> {
    if decimals > MAX_CURRENCY_DECIMALS {
        return Err(QuickLendXError::InvalidCurrency);
    }
    Ok(())
}

/// Compute `floor(amount * fee_bps / 10_000)` with strict, checked arithmetic.
///
/// This pins the exact bps formula used by the fee/settlement pipeline
/// (`fees.rs`, `profits.rs`, `payments.rs::allocate_repayment`) so that the
/// "no overflow downstream of an accepted invoice amount" invariant is
/// directly testable. For any amount accepted by
/// [`validate_invoice_amount_ceiling`] and any `fee_bps <= 10_000` the
/// multiplication cannot overflow — the ceiling guarantees it.
///
/// # Errors
/// * [`QuickLendXError::InvalidAmount`] — `amount <= 0`.
/// * [`QuickLendXError::InvalidFeeBasisPoints`] — `fee_bps > 10_000`.
/// * [`QuickLendXError::ArithmeticOverflow`] — the intermediate
///   `amount * fee_bps` would overflow `i128` (only reachable when `amount`
///   exceeds the ceiling, which the entrypoints reject before this runs).
pub fn checked_fee_amount(amount: i128, fee_bps: u32) -> Result<i128, QuickLendXError> {
    if amount <= 0 {
        return Err(QuickLendXError::InvalidAmount);
    }
    if fee_bps > BPS_DENOMINATOR as u32 {
        return Err(QuickLendXError::InvalidFeeBasisPoints);
    }
    amount
        .checked_mul(fee_bps as i128)
        .and_then(|product| product.checked_div(BPS_DENOMINATOR))
        .ok_or(QuickLendXError::ArithmeticOverflow)
}
