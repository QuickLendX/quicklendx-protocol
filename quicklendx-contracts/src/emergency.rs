//! Emergency withdraw / recovery for stuck funds.
//!
//! Admin-only, timelocked recovery of tokens sent to the contract by mistake or
//! stuck due to bugs. Use only as a last resort; see docs/contracts/emergency-recovery.md.
//!
//! # Security hardening
//! - Timelock integrity: Withdrawal cannot execute until unlock_at has passed.
//! - Expiration: Withdrawal expires if not executed within EXPIRATION_WINDOW after unlock_at.
//! - Cancellation guarantees: Cancelled withdrawals are invalidated and cannot be re-executed.
//! - Nonce tracking: Each initiation increments a nonce to prevent stale request reuse.

use crate::admin::AdminStorage;
use crate::errors::QuickLendXError;
use crate::payments::transfer_funds;
use soroban_sdk::{contracttype, symbol_short, Address, Env};

/// Default timelock: 24 hours. Withdrawal can only be executed after this delay.
pub const DEFAULT_EMERGENCY_TIMELOCK_SECS: u64 = 24 * 60 * 60;

/// Default expiration window: 7 days after unlock_at. Withdrawal becomes invalid if not executed.
pub const DEFAULT_EMERGENCY_EXPIRATION_SECS: u64 = 7 * 24 * 60 * 60;

/// Minimum timelock allowed (1 hour) to prevent overly aggressive timelocks.
pub const MIN_EMERGENCY_TIMELOCK_SECS: u64 = 60 * 60;

/// Maximum timelock allowed (30 days) to prevent overly long timelocks.
pub const MAX_EMERGENCY_TIMELOCK_SECS: u64 = 30 * 24 * 60 * 60;

const PENDING_WITHDRAWAL_KEY: soroban_sdk::Symbol = symbol_short!("emg_wd");
const CANCELLED_NONCE_KEY: soroban_sdk::Symbol = symbol_short!("emg_cnl");
const GLOBAL_NONCE_KEY: soroban_sdk::Symbol = symbol_short!("emg_nce");

/// A pending emergency withdrawal (single slot; new initiate overwrites or clears after execute).
#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(test, derive(Debug))]
pub struct PendingEmergencyWithdrawal {
    pub token: Address,
    pub amount: i128,
    pub target: Address,
    pub unlock_at: u64,
    pub expires_at: u64,
    pub initiated_at: u64,
    pub initiated_by: Address,
    pub nonce: u64,
    pub cancelled: bool,
    pub cancelled_at: u64,
}

pub struct EmergencyWithdraw;

impl EmergencyWithdraw {
    /// Get the current global nonce for emergency withdrawals.
    ///
    /// # Returns
    /// * `u64` - The current nonce value (starts at 1)
    pub fn get_nonce(env: &Env) -> u64 {
        env.storage().instance().get(&GLOBAL_NONCE_KEY).unwrap_or(1)
    }

    fn increment_nonce(env: &Env) -> u64 {
        let nonce = Self::get_nonce(env);
        let new_nonce = nonce.saturating_add(1);
        env.storage().instance().set(&GLOBAL_NONCE_KEY, &new_nonce);
        new_nonce
    }

    /// Check if a nonce has been cancelled.
    ///
    /// # Returns
    /// * `true` if the nonce was cancelled
    /// * `false` if the nonce was not cancelled or does not exist
    pub fn is_nonce_cancelled(env: &Env, nonce: u64) -> bool {
        let key = (CANCELLED_NONCE_KEY.clone(), nonce);
        env.storage()
            .instance()
            .get::<_, bool>(&key)
            .unwrap_or(false)
    }

    /// Mark a nonce as cancelled.
    fn mark_nonce_cancelled(env: &Env, nonce: u64) {
        let key = (CANCELLED_NONCE_KEY.clone(), nonce);
        env.storage().instance().set(&key, &true);
    }

