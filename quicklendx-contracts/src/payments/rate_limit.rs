//! Bounded rate limiters for payment/escrow and repayment operations.

use crate::errors::QuickLendXError;
use crate::storage::extend_persistent_ttl;
use soroban_sdk::{contracttype, symbol_short, Address, Env, Symbol};

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

/// Maximum number of repayment / profit-distribution operations
/// (`repay_escrow`) allowed per rate-limit window per repaying account.
#[cfg(not(test))]
pub const MAX_REPAYMENTS_PER_WINDOW: u32 = 10;
#[cfg(test)]
pub const MAX_REPAYMENTS_PER_WINDOW: u32 = 3;

/// Window duration for repayment distribution rate limiting (in seconds).
pub const REPAYMENT_RATE_LIMIT_WINDOW_SECS: u64 = PAYMENT_RATE_LIMIT_WINDOW_SECS;

const REPAYMENT_RATE_LIMIT_KEY: Symbol = symbol_short!("rep_rl");

/// Bounded rate limiter for repayment / profit-distribution operations.
///
/// Distinct from [`PaymentRateLimiter`] (which guards escrow creation): this
/// limiter guards the fee/principal/profit distribution path (`repay_escrow`)
/// and reports the actionable [`QuickLendXError::MutationLimitExceeded`] on
/// throttle so callers know the exact condition and retry window.
pub struct RepaymentRateLimiter;

impl RepaymentRateLimiter {
    fn key(account: &Address) -> (Symbol, Address) {
        (REPAYMENT_RATE_LIMIT_KEY, account.clone())
    }

    /// Check and advance the repayment rate limit for `account`.
    ///
    /// Running *before* any storage read, allocation, or token transfer, this
    /// rejects with [`QuickLendXError::MutationLimitExceeded`] when `account`
    /// exceeds the allowed repayment count in the active window. Rejection
    /// mutates no escrow/invoice/token state.
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
                .saturating_add(REPAYMENT_RATE_LIMIT_WINDOW_SECS)
        {
            record.window_start = now;
            record.count = 1;
        } else {
            if record.count >= MAX_REPAYMENTS_PER_WINDOW {
                return Err(QuickLendXError::MutationLimitExceeded);
            }
            record.count = record.count.saturating_add(1);
        }

        env.storage().persistent().set(&key, &record);
        extend_persistent_ttl(env, &key);
        Ok(())
    }

    /// Read the repayment rate limit record for `account` without mutating storage.
    pub fn get_rate_limit(env: &Env, account: &Address) -> PaymentRateLimitRecord {
        let key = Self::key(account);
        let now = env.ledger().timestamp();
        env.storage()
            .persistent()
            .get::<_, PaymentRateLimitRecord>(&key)
            .map(|mut r| {
                if now
                    >= r.window_start
                        .saturating_add(REPAYMENT_RATE_LIMIT_WINDOW_SECS)
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
