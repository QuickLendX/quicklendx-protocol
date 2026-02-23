//! Emergency withdraw / recovery for stuck funds.
//!
//! Admin-only, timelocked recovery of tokens sent to the contract by mistake or
//! stuck due to bugs. Use only as a last resort; see docs/contracts/emergency-recovery.md.

use crate::admin::AdminStorage;
use crate::errors::QuickLendXError;
use crate::payments::transfer_funds;
use soroban_sdk::{contracttype, symbol_short, Address, Env};

/// Default timelock: 24 hours. Withdrawal can only be executed after this delay.
pub const DEFAULT_EMERGENCY_TIMELOCK_SECS: u64 = 24 * 60 * 60;

const PENDING_WITHDRAWAL_KEY: soroban_sdk::Symbol = symbol_short!("emg_wd");

/// A pending emergency withdrawal (single slot; new initiate overwrites or clears after execute).
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PendingEmergencyWithdrawal {
    pub token: Address,
    pub amount: i128,
    pub target: Address,
    pub unlock_at: u64,
    pub initiated_at: u64,
    pub initiated_by: Address,
}

pub struct EmergencyWithdraw;

impl EmergencyWithdraw {
    /// Initiate an emergency withdrawal. Only admin. Call `execute_emergency_withdraw` after timelock.
    ///
    /// # Errors
    /// * `NotAdmin` if caller is not admin
    /// * `EmergencyWithdrawZeroAmount` if amount <= 0
    pub fn initiate(
        env: &Env,
        admin: &Address,
        token: Address,
        amount: i128,
        target: Address,
    ) -> Result<(), QuickLendXError> {
        admin.require_auth();
        AdminStorage::require_admin(env, admin)?;

        if amount <= 0 {
            return Err(QuickLendXError::InvalidAmount);
        }

        let now = env.ledger().timestamp();
        let unlock_at = now.saturating_add(DEFAULT_EMERGENCY_TIMELOCK_SECS);

        let pending = PendingEmergencyWithdrawal {
            token: token.clone(),
            amount,
            target: target.clone(),
            unlock_at,
            initiated_at: now,
            initiated_by: admin.clone(),
        };

        env.storage()
            .instance()
            .set(&PENDING_WITHDRAWAL_KEY, &pending);
        env.events().publish(
            (symbol_short!("emg_init"),),
            (token, amount, target, unlock_at, admin.clone()),
        );

        Ok(())
    }

    /// Execute the pending emergency withdrawal. Only after timelock has elapsed. Only admin.
    ///
    /// Transfers `amount` of `token` from the contract to the stored `target`.
    ///
    /// # Errors
    /// * `NotAdmin` if caller is not admin
    /// * `EmergencyWithdrawNotFound` if no pending withdrawal
    /// * `EmergencyWithdrawTimelockNotElapsed` if unlock_at has not passed
    /// * Transfer errors (e.g. `InsufficientFunds`) if contract balance is insufficient
    pub fn execute(env: &Env, admin: &Address) -> Result<(), QuickLendXError> {
        admin.require_auth();
        AdminStorage::require_admin(env, admin)?;

        let pending: PendingEmergencyWithdrawal = env
            .storage()
            .instance()
            .get(&PENDING_WITHDRAWAL_KEY)
            .ok_or(QuickLendXError::StorageKeyNotFound)?;

        let now = env.ledger().timestamp();
        if now < pending.unlock_at {
            return Err(QuickLendXError::OperationNotAllowed);
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
        env.events().publish(
            (symbol_short!("emg_exec"),),
            (
                pending.token.clone(),
                pending.amount,
                pending.target.clone(),
                admin.clone(),
            ),
        );

        Ok(())
    }

    /// Get the current pending emergency withdrawal, if any.
    pub fn get_pending(env: &Env) -> Option<PendingEmergencyWithdrawal> {
        env.storage().instance().get(&PENDING_WITHDRAWAL_KEY)
    }

    /// Cancel a pending emergency withdrawal (admin only).
    /// Useful if initiate was triggered by mistake or a threat has passed.
    ///
    /// # Errors
    /// * `NotAdmin` if caller is not admin
    /// * `StorageKeyNotFound` if no pending withdrawal exists
    pub fn cancel(env: &Env, admin: &Address) -> Result<(), QuickLendXError> {
        admin.require_auth();
        AdminStorage::require_admin(env, admin)?;

        let pending: PendingEmergencyWithdrawal = env
            .storage()
            .instance()
            .get(&PENDING_WITHDRAWAL_KEY)
            .ok_or(QuickLendXError::StorageKeyNotFound)?;

        env.storage().instance().remove(&PENDING_WITHDRAWAL_KEY);
        env.events().publish(
            (symbol_short!("emg_cncl"),),
            (
                pending.token.clone(),
                pending.amount,
                pending.target.clone(),
                admin.clone(),
            ),
        );

        Ok(())
    }
}
