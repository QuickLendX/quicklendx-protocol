//! Payment and escrow operations: create escrow, release, refund, and token transfers.
//!
//! Public release/refund entry points are wrapped with a reentrancy guard in lib.rs.

use crate::errors::QuickLendXError;
use crate::events::emit_escrow_created;
use crate::storage::{extend_persistent_ttl, InvoiceStorage};
use crate::types::RebuildReport;
use soroban_sdk::token;
use soroban_sdk::{contracttype, symbol_short, Address, BytesN, Env, Symbol, TryFromVal, Val, Vec};

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

/// Minimum transfer amount to prevent dust transfers.
/// Matches the test-mode MIN_TRANSFER from protocol_limits.rs.
#[cfg(not(test))]
pub const MIN_TRANSFER: i128 = 1_000_000; // 1 token (6 decimals)
#[cfg(test)]
pub const MIN_TRANSFER: i128 = 10;

/// Maximum number of payment/escrow operations allowed per rate-limit window per account.
#[cfg(not(test))]
pub const MAX_PAYMENTS_PER_WINDOW: u32 = 20;
#[cfg(test)]
pub const MAX_PAYMENTS_PER_WINDOW: u32 = 5;

/// Window duration for payment rate limiting (in seconds).
pub const PAYMENT_RATE_LIMIT_WINDOW_SECS: u64 = 60;

const PAYMENT_RATE_LIMIT_KEY: Symbol = symbol_short!("pay_rl");

/// Snapshot of an account's payment rate limit state.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PaymentRateLimitRecord {
    pub window_start: u64,
    pub count: u32,
}

/// Bounded rate limiter for payment, escrow, and funding commitment operations.
pub struct PaymentRateLimiter;

impl PaymentRateLimiter {
    fn key(account: &Address) -> (Symbol, Address) {
        (PAYMENT_RATE_LIMIT_KEY, account.clone())
    }

    /// Check and advance the rate limit for `account`.
    /// Rejects with [`QuickLendXError::OperationNotAllowed`] when the account exceeds
    /// the allowed operation count in the active window.
    pub fn check_and_record(env: &Env, account: &Address) -> Result<(), QuickLendXError> {
        let key = Self::key(account);
        let now = env.ledger().timestamp();
        let mut record = env
            .storage()
            .persistent()
            .get::<_, PaymentRateLimitRecord>(&key)
            .unwrap_or(PaymentRateLimitRecord {
                window_start: now,
                count: 0,
            });

        if now
            >= record
                .window_start
                .saturating_add(PAYMENT_RATE_LIMIT_WINDOW_SECS)
        {
            record.window_start = now;
            record.count = 1;
        } else {
            if record.count >= MAX_PAYMENTS_PER_WINDOW {
                return Err(QuickLendXError::OperationNotAllowed);
            }
            record.count = record.count.saturating_add(1);
        }

        env.storage().persistent().set(&key, &record);
        extend_persistent_ttl(env, &key);
        Ok(())
    }

    /// Read the rate limit record for `account` without mutating storage.
    pub fn get_rate_limit(env: &Env, account: &Address) -> PaymentRateLimitRecord {
        let key = Self::key(account);
        let now = env.ledger().timestamp();
        env.storage()
            .persistent()
            .get::<_, PaymentRateLimitRecord>(&key)
            .map(|mut r| {
                if now
                    >= r.window_start
                        .saturating_add(PAYMENT_RATE_LIMIT_WINDOW_SECS)
                {
                    r.window_start = now;
                    r.count = 0;
                }
                r
            })
            .unwrap_or(PaymentRateLimitRecord {
                window_start: now,
                count: 0,
            })
    }
}

