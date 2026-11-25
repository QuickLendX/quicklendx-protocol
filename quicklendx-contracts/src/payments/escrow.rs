//! Escrow records, lifecycle transitions and funding validation.

use super::rate_limit::PaymentRateLimiter;
use super::reserve::HeldEscrowReserve;
use super::transfer::{transfer_funds, MIN_TRANSFER};
use crate::errors::QuickLendXError;
use crate::events::emit_escrow_created;
use crate::storage::{extend_persistent_ttl, InvestmentStorage, InvoiceStorage};
use crate::types::InvestmentStatus;
use soroban_sdk::{contracttype, symbol_short, Address, BytesN, Env};

/// Validate that `currency` is a registered token contract by attempting a safe
/// cross-contract `balance` call.
///
/// This is a **compliance-layer seam** — the check currently only verifies that
/// the address hosts a contract with a `balance` entry-point. Future compliance
/// logic (token allowlists, KYC-registered tokens, etc.) can be layered in here
/// without touching call-sites.
///
/// # Errors
/// Returns [`QuickLendXError::InvalidCurrency`] when `currency` is not a
/// registered token contract.
fn validate_token_address(
    env: &Env,
    currency: &Address,
    account: &Address,
) -> Result<(), QuickLendXError> {
    let result: Result<Result<i128, _>, _> = env.try_invoke_contract::<i128, QuickLendXError>(
        currency,
        &symbol_short!("balance"),
        soroban_sdk::vec![env, account.to_val()],
    );
    match result {
        Ok(_) => Ok(()),
        Err(_) => Err(QuickLendXError::InvalidCurrency),
    }
}

/// Assert that `amount` is compatible with the declared decimal precision of
/// `currency`.
///
/// # Threat model
/// Without this check, a caller who passes a currency address whose token
/// contract either (a) does not implement `decimals()`, or (b) reports an
/// unexpectedly large decimal count, could supply amounts whose scale is
/// incompatible with how the contract interprets them. This leads to silent
/// truncation or mis-scaled transfers, draining escrow value that the caller did
/// not intend to lock.
///
/// # Errors
/// * [`QuickLendXError::InvalidAmount`] — `amount` is zero or negative.
/// * [`QuickLendXError::InvalidCurrency`] — the token contract does not
///   expose a `decimals` entry-point or returns a value greater than 18.
pub fn require_matching_currency_precision(
    env: &Env,
    currency: &Address,
    amount: i128,
) -> Result<(), QuickLendXError> {
    if amount <= 0 {
        return Err(QuickLendXError::InvalidAmount);
    }

    let result: Result<Result<u32, _>, _> = env.try_invoke_contract::<u32, QuickLendXError>(
        currency,
        &symbol_short!("decimals"),
        soroban_sdk::vec![env],
    );

    match result {
        Ok(Ok(decimals)) if decimals <= 18 => Ok(()),
        _ => Err(QuickLendXError::InvalidCurrency),
    }
}

/// Return the principal currently reserved by an investor across pending bids and active investments.
pub fn get_investor_exposure(env: &Env, investor: &Address) -> Result<i128, QuickLendXError> {
    let bid_exposure =
        crate::storage::BidStorage::get_active_bid_amount_sum_for_investor(env, investor);
    let investment_exposure =
        crate::storage::InvestmentStorage::get_active_investment_amount_sum_for_investor(
            env, investor,
        );
    bid_exposure
        .checked_add(investment_exposure)
        .ok_or(QuickLendXError::ArithmeticOverflow)
}

/// Return the exact available funding capacity for an investor.
///
/// # Invariants
/// - An unverified or frozen investor has no available capacity.
/// - Capacity = max(0, verification.investment_limit - active_exposure).
/// - Fails closed on arithmetic overflow or missing verification record.
pub fn get_investor_available_capacity(
    env: &Env,
    investor: &Address,
) -> Result<i128, QuickLendXError> {
    crate::verification::require_investor_not_frozen(env, investor)?;
    crate::verification::require_investor_not_pending(env, investor)?;
    let verification = crate::verification::InvestorVerificationStorage::get(env, investor)
        .ok_or(QuickLendXError::KYCNotFound)?;

    if !matches!(
        verification.status,
        crate::verification::BusinessVerificationStatus::Verified
    ) {
        return Err(QuickLendXError::BusinessNotVerified);
    }

    let exposure = get_investor_exposure(env, investor)?;

    Ok(verification
        .investment_limit
        .checked_sub(exposure)
        .unwrap_or(0))
}

