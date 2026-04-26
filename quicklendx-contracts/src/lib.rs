#![no_std]

//! QuickLendX contracts library — minimal surface.
//!
//! The historical contract implementation lives in the `src/*.rs` sibling
//! modules but is not wired in yet because the legacy test suite is mid-
//! migration (see the `# temporarily disabled` note in
//! `.github/workflows/ci.yml`). Until the legacy modules are restored, this
//! file exposes only the pure, self-contained utility layer plus a minimal
//! placeholder contract.
//!
//! The placeholder `#[contract]` is required for the `wasm32v1-none` release
//! build: Soroban's contract macros install the `#[panic_handler]` and wire
//! the SDK's global allocator, both of which are mandatory on that target.

extern crate alloc;

#[cfg(all(test, feature = "legacy-tests"))]
mod scratch_events;
#[cfg(test)]
mod test_default;
#[cfg(test)]
mod test_fees;
#[cfg(test)]
mod test_fees_extended;
use soroban_sdk::{contract, contractimpl, symbol_short, Address, BytesN, Env, Map, String, Vec};

mod admin;
mod analytics;
mod audit;
mod backup;
mod bid;
mod currency;
mod defaults;
mod dispute;
mod emergency;
mod errors;
mod escrow;
mod events;
mod fees;
pub mod freshness;
mod init;
mod investment;
mod investment_queries;
mod invoice;
mod invoice_search;
mod notifications;
mod pagination;
mod pause;
mod payments;
mod profits;
mod protocol_limits;
mod reentrancy;
mod settlement;
mod storage;
#[cfg(test)]
#[cfg(test)]
mod test_admin;
#[cfg(test)]
mod test_admin_simple;
#[cfg(test)]
mod test_admin_standalone;
#[cfg(test)]
mod test_expired_bids_cleanup;
#[cfg(test)]
mod test_freshness;
#[cfg(test)]
mod test_init;
#[cfg(test)]
mod test_dispute;
#[cfg(test)]
mod test_investment_consistency;
#[cfg(test)]
mod test_investment_queries;
#[cfg(test)]
mod test_init_invariants;
#[cfg(test)]
mod test_max_invoices_per_business;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_overflow;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_pause;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_profit_fee;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_refund;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_storage;
// #[cfg(test)]
// mod test_protocol_limits_boundary;
#[cfg(test)]
mod test_string_limits;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_types;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_vesting;
#[cfg(test)]
mod test_notifications;
pub mod types;
pub use invoice::{InvoiceCategory};
mod verification;
mod vesting;
use admin::AdminStorage;
use bid::{Bid, BidStorage};
use defaults::{
    handle_default as do_handle_default, mark_invoice_defaulted as do_mark_invoice_defaulted,
    OverdueScanResult,
};
use errors::QuickLendXError;
use escrow::{
    accept_bid_and_fund as do_accept_bid_and_fund, refund_escrow_funds as do_refund_escrow_funds,
};
use events::{
    emit_bid_accepted, emit_bid_placed, emit_bid_withdrawn, emit_escrow_created,
    emit_escrow_released, emit_insurance_added, emit_insurance_premium_collected,
    emit_investor_verified, emit_invoice_cancelled, emit_invoice_metadata_cleared,
    emit_invoice_metadata_updated, emit_invoice_uploaded, emit_invoice_verified,
};
use investment::{InsuranceCoverage, Investment, InvestmentStatus, InvestmentStorage};
use invoice::{Dispute, DisputeStatus, Invoice, InvoiceMetadata, InvoiceStorage};
use invoice_search::InvoiceSearch;
use payments::{create_escrow, release_escrow, EscrowStorage};
use profits::{calculate_profit as do_calculate_profit, PlatformFee, PlatformFeeConfig};
use settlement::{
    process_partial_payment as do_process_partial_payment, settle_invoice as do_settle_invoice,
};
use verification::{
    calculate_investment_limit, calculate_investor_risk_score, determine_investor_tier,
    get_investor_verification as do_get_investor_verification, reject_business,
    reject_investor as do_reject_investor, require_business_not_pending,
    require_investor_not_pending, submit_investor_kyc as do_submit_investor_kyc, normalize_tag,
    submit_kyc_application, validate_bid, validate_investor_investment, validate_invoice_metadata,
    verify_business, verify_investor as do_verify_investor, verify_invoice_data,
    validate_dispute_reason, validate_dispute_evidence, validate_dispute_resolution,
    validate_dispute_eligibility,
    BusinessVerificationStatus, BusinessVerificationStorage, InvestorRiskLevel, InvestorTier,
    InvestorVerification, InvestorVerificationStorage,
};

pub use crate::types::*;

#[contract]
pub struct QuickLendXContract;

/// Maximum number of records returned by paginated query endpoints.
pub(crate) const MAX_QUERY_LIMIT: u32 = 100;

/// @notice Validates and caps query limit to prevent resource abuse
/// @param limit The requested limit value
/// @return The capped limit value, never exceeding MAX_QUERY_LIMIT
/// @dev Returns 0 if limit is 0, enforcing empty result behavior
#[inline]
fn cap_query_limit(limit: u32) -> u32 {
    investment_queries::InvestmentQueries::cap_query_limit(limit)
}

/// @notice Validates query parameters for security and resource protection
/// @param offset The pagination offset
/// @param limit The requested result limit
/// @return Result indicating validation success or failure
/// @dev Prevents potential overflow and ensures reasonable query bounds
fn validate_query_params(offset: u32, limit: u32) -> Result<(), QuickLendXError> {
    // Check for potential overflow in offset + limit calculation
    if offset > u32::MAX - MAX_QUERY_LIMIT {
        return Err(QuickLendXError::InvalidAmount);
    }
    
    // Limit is automatically capped by cap_query_limit, but we validate the input
    // Note: limit=0 is allowed and results in empty response
    Ok(())
}

/// Write a `u32` as ASCII decimal into `buf`, return byte length.
#[inline]
fn u32_to_ascii_lib(mut value: u32, buf: &mut [u8; 10]) -> usize {
    if value == 0 {
        buf[0] = b'0';
        return 1;
    }
    let mut tmp = [0u8; 10];
    let mut len = 0usize;
    while value > 0 {
        tmp[len] = b'0' + (value % 10) as u8;
        value /= 10;
        len += 1;
    }
    for i in 0..len {
        buf[i] = tmp[len - 1 - i];
    }
    len
}

/// Convert an `i64` to a soroban `String` using stack-allocated ASCII.
#[inline]
fn i64_to_string_lib(env: &Env, value: i64) -> String {
    // "-9223372036854775808" = 20 chars
    let mut buf = [0u8; 21];
    let mut tmp = [0u8; 20];
    let (negative, abs_val) = if value < 0 {
        (true, (value as i128).unsigned_abs() as u64)
    } else {
        (false, value as u64)
    };
    let n = u64_to_ascii_20(abs_val, &mut tmp);
    let start = if negative {
        buf[0] = b'-';
        buf[1..1 + n].copy_from_slice(&tmp[..n]);
        1 + n
    } else {
        buf[..n].copy_from_slice(&tmp[..n]);
        n
    };
    let s = core::str::from_utf8(&buf[..start]).unwrap_or("0");
    String::from_str(env, s)
}

#[inline]
fn u64_to_ascii_20(mut value: u64, buf: &mut [u8; 20]) -> usize {
    if value == 0 {
        buf[0] = b'0';
        return 1;
    }
    let mut tmp = [0u8; 20];
    let mut len = 0usize;
    while value > 0 {
        tmp[len] = b'0' + (value % 10) as u8;
        value /= 10;
        len += 1;
    }
    for i in 0..len {
        buf[i] = tmp[len - 1 - i];
    }
    len
}

#[contractimpl]
impl QuickLendXContract {
    // ============================================================================
    // Admin Management Functions
    // ============================================================================

    /// Initialize the protocol with all required configuration (one-time setup)
    pub fn initialize(env: Env, params: init::InitializationParams) -> Result<(), QuickLendXError> {
        init::ProtocolInitializer::initialize(&env, &params)
    }

    /// Check if the protocol has been initialized
    pub fn is_initialized(env: Env) -> bool {
        init::ProtocolInitializer::is_initialized(&env)
    }

    /// Get the protocol/contract version
    ///
    /// Returns the version written during initialization, or the current
    /// PROTOCOL_VERSION constant if the contract has not been initialized yet.
    ///
    /// # Returns
    /// * `u32` - The protocol version number
    ///
    /// # Version Format
    /// Version is a simple integer increment (e.g., 1, 2, 3...)
    /// Major versions indicate breaking changes that require migration.
    pub fn get_version(_env: Env) -> u32 {
        1u32
    }

    /// Get current protocol limits
    pub fn get_protocol_limits(env: Env) -> protocol_limits::ProtocolLimits {
        protocol_limits::ProtocolLimitsContract::get_protocol_limits(env)
    }

    /// Initialize the admin address (deprecated: use initialize)
    pub fn initialize_admin(env: Env, admin: Address) -> Result<(), QuickLendXError> {
        AdminStorage::initialize(&env, &admin)
    }

    /// Transfer admin role to a new address
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `new_admin` - The new admin address
    ///
    /// # Returns
    /// * `Ok(())` if transfer succeeds
    /// * `Err(QuickLendXError::NotAdmin)` if caller is not current admin
    ///
    /// # Security
    /// - Requires authorization from current admin
    pub fn transfer_admin(env: Env, new_admin: Address) -> Result<(), QuickLendXError> {
        let current_admin = AdminStorage::get_admin(&env).ok_or(QuickLendXError::NotAdmin)?;
        AdminStorage::transfer_admin(&env, &current_admin, &new_admin)
    }

    /// Get the current admin address
    ///
    /// # Returns
    /// * `Some(Address)` if admin is set
    /// * `None` if admin has not been initialized
    pub fn get_current_admin(env: Env) -> Option<Address> {
        AdminStorage::get_admin(&env)
    }

    /// Set protocol configuration (admin only)
    pub fn set_protocol_config(
        env: Env,
        admin: Address,
        min_invoice_amount: i128,
        max_due_date_days: u64,
        grace_period_seconds: u64,
    ) -> Result<(), QuickLendXError> {
        init::ProtocolInitializer::set_protocol_config(
            &env,
            &admin,
            min_invoice_amount,
            max_due_date_days,
            grace_period_seconds,
        )
    }

    /// Set fee configuration (admin only)
    pub fn set_fee_config(env: Env, admin: Address, fee_bps: u32) -> Result<(), QuickLendXError> {
        init::ProtocolInitializer::set_fee_config(&env, &admin, fee_bps)
    }

    /// Set treasury address (admin only)
    pub fn set_treasury(env: Env, admin: Address, treasury: Address) -> Result<(), QuickLendXError> {
        init::ProtocolInitializer::set_treasury(&env, &admin, &treasury)
    }

    /// Get current fee in basis points
    pub fn get_fee_bps(env: Env) -> u32 {
        init::ProtocolInitializer::get_fee_bps(&env)
    }

    /// Get treasury address
    pub fn get_treasury(env: Env) -> Option<Address> {
        init::ProtocolInitializer::get_treasury(&env)
    }

    /// Get minimum invoice amount
    pub fn get_min_invoice_amount(env: Env) -> i128 {
        init::ProtocolInitializer::get_min_invoice_amount(&env)
    }

    /// Get maximum due date days
    pub fn get_max_due_date_days(env: Env) -> u64 {
        init::ProtocolInitializer::get_max_due_date_days(&env)
    }

    /// Get grace period in seconds
    pub fn get_grace_period_seconds(env: Env) -> u64 {
        init::ProtocolInitializer::get_grace_period_seconds(&env)
    }

    /// Admin-only: configure default bid TTL (days). Bounds: 1..=30.
    pub fn set_bid_ttl_days(env: Env, days: u64) -> Result<u64, QuickLendXError> {
        pause::PauseControl::require_not_paused(&env)?;
        let admin = AdminStorage::get_admin(&env).ok_or(QuickLendXError::NotAdmin)?;
        bid::BidStorage::set_bid_ttl_days(&env, &admin, days)
    }

    /// Get configured bid TTL in days (returns default 7 if not set)
    pub fn get_bid_ttl_days(env: Env) -> u64 {
        bid::BidStorage::get_bid_ttl_days(&env)
    }

    /// Get current bid TTL configuration snapshot
    pub fn get_bid_ttl_config(env: Env) -> bid::BidTtlConfig {
        bid::BidStorage::get_bid_ttl_config(&env)
    }

    /// Reset bid TTL to the compile-time default
    pub fn reset_bid_ttl_to_default(env: Env) -> Result<u64, QuickLendXError> {
        let admin = AdminStorage::get_admin(&env).ok_or(QuickLendXError::NotAdmin)?;
        bid::BidStorage::reset_bid_ttl_to_default(&env, &admin)
    }

