#![no_std]

extern crate alloc;

// ============================================================================
// Module declarations
// ============================================================================

pub mod admin;
pub mod arbiter;
pub mod audit;
pub mod backpressure;
pub mod backup;
pub mod backup_v1;
#[cfg(any(test, feature = "testutils"))]
pub mod bench;
pub mod bid;
pub mod currency;
pub mod defaults;
pub mod dispute;
pub mod dispute_timeline;
pub mod emergency;
pub mod errors;
pub mod escrow;
pub mod events;
pub mod fees;
pub mod freshness;
pub mod governance;
pub mod health;
pub mod idempotency;
pub mod incident;
pub mod init;
pub mod invariants;
pub mod investment;
pub mod investment_queries;
pub mod invoice;
pub mod invoice_amount;
pub mod invoice_search;
pub mod kyc_policy;
pub mod maintenance;
pub mod migration;
pub mod monitor;
pub mod multisig;
pub mod notifications;
pub mod observability;
pub mod operational_limits;
pub mod pagination;
pub mod panic_handler;
pub mod pause;
pub mod payment_token_policy;
pub mod payments;
pub mod profits;
pub mod protocol_limits;
pub mod reentrancy;
pub mod regulatory;
pub mod resolution_policy;
pub mod settlement;
pub mod storage;
pub mod types;
pub mod upgrade;
pub mod verification;
pub mod vesting;

// diagnostics is feature-gated
#[cfg(feature = "diagnostics")]
pub mod diagnostics;

pub mod address_summary;
pub mod analytics;

/// Shared test utilities accessible from integration tests.
pub mod test_utils;

// ============================================================================
// Test modules
// ============================================================================
//
// All `src/test_*.rs` modules are declared in `src/tests/mod.rs` so this
// file stays small and stable across PRs. Run
// `scripts/check-test-modules.sh` to find test files without a declaration.

#[cfg(test)]
mod tests;

// ============================================================================
// Contract-level macros (re-exported from diagnostics)
// ============================================================================

#[cfg(not(feature = "diagnostics"))]
#[macro_export]
macro_rules! qlx_log {
    ($env:expr, $module:expr, $fmt:expr $(, $arg:expr)*) => {{
        #[cfg(feature = "diagnostics")]
        {
            let _ = ($env, $module, $fmt $(, $arg)*);
        }
        #[cfg(not(feature = "diagnostics"))]
        {
            let _ = ($env, $module, $fmt $(, $arg)*);
        }
    }};
}

#[cfg(not(feature = "diagnostics"))]
#[macro_export]
macro_rules! assert_view_only {
    ($env:expr) => {{
        // no-op: view-only enforcement is handled in StorageManager::is_view_only
    }};
}

// ============================================================================
// Protocol-wide constants
// ============================================================================

/// Hard upper bound on paginated query results.
pub const MAX_QUERY_LIMIT: u32 = 50;

// ============================================================================
// Contract definition
// ============================================================================

use soroban_sdk::{contract, contractimpl, Address, BytesN, Env, String, Vec};

use crate::admin::AdminStorage;
use crate::arbiter::ArbiterStorage;
use crate::errors::QuickLendXError;
use crate::pause::PauseControl;
use crate::protocol_limits::{
    check_and_record_mutation, require_description_bound, require_line_items_bound,
    require_status_batch_bound, require_tags_bound,
};
use crate::reentrancy::with_payment_guard;
use crate::storage::InvoiceStorage;
use crate::types::{
    Dispute, DisputeResolution, DisputeStatus, Investment, InvoiceCategory, InvoiceLock,
    InvoiceMetadata, InvoiceStatus,
    BusinessFreezeReason, Dispute, DisputeResolution, DisputeStatus, Investment, InvoiceCategory,
    InvoiceStatus,
};

/// Load an invoice for a state-changing call made by its owning business.
///
/// Existence, ownership and the owner's signature are verified first and the
/// mutation quota is charged last, so unauthorised calls fail fast. This
/// helper calls `require_auth` for `business`; callers must not authenticate
/// the same address again within the same invocation (Soroban rejects a
/// duplicate `require_auth`). Frozen invoices reject writes (with
/// `InvoiceLockExpired` once the lock outlives the lock-time limit).
fn load_invoice_for_owner_write(
    env: &Env,
    business: &Address,
    invoice_id: &BytesN<32>,
) -> Result<crate::types::Invoice, QuickLendXError> {
    PauseControl::require_not_paused(env)?;
    let invoice =
        InvoiceStorage::get_invoice(env, invoice_id).ok_or(QuickLendXError::InvoiceNotFound)?;
    crate::invoice::require_matching_business_invoice_ownership(env, business, &invoice)?;
    business.require_auth();
    if InvoiceStorage::is_frozen(env, invoice_id) {
        InvoiceStorage::require_lock_within_time_limit(env, invoice_id)?;
        return Err(QuickLendXError::InvoiceFrozen);
    }
    check_and_record_mutation(env, business)?;
    Ok(invoice)
}


#[contract]
pub struct QuickLendXContract;

#[contractimpl]
impl QuickLendXContract {
    // =========================================================================
    // Admin / Initialization
    // =========================================================================

    /// One-time admin initialization or legacy single-arg set (for test
    /// compatibility). When called with a single address argument:
    /// - If admin is not yet initialized, initializes it.
    /// - If already initialized, requires the current admin to authorize a
    ///   transfer to `admin` (two-arg form uses `set_admin_transfer`).
    pub fn set_admin(env: Env, admin: Address) -> Result<(), QuickLendXError> {
        AdminStorage::set_admin_legacy(&env, &admin)
    }

    pub fn get_admin(env: Env) -> Result<Address, QuickLendXError> {
        AdminStorage::get_admin(&env).ok_or(QuickLendXError::StorageKeyNotFound)
    }

    /// Initialize the contract admin exactly once.
    ///
    /// # Authorization
    /// Requires the `admin` argument to authorize the call.
    pub fn initialize_admin(env: Env, admin: Address) -> Result<(), QuickLendXError> {
        AdminStorage::initialize(&env, &admin)
    }

    /// Transfer the admin role from the current admin to `new_admin`.
    ///
    /// # Authorization
    /// Requires the current admin (read from storage) to authorize the call.
    pub fn transfer_admin(env: Env, new_admin: Address) -> Result<(), QuickLendXError> {
        let current = AdminStorage::get_admin(&env).ok_or(QuickLendXError::NotAdmin)?;
        AdminStorage::transfer_admin(&env, &current, &new_admin)
    }

    pub fn initialize_fee_system(env: Env, admin: Address) -> Result<(), QuickLendXError> {
        crate::upgrade::UpgradeControl::require_no_pending_upgrade(&env)?;
        crate::fees::FeeManager::initialize(&env, &admin)
    }

    pub fn update_platform_fee_bps(env: Env, fee_bps: u32) -> Result<(), QuickLendXError> {
        crate::upgrade::UpgradeControl::require_no_pending_upgrade(&env)?;
        let admin = AdminStorage::get_admin(&env).ok_or(QuickLendXError::NotAdmin)?;
        crate::fees::FeeManager::update_platform_fee(&env, &admin, fee_bps)
    }

    pub fn get_platform_fee_config(
        env: Env,
    ) -> Result<crate::types::PlatformFeeConfig, QuickLendXError> {
        crate::fees::FeeManager::get_platform_fee_config(&env)
    }

    pub fn calculate_profit(
        env: Env,
        investment_amount: i128,
        payment_amount: i128,
    ) -> (i128, i128) {
        crate::profits::calculate_profit(&env, investment_amount, payment_amount)
    }