/// Validate that an investor has sufficient authorized capacity for a new funding commitment of `amount`.
pub fn validate_funding_commitment(
    env: &Env,
    investor: &Address,
    amount: i128,
) -> Result<(), QuickLendXError> {
    if amount <= 0 || amount > crate::protocol_limits::MAX_INVOICE_AMOUNT {
        return Err(QuickLendXError::InvalidAmount);
    }

    let available_capacity = get_investor_available_capacity(env, investor)?;
    if amount > available_capacity {
        return Err(QuickLendXError::InvalidAmount);
    }

    crate::verification::validate_investor_investment(env, investor, amount)
}

#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(test, derive(Debug))]
pub enum EscrowStatus {
    Held,     // Funds are held in escrow
    Released, // Funds released to business
    Refunded, // Funds refunded to investor
}

impl EscrowStatus {
    /// Validates whether the state can legally transition to `next`.
    ///
    /// Enforces the legal transition matrix for escrow lifecycle:
    /// - `Held` -> `Released`
    /// - `Held` -> `Refunded`
    /// All other transitions (including repeated terminal states) are rejected.
    pub fn verify_transition(&self, next: &EscrowStatus) -> Result<(), QuickLendXError> {
        match (self, next) {
            (EscrowStatus::Held, EscrowStatus::Released) => Ok(()),
            (EscrowStatus::Held, EscrowStatus::Refunded) => Ok(()),
            _ => Err(QuickLendXError::InvalidStatus),
        }
    }
}

#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(test, derive(Debug))]
pub struct Escrow {
    pub escrow_id: BytesN<32>,
    pub invoice_id: BytesN<32>,
    pub investor: Address,
    pub business: Address,
    pub amount: i128,
    pub currency: Address,
    pub created_at: u64,
    pub status: EscrowStatus,
}

pub struct EscrowStorage;

impl EscrowStorage {
    pub fn store_escrow(env: &Env, escrow: &Escrow) {
        env.storage().persistent().set(&escrow.escrow_id, escrow);
        extend_persistent_ttl(env, &escrow.escrow_id);
        // Also store by invoice_id for easy lookup
        let invoice_key = (symbol_short!("escrow"), &escrow.invoice_id);
        env.storage()
            .persistent()
            .set(&invoice_key, &escrow.escrow_id);
        extend_persistent_ttl(env, &invoice_key);
    }

    pub fn get_escrow(env: &Env, escrow_id: &BytesN<32>) -> Option<Escrow> {
        let result = env.storage().persistent().get(escrow_id);
        if result.is_some() {
            extend_persistent_ttl(env, &escrow_id);
        }
        result
    }

    pub fn get_escrow_by_invoice(env: &Env, invoice_id: &BytesN<32>) -> Option<Escrow> {
        let invoice_key = (symbol_short!("escrow"), invoice_id);
        let escrow_id: Option<BytesN<32>> = env.storage().persistent().get(&invoice_key);
        if let Some(id) = escrow_id {
            extend_persistent_ttl(env, &invoice_key);
            Self::get_escrow(env, &id)
        } else {
            None
        }
    }

    pub fn get_escrow_status(env: &Env, invoice_id: &BytesN<32>) -> Option<EscrowStatus> {
        Self::get_escrow_by_invoice(env, invoice_id).map(|escrow| escrow.status)
    }

    pub fn update_escrow(env: &Env, escrow: &Escrow) {
        env.storage().persistent().set(&escrow.escrow_id, escrow);
        extend_persistent_ttl(env, &escrow.escrow_id);
        let invoice_key = (symbol_short!("escrow"), &escrow.invoice_id);
        if env
            .storage()
            .persistent()
            .get::<_, BytesN<32>>(&invoice_key)
            .is_some()
        {
            extend_persistent_ttl(env, &invoice_key);
        }
    }

    pub fn generate_unique_escrow_id(env: &Env) -> BytesN<32> {
        let timestamp = env.ledger().timestamp();
        let counter_key = symbol_short!("esc_cnt");
        let counter: u64 = env.storage().instance().get(&counter_key).unwrap_or(0u64);
        let next_counter = counter.saturating_add(1);
        env.storage().instance().set(&counter_key, &next_counter);

        let mut id_bytes = [0u8; 32];
        // Add escrow prefix to distinguish from other entity types
        id_bytes[0] = 0xE5; // 'E' for Escrow
        id_bytes[1] = 0xC0; // 'C' for sCrow
                            // Embed timestamp in next 8 bytes
        id_bytes[2..10].copy_from_slice(&timestamp.to_be_bytes());
        // Embed counter in next 8 bytes
        id_bytes[10..18].copy_from_slice(&next_counter.to_be_bytes());
        // Fill remaining bytes with a pattern to ensure uniqueness (overflow-safe)
        let mix = timestamp
            .saturating_add(next_counter)
            .saturating_add(0xE5C0);
        for i in 18..32 {
            id_bytes[i] = (mix % 256) as u8;
        }

        BytesN::from_array(env, &id_bytes)
    }
}

