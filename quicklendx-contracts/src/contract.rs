use soroban_sdk::{contract, contractimpl, Address, BytesN, Env, Vec, Bytes, xdr::ToXdr};
use crate::admin::AdminStorage;
use crate::errors::QuickLendXError;
use crate::protocol_limits::MAX_INVOICE_AMOUNT;
use crate::protocol_limits::{
    check_and_record_mutation, require_batch_size_bound, require_description_bound,
    require_kyc_data_bound, require_status_batch_bound, require_tags_bound,
};
use crate::types::{
    Invoice, InvoiceStatus, InvoiceCategory, InvoiceMetadata, Bid, BidStatus,
    DisputeStatus, PaymentRecord, InvoiceRating, Escrow, EscrowStatus,
    BusinessFreezeReason,
};
use crate::storage::InvoiceStorage;
use crate::init::{ProtocolInitializer, InitializationParams};
use crate::protocol_limits::ProtocolLimitsContract;
use crate::verification::{BusinessVerificationStorage, InvestorVerificationStorage, submit_kyc_application, verify_business};
use crate::bid::BidStorage;
use crate::payments::EscrowStorage;
use crate::backup::{Backup, BackupStorage, BackupStatus, BackupRetentionPolicy};

#[contract]
pub struct QuickLendXContract;

#[contractimpl]
impl QuickLendXContract {
    /// Initialize the protocol with comprehensive parameters.
    pub fn initialize(
        env: Env,
        admin: Address,
        treasury: Address,
        fee_bps: u32,
        min_invoice_amount: i128,
        max_due_date_days: u64,
        grace_period_seconds: u64,
        initial_currencies: Vec<Address>,
        corridors: Vec<Address>,
    ) -> Result<(), QuickLendXError> {
        let params = InitializationParams {
            admin,
            treasury,
            fee_bps,
            min_invoice_amount,
            max_due_date_days,
            grace_period_seconds,
            initial_currencies,
            corridors,
            backfill_max_batch_size: 100,
        };
        ProtocolInitializer::initialize(&env, &params)
    }

    pub fn set_admin(env: Env, admin: Address, new_admin: Address) -> Result<(), QuickLendXError> {
        AdminStorage::set_admin(&env, &admin, &new_admin)?;
        Ok(())
    }

    /// Initialize the protocol admin only.
    pub fn initialize_admin(env: Env, admin: Address) -> Result<(), QuickLendXError> {
        AdminStorage::initialize(&env, &admin)
    }

    pub fn get_admin(env: Env) -> Result<Address, QuickLendXError> {
        AdminStorage::get_admin(&env).ok_or(QuickLendXError::StorageKeyNotFound)
    }

    /// Admin-gated, read-only protocol invariant self-check ("heartbeat").
    ///
    /// Aggregates the cross-module integrity checks (orphan investments, audit
    /// chain integrity, solvency, and storage-index coherence) into a single
    /// [`InvariantReport`] of `(check_name, passed, evidence)` rows for incident
    /// response. Authenticates `admin` before running; the checks never mutate
    /// state, so an unauthorized or failing call leaves the ledger unchanged.
    pub fn invariant_self_check(
        env: Env,
        admin: Address,
    ) -> Result<crate::invariants::InvariantReport, QuickLendXError> {
        crate::invariants::invariant_self_check(&env, &admin)
    }

    /// Initialize protocol limits.
    pub fn initialize_protocol_limits(
        env: Env,
        admin: Address,
    ) -> Result<(), QuickLendXError> {
        ProtocolLimitsContract::initialize(env, admin)
    }

