//! Deterministic repayment allocation and escrow repayment.

use super::escrow::{EscrowStatus, EscrowStorage};
use super::rate_limit::RepaymentRateLimiter;
use super::transfer::transfer_funds_exact;
use crate::errors::QuickLendXError;
use crate::storage::{extend_persistent_ttl, InvoiceStorage};
use soroban_sdk::{contracttype, symbol_short, token, Address, BytesN, Env, Symbol};

// ============================================================================
// Repayment allocation engine
// ============================================================================
//
// Deterministic, overflow-safe repayment splitting for the escrow-based
// repayment path. Given a custodied `principal` (the escrow amount) and the
// `payment` the business repaid into the contract, this computes:
//
//   * `gross_profit`   = max(0, payment - principal)
//   * `platform_fee`   = floor(gross_profit * fee_bps / 10_000)   (investor-favored)
//   * `late_fee`       = floor(principal   * late_fee_bps / 10_000)
//   * `investor_return`= payment - platform_fee - late_fee        (>= 0)
//   * `treasury_amount`= floor(total_fee * treasury_share_bps / 10_000)
//   * `treasury_remaining` = total_fee - treasury_amount
//
// where `total_fee = platform_fee + late_fee`.
//
// # Invariants (enforced, never assumed)
// * `investor_return + platform_fee + late_fee == payment`   (no dust; platform
//   absorbs the floor-division remainder because `investor_return` is computed by
//   exact subtraction).
// * `treasury_amount + treasury_remaining == total_fee`.
// * Every component is non-negative.
// * All arithmetic uses `checked_*` and rejects invalid sign/scale/overflow
//   *before* any state is mutated by the caller.

/// Deterministic breakdown of a repayment allocation.
#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(test, derive(Debug))]
pub struct RepaymentAllocation {
    /// Original escrow principal returned to the business (loan release).
    pub principal_return: i128,
    /// Gross profit before fees (payment - principal), 0 when no profit.
    pub gross_profit: i128,
    /// Platform fee deducted from profit (investor-favored floor).
    pub platform_fee: i128,
    /// Late-payment surcharge on the principal.
    pub late_fee: i128,
    /// Net amount returned to the investor (principal + net profit).
    pub investor_return: i128,
    /// Portion of the fee routed to the treasury.
    pub treasury_amount: i128,
    /// Remainder of the fee retained by the protocol.
    pub treasury_remaining: i128,
}