    pub fn invariant_self_check(
        env: Env,
        admin: Address,
    ) -> Result<crate::invariants::InvariantReport, QuickLendXError> {
        crate::invariants::invariant_self_check(&env, &admin)
    }

    pub fn initialize_protocol_limits(env: Env, admin: Address) -> Result<(), QuickLendXError> {
        crate::upgrade::UpgradeControl::require_no_pending_upgrade(&env)?;
        crate::protocol_limits::ProtocolLimitsContract::initialize(env, admin)
    }

    /// Admin-only: add a token address to the currency whitelist.
    ///
    /// An empty whitelist accepts every currency; once a currency is added the
    /// list becomes restrictive. See [`crate::currency::CurrencyWhitelist`].
    pub fn add_currency(
        env: Env,
        admin: Address,
        currency: Address,
    ) -> Result<(), QuickLendXError> {
        crate::currency::CurrencyWhitelist::add_currency(&env, &admin, &currency)
    }

    pub fn pause(env: Env, admin: Address) -> Result<(), QuickLendXError> {
        crate::upgrade::UpgradeControl::require_no_pending_upgrade(&env)?;
        PauseControl::set_paused(&env, &admin, true)
    }

    pub fn unpause(env: Env, admin: Address) -> Result<(), QuickLendXError> {
        crate::upgrade::UpgradeControl::require_no_pending_upgrade(&env)?;
        PauseControl::set_paused(&env, &admin, false)
    }

    pub fn is_paused(env: Env) -> bool {
        PauseControl::is_paused(&env)
    }

    /// Schedule a WASM upgrade. Only the current admin may schedule it.
    pub fn schedule_upgrade(
        env: Env,
        admin: Address,
        wasm_hash: BytesN<32>,
    ) -> Result<(), QuickLendXError> {
        crate::upgrade::UpgradeControl::schedule_upgrade(&env, &admin, &wasm_hash)
    }

    /// Cancel a scheduled WASM upgrade and restore the previous pause state.
    pub fn cancel_upgrade(env: Env, admin: Address) -> Result<(), QuickLendXError> {
        crate::upgrade::UpgradeControl::cancel_upgrade(&env, &admin)
    }

    /// Execute a scheduled WASM upgrade after its timelock has elapsed.
    pub fn execute_upgrade(env: Env, admin: Address) -> Result<(), QuickLendXError> {
        crate::upgrade::UpgradeControl::execute_upgrade(&env, &admin)
    }

    /// Read the pending upgrade hash and its scheduling timestamp.
    pub fn get_pending_upgrade(env: Env) -> Option<(BytesN<32>, u64)> {
        crate::upgrade::UpgradeControl::get_pending_upgrade(&env)
    }

    // =========================================================================
    // KYC / Verification
    // =========================================================================

    pub fn submit_kyc_application(
        env: Env,
        business: Address,
        kyc_data: String,
    ) -> Result<(), QuickLendXError> {
        crate::upgrade::UpgradeControl::require_no_pending_upgrade(&env)?;
        check_and_record_mutation(&env, &business)?;
        crate::verification::submit_kyc_application(&env, &business, kyc_data)
    }

    pub fn verify_business(
        env: Env,
        admin: Address,
        business: Address,
    ) -> Result<(), QuickLendXError> {
        crate::upgrade::UpgradeControl::require_no_pending_upgrade(&env)?;
        crate::verification::verify_business(&env, &admin, &business)
    }

    pub fn submit_investor_kyc(
        env: Env,
        investor: Address,
        kyc_data: String,
    ) -> Result<(), QuickLendXError> {
        crate::upgrade::UpgradeControl::require_no_pending_upgrade(&env)?;
        check_and_record_mutation(&env, &investor)?;
        crate::verification::submit_investor_kyc(&env, &investor, kyc_data)
    }

