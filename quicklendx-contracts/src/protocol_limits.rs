use soroban_sdk::{contracttype, Address, Env, String};

use crate::admin::AdminStorage;
use crate::errors::QuickLendXError;

#[allow(dead_code)]
#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(test, derive(Debug))]
pub struct ProtocolLimits {
    pub min_invoice_amount: i128,
    pub min_bid_amount: i128,
    pub min_bid_bps: u32,
    pub max_due_date_days: u64,
    pub grace_period_seconds: u64,
    pub max_invoices_per_business: u32,
}

#[allow(dead_code)]
const LIMITS_KEY: &str = "protocol_limits";

#[cfg(not(test))]
const DEFAULT_MIN_AMOUNT: i128 = 1_000_000; // 1 token (6 decimals)
#[cfg(test)]
const DEFAULT_MIN_AMOUNT: i128 = 10;

/// @notice Default minimum bid amount (smallest unit).
pub const DEFAULT_MIN_BID_AMOUNT: i128 = 10;
/// @notice Default minimum bid rate in basis points.
pub const DEFAULT_MIN_BID_BPS: u32 = 100; // 1%

#[allow(dead_code)]
const DEFAULT_MAX_DUE_DAYS: u64 = 365;
#[allow(dead_code)]
const DEFAULT_GRACE_PERIOD: u64 = 7 * 24 * 60 * 60; // 7 days
#[allow(dead_code)]
pub const DEFAULT_MAX_INVOICES_PER_BUSINESS: u32 = 100; // 0 = unlimited

// String length limits
pub const MAX_DESCRIPTION_LENGTH: u32 = 1024;
pub const MAX_NAME_LENGTH: u32 = 150;
pub const MAX_ADDRESS_LENGTH: u32 = 300;
pub const MAX_TAX_ID_LENGTH: u32 = 50;
pub const MAX_NOTES_LENGTH: u32 = 2000;
pub const MAX_TAG_LENGTH: u32 = 50;
pub const MAX_TRANSACTION_ID_LENGTH: u32 = 124;
pub const MAX_DISPUTE_REASON_LENGTH: u32 = 1000;
pub const MAX_DISPUTE_EVIDENCE_LENGTH: u32 = 2000;
pub const MAX_DISPUTE_RESOLUTION_LENGTH: u32 = 2000;
pub const MAX_NOTIFICATION_TITLE_LENGTH: u32 = 150;
pub const MAX_NOTIFICATION_MESSAGE_LENGTH: u32 = 1000;
pub const MAX_KYC_DATA_LENGTH: u32 = 5000;
pub const MAX_REJECTION_REASON_LENGTH: u32 = 500;
pub const MAX_FEEDBACK_LENGTH: u32 = 1000;

pub fn check_string_length(s: &String, max_len: u32) -> Result<(), QuickLendXError> {
    if s.len() > max_len {
        return Err(QuickLendXError::InvalidDescription);
    }
    Ok(())
}

#[allow(dead_code)]
pub struct ProtocolLimitsContract;

#[allow(dead_code)]
impl ProtocolLimitsContract {
    /// @notice Initialize protocol limits storage with defaults.
    /// @dev Backward-compat helper. Prefer using `QuickLendXContract::initialize()` which
    ///      sets protocol limits atomically.
    pub fn initialize(env: Env, admin: Address) -> Result<(), QuickLendXError> {
        admin.require_auth();
        AdminStorage::require_admin(&env, &admin)?;

        if env.storage().instance().has(&LIMITS_KEY) {
            return Err(QuickLendXError::OperationNotAllowed);
        }

        let limits = ProtocolLimits {
            min_invoice_amount: DEFAULT_MIN_AMOUNT,
            min_bid_amount: DEFAULT_MIN_BID_AMOUNT,
            min_bid_bps: DEFAULT_MIN_BID_BPS,
            max_due_date_days: DEFAULT_MAX_DUE_DAYS,
            grace_period_seconds: DEFAULT_GRACE_PERIOD,
            max_invoices_per_business: DEFAULT_MAX_INVOICES_PER_BUSINESS,
        };

        env.storage().instance().set(&LIMITS_KEY, &limits);
        Ok(())
    }