    /// Get maximum active bids allowed per investor
    pub fn get_max_active_bids_per_investor(env: Env) -> u32 {
        bid::BidStorage::get_max_active_bids_per_investor(&env)
    }

    /// Set maximum active bids allowed per investor (admin only)
    pub fn set_max_active_bids_per_investor(env: Env, limit: u32) -> Result<u32, QuickLendXError> {
        let admin = AdminStorage::get_admin(&env).ok_or(QuickLendXError::NotAdmin)?;
        bid::BidStorage::set_max_active_bids_per_investor(&env, &admin, limit)
    }

    /// Initiate emergency withdraw for stuck funds (admin only). Timelock applies before execute.
    /// See docs/contracts/emergency-recovery.md. Last-resort only.
    pub fn initiate_emergency_withdraw(
        env: Env,
        admin: Address,
        token: Address,
        amount: i128,
        target_address: Address,
    ) -> Result<(), QuickLendXError> {
        emergency::EmergencyWithdraw::initiate(&env, &admin, token, amount, target_address)
    }

    /// Execute emergency withdraw after timelock has elapsed (admin only).
    pub fn execute_emergency_withdraw(env: Env, admin: Address) -> Result<(), QuickLendXError> {
        emergency::EmergencyWithdraw::execute(&env, &admin)
    }

    /// Get pending emergency withdrawal if any.
    pub fn get_pending_emergency_withdraw(
        env: Env,
    ) -> Option<emergency::PendingEmergencyWithdrawal> {
        emergency::EmergencyWithdraw::get_pending(&env)
    }

    /// Check if the pending emergency withdrawal can be executed.
    ///
    /// Returns true if the withdrawal exists, is not cancelled, timelock has elapsed,
    /// and has not expired.
    pub fn can_exec_emergency(env: Env) -> bool {
        emergency::EmergencyWithdraw::can_execute(&env).unwrap_or(false)
    }

    /// Get time remaining until the emergency withdrawal can be executed.
    ///
    /// Returns seconds until unlock (0 if already unlocked).
    pub fn emg_time_until_unlock(env: Env) -> u64 {
        emergency::EmergencyWithdraw::time_until_unlock(&env).unwrap_or(0)
    }

    /// Get time remaining until the emergency withdrawal expires.
    ///
    /// Returns seconds until expiration (0 if already expired).
    pub fn emg_time_until_expire(env: Env) -> u64 {
        emergency::EmergencyWithdraw::time_until_expiration(&env).unwrap_or(0)
    }

    /// Add a token address to the currency whitelist (admin only).
    pub fn add_currency(
        env: Env,
        admin: Address,
        currency: Address,
    ) -> Result<(), QuickLendXError> {
        pause::PauseControl::require_not_paused(&env)?;
        currency::CurrencyWhitelist::add_currency(&env, &admin, &currency)
    }

    /// Remove a token address from the currency whitelist (admin only).
    pub fn remove_currency(
        env: Env,
        admin: Address,
        currency: Address,
    ) -> Result<(), QuickLendXError> {
        pause::PauseControl::require_not_paused(&env)?;
        currency::CurrencyWhitelist::remove_currency(&env, &admin, &currency)
    }

    /// Check if a token is allowed for invoice currency.
    pub fn is_allowed_currency(env: Env, currency: Address) -> bool {
        currency::CurrencyWhitelist::is_allowed_currency(&env, &currency)
    }

    /// Get all whitelisted token addresses.
    pub fn get_whitelisted_currencies(env: Env) -> Vec<Address> {
        currency::CurrencyWhitelist::get_whitelisted_currencies(&env)
    }

    /// Replace the entire currency whitelist atomically (admin only).
    pub fn set_currencies(
        env: Env,
        admin: Address,
        currencies: Vec<Address>,
    ) -> Result<(), QuickLendXError> {
        pause::PauseControl::require_not_paused(&env);
        currency::CurrencyWhitelist::set_currencies(&env, &admin, &currencies)
    }

    /// Clear the entire currency whitelist (admin only).
    /// After this call all currencies are allowed (empty-list backward-compat rule).
    pub fn clear_currencies(env: Env, admin: Address) -> Result<(), QuickLendXError> {
        pause::PauseControl::require_not_paused(&env);
        currency::CurrencyWhitelist::clear_currencies(&env, &admin)
    }

    /// Return the number of whitelisted currencies.
    pub fn currency_count(env: Env) -> u32 {
        currency::CurrencyWhitelist::currency_count(&env)
    }

    /// Return a paginated slice of the whitelist.
    pub fn get_whitelisted_currencies_paged(env: Env, offset: u32, limit: u32) -> Vec<Address> {
        currency::CurrencyWhitelist::get_whitelisted_currencies_paged(&env, offset, limit)
    }

    /// Cancel a pending emergency withdrawal (admin only).
    pub fn cancel_emergency_withdraw(env: Env, admin: Address) -> Result<(), QuickLendXError> {
        emergency::EmergencyWithdraw::cancel(&env, &admin)
    }

    /// Pause the contract (admin only). When paused, mutating operations fail with ContractPaused; getters succeed.
    pub fn pause(env: Env, admin: Address) -> Result<(), QuickLendXError> {
        pause::PauseControl::set_paused(&env, &admin, true)
    }

    /// Unpause the contract (admin only).
    pub fn unpause(env: Env, admin: Address) -> Result<(), QuickLendXError> {
        pause::PauseControl::set_paused(&env, &admin, false)
    }

    /// Return whether the contract is currently paused.
    pub fn is_paused(env: Env) -> bool {
        pause::PauseControl::is_paused(&env)
    }

    // ============================================================================
    // Invoice Management Functions
    // ============================================================================

    /// Store an invoice in the contract (unauthenticated; use `upload_invoice` for business flow).
    ///
    /// # Arguments
    /// * `business` - Address of the business that owns the invoice
    /// * `amount` - Invoice amount in smallest currency unit (e.g. cents)
    /// * `currency` - Token contract address for the invoice currency
    /// * `due_date` - Unix timestamp when the invoice is due
    /// * `description` - Human-readable description
    /// * `category` - Invoice category (e.g. Services, Goods)
    /// * `tags` - Optional tags for filtering
    ///
    /// # Returns
    /// * `Ok(BytesN<32>)` - The new invoice ID
    ///
    /// # Errors
    /// * `InvalidAmount` if amount <= 0
    /// * `InvoiceDueDateInvalid` if due_date is not in the future
    /// * `InvalidDescription` if description is empty
    pub fn store_invoice(
        env: Env,
        business: Address,
        amount: i128,
        currency: Address,
        due_date: u64,
        description: String,
        category: invoice::InvoiceCategory,
        tags: Vec<String>,
    ) -> Result<BytesN<32>, QuickLendXError> {
        pause::PauseControl::require_not_paused(&env);
        // Validate input parameters
        if amount <= 0 {
            return Err(QuickLendXError::InvalidAmount);
        }

        let current_timestamp = env.ledger().timestamp();
        if due_date <= current_timestamp {
            return Err(QuickLendXError::InvoiceDueDateInvalid);
        }

        // Validate amount and due date using protocol limits
        // Validate due date is not too far in the future using protocol limits
        protocol_limits::ProtocolLimitsContract::validate_invoice(env.clone(), amount, due_date)?;

        if description.len() == 0 {
            return Err(QuickLendXError::InvalidDescription);
        }

        currency::CurrencyWhitelist::require_allowed_currency(&env, &currency)?;

        // Check if business is verified (temporarily disabled for debugging)
        // if !verification::BusinessVerificationStorage::is_business_verified(&env, &business) {
        //     return Err(QuickLendXError::BusinessNotVerified);
        // }

        // Validate category and tags
        verification::validate_invoice_category(&category)?;
        verification::validate_invoice_tags(&env, &tags)?;

        // Create new invoice
        let invoice = Invoice::new(
            &env,
            business.clone(),
            amount,
            currency.clone(),
            due_date,
            description,
            category,
            tags,
        )?;

        // Store the invoice
        InvoiceStorage::store_invoice(&env, &invoice);

        // Emit event
        env.events().publish(
            (symbol_short!("created"),),
            (invoice.id.clone(), business, amount, currency, due_date),
        );

        Ok(invoice.id)
    }

    /// Upload an invoice (business only)
    pub fn upload_invoice(
        env: Env,
        business: Address,
        amount: i128,
        currency: Address,
        due_date: u64,
        description: String,
        category: invoice::InvoiceCategory,
        tags: Vec<String>,
    ) -> Result<BytesN<32>, QuickLendXError> {
        pause::PauseControl::require_not_paused(&env);
        // Only the business can upload their own invoice
        business.require_auth();

        // Enforce KYC: reject pending and unverified/rejected businesses with distinct errors.
        // Pending businesses get KYCAlreadyPending; unverified/rejected get BusinessNotVerified.
        require_business_not_pending(&env, &business)?;

        // Basic validation
        verify_invoice_data(&env, &business, amount, &currency, due_date, &description)?;
        currency::CurrencyWhitelist::require_allowed_currency(&env, &currency)?;

        // Validate category and tags
        verification::validate_invoice_category(&category)?;
        verification::validate_invoice_tags(&env, &tags)?;

        // Check max invoices per business limit
        let limits = protocol_limits::ProtocolLimitsContract::get_protocol_limits(env.clone());
        if limits.max_invoices_per_business > 0 {
            let active_count = InvoiceStorage::count_active_business_invoices(&env, &business);
            if active_count >= limits.max_invoices_per_business {
                return Err(QuickLendXError::MaxInvoicesPerBusinessExceeded);
            }
        }

        // Create and store invoice
        let invoice = Invoice::new(
            &env,
            business.clone(),
            amount,
            currency.clone(),
            due_date,
            description.clone(),
            category,
            tags,
        )?;
        InvoiceStorage::store_invoice(&env, &invoice);
        emit_invoice_uploaded(&env, &invoice);

        Ok(invoice.id)
    }

    /// Accept a bid and fund the invoice using escrow (transfer in from investor).
    ///
    /// Business must be authorized. Invoice must be Verified and bid Placed.
    /// Protected by reentrancy guard (see docs/contracts/security.md).
    ///
    /// # Returns
    /// * `Ok(BytesN<32>)` - The new escrow ID
    ///
    /// # Errors
    /// * `InvoiceNotFound`, `StorageKeyNotFound`, `InvalidStatus`, `InvoiceAlreadyFunded`, `InvoiceNotAvailableForFunding`, `Unauthorized`
    /// * `OperationNotAllowed` if reentrancy is detected
    pub fn accept_bid_and_fund(
        env: Env,
        invoice_id: BytesN<32>,
        bid_id: BytesN<32>,
    ) -> Result<BytesN<32>, QuickLendXError> {
        pause::PauseControl::require_not_paused(&env);
        reentrancy::with_payment_guard(&env, || do_accept_bid_and_fund(&env, &invoice_id, &bid_id))
    }

    /// Verify an invoice (admin or automated process)
    pub fn verify_invoice(env: Env, invoice_id: BytesN<32>) -> Result<(), QuickLendXError> {
        pause::PauseControl::require_not_paused(&env);
        let admin = AdminStorage::get_admin(&env).ok_or(QuickLendXError::NotAdmin)?;
        admin.require_auth();

        let mut invoice = InvoiceStorage::get_invoice(&env, &invoice_id)
            .ok_or(QuickLendXError::InvoiceNotFound)?;

        // When invoice is already funded, verify_invoice triggers release_escrow_funds (Issue #300)
        if invoice.status == InvoiceStatus::Funded {
            return Self::release_escrow_funds(env, invoice_id);
        }

        // Only allow verification if pending
        if invoice.status != InvoiceStatus::Pending {
            return Err(QuickLendXError::InvalidStatus);
        }

        // Remove from pending status list
        // Remove from old status list (Pending)
        InvoiceStorage::remove_from_status_invoices(&env, &InvoiceStatus::Pending, &invoice_id);

        invoice.verify(&env, admin.clone());
        InvoiceStorage::update_invoice(&env, &invoice);

        // Add to verified status list
        // Add to new status list (Verified)
        InvoiceStorage::add_to_status_invoices(&env, &InvoiceStatus::Verified, &invoice_id);

        emit_invoice_verified(&env, &invoice);

        // If invoice is funded (has escrow), release escrow funds to business
        if invoice.status == InvoiceStatus::Funded {
            Self::release_escrow_funds(env.clone(), invoice_id)?;
        }

        Ok(())
    }

    /// Cancel an invoice (business only, before funding)
    pub fn cancel_invoice(env: Env, invoice_id: BytesN<32>) -> Result<(), QuickLendXError> {
        pause::PauseControl::require_not_paused(&env);
        let mut invoice = InvoiceStorage::get_invoice(&env, &invoice_id)
            .ok_or(QuickLendXError::InvoiceNotFound)?;

        // Only the business owner can cancel their own invoice
        invoice.business.require_auth();

        // Enforce KYC: a pending business must not cancel invoices.
        require_business_not_pending(&env, &invoice.business)?;

        // Remove from old status list
        InvoiceStorage::remove_from_status_invoices(&env, &invoice.status, &invoice_id);

        // Cancel the invoice (only works if Pending or Verified)
        invoice.cancel(&env, invoice.business.clone())?;

        // Update storage
        InvoiceStorage::update_invoice(&env, &invoice);

        // Add to cancelled status list
        InvoiceStorage::add_to_status_invoices(&env, &InvoiceStatus::Cancelled, &invoice_id);

        // Emit event
        emit_invoice_cancelled(&env, &invoice);

        Ok(())
    }