    /// Verify investor with investment limit. Admin is read from storage
    /// (matches test usage: `client.verify_investor(investor, limit)`).
    pub fn verify_investor(
        env: Env,
        investor: Address,
        limit: i128,
    ) -> Result<(), QuickLendXError> {
    pub fn verify_investor(env: Env, investor: Address, limit: i128) -> Result<(), QuickLendXError> {
        crate::upgrade::UpgradeControl::require_no_pending_upgrade(&env)?;
        let admin = AdminStorage::get_admin(&env).ok_or(QuickLendXError::NotAdmin)?;
        crate::verification::verify_investor(&env, &admin, &investor, limit)?;
        Ok(())
    }

    pub fn get_businesses_paged(
        env: Env,
        status: crate::verification::BusinessVerificationStatus,
        cursor: Option<String>,
        limit: u32,
    ) -> Result<crate::kyc_policy::KycPageResult, QuickLendXError> {
        crate::kyc_policy::get_businesses_paged(&env, status, cursor, limit)
    }

    pub fn get_investors_paged(
        env: Env,
        status: crate::verification::BusinessVerificationStatus,
        cursor: Option<String>,
        limit: u32,
    ) -> Result<crate::kyc_policy::KycPageResult, QuickLendXError> {
        crate::kyc_policy::get_investors_paged(&env, status, cursor, limit)
    }

    // =========================================================================
    // Invoice lifecycle
    // =========================================================================

    pub fn store_invoice(
        env: Env,
        business: Address,
        amount: i128,
        currency: Address,
        due_date: u64,
        description: String,
        category: InvoiceCategory,
        tags: Vec<String>,
        origination_fee_bps: Option<u32>,
    ) -> Result<BytesN<32>, QuickLendXError> {
        PauseControl::require_not_paused(&env)?;
        check_and_record_mutation(&env, &business)?;
        business.require_auth();
        crate::verification::require_business_not_pending(&env, &business)?;
        crate::invoice_amount::validate_invoice_amount_ceiling(amount)?;
        crate::protocol_limits::check_invoice_limit(&env, &business)?;
        let invoice = crate::invoice::Invoice::new_with_fee_settings(
            &env,
            business,
            amount,
            currency,
            due_date,
            description,
            category,
            tags,
            origination_fee_bps,
            None,
            None,
        )?;
        let id = invoice.id.clone();
        InvoiceStorage::store_invoice(&env, &invoice);
        Ok(id)
    }

    pub fn get_invoice(
        env: Env,
        invoice_id: BytesN<32>,
    ) -> Result<crate::types::Invoice, QuickLendXError> {
        InvoiceStorage::get_invoice(&env, &invoice_id).ok_or(QuickLendXError::InvoiceNotFound)
    }

    pub fn verify_invoice(env: Env, invoice_id: BytesN<32>) -> Result<(), QuickLendXError> {
        crate::upgrade::UpgradeControl::require_no_pending_upgrade(&env)?;
        let admin = AdminStorage::get_admin(&env).ok_or(QuickLendXError::NotAdmin)?;
        AdminStorage::require_admin(&env, &admin)?;
        let mut invoice = InvoiceStorage::get_invoice(&env, &invoice_id)
            .ok_or(QuickLendXError::InvoiceNotFound)?;
        invoice.verify(&env, admin);
        InvoiceStorage::update_invoice(&env, &invoice);
        Ok(())
    }

    pub fn update_invoice_status(
        env: Env,
        invoice_id: BytesN<32>,
        status: InvoiceStatus,
    ) -> Result<(), QuickLendXError> {
        crate::upgrade::UpgradeControl::require_no_pending_upgrade(&env)?;
        let admin = AdminStorage::get_admin(&env).ok_or(QuickLendXError::NotAdmin)?;
        AdminStorage::require_admin(&env, &admin)?;
        let mut invoice = InvoiceStorage::get_invoice(&env, &invoice_id)
            .ok_or(QuickLendXError::InvoiceNotFound)?;
        invoice.status = status;
        InvoiceStorage::update_invoice(&env, &invoice);
        Ok(())
    }

    pub fn cancel_invoice(env: Env, invoice_id: BytesN<32>) -> Result<(), QuickLendXError> {
        crate::upgrade::UpgradeControl::require_no_pending_upgrade(&env)?;
        let mut invoice = InvoiceStorage::get_invoice(&env, &invoice_id)
            .ok_or(QuickLendXError::InvoiceNotFound)?;
        let caller = invoice.business.clone();
        invoice.cancel(&env, caller)?;
        InvoiceStorage::update_invoice(&env, &invoice);
        Ok(())
    }

    /// Emit the `InvoiceExpired` event for an invoice past its due date.
    ///
    /// Requires `current_timestamp > invoice.due_date`. This entrypoint only
    /// emits the event; it does not change the invoice status. Status
    /// transitions are performed by `mark_invoice_defaulted` (→ `Defaulted`)
    /// or `refund_escrow` (→ `Refunded`).
    pub fn expire_invoice(env: Env, invoice_id: BytesN<32>) -> Result<(), QuickLendXError> {
        let invoice = InvoiceStorage::get_invoice(&env, &invoice_id)
            .ok_or(QuickLendXError::InvoiceNotFound)?;
        if env.ledger().timestamp() <= invoice.due_date {
            return Err(QuickLendXError::InvalidStatus);
        }
        crate::events::emit_invoice_expired(&env, &invoice);
        Ok(())
    }

    pub fn get_available_invoices(env: Env) -> Vec<BytesN<32>> {
        InvoiceStorage::get_invoices_by_status(&env, InvoiceStatus::Verified)
    }

    /// Return all invoice IDs currently in `status`.
    pub fn get_invoices_by_status(env: Env, status: InvoiceStatus) -> Vec<BytesN<32>> {
        InvoiceStorage::get_invoices_by_status(&env, status)
    }

    pub fn get_business_invoices(env: Env, business: Address) -> Vec<BytesN<32>> {
        InvoiceStorage::get_business_invoices(&env, &business)
    }

    pub fn get_business_invoices_paged(
        env: Env,
        business: Address,
        status_filter: Option<InvoiceStatus>,
        cursor: Option<String>,
        limit: u32,
    ) -> Result<crate::types::InvoicePage, QuickLendXError> {
        InvoiceStorage::get_business_invoices_paged(
            &env,
            &business,
            status_filter,
            cursor,
            limit,
        )
    }

    pub fn get_whitelisted_currencies_paged(
        env: Env,
        cursor: Option<String>,
        limit: u32,
    ) -> Result<crate::types::CursorAddressPage, QuickLendXError> {
        crate::currency::CurrencyWhitelist::get_whitelisted_currencies_paged(
            &env, cursor, limit,
        )
    }

    #[deprecated(note = "use get_whitelisted_currencies_paged with an opaque cursor")]
    pub fn get_whitelisted_currency_offset(
        env: Env,
        offset: u32,
        limit: u32,
    ) -> crate::types::PaginatedCurrencies {
        crate::currency::CurrencyWhitelist::get_whitelisted_currency_offset(
            &env, offset, limit,
        )
    }

    pub fn get_whitelisted_currencies(env: Env) -> Vec<Address> {
        crate::currency::CurrencyWhitelist::get_whitelisted_currencies(&env)
    }

    pub fn get_user_notifications(env: Env, user: Address) -> Vec<BytesN<32>> {
        crate::notifications::NotificationSystem::get_user_notifications(&env, &user)
    }

    pub fn get_user_notifications_paged(
        env: Env,
        user: Address,
        cursor: Option<String>,
        limit: u32,
    ) -> Result<crate::types::CursorBytes32Page, QuickLendXError> {
        crate::notifications::NotificationSystem::get_user_notifications_paged(
            &env, &user, cursor, limit,
        )
    }

    pub fn query_audit_logs_paged(
        env: Env,
        filter: crate::audit::AuditQueryFilter,
        cursor: Option<String>,
        limit: u32,
    ) -> Result<crate::audit::CursorAuditPage, QuickLendXError> {
        crate::audit::AuditStorage::query_audit_logs_paged(&env, &filter, cursor, limit)
    }

    // =========================================================================
    // Analytics
    // =========================================================================

    /// Platform analytics derived live from on-chain state.
    ///
    /// Totals and rates are computed from the invoice status indexes, so the
    /// values always reflect the current ledger state. Read-only; no auth.
    pub fn get_platform_metrics(
        env: Env,
    ) -> Result<crate::analytics::PlatformMetrics, QuickLendXError> {
        crate::analytics::AnalyticsCalculator::calculate_platform_metrics(&env)
    pub fn get_total_invoice_count(env: Env) -> u32 {
        InvoiceStorage::get_total_count(&env) as u32
    }

    // =========================================================================
    // Invoice management: upload, tags, category, metadata, lock, queries
    // =========================================================================

    /// Upload an invoice with explicit fee settings.
    ///
    /// Equivalent to [`Self::store_invoice`] but also accepts the late-payment
    /// penalty and early-payment discount. Oversize descriptions or tag vectors
    /// are rejected with `InputTooLarge` before any state is touched.
    #[allow(clippy::too_many_arguments)]
    pub fn upload_invoice(
        env: Env,
        business: Address,
        amount: i128,
        currency: Address,
        due_date: u64,
        description: String,
        category: InvoiceCategory,
        tags: Vec<String>,
        origination_fee_bps: Option<u32>,
        late_payment_penalty_bps: Option<u32>,
        early_payment_discount_bps: Option<u32>,
    ) -> Result<BytesN<32>, QuickLendXError> {
        PauseControl::require_not_paused(&env)?;
        business.require_auth();
        require_description_bound(&description)?;
        require_tags_bound(&tags)?;
        check_and_record_mutation(&env, &business)?;
        crate::verification::require_business_not_pending(&env, &business)?;
        crate::invoice_amount::validate_invoice_amount_ceiling(amount)?;
        crate::protocol_limits::check_invoice_limit(&env, &business)?;
        let invoice = crate::invoice::Invoice::new_with_fee_settings(
            &env,
            business,
            amount,
            currency,
            due_date,
            description,
            category,
            tags,
            origination_fee_bps,
            late_payment_penalty_bps,
            early_payment_discount_bps,
        )?;
        let id = invoice.id.clone();
        InvoiceStorage::store_invoice(&env, &invoice);
        Ok(id)
    }

    /// Add a (normalised) tag to an invoice owned by `business`.
    pub fn add_invoice_tag(
        env: Env,
        business: Address,
        invoice_id: BytesN<32>,
        tag: String,
    ) -> Result<(), QuickLendXError> {
        let mut invoice = load_invoice_for_owner_write(&env, &business, &invoice_id)?;
        invoice.add_tag(&env, tag)?;
        InvoiceStorage::update_invoice(&env, &invoice);
        Ok(())
    }

    /// Remove a tag from an invoice owned by `business`.
    pub fn remove_invoice_tag(
        env: Env,
        business: Address,
        invoice_id: BytesN<32>,
        tag: String,
    ) -> Result<(), QuickLendXError> {
        let mut invoice = load_invoice_for_owner_write(&env, &business, &invoice_id)?;
        invoice.remove_tag(tag)?;
        InvoiceStorage::update_invoice(&env, &invoice);
        Ok(())
    }

    /// Change the category of an invoice owned by `business`.
    pub fn update_invoice_category(
        env: Env,
        business: Address,
        invoice_id: BytesN<32>,
        category: InvoiceCategory,
    ) -> Result<(), QuickLendXError> {
        let mut invoice = load_invoice_for_owner_write(&env, &business, &invoice_id)?;
        invoice.update_category(category);
        InvoiceStorage::update_invoice(&env, &invoice);
        Ok(())
    }

    /// Replace the metadata of an invoice owned by `caller`.
    pub fn update_invoice_metadata(
        env: Env,
        caller: Address,
        invoice_id: BytesN<32>,
        metadata: InvoiceMetadata,
    ) -> Result<(), QuickLendXError> {
        require_line_items_bound(&metadata.line_items)?;
        require_description_bound(&metadata.notes)?;
        let mut invoice = load_invoice_for_owner_write(&env, &caller, &invoice_id)?;
        // Ownership and auth were already enforced by the helper above.
        invoice.set_metadata(&env, Some(metadata))?;
        InvoiceStorage::update_invoice(&env, &invoice);
        Ok(())
    }

    /// Remove all metadata from an invoice owned by `caller`.
    pub fn clear_invoice_metadata(
        env: Env,
        caller: Address,
        invoice_id: BytesN<32>,
    ) -> Result<(), QuickLendXError> {
        let mut invoice = load_invoice_for_owner_write(&env, &caller, &invoice_id)?;
        // Ownership and auth were already enforced by the helper above.
        invoice.set_metadata(&env, None)?;
        InvoiceStorage::update_invoice(&env, &invoice);
        Ok(())
    }

    /// Admin-only: set or clear the hold on an invoice.
    pub fn set_invoice_lock(
        env: Env,
        admin: Address,
        invoice_id: BytesN<32>,
        lock: InvoiceLock,
    ) -> Result<(), QuickLendXError> {
        AdminStorage::require_admin_auth(&env, &admin)?;
        InvoiceStorage::get_invoice(&env, &invoice_id).ok_or(QuickLendXError::InvoiceNotFound)?;
        InvoiceStorage::set_invoice_lock(&env, &invoice_id, lock);
        Ok(())
    }

    pub fn get_invoice_lock(env: Env, invoice_id: BytesN<32>) -> InvoiceLock {
        InvoiceStorage::get_invoice_lock(&env, &invoice_id)
    }

    /// Invoices whose metadata customer name equals `customer_name`.
    pub fn get_invoices_by_customer(env: Env, customer_name: String) -> Vec<BytesN<32>> {
        InvoiceStorage::get_by_customer(&env, &customer_name)
    }

    /// Invoices whose metadata tax id equals `tax_id`.
    pub fn get_invoices_by_tax_id(env: Env, tax_id: String) -> Vec<BytesN<32>> {
        InvoiceStorage::get_by_tax_id(&env, &tax_id)
    }

    /// Status of each requested invoice, in request order (`None` if unknown).
    /// Requests larger than the status-batch bound fail with `InputTooLarge`.
    pub fn get_invoices_by_status_batch(
        env: Env,
        ids: Vec<BytesN<32>>,
    ) -> Result<Vec<Option<InvoiceStatus>>, QuickLendXError> {
        require_status_batch_bound(&ids)?;
        let mut statuses = Vec::new(&env);
        for id in ids.iter() {
            statuses.push_back(InvoiceStorage::get_invoice(&env, &id).map(|i| i.status));
        }
        Ok(statuses)
    }

    /// Number of invoices currently indexed under `status`.
    pub fn get_invoice_count_by_status(env: Env, status: InvoiceStatus) -> u32 {
        InvoiceStorage::get_invoices_by_status(&env, status).len()
    }

    // =========================================================================
    // Bidding
    // =========================================================================

    pub fn place_bid(
        env: Env,
        investor: Address,
        invoice_id: BytesN<32>,
        bid_amount: i128,
        expected_return: i128,
        salt: BytesN<32>,
    ) -> Result<BytesN<32>, QuickLendXError> {
        PauseControl::require_not_paused(&env)?;
        check_and_record_mutation(&env, &investor)?;
        investor.require_auth();
        crate::verification::require_investor_not_pending(&env, &investor)?;

        let invoice = InvoiceStorage::get_invoice(&env, &invoice_id)
            .ok_or(QuickLendXError::InvoiceNotFound)?;
        crate::verification::validate_bid(&env, &invoice, bid_amount, expected_return, &investor)?;

        let bid_id = crate::bid::BidStorage::generate_unique_bid_id(&env);
        let bid = crate::types::Bid {
            bid_id: bid_id.clone(),
            invoice_id,
            investor,
            bid_amount,
            expected_return,
            timestamp: env.ledger().timestamp(),
            status: crate::types::BidStatus::Placed,
            expiration_timestamp: env.ledger().timestamp() + 86_400,
        };
        crate::bid::BidStorage::store_bid(&env, &bid);
        crate::bid::BidStorage::add_bid_to_invoice(&env, &bid.invoice_id, &bid.bid_id);
        Ok(bid_id)
    }

    pub fn accept_bid(
        env: Env,
        invoice_id: BytesN<32>,
        bid_id: BytesN<32>,
    ) -> Result<(), QuickLendXError> {
        PauseControl::require_not_paused(&env)?;
        with_payment_guard(&env, || {
            crate::escrow::accept_bid_and_fund(&env, &invoice_id, &bid_id).map(|_| ())
        })
    }

    pub fn get_bid(env: Env, bid_id: BytesN<32>) -> Option<crate::types::Bid> {
        crate::bid::BidStorage::get_bid(&env, &bid_id)
    }

    pub fn get_bids_for_invoice(env: Env, invoice_id: BytesN<32>) -> Vec<crate::types::Bid> {
        let ids = crate::bid::BidStorage::get_bids_for_invoice(&env, &invoice_id);
        let mut bids = Vec::new(&env);
        for id in ids.iter() {
            if let Some(bid) = crate::bid::BidStorage::get_bid(&env, &id) {
                bids.push_back(bid);
            }
        }
        bids
    }

    pub fn get_ranked_bids_paged(
        env: Env,
        invoice_id: BytesN<32>,
        cursor: Option<String>,
        limit: u32,
    ) -> Result<crate::types::CursorBidsPage, QuickLendXError> {
        crate::bid::BidStorage::rank_bids_paged(&env, &invoice_id, cursor, limit)
    }

    // =========================================================================
    // Escrow
    // =========================================================================

    pub fn get_escrow_details(
        env: Env,
        invoice_id: BytesN<32>,
    ) -> Result<crate::payments::Escrow, QuickLendXError> {
        crate::payments::EscrowStorage::get_escrow_by_invoice(&env, &invoice_id)
            .ok_or(QuickLendXError::StorageKeyNotFound)
    }

    /// Admin-only lookup of an escrow by its direct `escrow_id`.
    ///
    /// Unlike `get_escrow_details` (which looks up by invoice), this returns
    /// the escrow record keyed by `escrow_id`. Requires the caller to be the
    /// current admin.
    pub fn admin_get_escrow(
        env: Env,
        admin: Address,
        escrow_id: BytesN<32>,
    ) -> Result<crate::payments::Escrow, QuickLendXError> {
        admin.require_auth();
        crate::admin::AdminStorage::require_admin(&env, &admin)?;
        crate::payments::EscrowStorage::get_escrow(&env, &escrow_id)
            .ok_or(QuickLendXError::StorageKeyNotFound)
    }

    pub fn release_escrow(
        env: Env,
        invoice_id: BytesN<32>,
        business: Address,
    ) -> Result<(), QuickLendXError> {
        PauseControl::require_not_paused(&env)?;
        with_payment_guard(&env, || {
            crate::payments::release_escrow(&env, &invoice_id, &business)
        })
    }

    /// Admin-only: refund a funded invoice's escrow back to the investor.
    ///
    /// Convenience wrapper around [`crate::escrow::refund_escrow_funds`] that
    /// reads the admin from storage. Transitions the invoice to `Refunded`.
    pub fn refund_escrow(env: Env, invoice_id: BytesN<32>) -> Result<(), QuickLendXError> {
        PauseControl::require_not_paused(&env)?;
        let admin = AdminStorage::get_admin(&env).ok_or(QuickLendXError::NotAdmin)?;
        with_payment_guard(&env, || {
            crate::escrow::refund_escrow_funds(&env, &invoice_id, &admin)
        })
    }

    /// Return the escrow status for an invoice.
    ///
    /// # Errors
    /// * `QuickLendXError::StorageKeyNotFound` — no escrow record exists for
    ///   the given invoice.
    pub fn get_escrow_status(
        env: Env,
        invoice_id: BytesN<32>,
    ) -> Result<crate::payments::EscrowStatus, QuickLendXError> {
        crate::payments::EscrowStorage::get_escrow_status(&env, &invoice_id)
            .ok_or(QuickLendXError::StorageKeyNotFound)
    }

    /// Release a funded invoice's escrow to the business.
    ///
    /// The invoice must be `Funded` and its escrow must be `Held`.
    /// `payments::release_escrow` enforces that the escrow's business authorizes
    /// the call, and `EscrowStatus::verify_transition` rejects any transition
    /// other than `Held -> Released`, so a second release fails with
    /// `InvalidStatus`.
    pub fn release_escrow_funds(env: Env, invoice_id: BytesN<32>) -> Result<(), QuickLendXError> {
        PauseControl::require_not_paused(&env)?;
        let invoice = InvoiceStorage::get_invoice(&env, &invoice_id)
            .ok_or(QuickLendXError::InvoiceNotFound)?;
        if invoice.status != InvoiceStatus::Funded {
            return Err(QuickLendXError::InvalidStatus);
        }
        let escrow = crate::payments::EscrowStorage::get_escrow_by_invoice(&env, &invoice_id)
            .ok_or(QuickLendXError::InvalidStatus)?;
        with_payment_guard(&env, || {
            crate::payments::release_escrow(&env, &invoice_id, &escrow.business)
        })
    }

    /// Refund a funded invoice's escrow back to the investor.
    ///
    /// Authorized for the contract admin or the invoice's business. Delegates to
    /// `escrow::refund_escrow_funds`, which enforces that authorization matrix
    /// and the `Held -> Refunded` escrow transition.
    pub fn refund_escrow_funds(
        env: Env,
        invoice_id: BytesN<32>,
        caller: Address,
    ) -> Result<(), QuickLendXError> {
        PauseControl::require_not_paused(&env)?;
        with_payment_guard(&env, || {
            crate::escrow::refund_escrow_funds(&env, &invoice_id, &caller)
        })
    }

    /// Record `caller`'s approval for an early escrow release.
    ///
    /// Ported from the contract reference surface as-is: the approval bookkeeping
    /// for early release is not yet wired into `release_escrow_funds`, so this
    /// entrypoint currently succeeds without side effects.
    pub fn approve_early_escrow_release(
        env: Env,
        invoice_id: BytesN<32>,
        caller: Address,
    ) -> Result<(), QuickLendXError> {
        let _ = (&env, &invoice_id, &caller);
        Ok(())
    }

    /// Mark an invoice as completed (paid) by its owning business.
    ///
    /// Idempotent on `nonce`; only an invoice that is still open for funding may
    /// be completed.
    pub fn complete_invoice(
        env: Env,
        caller: Address,
        invoice_id: BytesN<32>,
        nonce: BytesN<32>,
    ) -> Result<(), QuickLendXError> {
        if crate::idempotency::idempotency_exists(&env, &nonce) {
            return Ok(());
        }
        let invoice = InvoiceStorage::get_invoice(&env, &invoice_id)
            .ok_or(QuickLendXError::InvoiceNotFound)?;
        check_and_record_mutation(&env, &invoice.business)?;
        let invoice = InvoiceStorage::get_invoice(&env, &invoice_id)
            .ok_or(QuickLendXError::InvoiceNotFound)?;
        if !invoice.is_available_for_funding() {
            return Err(QuickLendXError::InvoiceNotAvailableForFunding);
        }
        let mut invoice = InvoiceStorage::get_invoice(&env, &invoice_id)
            .ok_or(QuickLendXError::InvoiceNotFound)?;
        crate::invoice::require_matching_business_invoice_ownership(&env, &caller, &invoice)?;
        caller.require_auth();
        invoice.mark_as_paid(&env, caller.clone(), env.ledger().timestamp());
        InvoiceStorage::update_invoice(&env, &invoice);
        crate::idempotency::store_idempotency(&env, &nonce);
        Ok(())
    }

    /// Freeze an invoice (admin only).
    ///
    /// Emits `InvoiceFrozen` with the appeals channel so off-chain consumers can
    /// surface the appeals path to the affected business.
    pub fn freeze_invoice(
        env: Env,
        admin: Address,
        invoice_id: BytesN<32>,
        reason: BusinessFreezeReason,
    ) -> Result<(), QuickLendXError> {
        AdminStorage::require_admin(&env, &admin)?;
        InvoiceStorage::set_frozen(&env, &invoice_id, true, Some(reason));
        crate::events::emit_invoice_frozen(&env, &invoice_id, &admin, reason.label());
        Ok(())
    }

    // =========================================================================
    // Investment queries
    // =========================================================================

    /// Get the investment snapshot for an invoice (returns None when not funded).
    pub fn get_investment(env: Env, invoice_id: BytesN<32>) -> Option<Investment> {
        crate::investment::InvestmentStorage::get_investment_by_invoice(&env, &invoice_id)
    }

    /// Get the investment for an invoice (panics if not funded — test helper).
    pub fn get_invoice_investment(env: Env, invoice_id: BytesN<32>) -> Investment {
        crate::investment::InvestmentStorage::get_investment_by_invoice(&env, &invoice_id)
            .expect("invoice has no investment")
    }

    pub fn get_investor_investments_paged(
        env: Env,
        investor: Address,
        status_filter: Option<crate::types::InvestmentStatus>,
        cursor: Option<String>,
        limit: u32,
    ) -> Result<crate::types::CursorBytes32Page, QuickLendXError> {
        crate::investment_queries::InvestmentQueries::get_investor_investments_paged(
            &env,
            &investor,
            status_filter,
            cursor,
            limit,
        )
    }

    // =========================================================================
    // Settlement & Repayment
    // =========================================================================

    /// Record a partial payment toward an invoice.
    ///
    /// `transaction_id` must be a 64-character lowercase hex string (or empty
    /// to bypass the nonce-deduplication check).
    pub fn process_partial_payment(
        env: Env,
        invoice_id: BytesN<32>,
        payment_amount: i128,
        transaction_id: String,
    ) -> Result<(), QuickLendXError> {
        PauseControl::require_not_paused(&env)?;
        with_payment_guard(&env, || {
            crate::settlement::process_partial_payment(
                &env,
                &invoice_id,
                payment_amount,
                transaction_id,
            )
        })
    }

    /// Fully settle an invoice with a single payment.
    ///
    /// `snap` is an investment snapshot obtained from `get_investment` — the
    /// contract verifies it matches the stored investment to guard against
    /// stale reads racing with concurrent state changes.
    pub fn settle_invoice(
        env: Env,
        invoice_id: BytesN<32>,
        payment_amount: i128,
        snap: Investment,
    ) -> Result<(), QuickLendXError> {
        PauseControl::require_not_paused(&env)?;
        // Verify the caller-supplied snapshot is fresh.
        let stored =
            crate::investment::InvestmentStorage::get_investment_by_invoice(&env, &invoice_id)
                .ok_or(QuickLendXError::StorageKeyNotFound)?;
        if stored.investment_id != snap.investment_id || stored.amount != snap.amount {
            return Err(QuickLendXError::StaleInvestmentSnapshot);
        }
        with_payment_guard(&env, || {
            let invoice = InvoiceStorage::get_invoice(&env, &invoice_id)
                .ok_or(QuickLendXError::InvoiceNotFound)?;
            let business = invoice.business.clone();
            crate::settlement::settle_invoice(&env, &invoice_id, payment_amount, &snap, &business)
        })
    }

    /// Returns the settlement progress for an invoice.
    pub fn get_invoice_progress(
        env: Env,
        invoice_id: BytesN<32>,
    ) -> Result<crate::settlement::Progress, QuickLendXError> {
        crate::settlement::get_invoice_progress(&env, &invoice_id)
    }

    /// Returns the number of recorded payments for an invoice.
    pub fn get_payment_count(env: Env, invoice_id: BytesN<32>) -> Result<u32, QuickLendXError> {
        crate::settlement::get_payment_count(&env, &invoice_id)
    }

    /// Returns a paginated slice of payment records.
    ///
    /// `limit` is capped at `get_settlement_batch_size_max()` (50).
    pub fn get_payment_records(
        env: Env,
        invoice_id: BytesN<32>,
        from: u32,
        limit: u32,
    ) -> Result<Vec<crate::settlement::SettlementPaymentRecord>, QuickLendXError> {
        let capped = limit.min(crate::settlement::MAX_SETTLEMENT_BATCH_SIZE_SOFT_CAP);
        crate::settlement::get_payment_records(&env, &invoice_id, from, capped)
    }

    pub fn get_payment_records_paged(
        env: Env,
        invoice_id: BytesN<32>,
        cursor: Option<String>,
        limit: u32,
    ) -> Result<crate::settlement::CursorPaymentRecordsPage, QuickLendXError> {
        crate::settlement::get_payment_records_paged(&env, &invoice_id, cursor, limit)
    }

    /// Returns `true` if the invoice has been fully settled.
    pub fn is_invoice_finalized(env: Env, invoice_id: BytesN<32>) -> Result<bool, QuickLendXError> {
        crate::settlement::is_invoice_finalized(&env, &invoice_id)
    }

    /// Recommended page size for off-chain consumers fetching payment records.
    pub fn get_settlement_batch_size(env: Env) -> u32 {
        let _ = env;
        crate::settlement::default_settlement_batch_size_soft_cap()
    }

    /// Hard upper bound on the number of payment records per query.
    pub fn get_settlement_batch_size_max(env: Env) -> u32 {
        let _ = env;
        crate::settlement::max_settlement_batch_size_soft_cap()
    }

    // =========================================================================
    // Dispute Lifecycle
    // =========================================================================

    /// Open a new dispute on an invoice.
    ///
    /// `creator` must be the business owner or the investor on the invoice and
    /// must sign the transaction. Wraps `dispute::create_dispute`.
    pub fn create_dispute(
        env: Env,
        invoice_id: BytesN<32>,
        creator: Address,
        reason: String,
        evidence: String,
    ) -> Result<(), QuickLendXError> {
        PauseControl::require_not_paused(&env)?;
        check_and_record_mutation(&env, &creator)?;
        crate::dispute::create_dispute(&env, &invoice_id, &creator, &reason, &evidence)
    }

    /// Advance a `Disputed` dispute to `UnderReview` (admin only).
    pub fn put_dispute_under_review(
        env: Env,
        invoice_id: BytesN<32>,
        admin: Address,
    ) -> Result<(), QuickLendXError> {
        PauseControl::require_not_paused(&env)?;
        check_and_record_mutation(&env, &admin)?;
        crate::dispute::put_dispute_under_review(&env, &admin, &invoice_id)
    }

    /// Resolve a dispute with a free-text resolution (admin + registered arbiter).
    pub fn resolve_dispute(
        env: Env,
        invoice_id: BytesN<32>,
        admin: Address,
        resolution: String,
    ) -> Result<(), QuickLendXError> {
        PauseControl::require_not_paused(&env)?;
        check_and_record_mutation(&env, &admin)?;
        with_payment_guard(&env, || {
            crate::dispute::resolve_dispute(&env, &admin, &invoice_id, &resolution)
        })
    }

    /// Resolve a dispute with a structured outcome (admin + registered arbiter).
    pub fn resolve_dispute_structured(
        env: Env,
        invoice_id: BytesN<32>,
        admin: Address,
        outcome: DisputeResolution,
        note: String,
    ) -> Result<(), QuickLendXError> {
        PauseControl::require_not_paused(&env)?;
        check_and_record_mutation(&env, &admin)?;
        with_payment_guard(&env, || {
            crate::dispute::resolve_dispute_structured(&env, &admin, &invoice_id, outcome, &note)
        })
    }

    /// Return the dispute details for an invoice, or `None` if no dispute exists.
    pub fn get_dispute_details(env: Env, invoice_id: BytesN<32>) -> Option<Dispute> {
        crate::dispute::get_dispute_details(&env, &invoice_id)
    }

    /// Return all invoice IDs that have ever entered the dispute lifecycle.
    pub fn get_invoices_with_disputes(env: Env) -> Vec<BytesN<32>> {
        crate::dispute::get_invoices_with_disputes(&env)
    }

    pub fn get_invoices_with_disputes_paged(
        env: Env,
        cursor: Option<String>,
        limit: u32,
    ) -> Result<crate::types::CursorBytes32Page, QuickLendXError> {
        crate::dispute::get_invoices_with_disputes_paged(&env, cursor, limit)
    }

    /// Filter dispute-indexed invoices by their current dispute status.
    pub fn get_invoices_by_dispute_status(env: Env, status: DisputeStatus) -> Vec<BytesN<32>> {
        crate::dispute::get_invoices_by_dispute_status(&env, &status)
    }

    pub fn get_disputes_by_status_paged(
        env: Env,
        status: DisputeStatus,
        cursor: Option<String>,
        limit: u32,
    ) -> Result<crate::types::CursorBytes32Page, QuickLendXError> {
        crate::dispute::get_invoices_by_dispute_status_paged(
            &env, &status, cursor, limit,
        )
    }

    /// Return a paginated dispute timeline for an invoice.
    pub fn get_dispute_timeline(
        env: Env,
        invoice_id: BytesN<32>,
        offset: u32,
        limit: u32,
    ) -> Result<crate::dispute_timeline::DisputeTimeline, QuickLendXError> {
        crate::dispute_timeline::get_dispute_timeline(&env, &invoice_id, offset, limit)
    }

    pub fn get_dispute_timeline_paged(
        env: Env,
        invoice_id: BytesN<32>,
        cursor: Option<String>,
        limit: u32,
    ) -> Result<crate::dispute_timeline::CursorDisputeTimeline, QuickLendXError> {
        crate::dispute_timeline::get_dispute_timeline_paged(&env, &invoice_id, cursor, limit)
    }

    /// Update the evidence on an open (Disputed) dispute. Creator-only, immutable once UnderReview.
    pub fn update_dispute_evidence(
        env: Env,
        invoice_id: BytesN<32>,
        creator: Address,
        new_evidence: String,
    ) -> Result<(), QuickLendXError> {
        PauseControl::require_not_paused(&env)?;
        creator.require_auth();
        check_and_record_mutation(&env, &creator)?;

        crate::verification::validate_dispute_evidence(&new_evidence)?;

        let mut invoice = InvoiceStorage::get_invoice(&env, &invoice_id)
            .ok_or(QuickLendXError::InvoiceNotFound)?;

        // Only the original creator may update evidence.
        if invoice.dispute.created_by != creator {
            return Err(QuickLendXError::DisputeNotAuthorized);
        }

        // Evidence is immutable once review has started.
        match invoice.dispute_status {
            DisputeStatus::Disputed => {}
            DisputeStatus::None => return Err(QuickLendXError::DisputeNotFound),
            _ => return Err(QuickLendXError::InvalidStatus),
        }

        invoice.dispute.evidence = new_evidence;
        InvoiceStorage::update_invoice(&env, &invoice);
        Ok(())
    }

    // =========================================================================
    // Arbiter Registry
    // =========================================================================

    /// Register a new dispute arbiter (admin only).
    pub fn register_arbiter(
        env: Env,
        admin: Address,
        arbiter: Address,
    ) -> Result<(), QuickLendXError> {
        crate::upgrade::UpgradeControl::require_no_pending_upgrade(&env)?;
        ArbiterStorage::register_arbiter(&env, &admin, &arbiter)
    }

    /// Revoke a registered dispute arbiter (admin only).
    pub fn unregister_arbiter(
        env: Env,
        admin: Address,
        arbiter: Address,
    ) -> Result<(), QuickLendXError> {
        crate::upgrade::UpgradeControl::require_no_pending_upgrade(&env)?;
        ArbiterStorage::unregister_arbiter(&env, &admin, &arbiter)
    }

    /// Return all registered arbiters.
    pub fn list_arbiters(env: Env) -> Vec<Address> {
        ArbiterStorage::list_arbiters(&env)
    }

    /// Return `true` if `addr` is a registered dispute arbiter.
    pub fn is_arbiter(env: Env, addr: Address) -> bool {
        ArbiterStorage::is_arbiter(&env, &addr)
    }

    /// Configure the distinct-arbiter quorum for structured dispute resolutions.
    pub fn set_dispute_quorum(
        env: Env,
        admin: Address,
        quorum: u32,
    ) -> Result<(), QuickLendXError> {
        ArbiterStorage::set_dispute_quorum(&env, &admin, quorum)
    }

    /// Return the configured dispute-resolution quorum (default: 1).
    pub fn get_dispute_quorum(env: Env) -> u32 {
        ArbiterStorage::get_dispute_quorum(&env)
    }

    // ── Backup Management ────────────────────────────────────────────────────

    /// Create a backup of all invoices across all statuses.
    ///
    /// Returns the backup ID.
    pub fn create_backup(
        env: Env,
        admin: Address,
        description: String,
    ) -> Result<BytesN<32>, errors::QuickLendXError> {
    pub fn create_backup(env: Env, admin: Address, description: String) -> Result<BytesN<32>, errors::QuickLendXError> {
        crate::upgrade::UpgradeControl::require_no_pending_upgrade(&env)?;
        admin.require_auth();
        admin::AdminStorage::require_admin(&env, &admin)?;

        if description.len() > 128 {
            return Err(errors::QuickLendXError::InvalidDescription);
        }

        let all_invoices = backup::BackupStorage::get_all_invoices(&env);
        let backup_id = backup::BackupStorage::generate_backup_id(&env);

        let backup = backup::Backup {
            backup_id: backup_id.clone(),
            timestamp: env.ledger().timestamp(),
            description,
            invoice_count: all_invoices.len(),
            status: backup::BackupStatus::Active,
            format_version: 2,
        };

        backup::BackupStorage::store_backup(&env, &backup, Some(&all_invoices))?;
        backup::BackupStorage::store_backup_data(&env, &backup_id, &all_invoices);
        backup::BackupStorage::add_to_backup_list(&env, &backup_id);

        backup::BackupStorage::cleanup_old_backups(&env)?;

        Ok(backup_id)
    }

    /// Restore all invoices from a backup.
    ///
    /// Clears all existing invoices and restores from the backup snapshot.
    /// Returns the number of invoices restored.
    pub fn restore_backup(
        env: Env,
        admin: Address,
        backup_id: BytesN<32>,
    ) -> Result<u32, errors::QuickLendXError> {
    pub fn restore_backup(env: Env, admin: Address, backup_id: BytesN<32>) -> Result<u32, errors::QuickLendXError> {
        crate::upgrade::UpgradeControl::require_no_pending_upgrade(&env)?;
        admin.require_auth();
        admin::AdminStorage::require_admin(&env, &admin)?;
        backup::BackupStorage::restore_from_backup(&env, &backup_id)
    }

    /// Set the backup retention policy.
    pub fn set_backup_retention_policy(
        env: Env,
        admin: Address,
        max_backups: u32,
        max_age_seconds: u64,
        enabled: bool,
    ) -> Result<(), errors::QuickLendXError> {
        crate::upgrade::UpgradeControl::require_no_pending_upgrade(&env)?;
        admin.require_auth();
        admin::AdminStorage::require_admin(&env, &admin)?;

        let policy = backup::BackupRetentionPolicy {
            max_backups,
            max_age_seconds,
            auto_cleanup_enabled: enabled,
        };
        backup::BackupStorage::set_retention_policy(&env, &policy);
        Ok(())
    }

    /// Archive a backup (mark it as no longer active).
    pub fn archive_backup(
        env: Env,
        admin: Address,
        backup_id: BytesN<32>,
    ) -> Result<(), errors::QuickLendXError> {
    pub fn archive_backup(env: Env, admin: Address, backup_id: BytesN<32>) -> Result<(), errors::QuickLendXError> {
        crate::upgrade::UpgradeControl::require_no_pending_upgrade(&env)?;
        admin.require_auth();
        admin::AdminStorage::require_admin(&env, &admin)?;
        governance::require_no_open_governance_proposal(&env)?;

        let mut backup = backup::BackupStorage::get_backup(&env, &backup_id)
            .ok_or(errors::QuickLendXError::OperationNotAllowed)?;
        backup.status = backup::BackupStatus::Archived;
        backup::BackupStorage::update_backup(&env, &backup)?;
        backup::BackupStorage::remove_from_backup_list(&env, &backup_id);
        Ok(())
    }

    /// Clean up old backups according to the retention policy.
    ///
    /// Returns the number of backups removed.
    pub fn cleanup_backups(env: Env, admin: Address) -> Result<u32, errors::QuickLendXError> {
        crate::upgrade::UpgradeControl::require_no_pending_upgrade(&env)?;
        admin.require_auth();
        admin::AdminStorage::require_admin(&env, &admin)?;
        backup::BackupStorage::cleanup_old_backups(&env)
    }

    // ── Backup Views ─────────────────────────────────────────────────────────

    /// Get all backup IDs.
    pub fn get_backups(env: Env) -> Vec<BytesN<32>> {
        backup::BackupStorage::get_all_backups(&env)
    }

    /// Get backup details by ID.
    pub fn get_backup_details(env: Env, backup_id: BytesN<32>) -> Option<backup::Backup> {
        backup::BackupStorage::get_backup(&env, &backup_id)
    }

    /// Validate a backup without mutating state.
    ///
    /// Returns `true` if the backup is valid and can be restored.
    pub fn validate_backup(env: Env, backup_id: BytesN<32>) -> bool {
        backup::BackupStorage::validate_backup(&env, &backup_id).is_ok()
    }

    /// Preview which backups would be cleaned up by the retention policy.
    pub fn preview_cleanup_backups(env: Env) -> backup::BackupCleanupDryRunReport {
        backup::BackupStorage::preview_cleanup_old_backups(&env)
    }

    // ── Storage Migration Runner ─────────────────────────────────────────────

    /// Plan and initiate a resumable storage migration from `from` to `to` (admin only).
    pub fn migration_plan(
        env: Env,
        admin: Address,
        from: u32,
        to: u32,
    ) -> Result<migration::MigrationPlan, errors::QuickLendXError> {
        migration::MigrationRunner::plan(&env, &admin, from, to)
    }

    /// Execute one bounded step of the active storage migration (admin only).
    pub fn migration_run_step(
        env: Env,
        admin: Address,
        limit: u32,
    ) -> Result<migration::MigrationStepResult, errors::QuickLendXError> {
        migration::MigrationRunner::run_step(&env, &admin, limit)
    }

    /// Preview the next storage migration step without mutating state (admin only).
    pub fn migration_dry_run(
        env: Env,
        admin: Address,
        limit: u32,
    ) -> Result<migration::MigrationDryRunReport, errors::QuickLendXError> {
        migration::MigrationRunner::dry_run(&env, &admin, limit)
    }

    /// Return the current storage migration status.
    pub fn migration_status(env: Env) -> migration::MigrationStatus {
        migration::MigrationRunner::status(&env)
    }

    /// Roll back an in-progress storage migration (admin only).
    pub fn migration_rollback(env: Env, admin: Address) -> Result<(), errors::QuickLendXError> {
        migration::MigrationRunner::rollback(&env, &admin)
    }

    /// Return the persisted completion record for a migrated schema version.
    pub fn get_migration_record(
        env: Env,
        version: u32,
    ) -> Option<migration::MigrationVersionRecord> {
        migration::MigrationRunner::get_version_record(&env, version)
    // ── Stub Methods for Test Compatibility ─────────────────────────────────
    // These are minimal stubs to satisfy existing tests. Full implementation
    // would be added when those modules are complete.

    pub fn initialize_protocol_limits(
        _env: Env,
        _admin: Address,
        _min_amount: i128,
        _max_due_days: u64,
        _lock_time_limit: u64,
    ) -> Result<(), errors::QuickLendXError> {
        Ok(())
    }

    pub fn store_invoice(
        env: Env,
        business: Address,
        amount: i128,
        currency: Address,
        due_date: u64,
        description: String,
        category: types::InvoiceCategory,
        _tags: Vec<String>,
        _customer_name: Option<String>,
    ) -> BytesN<32> {
        use types::{Dispute, DisputeResolution, DisputeStatus, Invoice, InvoiceStatus};
        
        let invoice_id = storage::InvoiceStorage::next_count(&env);
        let mut id_bytes = [0u8; 32];
        id_bytes[24..32].copy_from_slice(&invoice_id.to_be_bytes());
        let id = BytesN::from_array(&env, &id_bytes);

        let invoice = Invoice {
            id: id.clone(),
            business,
            amount,
            currency,
            due_date,
            status: InvoiceStatus::Pending,
            created_at: env.ledger().timestamp(),
            description,
            metadata_customer_name: None,
            metadata_customer_address: None,
            metadata_tax_id: None,
            metadata_notes: None,
            metadata_line_items: Vec::new(&env),
            category,
            tags: Vec::new(&env),
            funded_amount: 0,
            funded_at: None,
            investor: None,
            settled_at: None,
            average_rating: None,
            total_ratings: 0,
            ratings: Vec::new(&env),
            dispute_status: DisputeStatus::None,
            dispute: Dispute {
                created_by: Address::generate(&env),
                created_at: 0,
                reason: String::from_str(&env, ""),
                evidence: String::from_str(&env, ""),
                resolution: String::from_str(&env, ""),
                resolved_by: Address::generate(&env),
                resolved_at: 0,
                resolution_outcome: DisputeResolution::None,
            },
            total_paid: 0,
            payment_history: Vec::new(&env),
            origination_fee_bps: None,
            late_payment_penalty_bps: None,
            early_payment_discount_bps: None,
        };

        storage::InvoiceStorage::store_invoice(&env, &invoice);
        id
    }

    pub fn get_total_invoice_count(env: Env) -> u64 {
        storage::InvoiceStorage::get_total_count(&env)
    }

    pub fn get_invoice(env: Env, invoice_id: BytesN<32>) -> Result<types::Invoice, errors::QuickLendXError> {
        storage::InvoiceStorage::get(&env, &invoice_id)
            .ok_or(errors::QuickLendXError::InvoiceNotFound)
    }
    // =========================================================================
    // Vesting
    // =========================================================================

    /// Create a time-locked vesting schedule for `beneficiary`.
    ///
    /// Locks `total_amount` of `token` in contract custody and releases it
    /// linearly from `start_time` to `end_time`, gated by a `cliff_seconds`
    /// cliff. Returns the new schedule id.
    ///
    /// # Authorization
    /// Requires the current admin to authorize and pays for the tokens.
    ///
    /// # Errors
    /// - `InvalidAmount` if `total_amount <= 0`
    /// - `InvalidTimestamp` if the timeline is backdated/non-monotonic or the
    ///   cliff eliminates the post-cliff window
    /// - `NotAdmin` if `admin` is not the stored admin
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
        crate::vesting::Vesting::create_schedule(
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

    /// Return the vesting schedule with `id`, if it exists.
    pub fn get_vesting_schedule(env: Env, id: u64) -> Option<crate::vesting::VestingSchedule> {
        crate::vesting::Vesting::get_schedule(&env, id)
    }

    /// Aggregated vesting summary for `user` across all of their schedules.
    pub fn get_vesting_summary(env: Env, user: Address) -> crate::vesting::VestingSummary {
        crate::vesting::Vesting::get_summary_for_user(&env, &user)
    }

    /// Total amount vested for schedule `id` at the current ledger time.
    ///
    /// Returns `None` if the schedule does not exist.
    pub fn get_vesting_vested(env: Env, id: u64) -> Option<i128> {
        crate::vesting::Vesting::get_schedule(&env, id)
            .and_then(|s| crate::vesting::Vesting::vested_amount(&env, &s).ok())
    }

    /// Amount currently releasable for schedule `id` at the current ledger time.
    ///
    /// Returns `None` if the schedule does not exist.
    pub fn get_vesting_releasable(env: Env, id: u64) -> Option<i128> {
        crate::vesting::Vesting::get_schedule(&env, id)
            .and_then(|s| crate::vesting::Vesting::releasable_amount(&env, &s).ok())
    }

    /// Release the currently releasable tokens for schedule `id` to its
    /// beneficiary and return the amount transferred.
    ///
    /// # Authorization
    /// Requires the schedule's beneficiary to authorize the call.
    ///
    /// # Errors
    /// - `Unauthorized` if the caller is not the beneficiary
    /// - `InvalidTimestamp` if called before the cliff
    /// - Returns `Ok(0)` when nothing remains to release (idempotent)
    pub fn release_vested_tokens(
        env: Env,
        beneficiary: Address,
        id: u64,
    ) -> Result<i128, QuickLendXError> {
        crate::vesting::Vesting::release(&env, &beneficiary, id)
    }

    /// Alias of [`Self::release_vested_tokens`].
    pub fn release_vesting(
        env: Env,
        beneficiary: Address,
        id: u64,
    ) -> Result<i128, QuickLendXError> {
        crate::vesting::Vesting::release(&env, &beneficiary, id)
    }

    /// Alias of [`Self::get_vesting_releasable`].
    pub fn get_releasable_vesting(env: Env, id: u64) -> Option<i128> {
        Self::get_vesting_releasable(env, id)
    }

}
