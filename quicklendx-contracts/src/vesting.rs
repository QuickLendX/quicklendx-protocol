//! Token vesting module with time-locked release schedules.
//!
//! Supports admin-created vesting schedules that lock protocol tokens or rewards
//!
//! in the contract and release them linearly over time after an optional cliff.
//! Beneficiaries can claim vested tokens as they unlock.

use soroban_sdk::{contracttype, symbol_short, Address, Env, Symbol};

use crate::admin::AdminStorage;
use crate::errors::QuickLendXError;
use crate::payments::transfer_funds;

const VESTING_COUNTER_KEY: Symbol = symbol_short!("vest_cnt");
const VESTING_KEY: Symbol = symbol_short!("vest");

/// Events emitted by the vesting module.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum VestingEvent {
    NewSchedule {
        id: u64,
        beneficiary: Address,
        token: Address,
        amount: i128,
        cliff: u64,
        start: u64,
        end: u64,
    },
    Released {
        id: u64,
        beneficiary: Address,
        token: Address,
        amount: i128,
    },
}

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
        let next: u64 = env
            .storage()
            .instance()
            .get(&VESTING_COUNTER_KEY)
            .unwrap_or(0);
        let new_next = next.saturating_add(1);
        env.storage()
            .instance()
            .set(&VESTING_COUNTER_KEY, &new_next);
        new_next
    }

    fn key(id: u64) -> (Symbol, u64) {
        (VESTING_KEY, id)
    }

    pub fn store(env: &Env, schedule: &VestingSchedule) {
        env.storage()
            .persistent()
            .set(&Self::key(schedule.id), schedule);
    }

    pub fn get(env: &Env, id: u64) -> Option<VestingSchedule> {
        env.storage().persistent().get(&Self::key(id))
    }

    pub fn update(env: &Env, schedule: &VestingSchedule) {
        env.storage()
            .persistent()
            .set(&Self::key(schedule.id), schedule);
    }
}

/// Aggregated vesting summary for a single beneficiary across all their schedules.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VestingSummary {
    /// Number of schedules belonging to this user.
    pub grant_count: u32,
    /// Sum of `total_amount` across all schedules.
    pub total_granted: i128,
    /// Sum of `released_amount` across all schedules.
    pub total_released: i128,
    /// Sum of currently releasable amounts across all schedules.
    pub total_releasable: i128,
}

pub struct Vesting;

impl Vesting {
    /// Validate vesting schedule inputs and compute the derived cliff timestamp.
    ///
    /// # Security
    /// - Rejects zero-value schedules
    /// - Prevents backdated or non-monotonic timelines
    /// - Rejects cliff configurations that eliminate the post-cliff vesting window
    fn validate_schedule_inputs(
        env: &Env,
        total_amount: i128,
        start_time: u64,
        cliff_seconds: u64,
        end_time: u64,
    ) -> Result<u64, QuickLendXError> {
        if total_amount <= 0 {
            return Err(QuickLendXError::InvalidAmount);
        }

        let now = env.ledger().timestamp();
        if start_time < now {
            return Err(QuickLendXError::InvalidTimestamp);
        }
        if end_time <= start_time {
            return Err(QuickLendXError::InvalidTimestamp);
        }

        let cliff_time = start_time
            .checked_add(cliff_seconds)
            .ok_or(QuickLendXError::InvalidTimestamp)?;
        if cliff_time >= end_time {
            return Err(QuickLendXError::InvalidTimestamp);
        }

        Ok(cliff_time)
    }

    /// Validate schedule invariants before performing vesting arithmetic.
    fn validate_schedule_state(schedule: &VestingSchedule) -> Result<(), QuickLendXError> {
        if schedule.total_amount <= 0 {
            return Err(QuickLendXError::InvalidAmount);
        }
        if schedule.released_amount < 0 || schedule.released_amount > schedule.total_amount {
            return Err(QuickLendXError::InvalidAmount);
        }
        if schedule.start_time >= schedule.end_time {
            return Err(QuickLendXError::InvalidTimestamp);
        }
        if schedule.cliff_time < schedule.start_time || schedule.cliff_time >= schedule.end_time {
            return Err(QuickLendXError::InvalidTimestamp);
        }

        Ok(())
    }

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