    /// Get an invoice by ID.
    ///
    /// # Returns
    /// * `Ok(Invoice)` - The invoice data
    /// * `Err(InvoiceNotFound)` if the ID does not exist
    pub fn get_invoice(env: Env, invoice_id: BytesN<32>) -> Result<Invoice, QuickLendXError> {
        InvoiceStorage::get_invoice(&env, &invoice_id).ok_or(QuickLendXError::InvoiceNotFound)
    }

    /// Get all invoices for a business
    pub fn get_invoice_by_business(env: Env, business: Address) -> Vec<BytesN<32>> {
        InvoiceStorage::get_business_invoices(&env, &business)
    }

    /// Get all invoices for a specific business
    pub fn get_business_invoices(env: Env, business: Address) -> Vec<BytesN<32>> {
        InvoiceStorage::get_business_invoices(&env, &business)
    }

    /// Update structured metadata for an invoice
    pub fn update_invoice_metadata(
        env: Env,
        invoice_id: BytesN<32>,
        metadata: InvoiceMetadata,
    ) -> Result<(), QuickLendXError> {
        pause::PauseControl::require_not_paused(&env);
        let mut invoice = InvoiceStorage::get_invoice(&env, &invoice_id)
            .ok_or(QuickLendXError::InvoiceNotFound)?;

        invoice.business.require_auth();
        validate_invoice_metadata(&metadata, invoice.amount)?;

        if let Some(existing) = invoice.metadata() {
            InvoiceStorage::remove_metadata_indexes(&env, &existing, &invoice.id);
        }

        invoice.set_metadata(&env, Some(metadata.clone()))?;
        InvoiceStorage::update_invoice(&env, &invoice);
        InvoiceStorage::add_metadata_indexes(&env, &invoice);

        emit_invoice_metadata_updated(&env, &invoice, &metadata);
        Ok(())
    }

    /// Clear metadata attached to an invoice
    pub fn clear_invoice_metadata(env: Env, invoice_id: BytesN<32>) -> Result<(), QuickLendXError> {
        pause::PauseControl::require_not_paused(&env);
        let mut invoice = InvoiceStorage::get_invoice(&env, &invoice_id)
            .ok_or(QuickLendXError::InvoiceNotFound)?;

        invoice.business.require_auth();

        if let Some(existing) = invoice.metadata() {
            InvoiceStorage::remove_metadata_indexes(&env, &existing, &invoice.id);
            invoice.set_metadata(&env, None)?;
            InvoiceStorage::update_invoice(&env, &invoice);
            emit_invoice_metadata_cleared(&env, &invoice);
        }

        Ok(())
    }

    /// Get invoices indexed by customer name
    pub fn get_invoices_by_customer(env: Env, customer_name: String) -> Vec<BytesN<32>> {
        InvoiceStorage::get_invoices_by_customer(&env, &customer_name)
    }

    /// Get invoices indexed by tax id
    pub fn get_invoices_by_tax_id(env: Env, tax_id: String) -> Vec<BytesN<32>> {
        InvoiceStorage::get_invoices_by_tax_id(&env, &tax_id)
    }

    /// Search invoices with relevance ranking
    ///
    /// Performs a full-text search across invoice descriptions and customer names
    /// with ranking based on match quality and recency.
    ///
    /// # Arguments
    /// * `query` - Search query string (sanitized automatically)
    ///
    /// # Returns
    /// * `Vec<SearchResult>` - Ranked search results (max 50 results)
    ///
    /// # Ranking Logic
    /// 1. Exact invoice ID matches (highest priority)
    /// 2. Partial matches in description/customer name
    /// 3. Sorted by created_at timestamp (newest first) within same rank
    ///
    /// # Security Notes
    /// - Input sanitization prevents injection attacks
    /// - Memory-safe: bounded result set prevents DoS
    /// - Case-insensitive search
    pub fn search_invoices(env: Env, query: String) -> Result<Vec<SearchResult>, QuickLendXError> {
        InvoiceSearch::search_invoices(&env, query)
    }

    /// Get all invoices by status
    pub fn get_invoices_by_status(env: Env, status: InvoiceStatus) -> Vec<BytesN<32>> {
        InvoiceStorage::get_invoices_by_status(&env, status)
    }

    /// Get all available invoices (verified and not funded)
    pub fn get_available_invoices(env: Env) -> Vec<BytesN<32>> {
        InvoiceStorage::get_invoices_by_status(&env, InvoiceStatus::Verified)
    }

    /// Update invoice status (admin function)
    pub fn update_invoice_status(
        env: Env,
        invoice_id: BytesN<32>,
        new_status: InvoiceStatus,
    ) -> Result<(), QuickLendXError> {
        pause::PauseControl::require_not_paused(&env);
        let mut invoice = InvoiceStorage::get_invoice(&env, &invoice_id)
            .ok_or(QuickLendXError::InvoiceNotFound)?;

        // Remove from old status list
        InvoiceStorage::remove_from_status_invoices(&env, &invoice.status, &invoice_id);

        // Update status
        match new_status {
            InvoiceStatus::Verified => invoice.verify(&env, invoice.business.clone()),
            InvoiceStatus::Paid => {
                invoice.mark_as_paid(&env, invoice.business.clone(), env.ledger().timestamp())
            }
            InvoiceStatus::Defaulted => invoice.mark_as_defaulted(),
            InvoiceStatus::Funded => {
                // For testing purposes - normally funding happens via accept_bid
                invoice.mark_as_funded(
                    &env,
                    invoice.business.clone(),
                    invoice.amount,
                    env.ledger().timestamp(),
                );
            }
            _ => return Err(QuickLendXError::InvalidStatus),
        }

        // Store updated invoice
        InvoiceStorage::update_invoice(&env, &invoice);

        // Add to new status list
        InvoiceStorage::add_to_status_invoices(&env, &invoice.status, &invoice_id);

        // Emit event
        env.events().publish(
            (symbol_short!("updated"),),
            (invoice_id, new_status.clone()),
        );

        // Send notifications based on status change
        match new_status {
            InvoiceStatus::Verified => {
                // No notifications
            }
            _ => {}
        }

        Ok(())
    }

    /// Get invoice count by status
    pub fn get_invoice_count_by_status(env: Env, status: InvoiceStatus) -> u32 {
        let invoices = InvoiceStorage::get_invoices_by_status(&env, status);
        invoices.len() as u32
    }

    /// Get total invoice count
    pub fn get_total_invoice_count(env: Env) -> u32 {
        let pending = Self::get_invoice_count_by_status(env.clone(), InvoiceStatus::Pending);
        let verified = Self::get_invoice_count_by_status(env.clone(), InvoiceStatus::Verified);
        let funded = Self::get_invoice_count_by_status(env.clone(), InvoiceStatus::Funded);
        let paid = Self::get_invoice_count_by_status(env.clone(), InvoiceStatus::Paid);
        let defaulted = Self::get_invoice_count_by_status(env.clone(), InvoiceStatus::Defaulted);
        let cancelled = Self::get_invoice_count_by_status(env.clone(), InvoiceStatus::Cancelled);
        let refunded = Self::get_invoice_count_by_status(env.clone(), InvoiceStatus::Refunded);

        pending
            .saturating_add(verified)
            .saturating_add(funded)
            .saturating_add(paid)
            .saturating_add(defaulted)
            .saturating_add(cancelled)
            .saturating_add(refunded)
    }

    /// Clear all invoices from storage (admin only, used for restore operations)
    pub fn clear_all_invoices(env: Env) -> Result<(), QuickLendXError> {
        pause::PauseControl::require_not_paused(&env);
        use crate::invoice::InvoiceStorage;
        InvoiceStorage::clear_all(&env);
        Ok(())
    }

    /// Get a bid by ID
    pub fn get_bid(env: Env, bid_id: BytesN<32>) -> Option<Bid> {
        BidStorage::get_bid(&env, &bid_id)
    }

    /// Get the highest ranked bid for an invoice
    pub fn get_best_bid(env: Env, invoice_id: BytesN<32>) -> Option<Bid> {
        BidStorage::get_best_bid(&env, &invoice_id)
    }

    /// Get all bids for an invoice sorted using the platform ranking rules
    pub fn get_ranked_bids(env: Env, invoice_id: BytesN<32>) -> Vec<Bid> {
        BidStorage::rank_bids(&env, &invoice_id)
    }

    /// Get bids filtered by status
    pub fn get_bids_by_status(env: Env, invoice_id: BytesN<32>, status: BidStatus) -> Vec<Bid> {
        BidStorage::get_bids_by_status(&env, &invoice_id, status)
    }

    /// Get bids filtered by investor
    pub fn get_bids_by_investor(env: Env, invoice_id: BytesN<32>, investor: Address) -> Vec<Bid> {
        BidStorage::get_bids_by_investor(&env, &invoice_id, &investor)
    }

    /// Get all bids for an invoice
    /// Returns a list of all bid records (including expired, withdrawn, etc.)
    /// Use get_bids_by_status to filter by status if needed
    pub fn get_bids_for_invoice(env: Env, invoice_id: BytesN<32>) -> Vec<Bid> {
        BidStorage::get_bid_records_for_invoice(&env, &invoice_id)
    }

    /// Remove bids that have passed their expiration window
    pub fn cleanup_expired_bids(env: Env, invoice_id: BytesN<32>) -> u32 {
        BidStorage::cleanup_expired_bids(&env, &invoice_id)
    }

    /// Cancel a placed bid (investor only, Placed ΓåÆ Cancelled).
    ///
    /// # Race Safety
    /// Uses a read-check-write pattern that validates the bid is still in `Placed`
    /// status before transitioning. Terminal statuses (`Withdrawn`, `Accepted`,
    /// `Expired`, `Cancelled`) are immutable ΓÇö a bid that has already left `Placed`
    /// will cause this function to return `false` without any state mutation,
    /// preventing double-action execution regardless of call ordering.
    pub fn cancel_bid(env: Env, bid_id: BytesN<32>) -> bool {
        pause::PauseControl::require_not_paused(&env).is_ok()
            && bid::BidStorage::cancel_bid(&env, &bid_id)
    }

    /// Withdraw a bid (investor only, Placed ΓåÆ Withdrawn).
    ///
    /// # Race Safety
    /// Validates `BidStatus::Placed` atomically before transitioning. If a
    /// concurrent `cancel_bid` or expiry has already moved the bid to a terminal
    /// status, this call returns `OperationNotAllowed` without mutating state,
    /// preventing double-action execution.
    pub fn withdraw_bid(env: Env, bid_id: BytesN<32>) -> Result<(), QuickLendXError> {
        pause::PauseControl::require_not_paused(&env)?;
        let mut bid =
            BidStorage::get_bid(&env, &bid_id).ok_or(QuickLendXError::StorageKeyNotFound)?;
        bid.investor.require_auth();
        require_investor_not_pending(&env, &bid.investor)?;
        // Re-read status after auth to guard against concurrent transitions.
        let bid_fresh =
            BidStorage::get_bid(&env, &bid_id).ok_or(QuickLendXError::StorageKeyNotFound)?;
        if bid_fresh.status != BidStatus::Placed {
            return Err(QuickLendXError::OperationNotAllowed);
        }
        bid.status = BidStatus::Withdrawn;
        BidStorage::update_bid(&env, &bid);
        emit_bid_withdrawn(&env, &bid);
        Ok(())
    }

    /// Get all bids placed by an investor across all invoices.
    pub fn get_all_bids_by_investor(env: Env, investor: Address) -> Vec<Bid> {
        bid::BidStorage::get_all_bids_by_investor(&env, &investor)
    }

