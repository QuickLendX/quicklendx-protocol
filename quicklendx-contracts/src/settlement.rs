use crate::audit::log_payment_processed;
use crate::errors::QuickLendXError;
use crate::events::{emit_invoice_settled, emit_partial_payment};
use crate::investment::{InvestmentStatus, InvestmentStorage};
use crate::invoice::{InvoiceStatus, InvoiceStorage};
use crate::notifications::NotificationSystem;
use crate::payments::transfer_funds;
use crate::profits::calculate_profit;
use soroban_sdk::{BytesN, Env, String};

pub fn process_partial_payment(
    env: &Env,
    invoice_id: &BytesN<32>,
    payment_amount: i128,
    transaction_id: String,
) -> Result<(), QuickLendXError> {
    if payment_amount <= 0 {
        return Err(QuickLendXError::InvalidAmount);
    }

    let mut invoice =
        InvoiceStorage::get_invoice(env, invoice_id).ok_or(QuickLendXError::InvoiceNotFound)?;

    if invoice.status != InvoiceStatus::Funded {
        return Err(QuickLendXError::InvalidStatus);
    }

    let business = invoice.business.clone();
    business.require_auth();

    let tx_for_event = transaction_id.clone();
    let progress = invoice.record_payment(env, payment_amount, transaction_id)?;
    InvoiceStorage::update_invoice(env, &invoice);

    emit_partial_payment(
        env,
        &invoice,
        payment_amount,
        invoice.total_paid,
        progress,
        tx_for_event,
    );
    log_payment_processed(
        env,
        invoice.id.clone(),
        business.clone(),
        payment_amount,
        String::from_str(env, "partial"),
    );

    if invoice.is_fully_paid() {
        settle_invoice(env, invoice_id, invoice.total_paid)?;
    }

    Ok(())
}

pub fn settle_invoice(
    env: &Env,
    invoice_id: &BytesN<32>,
    payment_amount: i128,
) -> Result<(), QuickLendXError> {
    if payment_amount <= 0 {
        return Err(QuickLendXError::InvalidAmount);
    }

    // Get and validate invoice
    let mut invoice =
        InvoiceStorage::get_invoice(env, invoice_id).ok_or(QuickLendXError::InvoiceNotFound)?;

    if invoice.status != InvoiceStatus::Funded {
        return Err(QuickLendXError::InvalidStatus);
    }

    // Get investor from invoice
    let investor_address = invoice
        .investor
        .clone()
        .ok_or(QuickLendXError::NotInvestor)?;

    // Get investment details
    let investment = InvestmentStorage::get_investment_by_invoice(env, invoice_id)
        .ok_or(QuickLendXError::StorageKeyNotFound)?;

    // Ensure the recorded total reflects the latest payment attempt
    let mut total_payment = invoice.total_paid;
    if total_payment == 0 {
        invoice.record_payment(env, payment_amount, String::from_str(env, "settlement"))?;
        total_payment = invoice.total_paid;
    } else if payment_amount > total_payment {
        let additional = payment_amount.saturating_sub(total_payment);
        if additional > 0 {
            invoice.record_payment(env, additional, String::from_str(env, "settlement_adj"))?;
        }
        total_payment = invoice.total_paid;
    } else {
        total_payment = total_payment.max(payment_amount);
        invoice.total_paid = total_payment;
    }

    if total_payment < investment.amount || total_payment < invoice.amount {
        return Err(QuickLendXError::PaymentTooLow);
    }

    // Calculate profit and platform fee
    let (investor_return, platform_fee) = calculate_profit(env, investment.amount, total_payment);

    // Transfer funds to investor and platform
    let business_address = invoice.business.clone();
    transfer_funds(
        env,
        &invoice.currency,
        &business_address,
        &investor_address,
        investor_return,
    )?;

    if platform_fee > 0 {
        let platform_account = env.current_contract_address();
        transfer_funds(
            env,
            &invoice.currency,
            &business_address,
            &platform_account,
            platform_fee,
        )?;
    }

    // Update invoice status
    let previous_status = invoice.status.clone();
    invoice.mark_as_paid(env, business_address.clone(), env.ledger().timestamp());
    InvoiceStorage::update_invoice(env, &invoice);
    if previous_status != invoice.status {
        InvoiceStorage::remove_from_status_invoices(env, &previous_status, invoice_id);
        InvoiceStorage::add_to_status_invoices(env, &invoice.status, invoice_id);
    }

    // Update investment status
    let mut updated_investment = investment;
    updated_investment.status = InvestmentStatus::Completed;
    InvestmentStorage::update_investment(env, &updated_investment);

    log_payment_processed(
        env,
        invoice.id.clone(),
        business_address.clone(),
        total_payment,
        String::from_str(env, "final"),
    );

    // Emit settlement event
    emit_invoice_settled(env, &invoice, investor_return, platform_fee);

    // Send notification about payment received
    let _ = NotificationSystem::notify_payment_received(env, &invoice, total_payment);

    Ok(())
}
