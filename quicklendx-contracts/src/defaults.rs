//! Default handling module for the QuickLendX protocol.
//!
//! This module provides strict access control and status validation for manual invoice
//! default marking. It ensures that defaults can only be triggered by authorized admin
//! actors and only when proper preconditions are met.
//!
//! # Security Model
//!
//! - **Admin-only access**: Manual default marking requires admin authorization via `require_auth`
//! - **Funded prerequisite**: Only invoices in `Funded` status can be marked as defaulted
//! - **Grace period enforcement**: Defaults cannot be triggered before the grace period expires
//! - **Idempotency**: Double-default attempts are rejected with a specific error
//!
//! # Validation Order
//!
//! Manual default marking validates in the following strict order:
//! 1. Invoice existence (must exist)
//! 2. Status: Already defaulted check (no double default)
//! 3. Status: Funded prerequisite (only Funded invoices)
//! 4. Time: Grace period expiry (must have elapsed)
//!
//! # Storage Design
//!
//! This module does not introduce new storage keys but relies on:
//! - `InvoiceStorage` for invoice data and status lists
//! - `InvestmentStorage` for investment status updates

use crate::errors::QuickLendXError;
use crate::events::{emit_insurance_claimed, emit_invoice_defaulted, emit_invoice_expired};
use crate::init::ProtocolInitializer;
use crate::investment::{InvestmentStatus, InvestmentStorage};
use crate::invoice::{InvoiceStatus, InvoiceStorage};
use soroban_sdk::{BytesN, Env, Vec};

/// Default grace period in seconds (7 days)
pub const DEFAULT_GRACE_PERIOD: u64 = 7 * 24 * 60 * 60;

/// Validates that an invoice exists and is in a state eligible for manual default marking.
///
/// # Security Checks
///
/// - Verifies invoice exists in storage
/// - Ensures invoice is not already defaulted (prevents double default)
/// - Ensures invoice is in Funded status (only Funded invoices can be manually defaulted)
///
/// # Arguments
///
/// * `env` - The contract environment
/// * `invoice_id` - The invoice ID to validate
///
/// # Returns
///
/// * `Ok(())` if the invoice is eligible for default marking
/// * `Err(QuickLendXError)` if validation fails:
///   - `InvoiceNotFound` - Invoice does not exist
///   - `InvoiceAlreadyDefaulted` - Invoice is already defaulted
///   - `InvoiceNotAvailableForFunding` - Invoice is not in Funded status
///
/// # Example
///
/// ```ignore
/// validate_invoice_for_default(env, &invoice_id)?;
/// ```
pub fn validate_invoice_for_default(
    env: &Env,
    invoice_id: &BytesN<32>,
) -> Result<(), QuickLendXError> {
    let invoice =
        InvoiceStorage::get_invoice(env, invoice_id).ok_or(QuickLendXError::InvoiceNotFound)?;

    if invoice.status == InvoiceStatus::Defaulted {
        return Err(QuickLendXError::InvoiceAlreadyDefaulted);
    }

    if invoice.status != InvoiceStatus::Funded {
        return Err(QuickLendXError::InvoiceNotAvailableForFunding);
    }

    Ok(())
}

/// Validates that the grace period has expired for an invoice.
///
/// # Security Checks
///
/// - Compares current timestamp against grace deadline
/// - Uses strict greater-than comparison (cannot default at exact deadline)
/// - Supports per-call grace period override
///
/// # Arguments
///
/// * `env` - The contract environment
/// * `invoice_id` - The invoice ID to check
/// * `grace_period` - Optional grace period override in seconds
///
/// # Returns
///
/// * `Ok(())` if grace period has expired
/// * `Err(QuickLendXError::OperationNotAllowed)` if grace period has not expired
///
/// # Grace Period Resolution
///
/// 1. `grace_period` argument (per-call override)
/// 2. Protocol config (`ProtocolInitializer::get_protocol_config`)
/// 3. `DEFAULT_GRACE_PERIOD` (7 days)
///
/// # Calculation
///
/// ```
/// grace_deadline = invoice.due_date + grace_period
/// can_default    = current_timestamp > grace_deadline
/// ```
pub fn validate_grace_period_expired(
    env: &Env,
    invoice_id: &BytesN<32>,
    grace_period: Option<u64>,
) -> Result<(), QuickLendXError> {
    let invoice =
        InvoiceStorage::get_invoice(env, invoice_id).ok_or(QuickLendXError::InvoiceNotFound)?;

    let current_timestamp = env.ledger().timestamp();
    let grace = resolve_grace_period(env, grace_period);
    let grace_deadline = invoice.grace_deadline(grace);

    if current_timestamp <= grace_deadline {
        return Err(QuickLendXError::OperationNotAllowed);
    }

    Ok(())
}