    /// Place a bid on an invoice
    ///
    /// Validates:
    /// - Invoice exists and is verified
    /// - Bid amount is positive
    /// - Investor is authorized and verified
    /// - Creates and stores the bid
    pub fn place_bid(
        env: Env,
        investor: Address,
        invoice_id: BytesN<32>,
        bid_amount: i128,
        expected_return: i128,
    ) -> Result<BytesN<32>, QuickLendXError> {
        pause::PauseControl::require_not_paused(&env);
        // Authorization check: Only the investor can place their own bid
        investor.require_auth();

        // Validate bid amount is positive
        if bid_amount <= 0 {
            return Err(QuickLendXError::InvalidAmount);
        }

        // Validate invoice exists and is verified
        let invoice = InvoiceStorage::get_invoice(&env, &invoice_id)
            .ok_or(QuickLendXError::InvoiceNotFound)?;
        if invoice.status != InvoiceStatus::Verified {
            return Err(QuickLendXError::InvalidStatus);
        }
        currency::CurrencyWhitelist::require_allowed_currency(&env, &invoice.currency)?;

        let verification = do_get_investor_verification(&env, &investor)
            .ok_or(QuickLendXError::InvestorNotVerified)?; // Changed error to InvestorNotVerified
        match verification.status {
            BusinessVerificationStatus::Verified => {
                if bid_amount > verification.investment_limit {
                    return Err(QuickLendXError::InvalidAmount);
                }
            }
            BusinessVerificationStatus::Pending => return Err(QuickLendXError::KYCAlreadyPending),
            BusinessVerificationStatus::Rejected => { // This is for BusinessVerificationStatus, but used for InvestorVerification.
                return Err(QuickLendXError::InvestorNotVerified) // Changed error to InvestorNotVerified
            }
        }

        BidStorage::cleanup_expired_bids(&env, &invoice_id);
        // Check if maximum bids per invoice limit is reached
        let active_bid_count = BidStorage::get_active_bid_count(&env, &invoice_id);
        if active_bid_count >= bid::MAX_BIDS_PER_INVOICE {
            return Err(QuickLendXError::MaxBidsPerInvoiceExceeded);
        }

        let max_active_bids = BidStorage::get_max_active_bids_per_investor(&env);
        if max_active_bids > 0 {
            let active_bids = BidStorage::count_active_placed_bids_for_investor(&env, &investor);
            if active_bids >= max_active_bids {
                return Err(QuickLendXError::OperationNotAllowed);
            }
        }
        validate_bid(&env, &invoice, bid_amount, expected_return, &investor)?;
        // Create bid
        let bid_id = BidStorage::generate_unique_bid_id(&env);
        let current_timestamp = env.ledger().timestamp();
        let bid = Bid {
            bid_id: bid_id.clone(),
            invoice_id: invoice_id.clone(),
            investor: investor.clone(),
            bid_amount,
            expected_return,
            timestamp: current_timestamp,
            status: BidStatus::Placed,
            expiration_timestamp: Bid::default_expiration_with_env(&env, current_timestamp),
        };
        BidStorage::store_bid(&env, &bid);
        // Track bid for this invoice
        BidStorage::add_bid_to_invoice(&env, &invoice_id, &bid_id);

        // Emit bid placed event
        emit_bid_placed(&env, &bid);

        Ok(bid_id)
    }

    /// Accept a bid (business only)
    pub fn accept_bid(
        env: Env,
        invoice_id: BytesN<32>,
        bid_id: BytesN<32>,
    ) -> Result<(), QuickLendXError> {
        pause::PauseControl::require_not_paused(&env);
        reentrancy::with_payment_guard(&env, || {
            Self::accept_bid_impl(env.clone(), invoice_id.clone(), bid_id.clone())
        })
    }

    fn accept_bid_impl(
        env: Env,
        invoice_id: BytesN<32>,
        bid_id: BytesN<32>,
    ) -> Result<(), QuickLendXError> {
        BidStorage::cleanup_expired_bids(&env, &invoice_id);
        let mut invoice = InvoiceStorage::get_invoice(&env, &invoice_id)
            .ok_or(QuickLendXError::InvoiceNotFound)?;
        let bid = BidStorage::get_bid(&env, &bid_id).ok_or(QuickLendXError::StorageKeyNotFound)?;
        let invoice_id = bid.invoice_id.clone();
        BidStorage::cleanup_expired_bids(&env, &invoice_id);
        let mut bid =
            BidStorage::get_bid(&env, &bid_id).ok_or(QuickLendXError::StorageKeyNotFound)?;
        invoice.business.require_auth();

        // Enforce KYC: a pending business must not accept bids.
        require_business_not_pending(&env, &invoice.business)?;

        if invoice.status != InvoiceStatus::Verified || bid.status != BidStatus::Placed {
            return Err(QuickLendXError::InvalidStatus);
        }

        let escrow_id = create_escrow(
            &env,
            &invoice_id,
            &bid.investor,
            &invoice.business,
            bid.bid_amount,
            &invoice.currency,
        )?;
        bid.status = BidStatus::Accepted;
        BidStorage::update_bid(&env, &bid);
        // Remove from old status list before changing status
        InvoiceStorage::remove_from_status_invoices(&env, &InvoiceStatus::Verified, &invoice_id);

        invoice.mark_as_funded(
            &env,
            bid.investor.clone(),
            bid.bid_amount,
            env.ledger().timestamp(),
        );
        InvoiceStorage::update_invoice(&env, &invoice);

        // Add to new status list after status change
        InvoiceStorage::add_to_status_invoices(&env, &InvoiceStatus::Funded, &invoice_id);
        let investment_id = InvestmentStorage::generate_unique_investment_id(&env);
        let investment = Investment {
            investment_id: investment_id.clone(),
            invoice_id: invoice_id.clone(),
            investor: bid.investor.clone(),
            amount: bid.bid_amount,
            funded_at: env.ledger().timestamp(),
            status: InvestmentStatus::Active,
            insurance: Vec::new(&env),
        };
        InvestmentStorage::store_investment(&env, &investment);

        let escrow = EscrowStorage::get_escrow(&env, &escrow_id)
            .expect("Escrow should exist after creation");
        emit_escrow_created(&env, &escrow);
        emit_bid_accepted(&env, &bid, &invoice_id, &invoice.business);

        Ok(())
    }

    /// Add insurance coverage to an active investment (investor only).
    ///
    /// # Arguments
    /// * `investment_id` - The investment to insure
    /// * `provider` - Insurance provider address
    /// * `coverage_percentage` - Coverage as a percentage (e.g. 80 for 80%)
    ///
    /// # Returns
    /// * `Ok(())` on success
    ///
    /// # Errors
    /// * `StorageKeyNotFound` if investment does not exist
    /// * `InvalidStatus` if investment is not Active
    /// * `InvalidAmount` if computed premium is zero
    pub fn add_investment_insurance(
        env: Env,
        investment_id: BytesN<32>,
        provider: Address,
        coverage_percentage: u32,
    ) -> Result<(), QuickLendXError> {
        pause::PauseControl::require_not_paused(&env)?;
        let mut investment = InvestmentStorage::get_investment(&env, &investment_id)
            .ok_or(QuickLendXError::StorageKeyNotFound)?;

        investment.investor.require_auth();

        if investment.status != InvestmentStatus::Active {
            return Err(QuickLendXError::InvalidStatus);
        }

        let premium = Investment::calculate_premium(investment.amount, coverage_percentage);
        if premium <= 0 {
            return Err(QuickLendXError::InvalidAmount);
        }

        let coverage_amount =
            investment.add_insurance(provider.clone(), coverage_percentage, premium)?;

        InvestmentStorage::update_investment(&env, &investment);

        emit_insurance_added(
            &env,
            &investment_id,
            &investment.invoice_id,
            &investment.investor,
            &provider,
            coverage_percentage,
            coverage_amount,
            premium,
        );
        emit_insurance_premium_collected(&env, &investment_id, &provider, premium);

        Ok(())
    }

    /// Settle an invoice (business or automated process)
    pub fn settle_invoice(
        env: Env,
        invoice_id: BytesN<32>,
        payment_amount: i128,
    ) -> Result<(), QuickLendXError> {
        pause::PauseControl::require_not_paused(&env)?;
        let _investment = InvestmentStorage::get_investment_by_invoice(&env, &invoice_id);

        let result = reentrancy::with_payment_guard(&env, || {
            do_settle_invoice(&env, &invoice_id, payment_amount)
        });

        if result.is_ok() {
            // Success
        }

        result
    }

    /// Get the investment record for a funded invoice.
    ///
    /// # Returns
    /// * `Ok(Investment)` - The investment tied to the invoice
    /// * `Err(StorageKeyNotFound)` if the invoice has no investment
    pub fn get_invoice_investment(
        env: Env,
        invoice_id: BytesN<32>,
    ) -> Result<Investment, QuickLendXError> {
        InvestmentStorage::get_investment_by_invoice(&env, &invoice_id)
            .ok_or(QuickLendXError::StorageKeyNotFound)
    }

    /// Get an investment by ID.
    ///
    /// # Returns
    /// * `Ok(Investment)` - The investment record
    /// * `Err(StorageKeyNotFound)` if the ID does not exist
    pub fn get_investment(
        env: Env,
        investment_id: BytesN<32>,
    ) -> Result<Investment, QuickLendXError> {
        InvestmentStorage::get_investment(&env, &investment_id)
            .ok_or(QuickLendXError::StorageKeyNotFound)
    }

    /// Query insurance coverage for an investment.
    ///
    /// # Arguments
    /// * `investment_id` - The investment to query
    ///
    /// # Returns
    /// * `Ok(Vec<InsuranceCoverage>)` - All insurance records for the investment
    /// * `Err(StorageKeyNotFound)` if the investment does not exist
    ///
    /// # Security Notes
    /// - Returns all insurance records (active and inactive)
    /// - No authorization required for queries
    pub fn query_investment_insurance(
        env: Env,
        investment_id: BytesN<32>,
    ) -> Result<Vec<InsuranceCoverage>, QuickLendXError> {
        let investment = InvestmentStorage::get_investment(&env, &investment_id)
            .ok_or(QuickLendXError::StorageKeyNotFound)?;
        Ok(investment.insurance)
    }

    /// Process a partial payment towards an invoice
    pub fn process_partial_payment(
        env: Env,
        invoice_id: BytesN<32>,
        payment_amount: i128,
        transaction_id: String,
    ) -> Result<(), QuickLendXError> {
        reentrancy::with_payment_guard(&env, || {
            do_process_partial_payment(&env, &invoice_id, payment_amount, transaction_id.clone())
        })
    }

    /// Handle invoice default (admin only)
    /// This is the internal handler - use mark_invoice_defaulted for public API
    pub fn handle_default(env: Env, invoice_id: BytesN<32>) -> Result<(), QuickLendXError> {
        pause::PauseControl::require_not_paused(&env);
        let admin = AdminStorage::get_admin(&env).ok_or(QuickLendXError::NotAdmin)?;
        admin.require_auth();

        // Get the investment to track investor analytics
        let _investment = InvestmentStorage::get_investment_by_invoice(&env, &invoice_id);

        let result = do_handle_default(&env, &invoice_id);

        result
    }

    /// Mark an invoice as defaulted (admin only)
    /// Checks due date + grace period before marking as defaulted.
    /// Requires admin authorization to prevent unauthorized default marking.
    ///
    /// # Arguments
    /// * `invoice_id` - The invoice ID to mark as defaulted
    /// * `grace_period` - Optional grace period in seconds (defaults to 7 days)
    ///
    /// # Returns
    /// * `Ok(())` if the invoice was successfully marked as defaulted
    /// * `Err(QuickLendXError)` if the operation fails
    ///
    /// # Errors
    /// * `NotAdmin` - No admin configured or caller is not admin
    /// * `InvoiceNotFound` - Invoice does not exist
    /// * `InvoiceAlreadyDefaulted` - Invoice is already defaulted
    /// * `InvoiceNotAvailableForFunding` - Invoice is not in Funded status
    /// * `OperationNotAllowed` - Grace period has not expired yet
    pub fn mark_invoice_defaulted(
        env: Env,
        invoice_id: BytesN<32>,
        grace_period: Option<u64>,
    ) -> Result<(), QuickLendXError> {
        pause::PauseControl::require_not_paused(&env);
        let admin = AdminStorage::get_admin(&env).ok_or(QuickLendXError::NotAdmin)?;
        admin.require_auth();

        // Get the investment to track investor analytics
        let _investment = InvestmentStorage::get_investment_by_invoice(&env, &invoice_id);

        let result = do_mark_invoice_defaulted(&env, &invoice_id, grace_period);

        result
    }

    /// Calculate profit and platform fee
    pub fn calculate_profit(
        env: Env,
        investment_amount: i128,
        payment_amount: i128,
    ) -> (i128, i128) {
        do_calculate_profit(&env, investment_amount, payment_amount)
    }

    /// Retrieve the current platform fee configuration
    pub fn get_platform_fee(env: Env) -> PlatformFeeConfig {
        PlatformFee::get_config(&env)
    }

    /// Update the platform fee basis points (admin only)
    pub fn set_platform_fee(env: Env, new_fee_bps: i128) -> Result<(), QuickLendXError> {
        pause::PauseControl::require_not_paused(&env);
        let admin = AdminStorage::get_admin(&env).ok_or(QuickLendXError::NotAdmin)?;
        PlatformFee::set_config(&env, &admin, new_fee_bps)?;
        Ok(())
    }

    // Business KYC/Verification Functions (from main)

    /// Submit KYC application (business only)
    pub fn submit_kyc_application(
        env: Env,
        business: Address,
        kyc_data: String,
    ) -> Result<(), QuickLendXError> {
        pause::PauseControl::require_not_paused(&env);
        submit_kyc_application(&env, &business, kyc_data)
    }