    /// Store a new invoice on behalf of a KYC-verified business.
    ///
    /// # Authentication & KYC Policy (Issue #790)
    ///
    /// This function enforces a **two-layer authentication policy** to prevent
    /// unauthorized invoice creation and storage-based denial-of-service attacks:
    ///
    /// 1. **Business signature** - `business.require_auth()` is called first.
    ///    Only the business address itself may submit an invoice; no third party
    ///    (including the admin) can create invoices on behalf of a business.
    ///
    /// 2. **Verified KYC** - the business must have a `Verified` KYC record.
    ///    - `BusinessNotVerified` (1600) is returned if the business has no KYC
    ///      record or was rejected.
    ///    - `KYCAlreadyPending` (1601) is returned if the KYC application is
    ///      still awaiting admin review, preventing spam from unvetted entities.
    ///
    /// # Security Invariants
    /// - An unverified or pending business **cannot** create invoices.
    /// - Admin cannot bypass the business signature requirement.
    /// - Prevents storage DoS: only KYC-gated addresses can write invoice data.
    ///
    /// # Arguments
    /// * `env`         - The contract environment.
    /// * `business`    - The address of the invoice-issuing business (must sign).
    /// * `amount`      - Invoice face value in the smallest currency unit.
    /// * `currency`    - Token contract address for the invoice currency.
    /// * `due_date`    - Unix timestamp by which the invoice must be settled.
    /// * `description` - Human-readable invoice description.
    /// * `category`    - Invoice category (Services, Products, etc.).
    /// * `tags`        - Optional searchable tags (max 10, each 1-50 bytes).
    ///
    /// # Errors
    /// * `BusinessNotVerified` (1600) - business has no KYC record or is rejected.
    /// * `KYCAlreadyPending`   (1601) - business KYC is pending admin review.
    pub fn store_invoice(
        env: Env,
        business: Address,
        amount: i128,
        currency: Address,
        due_date: u64,
        description: soroban_sdk::Bytes,
        category: InvoiceCategory,
        tags: Vec<soroban_sdk::Bytes>,
        nonce: BytesN<32>,
    ) -> Result<BytesN<32>, QuickLendXError> {
        if let Some(existing_id) = crate::idempotency::get_idempotency_result::<BytesN<32>>(&env, &nonce) {
            return Ok(existing_id);
        }

        business.require_auth();

        // #2439 – per-address rate limit (cheap check before any heavy work)
        check_and_record_mutation(&env, &business)?;

        // #2439 – hard input-size ceilings (reject oversized payloads early)
        require_description_bound(&description)?;
        require_tags_bound(&tags)?;

        crate::verification::require_business_not_pending(&env, &business)?;
        crate::regulatory::require_regulatory_ok(&env, &business)?;

        if amount <= 0 || amount > MAX_INVOICE_AMOUNT {
            return Err(QuickLendXError::InvalidAmount);
        }

        ProtocolLimitsContract::check_invoice_limit(&env, &business)?;

        let invoice_id: BytesN<32> = env
            .crypto()
            .sha256(&env.ledger().timestamp().to_xdr(&env))
            .into();

        let invoice = Invoice {
            invoice_id: invoice_id.clone(),
            business,
            amount,
            currency,
            due_date,
            description,
            category,
            tags,
            status: InvoiceStatus::Pending,
            metadata: None,
            metadata_customer_name: None,
            metadata_tax_id: None,
            total_paid: 0,
            funded_amount: 0,
            funded_at: None,
            average_rating: None,
            total_ratings: 0,
            investor: None,
            dispute_status: DisputeStatus::None,
            dispute: None,
            payment_history: Vec::new(&env),
            ratings: Vec::new(&env),
            created_at: env.ledger().timestamp(),
            updated_at: env.ledger().timestamp(),
            settled_at: None,
        };

        InvoiceStorage::store_invoice(&env, &invoice);
        crate::idempotency::store_idempotency_result(&env, &nonce, &invoice_id);
        Ok(invoice_id)
    }

    pub fn get_invoice(env: Env, invoice_id: BytesN<32>) -> Result<Invoice, QuickLendXError> {
        InvoiceStorage::get(&env, &invoice_id).ok_or(QuickLendXError::InvoiceNotFound)
    }

    pub fn update_invoice_status(env: Env, admin: Address, invoice_id: BytesN<32>, status: InvoiceStatus) -> Result<(), QuickLendXError> {
        AdminStorage::require_admin(&env, &admin)?;
        let mut invoice = InvoiceStorage::get(&env, &invoice_id).ok_or(QuickLendXError::InvoiceNotFound)?;
        invoice.status = status;
        InvoiceStorage::update_invoice(&env, &invoice);
        Ok(())
    }

