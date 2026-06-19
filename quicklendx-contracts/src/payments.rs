//! Payment and escrow operations: create escrow, release, refund, and token transfers.
//!
//! Public release/refund entry points are wrapped with a reentrancy guard in lib.rs.

use crate::errors::QuickLendXError;
use crate::events::emit_escrow_created;
use crate::storage::{extend_persistent_ttl, InvoiceStorage};
use crate::types::RebuildReport;
use soroban_sdk::token;
use soroban_sdk::{contracttype, symbol_short, Address, BytesN, Env, Symbol};

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

pub struct EscrowStorage;

const HELD_ESCROW_RESERVE_KEY: Symbol = symbol_short!("esc_res");
const ESCROW_RESERVE_MARKER_KEY: Symbol = symbol_short!("esc_acc");

impl EscrowStorage {
    fn held_reserve_key(currency: &Address) -> (Symbol, Address) {
        (HELD_ESCROW_RESERVE_KEY.clone(), currency.clone())
    }

    fn reserve_marker_key(escrow_id: &BytesN<32>) -> (Symbol, BytesN<32>) {
        (ESCROW_RESERVE_MARKER_KEY.clone(), escrow_id.clone())
    }

    pub fn get_held_reserve(env: &Env, currency: &Address) -> i128 {
        let key = Self::held_reserve_key(currency);
        let reserve: i128 = env.storage().persistent().get(&key).unwrap_or(0);
        if reserve > 0 {
            extend_persistent_ttl(env, &key);
        }
        reserve
    }

    fn set_held_reserve(env: &Env, currency: &Address, amount: i128) {
        let key = Self::held_reserve_key(currency);
        if amount > 0 {
            env.storage().persistent().set(&key, &amount);
            extend_persistent_ttl(env, &key);
        } else {
            env.storage().persistent().remove(&key);
        }
    }

    fn held_reserve_after_increase(
        env: &Env,
        currency: &Address,
        amount: i128,
    ) -> Result<i128, QuickLendXError> {
        if amount <= 0 {
            return Err(QuickLendXError::InvalidAmount);
        }

        Self::get_held_reserve(env, currency)
            .checked_add(amount)
            .ok_or(QuickLendXError::ArithmeticOverflow)
    }

    fn held_reserve_after_decrease(
        env: &Env,
        currency: &Address,
        amount: i128,
    ) -> Result<i128, QuickLendXError> {
        if amount <= 0 {
            return Err(QuickLendXError::InvalidAmount);
        }

        let current = Self::get_held_reserve(env, currency);
        current
            .checked_sub(amount)
            .ok_or(QuickLendXError::ArithmeticOverflow)
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
        let capped = if limit > MAX_REBUILD_PAGE { MAX_REBUILD_PAGE } else { limit };

        let all_ids = InvoiceStorage::get_all_invoice_ids(env);
        let total = all_ids.len() as u32;

        if capped == 0 || offset > total {
            return Ok(RebuildReport {
                scanned: 0,
                reindexed: 0,
                next_offset: offset.min(total),
            });
        }

        let start = offset;
        let end = start.saturating_add(capped).min(total);
        let mut reserve = Self::get_held_reserve(env, currency);
        let mut reindexed = 0u32;
        let mut i = start;

        while i < end {
            if let Some(invoice_id) = all_ids.get(i) {
                if let Some(escrow) = Self::get_escrow_by_invoice(env, &invoice_id) {
                    if escrow.status == EscrowStatus::Held
                        && &escrow.currency == currency
                        && !Self::is_reserve_accounted(env, &escrow.escrow_id)
                    {
                        if escrow.amount <= 0 {
                            return Err(QuickLendXError::InvalidAmount);
                        }

                        reserve = reserve
                            .checked_add(escrow.amount)
                            .ok_or(QuickLendXError::ArithmeticOverflow)?;
                        Self::mark_reserve_accounted(env, &escrow.escrow_id);
                        reindexed = reindexed.saturating_add(1);
                    }
                }
            }
            i = i.saturating_add(1);
        }

        if reindexed > 0 {
            Self::set_held_reserve(env, currency, reserve);
        }

        Ok(RebuildReport {
            scanned: end.saturating_sub(start),
            reindexed,
            next_offset: end,
        })
    }

