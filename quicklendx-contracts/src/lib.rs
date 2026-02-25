#![no_std]
// QuickLendX Protocol - Invoice Financing Smart Contract
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
mod init;
mod investment;
mod invoice;
mod notifications;
mod payments;
mod profits;
mod protocol_limits;
mod reentrancy;
mod settlement;
pub mod types;
#[cfg(test)]
mod storage;
#[cfg(test)]
mod test_string_limits;
#[cfg(test)]
mod test_admin;
#[cfg(test)]
mod test_business_kyc;
#[cfg(test)]
mod test_cancel_refund;
#[cfg(test)]
mod test_dispute;
#[cfg(test)]
mod test_emergency_withdraw;
#[cfg(test)]
mod test_overflow;
#[cfg(test)]
mod test_profit_fee;
#[cfg(test)]
mod test_refund;
#[cfg(test)]
mod test_init;
#[cfg(test)]
mod test_storage;
mod verification;
use admin::AdminStorage;
use bid::{Bid, BidStatus, BidStorage};
use defaults::{
    create_dispute as do_create_dispute, get_dispute_details as do_get_dispute_details,
    get_invoices_by_dispute_status as do_get_invoices_by_dispute_status,
    get_invoices_with_disputes as do_get_invoices_with_disputes,
    handle_default as do_handle_default, mark_invoice_defaulted as do_mark_invoice_defaulted,
    put_dispute_under_review as do_put_dispute_under_review, resolve_dispute as do_resolve_dispute,
};
use errors::QuickLendXError;
use escrow::{
    accept_bid_and_fund as do_accept_bid_and_fund, refund_escrow_funds as do_refund_escrow_funds,
};
use events::{
    emit_audit_query, emit_audit_validation, emit_bid_accepted, emit_bid_placed,
    emit_bid_withdrawn, emit_escrow_created, emit_escrow_released, emit_insurance_added,
    emit_insurance_premium_collected, emit_investor_verified, emit_invoice_cancelled,
    emit_invoice_metadata_cleared, emit_invoice_metadata_updated, emit_invoice_uploaded,
    emit_invoice_verified,
};
use investment::{InsuranceCoverage, Investment, InvestmentStatus, InvestmentStorage};
use invoice::{DisputeStatus, Invoice, InvoiceMetadata, InvoiceStatus, InvoiceStorage};
use payments::{create_escrow, release_escrow, EscrowStorage};
use profits::{calculate_profit as do_calculate_profit, PlatformFee, PlatformFeeConfig};
use settlement::{
    process_partial_payment as do_process_partial_payment, settle_invoice as do_settle_invoice,
};
use verification::{
    calculate_investment_limit, calculate_investor_risk_score, determine_investor_tier,
    get_business_verification_status, get_investor_analytics,
    get_investor_verification as do_get_investor_verification, reject_business,
    reject_investor as do_reject_investor, submit_investor_kyc as do_submit_investor_kyc,
    submit_kyc_application, update_investor_analytics, validate_bid, validate_investor_investment,
    validate_invoice_metadata, verify_business, verify_investor as do_verify_investor,
    verify_invoice_data, BusinessVerificationStatus, BusinessVerificationStorage,
    InvestorRiskLevel, InvestorTier, InvestorVerification, InvestorVerificationStorage,
};

use crate::backup::{Backup, BackupRetentionPolicy, BackupStatus, BackupStorage};
use crate::notifications::{
    Notification, NotificationDeliveryStatus, NotificationPreferences, NotificationStats,
    NotificationSystem,
};
use analytics::{
    AnalyticsCalculator, AnalyticsStorage, BusinessReport, FinancialMetrics, InvestorAnalytics,
    InvestorPerformanceMetrics, InvestorReport, PerformanceMetrics, PlatformMetrics, TimePeriod,
    UserBehaviorMetrics,
};
use audit::{AuditLogEntry, AuditOperation, AuditQueryFilter, AuditStats, AuditStorage};

#[contract]
pub struct QuickLendXContract;

/// Maximum number of records returned by paginated query endpoints.
pub(crate) const MAX_QUERY_LIMIT: u32 = 100;

#[inline]
fn cap_query_limit(limit: u32) -> u32 {
    limit.min(MAX_QUERY_LIMIT)
}

#[contractimpl]
impl QuickLendXContract {
    // ============================================================================
    // Admin Management Functions
    // ============================================================================

    /// Initialize the protocol with all required configuration (one-time setup)
    pub fn initialize(
        env: Env,
        params: init::InitializationParams,
    ) -> Result<(), QuickLendXError> {
        params.admin.require_auth();
        init::ProtocolInitializer::initialize(&env, &params)
    }

    /// Check if the protocol has been initialized
    pub fn is_initialized(env: Env) -> bool {
        init::ProtocolInitializer::is_initialized(&env)
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
        AdminStorage::set_admin(&env, &current_admin, &new_admin)
    }

    /// Get the current admin address
    ///
    /// # Returns
    /// * `Some(Address)` if admin is set
    /// * `None` if admin has not been initialized
    pub fn get_current_admin(env: Env) -> Option<Address> {
        AdminStorage::get_admin(&env)
    }

    /// Admin-only: configure default bid TTL (days). Bounds: 1..=30.
    pub fn set_bid_ttl_days(env: Env, days: u64) -> Result<u64, QuickLendXError> {
        let admin = AdminStorage::get_admin(&env).ok_or(QuickLendXError::NotAdmin)?;
        bid::BidStorage::set_bid_ttl_days(&env, &admin, days)
    }