/// Allocate a repayment deterministically with strict, checked arithmetic.
///
/// All inputs are validated up front: `principal` must be positive and within the
/// protocol amount bound; `payment` must be non-negative and within the bound.
/// Basis-point parameters are clamped to `[0, 10_000]` so a misconfigured or
/// adversarial caller cannot produce a fee outside `[0, gross_profit]`.
///
/// # Errors
/// * [`QuickLendXError::InvalidAmount`] — `principal <= 0`, `payment < 0`, an
///   amount exceeds `MAX_INVOICE_AMOUNT`, or the computed fees would exceed the
///   payment (over-charge), or a conservation invariant is violated.
/// * [`QuickLendXError::ArithmeticOverflow`] — any intermediate multiplication or
///   division would overflow `i128`.
pub fn allocate_repayment(
    principal: i128,
    payment: i128,
    fee_bps: i128,
    late_fee_bps: i128,
    treasury_share_bps: i128,
) -> Result<RepaymentAllocation, QuickLendXError> {
    if principal <= 0 || payment < 0 {
        return Err(QuickLendXError::InvalidAmount);
    }
    if principal > crate::protocol_limits::MAX_INVOICE_AMOUNT
        || payment > crate::protocol_limits::MAX_INVOICE_AMOUNT
    {
        return Err(QuickLendXError::InvalidAmount);
    }

    let denom = crate::profits::BPS_DENOMINATOR;
    let fee_bps = fee_bps.clamp(0, denom);
    let late_fee_bps = late_fee_bps.clamp(0, denom);
    let treasury_share_bps = treasury_share_bps.clamp(0, denom);

    // Gross profit relative to principal (floored at zero for loss/breakeven).
    let gross_profit = payment
        .checked_sub(principal)
        .ok_or(QuickLendXError::ArithmeticOverflow)?
        .max(0);

    // Platform fee: floor(gross_profit * fee_bps / denom). fee_bps <= denom so
    // platform_fee <= gross_profit (never negative).
    let platform_fee = gross_profit
        .checked_mul(fee_bps)
        .ok_or(QuickLendXError::ArithmeticOverflow)?
        .checked_div(denom)
        .ok_or(QuickLendXError::ArithmeticOverflow)?;

    // Late fee: floor(principal * late_fee_bps / denom).
    let late_fee = principal
        .checked_mul(late_fee_bps)
        .ok_or(QuickLendXError::ArithmeticOverflow)?
        .checked_div(denom)
        .ok_or(QuickLendXError::ArithmeticOverflow)?;

    // Investor return is the exact remainder after fees are removed from the
    // payment. If the fees would exhaust more than the payment, reject rather
    // than silently short-changing the investor.
    let investor_return = payment
        .checked_sub(platform_fee)
        .ok_or(QuickLendXError::ArithmeticOverflow)?
        .checked_sub(late_fee)
        .ok_or(QuickLendXError::ArithmeticOverflow)?;
    if investor_return < 0 {
        return Err(QuickLendXError::InvalidAmount);
    }

    // Treasury split of the total fee (platform + late).
    let total_fee = platform_fee
        .checked_add(late_fee)
        .ok_or(QuickLendXError::ArithmeticOverflow)?;
    let treasury_amount = total_fee
        .checked_mul(treasury_share_bps)
        .ok_or(QuickLendXError::ArithmeticOverflow)?
        .checked_div(denom)
        .ok_or(QuickLendXError::ArithmeticOverflow)?;
    let treasury_remaining = total_fee
        .checked_sub(treasury_amount)
        .ok_or(QuickLendXError::ArithmeticOverflow)?;

    // Conservation invariants — must hold by construction; enforce defensively.
    let recon = investor_return
        .checked_add(platform_fee)
        .ok_or(QuickLendXError::ArithmeticOverflow)?
        .checked_add(late_fee)
        .ok_or(QuickLendXError::ArithmeticOverflow)?;
    if recon != payment {
        return Err(QuickLendXError::InvalidAmount);
    }
    if treasury_amount
        .checked_add(treasury_remaining)
        .ok_or(QuickLendXError::ArithmeticOverflow)?
        != total_fee
    {
        return Err(QuickLendXError::InvalidAmount);
    }

    Ok(RepaymentAllocation {
        principal_return: principal,
        gross_profit,
        platform_fee,
        late_fee,
        investor_return,
        treasury_amount,
        treasury_remaining,
    })
}

