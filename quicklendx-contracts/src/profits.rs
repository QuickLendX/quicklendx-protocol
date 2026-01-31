//! Profit and Fee Calculation Module for QuickLendX Protocol
//!
//! This module implements the centralized profit and fee calculation formulas
//! used throughout the protocol for invoice settlement.
//!
//! # Formula Overview
//!
//! When an invoice is settled, the payment flow is:
//! 1. Business pays `payment_amount` to settle the invoice
//! 2. If payment > investment (profit exists):
//!    - `gross_profit = payment_amount - investment_amount`
//!    - `platform_fee = floor(gross_profit * fee_bps / 10_000)`
//!    - `investor_profit = gross_profit - platform_fee`
//!    - `investor_return = investment_amount + investor_profit`
//! 3. If payment <= investment (no profit/loss):
//!    - `platform_fee = 0`
//!    - `investor_return = payment_amount`
//!
//! # Rounding Strategy
//!
//! - All divisions use integer floor division (truncation toward zero)
//! - Fees are always rounded DOWN to favor investors
//! - This ensures: `investor_return + platform_fee == payment_amount` (no dust)
//! - The platform absorbs any rounding loss
//!
//! # Overflow Safety
//!
//! - Uses `saturating_*` arithmetic to prevent overflow panics
//! - Maximum supported amounts: i128::MAX (approximately 1.7 × 10^38)
//! - Fee basis points capped at 1000 (10%)
//!
//! # Security Considerations
//!
//! - No floating point arithmetic (deterministic results)
//! - Immutable calculation functions (no state modification in core logic)
//! - Bounds checking on all inputs
//! - Fee configuration requires admin authorization

use crate::errors::QuickLendXError;
use crate::events::emit_platform_fee_updated;
use soroban_sdk::{contracttype, symbol_short, Address, Env};

// ============================================================================
// Constants
// ============================================================================

/// Default platform fee in basis points (2% = 200 bps)
pub const DEFAULT_PLATFORM_FEE_BPS: i128 = 200;

/// Maximum allowed platform fee in basis points (10% = 1000 bps)
pub const MAX_PLATFORM_FEE_BPS: i128 = 1_000;

/// Basis points denominator for percentage calculations (100% = 10,000 bps)
pub const BPS_DENOMINATOR: i128 = 10_000;

/// Minimum valid amount for calculations (must be positive)
pub const MIN_VALID_AMOUNT: i128 = 0;

// ============================================================================
// Data Types
// ============================================================================

/// Platform fee configuration stored on-chain
#[contracttype]
#[derive(Clone, Debug)]
pub struct PlatformFeeConfig {
    /// Fee in basis points (e.g., 200 = 2%)
    pub fee_bps: i128,
    /// Timestamp when config was last updated
    pub updated_at: u64,
    /// Address that last updated the config
    pub updated_by: Address,
}

/// Complete breakdown of profit and fee calculation
///
/// This struct provides full transparency into how funds are distributed
/// during settlement. It can be used for:
/// - Event emission with detailed breakdown
/// - Frontend display of fee calculations
/// - Audit trail and verification
/// - Testing and validation
#[contracttype]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProfitFeeBreakdown {
    /// Original investment amount (principal)
    pub investment_amount: i128,
    /// Total payment received from business
    pub payment_amount: i128,
    /// Gross profit before fees (payment - investment), 0 if no profit
    pub gross_profit: i128,
    /// Platform fee deducted from profit
    pub platform_fee: i128,
    /// Net profit after platform fee (gross_profit - platform_fee)
    pub investor_profit: i128,
    /// Total amount returned to investor (investment + investor_profit)
    pub investor_return: i128,
    /// Fee rate applied in basis points
    pub fee_bps_applied: i128,
}

// ============================================================================
// PlatformFee Implementation
// ============================================================================

/// Platform fee management and calculation
pub struct PlatformFee;

impl PlatformFee {
    /// Storage key for fee configuration
    /// Note: Uses "pf_cfg" to avoid conflict with fees.rs which uses "fee_cfg" for FeeStructure list
    const STORAGE_KEY: soroban_sdk::Symbol = symbol_short!("pf_cfg");

    /// Creates the default fee configuration
    fn default_config(env: &Env) -> PlatformFeeConfig {
        PlatformFeeConfig {
            fee_bps: DEFAULT_PLATFORM_FEE_BPS,
            updated_at: 0,
            updated_by: env.current_contract_address(),
        }
    }