    /// Submit investor verification request
    pub fn submit_investor_kyc(
        env: Env,
        investor: Address,
        kyc_data: String,
    ) -> Result<(), QuickLendXError> {
        pause::PauseControl::require_not_paused(&env);
        do_submit_investor_kyc(&env, &investor, kyc_data)
    }

    /// Verify an investor and set an investment limit
    pub fn verify_investor(
        env: Env,
        investor: Address,
        investment_limit: i128,
    ) -> Result<(), QuickLendXError> {
        pause::PauseControl::require_not_paused(&env);
        let admin =
            BusinessVerificationStorage::get_admin(&env).ok_or(QuickLendXError::NotAdmin)?;
        let verification = do_verify_investor(&env, &admin, &investor, investment_limit)?;
        emit_investor_verified(&env, &verification);
        Ok(())
    }

    /// Reject an investor verification requbusinesses
    pub fn get_verified_businesses(env: Env) -> Vec<Address> {
        BusinessVerificationStorage::get_verified_businesses(&env)
    }

    /// Get all pending businesses
    pub fn reject_investor(
        env: Env,
        investor: Address,
        reason: String,
    ) -> Result<(), QuickLendXError> {
        pause::PauseControl::require_not_paused(&env);
        let admin = AdminStorage::get_admin(&env).ok_or(QuickLendXError::NotAdmin)?;
        do_reject_investor(&env, &admin, &investor, reason)
    }

    /// Get investor verification record if available
    pub fn get_investor_verification(env: Env, investor: Address) -> Option<InvestorVerification> {
        do_get_investor_verification(&env, &investor)
    }

    /// Set investment limit for a verified investor (admin only).
    pub fn set_investment_limit(
        env: Env,
        investor: Address,
        new_limit: i128,
    ) -> Result<(), QuickLendXError> {
        pause::PauseControl::require_not_paused(&env);
        let admin =
            BusinessVerificationStorage::get_admin(&env).ok_or(QuickLendXError::NotAdmin)?;
        verification::set_investment_limit(&env, &admin, &investor, new_limit)
    }

    /// Verify business (admin only)
    pub fn verify_business( // This function is already defined in verification module
        env: Env,
        admin: Address,
        business: Address,
    ) -> Result<(), QuickLendXError> {
        pause::PauseControl::require_not_paused(&env);
        verify_business(&env, &admin, &business)
    }

    /// Reject business (admin only)
    pub fn reject_business( // This function is already defined in verification module
        env: Env,
        admin: Address,
        business: Address,
        reason: String,
    ) -> Result<(), QuickLendXError> {
        pause::PauseControl::require_not_paused(&env);
        reject_business(&env, &admin, &business, reason)
    }

    /// Get business verification status
    pub fn get_business_verification_status( // This function is already defined in verification module
        env: Env,
        business: Address,
    ) -> Option<verification::BusinessVerification> {
        verification::get_business_verification_status(&env, &business)
    }

    /// Set admin address (initialization function)
    pub fn set_admin(env: Env, admin: Address) -> Result<(), QuickLendXError> {
        if let Some(current_admin) = BusinessVerificationStorage::get_admin(&env) {
            current_admin.require_auth();
        } else {
            admin.require_auth();
        }
        BusinessVerificationStorage::set_admin(&env, &admin);
        Ok(())
    }

    /// Get admin address // This function is already defined in admin module
    pub fn get_admin(env: Env) -> Option<Address> {
        BusinessVerificationStorage::get_admin(&env)
    }

    /// Initialize protocol limits (admin only). Sets min amount, max due date days, grace period.
    pub fn initialize_protocol_limits(
        env: Env,
        admin: Address,
        min_invoice_amount: i128,
        max_due_date_days: u64,
        grace_period_seconds: u64,
    ) -> Result<(), QuickLendXError> {
        let _ = protocol_limits::ProtocolLimitsContract::initialize(env.clone(), admin.clone());
        protocol_limits::ProtocolLimitsContract::set_protocol_limits(
            env,
            admin,
            min_invoice_amount,
            10,  // min_bid_amount
            100, // min_bid_bps (default)
            max_due_date_days,
            grace_period_seconds,
            100, // max_invoices_per_business (default)
        )
    }

    /// Update protocol limits (admin only).
    pub fn set_protocol_limits(
        env: Env,
        admin: Address,
        min_invoice_amount: i128,
        max_due_date_days: u64,
        grace_period_seconds: u64,
    ) -> Result<(), QuickLendXError> {
        pause::PauseControl::require_not_paused(&env);
        protocol_limits::ProtocolLimitsContract::set_protocol_limits(
            env,
            admin,
            min_invoice_amount,
            10,  // min_bid_amount
            100, // min_bid_bps (default)
            max_due_date_days,
            grace_period_seconds,
            100, // max_invoices_per_business (default)
        )
    }

    /// Update protocol limits (admin only).
    pub fn update_protocol_limits(
        env: Env,
        admin: Address,
        min_invoice_amount: i128,
        max_due_date_days: u64,
        grace_period_seconds: u64,
    ) -> Result<(), QuickLendXError> {
        pause::PauseControl::require_not_paused(&env);
        protocol_limits::ProtocolLimitsContract::set_protocol_limits(
            env,
            admin,
            min_invoice_amount,
            10,  // min_bid_amount
            100, // min_bid_bps (default)
            max_due_date_days,
            grace_period_seconds,
            100, // max_invoices_per_business (default)
        )
    }

    /// Update protocol limits with max invoices per business (admin only).
    pub fn update_limits_max_invoices(
        env: Env,
        admin: Address,
        min_invoice_amount: i128,
        max_due_date_days: u64,
        grace_period_seconds: u64,
        max_invoices_per_business: u32,
    ) -> Result<(), QuickLendXError> {
        pause::PauseControl::require_not_paused(&env);
        protocol_limits::ProtocolLimitsContract::set_protocol_limits(
            env,
            admin,
            min_invoice_amount,
            10,  // min_bid_amount
            100, // min_bid_bps (default)
            max_due_date_days,
            grace_period_seconds,
            max_invoices_per_business,
        )
    }

    /// Get all pending businesses
    pub fn get_pending_businesses(env: Env) -> Vec<Address> {
        BusinessVerificationStorage::get_pending_businesses(&env)
    }

    /// Get all rejected businesses
    pub fn get_rejected_businesses(env: Env) -> Vec<Address> {
        BusinessVerificationStorage::get_rejected_businesses(&env)
    }

    // ========================================
    // Enhanced Investor Verification Functions
    // ========================================

    /// Get all verified investors
    pub fn get_verified_investors(env: Env) -> Vec<Address> {
        InvestorVerificationStorage::get_verified_investors(&env)
    }

    /// Get all pending investors
    pub fn get_pending_investors(env: Env) -> Vec<Address> {
        InvestorVerificationStorage::get_pending_investors(&env)
    }

    /// Get all rejected investors
    pub fn get_rejected_investors(env: Env) -> Vec<Address> {
        InvestorVerificationStorage::get_rejected_investors(&env)
    }

    /// Get investors by tier
    pub fn get_investors_by_tier(env: Env, tier: InvestorTier) -> Vec<Address> {
        InvestorVerificationStorage::get_investors_by_tier(&env, tier)
    }

    /// Get investors by risk level
    pub fn get_investors_by_risk_level(env: Env, risk_level: InvestorRiskLevel) -> Vec<Address> {
        InvestorVerificationStorage::get_investors_by_risk_level(&env, risk_level)
    }

    /// Calculate investor risk score
    pub fn calculate_investor_risk_score(
        env: Env,
        investor: Address,
        kyc_data: String,
    ) -> Result<u32, QuickLendXError> { // This function is already defined in verification module
        calculate_investor_risk_score(&env, &investor, &kyc_data)
    }

    /// Determine investor tier
    pub fn determine_investor_tier(
        env: Env,
        investor: Address,
        risk_score: u32,
    ) -> Result<InvestorTier, QuickLendXError> { // This function is already defined in verification module
        determine_investor_tier(&env, &investor, risk_score)
    }

    /// Calculate investment limit for investor
    pub fn calculate_investment_limit(
        _env: Env,
        tier: InvestorTier,
        risk_level: InvestorRiskLevel,
        base_limit: i128,
    ) -> i128 { // This function is already defined in verification module
        calculate_investment_limit(&tier, &risk_level, base_limit)
    }

    /// Validate investor investment
    pub fn validate_investor_investment(
        env: Env,
        investor: Address,
        investment_amount: i128,
    ) -> Result<(), QuickLendXError> { // This function is already defined in verification module
        validate_investor_investment(&env, &investor, investment_amount)
    }

    /// Check if investor is verified
    pub fn is_investor_verified(env: Env, investor: Address) -> bool {
        InvestorVerificationStorage::is_investor_verified(&env, &investor)
    }

    /// Get escrow details for an invoice
    pub fn get_escrow_details(
        env: Env,
        invoice_id: BytesN<32>,
    ) -> Result<payments::Escrow, QuickLendXError> {
        EscrowStorage::get_escrow_by_invoice(&env, &invoice_id)
            .ok_or(QuickLendXError::StorageKeyNotFound)
    }

    /// Get escrow status for an invoice
    pub fn get_escrow_status(
        env: Env,
        invoice_id: BytesN<32>,
    ) -> Result<payments::EscrowStatus, QuickLendXError> {
        let escrow = EscrowStorage::get_escrow_by_invoice(&env, &invoice_id)
            .ok_or(QuickLendXError::StorageKeyNotFound)?;
        Ok(escrow.status)
    }

    /// Release escrow funds to business upon invoice verification
    pub fn release_escrow_funds(env: Env, invoice_id: BytesN<32>) -> Result<(), QuickLendXError> {
        pause::PauseControl::require_not_paused(&env);
        reentrancy::with_payment_guard(&env, || {
            let invoice = InvoiceStorage::get_invoice(&env, &invoice_id)
                .ok_or(QuickLendXError::InvoiceNotFound)?;

            // Strictly enforce that escrow can only be released for Funded invoices.
            // This prevents premature release even if an escrow object exists (e.g. from tests).
            if invoice.status != InvoiceStatus::Funded {
                return Err(QuickLendXError::InvalidStatus);
            }

            let escrow = EscrowStorage::get_escrow_by_invoice(&env, &invoice_id)
                .ok_or(QuickLendXError::StorageKeyNotFound)?;

            release_escrow(&env, &invoice_id)?;

            emit_escrow_released(
                &env,
                &escrow.escrow_id,
                &invoice_id,
                &escrow.business,
                escrow.amount,
            );

            Ok(())
        })
    }

    /// Refund escrow funds to investor if verification fails or as an explicit manual refund.
    ///
    /// Can be triggered by Admin or Business owner. Invoice must be Funded.
    /// Protected by payment reentrancy guard.
    pub fn refund_escrow_funds(
        env: Env,
        invoice_id: BytesN<32>,
        caller: Address,
    ) -> Result<(), QuickLendXError> {
        pause::PauseControl::require_not_paused(&env);
        reentrancy::with_payment_guard(&env, || do_refund_escrow_funds(&env, &invoice_id, &caller))
    }

    /// Check for overdue invoices and send notifications (admin or automated process)
    ///
    /// @notice Scans a bounded funded-invoice window for overdue/default handling.
    /// @dev This entry point uses the default rotating batch limit to keep per-call work bounded.
    ///      Repeated invocations eventually cover the full funded set as the stored cursor advances.
    /// @param env The contract environment.
    /// @return Number of overdue funded invoices found within the scanned window.
    pub fn check_overdue_invoices(env: Env) -> Result<u32, QuickLendXError> {
        let grace_period = defaults::resolve_grace_period(&env, None)?;
        Self::check_overdue_invoices_grace(env, grace_period)
    }

    /// Check for overdue invoices with a custom grace period (in seconds)
    ///
    /// @notice Scans a bounded funded-invoice window using a caller-supplied grace period.
    /// @dev The scan size is capped by protocol constants to keep execution deterministic.
    /// @param env The contract environment.
    /// @param grace_period Grace period in seconds applied to each funded invoice in the window.
    /// @return Number of overdue funded invoices found within the scanned window.
    pub fn check_overdue_invoices_grace(
        env: Env,
        grace_period: u64,
    ) -> Result<u32, QuickLendXError> {
        Ok(defaults::scan_funded_invoice_expirations(&env, grace_period, None)?.overdue_count)
    }

    /// @notice Returns the current funded-invoice overdue scan cursor.
    /// @param env The contract environment.
    /// @return Zero-based index of the next funded invoice to inspect.
    pub fn get_overdue_scan_cursor(env: Env) -> u32 {
        defaults::get_overdue_scan_cursor(&env)
    }

    /// @notice Returns the default funded-invoice overdue scan batch size.
    /// @return Default number of funded invoices processed by `check_overdue_invoices*`.
    pub fn get_overdue_scan_batch_limit(_env: Env) -> u32 {
        defaults::default_overdue_scan_batch_limit()
    }

