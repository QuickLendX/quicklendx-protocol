use soroban_sdk::{contract, contractimpl, contracttype, Address, Env};

use crate::QuickLendXError;

/// Protocol limits configuration for invoice validation and default handling.
///
/// This struct defines system-wide constraints that ensure consistent risk management
/// across the platform. All values are configurable by administrators.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProtocolLimits {
    /// Minimum acceptable invoice value in smallest currency unit (e.g., stroops)
    pub min_invoice_amount: i128,
    /// Maximum days from current time for invoice due dates (1-730 days)
    pub max_due_date_days: u64,
    /// Grace period after due date before default can be triggered (0-2,592,000 seconds)
    pub grace_period_seconds: u64,
}

/// Storage key for protocol limits
const LIMITS_KEY: &str = "protocol_limits";
/// Storage key for admin address
const ADMIN_KEY: &str = "admin";
/// Default minimum invoice amount: 1 token with 6 decimals
const DEFAULT_MIN_AMOUNT: i128 = 1_000_000;
/// Default maximum due date: 365 days (1 year)
const DEFAULT_MAX_DUE_DAYS: u64 = 365;
/// Default grace period: 86400 seconds (24 hours)
const DEFAULT_GRACE_PERIOD: u64 = 86400;

/// Protocol limits contract for managing system-wide constraints
#[contract]
pub struct ProtocolLimitsContract;

#[contractimpl]
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

        // Validate max_due_date_days (must be 1-730)
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
                max_due_date_days: DEFAULT_MAX_DUE_DAYS,
                grace_period_seconds: DEFAULT_GRACE_PERIOD,
            })
    }

    /// Validate invoice parameters against current protocol limits.
    ///
    /// Checks if the given amount and due date meet the current protocol limits.
    /// This is a convenience function for invoice validation.
    ///
    /// # Arguments
    ///
    /// * `env` - The contract environment
    /// * `amount` - Invoice amount to validate
    /// * `due_date` - Invoice due date timestamp to validate
    ///
    /// # Returns
    ///
    /// * `true` - Invoice parameters are valid
    /// * `false` - Invoice parameters violate limits
    ///
    /// # Validation Rules
    ///
    /// - Amount must be >= min_invoice_amount
    /// - Due date must be <= current_time + (max_due_date_days * 86400)
    pub fn validate_invoice(env: Env, amount: i128, due_date: u64) -> bool {
        let limits = Self::get_protocol_limits(env.clone());
        let current_time = env.ledger().timestamp();

        // Check minimum amount
        if amount < limits.min_invoice_amount {
            return false;
        }

        // Check maximum due date (current time + max days in seconds)
        let max_due_date =
            current_time.saturating_add(limits.max_due_date_days.saturating_mul(86400));
        if due_date > max_due_date {
            return false;
        }

        true
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