    /// Retrieves the current platform fee configuration
    ///
    /// Returns the stored configuration or default (2%) if not configured.
    ///
    /// # Example
    /// ```ignore
    /// let config = PlatformFee::get_config(&env);
    /// assert_eq!(config.fee_bps, 200); // 2%
    /// ```
    pub fn get_config(env: &Env) -> PlatformFeeConfig {
        env.storage()
            .instance()
            .get(&Self::STORAGE_KEY)
            .unwrap_or_else(|| Self::default_config(env))
    }

    /// Updates the platform fee configuration
    ///
    /// # Arguments
    /// * `env` - Soroban environment
    /// * `admin` - Admin address (must be authorized)
    /// * `new_fee_bps` - New fee in basis points (0-1000)
    ///
    /// # Errors
    /// * `InvalidAmount` - If fee_bps < 0 or > 1000 (10%)
    ///
    /// # Security
    /// Requires admin authorization via `require_auth()`
    pub fn set_config(
        env: &Env,
        admin: &Address,
        new_fee_bps: i128,
    ) -> Result<PlatformFeeConfig, QuickLendXError> {
        admin.require_auth();

        // Validate fee bounds
        if new_fee_bps < 0 || new_fee_bps > MAX_PLATFORM_FEE_BPS {
            return Err(QuickLendXError::InvalidAmount);
        }

        let config = PlatformFeeConfig {
            fee_bps: new_fee_bps,
            updated_at: env.ledger().timestamp(),
            updated_by: admin.clone(),
        };

        env.storage().instance().set(&Self::STORAGE_KEY, &config);
        emit_platform_fee_updated(env, &config);
        Ok(config)
    }

    /// Core calculation: computes investor return and platform fee
    ///
    /// This is the primary calculation function used during settlement.
    ///
    /// # Formula
    /// ```text
    /// if payment_amount <= investment_amount:
    ///     investor_return = payment_amount
    ///     platform_fee = 0
    /// else:
    ///     gross_profit = payment_amount - investment_amount
    ///     platform_fee = floor(gross_profit * fee_bps / 10_000)
    ///     investor_return = payment_amount - platform_fee
    /// ```
    ///
    /// # Arguments
    /// * `env` - Soroban environment
    /// * `investment_amount` - Original investment (principal)
    /// * `payment_amount` - Total payment received
    ///
    /// # Returns
    /// Tuple of `(investor_return, platform_fee)`
    ///
    /// # Invariants
    /// - `investor_return + platform_fee == payment_amount` (no dust)
    /// - `platform_fee >= 0`
    /// - `platform_fee <= gross_profit * fee_bps / 10_000`
    ///
    /// # Example
    /// ```ignore
    /// // Investment: 1000, Payment: 1100 (10% return), Fee: 2%
    /// let (investor_return, platform_fee) = PlatformFee::calculate(&env, 1000, 1100);
    /// // gross_profit = 100
    /// // platform_fee = floor(100 * 200 / 10000) = 2
    /// // investor_return = 1100 - 2 = 1098
    /// assert_eq!(platform_fee, 2);
    /// assert_eq!(investor_return, 1098);
    /// ```
    pub fn calculate(env: &Env, investment_amount: i128, payment_amount: i128) -> (i128, i128) {
        let config = Self::get_config(env);
        Self::calculate_with_fee_bps(investment_amount, payment_amount, config.fee_bps)
    }

    /// Calculate with explicit fee basis points (pure function)
    ///
    /// This function is deterministic and does not read from storage,
    /// making it ideal for testing and frontend calculations.
    ///
    /// # Arguments
    /// * `investment_amount` - Original investment (principal)
    /// * `payment_amount` - Total payment received
    /// * `fee_bps` - Fee in basis points (0-10000)
    ///
    /// # Returns
    /// Tuple of `(investor_return, platform_fee)`
    pub fn calculate_with_fee_bps(
        investment_amount: i128,
        payment_amount: i128,
        fee_bps: i128,
    ) -> (i128, i128) {
        // Handle edge cases: no payment or negative amounts
        if payment_amount <= 0 {
            return (payment_amount.max(0), 0);
        }

        // No profit scenario: payment doesn't exceed investment
        // Investor gets full payment, no fee charged
        let gross_profit = payment_amount.saturating_sub(investment_amount);
        if gross_profit <= 0 {
            return (payment_amount, 0);
        }

        // Calculate platform fee using integer division (rounds down)
        // This ensures no dust and favors the investor
        let platform_fee = gross_profit
            .saturating_mul(fee_bps)
            .checked_div(BPS_DENOMINATOR)
            .unwrap_or(0);

        // Investor return = total payment - platform fee
        // This guarantees: investor_return + platform_fee == payment_amount
        let investor_return = payment_amount.saturating_sub(platform_fee);

        (investor_return, platform_fee)
    }