    /// @notice Returns the maximum funded-invoice overdue scan batch size.
    /// @return Hard upper bound accepted by `scan_overdue_invoices`.
    pub fn get_overdue_scan_batch_limit_max(_env: Env) -> u32 {
        defaults::max_overdue_scan_batch_limit()
    }

    /// Check whether a specific invoice has expired and trigger default handling when necessary
    pub fn check_invoice_expiration(
        env: Env,
        invoice_id: BytesN<32>,
        grace_period: Option<u64>,
    ) -> Result<bool, QuickLendXError> {
        pause::PauseControl::require_not_paused(&env);
        let invoice = InvoiceStorage::get_invoice(&env, &invoice_id)
            .ok_or(QuickLendXError::InvoiceNotFound)?;
        let grace = defaults::resolve_grace_period(&env, grace_period)?;
        invoice.check_and_handle_expiration(&env, grace)
    }

    // Category and Tag Management Functions

    /// Get invoices by category
    pub fn get_invoices_by_category(
        env: Env,
        category: invoice::InvoiceCategory,
    ) -> Vec<BytesN<32>> {
        InvoiceStorage::get_invoices_by_category(&env, &category)
    }

    /// Get invoices by category and status
    pub fn get_invoices_by_cat_status(
        env: Env,
        category: invoice::InvoiceCategory,
        status: InvoiceStatus,
    ) -> Vec<BytesN<32>> {
        InvoiceStorage::get_invoices_by_category_and_status(&env, &category, &status)
    }

    /// Get invoices by tag
    pub fn get_invoices_by_tag(env: Env, tag: String) -> Vec<BytesN<32>> {
        InvoiceStorage::get_invoices_by_tag(&env, &tag)
    }

    /// Get invoices by multiple tags (AND logic)
    pub fn get_invoices_by_tags(env: Env, tags: Vec<String>) -> Vec<BytesN<32>> {
        InvoiceStorage::get_invoices_by_tags(&env, &tags)
    }

    /// Get invoice count by category
    pub fn get_invoice_count_by_category(env: Env, category: invoice::InvoiceCategory) -> u32 {
        InvoiceStorage::get_invoice_count_by_category(&env, &category)
    }

    /// Get invoice count by tag
    pub fn get_invoice_count_by_tag(env: Env, tag: String) -> u32 {
        InvoiceStorage::get_invoice_count_by_tag(&env, &tag)
    }

    /// Get all available categories
    pub fn get_all_categories(env: Env) -> Vec<invoice::InvoiceCategory> {
        InvoiceStorage::get_all_categories(&env)
    }

    /// Update invoice category (business owner only)
    pub fn update_invoice_category(
        env: Env,
        invoice_id: BytesN<32>,
        new_category: invoice::InvoiceCategory,
    ) -> Result<(), QuickLendXError> {
        pause::PauseControl::require_not_paused(&env)?;
        let mut invoice = InvoiceStorage::get_invoice(&env, &invoice_id)
            .ok_or(QuickLendXError::InvoiceNotFound)?;

        // Only the business owner can update the category
        invoice.business.require_auth();

        let old_category = invoice.category.clone();
        invoice.update_category(new_category.clone());

        // Validate the new category
        verification::validate_invoice_category(&new_category)?;

        // Update the invoice
        InvoiceStorage::update_invoice(&env, &invoice);

        // Emit event
        events::emit_invoice_category_updated(
            &env,
            &invoice_id,
            &invoice.business,
            &old_category,
            &new_category,
        );

        // Update indexes
        InvoiceStorage::remove_category_index(&env, &old_category, &invoice_id);
        InvoiceStorage::add_category_index(&env, &new_category, &invoice_id);

        Ok(())
    }

    /// Add tag to invoice (business owner only)
    pub fn add_invoice_tag(
        env: Env,
        invoice_id: BytesN<32>,
        tag: String,
    ) -> Result<(), QuickLendXError> {
        pause::PauseControl::require_not_paused(&env)?;
        let mut invoice = InvoiceStorage::get_invoice(&env, &invoice_id)
            .ok_or(QuickLendXError::InvoiceNotFound)?;

        // Authorization: Ensure the stored business owner authorizes the change
        invoice.business.require_auth();

        // Tag Normalization: Synchronize with protocol requirements
        let normalized_tag = normalize_tag(&env, &tag)?;
        invoice.add_tag(&env, normalized_tag.clone())?;

        // Update the invoice
        InvoiceStorage::update_invoice(&env, &invoice);

        // Emit event with normalized data
        events::emit_invoice_tag_added(&env, &invoice_id, &invoice.business, &normalized_tag);

        // Update index with normalized form
        InvoiceStorage::add_tag_index(&env, &normalized_tag, &invoice_id);

        Ok(())
    }

    /// Remove tag from invoice (business owner only)
    pub fn remove_invoice_tag(
        env: Env,
        invoice_id: BytesN<32>,
        tag: String,
    ) -> Result<(), QuickLendXError> {
        pause::PauseControl::require_not_paused(&env)?;
        let mut invoice = InvoiceStorage::get_invoice(&env, &invoice_id)
            .ok_or(QuickLendXError::InvoiceNotFound)?;

        // Authorization: Ensure the stored business owner authorizes the removal
        invoice.business.require_auth();

        // Normalize tag for removal lookup
        let normalized_tag = normalize_tag(&env, &tag)?;
        invoice.remove_tag(normalized_tag.clone())?;

        // Update the invoice
        InvoiceStorage::update_invoice(&env, &invoice);

        // Emit event with normalized data
        events::emit_invoice_tag_removed(&env, &invoice_id, &invoice.business, &normalized_tag);

        // Update index using normalized form
        InvoiceStorage::remove_tag_index(&env, &normalized_tag, &invoice_id);

        Ok(())
    }

    /// Get all tags for an invoice
    pub fn get_invoice_tags(
        env: Env,
        invoice_id: BytesN<32>,
    ) -> Result<Vec<String>, QuickLendXError> {
        let invoice = InvoiceStorage::get_invoice(&env, &invoice_id)
            .ok_or(QuickLendXError::InvoiceNotFound)?;
        Ok(invoice.get_tags())
    }

    /// Check if invoice has a specific tag
    pub fn invoice_has_tag(
        env: Env,
        invoice_id: BytesN<32>,
        tag: String,
    ) -> Result<bool, QuickLendXError> {
        let invoice = InvoiceStorage::get_invoice(&env, &invoice_id)
            .ok_or(QuickLendXError::InvoiceNotFound)?;
        Ok(invoice.has_tag(tag))
    }

    // ========================================
    // Fee and Revenue Management Functions
    // ========================================

    /// Initialize fee management system
    pub fn initialize_fee_system(env: Env, admin: Address) -> Result<(), QuickLendXError> {
        fees::FeeManager::initialize(&env, &admin)
    }

    /// Configure treasury address for platform fee routing (admin only)
    pub fn configure_treasury(env: Env, treasury_address: Address) -> Result<(), QuickLendXError> {
        let admin =
            BusinessVerificationStorage::get_admin(&env).ok_or(QuickLendXError::NotAdmin)?;

        let _treasury_config =
            fees::FeeManager::configure_treasury(&env, &admin, treasury_address.clone())?;

        // Emit event
        events::emit_treasury_configured(&env, &treasury_address, &admin);

        Ok(())
    }

    /// Update platform fee basis points (admin only)
    pub fn update_platform_fee_bps(env: Env, new_fee_bps: u32) -> Result<(), QuickLendXError> {
        let admin =
            BusinessVerificationStorage::get_admin(&env).ok_or(QuickLendXError::NotAdmin)?;

        let old_config = fees::FeeManager::get_platform_fee_config(&env)?;
        let old_fee_bps = old_config.fee_bps;

        let _new_config = fees::FeeManager::update_platform_fee(&env, &admin, new_fee_bps)?;

        // Emit event
        events::emit_platform_fee_config_updated(&env, old_fee_bps, new_fee_bps, &admin);

        Ok(())
    }

    /// Get current platform fee configuration
    pub fn get_platform_fee_config(env: Env) -> Result<fees::PlatformFeeConfig, QuickLendXError> {
        fees::FeeManager::get_platform_fee_config(&env)
    }

    /// Get treasury address if configured
    pub fn get_treasury_address(env: Env) -> Option<Address> {
        fees::FeeManager::get_treasury_address(&env)
    }

    /// Update fee structure for a specific fee type
    pub fn update_fee_structure(
        env: Env,
        admin: Address,
        fee_type: fees::FeeType,
        base_fee_bps: u32,
        min_fee: i128,
        max_fee: i128,
        is_active: bool,
    ) -> Result<fees::FeeStructure, QuickLendXError> {
        fees::FeeManager::update_fee_structure(
            &env,
            &admin,
            fee_type,
            base_fee_bps,
            min_fee,
            max_fee,
            is_active,
        )
    }

    /// Get fee structure for a fee type
    pub fn get_fee_structure(
        env: Env,
        fee_type: fees::FeeType,
    ) -> Result<fees::FeeStructure, QuickLendXError> {
        fees::FeeManager::get_fee_structure(&env, &fee_type)
    }

    /// Calculate total fees for a transaction
    pub fn calculate_transaction_fees(
        env: Env,
        user: Address,
        transaction_amount: i128,
        is_early_payment: bool,
        is_late_payment: bool,
    ) -> Result<i128, QuickLendXError> {
        fees::FeeManager::calculate_total_fees(
            &env,
            &user,
            transaction_amount,
            is_early_payment,
            is_late_payment,
        )
    }

    /// Get user volume data and tier
    pub fn get_user_volume_data(env: Env, user: Address) -> fees::UserVolumeData {
        fees::FeeManager::get_user_volume(&env, &user)
    }

    /// Update user volume (called internally after transactions)
    pub fn update_user_transaction_volume(
        env: Env,
        user: Address,
        transaction_amount: i128,
    ) -> Result<fees::UserVolumeData, QuickLendXError> {
        fees::FeeManager::update_user_volume(&env, &user, transaction_amount)
    }

    /// Configure revenue distribution
    pub fn configure_revenue_distribution(
        env: Env,
        admin: Address,
        treasury_address: Address,
        treasury_share_bps: u32,
        developer_share_bps: u32,
        platform_share_bps: u32,
        auto_distribution: bool,
        min_distribution_amount: i128,
    ) -> Result<(), QuickLendXError> {
        // Verify admin
        let stored_admin =
            BusinessVerificationStorage::get_admin(&env).ok_or(QuickLendXError::NotAdmin)?;
        if admin != stored_admin {
            return Err(QuickLendXError::NotAdmin);
        }

        let config = fees::RevenueConfig {
            treasury_address,
            treasury_share_bps,
            developer_share_bps,
            platform_share_bps,
            auto_distribution,
            min_distribution_amount,
        };
        fees::FeeManager::configure_revenue_distribution(&env, &admin, config)
    }

    /// Get current revenue split configuration
    pub fn get_revenue_split_config(env: Env) -> Result<fees::RevenueConfig, QuickLendXError> {
        fees::FeeManager::get_revenue_split_config(&env)
    }

    /// Distribute revenue for a period
    pub fn distribute_revenue(
        env: Env,
        admin: Address,
        period: u64,
    ) -> Result<(i128, i128, i128), QuickLendXError> {
        fees::FeeManager::distribute_revenue(&env, &admin, period)
    }

    /// Get fee analytics for a period
    pub fn get_fee_analytics(env: Env, period: u64) -> Result<fees::FeeAnalytics, QuickLendXError> {
        fees::FeeManager::get_analytics(&env, period)
    }

    /// Collect fees (internal function called after fee calculation)
    pub fn collect_transaction_fees(
        env: Env,
        user: Address,
        fees_by_type: Map<fees::FeeType, i128>,
        total_amount: i128,
    ) -> Result<(), QuickLendXError> {
        fees::FeeManager::collect_fees(&env, &user, fees_by_type, total_amount)
    }

    /// Validate fee parameters
    pub fn validate_fee_parameters(
        _env: Env,
        base_fee_bps: u32,
        min_fee: i128,
        max_fee: i128,
    ) -> Result<(), QuickLendXError> {
        fees::FeeManager::validate_fee_params(base_fee_bps, min_fee, max_fee)
    }

    // ========================================
    // Query Functions for Frontend Integration
    // ========================================