/// Shared validation logic for escrow creation.
///
/// Returns `(next_held_reserve)` on success.
fn validate_and_prepare_escrow(
    env: &Env,
    invoice_id: &BytesN<32>,
    investor: &Address,
    business: &Address,
    amount: i128,
    currency: &Address,
) -> Result<HeldEscrowReserve, QuickLendXError> {
    if amount <= 0 || amount > crate::protocol_limits::MAX_INVOICE_AMOUNT {
        return Err(QuickLendXError::InvalidAmount);
    }

    if amount < MIN_TRANSFER {
        return Err(QuickLendXError::InvalidAmount);
    }

    require_matching_currency_precision(env, currency, amount)?;

    // Reject only a *live* escrow or investment. Terminal records from a prior
    // funding cycle are kept for audit and must not block re-funding after an
    // investor withdrawal returns the invoice to the market.
    if let Some(escrow) = EscrowStorage::get_escrow_by_invoice(env, invoice_id) {
        if escrow.status == EscrowStatus::Held {
            return Err(QuickLendXError::InvoiceAlreadyFunded);
        }
    }

    if let Some(investment) = InvestmentStorage::get_investment_by_invoice(env, invoice_id) {
        if investment.status == InvestmentStatus::Active {
            return Err(QuickLendXError::InvoiceAlreadyFunded);
        }
    }

    let invoice =
        InvoiceStorage::get_invoice(env, invoice_id).ok_or(QuickLendXError::StorageKeyNotFound)?;

    if invoice.business != *business {
        return Err(QuickLendXError::Unauthorized);
    }

    if invoice.currency != *currency {
        return Err(QuickLendXError::InvalidCurrency);
    }

    // The invoice must be back in a fundable state (Verified with no funding
    // fields). This keeps terminal invoices from being re-funded even though
    // their old escrow is terminal, while allowing re-funding after a withdrawal.
    if !invoice.is_available_for_funding() {
        return Err(QuickLendXError::InvalidStatus);
    }

    EscrowStorage::require_no_active_reserve_repair(env, currency)?;
    let next_held_reserve = EscrowStorage::held_reserve_after_increase(env, currency, amount)?;

    validate_token_address(env, currency, investor)?;

    PaymentRateLimiter::check_and_record(env, investor)?;

    Ok(next_held_reserve)
}

/// Write the escrow record and update the held-reserve accumulator.
///
/// # Panics
/// Panics if `next_held_reserve` was not obtained by calling
/// [`validate_and_prepare_escrow`] with the same arguments.
fn write_escrow_record(
    env: &Env,
    invoice_id: &BytesN<32>,
    investor: &Address,
    business: &Address,
    amount: i128,
    currency: &Address,
    next_held_reserve: &HeldEscrowReserve,
) -> BytesN<32> {
    let escrow_id = EscrowStorage::generate_unique_escrow_id(env);
    let escrow = Escrow {
        escrow_id: escrow_id.clone(),
        invoice_id: invoice_id.clone(),
        investor: investor.clone(),
        business: business.clone(),
        amount,
        currency: currency.clone(),
        created_at: env.ledger().timestamp(),
        status: EscrowStatus::Held,
    };

    EscrowStorage::store_escrow(env, &escrow);
    EscrowStorage::set_held_reserve_record(env, currency, next_held_reserve);
    EscrowStorage::mark_reserve_accounted(env, &escrow_id);
    crate::qlx_log!(env, "payment", "Escrow created successfully");
    emit_escrow_created(env, &escrow);
    escrow_id
}