    /// @notice Update protocol-wide limits used for invoice/bid validation.
    /// @dev Requires admin authorization.
    pub fn set_protocol_limits(
        env: Env,
        admin: Address,
        min_invoice_amount: i128,
        min_bid_amount: i128,
        min_bid_bps: u32,
        max_due_date_days: u64,
        grace_period_seconds: u64,
        max_invoices_per_business: u32,
    ) -> Result<(), QuickLendXError> {
        admin.require_auth();
        Self::set_protocol_limits_authed(
            &env,
            &admin,
            min_invoice_amount,
            min_bid_amount,
            min_bid_bps,
            max_due_date_days,
            grace_period_seconds,
            max_invoices_per_business,
        )
    }

    /// @notice Update protocol limits without calling `require_auth` again.
    /// @dev Intended for internal use when the caller has already been authorized
    ///      in the same invocation frame (e.g., during contract initialization).
    pub(crate) fn set_protocol_limits_authed(
        env: &Env,
        admin: &Address,
        min_invoice_amount: i128,
        min_bid_amount: i128,
        min_bid_bps: u32,
        max_due_date_days: u64,
        grace_period_seconds: u64,
        max_invoices_per_business: u32,
    ) -> Result<(), QuickLendXError> {
        AdminStorage::require_admin(env, admin)?;

        if min_invoice_amount <= 0 {
            return Err(QuickLendXError::InvalidAmount);
        }

        if min_bid_amount <= 0 {
            return Err(QuickLendXError::InvalidAmount);
        }

        if min_bid_bps > 10_000 {
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
            min_bid_amount,
            min_bid_bps,
            max_due_date_days,
            grace_period_seconds,
            max_invoices_per_business,
        };

        env.storage().instance().set(&LIMITS_KEY, &limits);
        Ok(())
    }

    /// @notice Read protocol limits.
    /// @dev Returns defaults when not configured.
    pub fn get_protocol_limits(env: Env) -> ProtocolLimits {
        env.storage()
            .instance()
            .get(&LIMITS_KEY)
            .unwrap_or(ProtocolLimits {
                min_invoice_amount: DEFAULT_MIN_AMOUNT,
                min_bid_amount: DEFAULT_MIN_BID_AMOUNT,
                min_bid_bps: DEFAULT_MIN_BID_BPS,
                max_due_date_days: DEFAULT_MAX_DUE_DAYS,
                grace_period_seconds: DEFAULT_GRACE_PERIOD,
                max_invoices_per_business: DEFAULT_MAX_INVOICES_PER_BUSINESS,
            })
    }

    /// @notice Validate invoice amount and due date against configured limits.
    pub fn validate_invoice(env: Env, amount: i128, due_date: u64) -> Result<(), QuickLendXError> {
        let limits = Self::get_protocol_limits(env.clone());
        let current_time = env.ledger().timestamp();

        if amount < limits.min_invoice_amount {
            return Err(QuickLendXError::InvalidAmount);
        }

        let max_due_date =
            current_time.saturating_add(limits.max_due_date_days.saturating_mul(86400));
        if due_date > max_due_date {
            return Err(QuickLendXError::InvoiceDueDateInvalid);
        }

        Ok(())
    }

    /// @notice Compute the default timestamp (due date + grace period).
    pub fn get_default_date(env: Env, due_date: u64) -> u64 {
        let limits = Self::get_protocol_limits(env.clone());
        due_date.saturating_add(limits.grace_period_seconds)
    }
}

/// @notice Returns true when protocol limits are stored in instance storage.
/// @dev Used by initialization sanity checks to detect partial state.
pub(crate) fn has_protocol_limits(env: &Env) -> bool {
    env.storage().instance().has(&LIMITS_KEY)
}

pub fn compute_min_bid_amount(invoice_amount: i128, limits: &ProtocolLimits) -> i128 {
    let percent_min = invoice_amount
        .saturating_mul(limits.min_bid_bps as i128)
        .saturating_div(10_000);
    if percent_min > limits.min_bid_amount {
        percent_min
    } else {
        limits.min_bid_amount
    }
}
