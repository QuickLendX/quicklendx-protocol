use crate::admin::AdminStorage;
use crate::backup::{Backup, BackupRetentionPolicy, BackupStatus, BackupStorage};
use crate::bid::BidStorage;
use crate::errors::QuickLendXError;
use crate::idempotency::{idempotency_exists, idempotency_key, store_idempotency};
use crate::init::{InitializationParams, ProtocolInitializer};
use crate::investment::InvestmentStorage;
use crate::pause::PauseControl;
use crate::payments::{Escrow, EscrowStatus, EscrowStorage};
use crate::protocol_limits::ProtocolLimitsContract;
use crate::protocol_limits::{
    check_and_record_mutation, require_batch_size_bound, require_description_bound,
    require_kyc_data_bound, require_status_batch_bound, require_tags_bound,
};
use crate::reentrancy::with_payment_guard;
use crate::storage::InvoiceStorage;
use crate::types::{
    Bid, BidStatus, BusinessFreezeReason, Invoice, InvoiceCategory, InvoiceLock, InvoiceMetadata,
    InvoiceStatus,
};
use crate::verification::{
    submit_kyc_application, verify_business, BusinessVerificationStorage,
    InvestorVerificationStorage,
};
use soroban_sdk::{contract, contractimpl, Address, BytesN, Env, String, Vec};

#[contract]
pub struct QuickLendXContract;

#[contractimpl]
impl QuickLendXContract {
    // ── Protocol initialization ─────────────────────────────────────────────

    /// Initialize the protocol. `params.admin` must sign. Can succeed once;
    /// repeating the call with identical params returns `Ok(())`, any other
    /// params return `OperationNotAllowed`.
    pub fn initialize(env: Env, params: InitializationParams) -> Result<(), QuickLendXError> {
        ProtocolInitializer::initialize(&env, &params)
    }

    /// `true` once `initialize` has completed.
    pub fn is_initialized(env: Env) -> bool {
        ProtocolInitializer::is_initialized(&env)
    }

    /// Protocol version recorded at initialization (or the current constant
    /// before initialization).
    pub fn get_version(env: Env) -> u32 {
        ProtocolInitializer::get_version(&env)
    }

    /// Current protocol configuration, `None` before initialization.
    pub fn get_protocol_config(env: Env) -> Option<ProtocolConfig> {
        ProtocolInitializer::get_protocol_config(&env)
    }

    /// Dry-run a configuration change (admin only, read-only).
    pub fn preview_protocol_config(
        env: Env,
        admin: Address,
        params: ProtocolConfigParams,
    ) -> Result<ProtocolConfigDiff, QuickLendXError> {
        ProtocolInitializer::preview_protocol_config(&env, &admin, params)
    }

    /// Initialize protocol limits.
    pub fn initialize_protocol_limits(env: Env, admin: Address) -> Result<(), QuickLendXError> {
        ProtocolLimitsContract::initialize(env, admin)
    }

    /// Update the protocol fee in basis points (admin only).
    pub fn set_fee_config(env: Env, admin: Address, fee_bps: u32) -> Result<(), QuickLendXError> {
        ProtocolInitializer::set_fee_config(&env, &admin, fee_bps)
    }

    /// Update the treasury address (admin only).
    pub fn set_treasury(
        env: Env,
        admin: Address,
        treasury: Address,
    ) -> Result<(), QuickLendXError> {
        ProtocolInitializer::set_treasury(&env, &admin, &treasury)
    }

    pub fn get_fee_bps(env: Env) -> u32 {
        ProtocolInitializer::get_fee_bps(&env)
    }

    pub fn get_treasury(env: Env) -> Option<Address> {
        ProtocolInitializer::get_treasury(&env)
    }

    pub fn get_min_invoice_amount(env: Env) -> i128 {
        ProtocolInitializer::get_min_invoice_amount(&env)
    }

    pub fn get_max_due_date_days(env: Env) -> u64 {
        ProtocolInitializer::get_max_due_date_days(&env)
    }

    pub fn get_grace_period_seconds(env: Env) -> u64 {
        ProtocolInitializer::get_grace_period_seconds(&env)
    }

    pub fn get_corridors(env: Env) -> Vec<Address> {
        ProtocolInitializer::get_corridors(&env)
    }

    // ── Admin management ────────────────────────────────────────────────────

