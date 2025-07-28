use crate::errors::QuickLendXError;
use crate::events::emit_invoice_defaulted;
use crate::investment::{Investment, InvestmentStatus, InvestmentStorage};
use crate::invoice::{Invoice, InvoiceStatus, InvoiceStorage};
use soroban_sdk::{BytesN, Env};

pub fn handle_default(env: &Env, invoice_id: &BytesN<32>) -> Result<(), QuickLendXError> {
    let mut invoice =
        InvoiceStorage::get_invoice(env, invoice_id).ok_or(QuickLendXError::InvoiceNotFound)?;

    // Only process funded invoices that aren't already defaulted
    if invoice.status != InvoiceStatus::Funded {
        return Err(QuickLendXError::InvalidStatus);
    }

    InvoiceStorage::remove_from_status_invoices(env, &InvoiceStatus::Funded, invoice_id);

    invoice.mark_as_defaulted();

    InvoiceStorage::update_invoice(env, &invoice);

    InvoiceStorage::add_to_status_invoices(env, &InvoiceStatus::Defaulted, invoice_id);

    // Update related investment status
    let mut investment = InvestmentStorage::get_investment(env, invoice_id)
        .ok_or(QuickLendXError::StorageKeyNotFound)?;
    investment.status = InvestmentStatus::Defaulted;
    InvestmentStorage::update_investment(env, &investment);

    // Emit default event
    emit_invoice_defaulted(env, &invoice);

    Ok(())
}

pub fn check_and_handle_expired_invoices(
    env: &Env,
    grace_period: Option<u64>,
) -> Result<(), QuickLendXError> {
    let funded_invoices = InvoiceStorage::get_invoices_by_status(env, &InvoiceStatus::Funded);

    for invoice_id in funded_invoices.iter() {
        // Get the invoice from storage
        let mut invoice = match InvoiceStorage::get_invoice(env, &invoice_id) {
            Some(inv) => inv,
            None => continue,
        };

        // Skip if already defaulted
        if invoice.status == InvoiceStatus::Defaulted {
            continue;
        }

        // Check expiration with optional grace period
        if invoice.is_overdue(env.ledger().timestamp(), grace_period) {
            InvoiceStorage::remove_from_status_invoices(env, &InvoiceStatus::Funded, &invoice_id);

            invoice.mark_as_defaulted();

            InvoiceStorage::update_invoice(env, &invoice);

            InvoiceStorage::add_to_status_invoices(env, &InvoiceStatus::Defaulted, &invoice_id);

            // Update related investment status
            if let Some(mut investment) = InvestmentStorage::get_investment(env, &invoice_id) {
                investment.status = InvestmentStatus::Defaulted;
                InvestmentStorage::update_investment(env, &investment);
            }

            // Emit default event
            emit_invoice_defaulted(env, &invoice);
        }
    }

    Ok(())
}