    /// Calculate complete profit and fee breakdown
    ///
    /// Provides a detailed breakdown of all components for transparency,
    /// event emission, and frontend display.
    ///
    /// # Arguments
    /// * `env` - Soroban environment
    /// * `investment_amount` - Original investment (principal)
    /// * `payment_amount` - Total payment received
    ///
    /// # Returns
    /// Complete `ProfitFeeBreakdown` struct with all calculation details
    ///
    /// # Example
    /// ```ignore
    /// let breakdown = PlatformFee::calculate_breakdown(&env, 1000, 1100);
    /// assert_eq!(breakdown.investment_amount, 1000);
    /// assert_eq!(breakdown.payment_amount, 1100);
    /// assert_eq!(breakdown.gross_profit, 100);
    /// assert_eq!(breakdown.platform_fee, 2);  // 2% of 100
    /// assert_eq!(breakdown.investor_profit, 98);
    /// assert_eq!(breakdown.investor_return, 1098);
    /// ```
    pub fn calculate_breakdown(
        env: &Env,
        investment_amount: i128,
        payment_amount: i128,
    ) -> ProfitFeeBreakdown {
        let config = Self::get_config(env);
        Self::calculate_breakdown_with_fee_bps(investment_amount, payment_amount, config.fee_bps)
    }

    /// Calculate breakdown with explicit fee basis points (pure function)
    ///
    /// Deterministic calculation without storage access.
    pub fn calculate_breakdown_with_fee_bps(
        investment_amount: i128,
        payment_amount: i128,
        fee_bps: i128,
    ) -> ProfitFeeBreakdown {
        let (investor_return, platform_fee) =
            Self::calculate_with_fee_bps(investment_amount, payment_amount, fee_bps);

        let gross_profit = payment_amount.saturating_sub(investment_amount).max(0);
        let investor_profit = gross_profit.saturating_sub(platform_fee);

        ProfitFeeBreakdown {
            investment_amount,
            payment_amount,
            gross_profit,
            platform_fee,
            investor_profit,
            investor_return,
            fee_bps_applied: fee_bps,
        }
    }
}

// ============================================================================
// Public API Functions
// ============================================================================

/// Calculate investor profit from a settlement
///
/// This function computes the net profit an investor receives after
/// platform fees are deducted.
///
/// # Formula
/// ```text
/// gross_profit = max(0, payment_amount - investment_amount)
/// platform_fee = floor(gross_profit * fee_bps / 10_000)
/// investor_profit = gross_profit - platform_fee
/// ```
///
/// # Arguments
/// * `env` - Soroban environment
/// * `investment_amount` - Original investment (principal)
/// * `payment_amount` - Total payment received
///
/// # Returns
/// The net profit amount for the investor (0 if no profit)
///
/// # Example
/// ```ignore
/// let profit = calculate_investor_profit(&env, 1000, 1100);
/// assert_eq!(profit, 98); // 100 profit - 2 fee
/// ```
pub fn calculate_investor_profit(env: &Env, investment_amount: i128, payment_amount: i128) -> i128 {
    let breakdown = PlatformFee::calculate_breakdown(env, investment_amount, payment_amount);
    breakdown.investor_profit
}

/// Calculate platform fee from a settlement
///
/// This function computes the fee amount collected by the platform
/// from the profit portion of a settlement.
///
/// # Formula
/// ```text
/// gross_profit = max(0, payment_amount - investment_amount)
/// platform_fee = floor(gross_profit * fee_bps / 10_000)
/// ```
///
/// # Arguments
/// * `env` - Soroban environment
/// * `investment_amount` - Original investment (principal)
/// * `payment_amount` - Total payment received
///
/// # Returns
/// The platform fee amount (0 if no profit)
///
/// # Example
/// ```ignore
/// let fee = calculate_platform_fee(&env, 1000, 1100);
/// assert_eq!(fee, 2); // 2% of 100 profit
/// ```
pub fn calculate_platform_fee(env: &Env, investment_amount: i128, payment_amount: i128) -> i128 {
    let breakdown = PlatformFee::calculate_breakdown(env, investment_amount, payment_amount);
    breakdown.platform_fee
}