    /// Initiate an emergency withdrawal. Only admin. Call `execute_emergency_withdraw` after timelock.
    ///
    /// # Arguments
    /// * `token` - The token contract address to withdraw
    /// * `amount` - The amount to withdraw (must be positive)
    /// * `target` - The address to receive the withdrawn funds
    ///
    /// # Errors
    /// * `NotAdmin` if caller is not admin
    /// * `InvalidAmount` if amount <= 0
    /// * `InvalidAddress` if token or target is the contract address
    pub fn initiate(
        env: &Env,
        admin: &Address,
        token: Address,
        amount: i128,
        target: Address,
    ) -> Result<(), QuickLendXError> {
        AdminStorage::require_admin_auth(env, admin)?;

        if amount <= 0 {
            return Err(QuickLendXError::InvalidAmount);
        }

        let contract = env.current_contract_address();
        if token == contract {
            return Err(QuickLendXError::InvalidAddress);
        }
        if target == contract {
            return Err(QuickLendXError::InvalidAddress);
        }

        let now = env.ledger().timestamp();
        let unlock_at = now.saturating_add(DEFAULT_EMERGENCY_TIMELOCK_SECS);
        let expires_at = unlock_at.saturating_add(DEFAULT_EMERGENCY_EXPIRATION_SECS);
        let nonce = Self::increment_nonce(env);

        let pending = PendingEmergencyWithdrawal {
            token: token.clone(),
            amount,
            target: target.clone(),
            unlock_at,
            expires_at,
            initiated_at: now,
            initiated_by: admin.clone(),
            nonce,
            cancelled: false,
            cancelled_at: 0,
        };

        env.storage()
            .instance()
            .set(&PENDING_WITHDRAWAL_KEY, &pending);
        crate::events::emit_emergency_withdrawal_initiated(
            env,
            token,
            amount,
            target,
            unlock_at,
            admin.clone(),
        );

        Ok(())
    }

    /// @notice Execute a queued emergency withdrawal after the timelock expires.
    /// @dev This path is also pause-exempt; the timelock remains the primary safety control.
    ///
    /// Execute the pending emergency withdrawal. Only after timelock has elapsed. Only admin.
    ///
    /// Transfers `amount` of `token` from the contract to the stored `target`.
    ///
    /// # Security checks
    /// - Verifies timelock has elapsed (unlock_at <= now)
    /// - Verifies withdrawal has not expired (expires_at > now)
    /// - Verifies withdrawal has not been cancelled
    ///
    /// # Errors
    /// * `NotAdmin` if caller is not admin
    /// * `EmergencyWithdrawNotFound` if no pending withdrawal
    /// * `EmergencyWithdrawTimelockNotElapsed` if unlock_at has not passed
    /// * `EmergencyWithdrawExpired` if expires_at has passed
    /// * `EmergencyWithdrawCancelled` if withdrawal was cancelled
    /// * Transfer errors (e.g. `InsufficientFunds`) if contract balance is insufficient
    pub fn execute(env: &Env, admin: &Address) -> Result<(), QuickLendXError> {
        AdminStorage::require_admin_auth(env, admin)?;

        let pending: PendingEmergencyWithdrawal = env
            .storage()
            .instance()
            .get(&PENDING_WITHDRAWAL_KEY)
            .ok_or(QuickLendXError::EmergencyWithdrawNotFound)?;

        let now = env.ledger().timestamp();

        if pending.cancelled {
            return Err(QuickLendXError::EmergencyWithdrawCancelled);
        }

        if now < pending.unlock_at {
            return Err(QuickLendXError::EmergencyWithdrawTimelockNotElapsed);
        }

        if now >= pending.expires_at {
            return Err(QuickLendXError::EmergencyWithdrawExpired);
        }

        let contract = env.current_contract_address();
        transfer_funds(
            env,
            &pending.token,
            &contract,
            &pending.target,
            pending.amount,
        )?;

        env.storage().instance().remove(&PENDING_WITHDRAWAL_KEY);
        crate::events::emit_emergency_withdrawal_executed(
            env,
            pending.token.clone(),
            pending.amount,
            pending.target.clone(),
            admin.clone(),
        );

        Ok(())
    }