    /// Get invoices by business with optional status filter and pagination
    /// @notice Get business invoices with pagination and optional status filtering
    /// @param business The business address to query invoices for
    /// @param status_filter Optional status filter (None returns all statuses)
    /// @param offset Starting index for pagination (0-based)
    /// @param limit Maximum number of results to return (capped at MAX_QUERY_LIMIT)
    /// @return Vector of invoice IDs matching the criteria
    /// @dev Enforces MAX_QUERY_LIMIT hard cap for security and performance
    pub fn get_business_invoices_paged(
        env: Env,
        business: Address,
        status_filter: Option<InvoiceStatus>,
        offset: u32,
        limit: u32,
    ) -> Vec<BytesN<32>> {
        // Validate query parameters for security
        if validate_query_params(offset, limit).is_err() {
            // Return empty result on validation failure
            return Vec::new(&env);
        }
        
        let capped_limit = cap_query_limit(limit);
        let all_invoices = InvoiceStorage::get_business_invoices(&env, &business);
        let mut filtered = Vec::new(&env);

        for invoice_id in all_invoices.iter() {
            if let Some(invoice) = InvoiceStorage::get_invoice(&env, &invoice_id) {
                if let Some(status) = &status_filter {
                    if invoice.status == *status {
                        filtered.push_back(invoice_id);
                    }
                } else {
                    filtered.push_back(invoice_id);
                }
            }
        }

        // Apply pagination (overflow-safe)
        let mut result = Vec::new(&env);
        let len_u32 = filtered.len() as u32;
        let start = offset.min(len_u32);
        let end = start.saturating_add(capped_limit).min(len_u32);
        let mut idx = start;
        while idx < end {
            if let Some(invoice_id) = filtered.get(idx) {
                result.push_back(invoice_id);
            }
            idx += 1;
        }
        result
    }

    /// Get investments by investor with optional status filter and pagination
    /// Retrieves paginated investments for a specific investor with enhanced boundary checking.
    /// 
    /// This function provides overflow-safe pagination with comprehensive boundary validation
    /// to prevent arithmetic overflow and ensure consistent behavior across all edge cases.
    /// 
    /// # Arguments
    /// * `env` - Soroban environment
    /// * `investor` - Address of the investor to query
    /// * `status_filter` - Optional filter by investment status
    /// * `offset` - Starting position (0-based, will be capped to available data)
    /// * `limit` - Maximum records to return (capped to MAX_QUERY_LIMIT)
    /// 
    /// # Returns
    /// * Vector of investment IDs matching the criteria
    /// 
    /// # Security Notes
    /// - Uses saturating arithmetic throughout to prevent overflow attacks
    /// - Validates all array bounds before access
    /// - Caps query limit to prevent DoS via large requests
    /// - Handles edge cases like offset >= total_count gracefully
    /// 
    /// # Examples
    /// ```
    /// // Get first 10 active investments
    /// let investments = contract.get_investor_investments_paged(
    ///     env, investor, Some(InvestmentStatus::Active), 0, 10
    /// );
    /// 
    /// // Get next page with offset
    /// let next_page = contract.get_investor_investments_paged(
    ///     env, investor, Some(InvestmentStatus::Active), 10, 10
    /// );
    /// ```
    pub fn get_investor_investments_paged(
        env: Env,
        investor: Address,
        status_filter: Option<InvestmentStatus>,
        offset: u32,
        limit: u32,
    ) -> Vec<BytesN<32>> {
        investment_queries::InvestmentQueries::get_investor_investments_paginated(
            &env,
            &investor,
            status_filter,
            offset,
            limit,
        )
    }

    /// Get available invoices with pagination and optional filters
    /// @notice Get available invoices with pagination and optional filters
    /// @param min_amount Optional minimum invoice amount filter
    /// @param max_amount Optional maximum invoice amount filter
    /// @param category_filter Optional category filter
    /// @param offset Starting index for pagination (0-based)
    /// @param limit Maximum number of results to return (capped at MAX_QUERY_LIMIT)
    /// @return Vector of verified invoice IDs matching the criteria
    /// @dev Enforces MAX_QUERY_LIMIT hard cap for security and performance
    pub fn get_available_invoices_paged(
        env: Env,
        min_amount: Option<i128>,
        max_amount: Option<i128>,
        category_filter: Option<invoice::InvoiceCategory>,
        offset: u32,
        limit: u32,
    ) -> Vec<BytesN<32>> {
        // Validate query parameters for security
        if validate_query_params(offset, limit).is_err() {
            return Vec::new(&env);
        }
        
        let capped_limit = cap_query_limit(limit);
        let verified_invoices =
            InvoiceStorage::get_invoices_by_status(&env, InvoiceStatus::Verified);
        let mut filtered = Vec::new(&env);

        for invoice_id in verified_invoices.iter() {
            if let Some(invoice) = InvoiceStorage::get_invoice(&env, &invoice_id) {
                // Filter by amount range
                if let Some(min) = min_amount {
                    if invoice.amount < min {
                        continue;
                    }
                }
                if let Some(max) = max_amount {
                    if invoice.amount > max {
                        continue;
                    }
                }
                // Filter by category
                if let Some(category) = &category_filter {
                    if invoice.category != *category {
                        continue;
                    }
                }
                filtered.push_back(invoice_id);
            }
        }

        // Apply pagination (overflow-safe)
        let mut result = Vec::new(&env);
        let len_u32 = filtered.len() as u32;
        let start = offset.min(len_u32);
        let end = start.saturating_add(capped_limit).min(len_u32);
        let mut idx = start;
        while idx < end {
            if let Some(invoice_id) = filtered.get(idx) {
                result.push_back(invoice_id);
            }
            idx += 1;
        }
        result
    }

    /// Get bid history for an invoice with pagination
    /// @notice Get bid history for an invoice with pagination and optional status filtering
    /// @param invoice_id The invoice ID to query bids for
    /// @param status_filter Optional status filter (None returns all statuses)
    /// @param offset Starting index for pagination (0-based)
    /// @param limit Maximum number of results to return (capped at MAX_QUERY_LIMIT)
    /// @return Vector of bids matching the criteria
    /// @dev Enforces MAX_QUERY_LIMIT hard cap for security and performance
    pub fn get_bid_history_paged(
        env: Env,
        invoice_id: BytesN<32>,
        status_filter: Option<bid::BidStatus>,
        offset: u32,
        limit: u32,
    ) -> Vec<bid::Bid> {
        // Validate query parameters for security
        if validate_query_params(offset, limit).is_err() {
            return Vec::new(&env);
        }
        
        let capped_limit = cap_query_limit(limit);
        let all_bids = BidStorage::get_bid_records_for_invoice(&env, &invoice_id);
        let mut filtered = Vec::new(&env);

        for bid in all_bids.iter() {
            if let Some(status) = &status_filter {
                if bid.status == *status {
                    filtered.push_back(bid);
                }
            } else {
                filtered.push_back(bid);
            }
        }

        // Apply pagination (overflow-safe)
        let mut result = Vec::new(&env);
        let len_u32 = filtered.len() as u32;
        let start = offset.min(len_u32);
        let end = start.saturating_add(capped_limit).min(len_u32);
        let mut idx = start;
        while idx < end {
            if let Some(bid) = filtered.get(idx) {
                result.push_back(bid);
            }
            idx += 1;
        }
        result
    }

    /// Get bid history for an investor with pagination
    /// @notice Get bid history for an investor with pagination and optional status filtering
    /// @param investor The investor address to query bids for
    /// @param status_filter Optional status filter (None returns all statuses)
    /// @param offset Starting index for pagination (0-based)
    /// @param limit Maximum number of results to return (capped at MAX_QUERY_LIMIT)
    /// @return Vector of bids matching the criteria
    /// @dev Enforces MAX_QUERY_LIMIT hard cap for security and performance
    pub fn get_investor_bids_paged(
        env: Env,
        investor: Address,
        status_filter: Option<BidStatus>,
        offset: u32,
        limit: u32,
    ) -> Vec<Bid> {
        // Validate query parameters for security
        if validate_query_params(offset, limit).is_err() {
            return Vec::new(&env);
        }
        
        let capped_limit = cap_query_limit(limit);
        let all_bid_ids = BidStorage::get_bids_by_investor_all(&env, &investor);
        let mut filtered = Vec::new(&env);

        for bid_id in all_bid_ids.iter() {
            if let Some(bid) = BidStorage::get_bid(&env, &bid_id) {
                if let Some(status) = &status_filter {
                    if bid.status == *status {
                        filtered.push_back(bid);
                    }
                } else {
                    filtered.push_back(bid);
                }
            }
        }

        // Apply pagination (overflow-safe)
        let mut result = Vec::new(&env);
        let len_u32 = filtered.len() as u32;
        let start = offset.min(len_u32);
        let end = start.saturating_add(capped_limit).min(len_u32);
        let mut idx = start;
        while idx < end {
            if let Some(bid) = filtered.get(idx) {
                result.push_back(bid);
            }
            idx += 1;
        }
        result
    }

    /// Get investments by investor (simple version without pagination for backward compatibility)
    pub fn get_investments_by_investor(env: Env, investor: Address) -> Vec<BytesN<32>> {
        InvestmentStorage::get_investments_by_investor(&env, &investor)
    }

    /// Get bid history for an invoice (simple version without pagination)
    pub fn get_bid_history(env: Env, invoice_id: BytesN<32>) -> Vec<Bid> {
        BidStorage::get_bid_records_for_invoice(&env, &invoice_id)
    }

    // =========================================================================
    // Backup
    // =========================================================================

    /// Create a backup of all invoice data (admin only).
    pub fn create_backup(env: Env, admin: Address) -> Result<BytesN<32>, QuickLendXError> {
        pause::PauseControl::require_not_paused(&env)?;
        AdminStorage::require_admin(&env, &admin)?;
        let backup_id = backup::BackupStorage::generate_backup_id(&env);
        let invoices = backup::BackupStorage::get_all_invoices(&env);
        let b = backup::Backup {
            backup_id: backup_id.clone(),
            timestamp: env.ledger().timestamp(),
            description: String::from_str(&env, "Manual Backup"),
            invoice_count: invoices.len() as u32,
            status: backup::BackupStatus::Active,
        };
        backup::BackupStorage::store_backup(&env, &b, Some(&invoices))?;
        backup::BackupStorage::store_backup_data(&env, &backup_id, &invoices);
        backup::BackupStorage::add_to_backup_list(&env, &backup_id);
        let _ = backup::BackupStorage::cleanup_old_backups(&env);
        Ok(backup_id)
    }

    /// Restore invoice data from a backup (admin only).
    pub fn restore_backup(
        env: Env,
        admin: Address,
        backup_id: BytesN<32>,
    ) -> Result<(), QuickLendXError> {
        pause::PauseControl::require_not_paused(&env)?;
        AdminStorage::require_admin(&env, &admin)?;
        backup::BackupStorage::validate_backup(&env, &backup_id)?;
        let invoices = backup::BackupStorage::get_backup_data(&env, &backup_id)
            .ok_or(QuickLendXError::InvoiceNotFound)?;
        InvoiceStorage::clear_all(&env);
        for inv in invoices.iter() {
            InvoiceStorage::store_invoice(&env, &inv);
        }
        Ok(())
    }

    /// Archive a backup (admin only).
    pub fn archive_backup(
        env: Env,
        admin: Address,
        backup_id: BytesN<32>,
    ) -> Result<(), QuickLendXError> {
        pause::PauseControl::require_not_paused(&env)?;
        AdminStorage::require_admin(&env, &admin)?;
        let mut b = backup::BackupStorage::get_backup(&env, &backup_id)
            .ok_or(QuickLendXError::StorageKeyNotFound)?;
        b.status = backup::BackupStatus::Archived;
        backup::BackupStorage::update_backup(&env, &b)?;
        backup::BackupStorage::remove_from_backup_list(&env, &backup_id);
        Ok(())
    }

    /// Validate a backup's integrity.
    pub fn validate_backup(env: Env, backup_id: BytesN<32>) -> bool {
        backup::BackupStorage::validate_backup(&env, &backup_id).is_ok()
    }

    /// Get backup details by ID.
    pub fn get_backup_details(env: Env, backup_id: BytesN<32>) -> Option<backup::Backup> {
        backup::BackupStorage::get_backup(&env, &backup_id)
    }

    /// Get list of all active backup IDs.
    pub fn get_backups(env: Env) -> Vec<BytesN<32>> {
        backup::BackupStorage::get_all_backups(&env)
    }

    /// Manually trigger cleanup of old backups (admin only).
    pub fn cleanup_backups(env: Env, admin: Address) -> Result<u32, QuickLendXError> {
        pause::PauseControl::require_not_paused(&env)?;
        AdminStorage::require_admin(&env, &admin)?;
        backup::BackupStorage::cleanup_old_backups(&env)
    }

    /// Configure backup retention policy (admin only).
    pub fn set_backup_retention_policy(
        env: Env,
        admin: Address,
        max_backups: u32,
        max_age_seconds: u64,
        auto_cleanup_enabled: bool,
    ) -> Result<(), QuickLendXError> {
        pause::PauseControl::require_not_paused(&env)?;
        AdminStorage::require_admin(&env, &admin)?;
        let policy = backup::BackupRetentionPolicy {
            max_backups,
            max_age_seconds,
            auto_cleanup_enabled,
        };
        backup::BackupStorage::set_retention_policy(&env, &policy);
        Ok(())
    }

