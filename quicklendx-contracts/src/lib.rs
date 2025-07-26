#![no_std]
use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, vec, Address, BytesN, Env, String, Vec,
};

mod backup;
mod bid;
mod defaults;
mod errors;
mod events;
mod investment;
mod invoice;
mod payments;
mod profits;
mod settlement;
mod verification;

use bid::{Bid, BidStatus, BidStorage};
use defaults::handle_default as do_handle_default;
use errors::QuickLendXError;
use events::{
    emit_escrow_created, emit_escrow_refunded, emit_escrow_released, emit_invoice_uploaded,
    emit_invoice_verified,
};
use investment::{Investment, InvestmentStatus, InvestmentStorage};
use invoice::{Invoice, InvoiceStatus, InvoiceStorage};
use payments::{create_escrow, refund_escrow, release_escrow, EscrowStorage};
use profits::calculate_profit as do_calculate_profit;
use settlement::settle_invoice as do_settle_invoice;
use verification::{
    get_business_verification_status, reject_business, submit_kyc_application, verify_business,
    verify_invoice_data, BusinessVerificationStorage,
};

use crate::backup::{Backup, BackupStatus, BackupStorage};

#[contract]
pub struct QuickLendXContract;

#[contractimpl]
impl QuickLendXContract {
    /// Store an invoice in the contract
    pub fn store_invoice(
        env: Env,
        business: Address,
        amount: i128,
        currency: Address,
        due_date: u64,
        description: String,
    ) -> Result<BytesN<32>, QuickLendXError> {
        // Validate input parameters
        if amount <= 0 {
            return Err(QuickLendXError::InvalidAmount);
        }

        let current_timestamp = env.ledger().timestamp();
        if due_date <= current_timestamp {
            return Err(QuickLendXError::InvoiceDueDateInvalid);
        }

        if description.len() == 0 {
            return Err(QuickLendXError::InvalidDescription);
        }

        // Create new invoice
        let invoice = Invoice::new(
            &env,
            business.clone(),
            amount,
            currency.clone(),
            due_date,
            description,
        );

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

        // Create and store invoice
        let invoice = Invoice::new(
            &env,
            business.clone(),
            amount,
            currency.clone(),
            due_date,
            description.clone(),
        );
        InvoiceStorage::store_invoice(&env, &invoice);
        emit_invoice_uploaded(&env, &invoice);
        Ok(invoice.id)
    }

    /// Verify an invoice (admin or automated process)
    pub fn verify_invoice(env: Env, invoice_id: BytesN<32>) -> Result<(), QuickLendXError> {
        let mut invoice = InvoiceStorage::get_invoice(&env, &invoice_id)
            .ok_or(QuickLendXError::InvoiceNotFound)?;
        // Only allow verification if pending
        if invoice.status != InvoiceStatus::Pending {
            return Err(QuickLendXError::InvalidStatus);
        }
        // (Optional: Only admin can verify, add check here if needed)
        invoice.verify();
        InvoiceStorage::update_invoice(&env, &invoice);
        emit_invoice_verified(&env, &invoice);

        // If invoice is funded (has escrow), release escrow funds to business
        if invoice.status == InvoiceStatus::Funded {
            Self::release_escrow_funds(env.clone(), invoice_id)?;
        }

        Ok(())
    }

