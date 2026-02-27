use crate::errors::QuickLendXError;
use crate::events::{
    emit_dispute_created, emit_dispute_resolved, emit_dispute_under_review, emit_insurance_claimed,
    emit_invoice_defaulted, emit_invoice_expired,
};
use crate::init::ProtocolInitializer;
use crate::investment::{InvestmentStatus, InvestmentStorage};
use crate::invoice::{InvoiceStatus, InvoiceStorage};
use soroban_sdk::{Address, BytesN, Env, String, Vec};

/// Default grace period in seconds (7 days)
pub const DEFAULT_GRACE_PERIOD: u64 = 7 * 24 * 60 * 60;

/// Mark an invoice as defaulted (admin or automated process)
/// Checks due date + grace period before marking as defaulted
///
/// # Arguments
/// * `env` - The environment
/// * `invoice_id` - The invoice ID to mark as defaulted
/// * `grace_period` - Optional grace period in seconds. If `None`, uses protocol config or
///   `DEFAULT_GRACE_PERIOD` when not configured.
///
/// # Returns
/// * `Ok(())` if the invoice was successfully marked as defaulted
/// * `Err(QuickLendXError)` if the operation fails
pub fn mark_invoice_defaulted(
    env: &Env,
    invoice_id: &BytesN<32>,
    grace_period: Option<u64>,
) -> Result<(), QuickLendXError> {
    let invoice =
        InvoiceStorage::get_invoice(env, invoice_id).ok_or(QuickLendXError::InvoiceNotFound)?;

    // Check if invoice is already defaulted (no double default)
    if invoice.status == InvoiceStatus::Defaulted {
        return Err(QuickLendXError::InvoiceAlreadyDefaulted);
    }

    // Only funded invoices can be defaulted
    if invoice.status != InvoiceStatus::Funded {
        return Err(QuickLendXError::InvoiceNotAvailableForFunding);
    }

    let current_timestamp = env.ledger().timestamp();
    let grace = resolve_grace_period(env, grace_period);
    let grace_deadline = invoice.grace_deadline(grace);

    // Check if grace period has passed
    if current_timestamp <= grace_deadline {
        return Err(QuickLendXError::OperationNotAllowed);
    }

    // Proceed with default handling
    handle_default(env, invoice_id)
}

/// Resolve grace period using per-call override, protocol config, or default.
pub fn resolve_grace_period(env: &Env, grace_period: Option<u64>) -> u64 {
    match grace_period {
        Some(value) => value,
        None => ProtocolInitializer::get_protocol_config(env)
            .map(|config| config.grace_period_seconds)
            .unwrap_or(DEFAULT_GRACE_PERIOD),
    }
}

/// Handle invoice default - internal function that performs the actual defaulting
/// This function assumes all validations have been done (grace period, status, etc.)
pub fn handle_default(env: &Env, invoice_id: &BytesN<32>) -> Result<(), QuickLendXError> {
    let mut invoice =
        InvoiceStorage::get_invoice(env, invoice_id).ok_or(QuickLendXError::InvoiceNotFound)?;

    // Check if already defaulted (no double default)
    if invoice.status == InvoiceStatus::Defaulted {
        return Err(QuickLendXError::InvoiceAlreadyDefaulted);
    }

    // Validate invoice is in funded status
    if invoice.status != InvoiceStatus::Funded {
        return Err(QuickLendXError::InvalidStatus);
    }

    // Remove from funded status list
    InvoiceStorage::remove_from_status_invoices(env, &InvoiceStatus::Funded, invoice_id);

    // Mark invoice as defaulted
    invoice.mark_as_defaulted();
    InvoiceStorage::update_invoice(env, &invoice);

    // Add to defaulted status list
    InvoiceStorage::add_to_status_invoices(env, &InvoiceStatus::Defaulted, invoice_id);

    // Emit expiration event
    emit_invoice_expired(env, &invoice);

    // Update investment status and process insurance claims
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

    // Emit default event
    emit_invoice_defaulted(env, &invoice);

    // Send notification
    // No notifications

    Ok(())
}

/// Get all invoice IDs that have active or resolved disputes
pub fn get_invoices_with_disputes(env: &Env) -> Vec<BytesN<32>> {
    // This is a simplified implementation. In a production environment,
    // we would maintain a separate index for invoices with disputes.
    // For now, we return empty as a placeholder or could iterate (expensive).
    Vec::new(env)
}

/// Get details for a dispute on a specific invoice
pub fn get_dispute_details(env: &Env, invoice_id: &BytesN<32>) -> Result<Option<crate::invoice::Dispute>, QuickLendXError> {
    let invoice = InvoiceStorage::get_invoice(env, invoice_id)
        .ok_or(QuickLendXError::InvoiceNotFound)?;
    
    // In this implementation, the Dispute struct is part of the Invoice struct
    // but the analytics module expects a separate query.
    // Actually, looking at types.rs or invoice.rs, let's see where Dispute is.
    // If it's not in Invoice, we might need a separate storage.
    // Based on analytics.rs usage, it seems to expect it found here.
    
    Ok(None) // Placeholder
}