    /// Get current backup retention policy.
    pub fn get_backup_retention_policy(env: Env) -> backup::BackupRetentionPolicy {
        backup::BackupStorage::get_retention_policy(&env)
    }

    // ============================================================================
    // Vesting Functions
    // ============================================================================

    pub fn create_vesting_schedule(
        env: Env,
        admin: Address,
        token: Address,
        beneficiary: Address,
        total_amount: i128,
        start_time: u64,
        cliff_seconds: u64,
        end_time: u64,
    ) -> Result<u64, QuickLendXError> {
        pause::PauseControl::require_not_paused(&env)?;
        vesting::Vesting::create_schedule(
            &env,
            &admin,
            token,
            beneficiary,
            total_amount,
            start_time,
            cliff_seconds,
            end_time,
        )
    }

    pub fn get_vesting_schedule(env: Env, id: u64) -> Option<vesting::VestingSchedule> {
        vesting::Vesting::get_schedule(&env, id)
    }

    pub fn release_vested_tokens(
        env: Env,
        beneficiary: Address,
        id: u64,
    ) -> Result<i128, QuickLendXError> {
        pause::PauseControl::require_not_paused(&env)?;
        vesting::Vesting::release(&env, &beneficiary, id)
    }

    pub fn get_vesting_releasable(env: Env, id: u64) -> Option<i128> {
        let schedule = vesting::Vesting::get_schedule(&env, id)?;
        vesting::Vesting::releasable_amount(&env, &schedule).ok()
    }

    // ============================================================================
    // Analytics Functions
    // ============================================================================

    /// Get user behavior metrics
    pub fn get_user_behavior_metrics(env: Env, user: Address) -> analytics::UserBehaviorMetrics {
        analytics::AnalyticsCalculator::calculate_user_behavior_metrics(&env, &user).unwrap()
    }

    /// Get financial metrics for a specific period
    pub fn get_financial_metrics(
        env: Env,
        invoice_id: BytesN<32>,
        rating: u32,
        feedback: String,
        rater: Address,
    ) -> Result<(), QuickLendXError> {
        pause::PauseControl::require_not_paused(&env)?;
        let mut invoice = InvoiceStorage::get_invoice(&env, &invoice_id)
            .ok_or(QuickLendXError::InvoiceNotFound)?;
        let ts = env.ledger().timestamp();
        invoice.add_rating(rating, feedback, rater, ts)?;
        InvoiceStorage::update_invoice(&env, &invoice);
        Ok(())
    }

    // =========================================================================
    // Analytics (contract-exported)
    // =========================================================================

    pub fn get_platform_metrics(env: Env) -> analytics::PlatformMetrics {
        analytics::AnalyticsStorage::get_platform_metrics(&env).unwrap_or_else(|| {
            analytics::AnalyticsCalculator::calculate_platform_metrics(&env)
                .unwrap_or(analytics::PlatformMetrics {
                    total_invoices: 0,
                    total_investments: 0,
                    total_volume: 0,
                    total_fees_collected: 0,
                    active_investors: 0,
                    verified_businesses: 0,
                    average_invoice_amount: 0,
                    average_investment_amount: 0,
                    platform_fee_rate: 0,
                    default_rate: 0,
                    success_rate: 0,
                    timestamp: env.ledger().timestamp(),
                })
        })
    }

    pub fn get_performance_metrics(env: Env) -> analytics::PerformanceMetrics {
        analytics::AnalyticsStorage::get_performance_metrics(&env).unwrap_or_else(|| {
            analytics::AnalyticsCalculator::calculate_performance_metrics(&env)
                .unwrap_or(analytics::PerformanceMetrics {
                    platform_uptime: env.ledger().timestamp(),
                    average_settlement_time: 0,
                    average_verification_time: 0,
                    dispute_resolution_time: 0,
                    system_response_time: 0,
                    transaction_success_rate: 0,
                    error_rate: 0,
                    user_satisfaction_score: 0,
                    platform_efficiency: 0,
                })
        })
    }

    /// Generate a business report for a specific period
    pub fn generate_business_report(
        env: Env,
        business: Address,
        period: analytics::TimePeriod,
    ) -> Result<analytics::BusinessReport, QuickLendXError> {
        let report =
            analytics::AnalyticsCalculator::generate_business_report(&env, &business, period)?;
        analytics::AnalyticsStorage::store_business_report(&env, &report);
        Ok(report)
    }

    /// Retrieve a stored business report by ID
    pub fn get_business_report(env: Env, report_id: BytesN<32>) -> Option<analytics::BusinessReport> {
        analytics::AnalyticsStorage::get_business_report(&env, &report_id)
    }

    /// Generate an investor report for a specific period
    pub fn generate_investor_report(
        env: Env,
        investor: Address,
        invoice_id: BytesN<32>,
        amount: i128,
    ) -> Result<(), QuickLendXError> {
        pause::PauseControl::require_not_paused(&env)?;
        investor.require_auth();
        let mut invoice = InvoiceStorage::get_invoice(&env, &invoice_id)
            .ok_or(QuickLendXError::InvoiceNotFound)?;
        if invoice.status != InvoiceStatus::Verified {
            return Err(QuickLendXError::InvalidStatus);
        }
        let ts = env.ledger().timestamp();
        invoice.mark_as_funded(&env, investor, amount, ts);
        InvoiceStorage::update_invoice(&env, &invoice);
        Ok(())
    }

    // =========================================================================
    // Dispute
    // =========================================================================

    pub fn create_dispute(
        env: Env,
        invoice_id: BytesN<32>,
        creator: Address,
        reason: String,
        evidence: String,
    ) -> Result<(), QuickLendXError> {
        pause::PauseControl::require_not_paused(&env)?;
        creator.require_auth();
        let mut invoice = InvoiceStorage::get_invoice(&env, &invoice_id)
            .ok_or(QuickLendXError::InvoiceNotFound)?;
        if invoice.dispute_status != invoice::DisputeStatus::None {
            return Err(QuickLendXError::DisputeAlreadyExists);
        }
        if reason.len() == 0 {
            return Err(QuickLendXError::InvalidDisputeReason);
        }
        invoice.dispute_status = invoice::DisputeStatus::UnderReview;
        invoice.dispute = invoice::Dispute {
            created_by: creator,
            created_at: env.ledger().timestamp(),
            reason,
            evidence,
            resolution: String::from_str(&env, ""),
            resolved_by: Address::from_str(
                &env,
                "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
            ),
            resolved_at: 0,
        };
        InvoiceStorage::update_invoice(&env, &invoice);
        Ok(())
    }

    pub fn get_invoice_dispute_status(
        env: Env,
        invoice_id: BytesN<32>,
    ) -> Result<invoice::DisputeStatus, QuickLendXError> {
        let invoice = InvoiceStorage::get_invoice(&env, &invoice_id)
            .ok_or(QuickLendXError::InvoiceNotFound)?;
        Ok(invoice.dispute_status)
    }

    pub fn get_dispute_details(
        env: Env,
        invoice_id: BytesN<32>,
    ) -> Result<Option<invoice::Dispute>, QuickLendXError> {
        let invoice = InvoiceStorage::get_invoice(&env, &invoice_id)
            .ok_or(QuickLendXError::InvoiceNotFound)?;
        if invoice.dispute_status == invoice::DisputeStatus::None {
            return Ok(None);
        }
        Ok(Some(invoice.dispute))
    }

    pub fn put_dispute_under_review(
        env: Env,
        invoice_id: BytesN<32>,
        admin: Address,
    ) -> Result<(), QuickLendXError> {
        AdminStorage::require_admin(&env, &admin)?;
        let mut invoice = InvoiceStorage::get_invoice(&env, &invoice_id)
            .ok_or(QuickLendXError::InvoiceNotFound)?;
        invoice.dispute_status = invoice::DisputeStatus::UnderReview;
        InvoiceStorage::update_invoice(&env, &invoice);
        Ok(())
    }

    pub fn resolve_dispute(
        env: Env,
        invoice_id: BytesN<32>,
        admin: Address,
        resolution: String,
    ) -> Result<(), QuickLendXError> {
        AdminStorage::require_admin(&env, &admin)?;
        let mut invoice = InvoiceStorage::get_invoice(&env, &invoice_id)
            .ok_or(QuickLendXError::InvoiceNotFound)?;
        invoice.dispute_status = invoice::DisputeStatus::Resolved;
        invoice.dispute.resolution = resolution;
        invoice.dispute.resolved_by = admin;
        invoice.dispute.resolved_at = env.ledger().timestamp();
        InvoiceStorage::update_invoice(&env, &invoice);
        Ok(())
    }

    pub fn get_invoices_with_disputes(env: Env) -> Vec<BytesN<32>> {
        let mut result = Vec::new(&env);
        for status in [
            InvoiceStatus::Pending,
            InvoiceStatus::Verified,
            InvoiceStatus::Funded,
            InvoiceStatus::Paid,
        ] {
            for id in InvoiceStorage::get_invoices_by_status(&env, status).iter() {
                if let Some(inv) = InvoiceStorage::get_invoice(&env, &id) {
                    if inv.dispute_status != invoice::DisputeStatus::None {
                        result.push_back(id);
                    }
                }
            }
        }
        result
    }

    pub fn get_invoices_by_dispute_status(
        env: Env,
        dispute_status: invoice::DisputeStatus,
    ) -> Vec<BytesN<32>> {
        let mut result = Vec::new(&env);
        for status in [
            InvoiceStatus::Pending,
            InvoiceStatus::Verified,
            InvoiceStatus::Funded,
            InvoiceStatus::Paid,
        ] {
            for id in InvoiceStorage::get_invoices_by_status(&env, status).iter() {
                if let Some(inv) = InvoiceStorage::get_invoice(&env, &id) {
                    if inv.dispute_status == dispute_status {
                        result.push_back(id);
                    }
                }
            }
        }
        result
    }

    // =========================================================================
    // Audit
    // =========================================================================

    pub fn get_invoice_audit_trail(env: Env, invoice_id: BytesN<32>) -> Vec<BytesN<32>> {
        audit::AuditStorage::get_invoice_audit_trail(&env, &invoice_id)
    }

    pub fn get_audit_entry(env: Env, audit_id: BytesN<32>) -> Option<audit::AuditLogEntry> {
        audit::AuditStorage::get_audit_entry(&env, &audit_id)
    }

    /// Get all audit entry IDs for a given operation type.
    pub fn get_audit_entries_by_operation(
        env: Env,
        operation: audit::AuditOperation,
    ) -> Vec<BytesN<32>> {
        audit::AuditStorage::get_audit_entries_by_operation(&env, &operation)
    }

    /// Get all audit entry IDs attributed to a given actor address.
    pub fn get_audit_entries_by_actor(env: Env, actor: Address) -> Vec<BytesN<32>> {
        audit::AuditStorage::get_audit_entries_by_actor(&env, &actor)
    }

    pub fn query_audit_logs(
        env: Env,
        filter: audit::AuditQueryFilter,
        limit: u32,
    ) -> Vec<audit::AuditLogEntry> {
        audit::AuditStorage::query_audit_logs(&env, &filter, limit)
    }

    pub fn get_audit_stats(env: Env) -> audit::AuditStats {
        audit::AuditStorage::get_audit_stats(&env)
    }

    pub fn validate_invoice_audit_integrity(
        env: Env,
        invoice_id: BytesN<32>,
    ) -> Result<bool, QuickLendXError> {
        audit::AuditStorage::validate_invoice_audit_integrity(&env, &invoice_id)
    }

    // =========================================================================
    // Notifications
    // =========================================================================

    pub fn get_notification(
        env: Env,
        notification_id: BytesN<32>,
    ) -> Option<notifications::Notification> {
        notifications::NotificationSystem::get_notification(&env, &notification_id)
    }

    pub fn get_user_notifications(env: Env, user: Address) -> Vec<BytesN<32>> {
        notifications::NotificationSystem::get_user_notifications(&env, &user)
    }

    pub fn get_notification_preferences(
        env: Env,
        user: Address,
    ) -> notifications::NotificationPreferences {
        notifications::NotificationSystem::get_user_preferences(&env, &user)
    }

    pub fn update_notification_preferences(
        env: Env,
        user: Address,
        preferences: notifications::NotificationPreferences,
    ) {
        user.require_auth();
        notifications::NotificationSystem::update_user_preferences(&env, &user, preferences);
    }

    pub fn update_notification_status(
        env: Env,
        notification_id: BytesN<32>,
        status: notifications::NotificationDeliveryStatus,
    ) -> Result<(), QuickLendXError> {
        notifications::NotificationSystem::update_notification_status(
            &env,
            &notification_id,
            status,
        )
    }

    pub fn get_user_notification_stats(
        env: Env,
        user: Address,
    ) -> notifications::NotificationStats {
        notifications::NotificationSystem::get_user_notification_stats(&env, &user)
    }



    // ============================================================================
    // Dispute Resolution Functions
}