    /// Get an invoice by ID
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
            InvoiceStatus::Verified => invoice.verify(),
            InvoiceStatus::Paid => invoice.mark_as_paid(env.ledger().timestamp()),
            InvoiceStatus::Defaulted => invoice.mark_as_defaulted(),
            _ => return Err(QuickLendXError::InvalidStatus),
        }

        // Store updated invoice
        InvoiceStorage::update_invoice(&env, &invoice);

        // Add to new status list
        InvoiceStorage::add_to_status_invoices(&env, &invoice.status, &invoice_id);

        // Emit event
        env.events()
            .publish((symbol_short!("updated"),), (invoice_id, new_status));

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

        pending + verified + funded + paid + defaulted
    }

    /// Get a bid by ID
    pub fn get_bid(env: Env, bid_id: BytesN<32>) -> Option<Bid> {
        BidStorage::get_bid(&env, &bid_id)
    }

    /// Place a bid on an invoice
    pub fn place_bid(
        env: Env,
        investor: Address,
        invoice_id: BytesN<32>,
        bid_amount: i128,
        expected_return: i128,
    ) -> Result<BytesN<32>, QuickLendXError> {
        // Only allow bids on verified invoices
        let invoice = InvoiceStorage::get_invoice(&env, &invoice_id)
            .ok_or(QuickLendXError::InvoiceNotFound)?;
        if invoice.status != InvoiceStatus::Verified {
            return Err(QuickLendXError::InvalidStatus);
        }
        if bid_amount <= 0 {
            return Err(QuickLendXError::InvalidAmount);
        }
        // Only the investor can place their own bid
        investor.require_auth();
        // Create bid
        let bid_id = BidStorage::generate_unique_bid_id(&env);
        let bid = Bid {
            bid_id: bid_id.clone(),
            invoice_id: invoice_id.clone(),
            investor: investor.clone(),
            bid_amount,
            expected_return,
            timestamp: env.ledger().timestamp(),
            status: BidStatus::Placed,
        };
        BidStorage::store_bid(&env, &bid);
        // Track bid for this invoice
        BidStorage::add_bid_to_invoice(&env, &invoice_id, &bid_id);
        Ok(bid_id)
    }

    /// Accept a bid (business only)
    pub fn accept_bid(
        env: Env,
        invoice_id: BytesN<32>,
        bid_id: BytesN<32>,
    ) -> Result<(), QuickLendXError> {
        let mut invoice = InvoiceStorage::get_invoice(&env, &invoice_id)
            .ok_or(QuickLendXError::InvoiceNotFound)?;
        let mut bid =
            BidStorage::get_bid(&env, &bid_id).ok_or(QuickLendXError::StorageKeyNotFound)?;
        // Only the business owner can accept a bid
        invoice.business.require_auth();
        // Only allow accepting if invoice is verified and bid is placed
        if invoice.status != InvoiceStatus::Verified || bid.status != BidStatus::Placed {
            return Err(QuickLendXError::InvalidStatus);
        }

        // Create escrow
        let escrow_id = create_escrow(
            &env,
            &invoice_id,
            &bid.investor,
            &invoice.business,
            bid.bid_amount,
            &invoice.currency,
        )?;
        // Mark bid as accepted
        bid.status = BidStatus::Accepted;
        BidStorage::update_bid(&env, &bid);
        // Mark invoice as funded
        invoice.mark_as_funded(
            bid.investor.clone(),
            bid.bid_amount,
            env.ledger().timestamp(),
        );
        InvoiceStorage::update_invoice(&env, &invoice);
        // Track investment
        let investment_id = InvestmentStorage::generate_unique_investment_id(&env);
        let investment = Investment {
            investment_id: investment_id.clone(),
            invoice_id: invoice_id.clone(),
            investor: bid.investor.clone(),
            amount: bid.bid_amount,
            funded_at: env.ledger().timestamp(),
            status: InvestmentStatus::Active,
        };
        InvestmentStorage::store_investment(&env, &investment);

        let escrow = EscrowStorage::get_escrow(&env, &escrow_id)
            .expect("Escrow should exist after creation");
        emit_escrow_created(&env, &escrow);

        Ok(())
    }

    /// Withdraw a bid (investor only, before acceptance)
    pub fn withdraw_bid(env: Env, bid_id: BytesN<32>) -> Result<(), QuickLendXError> {
        let mut bid =
            BidStorage::get_bid(&env, &bid_id).ok_or(QuickLendXError::StorageKeyNotFound)?;
        // Only the investor can withdraw their own bid
        bid.investor.require_auth();
        // Only allow withdrawal if bid is placed (not accepted/withdrawn)
        if bid.status != BidStatus::Placed {
            return Err(QuickLendXError::OperationNotAllowed);
        }
        bid.status = BidStatus::Withdrawn;
        BidStorage::update_bid(&env, &bid);
        Ok(())
    }

    /// Settle an invoice (business or automated process)
    pub fn settle_invoice(
        env: Env,
        invoice_id: BytesN<32>,
        payment_amount: i128,
        platform: Address,
        platform_fee_bps: i128,
    ) -> Result<(), QuickLendXError> {
        do_settle_invoice(
            &env,
            &invoice_id,
            payment_amount,
            &platform,
            platform_fee_bps,
        )
    }

    /// Handle invoice default (admin or automated process)
    pub fn handle_default(env: Env, invoice_id: BytesN<32>) -> Result<(), QuickLendXError> {
        do_handle_default(&env, &invoice_id)
    }

    /// Calculate profit and platform fee
    pub fn calculate_profit(
        _env: Env,
        investment_amount: i128,
        payment_amount: i128,
        platform_fee_bps: i128,
    ) -> (i128, i128) {
        do_calculate_profit(investment_amount, payment_amount, platform_fee_bps)
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
    pub fn set_admin(env: Env, admin: Address) {
        BusinessVerificationStorage::set_admin(&env, &admin);
    }

    /// Get admin address
    pub fn get_admin(env: Env) -> Option<Address> {
        BusinessVerificationStorage::get_admin(&env)
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

    /// Release escrow funds to business upon invoice verification
    pub fn release_escrow_funds(env: Env, invoice_id: BytesN<32>) -> Result<(), QuickLendXError> {
        let escrow = EscrowStorage::get_escrow_by_invoice(&env, &invoice_id)
            .ok_or(QuickLendXError::StorageKeyNotFound)?;

        // Release escrow funds
        release_escrow(&env, &invoice_id)?;

        // Emit event
        emit_escrow_released(
            &env,
            &escrow.escrow_id,
            &invoice_id,
            &escrow.business,
            escrow.amount,
        );

        Ok(())
    }

    /// Refund escrow funds to investor if verification fails
    pub fn refund_escrow_funds(env: Env, invoice_id: BytesN<32>) -> Result<(), QuickLendXError> {
        let escrow = EscrowStorage::get_escrow_by_invoice(&env, &invoice_id)
            .ok_or(QuickLendXError::StorageKeyNotFound)?;

        // Refund escrow funds
        refund_escrow(&env, &invoice_id)?;

        // Emit event
        emit_escrow_refunded(
            &env,
            &escrow.escrow_id,
            &invoice_id,
            &escrow.investor,
            escrow.amount,
        );

        Ok(())
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

    /// Get escrow details for an invoice
    pub fn get_escrow_details(
        env: Env,
        invoice_id: BytesN<32>,
    ) -> Result<payments::Escrow, QuickLendXError> {
        EscrowStorage::get_escrow_by_invoice(&env, &invoice_id)
            .ok_or(QuickLendXError::StorageKeyNotFound)
    }

    /// Create a backup of all invoice data
    pub fn create_backup(env: Env, description: String) -> Result<BytesN<32>, QuickLendXError> {
        // Only admin can create backups
        let admin =
            BusinessVerificationStorage::get_admin(&env).ok_or(QuickLendXError::NotAdmin)?;
        admin.require_auth();

        // Get all invoices
        let pending = InvoiceStorage::get_invoices_by_status(&env, &InvoiceStatus::Pending);
        let verified = InvoiceStorage::get_invoices_by_status(&env, &InvoiceStatus::Verified);
        let funded = InvoiceStorage::get_invoices_by_status(&env, &InvoiceStatus::Funded);
        let paid = InvoiceStorage::get_invoices_by_status(&env, &InvoiceStatus::Paid);
        let defaulted = InvoiceStorage::get_invoices_by_status(&env, &InvoiceStatus::Defaulted);

        // Combine all invoices
        let mut all_invoices = Vec::new(&env);
        for status_vec in [pending, verified, funded, paid, defaulted].iter() {
            for invoice_id in status_vec.iter() {
                if let Some(invoice) = InvoiceStorage::get_invoice(&env, &invoice_id) {
                    all_invoices.push_back(invoice);
                }
            }
        }

        // Create backup
        let backup_id = BackupStorage::generate_backup_id(&env);
        let backup = Backup {
            backup_id: backup_id.clone(),
            timestamp: env.ledger().timestamp(),
            description,
            invoice_count: all_invoices.len() as u32,
            status: BackupStatus::Active,
        };

        // Store backup and data
        BackupStorage::store_backup(&env, &backup);
        BackupStorage::store_backup_data(&env, &backup_id, &all_invoices);
        BackupStorage::add_to_backup_list(&env, &backup_id);

        // Clean up old backups (keep last 5)
        BackupStorage::cleanup_old_backups(&env, 5)?;

        // Emit event
        events::emit_backup_created(&env, &backup_id, backup.invoice_count);

        Ok(backup_id)
    }

    /// Restore invoice data from a backup
    pub fn restore_backup(env: Env, backup_id: BytesN<32>) -> Result<(), QuickLendXError> {
        // Only admin can restore backups
        let admin =
            BusinessVerificationStorage::get_admin(&env).ok_or(QuickLendXError::NotAdmin)?;
        admin.require_auth();

        // Validate backup first
        BackupStorage::validate_backup(&env, &backup_id)?;

        // Get backup data
        let invoices = BackupStorage::get_backup_data(&env, &backup_id)
            .ok_or(QuickLendXError::StorageKeyNotFound)?;

        // Clear current invoice data
        Self::clear_all_invoices(&env)?;

        // Restore invoices
        for invoice in invoices.iter() {
            InvoiceStorage::store_invoice(&env, &invoice);
        }

        // Emit event
        events::emit_backup_restored(&env, &backup_id, invoices.len() as u32);

        Ok(())
    }

    /// Validate a backup's integrity
    pub fn validate_backup(env: Env, backup_id: BytesN<32>) -> Result<bool, QuickLendXError> {
        let result = BackupStorage::validate_backup(&env, &backup_id).is_ok();
        events::emit_backup_validated(&env, &backup_id, result);
        Ok(result)
    }

    /// Archive a backup (mark as no longer active)
    pub fn archive_backup(env: Env, backup_id: BytesN<32>) -> Result<(), QuickLendXError> {
        // Only admin can archive backups
        let admin =
            BusinessVerificationStorage::get_admin(&env).ok_or(QuickLendXError::NotAdmin)?;
        admin.require_auth();

        let mut backup = BackupStorage::get_backup(&env, &backup_id)
            .ok_or(QuickLendXError::StorageKeyNotFound)?;

        backup.status = BackupStatus::Archived;
        BackupStorage::update_backup(&env, &backup);
        BackupStorage::remove_from_backup_list(&env, &backup_id);

        events::emit_backup_archived(&env, &backup_id);

        Ok(())
    }

    /// Get all available backups
    pub fn get_backups(env: Env) -> Vec<BytesN<32>> {
        BackupStorage::get_all_backups(&env)
    }

    /// Get backup details
    pub fn get_backup_details(env: Env, backup_id: BytesN<32>) -> Option<Backup> {
        BackupStorage::get_backup(&env, &backup_id)
    }

    /// Internal function to clear all invoice data
    fn clear_all_invoices(env: &Env) -> Result<(), QuickLendXError> {
        // Clear all status lists
        for status in [
            InvoiceStatus::Pending,
            InvoiceStatus::Verified,
            InvoiceStatus::Funded,
            InvoiceStatus::Paid,
            InvoiceStatus::Defaulted,
        ]
        .iter()
        {
            let invoices = InvoiceStorage::get_invoices_by_status(env, status);
            for invoice_id in invoices.iter() {
                // Remove from status list
                InvoiceStorage::remove_from_status_invoices(env, status, &invoice_id);
                // Remove the invoice itself
                env.storage().instance().remove(&invoice_id);
            }
        }

        // Clear all business invoices
        let verified_businesses = BusinessVerificationStorage::get_verified_businesses(env);
        for business in verified_businesses.iter() {
            let invoices = InvoiceStorage::get_business_invoices(env, &business);
            let key = (symbol_short!("business"), business.clone());
            env.storage().instance().remove(&key);
        }

        Ok(())
    }
}

#[cfg(test)]
mod test;