/// Calculate profit and fee (legacy function for backward compatibility)
///
/// Returns `(investor_return, platform_fee)` tuple.
///
/// # Note
/// This function is maintained for backward compatibility.
/// For new code, consider using `PlatformFee::calculate_breakdown()`
/// for more detailed information.
pub fn calculate_profit(env: &Env, investment_amount: i128, payment_amount: i128) -> (i128, i128) {
    PlatformFee::calculate(env, investment_amount, payment_amount)
}

/// Calculate treasury split from platform fees
///
/// Splits the platform fee between treasury and other recipients
/// based on configured shares.
///
/// # Formula
/// ```text
/// treasury_amount = floor(platform_fee * treasury_share_bps / 10_000)
/// remaining = platform_fee - treasury_amount
/// ```
///
/// # Arguments
/// * `platform_fee` - Total platform fee to split
/// * `treasury_share_bps` - Treasury share in basis points (e.g., 5000 = 50%)
///
/// # Returns
/// Tuple of `(treasury_amount, remaining_amount)`
///
/// # Invariants
/// - `treasury_amount + remaining_amount == platform_fee` (no dust)
///
/// # Example
/// ```ignore
/// let (treasury, remaining) = calculate_treasury_split(100, 5000);
/// assert_eq!(treasury, 50);     // 50% of 100
/// assert_eq!(remaining, 50);    // remaining 50%
/// ```
pub fn calculate_treasury_split(platform_fee: i128, treasury_share_bps: i128) -> (i128, i128) {
    if platform_fee <= 0 || treasury_share_bps <= 0 {
        return (0, platform_fee.max(0));
    }

    if treasury_share_bps >= BPS_DENOMINATOR {
        return (platform_fee, 0);
    }

    let treasury_amount = platform_fee
        .saturating_mul(treasury_share_bps)
        .checked_div(BPS_DENOMINATOR)
        .unwrap_or(0);

    // Remaining amount is computed by subtraction to avoid dust
    let remaining = platform_fee.saturating_sub(treasury_amount);

    (treasury_amount, remaining)
}

// ============================================================================
// Validation Functions
// ============================================================================

/// Validate that a calculation produces no dust
///
/// Verifies that `investor_return + platform_fee == payment_amount`
///
/// # Returns
/// `true` if calculation is dust-free, `false` otherwise
pub fn verify_no_dust(investor_return: i128, platform_fee: i128, payment_amount: i128) -> bool {
    investor_return.saturating_add(platform_fee) == payment_amount
}