    /// Current admin, `None` before initialization.
    pub fn get_current_admin(env: Env) -> Option<Address> {
        AdminStorage::get_admin(&env)
    }

    /// Transfer admin. With two-step mode enabled this only records
    /// `new_admin` as pending; `accept_admin_transfer` completes it.
    pub fn transfer_admin(
        env: Env,
        current_admin: Address,
        new_admin: Address,
    ) -> Result<(), QuickLendXError> {
        AdminStorage::transfer_admin(&env, &current_admin, &new_admin)
    }

    /// Start a two-step transfer to `pending_admin`.
    pub fn initiate_admin_transfer(
        env: Env,
        current_admin: Address,
        pending_admin: Address,
    ) -> Result<(), QuickLendXError> {
        AdminStorage::initiate_admin_transfer(&env, &current_admin, &pending_admin)
    }

    /// Complete a two-step transfer; `pending_admin` must sign.
    pub fn accept_admin_transfer(env: Env, pending_admin: Address) -> Result<(), QuickLendXError> {
        AdminStorage::accept_admin_transfer(&env, &pending_admin)
    }

    /// Cancel a pending two-step transfer (current admin only).
    pub fn cancel_admin_transfer(env: Env, current_admin: Address) -> Result<(), QuickLendXError> {
        AdminStorage::cancel_admin_transfer(&env, &current_admin)
    }

    /// Enable or disable two-step admin transfers (admin only).
    pub fn set_two_step_enabled(
        env: Env,
        admin: Address,
        enabled: bool,
    ) -> Result<(), QuickLendXError> {
        AdminStorage::set_two_step_enabled(&env, &admin, enabled)
    }

    pub fn is_two_step_enabled(env: Env) -> bool {
        AdminStorage::is_two_step_enabled(&env)
    }

    pub fn get_pending_admin(env: Env) -> Option<Address> {
        AdminStorage::get_pending_admin(&env)
    }

    // ── Invoice creation and lifecycle ──────────────────────────────────────