/// Return the principal currently reserved by an investor across pending bids and active investments.
pub fn get_investor_exposure(env: &Env, investor: &Address) -> i128 {
    let bid_exposure =
        crate::storage::BidStorage::get_active_bid_amount_sum_for_investor(env, investor);
    let investment_exposure =
        crate::storage::InvestmentStorage::get_active_investment_amount_sum_for_investor(
            env, investor,
        );
    bid_exposure.saturating_add(investment_exposure)
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

    let exposure = get_investor_exposure(env, investor);
    if exposure >= i128::MAX {
        return Err(QuickLendXError::ArithmeticOverflow);
    }

    Ok(verification.investment_limit.saturating_sub(exposure))
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

#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(test, derive(Debug))]
struct HeldEscrowReserve {
    amount: i128,
    complete: bool,
    repair_next_offset: u32,
}

pub struct EscrowStorage;

const HELD_ESCROW_RESERVE_KEY: Symbol = symbol_short!("esc_res");
const ESCROW_RESERVE_MARKER_KEY: Symbol = symbol_short!("esc_acc");
const HELD_RESERVE_REPAIR_IDS_KEY: Symbol = symbol_short!("esc_rids");
#[cfg(not(test))]
const MAX_REPAIR_SNAPSHOT_IDS: u64 = 1_000;
#[cfg(test)]
const MAX_REPAIR_SNAPSHOT_IDS: u64 = 3;

impl EscrowStorage {
    fn held_reserve_key(currency: &Address) -> (Symbol, Address) {
        (HELD_ESCROW_RESERVE_KEY.clone(), currency.clone())
    }

    fn reserve_marker_key(escrow_id: &BytesN<32>) -> (Symbol, BytesN<32>) {
        (ESCROW_RESERVE_MARKER_KEY.clone(), escrow_id.clone())
    }

    fn held_reserve_repair_ids_key(currency: &Address) -> (Symbol, Address) {
        (HELD_RESERVE_REPAIR_IDS_KEY.clone(), currency.clone())
    }

    fn empty_reserve() -> HeldEscrowReserve {
        HeldEscrowReserve {
            amount: 0,
            complete: false,
            repair_next_offset: 0,
        }
    }

    fn get_held_reserve_record(env: &Env, currency: &Address) -> Option<HeldEscrowReserve> {
        let key = Self::held_reserve_key(currency);
        let raw: Option<Val> = env.storage().persistent().get(&key);
        let raw = raw?;
        extend_persistent_ttl(env, &key);

        if let Ok(mut reserve) = HeldEscrowReserve::try_from_val(env, &raw) {
            if reserve.amount < 0 {
                reserve.amount = 0;
                reserve.complete = false;
                reserve.repair_next_offset = 0;
            }
            return Some(reserve);
        }

        i128::try_from_val(env, &raw)
            .ok()
            .map(|amount| HeldEscrowReserve {
                amount: amount.max(0),
                complete: false,
                repair_next_offset: 0,
            })
    }

    pub fn get_held_reserve(env: &Env, currency: &Address) -> i128 {
        Self::get_held_reserve_record(env, currency)
            .map(|reserve| reserve.amount)
            .unwrap_or(0)
    }

    pub fn is_held_reserve_complete(env: &Env, currency: &Address) -> bool {
        Self::get_held_reserve_record(env, currency)
            .map(|reserve| reserve.complete)
            .unwrap_or(false)
    }

    pub(crate) fn require_no_active_reserve_repair(
        env: &Env,
        currency: &Address,
    ) -> Result<(), QuickLendXError> {
        let repair_in_progress = Self::get_held_reserve_record(env, currency)
            .map(|reserve| !reserve.complete && reserve.repair_next_offset != 0)
            .unwrap_or(false);
        if repair_in_progress {
            return Err(QuickLendXError::InvalidStatus);
        }
        Ok(())
    }

    fn set_held_reserve_record(env: &Env, currency: &Address, reserve: &HeldEscrowReserve) {
        let key = Self::held_reserve_key(currency);
        env.storage().persistent().set(&key, reserve);
        extend_persistent_ttl(env, &key);
    }

    fn set_repair_snapshot(env: &Env, currency: &Address, ids: &Vec<BytesN<32>>) {
        let key = Self::held_reserve_repair_ids_key(currency);
        env.storage().persistent().set(&key, ids);
        extend_persistent_ttl(env, &key);
    }

    fn get_repair_snapshot(env: &Env, currency: &Address) -> Option<Vec<BytesN<32>>> {
        let key = Self::held_reserve_repair_ids_key(currency);
        let ids: Option<Vec<BytesN<32>>> = env.storage().persistent().get(&key);
        if ids.is_some() {
            extend_persistent_ttl(env, &key);
        }
        ids
    }

    fn clear_repair_snapshot(env: &Env, currency: &Address) {
        let key = Self::held_reserve_repair_ids_key(currency);
        env.storage().persistent().remove(&key);
    }

    fn held_reserve_after_increase(
        env: &Env,
        currency: &Address,
        amount: i128,
    ) -> Result<HeldEscrowReserve, QuickLendXError> {
        if amount <= 0 {
            return Err(QuickLendXError::InvalidAmount);
        }

        let mut reserve =
            Self::get_held_reserve_record(env, currency).unwrap_or_else(Self::empty_reserve);
        // Preserve `complete` as-is. Missing/legacy reserve state remains incomplete
        // and therefore emergency withdrawal stays fail-closed until repair completes.
        reserve.amount = reserve
            .amount
            .checked_add(amount)
            .ok_or(QuickLendXError::ArithmeticOverflow)?;
        Ok(reserve)
    }

    fn held_reserve_after_decrease(
        env: &Env,
        currency: &Address,
        amount: i128,
    ) -> Result<HeldEscrowReserve, QuickLendXError> {
        if amount <= 0 {
            return Err(QuickLendXError::InvalidAmount);
        }

        let mut reserve =
            Self::get_held_reserve_record(env, currency).unwrap_or_else(Self::empty_reserve);
        if reserve.amount < amount {
            // If the reserve undercounts the escrow amount, dirty the reserve instead of
            // blocking user release/refund. Emergency withdrawal remains fail-closed.
            reserve.amount = 0;
            reserve.complete = false;
            reserve.repair_next_offset = 0;
            return Ok(reserve);
        }

        reserve.amount -= amount;
        Ok(reserve)
    }

    fn mark_reserve_accounted(env: &Env, escrow_id: &BytesN<32>) {
        let key = Self::reserve_marker_key(escrow_id);
        env.storage().persistent().set(&key, &true);
        extend_persistent_ttl(env, &key);
    }

    fn is_reserve_accounted(env: &Env, escrow_id: &BytesN<32>) -> bool {
        let key = Self::reserve_marker_key(escrow_id);
        let accounted: bool = env.storage().persistent().get(&key).unwrap_or(false);
        if accounted {
            extend_persistent_ttl(env, &key);
        }
        accounted
    }

    fn clear_reserve_accounted(env: &Env, escrow_id: &BytesN<32>) {
        let key = Self::reserve_marker_key(escrow_id);
        env.storage().persistent().remove(&key);
    }

    pub fn repair_held_reserve_page(
        env: &Env,
        currency: &Address,
        offset: u32,
        limit: u32,
    ) -> Result<RebuildReport, QuickLendXError> {
        const MAX_REBUILD_PAGE: u32 = 100;

        if limit == 0 {
            return Err(QuickLendXError::InvalidAmount);
        }

        let capped = limit.min(MAX_REBUILD_PAGE);
        let ids = if offset == 0 {
            if InvoiceStorage::get_total_count(env) > MAX_REPAIR_SNAPSHOT_IDS {
                return Err(QuickLendXError::OperationNotAllowed);
            }
            let ids = InvoiceStorage::get_all_invoice_ids(env);
            if ids.len() as u64 > MAX_REPAIR_SNAPSHOT_IDS {
                return Err(QuickLendXError::OperationNotAllowed);
            }
            Self::set_repair_snapshot(env, currency, &ids);
            ids
        } else {
            Self::get_repair_snapshot(env, currency).ok_or(QuickLendXError::InvalidStatus)?
        };
        let total = ids.len();

        if offset > total {
            return Err(QuickLendXError::InvalidStatus);
        }

        let mut reserve = if offset == 0 {
            HeldEscrowReserve {
                amount: 0,
                complete: false,
                repair_next_offset: 0,
            }
        } else {
            let reserve = Self::get_held_reserve_record(env, currency)
                .ok_or(QuickLendXError::InvalidStatus)?;
            if reserve.complete || reserve.repair_next_offset != offset {
                return Err(QuickLendXError::InvalidStatus);
            }
            reserve
        };

        let start = offset;
        let end = start.saturating_add(capped).min(total);
        let mut reindexed = 0u32;
        let mut i = start;

        while i < end {
            if let Some(invoice_id) = ids.get(i) {
                if let Some(escrow) = Self::get_escrow_by_invoice(env, &invoice_id) {
                    if &escrow.currency == currency {
                        if escrow.status == EscrowStatus::Held {
                            if escrow.amount <= 0 {
                                return Err(QuickLendXError::InvalidAmount);
                            }

                            reserve.amount = reserve
                                .amount
                                .checked_add(escrow.amount)
                                .ok_or(QuickLendXError::ArithmeticOverflow)?;
                            Self::mark_reserve_accounted(env, &escrow.escrow_id);
                            reindexed = reindexed.saturating_add(1);
                        } else {
                            Self::clear_reserve_accounted(env, &escrow.escrow_id);
                        }
                    }
                }
            }
            i = i.saturating_add(1);
        }

        reserve.repair_next_offset = if end >= total { 0 } else { end };
        reserve.complete = end >= total;
        Self::set_held_reserve_record(env, currency, &reserve);
        if reserve.complete {
            Self::clear_repair_snapshot(env, currency);
        }

        Ok(RebuildReport {
            scanned: end.saturating_sub(start),
            reindexed,
            next_offset: reserve.repair_next_offset,
        })
    }

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

    /// Return the total locked escrow value for `currency` across all held escrows.
    ///
    /// Uses the pre-computed `HeldEscrowReserve` accumulator stored by
    /// `store_escrow`/`update_escrow`, so this is an O(1) read — no unbounded scan.
    /// Returns 0 if no escrows have been created for this currency.
    pub fn get_total_locked_escrow_for_currency(env: &Env, currency: &Address) -> i128 {
        Self::get_held_reserve(env, currency)
    }

    /// Return the total locked escrow value summed across up to `max_currencies`
    /// currencies found in the held-reserve index.
    ///
    /// `currencies` is a caller-supplied bounded list of currency addresses to
    /// aggregate. Callers should obtain the list from `get_whitelisted_currencies`
    /// or an off-chain index and paginate to stay within resource limits.
    pub fn get_total_locked_escrow_bounded(
        env: &Env,
        currencies: &Vec<Address>,
        max_currencies: u32,
    ) -> i128 {
        let mut total: i128 = 0;
        let limit = currencies.len().min(max_currencies);
        for i in 0..limit {
            let currency = currencies.get_unchecked(i);
            let held = Self::get_held_reserve(env, &currency);
            total = total.saturating_add(held);
        }
        total
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

    if EscrowStorage::get_escrow_by_invoice(env, invoice_id).is_some() {
        return Err(QuickLendXError::InvoiceAlreadyFunded);
    }

    let invoice = InvoiceStorage::get_invoice(env, invoice_id)
        .ok_or(QuickLendXError::StorageKeyNotFound)?;

    if invoice.business != *business {
        return Err(QuickLendXError::Unauthorized);
    }

    if invoice.currency != *currency {
        return Err(QuickLendXError::InvalidCurrency);
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
/// ## One-Escrow-Per-Invoice Guard
/// If an escrow record already exists for `invoice_id` (regardless of its status),
/// this function returns [`QuickLendXError::InvoiceAlreadyFunded`] **before** any
/// token transfer occurs. This is the innermost uniqueness guard; see also
/// `escrow::load_accept_bid_context` for the outer guard and `test_escrow_uniqueness.rs`
/// for the full attack-vector test suite.
///
/// # Returns
/// * `Ok(escrow_id)` - The new escrow ID
///
/// # Errors
/// * [`QuickLendXError::InvalidAmount`] - `amount` is zero or negative.
/// * [`QuickLendXError::InvalidStatus`] - reserve repair is active for this token.
/// * [`QuickLendXError::InvoiceAlreadyFunded`] - an escrow record already exists for this invoice.
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

    EscrowStorage::store_escrow(env, &escrow);
    EscrowStorage::set_held_reserve_record(env, currency, &next_held_reserve);
    EscrowStorage::mark_reserve_accounted(env, &escrow_id);
    crate::qlx_log!(env, "payment", "Escrow created successfully");
    emit_escrow_created(env, &escrow);
    crate::audit::log_escrow_created(
        env,
        invoice_id.clone(),
        investor.clone(),
        amount,
        escrow_id.clone(),
    );
    Ok(escrow_id)
}

/// Release escrow funds to business (contract -> business).
///
/// # Requirements
/// - Escrow must be in `Held` status.
/// - The invoice should ideally be in `Funded` or `Paid` status (enforced by caller in `lib.rs`).
///
/// # Security
/// - Idempotency: Once released, status becomes `Released`, preventing repeated transfers.
/// - Atomic: Funds are transferred before updating status in storage; if transfer fails,
///   the operation can be safely retried.
///
/// # Errors
/// * [`QuickLendXError::StorageKeyNotFound`] - no escrow record exists for this invoice.
/// * [`QuickLendXError::InvalidStatus`] - escrow is not in `Held` status (already released/refunded).
///   Also returned while reserve repair is active for this token.
/// * [`QuickLendXError::InsufficientFunds`] - contract balance is below the escrow amount
///   (should never happen in normal operation; indicates a critical invariant violation).
/// * [`QuickLendXError::TokenTransferFailed`] - the token contract panicked; escrow status is
///   **not** updated so the release can be safely retried.
pub fn release_escrow(env: &Env, invoice_id: &BytesN<32>) -> Result<(), QuickLendXError> {
    let mut escrow = EscrowStorage::get_escrow_by_invoice(env, invoice_id)
        .ok_or(QuickLendXError::StorageKeyNotFound)?;

    let invoice = InvoiceStorage::get_invoice(env, invoice_id)
        .ok_or(QuickLendXError::StorageKeyNotFound)?;

    if escrow.business != invoice.business {
        return Err(QuickLendXError::Unauthorized);
    }

    if escrow.status != EscrowStatus::Held {
        // Prevents repeated release (idempotency)
        return Err(QuickLendXError::InvalidStatus);
    }

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
        "Escrow released to business: amount={}",
        escrow.amount
    );

    Ok(())
}

/// Refund escrow funds to investor (contract -> investor). Escrow must be Held.
///
/// # Errors
/// * [`QuickLendXError::StorageKeyNotFound`] - no escrow record exists for this invoice.
/// * [`QuickLendXError::InvalidStatus`] - escrow is not in `Held` status.
///   Also returned while reserve repair is active for this token.
/// * [`QuickLendXError::InsufficientFunds`] - contract balance is below the escrow amount.
/// * [`QuickLendXError::TokenTransferFailed`] - the token contract panicked; escrow status is
///   **not** updated so the refund can be safely retried.
pub fn refund_escrow(env: &Env, invoice_id: &BytesN<32>) -> Result<(), QuickLendXError> {
    let mut escrow = EscrowStorage::get_escrow_by_invoice(env, invoice_id)
        .ok_or(QuickLendXError::StorageKeyNotFound)?;

    let invoice = InvoiceStorage::get_invoice(env, invoice_id)
        .ok_or(QuickLendXError::StorageKeyNotFound)?;

    if let Some(ref inv_investor) = invoice.investor {
        if escrow.investor != *inv_investor {
            return Err(QuickLendXError::Unauthorized);
        }
    }

    if escrow.status != EscrowStatus::Held {
        return Err(QuickLendXError::InvalidStatus);
    }

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

    // Refund funds from escrow (contract) back to investor
    let contract_address = env.current_contract_address();
    transfer_funds(
        env,
        &escrow.currency,
        &contract_address,
        &escrow.investor,
        escrow.amount,
    )?;

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
        escrow.amount,
    );
    crate::audit::log_escrow_refunded(
        env,
        invoice_id.clone(),
        escrow.investor.clone(),
        escrow.amount,
        escrow.escrow_id.clone(),
    );
    crate::qlx_log!(
        env,
        "payment",
        "Escrow refunded to investor: amount={}",
        escrow.amount
    );

    Ok(())
}

/// Transfer token funds from one address to another. Uses allowance when `from` is not the contract.
///
/// # Errors
/// * [`QuickLendXError::InvalidAmount`] - `amount` is zero or negative.
/// * [`QuickLendXError::InsufficientFunds`] - `from` balance is below `amount`.
/// * [`QuickLendXError::OperationNotAllowed`] - allowance granted to the contract is below `amount`.
/// * [`QuickLendXError::TokenTransferFailed`] - the underlying Stellar token call panicked or
///   returned an error. No funds moved when this error is returned.
///
/// # Security
/// - Balance and allowance are checked **before** the token call so that the contract
///   never enters a partial-transfer state.
/// - When `from == to` the function is a no-op (returns `Ok(())`).

pub fn transfer_funds(
    env: &Env,
    currency: &Address,
    from: &Address,
    to: &Address,
    amount: i128,
) -> Result<(), QuickLendXError> {
    // Reject amounts below the minimum transfer threshold (dust prevention) or exceeding upper bound
    if amount < MIN_TRANSFER || amount > crate::protocol_limits::MAX_INVOICE_AMOUNT {
        return Err(QuickLendXError::InvalidAmount);
    }

    if from == to {
        return Err(QuickLendXError::SelfTransfer);
    }

    let token_client = token::Client::new(env, currency);
    let contract_address = env.current_contract_address();

    // Ensure sufficient balance exists before attempting transfer.
    let available_balance = token_client.balance(from);
    if available_balance < amount {
        return Err(QuickLendXError::InsufficientFunds);
    }

    if from == &contract_address {
        token_client.transfer(from, to, &amount);
        return Ok(());
    }

    let allowance = token_client.allowance(from, &contract_address);
    if allowance < amount {
        return Err(QuickLendXError::OperationNotAllowed);
    }

    token_client.transfer_from(&contract_address, from, to, &amount);
    Ok(())
}

#[cfg(test)]
mod payments_tests {
    use super::*;
    use soroban_sdk::{testutils::Address as _, token, Address, BytesN, Env};

    fn contract_env() -> (Env, Address) {
        use crate::QuickLendXContract;
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(QuickLendXContract, ());
        (env, contract_id)
    }

    fn mint_and_approve(
        env: &Env,
        contract_id: &Address,
        token_admin: &Address,
        holder: &Address,
        balance: i128,
        allowance: i128,
    ) -> Address {
        let currency = env
            .register_stellar_asset_contract_v2(token_admin.clone())
            .address();
        let sac = token::StellarAssetClient::new(env, &currency);
        let tok = token::Client::new(env, &currency);
        sac.mint(holder, &balance);
        let expiry = env.ledger().sequence() + 10_000;
        tok.approve(holder, contract_id, &allowance, &expiry);
        currency
    }

    // -----------------------------------------------------------------------
    // Zero-amount boundary
    // -----------------------------------------------------------------------

    /// Passing `amount = 0` to `create_escrow` must return `InvalidAmount`
    /// and must not create any escrow record or transfer any funds.
    #[test]
    fn test_create_escrow_zero_amount_returns_invalid_amount() {
        let (env, contract_id) = contract_env();
        let investor = Address::generate(&env);
        let business = Address::generate(&env);
        let invoice_id = BytesN::from_array(&env, &[0u8; 32]);
        let token_admin = Address::generate(&env);
        let currency = env
            .register_stellar_asset_contract_v2(token_admin)
            .address();

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

        let result = env.as_contract(&contract_id, || {
            create_escrow(
                &env,
                &invoice_id,
                &investor,
                &Address::generate(&env),
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
    fn test_create_escrow_amount_exceeds_balance_returns_insufficient_funds() {
        let (env, contract_id) = contract_env();
        let investor = Address::generate(&env);
        let token_admin = Address::generate(&env);
        let currency = mint_and_approve(&env, &contract_id, &token_admin, &investor, 5_000, 10_000);

        let invoice_id = BytesN::from_array(&env, &[3u8; 32]);
        let tok = token::Client::new(&env, &currency);

        let investor_bal = tok.balance(&investor);
        let contract_bal = tok.balance(&contract_id);

        let result = env.as_contract(&contract_id, || {
            create_escrow(
                &env,
                &invoice_id,
                &investor,
                &Address::generate(&env),
                5_001,
                &currency,
            )
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

        let result = env.as_contract(&contract_id, || {
            create_escrow(
                &env,
                &invoice_id,
                &investor,
                &Address::generate(&env),
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
                &Address::generate(&env),
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

        // First escrow
        let r1 = env.as_contract(&contract_id, || {
            create_escrow(
                &env,
                &invoice_id,
                &investor1,
                &Address::generate(&env),
                10_000,
                &currency,
            )
        });
        assert!(r1.is_ok(), "first escrow must succeed");

        // Second attempt (different investor) must fail
        let r2 = env.as_contract(&contract_id, || {
            create_escrow(
                &env,
                &invoice_id,
                &investor2,
                &Address::generate(&env),
                5_000,
                &currency,
            )
        });
        assert_eq!(r2, Err(QuickLendXError::InvoiceAlreadyFunded));
    }
}