/// Validate calculation inputs
///
/// Checks that amounts are within valid bounds for safe calculation.
///
/// # Arguments
/// * `investment_amount` - Must be >= 0
/// * `payment_amount` - Must be >= 0
///
/// # Returns
/// `Ok(())` if valid, `Err(InvalidAmount)` otherwise
pub fn validate_calculation_inputs(
    investment_amount: i128,
    payment_amount: i128,
) -> Result<(), QuickLendXError> {
    if investment_amount < MIN_VALID_AMOUNT || payment_amount < MIN_VALID_AMOUNT {
        return Err(QuickLendXError::InvalidAmount);
    }
    Ok(())
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // Test helper to create a mock breakdown for comparison
    fn make_breakdown(
        investment: i128,
        payment: i128,
        gross_profit: i128,
        platform_fee: i128,
        investor_profit: i128,
        investor_return: i128,
        fee_bps: i128,
    ) -> ProfitFeeBreakdown {
        ProfitFeeBreakdown {
            investment_amount: investment,
            payment_amount: payment,
            gross_profit,
            platform_fee,
            investor_profit,
            investor_return,
            fee_bps_applied: fee_bps,
        }
    }

    #[test]
    fn test_basic_profit_calculation() {
        // Investment: 1000, Payment: 1100, Fee: 2% (200 bps)
        // Profit: 100, Fee: 2, Investor gets: 1098
        let (investor_return, platform_fee) = PlatformFee::calculate_with_fee_bps(1000, 1100, 200);
        assert_eq!(platform_fee, 2);
        assert_eq!(investor_return, 1098);
        assert!(verify_no_dust(investor_return, platform_fee, 1100));
    }

    #[test]
    fn test_exact_payment_no_profit() {
        // Payment equals investment - no profit, no fee
        let (investor_return, platform_fee) = PlatformFee::calculate_with_fee_bps(1000, 1000, 200);
        assert_eq!(platform_fee, 0);
        assert_eq!(investor_return, 1000);
        assert!(verify_no_dust(investor_return, platform_fee, 1000));
    }

    #[test]
    fn test_underpayment_loss() {
        // Payment less than investment - investor takes loss, no fee
        let (investor_return, platform_fee) = PlatformFee::calculate_with_fee_bps(1000, 900, 200);
        assert_eq!(platform_fee, 0);
        assert_eq!(investor_return, 900);
        assert!(verify_no_dust(investor_return, platform_fee, 900));
    }

    #[test]
    fn test_overpayment_high_profit() {
        // High profit scenario
        // Investment: 1000, Payment: 2000 (100% return), Fee: 2%
        // Profit: 1000, Fee: 20, Investor gets: 1980
        let (investor_return, platform_fee) = PlatformFee::calculate_with_fee_bps(1000, 2000, 200);
        assert_eq!(platform_fee, 20);
        assert_eq!(investor_return, 1980);
        assert!(verify_no_dust(investor_return, platform_fee, 2000));
    }

    #[test]
    fn test_zero_payment() {
        let (investor_return, platform_fee) = PlatformFee::calculate_with_fee_bps(1000, 0, 200);
        assert_eq!(platform_fee, 0);
        assert_eq!(investor_return, 0);
    }

    #[test]
    fn test_zero_investment() {
        // Zero investment means all payment is profit
        let (investor_return, platform_fee) = PlatformFee::calculate_with_fee_bps(0, 1000, 200);
        assert_eq!(platform_fee, 20); // 2% of 1000
        assert_eq!(investor_return, 980);
        assert!(verify_no_dust(investor_return, platform_fee, 1000));
    }

    #[test]
    fn test_zero_fee() {
        // No fee means investor gets full payment
        let (investor_return, platform_fee) = PlatformFee::calculate_with_fee_bps(1000, 1100, 0);
        assert_eq!(platform_fee, 0);
        assert_eq!(investor_return, 1100);
    }

    #[test]
    fn test_max_fee() {
        // Maximum 10% fee
        // Profit: 100, Fee: 10, Investor gets: 1090
        let (investor_return, platform_fee) = PlatformFee::calculate_with_fee_bps(1000, 1100, 1000);
        assert_eq!(platform_fee, 10);
        assert_eq!(investor_return, 1090);
        assert!(verify_no_dust(investor_return, platform_fee, 1100));
    }

    #[test]
    fn test_rounding_down_small_profit() {
        // Small profit where rounding matters
        // Profit: 1, Fee at 2%: 0.02 -> rounds to 0
        let (investor_return, platform_fee) = PlatformFee::calculate_with_fee_bps(1000, 1001, 200);
        assert_eq!(platform_fee, 0); // Rounds down to 0
        assert_eq!(investor_return, 1001);
        assert!(verify_no_dust(investor_return, platform_fee, 1001));
    }

    #[test]
    fn test_rounding_boundary() {
        // Profit where fee is exactly at boundary
        // Profit: 50, Fee at 2%: 1 (exactly)
        let (investor_return, platform_fee) = PlatformFee::calculate_with_fee_bps(1000, 1050, 200);
        assert_eq!(platform_fee, 1);
        assert_eq!(investor_return, 1049);
        assert!(verify_no_dust(investor_return, platform_fee, 1050));
    }

    #[test]
    fn test_rounding_just_below_boundary() {
        // Profit: 49, Fee at 2%: 0.98 -> rounds to 0
        let (investor_return, platform_fee) = PlatformFee::calculate_with_fee_bps(1000, 1049, 200);
        assert_eq!(platform_fee, 0); // Rounds down
        assert_eq!(investor_return, 1049);
        assert!(verify_no_dust(investor_return, platform_fee, 1049));
    }

    #[test]
    fn test_large_amounts() {
        // Large amounts to verify no overflow
        let investment = 1_000_000_000_000_i128; // 1 trillion
        let payment = 1_100_000_000_000_i128; // 10% return
        let (investor_return, platform_fee) =
            PlatformFee::calculate_with_fee_bps(investment, payment, 200);

        // Profit: 100 billion, Fee: 2 billion
        assert_eq!(platform_fee, 2_000_000_000);
        assert_eq!(investor_return, 1_098_000_000_000);
        assert!(verify_no_dust(investor_return, platform_fee, payment));
    }

    #[test]
    fn test_breakdown_complete() {
        let breakdown = PlatformFee::calculate_breakdown_with_fee_bps(1000, 1100, 200);

        assert_eq!(breakdown.investment_amount, 1000);
        assert_eq!(breakdown.payment_amount, 1100);
        assert_eq!(breakdown.gross_profit, 100);
        assert_eq!(breakdown.platform_fee, 2);
        assert_eq!(breakdown.investor_profit, 98);
        assert_eq!(breakdown.investor_return, 1098);
        assert_eq!(breakdown.fee_bps_applied, 200);

        // Verify no dust in breakdown
        assert_eq!(
            breakdown.investor_return + breakdown.platform_fee,
            breakdown.payment_amount
        );
    }

    #[test]
    fn test_breakdown_no_profit() {
        let breakdown = PlatformFee::calculate_breakdown_with_fee_bps(1000, 900, 200);

        assert_eq!(breakdown.gross_profit, 0);
        assert_eq!(breakdown.platform_fee, 0);
        assert_eq!(breakdown.investor_profit, 0);
        assert_eq!(breakdown.investor_return, 900);
    }

    #[test]
    fn test_treasury_split_basic() {
        // 50% split
        let (treasury, remaining) = calculate_treasury_split(100, 5000);
        assert_eq!(treasury, 50);
        assert_eq!(remaining, 50);
        assert_eq!(treasury + remaining, 100);
    }

    #[test]
    fn test_treasury_split_uneven() {
        // 30% split of 100 = 30 treasury, 70 remaining
        let (treasury, remaining) = calculate_treasury_split(100, 3000);
        assert_eq!(treasury, 30);
        assert_eq!(remaining, 70);
        assert_eq!(treasury + remaining, 100);
    }

    #[test]
    fn test_treasury_split_rounding() {
        // 33.33% split of 100 = 33 treasury, 67 remaining
        let (treasury, remaining) = calculate_treasury_split(100, 3333);
        assert_eq!(treasury, 33);
        assert_eq!(remaining, 67);
        assert_eq!(treasury + remaining, 100);
    }

    #[test]
    fn test_treasury_split_zero_fee() {
        let (treasury, remaining) = calculate_treasury_split(0, 5000);
        assert_eq!(treasury, 0);
        assert_eq!(remaining, 0);
    }

    #[test]
    fn test_treasury_split_zero_share() {
        let (treasury, remaining) = calculate_treasury_split(100, 0);
        assert_eq!(treasury, 0);
        assert_eq!(remaining, 100);
    }

    #[test]
    fn test_treasury_split_full_share() {
        let (treasury, remaining) = calculate_treasury_split(100, 10000);
        assert_eq!(treasury, 100);
        assert_eq!(remaining, 0);
    }

    #[test]
    fn test_validate_inputs_valid() {
        assert!(validate_calculation_inputs(1000, 1100).is_ok());
        assert!(validate_calculation_inputs(0, 0).is_ok());
    }

    #[test]
    fn test_validate_inputs_negative() {
        assert!(validate_calculation_inputs(-1, 1000).is_err());
        assert!(validate_calculation_inputs(1000, -1).is_err());
    }

    #[test]
    fn test_verify_no_dust_positive() {
        assert!(verify_no_dust(1098, 2, 1100));
        assert!(verify_no_dust(1000, 0, 1000));
    }

    #[test]
    fn test_verify_no_dust_negative() {
        assert!(!verify_no_dust(1097, 2, 1100)); // Off by 1
        assert!(!verify_no_dust(1099, 2, 1100)); // Off by 1
    }

    #[test]
    fn test_various_fee_percentages() {
        // Test different fee percentages
        let test_cases = [
            (100, 50),   // 0.5% -> 50 bps
            (100, 100),  // 1%
            (100, 200),  // 2%
            (100, 500),  // 5%
            (100, 1000), // 10%
        ];

        for (profit, fee_bps) in test_cases {
            let payment = 1000 + profit;
            let (investor_return, platform_fee) =
                PlatformFee::calculate_with_fee_bps(1000, payment, fee_bps);

            let expected_fee = profit * fee_bps / 10_000;
            assert_eq!(platform_fee, expected_fee, "Failed for fee_bps={}", fee_bps);
            assert!(verify_no_dust(investor_return, platform_fee, payment));
        }
    }
}
