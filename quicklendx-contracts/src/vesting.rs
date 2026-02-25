//! Token vesting module with time-locked release schedules.
//!
//! Supports admin-created vesting schedules that lock protocol tokens or rewards
//! in the contract and release them linearly over time after an optional cliff.
//! Beneficiaries can claim vested tokens as they unlock.

use soroban_sdk::{contracttype, symbol_short, Address, Env, Symbol};

use crate::admin::AdminStorage;
use crate::errors::QuickLendXError;
use crate::payments::transfer_funds;

const VESTING_COUNTER_KEY: Symbol = symbol_short!("vest_cnt");
const VESTING_KEY: Symbol = symbol_short!("vest");

/// Vesting schedule stored on-chain.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VestingSchedule {
    pub id: u64,
    pub token: Address,
    pub beneficiary: Address,
    pub total_amount: i128,
    pub released_amount: i128,
    pub start_time: u64,
    pub cliff_time: u64,
    pub end_time: u64,
    pub created_at: u64,
    pub created_by: Address,
}

pub struct VestingStorage;

impl VestingStorage {
    fn next_id(env: &Env) -> u64 {
        let next: u64 = env.storage().instance().get(&VESTING_COUNTER_KEY).unwrap_or(0);
        let new_next = next.saturating_add(1);
        env.storage().instance().set(&VESTING_COUNTER_KEY, &new_next);
        new_next
    }

    fn key(id: u64) -> (Symbol, u64) {
        (VESTING_KEY, id)
    }

    pub fn store(env: &Env, schedule: &VestingSchedule) {
        env.storage().persistent().set(&Self::key(schedule.id), schedule);
    }

    pub fn get(env: &Env, id: u64) -> Option<VestingSchedule> {
        env.storage().persistent().get(&Self::key(id))
    }

    pub fn update(env: &Env, schedule: &VestingSchedule) {
        env.storage().persistent().set(&Self::key(schedule.id), schedule);
    }
}

pub struct Vesting;

impl Vesting {
    /// Create a new vesting schedule for a beneficiary.
    ///
    /// # Arguments
    /// * `admin` - Admin address that funds the vesting
    /// * `token` - Token address to lock
    /// * `beneficiary` - Address receiving vested tokens
    /// * `total_amount` - Total amount to vest (must be > 0)
    /// * `start_time` - Unix timestamp when vesting starts
    /// * `cliff_seconds` - Seconds after start before any release
    /// * `end_time` - Unix timestamp when all tokens are vested
    ///
    /// # Security
    /// - Requires admin authorization
    /// - Transfers tokens into contract custody immediately
    pub fn create_schedule(
        env: &Env,
        admin: &Address,
        token: Address,
        beneficiary: Address,
        total_amount: i128,
        start_time: u64,
        cliff_seconds: u64,
        end_time: u64,
    ) -> Result<u64, QuickLendXError> {
        admin.require_auth();
        AdminStorage::require_admin(env, admin)?;

        if total_amount <= 0 {
            return Err(QuickLendXError::InvalidAmount);
        }
        if end_time <= start_time {
            return Err(QuickLendXError::InvalidTimestamp);
        }

        let cliff_time = start_time
            .checked_add(cliff_seconds)
            .ok_or(QuickLendXError::InvalidTimestamp)?;
        if cliff_time > end_time {
            return Err(QuickLendXError::InvalidTimestamp);
        }

        let id = VestingStorage::next_id(env);
        let now = env.ledger().timestamp();

        let schedule = VestingSchedule {
            id,
            token: token.clone(),
            beneficiary: beneficiary.clone(),
            total_amount,
            released_amount: 0,
            start_time,
            cliff_time,
            end_time,
            created_at: now,
            created_by: admin.clone(),
        };

        // Move tokens into contract custody.
        let contract = env.current_contract_address();
        transfer_funds(env, &token, admin, &contract, total_amount)?;

        VestingStorage::store(env, &schedule);
        env.events().publish(
            (symbol_short!("vest_new"),),
            (id, beneficiary, token, total_amount, start_time, cliff_time, end_time),
        );

        Ok(id)
    }

    /// Return the vesting schedule, if present.
    pub fn get_schedule(env: &Env, id: u64) -> Option<VestingSchedule> {
        VestingStorage::get(env, id)
    }

    /// Calculate total vested amount for a schedule at current time.
    pub fn vested_amount(env: &Env, schedule: &VestingSchedule) -> Result<i128, QuickLendXError> {
        let now = env.ledger().timestamp();
        if now < schedule.cliff_time {
            return Ok(0);
        }
        if now <= schedule.start_time {
            return Ok(0);
        }
        if now >= schedule.end_time {
            return Ok(schedule.total_amount);
        }

        let duration = schedule.end_time.saturating_sub(schedule.start_time);
        if duration == 0 {
            return Err(QuickLendXError::InvalidTimestamp);
        }
        let elapsed = now.saturating_sub(schedule.start_time);
        let numerator = schedule
            .total_amount
            .checked_mul(elapsed as i128)
            .ok_or(QuickLendXError::InvalidAmount)?;
        Ok(numerator / duration as i128)
    }

    /// Compute how much can be released right now.
    pub fn releasable_amount(
        env: &Env,
        schedule: &VestingSchedule,
    ) -> Result<i128, QuickLendXError> {
        let vested = Self::vested_amount(env, schedule)?;
        Ok((vested - schedule.released_amount).max(0))
    }

    /// Release vested tokens to the beneficiary.
    ///
    /// # Security
    /// - Requires beneficiary authorization
    /// - Enforces timelock/cliff and prevents over-release
    pub fn release(
        env: &Env,
        beneficiary: &Address,
        id: u64,
    ) -> Result<i128, QuickLendXError> {
        beneficiary.require_auth();

        let mut schedule = VestingStorage::get(env, id)
            .ok_or(QuickLendXError::StorageKeyNotFound)?;

        if &schedule.beneficiary != beneficiary {
            return Err(QuickLendXError::Unauthorized);
        }

        let releasable = Self::releasable_amount(env, &schedule)?;
        if releasable <= 0 {
            return Err(QuickLendXError::OperationNotAllowed);
        }

        let contract = env.current_contract_address();
        transfer_funds(env, &schedule.token, &contract, beneficiary, releasable)?;

        schedule.released_amount = schedule
            .released_amount
            .saturating_add(releasable);
        VestingStorage::update(env, &schedule);

        env.events().publish(
            (symbol_short!("vest_rel"),),
            (id, beneficiary.clone(), schedule.token, releasable),
        );

        Ok(releasable)
    }
}