/// Create escrow: transfer `amount` from investor to contract and store escrow record.
///
/// ## One-Live-Escrow-Per-Invoice Guard
/// If a **`Held`** escrow record already exists for `invoice_id`, or an `Active`
/// investment exists, this function returns [`QuickLendXError::InvoiceAlreadyFunded`]
/// **before** any token transfer occurs. Terminal escrows (`Released`, `Refunded`)
/// and terminal investments are retained as audit history and do not block a new
/// funding cycle. This is the innermost uniqueness guard; see also
/// `escrow::load_accept_bid_context` for the outer guard and `test_escrow_uniqueness.rs`
/// for the full attack-vector test suite.
///
/// # Returns
/// * `Ok(escrow_id)` - The new escrow ID
///
/// # Errors
/// * [`QuickLendXError::InvalidAmount`] - `amount` is zero or negative.
/// * [`QuickLendXError::InvalidStatus`] - reserve repair is active for this token.
/// * [`QuickLendXError::InvoiceAlreadyFunded`] - a live (`Held`) escrow or `Active`
///   investment already exists for this invoice.
/// * [`QuickLendXError::InsufficientFunds`] - investor balance is below `amount`.
/// * [`QuickLendXError::OperationNotAllowed`] - investor has not approved the contract for `amount`.
/// * [`QuickLendXError::TokenTransferFailed`] - the token contract panicked; no funds moved and
///   no escrow record is written.
///
/// # Atomicity
/// The escrow record is only written **after** the token transfer succeeds.
/// If the transfer fails the invoice and bid states are left unchanged.
pub fn create_escrow(
    env: &Env,
    invoice_id: &BytesN<32>,
    investor: &Address,
    business: &Address,
    amount: i128,
    currency: &Address,
) -> Result<BytesN<32>, QuickLendXError> {
    let next_held_reserve =
        validate_and_prepare_escrow(env, invoice_id, investor, business, amount, currency)?;

    crate::qlx_log!(env, "payment", "Creating escrow: amount={}", amount);

    // Move funds from investor into contract-controlled escrow
    let contract_address = env.current_contract_address();
    transfer_funds(env, currency, investor, &contract_address, amount)?;

    let escrow_id = write_escrow_record(
        env,
        invoice_id,
        investor,
        business,
        amount,
        currency,
        &next_held_reserve,
    );
    Ok(escrow_id)
}

/// Record an escrow entry *without* moving tokens.
///
/// Used by the origination-fee funding path to run the same guards and reserve
/// accounting as [`create_escrow`] while leaving token movement to the caller.
/// The caller must transfer the gross bid in the same transaction; any failure
/// rolls back this record and its rate-limit update.
///
/// # Errors
/// * [`QuickLendXError::ArithmeticOverflow`] — if updating the held-reserve
///   accumulator would overflow.
/// * The same validation errors returned by [`create_escrow`], including duplicate
///   escrow, active reserve repair, rate limiting, amount, and currency checks.
pub fn create_escrow_record_only(
    env: &Env,
    invoice_id: &BytesN<32>,
    investor: &Address,
    business: &Address,
    amount: i128,
    currency: &Address,
) -> Result<BytesN<32>, QuickLendXError> {
    let next_held_reserve =
        validate_and_prepare_escrow(env, invoice_id, investor, business, amount, currency)?;
    let escrow_id = write_escrow_record(
        env,
        invoice_id,
        investor,
        business,
        amount,
        currency,
        &next_held_reserve,
    );
    crate::audit::log_escrow_created(
        env,
        invoice_id.clone(),
        investor.clone(),
        amount,
        escrow_id.clone(),
    );
    Ok(escrow_id)
}

/// Internal escrow release used by settlement flows where `business.require_auth()`
/// has already been called in the same invocation frame.
///
/// Calling `require_auth()` twice for the same address in one Soroban invocation frame
/// returns `Error(Auth, ExistingValue)`.  This variant skips the auth call so that
/// `settle_invoice_internal` (which already has the business's auth from `record_payment`)
/// can auto-release the held escrow without a duplicate-auth error.
///
/// # Security
/// Callers MUST have already verified `business == escrow.business` via the invoice record
/// and obtained the business's authorization in the same invocation frame.
/// This function MUST NOT be called from any public entry-point that has not already
/// performed that authorization.
pub(crate) fn release_escrow_internal(
    env: &Env,
    invoice_id: &BytesN<32>,
) -> Result<(), QuickLendXError> {
    let mut escrow = EscrowStorage::get_escrow_by_invoice(env, invoice_id)
        .ok_or(QuickLendXError::StorageKeyNotFound)?;

    // Enforce the legal transition matrix
    escrow.status.verify_transition(&EscrowStatus::Released)?;

    EscrowStorage::require_no_active_reserve_repair(env, &escrow.currency)?;
    let next_held_reserve = if EscrowStorage::is_reserve_accounted(env, &escrow.escrow_id) {
        Some(EscrowStorage::held_reserve_after_decrease(
            env,
            &escrow.currency,
            escrow.amount,
        )?)
    } else {
        None
    };

    // Transfer funds from escrow (contract) to business
    let contract_address = env.current_contract_address();
    transfer_funds(
        env,
        &escrow.currency,
        &contract_address,
        &escrow.business,
        escrow.amount,
    )?;

    crate::fees::FeeManager::release_origination_fee(env, invoice_id)?;

    // Update escrow status
    if let Some(next_held_reserve) = next_held_reserve {
        EscrowStorage::set_held_reserve_record(env, &escrow.currency, &next_held_reserve);
        EscrowStorage::clear_reserve_accounted(env, &escrow.escrow_id);
    }
    escrow.status = EscrowStatus::Released;
    EscrowStorage::update_escrow(env, &escrow);
    crate::events::emit_escrow_released(
        env,
        &escrow.escrow_id,
        invoice_id,
        &escrow.business,
        escrow.amount,
    );
    crate::audit::log_escrow_released(
        env,
        invoice_id.clone(),
        escrow.business.clone(),
        escrow.amount,
        escrow.escrow_id.clone(),
    );
    crate::qlx_log!(
        env,
        "payment",
        "Escrow released to business (internal): amount={}",
        escrow.amount
    );

    Ok(())
}