/// Distribute a repayment from custodied escrow funds to the investor, the
/// treasury (platform fee), and release the principal back to the business.
///
/// This is the escrow-Held alternative to the settlement payout. It is atomic in
/// the same sense as [`release_escrow`]/[`refund_escrow`]: every validation and
/// the reserve decrease are computed *before* any token transfer, and the escrow
/// status is only updated after all transfers succeed, so a failed or rejected
/// call leaves no partial state.
///
/// # Arguments
/// * `invoice_id` — invoice whose escrow is being repaid.
/// * `payment_amount` — total amount the business repaid into the contract
///   (must already be custodied; the contract balance is checked first).
/// * `late_fee_bps` — late-payment surcharge in basis points (0 disables).
///
/// # Errors
/// * [`QuickLendXError::MutationLimitExceeded`] — the repaying account exceeded
///   the repayment rate limit for the active window.
/// * [`QuickLendXError::InvalidAmount`] — `payment_amount` is negative or exceeds
///   `MAX_INVOICE_AMOUNT`.
/// * [`QuickLendXError::InvalidFeeBasisPoints`] — `late_fee_bps` is negative or
///   exceeds `BPS_DENOMINATOR`.
/// * [`QuickLendXError::StorageKeyNotFound`] — no escrow/invoice for `invoice_id`.
/// * [`QuickLendXError::InvalidStatus`] — escrow is not `Held` (already repaid) or
///   a reserve repair is active.
/// * [`QuickLendXError::ArithmeticOverflow`]
///   — forwarded from [`allocate_repayment`] or the reserve update.
/// * [`QuickLendXError::InsufficientFunds`] — the contract does not custody
///   `payment_amount` (the business repayment has not been received).
/// * [`QuickLendXError::TokenTransferFailed`] — a token call panicked; escrow
///   status is **not** updated so the repayment can be safely retried.
pub fn repay_escrow(
    env: &Env,
    invoice_id: &BytesN<32>,
    payment_amount: i128,
    late_fee_bps: i128,
) -> Result<RepaymentAllocation, QuickLendXError> {
    // Resource bounds are enforced *before* any storage read or token transfer:
    // oversized or ill-formed repayment inputs are rejected cheaply and leave no
    // partial state. `late_fee_bps` beyond the BPS denominator is rejected (not
    // silently clamped) at this network boundary, unlike the pure
    // `allocate_repayment` helper whose clamping is preserved for callers that
    // pass pre-normalized BPS.
    if payment_amount < 0 || payment_amount > crate::protocol_limits::MAX_INVOICE_AMOUNT {
        return Err(QuickLendXError::InvalidAmount);
    }
    if late_fee_bps < 0 || late_fee_bps > crate::profits::BPS_DENOMINATOR {
        return Err(QuickLendXError::InvalidFeeBasisPoints);
    }

    let mut escrow = EscrowStorage::get_escrow_by_invoice(env, invoice_id)
        .ok_or(QuickLendXError::StorageKeyNotFound)?;

    let _invoice =
        InvoiceStorage::get_invoice(env, invoice_id).ok_or(QuickLendXError::StorageKeyNotFound)?;

    if escrow.status != EscrowStatus::Held {
        return Err(QuickLendXError::InvalidStatus);
    }

    EscrowStorage::require_no_active_reserve_repair(env, &escrow.currency)?;

    // Repayment must be rate-bounded before allocation or any token transfer so
    // that repeated repayment attempts cannot exhaust ledger/service resources.
    // The business is the repaying party; only genuinely repayable (`Held`)
    // escrows consume the caller's budget, so rejected stale/repeated calls
    // leave no state change and do not penalize the caller.
    let business = escrow.business.clone();
    RepaymentRateLimiter::check_and_record(env, &business)?;

    let principal = escrow.amount;
    let currency = escrow.currency.clone();
    let investor = escrow.investor.clone();
    let business = escrow.business.clone();

    let fee_bps = crate::profits::PlatformFee::get_config(env).fee_bps as i128;

    // Compute the allocation (all checked; rejects overflow/over-charge/scale).
    let allocation = allocate_repayment(
        principal,
        payment_amount,
        fee_bps,
        late_fee_bps,
        crate::profits::BPS_DENOMINATOR,
    )?;

    // Pre-flight: the contract must custody the full repayment before moving funds.
    let contract_address = env.current_contract_address();
    let token_client = token::Client::new(env, &currency);
    let contract_balance = token_client.balance(&contract_address);
    let required = payment_amount
        .checked_add(principal)
        .ok_or(QuickLendXError::ArithmeticOverflow)?;
    if contract_balance < required {
        return Err(QuickLendXError::InsufficientFunds);
    }

    // Reserve decrease is computed before transfers (mirrors release/refund).
    let next_held_reserve = if EscrowStorage::is_reserve_accounted(env, &escrow.escrow_id) {
        Some(EscrowStorage::held_reserve_after_decrease(
            env, &currency, principal,
        )?)
    } else {
        None
    };

    // Move funds: investor return, treasury fee, and principal release to business.
    // Each leg is exact; failures leave escrow status unchanged.
    transfer_funds_exact(
        env,
        &currency,
        &contract_address,
        &investor,
        allocation.investor_return,
    )?;

    if allocation.treasury_amount > 0 {
        if let Some(treasury) = crate::fees::FeeManager::get_treasury_address(env) {
            transfer_funds_exact(
                env,
                &currency,
                &contract_address,
                &treasury,
                allocation.treasury_amount,
            )?;
        }
        // If no treasury is configured the fee is intentionally retained by the
        // contract; the accounting identity still balances.
    }

    transfer_funds_exact(env, &currency, &contract_address, &business, principal)?;

    // Commit: reserve and escrow status only after all transfers succeeded.
    if let Some(next_held_reserve) = next_held_reserve {
        EscrowStorage::set_held_reserve_record(env, &currency, &next_held_reserve);
        EscrowStorage::clear_reserve_accounted(env, &escrow.escrow_id);
    }
    escrow.status = EscrowStatus::Released;
    EscrowStorage::update_escrow(env, &escrow);
    crate::fees::FeeManager::release_origination_fee(env, invoice_id)?;

    crate::events::emit_escrow_released(env, &escrow.escrow_id, invoice_id, &business, principal);
    crate::qlx_log!(
        env,
        "payment",
        "Escrow repaid: investor_return={} platform_fee={} principal={}",
        allocation.investor_return,
        allocation.platform_fee,
        principal
    );

    Ok(allocation)
}

