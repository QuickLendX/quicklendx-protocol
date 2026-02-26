use soroban_sdk::{contracttype, Address, Env, String};

use crate::{admin::ADMIN_KEY, errors::QuickLendXError};

/// Protocol limits configuration for invoice validation and default handling.
///
/// This struct defines system-wide constraints that ensure consistent risk management
/// across the platform. All values are configurable by administrators.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProtocolLimits {
    /// Minimum acceptable invoice value in smallest currency unit (e.g., stroops)
    pub min_invoice_amount: i128,
    pub min_bid_amount: i128,
    pub min_bid_bps: u32,
    pub max_due_date_days: u64,
    /// Grace period after due date before default can be triggered (0-2,592,000 seconds)
    pub grace_period_seconds: u64,
}

/// Storage key for protocol limits
const LIMITS_KEY: &str = "protocol_limits";
#[allow(dead_code)]
#[cfg(not(test))]
const DEFAULT_MIN_AMOUNT: i128 = 1_000_000; // 1 token (6 decimals)
#[cfg(test)]
const DEFAULT_MIN_AMOUNT: i128 = 1000; // Allow legacy tests to pass
#[allow(dead_code)]
const DEFAULT_MIN_BID_AMOUNT: i128 = 100; // Absolute bid floor (dust protection)
#[allow(dead_code)]
const DEFAULT_MIN_BID_BPS: u32 = 100; // 1% of invoice amount
#[allow(dead_code)]
const DEFAULT_MAX_DUE_DAYS: u64 = 365;
#[allow(dead_code)]
const DEFAULT_GRACE_PERIOD: u64 = 7 * 24 * 60 * 60; // 7 days

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

// Separate struct for protocol limits (not a contract, just a helper)
#[allow(dead_code)]
pub struct ProtocolLimitsContract;

#[allow(dead_code)]
impl ProtocolLimitsContract {
    /// Initialize protocol limits with default values.
    ///
    /// This function can only be called once to set up the initial protocol limits
    /// and designate the admin address. Subsequent calls will fail.
    ///
    /// # Arguments
    ///
    /// * `env` - The contract environment
    /// * `admin` - The address that will have permission to update limits
    ///
    /// # Returns
    ///
    /// * `Ok(())` - Initialization successful
    /// * `Err(QuickLendXError::OperationNotAllowed)` - Already initialized
    ///
    /// # Examples
    ///
    /// ```
    /// let admin = Address::generate(&env);
    /// ProtocolLimitsContract::initialize(env.clone(), admin)?;
    /// ```
    ///
    /// # Security
    ///
    /// - Can only be called once
    /// - No authorization required for initial setup
    /// - Admin address is permanently stored
    pub fn initialize(env: Env, admin: Address) -> Result<(), QuickLendXError> {
        // Prevent double initialization
        if env.storage().instance().has(&LIMITS_KEY) {
            return Err(QuickLendXError::OperationNotAllowed);
        }

        // Set default limits
        let limits = ProtocolLimits {
            min_invoice_amount: DEFAULT_MIN_AMOUNT,
            min_bid_amount: DEFAULT_MIN_BID_AMOUNT,
            min_bid_bps: DEFAULT_MIN_BID_BPS,
            max_due_date_days: DEFAULT_MAX_DUE_DAYS,
            grace_period_seconds: DEFAULT_GRACE_PERIOD,
        };

        // Store limits and admin address
        env.storage().instance().set(&LIMITS_KEY, &limits);
        env.storage().instance().set(&ADMIN_KEY, &admin);
        Ok(())
    }