    /// @notice Return the current pending emergency withdrawal, if any.
    pub fn get_pending(env: &Env) -> Option<PendingEmergencyWithdrawal> {
        env.storage().instance().get(&PENDING_WITHDRAWAL_KEY)
    }

    /// @notice Cancel a pending emergency withdrawal.
    /// @dev Cancellation remains available while paused to let admins abort a queued recovery action.
    ///
    /// Cancel a pending emergency withdrawal (admin only).
    ///
    /// Marks the current pending withdrawal as cancelled and records the nonce
    /// to prevent any future execution attempts with the same nonce.
    /// Useful if initiate was triggered by mistake or a threat has passed.
    ///
    /// # Security guarantees
    /// - Cancelled withdrawals cannot be re-executed even if the timelock has passed
    /// - The cancellation is permanent and stored per-nonce
    /// - Only the current pending withdrawal can be cancelled
    ///
    /// # Errors
    /// * `NotAdmin` if caller is not admin
    /// * `EmergencyWithdrawNotFound` if no pending withdrawal exists
    /// * `EmergencyWithdrawCancelled` if withdrawal is already cancelled
    pub fn cancel(env: &Env, admin: &Address) -> Result<(), QuickLendXError> {
        AdminStorage::require_admin_auth(env, admin)?;

        let mut pending: PendingEmergencyWithdrawal = env
            .storage()
            .instance()
            .get(&PENDING_WITHDRAWAL_KEY)
            .ok_or(QuickLendXError::EmergencyWithdrawNotFound)?;

        if pending.cancelled {
            return Err(QuickLendXError::EmergencyWithdrawCancelled);
        }

        let now = env.ledger().timestamp();
        pending.cancelled = true;
        pending.cancelled_at = now;

        Self::mark_nonce_cancelled(env, pending.nonce);

        env.storage()
            .instance()
            .set(&PENDING_WITHDRAWAL_KEY, &pending);

        env.storage().instance().remove(&PENDING_WITHDRAWAL_KEY);
        crate::events::emit_emergency_withdrawal_cancelled(
            env,
            pending.token.clone(),
            pending.amount,
            pending.target.clone(),
            admin.clone(),
        );

        Ok(())
    }

    /// Check if a pending withdrawal is currently valid for execution.
    ///
    /// A withdrawal is valid if:
    /// - It exists (not None)
    /// - It has not been cancelled
    /// - The timelock has elapsed (unlock_at <= now)
    /// - It has not expired (expires_at > now)
    ///
    /// # Returns
    /// * `Some(true)` if the withdrawal can be executed
    /// * `Some(false)` if the withdrawal exists but cannot be executed yet
    /// * `None` if no pending withdrawal exists
    pub fn can_execute(env: &Env) -> Option<bool> {
        let pending = Self::get_pending(env)?;
        let now = env.ledger().timestamp();

        Some(!pending.cancelled && now >= pending.unlock_at && now < pending.expires_at)
    }

    /// Get time remaining until the withdrawal can be executed.
    ///
    /// # Returns
    /// * `Some(remaining_secs)` - Seconds until timelock elapses (0 if already elapsed)
    /// * `None` if no pending withdrawal exists
    pub fn time_until_unlock(env: &Env) -> Option<u64> {
        let pending = Self::get_pending(env)?;
        let now = env.ledger().timestamp();

        if now >= pending.unlock_at {
            Some(0)
        } else {
            Some(pending.unlock_at.saturating_sub(now))
        }
    }

    /// Get time remaining until the withdrawal expires (becomes invalid).
    ///
    /// # Returns
    /// * `Some(remaining_secs)` - Seconds until expiration (0 if already expired)
    /// * `None` if no pending withdrawal exists
    pub fn time_until_expiration(env: &Env) -> Option<u64> {
        let pending = Self::get_pending(env)?;
        let now = env.ledger().timestamp();

        if now >= pending.expires_at {
            Some(0)
        } else {
            Some(pending.expires_at.saturating_sub(now))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
}
