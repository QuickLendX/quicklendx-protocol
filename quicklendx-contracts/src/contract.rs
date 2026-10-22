//! QuickLendX contract entrypoints.
//!
//! This file is the single `#[contract]` type for the crate. It currently
//! exposes the protocol-initialization and admin-management slice (issue
//! #2925) and invoice creation and lifecycle (issue #2926); the other modules
//! (bids, escrow, settlement, disputes, KYC, fees, pause, backup) are wired in
//! by their own issues.
//!
//! Every entrypoint is a thin wrapper: authentication, validation, storage
//! and events all live in [`crate::init::ProtocolInitializer`] and
//! [`crate::admin::AdminStorage`]. The contract layer only forwards `&env`.
//!
//! There is deliberately no standalone `initialize_admin(admin)` entrypoint.
//! It would let anyone who can sign for *some* address install that address
//! as admin before the deployer calls `initialize`, which would then leave
//! the protocol without a treasury, fee or config. Admin is set only through
//! `initialize`.

use soroban_sdk::{contract, contractimpl, Address, BytesN, Env, String, Vec};

use crate::admin::AdminStorage;
use crate::currency::CurrencyWhitelist;
use crate::errors::QuickLendXError;
use crate::init::{
    InitializationParams, ProtocolConfig, ProtocolConfigDiff, ProtocolConfigParams,
    ProtocolInitializer,
};
use crate::invoice::{require_matching_business_invoice_ownership, Invoice};
use crate::maintenance::MaintenanceControl;
use crate::pause::PauseControl;
use crate::protocol_limits::{
    check_and_record_mutation, check_invoice_limit, require_status_batch_bound, require_tags_bound,
    ProtocolLimitsContract, MAX_INPUT_DESCRIPTION_BYTES,
};
use crate::storage::InvoiceStorage;
use crate::types::{InvoiceCategory, InvoiceMetadata, InvoiceStatus};

/// Guards shared by every state-changing invoice entrypoint.
fn require_writable(env: &Env) -> Result<(), QuickLendXError> {
    PauseControl::require_not_paused(env)?;
    MaintenanceControl::require_write_allowed(env)
}

fn load_invoice(env: &Env, invoice_id: &BytesN<32>) -> Result<Invoice, QuickLendXError> {
    InvoiceStorage::get(env, invoice_id).ok_or(QuickLendXError::InvoiceNotFound)
}

/// Load an invoice owned by `business` and require `business` to sign.
/// For `Invoice` methods that do not authenticate on their own.
fn load_owned_invoice(
    env: &Env,
    business: &Address,
    invoice_id: &BytesN<32>,
) -> Result<Invoice, QuickLendXError> {
    let invoice = load_invoice(env, invoice_id)?;
    require_matching_business_invoice_ownership(env, business, &invoice)?;
    business.require_auth();
    Ok(invoice)
}

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

    /// Update protocol configuration (admin only).
    pub fn set_protocol_config(
        env: Env,
        admin: Address,
        min_invoice_amount: i128,
        max_due_date_days: u64,
        grace_period_seconds: u64,
        backfill_max_batch_size: u32,
    ) -> Result<(), QuickLendXError> {
        ProtocolInitializer::set_protocol_config(
            &env,
            &admin,
            min_invoice_amount,
            max_due_date_days,
            grace_period_seconds,
            backfill_max_batch_size,
        )
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
        origination_fee_bps: Option<u32>,
        late_payment_penalty_bps: Option<u32>,
        early_payment_discount_bps: Option<u32>,
        nonce: BytesN<32>,
    ) -> Result<BytesN<32>, QuickLendXError> {
        // 1. Only the business itself may create its invoices.
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

        // 5. Build through the validating constructor (collision-checked ID).
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
        )?;

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

    pub fn get_invoice(env: Env, invoice_id: BytesN<32>) -> Result<Invoice, QuickLendXError> {
        load_invoice(&env, &invoice_id)
    }

    // ── Admin management ────────────────────────────────────────────────────

    /// Current admin, `None` before initialization.
    pub fn get_current_admin(env: Env) -> Option<Address> {
        AdminStorage::get_admin(&env)
    }

    pub fn get_invoices_by_status(env: Env, status: InvoiceStatus) -> Vec<BytesN<32>> {
        InvoiceStorage::get_invoices_by_status(&env, status)
    }

    /// Number of invoices ever created.
    pub fn get_total_invoice_count(env: Env) -> u64 {
        InvoiceStorage::get_total_count(&env)
    }

    /// Number of invoices currently in `status` (from the status index).
    pub fn get_invoice_count_by_status(env: Env, status: InvoiceStatus) -> u32 {
        InvoiceStorage::get_invoices_by_status(&env, status).len()
    }

    pub fn get_invoices_by_customer(env: Env, customer_name: String) -> Vec<BytesN<32>> {
        InvoiceStorage::get_by_customer(&env, &customer_name)
    }

    pub fn get_invoices_by_tax_id(env: Env, tax_id: String) -> Vec<BytesN<32>> {
        InvoiceStorage::get_by_tax_id(&env, &tax_id)
    }

    /// Status of each requested invoice (`None` for unknown IDs). At most
    /// `MAX_INPUT_STATUS_BATCH_SIZE` IDs per call.
    pub fn get_invoices_by_status_batch(
        env: Env,
        invoice_ids: Vec<BytesN<32>>,
    ) -> Result<Vec<Option<InvoiceStatus>>, QuickLendXError> {
        require_status_batch_bound(&invoice_ids)?;
        let mut out = Vec::new(&env);
        for id in invoice_ids.iter() {
            out.push_back(InvoiceStorage::get(&env, &id).map(|i| i.status));
        }
        Ok(out)
    }

    /// Mark an invoice verified. `admin` must be the admin and must sign.
    pub fn verify_invoice(
        env: Env,
        admin: Address,
        invoice_id: BytesN<32>,
    ) -> Result<(), QuickLendXError> {
        investor.require_auth();
        let mut invoice = InvoiceStorage::get(&env, &invoice_id).ok_or(QuickLendXError::InvoiceNotFound)?;
        if invoice.investor != Some(investor.clone()) {
            return Err(QuickLendXError::NotRater);
        }
        invoice.add_rating(rating, String::from_str(&env, "rating"), investor, env.ledger().timestamp())?;
        InvoiceStorage::update_invoice(&env, &invoice);
        Ok(())
    }

    /// Set customer metadata. `Invoice::update_metadata` checks ownership and
    /// requires the business signature.
    pub fn update_invoice_metadata(
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

    /// Remove customer metadata. `Invoice::clear_metadata` checks ownership
    /// and requires the business signature.
    pub fn clear_invoice_metadata(
        env: Env,
        business: Address,
        invoice_id: BytesN<32>,
    ) -> Result<(), QuickLendXError> {
        require_writable(&env)?;
        let mut invoice = load_invoice(&env, &invoice_id)?;
        invoice.clear_metadata(&env, &business)?;
        InvoiceStorage::update_invoice(&env, &invoice);
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