    /// Get configured bid TTL in days (returns default 7 if not set)
    pub fn get_bid_ttl_days(env: Env) -> u64 {
        bid::BidStorage::get_bid_ttl_days(&env)
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

    /// Add a token address to the currency whitelist (admin only).
    pub fn add_currency(
        env: Env,
        admin: Address,
        currency: Address,
    ) -> Result<(), QuickLendXError> {
        currency::CurrencyWhitelist::add_currency(&env, &admin, &currency)
    }

    /// Remove a token address from the currency whitelist (admin only).
    pub fn remove_currency(
        env: Env,
        admin: Address,
        currency: Address,
    ) -> Result<(), QuickLendXError> {
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
        currency::CurrencyWhitelist::set_currencies(&env, &admin, &currencies)
    }

    /// Clear the entire currency whitelist (admin only).
    /// After this call all currencies are allowed (empty-list backward-compat rule).
    pub fn clear_currencies(env: Env, admin: Address) -> Result<(), QuickLendXError> {
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

    // ============================================================================
    // Protocol Limits Management Functions
    // ============================================================================

    /// Initialize protocol limits with default values (one-time operation).
    ///
    /// Sets up system-wide constraints for invoice validation and default handling.
    /// Can only be called once. Subsequent calls will fail.
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
    /// # Security
    ///
    /// - Can only be called once
    /// - No authorization required for initial setup
    /// - Admin address is permanently stored
    pub fn initialize_protocol_limits(env: Env, admin: Address) -> Result<(), QuickLendXError> {
        protocol_limits::ProtocolLimitsContract::initialize(env, admin)
    }

    /// Update protocol limits (admin only).
    ///
    /// Allows the admin to update system-wide limits. All parameters are validated
    /// before being stored.
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
    /// - Requires admin authorization
    /// - All parameters validated before storage
    pub fn set_protocol_limits(
        env: Env,
        admin: Address,
        min_invoice_amount: i128,
        max_due_date_days: u64,
        grace_period_seconds: u64,
    ) -> Result<(), QuickLendXError> {
        protocol_limits::ProtocolLimitsContract::set_protocol_limits(
            env,
            admin,
            min_invoice_amount,
            max_due_date_days,
            grace_period_seconds,
        )
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
    pub fn get_protocol_limits(env: Env) -> protocol_limits::ProtocolLimits {
        protocol_limits::ProtocolLimitsContract::get_protocol_limits(env)
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
        // Validate input parameters
        if amount <= 0 {
            return Err(QuickLendXError::InvalidAmount);
        }

        let current_timestamp = env.ledger().timestamp();
        if due_date <= current_timestamp {
            return Err(QuickLendXError::InvoiceDueDateInvalid);
        }

        // Validate due date is not too far in the future using protocol limits
        if !protocol_limits::ProtocolLimitsContract::validate_invoice(env.clone(), amount, due_date) {
            return Err(QuickLendXError::InvoiceDueDateInvalid);
        }

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
        verification::validate_invoice_tags(&tags)?;

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
        // Only the business can upload their own invoice
        business.require_auth();

        // Check if business is verified
        let verification = get_business_verification_status(&env, &business);
        if verification.is_none()
            || !matches!(
                verification.unwrap().status,
                verification::BusinessVerificationStatus::Verified
            )
        {
            return Err(QuickLendXError::BusinessNotVerified);
        }

        // Basic validation
        verify_invoice_data(&env, &business, amount, &currency, due_date, &description)?;
        currency::CurrencyWhitelist::require_allowed_currency(&env, &currency)?;

        // Validate category and tags
        verification::validate_invoice_category(&category)?;
        verification::validate_invoice_tags(&tags)?;

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
        audit::log_invoice_uploaded(&env, invoice.id.clone(), business, invoice.amount);

        // Send notification
        let _ = NotificationSystem::notify_invoice_created(&env, &invoice);

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
        reentrancy::with_payment_guard(&env, || do_accept_bid_and_fund(&env, &invoice_id, &bid_id))
    }

    /// Verify an invoice (admin or automated process)
    pub fn verify_invoice(env: Env, invoice_id: BytesN<32>) -> Result<(), QuickLendXError> {
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
        audit::log_invoice_verified(&env, invoice_id.clone(), admin);

        // Send notification
        let _ = NotificationSystem::notify_invoice_verified(&env, &invoice);

        // If invoice is funded (has escrow), release escrow funds to business
        if invoice.status == InvoiceStatus::Funded {
            Self::release_escrow_funds(env.clone(), invoice_id)?;
        }

        Ok(())
    }

    /// Cancel an invoice (business only, before funding)
    pub fn cancel_invoice(env: Env, invoice_id: BytesN<32>) -> Result<(), QuickLendXError> {
        let mut invoice = InvoiceStorage::get_invoice(&env, &invoice_id)
            .ok_or(QuickLendXError::InvoiceNotFound)?;

        // Only the business owner can cancel their own invoice
        invoice.business.require_auth();

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
        audit::log_invoice_cancelled(&env, invoice_id, invoice.business.clone());

        // Send notification (optional - could notify interested investors)
        let _ = NotificationSystem::notify_invoice_status_changed(
            &env,
            &invoice,
            &InvoiceStatus::Pending, // Could be Pending or Verified
            &InvoiceStatus::Cancelled,
        );

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

    /// Get all invoices by status
    pub fn get_invoices_by_status(env: Env, status: InvoiceStatus) -> Vec<BytesN<32>> {
        InvoiceStorage::get_invoices_by_status(&env, &status)
    }

    /// Get all available invoices (verified and not funded)
    pub fn get_available_invoices(env: Env) -> Vec<BytesN<32>> {
        InvoiceStorage::get_invoices_by_status(&env, &InvoiceStatus::Verified)
    }

    /// Update invoice status (admin function)
    pub fn update_invoice_status(
        env: Env,
        invoice_id: BytesN<32>,
        new_status: InvoiceStatus,
    ) -> Result<(), QuickLendXError> {
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
                let _ = NotificationSystem::notify_invoice_verified(&env, &invoice);
            }
            InvoiceStatus::Paid => {
                let _ = NotificationSystem::notify_payment_received(&env, &invoice, invoice.amount);
            }
            InvoiceStatus::Defaulted => {
                let _ = NotificationSystem::notify_invoice_defaulted(&env, &invoice);
            }
            _ => {}
        }

        Ok(())
    }

    /// Get invoice count by status
    pub fn get_invoice_count_by_status(env: Env, status: InvoiceStatus) -> u32 {
        let invoices = InvoiceStorage::get_invoices_by_status(&env, &status);
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
    pub fn clear_all_invoices(env: &Env) -> Result<(), QuickLendXError> {
        use crate::invoice::InvoiceStorage;
        InvoiceStorage::clear_all(env);
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

    /// Cancel a placed bid (investor only, Placed → Cancelled).
    pub fn cancel_bid(env: Env, bid_id: BytesN<32>) -> bool {
        bid::BidStorage::cancel_bid(&env, &bid_id)
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
            .ok_or(QuickLendXError::BusinessNotVerified)?;
        match verification.status {
            BusinessVerificationStatus::Verified => {
                if bid_amount > verification.investment_limit {
                    return Err(QuickLendXError::InvalidAmount);
                }
            }
            BusinessVerificationStatus::Pending => return Err(QuickLendXError::KYCAlreadyPending),
            BusinessVerificationStatus::Rejected => {
                return Err(QuickLendXError::BusinessNotVerified)
            }
        }

        BidStorage::cleanup_expired_bids(&env, &invoice_id);
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
        audit::log_bid_placed(&env, invoice_id, investor, bid_amount, bid_id.clone());

        // Send notification for business about new bid
        let _ = NotificationSystem::notify_bid_received(&env, &invoice, &bid);

        Ok(bid_id)
    }

    /// Accept a bid (business only)
    pub fn accept_bid(
        env: Env,
        invoice_id: BytesN<32>,
        bid_id: BytesN<32>,
    ) -> Result<(), QuickLendXError> {
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
        audit::log_bid_accepted(
            &env,
            invoice_id.clone(),
            invoice.business.clone(),
            bid.bid_amount,
        );
        audit::log_escrow_created(
            &env,
            invoice_id.clone(),
            bid.investor.clone(),
            bid.bid_amount,
            escrow_id,
        );
        let _ = NotificationSystem::notify_bid_accepted(&env, &invoice, &bid);
        let _ = NotificationSystem::notify_invoice_status_changed(
            &env,
            &invoice,
            &InvoiceStatus::Verified,
            &InvoiceStatus::Funded,
        );

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

    /// Withdraw a bid (investor only, before acceptance)
    ///
    /// Validates:
    /// - Bid exists
    /// - Caller is the bid owner (authorization check)
    /// - Bid is in Placed status (prevents withdrawal of accepted/expired/withdrawn bids)
    /// - Updates bid status to Withdrawn
    pub fn withdraw_bid(env: Env, bid_id: BytesN<32>) -> Result<(), QuickLendXError> {
        // Get bid and validate it exists
        let mut bid =
            BidStorage::get_bid(&env, &bid_id).ok_or(QuickLendXError::StorageKeyNotFound)?;

        // Authorization check: Only the investor who owns the bid can withdraw it
        bid.investor.require_auth();

        // Status validation: Only allow withdrawal if bid is placed
        // Prevents withdrawal of accepted, withdrawn, or expired bids
        if bid.status != BidStatus::Placed {
            return Err(QuickLendXError::OperationNotAllowed);
        }
        bid.status = BidStatus::Withdrawn;
        BidStorage::update_bid(&env, &bid);

        // Emit bid withdrawn event
        emit_bid_withdrawn(&env, &bid);
        audit::log_bid_withdrawn(&env, bid.invoice_id.clone(), bid.investor.clone(), bid_id);

        Ok(())
    }

    /// Settle an invoice (business or automated process)
    pub fn settle_invoice(
        env: Env,
        invoice_id: BytesN<32>,
        payment_amount: i128,
    ) -> Result<(), QuickLendXError> {
        let investment = InvestmentStorage::get_investment_by_invoice(&env, &invoice_id);

        let result = reentrancy::with_payment_guard(&env, || {
            do_settle_invoice(&env, &invoice_id, payment_amount)
        });

        if result.is_ok() {
            if let Some(inv) = investment {
                let is_successful = payment_amount >= inv.amount;
                let _ = update_investor_analytics(&env, &inv.investor, inv.amount, is_successful);
            }
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
        do_process_partial_payment(&env, &invoice_id, payment_amount, transaction_id)
    }

    /// Handle invoice default (admin only)
    /// This is the internal handler - use mark_invoice_defaulted for public API
    pub fn handle_default(env: Env, invoice_id: BytesN<32>) -> Result<(), QuickLendXError> {
        let admin = AdminStorage::get_admin(&env).ok_or(QuickLendXError::NotAdmin)?;
        admin.require_auth();

        // Get the investment to track investor analytics
        let investment = InvestmentStorage::get_investment_by_invoice(&env, &invoice_id);

        let result = do_handle_default(&env, &invoice_id);

        // Update investor analytics for failed investment
        if result.is_ok() {
            if let Some(inv) = investment {
                let _ = update_investor_analytics(&env, &inv.investor, inv.amount, false);
            }
        }

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
        let admin = AdminStorage::get_admin(&env).ok_or(QuickLendXError::NotAdmin)?;
        admin.require_auth();

        // Get the investment to track investor analytics
        let investment = InvestmentStorage::get_investment_by_invoice(&env, &invoice_id);

        let result = do_mark_invoice_defaulted(&env, &invoice_id, grace_period);

        // Update investor analytics for failed investment
        if result.is_ok() {
            if let Some(inv) = investment {
                let _ = update_investor_analytics(&env, &inv.investor, inv.amount, false);
            }
        }

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
        let admin = AdminStorage::get_admin(&env).ok_or(QuickLendXError::NotAdmin)?;
        PlatformFee::set_config(&env, &admin, new_fee_bps)?;
        Ok(())
    }

    // Rating Functions (from feat-invoice_rating_system)

    /// Add a rating to an invoice (investor only)
    pub fn add_invoice_rating(
        env: Env,
        invoice_id: BytesN<32>,
        rating: u32,
        feedback: String,
        rater: Address,
    ) -> Result<(), QuickLendXError> {
        let mut invoice = InvoiceStorage::get_invoice(&env, &invoice_id)
            .ok_or(QuickLendXError::InvoiceNotFound)?;

        // Only the investor who funded the invoice can rate it
        rater.require_auth();

        invoice.add_rating(rating, feedback, rater.clone(), env.ledger().timestamp())?;
        InvoiceStorage::update_invoice(&env, &invoice);

        // Emit rating event
        env.events()
            .publish((symbol_short!("rated"),), (invoice_id, rating, rater));

        Ok(())
    }

    /// Get invoices with ratings above a threshold
    pub fn get_invoices_with_rating_above(env: Env, threshold: u32) -> Vec<BytesN<32>> {
        InvoiceStorage::get_invoices_with_rating_above(&env, threshold)
    }

    /// Get business invoices with ratings above a threshold
    pub fn get_business_rated_invoices(
        env: Env,
        business: Address,
        threshold: u32,
    ) -> Vec<BytesN<32>> {
        InvoiceStorage::get_business_invoices_with_rating_above(&env, &business, threshold)
    }

    /// Get count of invoices with ratings
    pub fn get_invoices_with_ratings_count(env: Env) -> u32 {
        InvoiceStorage::get_invoices_with_ratings_count(&env)
    }

    /// Get invoice rating statistics
    pub fn get_invoice_rating_stats(
        env: Env,
        invoice_id: BytesN<32>,
    ) -> Result<(Option<u32>, u32, Option<u32>, Option<u32>), QuickLendXError> {
        let invoice = InvoiceStorage::get_invoice(&env, &invoice_id)
            .ok_or(QuickLendXError::InvoiceNotFound)?;

        Ok((
            invoice.average_rating,
            invoice.total_ratings,
            invoice.get_highest_rating(),
            invoice.get_lowest_rating(),
        ))
    }

    // Business KYC/Verification Functions (from main)

    /// Submit KYC application (business only)
    pub fn submit_kyc_application(
        env: Env,
        business: Address,
        kyc_data: String,
    ) -> Result<(), QuickLendXError> {
        submit_kyc_application(&env, &business, kyc_data)
    }

    /// Submit investor kyc verification request
    pub fn submit_investor_kyc(
        env: Env,
        investor: Address,
        kyc_data: String,
    ) -> Result<(), QuickLendXError> {
        do_submit_investor_kyc(&env, &investor, kyc_data)
    }

    /// Verify an investor and set an investment limit
    pub fn verify_investor(
        env: Env,
        investor: Address,
        investment_limit: i128,
    ) -> Result<(), QuickLendXError> {
        let admin =
            BusinessVerificationStorage::get_admin(&env).ok_or(QuickLendXError::NotAdmin)?;
        let verification = do_verify_investor(&env, &admin, &investor, investment_limit)?;
        emit_investor_verified(&env, &verification);
        Ok(())
    }

    /// Reject an investor verification request
    pub fn reject_investor(
        env: Env,
        investor: Address,
        reason: String,
    ) -> Result<(), QuickLendXError> {
        let admin =
            BusinessVerificationStorage::get_admin(&env).ok_or(QuickLendXError::NotAdmin)?;
        do_reject_investor(&env, &admin, &investor, reason)
    }

    /// Get investor verification record if available
    pub fn get_investor_verification(env: Env, investor: Address) -> Option<InvestorVerification> {
        do_get_investor_verification(&env, &investor)
    }

    /// Set investment limit for a verified investor (admin only)
    pub fn set_investment_limit(
        env: Env,
        investor: Address,
        new_limit: i128,
    ) -> Result<(), QuickLendXError> {
        let admin =
            BusinessVerificationStorage::get_admin(&env).ok_or(QuickLendXError::NotAdmin)?;
        verification::set_investment_limit(&env, &admin, &investor, new_limit)
    }

    /// Verify business (admin only)
    pub fn verify_business(
        env: Env,
        admin: Address,
        business: Address,
    ) -> Result<(), QuickLendXError> {
        verify_business(&env, &admin, &business)
    }

    /// Reject business (admin only)
    pub fn reject_business(
        env: Env,
        admin: Address,
        business: Address,
        reason: String,
    ) -> Result<(), QuickLendXError> {
        reject_business(&env, &admin, &business, reason)
    }

    /// Get business verification status
    pub fn get_business_verification_status(
        env: Env,
        business: Address,
    ) -> Option<verification::BusinessVerification> {
        get_business_verification_status(&env, &business)
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

    /// Get admin address
    pub fn get_admin(env: Env) -> Option<Address> {
        BusinessVerificationStorage::get_admin(&env)
    }

    /// Initialize protocol limits (admin only). Sets min amount, max due date days, grace period.
    pub fn initialize_protocol_limits(
        env: Env,
        admin: Address,
        min_invoice_amount: i128,
        min_bid_amount: i128,
        min_bid_bps: u32,
        max_due_date_days: u64,
        grace_period_seconds: u64,
    ) -> Result<(), QuickLendXError> {
        let _ = protocol_limits::ProtocolLimitsContract::initialize(env.clone(), admin.clone());
        protocol_limits::ProtocolLimitsContract::set_protocol_limits(
            env,
            admin,
            min_invoice_amount,
            min_bid_amount,
            min_bid_bps,
            max_due_date_days,
            grace_period_seconds,
        )
    }

    /// Update protocol limits (admin only).
    pub fn set_protocol_limits(
        env: Env,
        admin: Address,
        min_invoice_amount: i128,
        min_bid_amount: i128,
        min_bid_bps: u32,
        max_due_date_days: u64,
        grace_period_seconds: u64,
    ) -> Result<(), QuickLendXError> {
        protocol_limits::ProtocolLimitsContract::set_protocol_limits(
            env,
            admin,
            min_invoice_amount,
            min_bid_amount,
            min_bid_bps,
            max_due_date_days,
            grace_period_seconds,
        )
    }

    /// Get all verified businesses
    pub fn get_verified_businesses(env: Env) -> Vec<Address> {
        BusinessVerificationStorage::get_verified_businesses(&env)
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
    ) -> Result<u32, QuickLendXError> {
        calculate_investor_risk_score(&env, &investor, &kyc_data)
    }

    /// Determine investor tier
    pub fn determine_investor_tier(
        env: Env,
        investor: Address,
        risk_score: u32,
    ) -> Result<InvestorTier, QuickLendXError> {
        determine_investor_tier(&env, &investor, risk_score)
    }

    /// Calculate investment limit for investor
    pub fn calculate_investment_limit(
        _env: Env,
        tier: InvestorTier,
        risk_level: InvestorRiskLevel,
        base_limit: i128,
    ) -> i128 {
        calculate_investment_limit(&tier, &risk_level, base_limit)
    }

    /// Update investor analytics after investment
    pub fn update_investor_analytics(
        env: Env,
        investor: Address,
        investment_amount: i128,
        is_successful: bool,
    ) -> Result<(), QuickLendXError> {
        update_investor_analytics(&env, &investor, investment_amount, is_successful)
    }

    /// Get investor analytics
    pub fn get_investor_analytics(
        env: Env,
        investor: Address,
    ) -> Result<InvestorVerification, QuickLendXError> {
        get_investor_analytics(&env, &investor)
    }

    /// Validate investor investment
    pub fn validate_investor_investment(
        env: Env,
        investor: Address,
        investment_amount: i128,
    ) -> Result<(), QuickLendXError> {
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
        reentrancy::with_payment_guard(&env, || {
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
        reentrancy::with_payment_guard(&env, || do_refund_escrow_funds(&env, &invoice_id, &caller))
    }

    ///== Notification Management Functions ==///
    /// Get a notification by ID
    pub fn get_notification(env: Env, notification_id: BytesN<32>) -> Option<Notification> {
        NotificationSystem::get_notification(&env, &notification_id)
    }

    /// Update notification delivery status
    pub fn update_notification_status(
        env: Env,
        notification_id: BytesN<32>,
        status: NotificationDeliveryStatus,
    ) -> Result<(), QuickLendXError> {
        NotificationSystem::update_notification_status(&env, &notification_id, status)
    }

    /// Get all notifications for a user
    pub fn get_user_notifications(env: Env, user: Address) -> Vec<BytesN<32>> {
        NotificationSystem::get_user_notifications(&env, &user)
    }

    /// Get user notification preferences
    pub fn get_notification_preferences(env: Env, user: Address) -> NotificationPreferences {
        NotificationSystem::get_user_preferences(&env, &user)
    }

    /// Update user notification preferences
    pub fn update_notification_preferences(
        env: Env,
        user: Address,
        preferences: NotificationPreferences,
    ) -> Result<(), QuickLendXError> {
        user.require_auth();
        NotificationSystem::update_user_preferences(&env, &user, preferences);
        Ok(())
    }

    /// Get notification statistics for a user
    pub fn get_user_notification_stats(env: Env, user: Address) -> NotificationStats {
        NotificationSystem::get_user_notification_stats(&env, &user)
    }

    /// Check for overdue invoices and send notifications (admin or automated process)
    pub fn check_overdue_invoices(env: Env) -> Result<u32, QuickLendXError> {
        let grace_period = defaults::resolve_grace_period(&env, None);
        Self::check_overdue_invoices_grace(env, grace_period)
    }

    /// Check for overdue invoices with a custom grace period (in seconds)
    pub fn check_overdue_invoices_grace(
        env: Env,
        grace_period: u64,
    ) -> Result<u32, QuickLendXError> {
        let current_timestamp = env.ledger().timestamp();
        let funded_invoices = InvoiceStorage::get_invoices_by_status(&env, &InvoiceStatus::Funded);
        let mut overdue_count = 0u32;

        for invoice_id in funded_invoices.iter() {
            if let Some(invoice) = InvoiceStorage::get_invoice(&env, &invoice_id) {
                if invoice.is_overdue(current_timestamp) {
                    let _ = NotificationSystem::notify_payment_overdue(&env, &invoice);
                    overdue_count += 1;
                }
                let _ = invoice.check_and_handle_expiration(&env, grace_period)?;
            }
        }

        Ok(overdue_count)
    }

    /// Check whether a specific invoice has expired and trigger default handling when necessary
    pub fn check_invoice_expiration(
        env: Env,
        invoice_id: BytesN<32>,
        grace_period: Option<u64>,
    ) -> Result<bool, QuickLendXError> {
        let invoice = InvoiceStorage::get_invoice(&env, &invoice_id)
            .ok_or(QuickLendXError::InvoiceNotFound)?;
        let grace = defaults::resolve_grace_period(&env, grace_period);
        invoice.check_and_handle_expiration(&env, grace)
    }

    /// Get audit trail for an invoice
    pub fn get_invoice_audit_trail(env: Env, invoice_id: BytesN<32>) -> Vec<BytesN<32>> {
        AuditStorage::get_invoice_audit_trail(&env, &invoice_id)
    }

    /// Get audit entry by ID
    pub fn get_audit_entry(
        env: Env,
        audit_id: BytesN<32>,
    ) -> Result<AuditLogEntry, QuickLendXError> {
        AuditStorage::get_audit_entry(&env, &audit_id).ok_or(QuickLendXError::AuditLogNotFound)
    }

    /// Query audit logs with filters
    pub fn query_audit_logs(env: Env, filter: AuditQueryFilter, limit: u32) -> Vec<AuditLogEntry> {
        let capped_limit = cap_query_limit(limit);
        let results = AuditStorage::query_audit_logs(&env, &filter, capped_limit);
        emit_audit_query(
            &env,
            String::from_str(&env, "query_audit_logs"),
            results.len() as u32,
        );
        results
    }

    /// Get audit statistics
    pub fn get_audit_stats(env: Env) -> AuditStats {
        AuditStorage::get_audit_stats(&env)
    }

    /// Validate audit log integrity for an invoice
    pub fn validate_invoice_audit_integrity(
        env: Env,
        invoice_id: BytesN<32>,
    ) -> Result<bool, QuickLendXError> {
        let is_valid = AuditStorage::validate_invoice_audit_integrity(&env, &invoice_id)?;
        emit_audit_validation(&env, &invoice_id, is_valid);
        Ok(is_valid)
    }

    /// Get audit entries by operation type
    pub fn get_audit_entries_by_operation(env: Env, operation: AuditOperation) -> Vec<BytesN<32>> {
        AuditStorage::get_audit_entries_by_operation(&env, &operation)
    }

    /// Get audit entries by actor
    pub fn get_audit_entries_by_actor(env: Env, actor: Address) -> Vec<BytesN<32>> {
        AuditStorage::get_audit_entries_by_actor(&env, &actor)
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
        let mut invoice = InvoiceStorage::get_invoice(&env, &invoice_id)
            .ok_or(QuickLendXError::InvoiceNotFound)?;

        // Only the business owner can add tags
        invoice.business.require_auth();

        // Add the tag
        invoice.add_tag(&env, tag.clone())?;

        // Update the invoice
        InvoiceStorage::update_invoice(&env, &invoice);

        // Emit event
        events::emit_invoice_tag_added(&env, &invoice_id, &invoice.business, &tag);

        // Update index
        InvoiceStorage::add_tag_index(&env, &tag, &invoice_id);

        Ok(())
    }

    /// Remove tag from invoice (business owner only)
    pub fn remove_invoice_tag(
        env: Env,
        invoice_id: BytesN<32>,
        tag: String,
    ) -> Result<(), QuickLendXError> {
        let mut invoice = InvoiceStorage::get_invoice(&env, &invoice_id)
            .ok_or(QuickLendXError::InvoiceNotFound)?;

        // Only the business owner can remove tags
        invoice.business.require_auth();

        // Remove the tag
        invoice.remove_tag(tag.clone())?;

        // Update the invoice
        InvoiceStorage::update_invoice(&env, &invoice);

        // Emit event
        events::emit_invoice_tag_removed(&env, &invoice_id, &invoice.business, &tag);

        // Update index
        InvoiceStorage::remove_tag_index(&env, &tag, &invoice_id);

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

    // Dispute Resolution Functions

    /// Create a dispute for an invoice
    pub fn create_dispute(
        env: Env,
        invoice_id: BytesN<32>,
        creator: Address,
        reason: String,
        evidence: String,
    ) -> Result<(), QuickLendXError> {
        do_create_dispute(&env, &invoice_id, &creator, reason, evidence)
    }

    /// Put a dispute under review (admin function)
    pub fn put_dispute_under_review(
        env: Env,
        invoice_id: BytesN<32>,
        reviewer: Address,
    ) -> Result<(), QuickLendXError> {
        do_put_dispute_under_review(&env, &invoice_id, &reviewer)
    }

    /// Resolve a dispute (admin function)
    pub fn resolve_dispute(
        env: Env,
        invoice_id: BytesN<32>,
        resolver: Address,
        resolution: String,
    ) -> Result<(), QuickLendXError> {
        do_resolve_dispute(&env, &invoice_id, &resolver, resolution)
    }

    /// Get dispute details for an invoice
    pub fn get_dispute_details(
        env: Env,
        invoice_id: BytesN<32>,
    ) -> Result<Option<invoice::Dispute>, QuickLendXError> {
        do_get_dispute_details(&env, &invoice_id)
    }

    /// Get all invoices with disputes
    pub fn get_invoices_with_disputes(env: Env) -> Vec<BytesN<32>> {
        do_get_invoices_with_disputes(&env)
    }

    /// Get invoices by dispute status
    pub fn get_invoices_by_dispute_status(
        env: Env,
        dispute_status: DisputeStatus,
    ) -> Vec<BytesN<32>> {
        do_get_invoices_by_dispute_status(&env, dispute_status)
    }

    /// Get dispute status for an invoice
    pub fn get_invoice_dispute_status(
        env: Env,
        invoice_id: BytesN<32>,
    ) -> Result<DisputeStatus, QuickLendXError> {
        let invoice = InvoiceStorage::get_invoice(&env, &invoice_id)
            .ok_or(QuickLendXError::InvoiceNotFound)?;
        Ok(invoice.dispute_status)
    }

    // Analytics and Reporting Functions

    /// Get current platform metrics
    pub fn get_platform_metrics(env: Env) -> Result<PlatformMetrics, QuickLendXError> {
        AnalyticsCalculator::calculate_platform_metrics(&env)
    }

    /// Update platform metrics (admin only)
    pub fn update_platform_metrics(env: Env) -> Result<(), QuickLendXError> {
        let admin =
            BusinessVerificationStorage::get_admin(&env).ok_or(QuickLendXError::NotAdmin)?;
        admin.require_auth();

        let metrics = AnalyticsCalculator::calculate_platform_metrics(&env)?;
        AnalyticsStorage::store_platform_metrics(&env, &metrics);

        // Emit event
        events::emit_platform_metrics_updated(
            &env,
            metrics.total_invoices,
            metrics.total_volume,
            metrics.total_fees_collected,
            metrics.success_rate,
        );

        Ok(())
    }

    /// Get performance metrics
    pub fn get_performance_metrics(env: Env) -> Result<PerformanceMetrics, QuickLendXError> {
        AnalyticsCalculator::calculate_performance_metrics(&env)
    }

    /// Update performance metrics (admin only)
    pub fn update_performance_metrics(env: Env) -> Result<(), QuickLendXError> {
        let admin =
            BusinessVerificationStorage::get_admin(&env).ok_or(QuickLendXError::NotAdmin)?;
        admin.require_auth();

        let metrics = AnalyticsCalculator::calculate_performance_metrics(&env)?;
        AnalyticsStorage::store_performance_metrics(&env, &metrics);

        // Emit event
        events::emit_performance_metrics_updated(
            &env,
            metrics.average_settlement_time,
            metrics.transaction_success_rate,
            metrics.user_satisfaction_score,
        );

        Ok(())
    }

    /// Get user behavior metrics
    pub fn get_user_behavior_metrics(
        env: Env,
        user: Address,
    ) -> Result<UserBehaviorMetrics, QuickLendXError> {
        AnalyticsCalculator::calculate_user_behavior_metrics(&env, &user)
    }

    /// Update user behavior metrics
    pub fn update_user_behavior_metrics(env: Env, user: Address) -> Result<(), QuickLendXError> {
        user.require_auth();

        let behavior = AnalyticsCalculator::calculate_user_behavior_metrics(&env, &user)?;
        AnalyticsStorage::store_user_behavior(&env, &user, &behavior);

        // Emit event
        events::emit_user_behavior_analyzed(
            &env,
            &user,
            behavior.total_investments_made,
            behavior.success_rate,
            behavior.risk_score,
        );

        Ok(())
    }

    /// Get financial metrics for a specific period
    pub fn get_financial_metrics(
        env: Env,
        period: TimePeriod,
    ) -> Result<FinancialMetrics, QuickLendXError> {
        let metrics = AnalyticsCalculator::calculate_financial_metrics(&env, period.clone())?;

        // Emit event
        events::emit_financial_metrics_calculated(
            &env,
            &period,
            metrics.total_volume,
            metrics.total_fees,
            metrics.average_return_rate,
        );

        Ok(metrics)
    }

    /// Generate business report
    pub fn generate_business_report(
        env: Env,
        business: Address,
        period: TimePeriod,
    ) -> Result<BusinessReport, QuickLendXError> {
        business.require_auth();

        let report =
            AnalyticsCalculator::generate_business_report(&env, &business, period.clone())?;
        AnalyticsStorage::store_business_report(&env, &report);

        // Emit event
        events::emit_business_report_generated(
            &env,
            &report.report_id,
            &business,
            &period,
            report.invoices_uploaded,
            report.success_rate,
        );

        Ok(report)
    }

    /// Generate investor report
    pub fn generate_investor_report(
        env: Env,
        investor: Address,
        period: TimePeriod,
    ) -> Result<InvestorReport, QuickLendXError> {
        investor.require_auth();

        let report =
            AnalyticsCalculator::generate_investor_report(&env, &investor, period.clone())?;
        AnalyticsStorage::store_investor_report(&env, &report);

        // Emit event
        events::emit_investor_report_generated(
            &env,
            &report.report_id,
            &investor,
            &period,
            report.investments_made,
            report.average_return_rate,
        );

        Ok(report)
    }

    /// Get business report by ID
    pub fn get_business_report(env: Env, report_id: BytesN<32>) -> Option<BusinessReport> {
        AnalyticsStorage::get_business_report(&env, &report_id)
    }

    /// Get investor report by ID
    pub fn get_investor_report(env: Env, report_id: BytesN<32>) -> Option<InvestorReport> {
        AnalyticsStorage::get_investor_report(&env, &report_id)
    }

    /// Get analytics data summary
    pub fn get_analytics_summary(
        env: Env,
    ) -> Result<(PlatformMetrics, PerformanceMetrics), QuickLendXError> {
        let platform_metrics = AnalyticsStorage::get_platform_metrics(&env)
            .unwrap_or_else(|| AnalyticsCalculator::calculate_platform_metrics(&env).unwrap());

        let performance_metrics = AnalyticsStorage::get_performance_metrics(&env)
            .unwrap_or_else(|| AnalyticsCalculator::calculate_performance_metrics(&env).unwrap());

        Ok((platform_metrics, performance_metrics))
    }

    /// Export analytics data (admin only)
    pub fn export_analytics_data(
        env: Env,
        export_type: String,
        filters: Vec<String>,
    ) -> Result<String, QuickLendXError> {
        let admin =
            BusinessVerificationStorage::get_admin(&env).ok_or(QuickLendXError::NotAdmin)?;
        admin.require_auth();

        // Emit event
        events::emit_analytics_export(&env, &export_type, &admin, filters.len() as u32);

        // Return a summary string
        Ok(String::from_str(&env, "Analytics data exported"))
    }

    /// Query analytics data with filters
    pub fn query_analytics_data(
        env: Env,
        query_type: String,
        filters: Vec<String>,
        limit: u32,
    ) -> Result<Vec<String>, QuickLendXError> {
        let capped_limit = cap_query_limit(limit);
        // Emit event
        events::emit_analytics_query(&env, &query_type, filters.len() as u32, capped_limit);

        // Return basic analytics data
        let mut results = Vec::new(&env);
        results.push_back(String::from_str(&env, "Analytics query completed"));

        Ok(results)
    }

    /// Get analytics trends over time
    pub fn get_analytics_trends(
        env: Env,
        period: TimePeriod,
        _metric_type: String,
    ) -> Result<Vec<(u64, i128)>, QuickLendXError> {
        let mut trends = Vec::new(&env);
        let current_timestamp = env.ledger().timestamp();

        // Generate sample trend data based on period
        let (start_date, _) =
            AnalyticsCalculator::get_period_dates(current_timestamp, period.clone());
        let interval = match period {
            TimePeriod::Daily => 24 * 60 * 60,          // 1 day
            TimePeriod::Weekly => 7 * 24 * 60 * 60,     // 1 week
            TimePeriod::Monthly => 30 * 24 * 60 * 60,   // 1 month
            TimePeriod::Quarterly => 90 * 24 * 60 * 60, // 1 quarter
            TimePeriod::Yearly => 365 * 24 * 60 * 60,   // 1 year
            TimePeriod::AllTime => current_timestamp - start_date,
        };

        let mut timestamp = start_date;
        while timestamp < current_timestamp {
            // Calculate metrics for this time period
            let period_metrics =
                AnalyticsCalculator::calculate_financial_metrics(&env, period.clone())?;

            let value = period_metrics.total_volume;

            trends.push_back((timestamp, value));
            timestamp = timestamp.saturating_add(interval);
        }

        Ok(trends)
    }

    // ========================================
    // Enhanced Investor Analytics Functions
    // ========================================

    /// Calculate comprehensive investor analytics
    pub fn calculate_investor_analytics(
        env: Env,
        investor: Address,
    ) -> Result<InvestorAnalytics, QuickLendXError> {
        let analytics = AnalyticsCalculator::calculate_investor_analytics(&env, &investor)?;
        AnalyticsStorage::store_investor_analytics(&env, &investor, &analytics);
        Ok(analytics)
    }

    /// Get stored investor analytics
    pub fn get_investor_analytics_data(env: Env, investor: Address) -> Option<InvestorAnalytics> {
        AnalyticsStorage::get_investor_analytics(&env, &investor)
    }

    /// Calculate investor performance metrics for the platform
    pub fn calc_investor_perf_metrics(
        env: Env,
    ) -> Result<InvestorPerformanceMetrics, QuickLendXError> {
        let metrics = AnalyticsCalculator::calc_investor_perf_metrics(&env)?;
        AnalyticsStorage::store_investor_performance(&env, &metrics);
        Ok(metrics)
    }

    /// Get stored investor performance metrics
    pub fn get_investor_performance_metrics(env: Env) -> Option<InvestorPerformanceMetrics> {
        AnalyticsStorage::get_investor_performance(&env)
    }

    /// Update investor analytics (admin only)
    pub fn update_investor_analytics_data(
        env: Env,
        investor: Address,
    ) -> Result<(), QuickLendXError> {
        let admin =
            BusinessVerificationStorage::get_admin(&env).ok_or(QuickLendXError::NotAdmin)?;
        admin.require_auth();

        let analytics = AnalyticsCalculator::calculate_investor_analytics(&env, &investor)?;
        AnalyticsStorage::store_investor_analytics(&env, &investor, &analytics);

        // Emit event
        events::emit_investor_analytics_updated(
            &env,
            &investor,
            analytics.success_rate,
            analytics.risk_score,
            analytics.compliance_score,
        );

        Ok(())
    }

    /// Update platform investor performance metrics (admin only)
    pub fn update_investor_performance_data(env: Env) -> Result<(), QuickLendXError> {
        let admin =
            BusinessVerificationStorage::get_admin(&env).ok_or(QuickLendXError::NotAdmin)?;
        admin.require_auth();

        let metrics = AnalyticsCalculator::calc_investor_perf_metrics(&env)?;
        AnalyticsStorage::store_investor_performance(&env, &metrics);

        // Emit event
        events::emit_investor_performance_updated(
            &env,
            metrics.total_investors,
            metrics.verified_investors,
            metrics.platform_success_rate,
            metrics.average_risk_score,
        );

        Ok(())
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
        admin.require_auth();

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
    pub fn get_business_invoices_paged(
        env: Env,
        business: Address,
        status_filter: Option<InvoiceStatus>,
        offset: u32,
        limit: u32,
    ) -> Vec<BytesN<32>> {
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
    pub fn get_investor_investments_paged(
        env: Env,
        investor: Address,
        status_filter: Option<InvestmentStatus>,
        offset: u32,
        limit: u32,
    ) -> Vec<BytesN<32>> {
        let capped_limit = cap_query_limit(limit);
        let all_investment_ids = InvestmentStorage::get_investments_by_investor(&env, &investor);
        let mut filtered = Vec::new(&env);

        for investment_id in all_investment_ids.iter() {
            if let Some(investment) = InvestmentStorage::get_investment(&env, &investment_id) {
                if let Some(status) = &status_filter {
                    if investment.status == *status {
                        filtered.push_back(investment_id);
                    }
                } else {
                    filtered.push_back(investment_id);
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
            if let Some(investment_id) = filtered.get(idx) {
                result.push_back(investment_id);
            }
            idx += 1;
        }
        result
    }

    /// Get available invoices with pagination and optional filters
    pub fn get_available_invoices_paged(
        env: Env,
        min_amount: Option<i128>,
        max_amount: Option<i128>,
        category_filter: Option<invoice::InvoiceCategory>,
        offset: u32,
        limit: u32,
    ) -> Vec<BytesN<32>> {
        let capped_limit = cap_query_limit(limit);
        let verified_invoices =
            InvoiceStorage::get_invoices_by_status(&env, &InvoiceStatus::Verified);
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
    pub fn get_bid_history_paged(
        env: Env,
        invoice_id: BytesN<32>,
        status_filter: Option<BidStatus>,
        offset: u32,
        limit: u32,
    ) -> Vec<Bid> {
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
    pub fn get_investor_bids_paged(
        env: Env,
        investor: Address,
        status_filter: Option<BidStatus>,
        offset: u32,
        limit: u32,
    ) -> Vec<Bid> {
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

    // ============================================================================
    // Backup and Restore Methods
    // ============================================================================

    /// Create a backup of all current invoice data (admin only)
    pub fn create_backup(env: Env, admin: Address) -> Result<BytesN<32>, QuickLendXError> {
        let current_admin = AdminStorage::get_admin(&env).ok_or(QuickLendXError::NotAdmin)?;
        if current_admin != admin {
            return Err(QuickLendXError::NotAdmin);
        }
        admin.require_auth();

        let all_invoices = BackupStorage::get_all_invoices(&env);
        let backup_id = BackupStorage::generate_backup_id(&env);
        let count = all_invoices.len() as u32;

        let backup = Backup {
            backup_id: backup_id.clone(),
            timestamp: env.ledger().timestamp(),
            description: String::from_str(&env, "Admin created backup"),
            invoice_count: count,
            status: BackupStatus::Active,
        };

        BackupStorage::store_backup(&env, &backup);
        BackupStorage::store_backup_data(&env, &backup_id, &all_invoices);
        BackupStorage::add_to_backup_list(&env, &backup_id);

        // Clean up old backups based on retention policy
        BackupStorage::cleanup_old_backups(&env)?;

        events::emit_backup_created(&env, &backup_id, count);
        Ok(backup_id)
    }

    /// Restore a backup, clearing current invoices and reinstating the backup (admin only)
    pub fn restore_backup(
        env: Env,
        admin: Address,
        backup_id: BytesN<32>,
    ) -> Result<(), QuickLendXError> {
        let current_admin = AdminStorage::get_admin(&env).ok_or(QuickLendXError::NotAdmin)?;
        if current_admin != admin {
            return Err(QuickLendXError::NotAdmin);
        }
        admin.require_auth();

        let backup = BackupStorage::get_backup(&env, &backup_id)
            .ok_or(QuickLendXError::StorageKeyNotFound)?;
        if backup.status == BackupStatus::Corrupted {
            return Err(QuickLendXError::StorageError); // Cannot restore a corrupted backup
        }

        let saved_invoices = BackupStorage::get_backup_data(&env, &backup_id)
            .ok_or(QuickLendXError::StorageKeyNotFound)?;

        // Clear current invoice data
        let current_invoices = BackupStorage::get_all_invoices(&env);
        for inv in current_invoices.iter() {
            crate::invoice::InvoiceStorage::delete_invoice(&env, &inv.id);
        }

        // Re-insert backed up data
        for inv in saved_invoices.iter() {
            crate::invoice::InvoiceStorage::store_invoice(&env, &inv);
        }

        events::emit_backup_restored(&env, &backup_id, saved_invoices.len() as u32);
        Ok(())
    }

    /// Validate a backup's integrity
    pub fn validate_backup(env: Env, backup_id: BytesN<32>) -> Result<bool, QuickLendXError> {
        match BackupStorage::validate_backup(&env, &backup_id) {
            Ok(_) => {
                events::emit_backup_validated(&env, &backup_id, true);
                Ok(true)
            }
            Err(_) => {
                if let Some(mut backup) = BackupStorage::get_backup(&env, &backup_id) {
                    backup.status = BackupStatus::Corrupted;
                    BackupStorage::update_backup(&env, &backup);
                }
                events::emit_backup_validated(&env, &backup_id, false);
                Ok(false)
            }
        }
    }

    /// Archive a backup
    pub fn archive_backup(
        env: Env,
        admin: Address,
        backup_id: BytesN<32>,
    ) -> Result<(), QuickLendXError> {
        let current_admin = AdminStorage::get_admin(&env).ok_or(QuickLendXError::NotAdmin)?;
        if current_admin != admin {
            return Err(QuickLendXError::NotAdmin);
        }
        admin.require_auth();

        let mut backup = BackupStorage::get_backup(&env, &backup_id)
            .ok_or(QuickLendXError::StorageKeyNotFound)?;
        backup.status = BackupStatus::Archived;
        BackupStorage::update_backup(&env, &backup);
        BackupStorage::remove_from_backup_list(&env, &backup_id);

        events::emit_backup_archived(&env, &backup_id);
        Ok(())
    }

    /// Set backup retention policy (admin only)
    pub fn set_backup_retention_policy(
        env: Env,
        admin: Address,
        max_backups: u32,
        max_age_seconds: u64,
        auto_cleanup_enabled: bool,
    ) -> Result<(), QuickLendXError> {
        let current_admin = AdminStorage::get_admin(&env).ok_or(QuickLendXError::NotAdmin)?;
        if current_admin != admin {
            return Err(QuickLendXError::NotAdmin);
        }
        admin.require_auth();

        let policy = BackupRetentionPolicy {
            max_backups,
            max_age_seconds,
            auto_cleanup_enabled,
        };

        BackupStorage::set_retention_policy(&env, &policy);
        events::emit_retention_policy_updated(&env, max_backups, max_age_seconds, auto_cleanup_enabled);

        Ok(())
    }

    /// Get current backup retention policy
    pub fn get_backup_retention_policy(env: Env) -> BackupRetentionPolicy {
        BackupStorage::get_retention_policy(&env)
    }

    /// Manually trigger backup cleanup (admin only)
    pub fn cleanup_backups(env: Env, admin: Address) -> Result<u32, QuickLendXError> {
        let current_admin = AdminStorage::get_admin(&env).ok_or(QuickLendXError::NotAdmin)?;
        if current_admin != admin {
            return Err(QuickLendXError::NotAdmin);
        }
        admin.require_auth();

        let removed_count = BackupStorage::cleanup_old_backups(&env)?;
        events::emit_backups_cleaned(&env, removed_count);

        Ok(removed_count)
    }

    /// Get all available backup IDs
    pub fn get_backups(env: Env) -> Vec<BytesN<32>> {
        BackupStorage::get_all_backups(&env)
    }

    /// Get backup details
    pub fn get_backup_details(env: Env, backup_id: BytesN<32>) -> Option<Backup> {
        BackupStorage::get_backup(&env, &backup_id)
    }
}

#[cfg(test)]
mod test;

#[cfg(test)]
mod test_bid;

#[cfg(test)]
mod test_bid_ranking;

#[cfg(test)]
mod test_fees;

#[cfg(test)]
mod test_escrow;

#[cfg(test)]
mod test_audit;
#[cfg(test)]
mod test_currency;
#[cfg(test)]
mod test_errors;
#[cfg(test)]
mod test_events;

#[cfg(test)]
mod test_default;

#[cfg(test)]
mod test_invariants;
#[cfg(test)]
mod test_investment_queries;
#[cfg(test)]
mod test_partial_payments;
#[cfg(test)]
mod test_queries;
#[cfg(test)]
mod test_reentrancy;

#[cfg(test)]
mod test_backup;
#[cfg(test)]
mod test_escrow_refund;
#[cfg(test)]
mod test_fuzz;
#[cfg(test)]
mod test_insurance;
#[cfg(test)]
mod test_investor_kyc;
#[cfg(test)]
mod test_limit;
#[cfg(test)]
mod test_profit_fee_formula;
#[cfg(test)]
mod test_revenue_split;
#[cfg(test)]
mod test_types;
#[cfg(test)]
mod test_lifecycle;
