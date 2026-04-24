use soroban_sdk::{contract, contractimpl, Address, BytesN, Env, Vec, Bytes, xdr::ToXdr};
use crate::admin::AdminStorage;
use crate::errors::QuickLendXError;
use crate::types::{
    Invoice, InvoiceStatus, InvoiceCategory, InvoiceMetadata, Bid, BidStatus, 
    DisputeStatus, PaymentRecord, InvoiceRating, Escrow, EscrowStatus
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
    ) -> Result<(), QuickLendXError> {
        let params = InitializationParams {
            admin,
            treasury,
            fee_bps,
            min_invoice_amount,
            max_due_date_days,
            grace_period_seconds,
            initial_currencies,
        };
        ProtocolInitializer::initialize(&env, &params)
    }

    /// Set the protocol admin (transfer).
    pub fn set_admin(env: Env, admin: Address, new_admin: Address) {
        AdminStorage::set_admin(&env, &admin, &new_admin).expect("Admin transfer failed");
    }

    /// Initialize the protocol admin only.
    pub fn initialize_admin(env: Env, admin: Address) -> Result<(), QuickLendXError> {
        AdminStorage::initialize(&env, &admin)
    }

    /// Get the current protocol admin.
    pub fn get_admin(env: Env) -> Address {
        AdminStorage::get_admin(&env).expect("Admin not initialized")
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
    /// 1. **Business signature** — `business.require_auth()` is called first.
    ///    Only the business address itself may submit an invoice; no third party
    ///    (including the admin) can create invoices on behalf of a business.
    ///
    /// 2. **Verified KYC** — the business must have a `Verified` KYC record.
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
    /// * `tags`        - Optional searchable tags (max 10, each 1–50 bytes).
    ///
    /// # Errors
    /// * `BusinessNotVerified` (1600) — business has no KYC record or is rejected.
    /// * `KYCAlreadyPending`   (1601) — business KYC is pending admin review.
    pub fn store_invoice(
        env: Env,
        business: Address,
        amount: i128,
        currency: Address,
        due_date: u64,
        description: soroban_sdk::Bytes,
        category: InvoiceCategory,
        tags: Vec<soroban_sdk::Bytes>,
    ) -> Result<BytesN<32>, QuickLendXError> {
        // POLICY LAYER 1: Require explicit authorization from the business address.
        // This ensures only the business itself can create invoices — not the admin,
        // not a third party. Prevents impersonation and unauthorized storage writes.
        business.require_auth();

        // POLICY LAYER 2: Enforce KYC gating.
        // Pending businesses are explicitly rejected with KYCAlreadyPending so
        // callers can distinguish "not yet approved" from "rejected/unknown".
        // This is the primary anti-spam control: only vetted businesses may write
        // invoice data to on-chain storage.
        crate::verification::require_business_not_pending(&env, &business)?;

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
        Ok(invoice_id)
    }

    pub fn get_invoice(env: Env, invoice_id: BytesN<32>) -> Invoice {
        InvoiceStorage::get(&env, &invoice_id).expect("Invoice not found")
    }

    pub fn update_invoice_status(env: Env, invoice_id: BytesN<32>, status: InvoiceStatus) {
        let mut invoice = InvoiceStorage::get(&env, &invoice_id).expect("Invoice not found");
        invoice.status = status;
        InvoiceStorage::update_invoice(&env, &invoice);
    }

    pub fn verify_invoice(env: Env, invoice_id: BytesN<32>) {
        let mut invoice = InvoiceStorage::get(&env, &invoice_id).expect("Invoice not found");
        invoice.status = InvoiceStatus::Verified;
        InvoiceStorage::update_invoice(&env, &invoice);
    }

    pub fn place_bid(
        env: Env,
        investor: Address,
        invoice_id: BytesN<32>,
        bid_amount: i128,
        expected_return: i128,
    ) -> BytesN<32> {
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
        bid_id
    }

    pub fn accept_bid(env: Env, invoice_id: BytesN<32>, bid_id: BytesN<32>) {
        let mut invoice = InvoiceStorage::get(&env, &invoice_id).expect("Invoice not found");
        let bid = BidStorage::get_bid(&env, &bid_id).expect("Bid not found");
        
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

    pub fn withdraw_bid(env: Env, bid_id: BytesN<32>) {
        let mut bid = BidStorage::get_bid(&env, &bid_id).expect("Bid not found");
        bid.status = BidStatus::Withdrawn;
        BidStorage::store_bid(&env, &bid);
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

    pub fn submit_kyc_application(env: Env, business: Address, kyc_data: soroban_sdk::Bytes) -> Result<(), QuickLendXError> {
        submit_kyc_application(&env, &business, kyc_data)
    }

    pub fn verify_business(env: Env, admin: Address, business: Address) -> Result<(), QuickLendXError> {
        verify_business(&env, &admin, &business)
    }

    pub fn submit_investor_kyc(env: Env, investor: Address, kyc_data: soroban_sdk::Bytes) -> Result<(), QuickLendXError> {
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

    pub fn update_invoice_metadata(env: Env, invoice_id: BytesN<32>, metadata: InvoiceMetadata) {
        let mut invoice = InvoiceStorage::get(&env, &invoice_id).expect("Invoice not found");
        invoice.update_metadata(metadata);
        InvoiceStorage::update_invoice(&env, &invoice);
    }

    pub fn clear_invoice_metadata(env: Env, invoice_id: BytesN<32>) {
        let mut invoice = InvoiceStorage::get(&env, &invoice_id).expect("Invoice not found");
        invoice.clear_metadata();
        InvoiceStorage::update_invoice(&env, &invoice);
    }

    pub fn get_invoices_by_customer(env: Env, customer_name: soroban_sdk::Bytes) -> Vec<BytesN<32>> {
        InvoiceStorage::get_by_customer(&env, &customer_name)
    }

    pub fn get_invoices_by_tax_id(env: Env, tax_id: soroban_sdk::Bytes) -> Vec<BytesN<32>> {
        InvoiceStorage::get_by_tax_id(&env, &tax_id)
    }

    pub fn get_invoices_by_status_batch(env: Env, ids: Vec<BytesN<32>>) -> Vec<Option<InvoiceStatus>> {
        let mut results = Vec::new(&env);
        for id in ids.iter() {
            if results.len() >= 50 { break; }
            let status = InvoiceStorage::get(&env, &id).map(|i| i.status);
            results.push_back(status);
        }
        results
    }

    pub fn add_invoice_rating(
        env: Env,
        invoice_id: BytesN<32>,
        rating: u32,
        comment: soroban_sdk::Bytes,
        investor: Address,
    ) -> Result<(), QuickLendXError> {
        let mut invoice = InvoiceStorage::get(&env, &invoice_id).expect("Invoice not found");
        invoice.add_rating(rating, comment, investor, env.ledger().timestamp())?;
        InvoiceStorage::update_invoice(&env, &invoice);
        Ok(())
    }

    pub fn get_escrow_details(env: Env, invoice_id: BytesN<32>) -> Escrow {
        EscrowStorage::get_escrow_by_invoice(&env, &invoice_id).expect("Escrow not found")
    }

    pub fn get_escrow_status(env: Env, invoice_id: BytesN<32>) -> EscrowStatus {
        EscrowStorage::get_escrow_status(&env, &invoice_id).expect("Escrow not found")
    }

    pub fn release_escrow_funds(env: Env, invoice_id: BytesN<32>) {
        let mut escrow = EscrowStorage::get_escrow_by_invoice(&env, &invoice_id).expect("Escrow not found");
        escrow.status = EscrowStatus::Released;
        escrow.released_at = Some(env.ledger().timestamp());
        EscrowStorage::update_escrow(&env, &escrow);
    }

    pub fn refund_escrow_funds(env: Env, invoice_id: BytesN<32>, admin: Address) {
        AdminStorage::require_admin(&env, &admin).expect("Admin only");
        let mut escrow = EscrowStorage::get_escrow_by_invoice(&env, &invoice_id).expect("Escrow not found");
        escrow.status = EscrowStatus::Refunded;
        escrow.refunded_at = Some(env.ledger().timestamp());
        EscrowStorage::update_escrow(&env, &escrow);
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
    ) {
        AdminStorage::require_admin(&env, &admin).expect("Admin only");
        let policy = BackupRetentionPolicy {
            max_backups,
            max_age_seconds,
            auto_cleanup_enabled: enabled,
        };
        BackupStorage::set_retention_policy(&env, &policy);
    }

    pub fn archive_backup(env: Env, admin: Address, backup_id: BytesN<32>) -> Result<(), QuickLendXError> {
        AdminStorage::require_admin(&env, &admin)?;
        let mut backup = BackupStorage::get_backup(&env, &backup_id).ok_or(QuickLendXError::OperationNotAllowed)?;
        backup.status = BackupStatus::Archived;
        BackupStorage::update_backup(&env, &backup)
    }

    /// Manually trigger retention-policy cleanup (admin only).
    ///
    /// Applies the current [`BackupRetentionPolicy`] immediately and returns
    /// the number of active backups that were purged.  Archived backups are
    /// never removed by this operation.
    pub fn cleanup_backups(env: Env, admin: Address) -> Result<u32, QuickLendXError> {
        AdminStorage::require_admin(&env, &admin)?;
        BackupStorage::cleanup_old_backups(&env)
    }

    /// Return all invoice IDs belonging to a specific business address.
    pub fn get_invoices_by_business(env: Env, business: Address) -> Vec<BytesN<32>> {
        InvoiceStorage::get_business_invoices(&env, &business)
    }

    /// Return all invoice IDs that currently have the given status.
    pub fn get_invoices_by_status(env: Env, status: InvoiceStatus) -> Vec<BytesN<32>> {
        InvoiceStorage::get_invoices_by_status(&env, status)
    }
}