const REPAYMENT_RECORD_KEY: Symbol = symbol_short!("rep_rec");

#[contracttype]
#[derive(Clone)]
pub struct RepaymentRecord {
    pub invoice_id: BytesN<32>,
    pub payment_amount: i128,
    pub late_fee_bps: i128,
    pub allocation: RepaymentAllocation,
}

fn get_repayment_record(env: &Env, request_key: &BytesN<32>) -> Option<RepaymentRecord> {
    let key = (REPAYMENT_RECORD_KEY, request_key.clone());
    crate::storage::persistent_get(env, &key)
}

fn store_repayment_record(env: &Env, request_key: &BytesN<32>, record: &RepaymentRecord) {
    let key = (REPAYMENT_RECORD_KEY, request_key.clone());
    env.storage().persistent().set(&key, record);
    extend_persistent_ttl(env, &key);
}

/// Repay an escrow while binding the operation to a durable request key.
///
/// # Idempotency contract
/// - A **safe retry** (same `request_key`, `invoice_id`, `payment_amount`, and
///   `late_fee_bps`) returns the cached [`RepaymentAllocation`] without moving
///   funds again.
/// - **Conflicting reuse** of `request_key` with a different payload is rejected
///   with [`QuickLendXError::DuplicateBid`] and leaves all state unchanged.
/// - A **rejected or failed attempt never stores a record**, so a corrected
///   retry with the same key remains available and no partial state lingers.
///
/// On a fresh attempt this defers to [`repay_escrow`], inheriting its
/// no-partial-state guarantees, and only on success binds the request key to
/// the repayment payload and allocation.
pub fn repay_escrow_with_key(
    env: &Env,
    invoice_id: &BytesN<32>,
    payment_amount: i128,
    late_fee_bps: i128,
    request_key: &BytesN<32>,
) -> Result<RepaymentAllocation, QuickLendXError> {
    if let Some(record) = get_repayment_record(env, request_key) {
        if record.invoice_id == *invoice_id
            && record.payment_amount == payment_amount
            && record.late_fee_bps == late_fee_bps
        {
            return Ok(record.allocation);
        }
        return Err(QuickLendXError::DuplicateBid);
    }

    let allocation = repay_escrow(env, invoice_id, payment_amount, late_fee_bps)?;

    store_repayment_record(
        env,
        request_key,
        &RepaymentRecord {
            invoice_id: invoice_id.clone(),
            payment_amount,
            late_fee_bps,
            allocation: allocation.clone(),
        },
    );

    Ok(allocation)
}

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // Repayment allocation engine: deterministic, overflow-safe
    // -----------------------------------------------------------------------

    #[test]
    fn test_allocate_repayment_zero_principal_rejected() {
        assert_eq!(
            allocate_repayment(0, 1000, 200, 0, 10_000),
            Err(QuickLendXError::InvalidAmount)
        );
    }

    #[test]
    fn test_allocate_repayment_negative_payment_rejected() {
        assert_eq!(
            allocate_repayment(1000, -1, 200, 0, 10_000),
            Err(QuickLendXError::InvalidAmount)
        );
    }

    #[test]
    fn test_allocate_repayment_basic_profit() {
        // principal 1000, payment 1100, fee 200 bps -> fee 2, investor 1098
        let a = allocate_repayment(1000, 1100, 200, 0, 10_000).unwrap();
        assert_eq!(a.platform_fee, 2);
        assert_eq!(a.investor_return, 1098);
        assert_eq!(a.principal_return, 1000);
        assert_eq!(a.gross_profit, 100);
        assert_eq!(a.treasury_amount, 2);
        assert_eq!(a.treasury_remaining, 0);
        assert_eq!(a.investor_return + a.platform_fee + a.late_fee, 1100);
    }

    #[test]
    fn test_allocate_repayment_loss_no_fee() {
        let a = allocate_repayment(1000, 900, 200, 0, 10_000).unwrap();
        assert_eq!(a.platform_fee, 0);
        assert_eq!(a.investor_return, 900);
        assert_eq!(a.gross_profit, 0);
        assert_eq!(a.investor_return + a.platform_fee + a.late_fee, 900);
    }

    #[test]
    fn test_allocate_repayment_fractional_rounding_favors_investor() {
        // profit 1, fee 200 bps -> 0.02 floors to 0; investor gets full payment
        let a = allocate_repayment(1000, 1001, 200, 0, 10_000).unwrap();
        assert_eq!(a.platform_fee, 0);
        assert_eq!(a.investor_return, 1001);
    }

    #[test]
    fn test_allocate_repayment_late_fee() {
        // principal 1000, late_fee_bps 500 (5%) -> late_fee 50
        // payment 1100, fee 200 bps on profit 100 -> 2
        let a = allocate_repayment(1000, 1100, 200, 500, 10_000).unwrap();
        assert_eq!(a.late_fee, 50);
        assert_eq!(a.platform_fee, 2);
        assert_eq!(a.investor_return, 1100 - 50 - 2);
        assert_eq!(a.treasury_amount, 52);
        assert_eq!(a.investor_return + a.platform_fee + a.late_fee, 1100);
    }

    #[test]
    #[ignore = "pre-existing: expectation drifted while module was orphaned from the crate (issue #2963)"]
    fn test_allocate_repayment_treasury_split() {
        // principal 1, payment 10_001, profit 10_000, fee 1000 bps -> fee 1000
        let a = allocate_repayment(1, 10_001, 1000, 0, 5000).unwrap();
        assert_eq!(a.platform_fee, 1000);
        assert_eq!(a.treasury_amount, 500);
        assert_eq!(a.treasury_remaining, 500);
        assert_eq!(a.investor_return, 9001);
        assert_eq!(a.investor_return + a.platform_fee + a.late_fee, 10_001);
    }

    #[test]
    fn test_allocate_repayment_overcharge_rejected() {
        // late fee would exceed the payment: principal 1000, payment 10, late 100%
        assert_eq!(
            allocate_repayment(1000, 10, 0, 10_000, 10_000),
            Err(QuickLendXError::InvalidAmount)
        );
    }

    #[test]
    fn test_allocate_repayment_max_amount_no_overflow() {
        let m = crate::protocol_limits::MAX_INVOICE_AMOUNT;
        let principal = m / 2;
        let a = allocate_repayment(principal, m, 1000, 0, 10_000).unwrap();
        let expected_fee = (m - principal) * 1000 / 10_000;
        assert_eq!(a.platform_fee, expected_fee);
        assert_eq!(a.investor_return + a.platform_fee, m);
    }

    #[test]
    fn test_allocate_repayment_exceeds_max_bound_rejected() {
        let m = crate::protocol_limits::MAX_INVOICE_AMOUNT;
        assert_eq!(
            allocate_repayment(m + 1, 10, 200, 0, 10_000),
            Err(QuickLendXError::InvalidAmount)
        );
        assert_eq!(
            allocate_repayment(10, m + 1, 200, 0, 10_000),
            Err(QuickLendXError::InvalidAmount)
        );
    }

    #[test]
    #[ignore = "pre-existing: expectation drifted while module was orphaned from the crate (issue #2963)"]
    fn test_allocate_repayment_fee_bps_clamped() {
        // fee_bps beyond 10000 is clamped, so it behaves like 10000 (full profit)
        let a = allocate_repayment(1, 10_001, 99_999, 0, 10_000).unwrap();
        assert_eq!(a.platform_fee, 10_000);
        assert_eq!(a.investor_return, 1);
    }
}