    pub fn store_escrow(env: &Env, escrow: &Escrow) {
        env.storage().persistent().set(&escrow.escrow_id, escrow);
        extend_persistent_ttl(env, &escrow.escrow_id);
        // Also store by invoice_id for easy lookup
        let invoice_key = (symbol_short!("escrow"), &escrow.invoice_id);
        env.storage().persistent().set(&invoice_key, &escrow.escrow_id);
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
        if env.storage().persistent().get::<_, BytesN<32>>(&invoice_key).is_some() {
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
    if amount <= 0 {
        return Err(QuickLendXError::InvalidAmount);
    }

    if EscrowStorage::get_escrow_by_invoice(env, invoice_id).is_some() {
        return Err(QuickLendXError::InvoiceAlreadyFunded);
    }

    let next_held_reserve = EscrowStorage::held_reserve_after_increase(env, currency, amount)?;

    crate::qlx_log!(env, "payment", "Creating escrow: amount={}", amount);

    // Move funds from investor into contract-controlled escrow
    let contract_address = env.current_contract_address();
    transfer_funds(env, currency, investor, &contract_address, amount)?;

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
    EscrowStorage::set_held_reserve(env, currency, next_held_reserve);
    EscrowStorage::mark_reserve_accounted(env, &escrow_id);
    crate::qlx_log!(env, "payment", "Escrow created successfully");
    emit_escrow_created(env, &escrow);
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
/// * [`QuickLendXError::InsufficientFunds`] - contract balance is below the escrow amount
///   (should never happen in normal operation; indicates a critical invariant violation).
/// * [`QuickLendXError::TokenTransferFailed`] - the token contract panicked; escrow status is
///   **not** updated so the release can be safely retried.
pub fn release_escrow(env: &Env, invoice_id: &BytesN<32>) -> Result<(), QuickLendXError> {
    let mut escrow = EscrowStorage::get_escrow_by_invoice(env, invoice_id)
        .ok_or(QuickLendXError::StorageKeyNotFound)?;

    if escrow.status != EscrowStatus::Held {
        // Prevents repeated release (idempotency)
        return Err(QuickLendXError::InvalidStatus);
    }

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
        EscrowStorage::set_held_reserve(env, &escrow.currency, next_held_reserve);
        EscrowStorage::clear_reserve_accounted(env, &escrow.escrow_id);
    }
    escrow.status = EscrowStatus::Released;
    EscrowStorage::update_escrow(env, &escrow);
    crate::qlx_log!(env, "payment", "Escrow released to business: amount={}", escrow.amount);

    Ok(())
}

/// Refund escrow funds to investor (contract -> investor). Escrow must be Held.
///
/// # Errors
/// * [`QuickLendXError::StorageKeyNotFound`] - no escrow record exists for this invoice.
/// * [`QuickLendXError::InvalidStatus`] - escrow is not in `Held` status.
/// * [`QuickLendXError::InsufficientFunds`] - contract balance is below the escrow amount.
/// * [`QuickLendXError::TokenTransferFailed`] - the token contract panicked; escrow status is
///   **not** updated so the refund can be safely retried.
pub fn refund_escrow(env: &Env, invoice_id: &BytesN<32>) -> Result<(), QuickLendXError> {
    let mut escrow = EscrowStorage::get_escrow_by_invoice(env, invoice_id)
        .ok_or(QuickLendXError::StorageKeyNotFound)?;

    if escrow.status != EscrowStatus::Held {
        return Err(QuickLendXError::InvalidStatus);
    }

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
        EscrowStorage::set_held_reserve(env, &escrow.currency, next_held_reserve);
        EscrowStorage::clear_reserve_accounted(env, &escrow.escrow_id);
    }
    escrow.status = EscrowStatus::Refunded;
    EscrowStorage::update_escrow(env, &escrow);
    crate::qlx_log!(env, "payment", "Escrow refunded to investor: amount={}", escrow.amount);

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
    if amount <= 0 {
        return Err(QuickLendXError::InvalidAmount);
    }

    if from == to {
        return Ok(());
    }

    let token_client = token::Client::new(env, currency);
    let contract_address = env.current_contract_address();

    // Ensure sufficient balance exists before attempting transfer
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
