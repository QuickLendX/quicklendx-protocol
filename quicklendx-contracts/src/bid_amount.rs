//! Bid amount precision and overflow validation (Issue QE-2026-08).
//!
//! # Scope
//!
//! This module is the single, documented enforcement point for the **exact
//! integer rules** that every bid amount pair (`bid_amount`, `expected_return`)
//! must satisfy before any state change on the bid-submission and
//! auction-selection paths. Amounts are denominated in the smallest unit of the
//! invoice currency (integers only — there is no fractional representation
//! on-chain), so "precision" here means:
//!
//! 1. **Sign** — zero and negative `bid_amount` / `expected_return` values are
//!    invalid (`InvalidAmount`). This reproduces the historical
//!    `bid.bid_amount <= 0` guard in `bid::verify_bid_match`.
//! 2. **Ceiling (overflow safety)** — amounts above [`MAX_BID_AMOUNT`]
//!    (`i128::MAX / 10_000`) are invalid (`InvalidAmount`). The ceiling is
//!    chosen so that every downstream bps computation on an accepted bid
//!    (`amount * bps / 10_000` in `fees.rs` / `profits.rs`) is overflow-free
//!    for any `bps <= 10_000`, and so that the auction-selection profit key
//!    `expected_return - bid_amount` can never overflow `i128`.
//! 3. **Invoice ceiling** — a bid may not finance more than the invoice's face
//!    value: `bid_amount > invoice_amount` is invalid (`InvalidAmount`). This
//!    makes explicit the rule that `bid::verify_bid_match` documents in its
//!    error table ("bid amount > invoice amount → `InvalidAmount`") but never
//!    actually enforced in code — see *Compatibility* below.
//! 4. **Return floor** — `expected_return < bid_amount` is invalid
//!    (`InvalidAmount`). An investor whose expected return does not cover the
//!    principal is offering a negative-profit bid; rejecting it at the boundary
//!    keeps the auction-selection profit key non-negative and its ranking
//!    deterministic even under adversarial input.
//!
//! Every helper in this module is a **pure, side-effect-free function**: it
//! performs no storage access and no arithmetic beyond the documented checks.
//! Callers (contract entrypoints and the bid-matching helpers) invoke these
//! helpers **before** writing any state, so a rejected, stale, repeated, or
//! failed call can never leave a partial or unauthorized bid record behind.
//!
//! # Relationship to the legacy tree
//!
//! * `bid::verify_bid_match` applied the sign rule inline as
//!   `if bid.bid_amount <= 0 { return Err(InvalidAmount); }`.
//!   [`validate_bid_amount_ceiling`] reproduces that exact predicate (same
//!   error) and adds the overflow ceiling.
//! * `bid::compare_bids` ranks bids by
//!   `profit = expected_return.saturating_sub(bid_amount)`. `saturating_sub`
//!   silently clamps adversarial inputs (a huge `expected_return`, a negative
//!   `bid_amount`) to `i128::MAX` / `0`, which can make ranking
//!   non-deterministic or unfair. [`checked_bid_profit`] pins the exact
//!   subtraction; for any pair accepted by [`validate_bid`] the result is a
//!   non-negative value that never overflows, so the saturating and checked
//!   forms agree and the ranking is deterministic.
//! * `bid::BidStorage::get_active_bid_amount_sum_for_investor` aggregates
//!   exposure with `saturating_add`. [`checked_bid_amount_sum`] pins the exact
//!   addition so an overflowing aggregate surfaces as `ArithmeticOverflow`
//!   instead of a clamped total.
//! * The `i128::MAX / 10_000` ceiling mirrors
//!   `invoice_amount::MAX_INVOICE_AMOUNT`, so a bid that satisfies the invoice
//!   ceiling automatically satisfies the bid ceiling.
//!
//! # Compatibility, migration, and rollback
//!
//! * **`bid_amount <= 0` → `InvalidAmount`**: unchanged.
//! * **Overflow ceiling**: new, but unreachable in practice for any bid on an
//!   invoice whose amount already passed `invoice_amount::validate_invoice_amount_ceiling`
//!   (the invoice ceiling is the same value), because rule 3 caps `bid_amount`
//!   at `invoice_amount`. It is a defence-in-depth guard for direct callers.
//! * **Invoice ceiling (`bid_amount <= invoice_amount`)**: this is a
//!   *documented* rule in `verify_bid_match` that the code never enforced. It
//!   is now enforced. A bid that offered to finance *more* than the invoice's
//!   face value — economically nonsensical, and previously accepted — is now
//!   rejected with `InvalidAmount`. This is the one explicit behavioural
//!   tightening; it aligns code with the long-standing documented contract.
//! * **Return floor (`expected_return >= bid_amount`)**: new explicit rule.
//!   Negative-profit bids were previously accepted and then ranked with a
//!   `saturating_sub`-clamped profit of `0`. They are now rejected at
//!   submission.
//! * **Error codes / response shapes**: no codes are added or removed; every
//!   rejection uses the existing `QuickLendXError::InvalidAmount` /
//!   `ArithmeticOverflow`.
//! * **Migration**: none. No stored state is touched; existing bids are
//!   unaffected (the new rules gate *new* submissions only).
//! * **Rollback**: reverting this module and its call sites restores the inline
//!   `bid_amount <= 0` predicate and the `saturating_*` arithmetic. No data
//!   migration is needed in either direction.
//!
//! # Operational limitations and security assumptions
//!
//! * `expected_return` is bounded above only by [`MAX_BID_AMOUNT`], not by the
//!   invoice amount: an optimistic return expectation is a matter of investor
//!   risk appetite, not a protocol-integrity concern, and the settlement path
//!   pays out against real repayments regardless of the recorded expectation.
//! * Amounts are `i128` smallest-units. "Fractional" token values (e.g. `1.5`
//!   tokens at 6 decimals) are represented exactly as integers (`1_500_000`)
//!   and are accepted like any other positive integer.
//! * The bps helper floors toward zero (`floor(amount * bps / 10_000)`),
//!   matching the existing fee pipeline in `fees.rs` / `profits.rs`.
//! * `MAX_BID_AMOUNT` assumes `BPS_DENOMINATOR == 10_000`. If the fee
//!   denominator ever changes, the ceiling must be re-derived.
//!
//! # Invariants (enforced by this module)
//!
//! * `0 < bid_amount <= MAX_BID_AMOUNT` for every accepted bid.
//! * `bid_amount <= invoice_amount` when a positive invoice amount is supplied.
//! * `0 < expected_return <= MAX_BID_AMOUNT` and `expected_return >= bid_amount`
//!   for every accepted bid.
//! * For any accepted `(bid_amount, expected_return)` pair:
//!   `expected_return - bid_amount` is in `[0, MAX_BID_AMOUNT)` and never
//!   overflows `i128` — so the auction-selection ranking key is exact.
//! * For any accepted `bid_amount` and any `bps <= 10_000`:
//!   `bid_amount * bps` never overflows `i128`; the fee is
//!   `floor(bid_amount * bps / 10_000)`.
//! * Validation is deterministic and pure: the same rejected input yields the
//!   same error on every call, and no call mutates state.