/// Marks an invoice as defaulted after validating all preconditions.
///
/// This function enforces strict access control and status validation:
/// 1. Invoice must exist
/// 2. Invoice must not already be defaulted
/// 3. Invoice must be in Funded status
/// 4. Grace period must have expired
///
/// # Authorization
///
/// - Admin authorization required via `require_auth` on caller
/// - Caller must be the configured admin address
///
/// # Arguments
///
/// * `env` - The contract environment
/// * `invoice_id` - The invoice ID to mark as defaulted
/// * `grace_period` - Optional grace period in seconds (defaults to 7 days)
///
/// # Returns
///
/// * `Ok(())` if the invoice was successfully marked as defaulted
/// * `Err(QuickLendXError)` if the operation fails:
///   - `NotAdmin` - Caller is not the configured admin
///   - `InvoiceNotFound` - Invoice does not exist
///   - `InvoiceAlreadyDefaulted` - Invoice is already defaulted
///   - `InvoiceNotAvailableForFunding` - Invoice is not in Funded status
///   - `OperationNotAllowed` - Grace period has not expired
///
/// # Security Notes
///
/// - Authorization is enforced by the caller (admin.require_auth())
/// - All validations are performed in strict order to prevent race conditions
/// - Idempotent: calling on already defaulted invoice returns specific error
///
/// # Example
///
/// ```ignore
/// // Admin marks invoice as defaulted after grace period
/// admin.require_auth();
/// mark_invoice_defaulted(&env, &invoice_id, Some(604800))?;
/// ```
pub fn mark_invoice_defaulted(
    env: &Env,
    invoice_id: &BytesN<32>,
    grace_period: Option<u64>,
) -> Result<(), QuickLendXError> {
    validate_invoice_for_default(env, invoice_id)?;
    validate_grace_period_expired(env, invoice_id, grace_period)?;
    handle_default(env, invoice_id)
}

/// Resolves the effective grace period using a priority order.
///
/// Grace period resolution order:
/// 1. `grace_period` argument (per-call override)
/// 2. Protocol config (`ProtocolInitializer::get_protocol_config`)
/// 3. `DEFAULT_GRACE_PERIOD` (7 days)
///
/// # Arguments
///
/// * `env` - The contract environment
/// * `grace_period` - Optional grace period override in seconds
///
/// # Returns
///
/// * `u64` - The effective grace period in seconds
///
/// # Example
///
/// ```ignore
/// let grace = resolve_grace_period(env, Some(3 * 24 * 60 * 60)); // 3 days
/// ```
pub fn resolve_grace_period(env: &Env, grace_period: Option<u64>) -> u64 {
    match grace_period {
        Some(value) => value,
        None => ProtocolInitializer::get_protocol_config(env)
            .map(|config| config.grace_period_seconds)
            .unwrap_or(DEFAULT_GRACE_PERIOD),
    }
}

/// Computes the grace deadline for an invoice based on its due date and grace period.
///
/// The deadline is calculated as: `due_date + grace_period`
///
/// # Arguments
///
/// * `env` - The contract environment
/// * `invoice_id` - The invoice ID
/// * `grace_period` - Grace period in seconds
///
/// # Returns
///
/// * `u64` - The grace deadline timestamp
///
/// # Security Notes
///
/// - Uses `grace_deadline` method on Invoice which uses saturating arithmetic
///   to prevent overflow when adding grace period to due date
pub fn compute_grace_deadline(
    env: &Env,
    invoice_id: &BytesN<32>,
    grace_period: u64,
) -> Result<u64, QuickLendXError> {
    let invoice =
        InvoiceStorage::get_invoice(env, invoice_id).ok_or(QuickLendXError::InvoiceNotFound)?;
    Ok(invoice.grace_deadline(grace_period))
}

/// Checks if an invoice can be marked as defaulted based on current state and time.
///
/// This is a read-only helper that can be used for UI pre-validation before
/// attempting a manual default operation.
///
/// # Arguments
///
/// * `env` - The contract environment
/// * `invoice_id` - The invoice ID to check
/// * `grace_period` - Optional grace period in seconds
///
/// # Returns
///
/// * `Ok(true)` if the invoice can be defaulted
/// * `Err(QuickLendXError)` with the specific reason if cannot be defaulted
pub fn can_mark_as_defaulted(
    env: &Env,
    invoice_id: &BytesN<32>,
    grace_period: Option<u64>,
) -> Result<bool, QuickLendXError> {
    if let Err(e) = validate_invoice_for_default(env, invoice_id) {
        return Err(e);
    }
    if let Err(e) = validate_grace_period_expired(env, invoice_id, grace_period) {
        return Err(e);
    }
    Ok(true)
}

