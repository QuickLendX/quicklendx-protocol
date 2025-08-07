use crate::errors::QuickLendXError;
use crate::events::emit_invoice_defaulted;
use crate::investment::{InvestmentStatus, InvestmentStorage};
use crate::invoice::{InvoiceStatus, InvoiceStorage};
use crate::notifications::NotificationSystem;
use soroban_sdk::{BytesN, Env};

pub fn handle_default(env: &Env, invoice_id: &BytesN<32>) -> Result<(), QuickLendXError> {
    let mut invoice =
        InvoiceStorage::get_invoice(env, invoice_id).ok_or(QuickLendXError::InvoiceNotFound)?;
    if invoice.status != InvoiceStatus::Funded {
        return Err(QuickLendXError::InvalidStatus);
    }
    invoice.mark_as_defaulted();
    InvoiceStorage::update_invoice(env, &invoice);
    let mut investment = InvestmentStorage::get_investment(env, invoice_id)
        .ok_or(QuickLendXError::StorageKeyNotFound)?;
    investment.status = InvestmentStatus::Withdrawn;
    InvestmentStorage::update_investment(env, &investment);
    emit_invoice_defaulted(env, &invoice);

    // Send notification about invoice default
    let _ = NotificationSystem::notify_invoice_defaulted(env, &invoice);

    Ok(())
}
