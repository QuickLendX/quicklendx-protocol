//! Token transfers to, from and within the contract.

use crate::errors::QuickLendXError;
use soroban_sdk::{token, Address, Env};

/// Minimum transfer amount to prevent dust transfers.
/// Matches the test-mode MIN_TRANSFER from protocol_limits.rs.
#[cfg(not(test))]
pub const MIN_TRANSFER: i128 = 1_000_000; // 1 token (6 decimals)
#[cfg(test)]
pub const MIN_TRANSFER: i128 = 10;

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

/// Internal transfer used by exact repayment allocation where the amount is the
/// precise, already-validated output of [`allocate_repayment`] (and may legitimately
/// fall below `MIN_TRANSFER`).
///
/// Same safety guarantees as [`transfer_funds`] (balance/allowance checked *before*
/// the token call, `from == to` rejected) but without the dust floor, so that the
/// investor return, platform fee, and principal legs of a repayment can be moved
/// exactly without truncation. Still enforces the upper `MAX_INVOICE_AMOUNT` bound.
pub(crate) fn transfer_funds_exact(
    env: &Env,
    currency: &Address,
    from: &Address,
    to: &Address,
    amount: i128,
) -> Result<(), QuickLendXError> {
    if amount <= 0 {
        return Err(QuickLendXError::InvalidAmount);
    }

    if amount > crate::protocol_limits::MAX_INVOICE_AMOUNT {
        return Err(QuickLendXError::InvalidAmount);
    }

    if from == to {
        return Err(QuickLendXError::SelfTransfer);
    }

    let token_client = token::Client::new(env, currency);
    let contract_address = env.current_contract_address();

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