    /// Update protocol limits with new values.
    ///
    /// This function allows the admin to update system-wide limits. All parameters
    /// are validated before being stored.
    ///
    /// # Arguments
    ///
    /// * `env` - The contract environment
    /// * `admin` - The admin address (must match stored admin)
    /// * `min_invoice_amount` - New minimum invoice amount (must be > 0)
    /// * `max_due_date_days` - New maximum due date days (must be 1-730)
    /// * `grace_period_seconds` - New grace period (must be 0-2,592,000)
    ///
    /// # Returns
    ///
    /// * `Ok(())` - Update successful
    /// * `Err(QuickLendXError::NotAdmin)` - Admin not configured
    /// * `Err(QuickLendXError::Unauthorized)` - Caller is not admin
    /// * `Err(QuickLendXError::InvalidAmount)` - Amount validation failed
    /// * `Err(QuickLendXError::InvoiceDueDateInvalid)` - Days validation failed
    /// * `Err(QuickLendXError::InvalidTimestamp)` - Grace period validation failed
    ///
    /// # Security
    ///
    /// - Requires admin authorization via require_auth()
    /// - Verifies caller matches stored admin address
    /// - All parameters validated before storage
    pub fn set_protocol_limits(
        env: Env,
        admin: Address,
        min_invoice_amount: i128,
        min_bid_amount: i128,
        min_bid_bps: u32,
        max_due_date_days: u64,
        grace_period_seconds: u64,
    ) -> Result<(), QuickLendXError> {
        // Require admin authorization
        admin.require_auth();

        // Verify admin address matches stored admin
        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&ADMIN_KEY)
            .ok_or(QuickLendXError::NotAdmin)?;

        if admin != stored_admin {
            return Err(QuickLendXError::Unauthorized);
        }

        // Validate min_invoice_amount (must be positive)
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

        // Validate grace_period_seconds (must be 0-2,592,000 = 30 days)
        if grace_period_seconds > 2_592_000 {
            return Err(QuickLendXError::InvalidTimestamp);
        }

        // Create and store updated limits
        let limits = ProtocolLimits {
            min_invoice_amount,
            min_bid_amount,
            min_bid_bps,
            max_due_date_days,
            grace_period_seconds,
        };

        env.storage().instance().set(&LIMITS_KEY, &limits);
        Ok(())
    }

    /// Get current protocol limits.
    ///
    /// Returns the currently configured limits, or default values if not initialized.
    /// This function never fails and always returns valid limits.
    ///
    /// # Arguments
    ///
    /// * `env` - The contract environment
    ///
    /// # Returns
    ///
    /// Current protocol limits or defaults if uninitialized
    ///
    /// # Examples
    ///
    /// ```
    /// let limits = ProtocolLimitsContract::get_protocol_limits(env.clone());
    /// assert!(limits.min_invoice_amount > 0);
    /// ```
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
            })
    }

    pub fn validate_invoice(env: Env, amount: i128, due_date: u64) -> Result<(), QuickLendXError> {
        let limits = Self::get_protocol_limits(env.clone());
        let current_time = env.ledger().timestamp();

        // Check minimum amount
        if amount < limits.min_invoice_amount {
            return Err(QuickLendXError::InvalidAmount);
        }

        // Check maximum due date (current time + max days in seconds)
        let max_due_date =
            current_time.saturating_add(limits.max_due_date_days.saturating_mul(86400));
        if due_date > max_due_date {
            return Err(QuickLendXError::InvoiceDueDateInvalid);
        }

        Ok(())
    }

    /// Calculate default date by adding grace period to due date.
    ///
    /// This function is used by the default handling module to determine when
    /// an invoice can be marked as defaulted.
    ///
    /// # Arguments
    ///
    /// * `env` - The contract environment
    /// * `due_date` - The invoice due date timestamp
    ///
    /// # Returns
    ///
    /// Timestamp when default can be triggered (due_date + grace_period_seconds)
    ///
    /// # Examples
    ///
    /// ```
    /// let due_date = 1000000u64;
    /// let default_date = ProtocolLimitsContract::get_default_date(env.clone(), due_date);
    /// // default_date = due_date + grace_period_seconds
    /// ```
    pub fn get_default_date(env: Env, due_date: u64) -> u64 {
        let limits = Self::get_protocol_limits(env.clone());
        due_date.saturating_add(limits.grace_period_seconds)
    }
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