use crate::errors::QuickLendXError;

/// Hard upper bound for bid amounts (smallest units).
///
/// `i128::MAX / 10_000` guarantees that `amount * bps` cannot overflow `i128`
/// for any `bps <= BPS_DENOMINATOR` (10_000), and that the auction-selection
/// profit key `expected_return - bid_amount` cannot overflow for any pair of
/// accepted amounts. Deliberately identical to
/// `invoice_amount::MAX_INVOICE_AMOUNT`; the equality is locked by
/// [`test_max_bid_amount_matches_invoice_ceiling`] in the test module.
pub const MAX_BID_AMOUNT: i128 = i128::MAX / 10_000;

/// Basis-point denominator used by every fee/split formula.
///
/// 100 % == 10_000 bps. Mirrors `profits::BPS_DENOMINATOR` and
/// `invoice_amount::BPS_DENOMINATOR`.
pub const BPS_DENOMINATOR: i128 = 10_000;

/// Validate a single bid amount against the sign and overflow-ceiling rules.
///
/// This is the historical inline predicate from `bid::verify_bid_match`
/// (`bid.bid_amount <= 0 → Err(InvalidAmount)`) extended with the overflow
/// ceiling:
///
/// ```text
/// amount <= 0 || amount > MAX_BID_AMOUNT  →  Err(InvalidAmount)
/// ```
///
/// The boundary is **inclusive at the top**: `amount == MAX_BID_AMOUNT` is
/// accepted.
///
/// # Errors
/// * [`QuickLendXError::InvalidAmount`] — `amount <= 0` or
///   `amount > MAX_BID_AMOUNT`.
pub fn validate_bid_amount_ceiling(amount: i128) -> Result<(), QuickLendXError> {
    if amount <= 0 || amount > MAX_BID_AMOUNT {
        return Err(QuickLendXError::InvalidAmount);
    }
    Ok(())
}

/// Validate an `expected_return` against the sign, overflow-ceiling, and
/// return-floor rules, given the bid's principal `bid_amount`.
///
/// Equivalent to [`validate_bid_amount_ceiling`] applied to `expected_return`,
/// plus the floor `expected_return >= bid_amount` (a bid must at least return
/// its principal). The floor is **inclusive**: `expected_return == bid_amount`
/// (a zero-profit bid) is accepted.
///
/// `bid_amount` is assumed to have already passed
/// [`validate_bid_amount_ceiling`]; callers that use [`validate_bid`] get that
/// ordering for free.
///
/// # Errors
/// * [`QuickLendXError::InvalidAmount`] — `expected_return <= 0`,
///   `expected_return > MAX_BID_AMOUNT`, or `expected_return < bid_amount`.
pub fn validate_expected_return(
    expected_return: i128,
    bid_amount: i128,
) -> Result<(), QuickLendXError> {
    validate_bid_amount_ceiling(expected_return)?;
    if expected_return < bid_amount {
        return Err(QuickLendXError::InvalidAmount);
    }
    Ok(())
}

