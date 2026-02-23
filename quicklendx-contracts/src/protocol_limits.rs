use soroban_sdk::{contract, contractimpl, contracttype, Address, Env};

use crate::QuickLendXError;

#[allow(dead_code)]
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProtocolLimits {
    pub min_invoice_amount: i128,
    pub max_due_date_days: u64,
    pub grace_period_seconds: u64,
}

#[allow(dead_code)]
const LIMITS_KEY: &str = "protocol_limits";
#[allow(dead_code)]
const DEFAULT_MIN_AMOUNT: i128 = 1_000_000; // 1 token (6 decimals)
#[allow(dead_code)]
const DEFAULT_MAX_DUE_DAYS: u64 = 365;
#[allow(dead_code)]
const DEFAULT_GRACE_PERIOD: u64 = 86400; // 24 hours

#[allow(dead_code)]
#[contract]
pub struct ProtocolLimitsContract;

#[allow(dead_code)]
#[contractimpl]
impl ProtocolLimitsContract {
    pub fn initialize(env: Env, admin: Address) -> Result<(), QuickLendXError> {
        if env.storage().instance().has(&LIMITS_KEY) {
            return Err(QuickLendXError::OperationNotAllowed);
        }

        let limits = ProtocolLimits {
            min_invoice_amount: DEFAULT_MIN_AMOUNT,
            max_due_date_days: DEFAULT_MAX_DUE_DAYS,
            grace_period_seconds: DEFAULT_GRACE_PERIOD,
        };

        env.storage().instance().set(&LIMITS_KEY, &limits);
        env.storage().instance().set(&"admin", &admin);
        Ok(())
    }

    pub fn set_protocol_limits(
        env: Env,
        admin: Address,
        min_invoice_amount: i128,
        max_due_date_days: u64,
        grace_period_seconds: u64,
    ) -> Result<(), QuickLendXError> {
        admin.require_auth();

        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&"admin")
            .ok_or(QuickLendXError::NotAdmin)?;

        if admin != stored_admin {
            return Err(QuickLendXError::Unauthorized);
        }

        if min_invoice_amount <= 0 {
            return Err(QuickLendXError::InvalidAmount);
        }

        if max_due_date_days == 0 || max_due_date_days > 730 {
            return Err(QuickLendXError::InvoiceDueDateInvalid);
        }

        if grace_period_seconds > 2_592_000 {
            return Err(QuickLendXError::InvalidTimestamp);
        }

        let limits = ProtocolLimits {
            min_invoice_amount,
            max_due_date_days,
            grace_period_seconds,
        };

        env.storage().instance().set(&LIMITS_KEY, &limits);
        Ok(())
    }

    pub fn get_protocol_limits(env: Env) -> ProtocolLimits {
        env.storage()
            .instance()
            .get(&LIMITS_KEY)
            .unwrap_or(ProtocolLimits {
                min_invoice_amount: DEFAULT_MIN_AMOUNT,
                max_due_date_days: DEFAULT_MAX_DUE_DAYS,
                grace_period_seconds: DEFAULT_GRACE_PERIOD,
            })
    }

    pub fn validate_invoice(env: Env, amount: i128, due_date: u64) -> bool {
        let limits = Self::get_protocol_limits(env.clone());
        let current_time = env.ledger().timestamp();

        if amount < limits.min_invoice_amount {
            return false;
        }

        let max_due_date = current_time + (limits.max_due_date_days * 86400);
        if due_date > max_due_date {
            return false;
        }

        true
    }

    pub fn get_default_date(env: Env, due_date: u64) -> u64 {
        let limits = Self::get_protocol_limits(env.clone());
        due_date + limits.grace_period_seconds
    }
}