        let cliff_time =
            Self::validate_schedule_inputs(env, total_amount, start_time, cliff_seconds, end_time)?;

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
            (symbol_short!("vesting"), symbol_short!("created")),
            (
                id,
                beneficiary.clone(),
                token.clone(),
                total_amount,
                start_time,
                cliff_time,
                end_time,
            ),
        );

        Ok(id)
    }

    /// Return the vesting schedule, if present.
    pub fn get_schedule(env: &Env, id: u64) -> Option<VestingSchedule> {
        VestingStorage::get(env, id)
    }

    /// Return an aggregated summary of all vesting schedules for `user`.
    ///
    /// Scans every schedule from id 1 up to the current counter value and
    /// collects those whose `beneficiary` matches `user`.  Returns a zeroed
    /// `VestingSummary` when no matching schedule exists.
    pub fn get_summary_for_user(env: &Env, user: &Address) -> VestingSummary {
        let max_id: u64 = env
            .storage()
            .instance()
            .get(&VESTING_COUNTER_KEY)
            .unwrap_or(0);

        let mut grant_count: u32 = 0;
        let mut total_granted: i128 = 0;
        let mut total_released: i128 = 0;
        let mut total_releasable: i128 = 0;

        for id in 1..=max_id {
            if let Some(schedule) = VestingStorage::get(env, id) {
                if &schedule.beneficiary == user {
                    grant_count = grant_count.saturating_add(1);
                    total_granted = total_granted.saturating_add(schedule.total_amount);
                    total_released = total_released.saturating_add(schedule.released_amount);
                    if let Ok(r) = Self::releasable_amount(env, &schedule) {
                        total_releasable = total_releasable.saturating_add(r);
                    }
                }
            }
        }

        VestingSummary {
            grant_count,
            total_granted,
            total_released,
            total_releasable,
        }
    }

    /// Calculate total vested amount for a schedule at current time.
    ///
    /// # Vesting curve
    ///
    /// The curve is **linear** from `start_time` to `end_time`, gated by a cliff:
    ///
    /// ```text
    /// vested(t) = 0                                          if t < cliff_time
    ///           = total_amount                               if t >= end_time
    ///           = total_amount * (t - start_time)           otherwise
    ///                          / (end_time - start_time)
    /// ```
    ///
    /// Integer division **truncates** (rounds toward zero), so the beneficiary
    /// may receive up to 1 token less than the real-valued curve until the next
    /// second boundary.  The final release at `end_time` always delivers the
    /// exact `total_amount`, eliminating any accumulated rounding dust.
    ///
    /// # Overflow safety
    ///
    /// - `elapsed` and `duration` are computed with `saturating_sub` on `u64`,
    ///   so they are always ≥ 0 and ≤ `u64::MAX`.
    /// - The numerator `total_amount * elapsed` uses `checked_mul` on `i128`;
    ///   overflow returns `InvalidAmount` rather than wrapping.
    /// - Because `elapsed < duration` in the linear branch, the quotient is
    ///   strictly less than `total_amount`, so the result fits in `i128`.
    pub fn vested_amount(env: &Env, schedule: &VestingSchedule) -> Result<i128, QuickLendXError> {
        Self::validate_schedule_state(schedule)?;

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

        // duration > 0 is guaranteed by validate_schedule_state (end > start).
        let duration = schedule.end_time.saturating_sub(schedule.start_time);
        if duration == 0 {
            return Err(QuickLendXError::InvalidTimestamp);
        }
        // elapsed < duration because now < end_time, so the quotient < total_amount.
        let elapsed = now.saturating_sub(schedule.start_time);
        let numerator = schedule
            .total_amount
            .checked_mul(elapsed as i128)
            .ok_or(QuickLendXError::InvalidAmount)?;
        Ok(numerator / duration as i128)
    }

    /// Compute how much can be released right now.
    ///
    /// `releasable = vested_amount(now) - released_amount`
    ///
    /// This is always ≥ 0 because `released_amount` is only ever incremented
    /// by the return value of a previous `releasable_amount` call, and
    /// `vested_amount` is monotonically non-decreasing.  `checked_sub` is used
    /// as a defence-in-depth guard; a negative result would indicate state
    /// corruption and returns `InvalidAmount`.
    pub fn releasable_amount(
        env: &Env,
        schedule: &VestingSchedule,
    ) -> Result<i128, QuickLendXError> {
        let vested = Self::vested_amount(env, schedule)?;
        // Defence-in-depth: checked_sub catches any state corruption where
        // released_amount somehow exceeds vested_amount.
        let releasable = vested
            .checked_sub(schedule.released_amount)
            .ok_or(QuickLendXError::InvalidAmount)?;
        Ok(releasable)
    }

    /// Release vested tokens to the beneficiary.
    ///
    /// # Security
    /// - Requires beneficiary authorization
    /// - Enforces timelock/cliff: returns `InvalidTimestamp` if called before cliff
    /// - Prevents over-release via `released_amount` tracking
    /// - Idempotent after full release: returns `Ok(0)` when nothing remains
    pub fn release(env: &Env, beneficiary: &Address, id: u64) -> Result<i128, QuickLendXError> {
        beneficiary.require_auth();

        let mut schedule =
            VestingStorage::get(env, id).ok_or(QuickLendXError::StorageKeyNotFound)?;

        if &schedule.beneficiary != beneficiary {
            return Err(QuickLendXError::Unauthorized);
        }

        // Enforce cliff: reject early calls with a typed error so callers can distinguish
        // "too early" from "already fully released".
        let now = env.ledger().timestamp();
        if now < schedule.cliff_time {
            return Err(QuickLendXError::InvalidTimestamp);
        }

        let releasable = Self::releasable_amount(env, &schedule)?;
        if releasable <= 0 {
            // Idempotent behavior: repeated calls return 0 instead of error
            return Ok(0);
        }
        let contract = env.current_contract_address();
        transfer_funds(env, &schedule.token, &contract, beneficiary, releasable)?;

        schedule.released_amount = schedule
            .released_amount
            .checked_add(releasable)
            .ok_or(QuickLendXError::InvalidAmount)?;
        Self::validate_schedule_state(&schedule)?;
        VestingStorage::update(env, &schedule);

        env.events().publish(
            (symbol_short!("vesting"), symbol_short!("released")),
            (id, beneficiary.clone(), schedule.token.clone(), releasable),
        );
        Ok(releasable)
    }
}
