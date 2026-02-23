//! Reentrancy guard for payment and escrow flows.
//!
//! Prevents intermediate re-entry into payment/escrow operations that could
//! lead to double-spend or state corruption. Uses a single process-wide lock
//! in instance storage.

use crate::errors::QuickLendXError;
use soroban_sdk::{symbol_short, Env};

/// Runs a closure with the payment/escrow reentrancy guard held.
///
/// At entry, if the lock is already set, returns `Err(OperationNotAllowed)`.
/// Otherwise sets the lock, runs `f`, then clears the lock on success or failure.
///
/// # Errors
/// * `QuickLendXError::OperationNotAllowed` if called while another payment/escrow
///   operation is in progress (re-entrant call).
pub fn with_payment_guard<F, R>(env: &Env, f: F) -> Result<R, QuickLendXError>
where
    F: FnOnce() -> Result<R, QuickLendXError>,
{
    let key = symbol_short!("pay_lock");
    if env.storage().instance().get(&key).unwrap_or(false) {
        return Err(QuickLendXError::OperationNotAllowed);
    }
    env.storage().instance().set(&key, &true);
    let result = f();
    env.storage().instance().set(&key, &false);
    result
}