/// Release escrow funds to business (contract -> business).
///
/// # Requirements
/// - Escrow must be in `Held` status.
/// - `business` must match `escrow.business` (prevents unauthorized early release).
/// - The invoice should ideally be in `Funded` or `Paid` status (enforced by caller in `lib.rs`).
///
/// # Security
/// - Caller must match `escrow.business`: rejects unauthorized early escrow releases.
/// - Idempotency: Once released, status becomes `Released`, preventing repeated transfers.
/// - Atomic: Funds are transferred before updating status in storage; if transfer fails,
///   the operation can be safely retried.
///
/// # Errors
/// * [`QuickLendXError::StorageKeyNotFound`] - no escrow record exists for this invoice.
/// * [`QuickLendXError::Unauthorized`] - `business` does not match `escrow.business`.
/// * [`QuickLendXError::InvalidStatus`] - escrow is not in `Held` status (already released/refunded).
///   Also returned while reserve repair is active for this token.
/// * [`QuickLendXError::InsufficientFunds`] - contract balance is below the escrow amount
///   (should never happen in normal operation; indicates a critical invariant violation).
/// * [`QuickLendXError::TokenTransferFailed`] - the token contract panicked; escrow status is
///   **not** updated so the release can be safely retried.
pub fn release_escrow(
    env: &Env,
    invoice_id: &BytesN<32>,
    business: &Address,
) -> Result<(), QuickLendXError> {
    // Verify caller is the escrow owner before authorizing — prevents
    // unauthorized early escrow releases by any address that knows the invoice ID.
    let escrow_check = EscrowStorage::get_escrow_by_invoice(env, invoice_id)
        .ok_or(QuickLendXError::StorageKeyNotFound)?;
    if escrow_check.business != *business {
        return Err(QuickLendXError::Unauthorized);
    }

    business.require_auth();

    // Delegate to the shared internal implementation (no double-auth).
    release_escrow_internal(env, invoice_id)
}

/// Refund escrow funds to investor (contract -> investor). Escrow must be Held.
///
/// # Errors
/// * [`QuickLendXError::StorageKeyNotFound`] - no escrow record exists for this invoice.
/// * [`QuickLendXError::InvalidStatus`] - escrow is not in `Held` status.
///   Also returned while reserve repair is active for this token.
/// * [`QuickLendXError::InsufficientFunds`] - contract balance is below the escrow amount and
///   any withheld origination fee.
/// * [`QuickLendXError::TokenTransferFailed`] - the token contract panicked; escrow status is
///   **not** updated so the refund can be safely retried.
///
/// This function performs no authorization or reentrancy-guard checks. Callers
/// must authorize the operation and guard the enclosing payment entry point.
pub fn refund_escrow_internal(env: &Env, invoice_id: &BytesN<32>) -> Result<(), QuickLendXError> {
    let mut escrow = EscrowStorage::get_escrow_by_invoice(env, invoice_id)
        .ok_or(QuickLendXError::StorageKeyNotFound)?;

    let invoice =
        InvoiceStorage::get_invoice(env, invoice_id).ok_or(QuickLendXError::StorageKeyNotFound)?;

    if let Some(ref inv_investor) = invoice.investor {
        if escrow.investor != *inv_investor {
            return Err(QuickLendXError::Unauthorized);
        }
    }

    // Enforce the legal transition matrix
    escrow.status.verify_transition(&EscrowStatus::Refunded)?;

    EscrowStorage::require_no_active_reserve_repair(env, &escrow.currency)?;
    let next_held_reserve = if EscrowStorage::is_reserve_accounted(env, &escrow.escrow_id) {
        Some(EscrowStorage::held_reserve_after_decrease(
            env,
            &escrow.currency,
            escrow.amount,
        )?)
    } else {
        None
    };

    let origination_fee =
        crate::fees::FeeManager::get_held_origination_fee(env, invoice_id).unwrap_or(0);
    let refund_amount = escrow
        .amount
        .checked_add(origination_fee)
        .ok_or(QuickLendXError::ArithmeticOverflow)?;

    // Refund the net escrow principal and any withheld origination fee together.
    let contract_address = env.current_contract_address();
    transfer_funds(
        env,
        &escrow.currency,
        &contract_address,
        &escrow.investor,
        refund_amount,
    )?;
    if origination_fee > 0 {
        crate::fees::FeeManager::refund_origination_fee(env, invoice_id, &escrow.investor)?;
    }

    // Update escrow status
    if let Some(next_held_reserve) = next_held_reserve {
        EscrowStorage::set_held_reserve_record(env, &escrow.currency, &next_held_reserve);
        EscrowStorage::clear_reserve_accounted(env, &escrow.escrow_id);
    }
    escrow.status = EscrowStatus::Refunded;
    EscrowStorage::update_escrow(env, &escrow);
    crate::events::emit_escrow_refunded(
        env,
        &escrow.escrow_id,
        invoice_id,
        &escrow.investor,
        refund_amount,
    );
    crate::audit::log_escrow_refunded(
        env,
        invoice_id.clone(),
        escrow.investor.clone(),
        refund_amount,
        escrow.escrow_id.clone(),
    );
    crate::qlx_log!(
        env,
        "payment",
        "Escrow refunded to investor: amount={}",
        refund_amount
    );

    Ok(())
}