    pub fn verify_invoice(env: Env, admin: Address, invoice_id: BytesN<32>) -> Result<(), QuickLendXError> {
        AdminStorage::require_admin(&env, &admin)?;
        let mut invoice = InvoiceStorage::get(&env, &invoice_id).ok_or(QuickLendXError::InvoiceNotFound)?;
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

    pub fn accept_bid(env: Env, invoice_id: BytesN<32>, bid_id: BytesN<32>) -> Result<(), QuickLendXError> {
        if InvoiceStorage::is_frozen(&env, &invoice_id) {
            InvoiceStorage::require_lock_within_time_limit(&env, &invoice_id)?;
            return Err(QuickLendXError::InvoiceFrozen);
        }
        let mut invoice = InvoiceStorage::get(&env, &invoice_id).ok_or(QuickLendXError::InvoiceNotFound)?;
        let bid = BidStorage::get_bid(&env, &bid_id).unwrap();
        
        invoice.mark_as_funded(&env, bid.investor.clone(), bid.bid_amount, env.ledger().timestamp());
        InvoiceStorage::update_invoice(&env, &invoice);
        
        let mut bid = bid;
        bid.status = BidStatus::Accepted;
        BidStorage::store_bid(&env, &bid);
        
        let escrow_id = crate::payments::EscrowStorage::generate_unique_escrow_id(&env);
        let escrow = Escrow {
            escrow_id,
            invoice_id,
            investor: bid.investor,
            business: invoice.business,
            amount: bid.bid_amount,
            currency: invoice.currency,
            created_at: env.ledger().timestamp(),
            released_at: None,
            refunded_at: None,
            status: EscrowStatus::Held,
        };
        crate::payments::EscrowStorage::store_escrow(&env, &escrow);
        Ok(())
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

    pub fn get_best_bid(env: Env, invoice_id: BytesN<32>) -> Option<Bid> {
        BidStorage::get_best_bid(&env, &invoice_id)
    }

    pub fn get_bids_by_status(env: Env, invoice_id: BytesN<32>, status: BidStatus) -> Vec<Bid> {
        BidStorage::get_bids_by_status(&env, &invoice_id, status)
    }

    pub fn get_bids_by_investor(env: Env, invoice_id: BytesN<32>, investor: Address) -> Vec<Bid> {
        BidStorage::get_bids_by_investor(&env, &invoice_id, &investor)
    }

    pub fn get_investor_active_exposure(env: Env, investor: Address) -> i128 {
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

    pub fn submit_kyc_application(env: Env, business: Address, kyc_data: soroban_sdk::Bytes) -> Result<(), QuickLendXError> {
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
        crate::events::emit_invoice_frozen(
            &env,
            &invoice_id,
            &admin,
            reason.label(),
        );
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

    pub fn verify_business(env: Env, admin: Address, business: Address) -> Result<(), QuickLendXError> {
        verify_business(&env, &admin, &business)
    }

    /// Delete a business, removing it from any status list and marking as deleted.
    pub fn delete_business(env: Env, business: Address) -> Result<(), QuickLendXError> {
        crate::governance::require_no_open_governance_proposal(&env)?;
        BusinessVerificationStorage::delete_business(&env, &business)
    }

    pub fn submit_investor_kyc(env: Env, investor: Address, kyc_data: soroban_sdk::Bytes) -> Result<(), QuickLendXError> {
        // #2439 – input-size ceiling
        require_kyc_data_bound(&kyc_data)?;
        check_and_record_mutation(&env, &investor)?;
        InvestorVerificationStorage::submit(&env, &investor, kyc_data)
    }

    pub fn verify_investor(env: Env, investor: Address, limit: i128) {
        InvestorVerificationStorage::verify_investor(&env, &investor, limit);
    }

    pub fn get_available_invoices(env: Env) -> Vec<BytesN<32>> {
        InvoiceStorage::get_invoices_by_status(&env, InvoiceStatus::Verified)
    }

    pub fn get_business_invoices(env: Env, business: Address) -> Vec<BytesN<32>> {
        InvoiceStorage::get_business_invoices(&env, &business)
    }

    pub fn get_total_invoice_count(env: Env) -> u32 {
        InvoiceStorage::get_total_count(&env)
    }

    pub fn get_invoice_count_by_status(env: Env, status: InvoiceStatus) -> u32 {
        InvoiceStorage::get_count_by_status(&env, status)
    }

    pub fn update_invoice_metadata(env: Env, caller: Address, invoice_id: BytesN<32>, metadata: InvoiceMetadata, nonce: BytesN<32>) -> Result<(), QuickLendXError> {
        if crate::idempotency::idempotency_exists(&env, &nonce) {
            return Ok(());
        }
        // #2439 – rate-limit on invoice owner
        let invoice = InvoiceStorage::get(&env, &invoice_id).ok_or(QuickLendXError::InvoiceNotFound)?;
        check_and_record_mutation(&env, &invoice.business)?;
        let mut invoice = InvoiceStorage::get(&env, &invoice_id).ok_or(QuickLendXError::InvoiceNotFound)?;
        invoice.update_metadata(&env, &caller, metadata)?;
        InvoiceStorage::update_invoice(&env, &invoice);
        crate::idempotency::store_idempotency(&env, &nonce);
        Ok(())
    }

    pub fn cancel_invoice(env: Env, caller: Address, invoice_id: BytesN<32>, nonce: BytesN<32>) -> Result<(), QuickLendXError> {
        if crate::idempotency::idempotency_exists(&env, &nonce) {
            return Ok(());
        }
        // #2439 – rate-limit on the invoice owner (read-only lookup first)
        let invoice = InvoiceStorage::get(&env, &invoice_id).ok_or(QuickLendXError::InvoiceNotFound)?;
        check_and_record_mutation(&env, &invoice.business)?;
        // Re-read after the check to ensure we operate on the latest state
        let mut invoice = InvoiceStorage::get(&env, &invoice_id).ok_or(QuickLendXError::InvoiceNotFound)?;
        invoice.cancel(&env, caller.clone())?;
        InvoiceStorage::update_invoice(&env, &invoice);
        crate::idempotency::store_idempotency(&env, &nonce);
        Ok(())
    }

    pub fn complete_invoice(env: Env, caller: Address, invoice_id: BytesN<32>, nonce: BytesN<32>) -> Result<(), QuickLendXError> {
        if crate::idempotency::idempotency_exists(&env, &nonce) {
            return Ok(());
        }
        // #2439 – rate-limit on the invoice owner
        let invoice = InvoiceStorage::get(&env, &invoice_id).ok_or(QuickLendXError::InvoiceNotFound)?;
        check_and_record_mutation(&env, &invoice.business)?;
        let mut invoice = InvoiceStorage::get(&env, &invoice_id).ok_or(QuickLendXError::InvoiceNotFound)?;
        crate::invoice::require_matching_business_invoice_ownership(&env, &caller, &invoice)?;
        caller.require_auth();
        invoice.mark_as_paid(&env, caller.clone(), env.ledger().timestamp());
        InvoiceStorage::update_invoice(&env, &invoice);
        crate::idempotency::store_idempotency(&env, &nonce);
        Ok(())
    }

    pub fn clear_invoice_metadata(env: Env, caller: Address, invoice_id: BytesN<32>) -> Result<(), QuickLendXError> {
        crate::governance::require_no_open_governance_proposal(&env)?;
        let mut invoice = InvoiceStorage::get(&env, &invoice_id).ok_or(QuickLendXError::InvoiceNotFound)?;
        invoice.clear_metadata(&env, &caller)?;
        InvoiceStorage::update_invoice(&env, &invoice);
        Ok(())
    }

    pub fn get_invoices_by_customer(env: Env, customer_name: soroban_sdk::Bytes) -> Vec<BytesN<32>> {
        InvoiceStorage::get_by_customer(&env, &customer_name)
    }

    pub fn get_invoices_by_tax_id(env: Env, tax_id: soroban_sdk::Bytes) -> Vec<BytesN<32>> {
        InvoiceStorage::get_by_tax_id(&env, &tax_id)
    }

    pub fn get_invoices_by_status_batch(env: Env, ids: Vec<BytesN<32>>) -> Result<Vec<Option<InvoiceStatus>>, QuickLendXError> {
        // #2439 – hard ceiling on the number of IDs accepted per call
        require_status_batch_bound(&ids)?;
        let mut results = Vec::new(&env);
        for id in ids.iter() {
            if results.len() >= 50 { break; }
            let status = InvoiceStorage::get(&env, &id).map(|i| i.status);
            results.push_back(status);
        }
        Ok(results)
    }

    pub fn add_invoice_rating(
        env: Env,
        invoice_id: BytesN<32>,
        rating: u32,
        comment: soroban_sdk::Bytes,
        investor: Address,
    ) -> Result<(), QuickLendXError> {
        let mut invoice = InvoiceStorage::get(&env, &invoice_id).ok_or(QuickLendXError::InvoiceNotFound)?;
        invoice.add_rating(rating, comment, investor, env.ledger().timestamp())?;
        InvoiceStorage::update_invoice(&env, &invoice);
        Ok(())
    }

    pub fn get_escrow_details(env: Env, invoice_id: BytesN<32>) -> Result<Escrow, QuickLendXError> {
        EscrowStorage::get_escrow_by_invoice(&env, &invoice_id).ok_or(QuickLendXError::StorageKeyNotFound)
    }

    pub fn get_escrow_status(env: Env, invoice_id: BytesN<32>) -> Result<EscrowStatus, QuickLendXError> {
        EscrowStorage::get_escrow_status(&env, &invoice_id).ok_or(QuickLendXError::StorageKeyNotFound)
    }

    pub fn release_escrow_funds(env: Env, invoice_id: BytesN<32>) -> Result<(), QuickLendXError> {
        let mut escrow = EscrowStorage::get_escrow_by_invoice(&env, &invoice_id).unwrap();
        escrow.status = EscrowStatus::Released;
        escrow.released_at = Some(env.ledger().timestamp());
        EscrowStorage::update_escrow(&env, &escrow);
        Ok(())
    }

    pub fn refund_escrow_funds(env: Env, invoice_id: BytesN<32>, admin: Address) -> Result<(), QuickLendXError> {
        AdminStorage::require_admin(&env, &admin)?;
        let mut escrow = EscrowStorage::get_escrow_by_invoice(&env, &invoice_id).unwrap();
        escrow.status = EscrowStatus::Refunded;
        escrow.refunded_at = Some(env.ledger().timestamp());
        EscrowStorage::update_escrow(&env, &escrow);
        Ok(())
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
            description: soroban_sdk::Bytes::from_slice(&env, "Automatic Backup".as_bytes()),
            invoice_count: all_invoices.len(),
            status: BackupStatus::Active,
        };
        
        BackupStorage::store_backup(&env, &backup, Some(&all_invoices))?;
        BackupStorage::store_backup_data(&env, &backup_id, &all_invoices);
        BackupStorage::add_to_backup_list(&env, &backup_id);
        
        BackupStorage::cleanup_old_backups(&env)?;
        
        Ok(backup_id)
    }
    
    pub fn restore_backup(env: Env, admin: Address, backup_id: BytesN<32>) -> Result<(), QuickLendXError> {
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
        admin: Address,
        max_backups: u32,
        max_age_seconds: u64,
        enabled: bool,
    ) -> Result<(), QuickLendXError> {
        AdminStorage::require_admin(&env, &admin)?;
        let policy = BackupRetentionPolicy {
            max_backups,
            max_age_seconds,
            auto_cleanup_enabled: enabled,
        };
        BackupStorage::set_retention_policy(&env, &policy);
        Ok(())
    }

    pub fn archive_backup(env: Env, admin: Address, backup_id: BytesN<32>) -> Result<(), QuickLendXError> {
        AdminStorage::require_admin(&env, &admin)?;
        crate::governance::require_no_open_governance_proposal(&env)?;
        let mut backup = BackupStorage::get_backup(&env, &backup_id).ok_or(QuickLendXError::OperationNotAllowed)?;
        backup.status = BackupStatus::Archived;
        BackupStorage::update_backup(&env, &backup)?;
        BackupStorage::remove_from_backup_list(&env, &backup_id);
        Ok(())
    }

    /// Rebuild customer, tax_id, and tag secondary indexes from the canonical invoice list.
    /// Admin-only. Returns the number of invoices processed.
    pub fn admin_reindex_invoices(env: Env, admin: Address) -> Result<u32, QuickLendXError> {
        AdminStorage::require_admin(&env, &admin)?;
        admin.require_auth();

        let all_ids = InvoiceStorage::get_all_invoice_ids(&env);
        let mut count: u32 = 0;

        for invoice_id in all_ids.iter() {
            let invoice = match InvoiceStorage::get(&env, &invoice_id) {
                Some(inv) => inv,
                None => continue,
            };

            // Rebuild customer index
            if let Some(ref name) = invoice.metadata_customer_name {
                InvoiceStorage::add_to_customer_index(&env, name, &invoice_id);
            }

            // Rebuild tax_id index
            if let Some(ref tax_id) = invoice.metadata_tax_id {
                InvoiceStorage::add_to_tax_id_index(&env, tax_id, &invoice_id);
            }

            // Rebuild tag indexes
            for tag in invoice.tags.iter() {
                InvoiceStorage::add_tag_index(&env, &tag, &invoice_id);
            }

            count += 1;
        }

        Ok(count)
    }
}