/// Full bid-submission boundary check.
///
/// Validates the complete `(bid_amount, expected_return)` pair for a bid on an
/// invoice of face value `invoice_amount`, in this order:
///
/// 1. `bid_amount` — sign + overflow ceiling ([`validate_bid_amount_ceiling`]).
/// 2. `bid_amount <= invoice_amount` when `invoice_amount > 0` (the invoice
///    ceiling). A non-positive `invoice_amount` disables this check so the
///    helper stays usable by callers that do not have an invoice in hand; the
///    invoice lifecycle rejects non-positive amounts long before a bid is
///    placed.
/// 3. `expected_return` — sign + overflow ceiling + return floor
///    ([`validate_expected_return`]).
///
/// On success the caller has the guarantee that
/// `0 < bid_amount <= min(invoice_amount, MAX_BID_AMOUNT)` and
/// `bid_amount <= expected_return <= MAX_BID_AMOUNT`, which is exactly the
/// precondition [`checked_bid_profit`] needs to be overflow-free and
/// non-negative.
///
/// # Errors
/// * [`QuickLendXError::InvalidAmount`] — any rule above is violated.
pub fn validate_bid(
    bid_amount: i128,
    expected_return: i128,
    invoice_amount: i128,
) -> Result<(), QuickLendXError> {
    validate_bid_amount_ceiling(bid_amount)?;
    if invoice_amount > 0 && bid_amount > invoice_amount {
        return Err(QuickLendXError::InvalidAmount);
    }
    validate_expected_return(expected_return, bid_amount)?;
    Ok(())
}

/// Compute the auction-selection profit key `expected_return - bid_amount` with
/// strict, checked arithmetic.
///
/// `bid::compare_bids` ranks bids by this value using `saturating_sub`, which
/// silently clamps on overflow. This helper pins the exact subtraction so that:
///
/// * for any pair accepted by [`validate_bid`] the result is in
///   `[0, MAX_BID_AMOUNT)` and equal to the `saturating_sub` result (proving
///   the ranking is deterministic for valid inputs), and
/// * an out-of-contract pair that would overflow `i128` surfaces as
///   `ArithmeticOverflow` instead of a clamped, attacker-chosen rank.
///
/// # Errors
/// * [`QuickLendXError::ArithmeticOverflow`] — `expected_return - bid_amount`
///   does not fit in `i128` (only reachable for inputs outside the
///   [`validate_bid`] contract).
pub fn checked_bid_profit(
    expected_return: i128,
    bid_amount: i128,
) -> Result<i128, QuickLendXError> {
    expected_return
        .checked_sub(bid_amount)
        .ok_or(QuickLendXError::ArithmeticOverflow)
}

/// Add one bid amount to a running per-investor exposure total with strict,
/// checked arithmetic.
///
/// `bid::BidStorage::get_active_bid_amount_sum_for_investor` accumulates with
/// `saturating_add`; this helper pins the exact addition so an overflowing
/// aggregate is reported (`ArithmeticOverflow`) rather than silently clamped to
/// `i128::MAX` — a clamped total would understate exposure relative to reality
/// and could let an investor exceed a configured cap.
///
/// # Errors
/// * [`QuickLendXError::ArithmeticOverflow`] — `running_total + bid_amount`
///   does not fit in `i128`.
pub fn checked_bid_amount_sum(
    running_total: i128,
    bid_amount: i128,
) -> Result<i128, QuickLendXError> {
    running_total
        .checked_add(bid_amount)
        .ok_or(QuickLendXError::ArithmeticOverflow)
}

/// Compute `floor(amount * fee_bps / 10_000)` with strict, checked arithmetic.
///
/// Pins the exact bps formula the fee/settlement pipeline applies to an
/// accepted bid amount (`fees.rs`, `profits.rs`) so the "no overflow downstream
/// of an accepted bid amount" invariant is directly testable. For any `amount`
/// accepted by [`validate_bid_amount_ceiling`] and any `fee_bps <= 10_000` the
/// multiplication cannot overflow — the ceiling guarantees it.
///
/// # Errors
/// * [`QuickLendXError::InvalidAmount`] — `amount <= 0`.
/// * [`QuickLendXError::InvalidFeeBasisPoints`] — `fee_bps > 10_000`.
/// * [`QuickLendXError::ArithmeticOverflow`] — the intermediate
///   `amount * fee_bps` would overflow `i128` (only reachable when `amount`
///   exceeds the ceiling, which the entrypoints reject first).
pub fn checked_bid_fee_amount(amount: i128, fee_bps: u32) -> Result<i128, QuickLendXError> {
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
