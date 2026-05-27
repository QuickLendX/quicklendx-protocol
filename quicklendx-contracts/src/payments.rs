//! Payment and escrow operations: create escrow, release, refund, and token transfers.
//!
//! Public release/refund entry points are wrapped with a reentrancy guard in lib.rs.

use crate::errors::QuickLendXError;
use crate::events::emit_escrow_created;
use soroban_sdk::token;
use soroban_sdk::{contracttype, symbol_short, Address, BytesN, Env};

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

impl EscrowStorage {
    pub fn store_escrow(env: &Env, escrow: &Escrow) {
        env.storage().instance().set(&escrow.escrow_id, escrow);
        // Also store by invoice_id for easy lookup
        env.storage().instance().set(
            &(symbol_short!("escrow"), &escrow.invoice_id),
            &escrow.escrow_id,
        );
    }

    pub fn get_escrow(env: &Env, escrow_id: &BytesN<32>) -> Option<Escrow> {
        env.storage().instance().get(escrow_id)
    }

    pub fn get_escrow_by_invoice(env: &Env, invoice_id: &BytesN<32>) -> Option<Escrow> {
        let escrow_id: Option<BytesN<32>> = env
            .storage()
            .instance()
            .get(&(symbol_short!("escrow"), invoice_id));
        if let Some(id) = escrow_id {
            Self::get_escrow(env, &id)
        } else {
            None
        }
    }

    pub fn update_escrow(env: &Env, escrow: &Escrow) {
        env.storage().instance().set(&escrow.escrow_id, escrow);
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
    escrow.status = EscrowStatus::Released;
    EscrowStorage::update_escrow(env, &escrow);

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
    escrow.status = EscrowStatus::Refunded;
    EscrowStorage::update_escrow(env, &escrow);

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