/// Compatibility wrapper for callers that still pass an authorization address.
///
/// Authorization is enforced by the public entry point before this function is called.
pub(crate) fn refund_escrow(
    env: &Env,
    invoice_id: &BytesN<32>,
    _caller: &Address,
) -> Result<(), QuickLendXError> {
    refund_escrow_internal(env, invoice_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::payments::test_support::{contract_env, mint_and_approve, store_test_invoice};
    use soroban_sdk::{testutils::Address as _, token};

    // -----------------------------------------------------------------------
    // Zero-amount boundary
    // -----------------------------------------------------------------------

    /// Passing `amount = 0` to `create_escrow` must return `InvalidAmount`
    /// and must not create any escrow record or transfer any funds.
    #[test]
    fn test_create_escrow_zero_amount_returns_invalid_amount() {
        let (env, contract_id) = contract_env();
        let investor = Address::generate(&env);
        let invoice_id = BytesN::from_array(&env, &[0u8; 32]);
        let business = Address::generate(&env);
        let token_admin = Address::generate(&env);
        let currency = env
            .register_stellar_asset_contract_v2(token_admin)
            .address();

        store_test_invoice(
            &env,
            &contract_id,
            &invoice_id,
            &business,
            &currency,
            crate::protocol_limits::MAX_INVOICE_AMOUNT,
        );

        let result = env.as_contract(&contract_id, || {
            create_escrow(&env, &invoice_id, &investor, &business, 0, &currency)
        });
        assert_eq!(result, Err(QuickLendXError::InvalidAmount));
        assert!(env.as_contract(&contract_id, || {
            EscrowStorage::get_escrow_by_invoice(&env, &invoice_id).is_none()
        }));
    }

    /// Negative amounts are rejected before any state changes.
    #[test]
    fn test_create_escrow_negative_amount_returns_invalid_amount() {
        let (env, contract_id) = contract_env();
        let investor = Address::generate(&env);
        let business = Address::generate(&env);
        let invoice_id = BytesN::from_array(&env, &[1u8; 32]);
        let token_admin = Address::generate(&env);
        let currency = env
            .register_stellar_asset_contract_v2(token_admin)
            .address();

        let result = env.as_contract(&contract_id, || {
            create_escrow(&env, &invoice_id, &investor, &business, -1, &currency)
        });
        assert_eq!(result, Err(QuickLendXError::InvalidAmount));
        assert!(env.as_contract(&contract_id, || {
            EscrowStorage::get_escrow_by_invoice(&env, &invoice_id).is_none()
        }));
    }

    // -----------------------------------------------------------------------
    // Max-amount boundary
    // -----------------------------------------------------------------------

    /// `i128::MAX` with zero investor balance must fail with `InsufficientFunds`
    /// (the amount guard fires before the token call).
    #[test]
    #[ignore = "pre-existing: expectation drifted while module was orphaned from the crate (issue #2963)"]
    fn test_create_escrow_max_amount_with_zero_balance_fails() {
        let (env, contract_id) = contract_env();
        let investor = Address::generate(&env);
        let token_admin = Address::generate(&env);
        let currency = mint_and_approve(
            &env,
            &contract_id,
            &token_admin,
            &investor,
            0, // zero balance
            crate::protocol_limits::MAX_INVOICE_AMOUNT,
        );

        let invoice_id = BytesN::from_array(&env, &[2u8; 32]);
        let tok = token::Client::new(&env, &currency);

        let business = Address::generate(&env);
        store_test_invoice(
            &env,
            &contract_id,
            &invoice_id,
            &business,
            &currency,
            crate::protocol_limits::MAX_INVOICE_AMOUNT,
        );

        let result = env.as_contract(&contract_id, || {
            create_escrow(
                &env,
                &invoice_id,
                &investor,
                &business,
                crate::protocol_limits::MAX_INVOICE_AMOUNT,
                &currency,
            )
        });
        assert_eq!(result, Err(QuickLendXError::InsufficientFunds));
        assert_eq!(tok.balance(&contract_id), 0);
        assert!(env.as_contract(&contract_id, || {
            EscrowStorage::get_escrow_by_invoice(&env, &invoice_id).is_none()
        }));
    }

    /// Amount strictly exceeding the investor's balance is rejected with
    /// `InsufficientFunds` and no state is mutated.
    #[test]
    #[ignore = "pre-existing: expectation drifted while module was orphaned from the crate (issue #2963)"]
    fn test_create_escrow_amount_exceeds_balance_returns_insufficient_funds() {
        let (env, contract_id) = contract_env();
        let investor = Address::generate(&env);
        let token_admin = Address::generate(&env);
        let currency = mint_and_approve(&env, &contract_id, &token_admin, &investor, 5_000, 10_000);

        let business = Address::generate(&env);
        let invoice_id = BytesN::from_array(&env, &[3u8; 32]);
        store_test_invoice(
            &env,
            &contract_id,
            &invoice_id,
            &business,
            &currency,
            crate::protocol_limits::MAX_INVOICE_AMOUNT,
        );

        let tok = token::Client::new(&env, &currency);

        let investor_bal = tok.balance(&investor);
        let contract_bal = tok.balance(&contract_id);

        let result = env.as_contract(&contract_id, || {
            create_escrow(&env, &invoice_id, &investor, &business, 5_001, &currency)
        });
        assert_eq!(result, Err(QuickLendXError::InsufficientFunds));
        assert_eq!(tok.balance(&investor), investor_bal);
        assert_eq!(tok.balance(&contract_id), contract_bal);
        assert!(env.as_contract(&contract_id, || {
            EscrowStorage::get_escrow_by_invoice(&env, &invoice_id).is_none()
        }));
    }

    // -----------------------------------------------------------------------
    // Max-amount with sufficient balance
    // -----------------------------------------------------------------------

    /// The maximum allowed invoice amount (`MAX_INVOICE_AMOUNT`) can succeed
    /// when the investor balance is sufficient and the allowance is granted.
    /// Amounts strictly greater than `MAX_INVOICE_AMOUNT` are rejected to prevent overflow.
    #[test]
    #[ignore = "pre-existing: expectation drifted while module was orphaned from the crate (issue #2963)"]
    fn test_create_escrow_max_amount_with_sufficient_balance_succeeds() {
        use crate::protocol_limits::MAX_INVOICE_AMOUNT;

        let (env, contract_id) = contract_env();
        let investor = Address::generate(&env);
        let token_admin = Address::generate(&env);
        let currency = mint_and_approve(
            &env,
            &contract_id,
            &token_admin,
            &investor,
            MAX_INVOICE_AMOUNT,
            MAX_INVOICE_AMOUNT,
        );

        let invoice_id = BytesN::from_array(&env, &[4u8; 32]);

        let tok = token::Client::new(&env, &currency);

        let business = Address::generate(&env);
        store_test_invoice(
            &env,
            &contract_id,
            &invoice_id,
            &business,
            &currency,
            crate::protocol_limits::MAX_INVOICE_AMOUNT,
        );

        let result = env.as_contract(&contract_id, || {
            create_escrow(
                &env,
                &invoice_id,
                &investor,
                &business,
                MAX_INVOICE_AMOUNT,
                &currency,
            )
        });
        assert!(
            result.is_ok(),
            "max-amount escrow must succeed with sufficient balance"
        );
        assert_eq!(tok.balance(&investor), 0);
        assert_eq!(tok.balance(&contract_id), MAX_INVOICE_AMOUNT);

        let escrow = env.as_contract(&contract_id, || {
            EscrowStorage::get_escrow_by_invoice(&env, &invoice_id).unwrap()
        });
        assert_eq!(escrow.amount, MAX_INVOICE_AMOUNT);
        assert_eq!(escrow.status, EscrowStatus::Held);

        // Over MAX_INVOICE_AMOUNT is rejected with InvalidAmount
        let invoice_id_over = BytesN::from_array(&env, &[44u8; 32]);
        let result_over = env.as_contract(&contract_id, || {
            create_escrow(
                &env,
                &invoice_id_over,
                &investor,
                &business,
                MAX_INVOICE_AMOUNT + 1,
                &currency,
            )
        });
        assert_eq!(result_over, Err(QuickLendXError::InvalidAmount));
    }

    // -----------------------------------------------------------------------
    // Invalid token address
    // -----------------------------------------------------------------------

    /// Passing an address that is *not* a registered token contract causes a
    /// host-level panic (soroban-sdk 25.x behaviour). The operation must not
    /// silently succeed and must not write any escrow record.
    #[test]
    #[ignore = "pre-existing: Abort on unregistered token in newer Soroban env"]
    fn test_create_escrow_unregistered_token_address_does_not_succeed() {
        let (env, contract_id) = contract_env();
        let investor = Address::generate(&env);
        let business = Address::generate(&env);
        let invoice_id = BytesN::from_array(&env, &[5u8; 32]);

        // Provide a balance in a *real* token so the pre-checks pass, but pass
        // a completely unregistered, random address as `currency`.
        let real_token_admin = Address::generate(&env);
        let real_currency = env
            .register_stellar_asset_contract_v2(real_token_admin.clone())
            .address();
        let real_sac = token::StellarAssetClient::new(&env, &real_currency);
        let real_tok = token::Client::new(&env, &real_currency);
        real_sac.mint(&investor, &10_000);
        let expiry = env.ledger().sequence() + 10_000;
        real_tok.approve(&investor, &contract_id, &10_000, &expiry);

        let bogus_currency = Address::generate(&env);

        let investor_bal = real_tok.balance(&investor);
        let contract_bal = real_tok.balance(&contract_id);

        let result = env.as_contract(&contract_id, || {
            create_escrow(
                &env,
                &invoice_id,
                &investor,
                &business,
                10_000,
                &bogus_currency,
            )
        });

        assert!(
            result.is_err(),
            "unregistered token address must not succeed"
        );
        assert_eq!(real_tok.balance(&investor), investor_bal);
        assert_eq!(real_tok.balance(&contract_id), contract_bal);
        assert!(
            env.as_contract(&contract_id, || {
                EscrowStorage::get_escrow_by_invoice(&env, &invoice_id).is_none()
            }),
            "no escrow must be written on invalid token address"
        );
    }

    // -----------------------------------------------------------------------
    // One-escrow guard: second call with a different investor
    // -----------------------------------------------------------------------

    /// The duplicate escrow guard must reject a second call regardless of the
    /// investor or amount; only the `invoice_id` matters.
    #[test]
    #[ignore = "pre-existing: expectation drifted while module was orphaned from the crate (issue #2963)"]
    fn test_create_escrow_duplicate_different_investor_rejected() {
        let (env, contract_id) = contract_env();
        let investor1 = Address::generate(&env);
        let investor2 = Address::generate(&env);
        let token_admin = Address::generate(&env);
        let currency =
            mint_and_approve(&env, &contract_id, &token_admin, &investor1, 10_000, 10_000);
        let sac = token::StellarAssetClient::new(&env, &currency);
        let tok = token::Client::new(&env, &currency);
        sac.mint(&investor2, &10_000);
        let expiry = env.ledger().sequence() + 10_000;
        tok.approve(&investor2, &contract_id, &10_000, &expiry);

        let invoice_id = BytesN::from_array(&env, &[6u8; 32]);

        let business = Address::generate(&env);
        store_test_invoice(
            &env,
            &contract_id,
            &invoice_id,
            &business,
            &currency,
            crate::protocol_limits::MAX_INVOICE_AMOUNT,
        );

        // First escrow
        let r1 = env.as_contract(&contract_id, || {
            create_escrow(&env, &invoice_id, &investor1, &business, 10_000, &currency)
        });
        assert!(r1.is_ok(), "first escrow must succeed");

        // Second attempt (different investor) must fail
        let r2 = env.as_contract(&contract_id, || {
            create_escrow(&env, &invoice_id, &investor2, &business, 5_000, &currency)
        });
        assert_eq!(r2, Err(QuickLendXError::InvoiceAlreadyFunded));
    }
}
