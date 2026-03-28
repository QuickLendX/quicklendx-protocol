//! Reentrancy guard for payment and escrow flows.
//!
//! Soroban does not currently support EVM-style fallback hooks during token
//! transfers, but nested contract execution is still worth defending against as
//! a protocol invariant. This module owns the single lock used by the guarded
//! payment entry points and exposes a small helper for regression tests to
//! verify lock cleanup after rejected nested calls.

use crate::errors::QuickLendXError;
use soroban_sdk::{symbol_short, Env, Symbol};

fn payment_guard_key() -> Symbol {
    symbol_short!("pay_lock")
}

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
    let key = payment_guard_key();
    if env.storage().instance().get(&key).unwrap_or(false) {
        return Err(QuickLendXError::OperationNotAllowed);
    }
    env.storage().instance().set(&key, &true);
    let result = f();
    env.storage().instance().set(&key, &false);
    result
}

/// Returns whether the payment-path guard is currently locked.
pub(crate) fn is_payment_guard_locked(env: &Env) -> bool {
    env.storage()
        .instance()
        .get(&payment_guard_key())
        .unwrap_or(false)
}