    /// Create an invoice for a KYC-verified `business` (which must sign) and
    /// return its ID.
    ///
    /// Order of checks: business signature; idempotency (a repeat of the same
    /// `(business, nonce)` returns the original ID); rate limit and input size
    /// bounds; KYC, regulatory, currency whitelist and protocol limits;
    /// `Invoice::new` (amount ceiling, due date, bps, tags); storage; event,
    /// audit entry and notification; idempotency record.
    #[allow(clippy::too_many_arguments)]
    pub fn store_invoice(
        env: Env,
        business: Address,
        amount: i128,
        currency: Address,
        due_date: u64,
        description: String,
        category: InvoiceCategory,
        tags: Vec<String>,
        nonce: BytesN<32>,
    ) -> Result<BytesN<32>, QuickLendXError> {
        if let Some(existing_id) =
            crate::idempotency::get_idempotency_result::<BytesN<32>>(&env, &nonce)
        {
            return Ok(existing_id);
        }

        business.require_auth();

        // 2. Idempotency: the key is bound to the business, so another
        //    business reusing this nonce gets its own invoice, not this ID.
        let idem_key = crate::idempotency::invoice_create_key(&env, &business, &nonce);
        if let Some(existing) =
            crate::idempotency::get_idempotency_result::<BytesN<32>>(&env, &idem_key)
        {
            return Ok(existing);
        }
        require_writable(&env)?;

        // 3. Rate limit and hard input-size ceilings.
        check_and_record_mutation(&env, &business)?;
        if description.len() > MAX_INPUT_DESCRIPTION_BYTES {
            return Err(QuickLendXError::InputTooLarge);
        }
        require_tags_bound(&tags)?;

        // 4. KYC, regulatory, currency and protocol limits.
        crate::verification::require_business_not_pending(&env, &business)?;
        crate::regulatory::require_regulatory_ok(&env, &business)?;
        CurrencyWhitelist::require_allowed_currency(&env, &currency)?;
        ProtocolLimitsContract::validate_invoice(env.clone(), amount, due_date)?;
        check_invoice_limit(&env, &business)?;

        // #2432 — exact sign/ceiling validation before any state is written.
        // Semantically identical to the historical inline predicate
        // (`amount <= 0 || amount > MAX_INVOICE_AMOUNT`); see `invoice_amount`.
        crate::invoice_amount::validate_invoice_amount_ceiling(amount)?;

        crate::protocol_limits::check_invoice_limit(&env, &business)?;

        let invoice = Invoice::new(
            &env,
            business,
            amount,
            currency,
            due_date,
            description,
            category,
            tags,
            None,
            None,
            None,
        )?;
        let invoice_id = invoice.id.clone();

        // 6. Persist with all secondary indexes.
        InvoiceStorage::store_invoice(&env, &invoice);

        // 7. Event, audit trail, notification (a notification failure must
        //    not undo the invoice).
        crate::events::emit_invoice_uploaded(&env, &invoice);
        crate::audit::log_invoice_created(&env, &invoice);
        let _ = crate::notifications::NotificationSystem::notify_invoice_created(&env, &invoice);

        // 8. Remember the result for this (business, nonce).
        crate::idempotency::store_idempotency_result(&env, &idem_key, &invoice.id);
        Ok(invoice.id)
    }

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
    ) -> BytesN<32> {
        let invoice = Invoice::new(
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
        )
        .unwrap();
        let invoice_id = invoice.id.clone();
        InvoiceStorage::store_invoice(&env, &invoice);
        invoice_id
    }

    pub fn get_invoice(env: Env, invoice_id: BytesN<32>) -> Result<Invoice, QuickLendXError> {
        load_invoice(&env, &invoice_id)
    }

    pub fn update_invoice_status(
        env: Env,
        admin: Address,
        invoice_id: BytesN<32>,
        status: InvoiceStatus,
    ) -> Result<(), QuickLendXError> {
        AdminStorage::require_admin(&env, &admin)?;
        let mut invoice =
            InvoiceStorage::get(&env, &invoice_id).ok_or(QuickLendXError::InvoiceNotFound)?;
        invoice.status = status;
        InvoiceStorage::update_invoice(&env, &invoice);
        Ok(())
    }

     pub fn verify_invoice(env: Env, admin: Address, invoice_id: BytesN<32>) -> Result<(), QuickLendXError> {
        AdminStorage::require_admin(&env, &admin)?;
        let mut invoice = InvoiceStorage::get(&env, &invoice_id).ok_or(QuickLendXError::InvoiceNotFound)?;

        // Issue #300: re-verifying an already-funded invoice triggers the
        // pending escrow release instead of downgrading the invoice status.
        if invoice.status == InvoiceStatus::Funded {
            if let Some(escrow) = EscrowStorage::get_escrow_by_invoice(&env, &invoice_id) {
                if escrow.status == crate::payments::EscrowStatus::Held {
                    with_payment_guard(&env, || {
                        crate::payments::release_escrow(&env, &invoice_id, &escrow.business)
                    })?;
                }
            }
            return Ok(());
        }

        invoice.verify(&env, admin.clone());
        InvoiceStorage::update_invoice(&env, &invoice);
        Ok(())
    }

    pub fn place_bid(
        env: Env,
        investor: Address,
        invoice_id: BytesN<32>,
        bid_amount: i128,
        expected_return: i128,
        salt: BytesN<32>,
    ) -> Result<BytesN<32>, QuickLendXError> {
        // Idempotency check
        let idem_key = idempotency_key(&invoice_id, &investor, &salt, &env);
        if idempotency_exists(&env, &idem_key) {
            return Err(QuickLendXError::DuplicateBid);
        }
        // #2439 – per-address rate limit
        check_and_record_mutation(&env, &investor)?;

        // Bidding is only permitted while the invoice is open for funding.
        // Pending (unverified) and any terminal/Funded state must reject new bids.
        let invoice = InvoiceStorage::get_invoice(&env, &invoice_id)
            .ok_or(QuickLendXError::InvoiceNotFound)?;
        if !invoice.is_available_for_funding() {
            return Err(QuickLendXError::InvoiceNotAvailableForFunding);
        }

        if InvoiceStorage::is_frozen(&env, &invoice_id) {
            InvoiceStorage::require_lock_within_time_limit(&env, &invoice_id)?;
            return Err(QuickLendXError::InvoiceFrozen);
        }
        // Store idempotency marker
        store_idempotency(&env, &idem_key);
        let bid_id = BidStorage::generate_unique_bid_id(&env);
        let bid = Bid {
            bid_id: bid_id.clone(),
            invoice_id,
            investor,
            bid_amount,
            expected_return,
            status: BidStatus::Placed,
            timestamp: env.ledger().timestamp(),
            expiration_timestamp: env.ledger().timestamp() + 86400,
        };
        BidStorage::store_bid(&env, &bid);
        Ok(bid_id)
    }

    pub fn accept_bid_and_fund(
        env: Env,
        invoice_id: BytesN<32>,
        bid_id: BytesN<32>,
    ) -> Result<BytesN<32>, QuickLendXError> {
        PauseControl::require_not_paused(&env)?;
        with_payment_guard(&env, || {
            crate::escrow::accept_bid_and_fund(&env, &invoice_id, &bid_id)
        })
    }

    pub fn accept_bid_and_fund_with_key(
        env: Env,
        invoice_id: BytesN<32>,
        bid_id: BytesN<32>,
        request_key: BytesN<32>,
    ) -> Result<BytesN<32>, QuickLendXError> {
        PauseControl::require_not_paused(&env)?;
        with_payment_guard(&env, || {
            crate::escrow::accept_bid_and_fund_with_key(&env, &invoice_id, &bid_id, &request_key)
        })
    }

    pub fn accept_bid(
        env: Env,
        invoice_id: BytesN<32>,
        bid_id: BytesN<32>,
    ) -> Result<(), QuickLendXError> {
        Self::accept_bid_and_fund(env, invoice_id, bid_id).map(|_| ())
    }

    pub fn get_bid(env: Env, bid_id: BytesN<32>) -> Option<Bid> {
        BidStorage::get_bid(&env, &bid_id)
    }

    pub fn get_bids_for_invoice(env: Env, invoice_id: BytesN<32>) -> Vec<Bid> {
        let ids = BidStorage::get_bids_for_invoice(&env, &invoice_id);
        let mut bids = Vec::new(&env);
        for id in ids.iter() {
            if let Some(bid) = BidStorage::get_bid(&env, &id) {
                bids.push_back(bid);
            }
        }
        bids
    }

    pub fn withdraw_bid(env: Env, bid_id: BytesN<32>) -> Result<(), QuickLendXError> {
        let mut bid = BidStorage::get_bid(&env, &bid_id).unwrap();
        bid.status = BidStatus::Withdrawn;
        BidStorage::store_bid(&env, &bid);
        Ok(())
    }

    pub fn cleanup_expired_bids(env: Env, invoice_id: BytesN<32>) -> u32 {
        BidStorage::cleanup_expired_bids(&env, &invoice_id)
    }

    pub fn get_ranked_bids(env: Env, invoice_id: BytesN<32>) -> Vec<Bid> {
        BidStorage::rank_bids(&env, &invoice_id)
    }

    pub fn get_ranked_bids_paged(
        env: Env,
        invoice_id: BytesN<32>,
        offset: u32,
        limit: u32,
    ) -> crate::types::PaginatedBids {
        let (items, total_count, has_more) =
            BidStorage::rank_bids_paged(&env, &invoice_id, offset, limit);
        crate::types::PaginatedBids {
            items,
            total_count,
            has_more,
        }
    }

    pub fn get_best_bid(env: Env, invoice_id: BytesN<32>) -> Option<Bid> {
        BidStorage::get_best_bid(&env, &invoice_id)
    }

    pub fn get_bids_by_status(env: Env, invoice_id: BytesN<32>, status: BidStatus) -> Vec<Bid> {
        BidStorage::get_bids_by_status(&env, &invoice_id, status)
    }

    pub fn get_bids_by_investor(env: Env, invoice_id: BytesN<32>, investor: Address) -> Vec<Bid> {
        BidStorage::get_bids_by_investor(&env, &invoice_id, &investor)
    }

    pub fn get_investor_active_exposure(
        env: Env,
        investor: Address,
    ) -> Result<i128, QuickLendXError> {
        crate::payments::get_investor_exposure(&env, &investor)
    }

    pub fn get_investor_available_capacity(
        env: Env,
        investor: Address,
    ) -> Result<i128, QuickLendXError> {
        crate::payments::get_investor_available_capacity(&env, &investor)
    }

    pub fn validate_funding_commitment(
        env: Env,
        investor: Address,
        amount: i128,
    ) -> Result<(), QuickLendXError> {
        crate::payments::validate_funding_commitment(&env, &investor, amount)
    }

    pub fn submit_kyc_application(
        env: Env,
        business: Address,
        kyc_data: String,
    ) -> Result<(), QuickLendXError> {
        // #2439 – input-size ceiling before any expensive work
        require_kyc_data_bound(&kyc_data)?;
        check_and_record_mutation(&env, &business)?;
        submit_kyc_application(&env, &business, kyc_data)
    }

    pub fn freeze_invoice(
        env: Env,
        admin: Address,
        invoice_id: BytesN<32>,
        reason: BusinessFreezeReason,
    ) -> Result<(), QuickLendXError> {
        crate::admin::AdminStorage::require_admin(&env, &admin)?;
        InvoiceStorage::set_frozen(&env, &invoice_id, true, Some(reason));
        // Emit InvoiceFrozen with freeze_appeal_channel so off-chain consumers
        // (dashboards, notification pipelines, indexers) can immediately surface
        // the appeals path to the affected business.  Issue #1959.
        crate::events::emit_invoice_frozen(&env, &invoice_id, &admin, reason.label());
        Ok(())
    }

    pub fn set_invoice_lock(
        env: Env,
        admin: Address,
        invoice_id: BytesN<32>,
        lock: InvoiceLock,
    ) -> Result<(), QuickLendXError> {
        crate::admin::AdminStorage::require_admin(&env, &admin)?;
        InvoiceStorage::set_invoice_lock(&env, &invoice_id, lock);
        Ok(())
    }

    pub fn get_invoice_lock(env: Env, invoice_id: BytesN<32>) -> InvoiceLock {
        InvoiceStorage::get_invoice_lock(&env, &invoice_id)
    }

    pub fn verify_business(
        env: Env,
        admin: Address,
        business: Address,
    ) -> Result<(), QuickLendXError> {
        verify_business(&env, &admin, &business)
    }

    /// Delete a business, removing it from any status list and marking as deleted.
    pub fn delete_business(env: Env, business: Address) -> Result<(), QuickLendXError> {
        crate::governance::require_no_open_governance_proposal(&env)?;
        BusinessVerificationStorage::delete_business(&env, &business)
    }

    pub fn submit_investor_kyc(
        env: Env,
        investor: Address,
        kyc_data: String,
    ) -> Result<(), QuickLendXError> {
        // #2439 – input-size ceiling
        require_kyc_data_bound(&kyc_data)?;
        check_and_record_mutation(&env, &investor)?;
        InvestorVerificationStorage::submit(&env, &investor, kyc_data)
    }

    pub fn verify_investor(
        env: Env,
        investor: Address,
        limit: i128,
    ) -> Result<(), QuickLendXError> {
        let admin = AdminStorage::get_admin(&env).ok_or(QuickLendXError::StorageKeyNotFound)?;
        crate::verification::verify_investor(&env, &admin, &investor, limit).map(|_| ())
    }

    pub fn reject_business(
        env: Env,
        admin: Address,
        business: Address,
        reason: String,
    ) -> Result<(), QuickLendXError> {
        crate::verification::reject_business(&env, &admin, &business, reason)
    }

    pub fn reject_investor(
        env: Env,
        admin: Address,
        investor: Address,
        reason: String,
    ) -> Result<(), QuickLendXError> {
        crate::verification::reject_investor(&env, &admin, &investor, reason)
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

    pub fn get_available_invoices(env: Env) -> Vec<BytesN<32>> {
        InvoiceStorage::get_invoices_by_status(&env, InvoiceStatus::Verified)
    }

    pub fn get_business_invoices(env: Env, business: Address) -> Vec<BytesN<32>> {
        InvoiceStorage::get_business_invoices(&env, &business)
    }

    pub fn get_total_invoice_count(env: Env) -> u32 {
        InvoiceStorage::get_total_count(&env) as u32
    }

    /// Number of invoices currently in `status` (from the status index).
    pub fn get_invoice_count_by_status(env: Env, status: InvoiceStatus) -> u32 {
        InvoiceStorage::get_invoices_by_status(&env, status).len()
    }

    pub fn update_invoice_metadata(
        env: Env,
        caller: Address,
        invoice_id: BytesN<32>,
        metadata: InvoiceMetadata,
        nonce: BytesN<32>,
    ) -> Result<(), QuickLendXError> {
        if crate::idempotency::idempotency_exists(&env, &nonce) {
            return Ok(());
        }
        // #2439 – rate-limit on invoice owner
        let invoice =
            InvoiceStorage::get(&env, &invoice_id).ok_or(QuickLendXError::InvoiceNotFound)?;
        check_and_record_mutation(&env, &invoice.business)?;
        let mut invoice =
            InvoiceStorage::get(&env, &invoice_id).ok_or(QuickLendXError::InvoiceNotFound)?;
        invoice.update_metadata(&env, &caller, metadata)?;
        InvoiceStorage::update_invoice(&env, &invoice);
        
        crate::idempotency::store_idempotency(&env, &nonce);
        Ok(())
    }

    pub fn cancel_invoice(
        env: Env,
        caller: Address,
        invoice_id: BytesN<32>,
        nonce: BytesN<32>,
    ) -> Result<(), QuickLendXError> {
        if crate::idempotency::idempotency_exists(&env, &nonce) {
            return Ok(());
        }
        // #2439 – rate-limit on the invoice owner (read-only lookup first)
        let invoice =
            InvoiceStorage::get(&env, &invoice_id).ok_or(QuickLendXError::InvoiceNotFound)?;
        check_and_record_mutation(&env, &invoice.business)?;
        // Bidding is only permitted while the invoice is open for funding.
        // Pending (unverified) and any terminal/Funded state must reject new bids.
        let invoice = InvoiceStorage::get_invoice(&env, &invoice_id)
            .ok_or(QuickLendXError::InvoiceNotFound)?;
        if !invoice.is_available_for_funding() {
            return Err(QuickLendXError::InvoiceNotAvailableForFunding);
        }
        // Re-read after the check to ensure we operate on the latest state
        let mut invoice =
            InvoiceStorage::get(&env, &invoice_id).ok_or(QuickLendXError::InvoiceNotFound)?;
        invoice.cancel(&env, caller.clone())?;
        InvoiceStorage::update_invoice(&env, &invoice);
        crate::idempotency::store_idempotency(&env, &nonce);
        Ok(())
    }

    pub fn complete_invoice(
        env: Env,
        caller: Address,
        invoice_id: BytesN<32>,
        nonce: BytesN<32>,
    ) -> Result<(), QuickLendXError> {
        if crate::idempotency::idempotency_exists(&env, &nonce) {
            return Ok(());
        }
        // #2439 – rate-limit on the invoice owner
        let invoice =
            InvoiceStorage::get(&env, &invoice_id).ok_or(QuickLendXError::InvoiceNotFound)?;
        check_and_record_mutation(&env, &invoice.business)?;
        // Bidding is only permitted while the invoice is open for funding.
        // Pending (unverified) and any terminal/Funded state must reject new bids.
        let invoice = InvoiceStorage::get_invoice(&env, &invoice_id)
            .ok_or(QuickLendXError::InvoiceNotFound)?;
        if !invoice.is_available_for_funding() {
            return Err(QuickLendXError::InvoiceNotAvailableForFunding);
        }
        let mut invoice =
            InvoiceStorage::get(&env, &invoice_id).ok_or(QuickLendXError::InvoiceNotFound)?;
        crate::invoice::require_matching_business_invoice_ownership(&env, &caller, &invoice)?;
        caller.require_auth();
        invoice.mark_as_paid(&env, caller.clone(), env.ledger().timestamp());
        InvoiceStorage::update_invoice(&env, &invoice);
        crate::idempotency::store_idempotency(&env, &nonce);
        Ok(())
    }

    pub fn clear_invoice_metadata(
        env: Env,
        caller: Address,
        invoice_id: BytesN<32>,
    ) -> Result<(), QuickLendXError> {
        crate::governance::require_no_open_governance_proposal(&env)?;
        let mut invoice =
            InvoiceStorage::get(&env, &invoice_id).ok_or(QuickLendXError::InvoiceNotFound)?;
        invoice.clear_metadata(&env, &caller)?;
        InvoiceStorage::update_invoice(&env, &invoice);
        Ok(())
    }

    pub fn get_invoices_by_customer(
        env: Env,
        customer_name: soroban_sdk::Bytes,
    ) -> Vec<BytesN<32>> {
        InvoiceStorage::get_by_customer(&env, &customer_name.into())
    }

    pub fn get_invoices_by_tax_id(env: Env, tax_id: soroban_sdk::Bytes) -> Vec<BytesN<32>> {
        InvoiceStorage::get_by_tax_id(&env, &tax_id.into())
    }

    pub fn get_invoices_by_status_batch(
        env: Env,
        ids: Vec<BytesN<32>>,
    ) -> Result<Vec<Option<InvoiceStatus>>, QuickLendXError> {
        // #2439 – hard ceiling on the number of IDs accepted per call
        require_status_batch_bound(&ids)?;
        let mut results = Vec::new(&env);
        for id in ids.iter() {
            if results.len() >= 50 {
                break;
            }
            let status = InvoiceStorage::get(&env, &id).map(|i| i.status);
            results.push_back(status);
        }
        Ok(out)
    }

    /// Mark an invoice verified. `admin` must be the admin and must sign.
    pub fn verify_invoice(
        env: Env,
        admin: Address,
        invoice_id: BytesN<32>,
    ) -> Result<(), QuickLendXError> {
        let mut invoice =
            InvoiceStorage::get(&env, &invoice_id).ok_or(QuickLendXError::InvoiceNotFound)?;
        invoice.add_rating(rating, comment.into(), investor, env.ledger().timestamp())?;
        InvoiceStorage::update_invoice(&env, &invoice);
        Ok(())
    }

    pub fn get_escrow_details(env: Env, invoice_id: BytesN<32>) -> Result<Escrow, QuickLendXError> {
        EscrowStorage::get_escrow_by_invoice(&env, &invoice_id)
            .ok_or(QuickLendXError::StorageKeyNotFound)
    }

    pub fn get_escrow_status(
        env: Env,
        invoice_id: BytesN<32>,
    ) -> Result<EscrowStatus, QuickLendXError> {
        EscrowStorage::get_escrow_status(&env, &invoice_id)
            .ok_or(QuickLendXError::StorageKeyNotFound)
    }

    pub fn get_investment(
        env: Env,
        investment_id: BytesN<32>,
    ) -> Result<crate::types::Investment, QuickLendXError> {
        InvestmentStorage::get_investment(&env, &investment_id)
            .ok_or(QuickLendXError::StorageKeyNotFound)
    }

    pub fn get_invoice_investment(
        env: Env,
        invoice_id: BytesN<32>,
    ) -> Result<crate::types::Investment, QuickLendXError> {
        InvestmentStorage::get_investment_by_invoice(&env, &invoice_id)
            .ok_or(QuickLendXError::StorageKeyNotFound)
    }

    pub fn get_investments_by_investor(env: Env, investor: Address) -> Vec<BytesN<32>> {
        InvestmentStorage::get_investments_by_investor(&env, &investor)
    }

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

    pub fn withdraw_investment(
        env: Env,
        invoice_id: BytesN<32>,
        investor: Address,
    ) -> Result<(), QuickLendXError> {
        PauseControl::require_not_paused(&env)?;
        with_payment_guard(&env, || {
            crate::escrow::withdraw_investment(&env, &invoice_id, &investor)
        })
    }

    pub fn approve_early_escrow_release(
        _env: Env,
        _invoice_id: BytesN<32>,
        _caller: Address,
    ) -> Result<(), QuickLendXError> {
        Ok(())
    }

    pub fn release_escrow_funds(env: Env, invoice_id: BytesN<32>) -> Result<(), QuickLendXError> {
        PauseControl::require_not_paused(&env)?;
        let invoice = InvoiceStorage::get(&env, &invoice_id)
            .ok_or(QuickLendXError::InvoiceNotFound)?;
        // Only a Funded invoice may have its escrow released. This also covers
        // the "no escrow exists" case (Verified/Pending/Refunded invoices).
        if invoice.status != InvoiceStatus::Funded {
            return Err(QuickLendXError::InvalidStatus);
        }
        let escrow = EscrowStorage::get_escrow_by_invoice(&env, &invoice_id)
            .ok_or(QuickLendXError::InvalidStatus)?;
        with_payment_guard(&env.clone(), move || {
            crate::payments::release_escrow(&env, &invoice_id, &escrow.business)
        })
    }

    // Backup & Restore
    pub fn create_backup(env: Env, admin: Address) -> Result<BytesN<32>, QuickLendXError> {
        AdminStorage::require_admin(&env, &admin)?;

        let mut all_invoices = Vec::new(&env);
        for status in [
            InvoiceStatus::Pending,
            InvoiceStatus::Verified,
            InvoiceStatus::Funded,
            InvoiceStatus::Paid,
            InvoiceStatus::Defaulted,
        ] {
            let ids = InvoiceStorage::get_invoices_by_status(&env, status);
            for id in ids.iter() {
                if let Some(invoice) = InvoiceStorage::get(&env, &id) {
                    all_invoices.push_back(invoice);
                }
            }
        }

        let backup_id = BackupStorage::generate_backup_id(&env);
        let backup = Backup {
            backup_id: backup_id.clone(),
            timestamp: env.ledger().timestamp(),
            description: String::from_str(&env, "Automatic Backup"),
            invoice_count: all_invoices.len(),
            status: BackupStatus::Active,
            format_version: 2,
        };

        BackupStorage::store_backup(&env, &backup, Some(&all_invoices))?;
        BackupStorage::store_backup_data(&env, &backup_id, &all_invoices);
        BackupStorage::add_to_backup_list(&env, &backup_id);

        BackupStorage::cleanup_old_backups(&env)?;

        Ok(backup_id)
    }

    pub fn restore_backup(
        env: Env,
        admin: Address,
        backup_id: BytesN<32>,
    ) -> Result<(), QuickLendXError> {
        AdminStorage::require_admin(&env, &admin)?;
        BackupStorage::restore_from_backup(&env, &backup_id).map(|_| ())
    }

    pub fn get_backups(env: Env) -> Vec<BytesN<32>> {
        BackupStorage::get_all_backups(&env)
    }

    pub fn validate_backup(env: Env, backup_id: BytesN<32>) -> bool {
        BackupStorage::validate_backup(&env, &backup_id).is_ok()
    }

    pub fn get_backup_details(env: Env, backup_id: BytesN<32>) -> Option<Backup> {
        BackupStorage::get_backup(&env, &backup_id)
    }

    pub fn set_backup_retention_policy(
        env: Env,
        business: Address,
        invoice_id: BytesN<32>,
        metadata: InvoiceMetadata,
    ) -> Result<(), QuickLendXError> {
        require_writable(&env)?;
        let mut invoice = load_invoice(&env, &invoice_id)?;
        invoice.update_metadata(&env, &business, metadata)?;
        InvoiceStorage::update_invoice(&env, &invoice);
        Ok(())
    }

    pub fn archive_backup(
        env: Env,
        admin: Address,
        backup_id: BytesN<32>,
    ) -> Result<(), QuickLendXError> {
        AdminStorage::require_admin(&env, &admin)?;
        crate::governance::require_no_open_governance_proposal(&env)?;
        let mut backup = BackupStorage::get_backup(&env, &backup_id)
            .ok_or(QuickLendXError::OperationNotAllowed)?;
        backup.status = BackupStatus::Archived;
        BackupStorage::update_backup(&env, &backup)?;
        BackupStorage::remove_from_backup_list(&env, &backup_id);
        Ok(())
    }

    pub fn add_invoice_tag(
        env: Env,
        business: Address,
        invoice_id: BytesN<32>,
        tag: String,
    ) -> Result<(), QuickLendXError> {
        require_writable(&env)?;
        let mut invoice = load_owned_invoice(&env, &business, &invoice_id)?;
        invoice.add_tag(&env, tag)?;
        InvoiceStorage::update_invoice(&env, &invoice);
        Ok(())
    }

    pub fn remove_invoice_tag(
        env: Env,
        business: Address,
        invoice_id: BytesN<32>,
        tag: String,
    ) -> Result<(), QuickLendXError> {
        require_writable(&env)?;
        let mut invoice = load_owned_invoice(&env, &business, &invoice_id)?;
        invoice.remove_tag(tag)?;
        InvoiceStorage::update_invoice(&env, &invoice);
        Ok(())
    }

    pub fn update_invoice_category(
        env: Env,
        business: Address,
        invoice_id: BytesN<32>,
        category: InvoiceCategory,
    ) -> Result<(), QuickLendXError> {
        require_writable(&env)?;
        let mut invoice = load_owned_invoice(&env, &business, &invoice_id)?;
        invoice.update_category(category);
        InvoiceStorage::update_invoice(&env, &invoice);
        Ok(())
    }
}