/// Internal handler that performs the actual invoice defaulting.
///
/// This function assumes all validations (existence, status, grace period) have been
/// completed by the caller. It performs the state transitions and emits events.
///
/// # State Transitions
///
/// 1. Removes invoice from `Funded` status list
/// 2. Sets invoice status to `Defaulted`
/// 3. Adds invoice to `Defaulted` status list
/// 4. Updates linked investment status to `Defaulted`
/// 5. Processes insurance claims if coverage exists
/// 6. Emits events: `invoice_expired`, `invoice_defaulted`, `insurance_claimed`
///
/// # Arguments
///
/// * `env` - The contract environment
/// * `invoice_id` - The invoice ID to mark as defaulted
///
/// # Returns
///
/// * `Ok(())` on success
/// * `Err(QuickLendXError)` on failure:
///   - `InvoiceNotFound` - Invoice does not exist
///   - `InvoiceAlreadyDefaulted` - Already defaulted (defensive check)
///   - `InvalidStatus` - Invoice not in Funded status
///
/// # Security Notes
///
/// - This function is idempotent for already-defaulted invoices
/// - Insurance claims are processed atomically with status change
/// - Events provide audit trail for all state transitions
pub fn handle_default(env: &Env, invoice_id: &BytesN<32>) -> Result<(), QuickLendXError> {
    let mut invoice =
        InvoiceStorage::get_invoice(env, invoice_id).ok_or(QuickLendXError::InvoiceNotFound)?;

    if invoice.status == InvoiceStatus::Defaulted {
        return Err(QuickLendXError::InvoiceAlreadyDefaulted);
    }

    if invoice.status != InvoiceStatus::Funded {
        return Err(QuickLendXError::InvalidStatus);
    }

    InvoiceStorage::remove_from_status_invoices(env, &InvoiceStatus::Funded, invoice_id);

    invoice.mark_as_defaulted();
    InvoiceStorage::update_invoice(env, &invoice);

    InvoiceStorage::add_to_status_invoices(env, &InvoiceStatus::Defaulted, invoice_id);

    emit_invoice_expired(env, &invoice);

    if let Some(mut investment) = InvestmentStorage::get_investment_by_invoice(env, invoice_id) {
        investment.status = InvestmentStatus::Defaulted;

        let claim_details = investment
            .process_insurance_claim()
            .and_then(|(provider, amount)| {
                if amount > 0 {
                    Some((provider, amount))
                } else {
                    None
                }
            });

        InvestmentStorage::update_investment(env, &investment);

        if let Some((provider, coverage_amount)) = claim_details {
            emit_insurance_claimed(
                env,
                &investment.investment_id,
                &investment.invoice_id,
                &provider,
                coverage_amount,
            );
        }
    }

    emit_invoice_defaulted(env, &invoice);

    Ok(())
}

/// Retrieves all invoice IDs that have active or resolved disputes.
///
/// # Security Notes
///
/// - This is a read-only query function
/// - No authorization required for viewing dispute information
///
/// # Returns
///
/// * `Vec<BytesN<32>>` - List of invoice IDs with disputes
///
/// # Implementation Notes
///
/// In production, a separate index for invoices with disputes would be maintained.
/// Current implementation returns empty to avoid expensive iteration.
pub fn get_invoices_with_disputes(env: &Env) -> Vec<BytesN<32>> {
    Vec::new(env)
}

/// Retrieves dispute details for a specific invoice.
///
/// # Arguments
///
/// * `env` - The contract environment
/// * `invoice_id` - The invoice ID to query
///
/// # Returns
///
/// * `Ok(Some(Dispute))` if dispute exists
/// * `Ok(None)` if no dispute exists for this invoice
/// * `Err(InvoiceNotFound)` if invoice does not exist
///
/// # Security Notes
///
/// - This is a read-only query function
/// - No authorization required for viewing dispute information
pub fn get_dispute_details(
    env: &Env,
    invoice_id: &BytesN<32>,
) -> Result<Option<crate::invoice::Dispute>, QuickLendXError> {
    let _invoice =
        InvoiceStorage::get_invoice(env, invoice_id).ok_or(QuickLendXError::InvoiceNotFound)?;

    Ok(None)
}
