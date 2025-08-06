use crate::errors::QuickLendXError;
use crate::events::emit_invoice_defaulted;
use crate::investment::{Investment, InvestmentStatus, InvestmentStorage};
use crate::invoice::{Invoice, InvoiceStatus, InvoiceStorage};
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

    // Process insurance claim if coverage exists
    if let Some(ref insurance) = investment.insurance {
        if insurance.active {
            let claim_amount = InvestmentStorage::process_insurance_claim(env, &investment.investment_id)?;
            crate::events::emit_insurance_claimed(env, &investment.investment_id, &insurance.provider, claim_amount);
        }
    }

    emit_invoice_defaulted(env, &invoice);
    Ok(())
}
